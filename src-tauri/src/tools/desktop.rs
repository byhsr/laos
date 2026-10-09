// PC control: native mouse/keyboard input, screen capture, window/app control,
// and clipboard/system actions. Granted to an agent only when it holds the
// `pc_control` permission (see agents::build_tools). Windows-first: on other
// platforms the toolset is empty, so the permission grants nothing.
//
// The screen capture returns an image for the model to SEE (via ToolOutput's
// images); the input tools take coordinates in that image's pixel space, and
// DPI awareness is set so those pixels map 1:1 to real screen pixels.

// Non-Windows build: no desktop tools.
#[cfg(not(windows))]
pub(crate) fn desktop_tools() -> Vec<Box<dyn crate::tools::AgentTool>> {
  Vec::new()
}

#[cfg(windows)]
pub(crate) use win::desktop_tools;

#[cfg(windows)]
mod win {
  use async_trait::async_trait;
  use base64::engine::general_purpose::STANDARD as BASE64;
  use base64::Engine as _;
  use enigo::{Axis, Button, Coordinate, Direction, Enigo, Key, Keyboard, Mouse, Settings};
  use std::io::Cursor;
  use std::sync::Once;
  use xcap::image::{imageops, ImageFormat, RgbaImage};
  use xcap::Monitor;

  use windows::core::{BOOL, PCWSTR};
  use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
  use windows::Win32::System::Shutdown::LockWorkStation;
  use windows::Win32::UI::Shell::ShellExecuteW;
  use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindowTextLengthW, GetWindowTextW, IsWindowVisible, PostMessageW, ShowWindow,
    SetForegroundWindow, SW_MAXIMIZE, SW_MINIMIZE, SW_RESTORE, SW_SHOWNORMAL, WM_CLOSE,
  };

  use crate::tools::{clip, str_arg, AgentTool, ToolOutput};

  const MAX_IMAGE_DIM: u32 = 1280;

  fn int_arg(args: &serde_json::Value, key: &str) -> Option<i32> {
    args.get(key).and_then(|v| v.as_i64()).map(|v| v as i32)
  }

  fn hwnd_arg(args: &serde_json::Value) -> Result<HWND, String> {
    let raw = args.get("handle").and_then(|v| v.as_str())
      .or_else(|| args.get("handle").and_then(|v| v.as_i64()).map(|_| ""))
      .unwrap_or("");
    let id: isize = if raw.is_empty() {
      args.get("handle").and_then(|v| v.as_i64()).ok_or("A window 'handle' is required (from list_windows).")? as isize
    } else {
      raw.trim().parse().map_err(|_| "Window handle must be a number from list_windows.")?
    };
    Ok(HWND(id as *mut core::ffi::c_void))
  }

  // One DPI-awareness call for the process, then a fresh Enigo per action (it is
  // cheap and avoids holding a broken session).
  fn enigo() -> Result<Enigo, String> {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| { let _ = enigo::set_dpi_awareness(); });
    Enigo::new(&Settings::default()).map_err(|e| format!("input control unavailable: {e}"))
  }

  fn button_arg(args: &serde_json::Value) -> Button {
    match args.get("button").and_then(|v| v.as_str()).unwrap_or("left").to_lowercase().as_str() {
      "right" => Button::Right,
      "middle" => Button::Middle,
      _ => Button::Left,
    }
  }

  // -------------------------------------------------------------------------
  // Screen capture (vision)
  // -------------------------------------------------------------------------
  struct ScreenCapture;

  #[async_trait]
  impl AgentTool for ScreenCapture {
    fn name(&self) -> String { "screen_capture".into() }
    fn description(&self) -> String {
      "Capture the screen as an image so you can see what is on it. Returns the image plus its pixel size. Coordinates you pass to the mouse tools are pixels in THIS image (origin top-left). Optional: monitor (0-based index; default is the primary monitor).".into()
    }
    fn params_schema(&self) -> serde_json::Value {
      serde_json::json!({ "type": "object", "properties": { "monitor": { "type": "integer", "description": "0-based monitor index; defaults to the primary monitor" } }, "required": [] })
    }
    async fn run(&self, _args: &serde_json::Value) -> Result<String, String> {
      Err("screen_capture returns an image and needs the interactive chat, which can carry images.".into())
    }
    async fn run_with_images(&self, args: &serde_json::Value) -> Result<ToolOutput, String> {
      let monitors = Monitor::all().map_err(|e| e.to_string())?;
      let wanted = args.get("monitor").and_then(|v| v.as_u64()).map(|v| v as usize);
      let monitor = match wanted {
        Some(i) => monitors.get(i).ok_or_else(|| format!("No monitor at index {i} ({} found).", monitors.len()))?,
        None => monitors.iter().find(|m| m.is_primary().unwrap_or(false)).or_else(|| monitors.first())
          .ok_or("No monitors found.")?,
      };
      let img: RgbaImage = monitor.capture_image().map_err(|e| e.to_string())?;
      let (w, h) = img.dimensions();
      let (sw, sh) = downscale(w, h);
      let img = if (sw, sh) != (w, h) { imageops::resize(&img, sw, sh, imageops::FilterType::Triangle) } else { img };
      let mut png = Vec::new();
      img.write_to(&mut Cursor::new(&mut png), ImageFormat::Png).map_err(|e| e.to_string())?;
      let b64 = BASE64.encode(&png);
      let scale = monitor.scale_factor().unwrap_or(1.0);
      let text = format!(
        "Screen captured: the physical screen is {w}x{h} px (scale {scale:.2}). The image below is {sw}x{sh} px. Give mouse coordinates in THIS image's pixel space (origin top-left, x right, y down).",
      );
      Ok(ToolOutput { text, images: vec![b64] })
    }
  }

  fn downscale(w: u32, h: u32) -> (u32, u32) {
    let max = w.max(h);
    if max <= MAX_IMAGE_DIM { return (w, h); }
    let f = MAX_IMAGE_DIM as f32 / max as f32;
    (((w as f32 * f) as u32).max(1), ((h as f32 * f) as u32).max(1))
  }

  // -------------------------------------------------------------------------
  // Windows
  // -------------------------------------------------------------------------
  unsafe extern "system" fn enum_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let list = &mut *(lparam.0 as *mut Vec<(isize, String)>);
    if IsWindowVisible(hwnd).as_bool() {
      let len = GetWindowTextLengthW(hwnd);
      if len > 0 {
        let mut buf = vec![0u16; (len + 1) as usize];
        let n = GetWindowTextW(hwnd, &mut buf);
        let title = String::from_utf16_lossy(&buf[..n.max(0) as usize]);
        if !title.trim().is_empty() { list.push((hwnd.0 as isize, title)); }
      }
    }
    BOOL(1)
  }

  struct ListWindows;

  #[async_trait]
  impl AgentTool for ListWindows {
    fn name(&self) -> String { "list_windows".into() }
    fn description(&self) -> String { "List the visible top-level windows as '<handle>: <title>'. Use a handle with focus_window / minimize_window / maximize_window / close_window.".into() }
    fn params_schema(&self) -> serde_json::Value { serde_json::json!({ "type": "object", "properties": {}, "required": [] }) }
    async fn run(&self, _args: &serde_json::Value) -> Result<String, String> {
      let mut list: Vec<(isize, String)> = Vec::new();
      unsafe { let _ = EnumWindows(Some(enum_proc), LPARAM(&mut list as *mut _ as isize)); }
      if list.is_empty() { return Ok("No visible windows.".into()); }
      Ok(list.iter().map(|(h, t)| format!("{h}: {t}")).collect::<Vec<_>>().join("\n"))
    }
  }

  macro_rules! window_tool {
    ($struct:ident, $name:literal, $desc:literal, $body:expr) => {
      struct $struct;
      #[async_trait]
      impl AgentTool for $struct {
        fn name(&self) -> String { $name.into() }
        fn description(&self) -> String { $desc.into() }
        fn params_schema(&self) -> serde_json::Value {
          serde_json::json!({ "type": "object", "properties": { "handle": { "type": "string", "description": "Window handle from list_windows" } }, "required": ["handle"] })
        }
        async fn run(&self, args: &serde_json::Value) -> Result<String, String> {
          let hwnd = hwnd_arg(args)?;
          #[allow(clippy::redundant_closure_call)]
          let f: fn(HWND) -> Result<String, String> = $body;
          f(hwnd)
        }
      }
    };
  }

  window_tool!(FocusWindow, "focus_window", "Bring a window to the foreground and restore it if minimized. Params: handle (string).", |hwnd| unsafe {
    let _ = ShowWindow(hwnd, SW_RESTORE);
    let _ = SetForegroundWindow(hwnd);
    Ok("Window focused.".into())
  });

  window_tool!(MinimizeWindow, "minimize_window", "Minimize a window. Params: handle (string).", |hwnd| unsafe {
    let _ = ShowWindow(hwnd, SW_MINIMIZE);
    Ok("Window minimized.".into())
  });

  window_tool!(MaximizeWindow, "maximize_window", "Maximize a window. Params: handle (string).", |hwnd| unsafe {
    let _ = ShowWindow(hwnd, SW_MAXIMIZE);
    Ok("Window maximized.".into())
  });

  window_tool!(CloseWindow, "close_window", "Ask a window to close (sends WM_CLOSE). Params: handle (string).", |hwnd| {
    unsafe { PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0)) }.map_err(|e| e.to_string())?;
    Ok("Close requested.".into())
  });

  // -------------------------------------------------------------------------
  // Mouse
  // -------------------------------------------------------------------------
  macro_rules! mouse_tool {
    ($struct:ident, $name:literal, $desc:literal, $props:expr, $req:expr, $body:expr) => {
      struct $struct;
      #[async_trait]
      impl AgentTool for $struct {
        fn name(&self) -> String { $name.into() }
        fn description(&self) -> String { $desc.into() }
        fn params_schema(&self) -> serde_json::Value {
          serde_json::json!({ "type": "object", "properties": $props, "required": $req })
        }
        async fn run(&self, args: &serde_json::Value) -> Result<String, String> {
          #[allow(clippy::redundant_closure_call)]
          ($body)(args)
        }
      }
    };
  }

  mouse_tool!(MouseMove, "mouse_move", "Move the mouse to (x, y) in screen pixels (the coordinate space of the last screen_capture image).", serde_json::json!({ "x": { "type": "integer" }, "y": { "type": "integer" } }), serde_json::json!(["x", "y"]), |args: &serde_json::Value| {
    let (x, y) = (int_arg(args, "x").ok_or("mouse_move requires x and y.")?, int_arg(args, "y").ok_or("mouse_move requires x and y.")?);
    enigo()?.move_mouse(x, y, Coordinate::Abs).map_err(|e| e.to_string())?;
    Ok(format!("Moved mouse to ({x}, {y})."))
  });

  mouse_tool!(MouseClick, "mouse_click", "Move the mouse to (x, y) and click. button: left | right | middle (default left).", serde_json::json!({ "x": { "type": "integer" }, "y": { "type": "integer" }, "button": { "type": "string" } }), serde_json::json!(["x", "y"]), |args: &serde_json::Value| {
    let (x, y) = (int_arg(args, "x").ok_or("mouse_click requires x and y.")?, int_arg(args, "y").ok_or("mouse_click requires x and y.")?);
    let button = button_arg(args);
    let mut e = enigo()?;
    e.move_mouse(x, y, Coordinate::Abs).map_err(|e| e.to_string())?;
    e.button(button, Direction::Click).map_err(|e| e.to_string())?;
    Ok(format!("Clicked {button:?} at ({x}, {y})."))
  });

  mouse_tool!(MouseDrag, "mouse_drag", "Press the mouse at (x1, y1), move to (x2, y2), release. button: left | right | middle.", serde_json::json!({ "x1": { "type": "integer" }, "y1": { "type": "integer" }, "x2": { "type": "integer" }, "y2": { "type": "integer" }, "button": { "type": "string" } }), serde_json::json!(["x1", "y1", "x2", "y2"]), |args: &serde_json::Value| {
    let x1 = int_arg(args, "x1").ok_or("mouse_drag requires x1,y1,x2,y2.")?;
    let y1 = int_arg(args, "y1").ok_or("mouse_drag requires x1,y1,x2,y2.")?;
    let x2 = int_arg(args, "x2").ok_or("mouse_drag requires x1,y1,x2,y2.")?;
    let y2 = int_arg(args, "y2").ok_or("mouse_drag requires x1,y1,x2,y2.")?;
    let button = button_arg(args);
    let mut e = enigo()?;
    e.move_mouse(x1, y1, Coordinate::Abs).map_err(|e| e.to_string())?;
    e.button(button, Direction::Press).map_err(|e| e.to_string())?;
    e.move_mouse(x2, y2, Coordinate::Abs).map_err(|e| e.to_string())?;
    e.button(button, Direction::Release).map_err(|e| e.to_string())?;
    Ok(format!("Dragged {button:?} from ({x1}, {y1}) to ({x2}, {y2})."))
  });

  mouse_tool!(Scroll, "scroll", "Scroll the mouse wheel. direction: up | down | left | right (default down). amount: number of wheel steps (default 3).", serde_json::json!({ "direction": { "type": "string" }, "amount": { "type": "integer" } }), serde_json::json!([]), |args: &serde_json::Value| {
    let amount = int_arg(args, "amount").unwrap_or(3).max(1);
    let dir = args.get("direction").and_then(|v| v.as_str()).unwrap_or("down").to_lowercase();
    let (length, axis) = match dir.as_str() {
      "up" => (-amount, Axis::Vertical),
      "left" => (-amount, Axis::Horizontal),
      "right" => (amount, Axis::Horizontal),
      _ => (amount, Axis::Vertical),
    };
    enigo()?.scroll(length, axis).map_err(|e| e.to_string())?;
    Ok(format!("Scrolled {dir} by {amount}."))
  });

  // -------------------------------------------------------------------------
  // Keyboard
  // -------------------------------------------------------------------------
  struct TypeText;

  #[async_trait]
  impl AgentTool for TypeText {
    fn name(&self) -> String { "type_text".into() }
    fn description(&self) -> String { "Type a string of text at the current focus (like using the keyboard).".into() }
    fn params_schema(&self) -> serde_json::Value {
      serde_json::json!({ "type": "object", "properties": { "text": { "type": "string" } }, "required": ["text"] })
    }
    async fn run(&self, args: &serde_json::Value) -> Result<String, String> {
      let text = str_arg(args, "text").ok_or("type_text requires 'text'.")?;
      enigo()?.text(&text).map_err(|e| e.to_string())?;
      Ok(format!("Typed {} characters.", text.chars().count()))
    }
  }

  struct PressKeys;

  #[async_trait]
  impl AgentTool for PressKeys {
    fn name(&self) -> String { "press_keys".into() }
    fn description(&self) -> String {
      "Press a key or a chord, then release. Params: keys (array of key names), e.g. [\"ctrl\",\"c\"] to copy, [\"enter\"], [\"alt\",\"f4\"]. Names: ctrl/control, shift, alt, meta/win, enter/return, tab, esc, space, backspace, delete, up/down/left/right, home, end, pageup, pagedown, f1..f24, or a single character.".into()
    }
    fn params_schema(&self) -> serde_json::Value {
      serde_json::json!({ "type": "object", "properties": { "keys": { "type": "array", "items": { "type": "string" } } }, "required": ["keys"] })
    }
    async fn run(&self, args: &serde_json::Value) -> Result<String, String> {
      let names: Vec<String> = args.get("keys").and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect())
        .unwrap_or_default();
      if names.is_empty() { return Err("press_keys requires a non-empty 'keys' array.".into()); }
      let mut keys = Vec::new();
      for n in &names { keys.push(map_key(n).ok_or_else(|| format!("Unknown key '{n}'."))?); }
      let mut e = enigo()?;
      for k in &keys { e.key(*k, Direction::Press).map_err(|e| e.to_string())?; }
      for k in keys.iter().rev() { e.key(*k, Direction::Release).map_err(|e| e.to_string())?; }
      Ok(format!("Pressed {}.", names.join("+")))
    }
  }

  fn map_key(name: &str) -> Option<Key> {
    let n = name.trim().to_lowercase();
    Some(match n.as_str() {
      "ctrl" | "control" => Key::Control,
      "shift" => Key::Shift,
      "alt" | "option" => Key::Alt,
      "meta" | "win" | "super" | "cmd" | "command" => Key::Meta,
      "enter" | "return" => Key::Return,
      "tab" => Key::Tab,
      "esc" | "escape" => Key::Escape,
      "space" => Key::Space,
      "backspace" => Key::Backspace,
      "delete" | "del" => Key::Delete,
      "up" => Key::UpArrow,
      "down" => Key::DownArrow,
      "left" => Key::LeftArrow,
      "right" => Key::RightArrow,
      "home" => Key::Home,
      "end" => Key::End,
      "pageup" => Key::PageUp,
      "pagedown" => Key::PageDown,
      "capslock" => Key::CapsLock,
      "f1" => Key::F1, "f2" => Key::F2, "f3" => Key::F3, "f4" => Key::F4,
      "f5" => Key::F5, "f6" => Key::F6, "f7" => Key::F7, "f8" => Key::F8,
      "f9" => Key::F9, "f10" => Key::F10, "f11" => Key::F11, "f12" => Key::F12,
      "f13" => Key::F13, "f14" => Key::F14, "f15" => Key::F15, "f16" => Key::F16,
      "f17" => Key::F17, "f18" => Key::F18, "f19" => Key::F19, "f20" => Key::F20,
      "f21" => Key::F21, "f22" => Key::F22, "f23" => Key::F23, "f24" => Key::F24,
      _ => {
        let mut chars = n.chars();
        let c = chars.next()?;
        if chars.next().is_some() { return None; }
        Key::Unicode(c)
      }
    })
  }

  // -------------------------------------------------------------------------
  // Clipboard & system
  // -------------------------------------------------------------------------
  struct ReadClipboard;

  #[async_trait]
  impl AgentTool for ReadClipboard {
    fn name(&self) -> String { "read_clipboard".into() }
    fn description(&self) -> String { "Read the current clipboard text.".into() }
    fn params_schema(&self) -> serde_json::Value { serde_json::json!({ "type": "object", "properties": {}, "required": [] }) }
    async fn run(&self, _args: &serde_json::Value) -> Result<String, String> {
      let text = arboard::Clipboard::new().map_err(|e| e.to_string())?.get_text().unwrap_or_default();
      Ok(if text.trim().is_empty() { "Clipboard is empty.".into() } else { clip(&text) })
    }
  }

  struct WriteClipboard;

  #[async_trait]
  impl AgentTool for WriteClipboard {
    fn name(&self) -> String { "write_clipboard".into() }
    fn description(&self) -> String { "Replace the clipboard with the given text. Params: text (string).".into() }
    fn params_schema(&self) -> serde_json::Value {
      serde_json::json!({ "type": "object", "properties": { "text": { "type": "string" } }, "required": ["text"] })
    }
    async fn run(&self, args: &serde_json::Value) -> Result<String, String> {
      let text = str_arg(args, "text").ok_or("write_clipboard requires 'text'.")?;
      arboard::Clipboard::new().map_err(|e| e.to_string())?.set_text(text).map_err(|e| e.to_string())?;
      Ok("Clipboard updated.".into())
    }
  }

  struct LaunchApp;

  #[async_trait]
  impl AgentTool for LaunchApp {
    fn name(&self) -> String { "launch_app".into() }
    fn description(&self) -> String { "Launch a program (e.g. notepad, explorer, chrome) with optional arguments. Params: program (string), args (array of strings, optional).".into() }
    fn params_schema(&self) -> serde_json::Value {
      serde_json::json!({ "type": "object", "properties": { "program": { "type": "string" }, "args": { "type": "array", "items": { "type": "string" } } }, "required": ["program"] })
    }
    async fn run(&self, args: &serde_json::Value) -> Result<String, String> {
      let program = str_arg(args, "program").ok_or("launch_app requires 'program'.")?;
      let extra: Vec<String> = args.get("args").and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect()).unwrap_or_default();
      std::process::Command::new(&program).args(&extra).spawn()
        .map_err(|e| format!("Could not launch '{program}': {e}"))?;
      Ok(format!("Launched {program}."))
    }
  }

  struct OpenPath;

  #[async_trait]
  impl AgentTool for OpenPath {
    fn name(&self) -> String { "open_path".into() }
    fn description(&self) -> String { "Open a file, folder, or URL with its default application. Params: path (string).".into() }
    fn params_schema(&self) -> serde_json::Value {
      serde_json::json!({ "type": "object", "properties": { "path": { "type": "string" } }, "required": ["path"] })
    }
    async fn run(&self, args: &serde_json::Value) -> Result<String, String> {
      let path = str_arg(args, "path").ok_or("open_path requires 'path'.")?;
      let op = wide("open");
      let file = wide(&path);
      let result = unsafe { ShellExecuteW(None, PCWSTR(op.as_ptr()), PCWSTR(file.as_ptr()), None, None, SW_SHOWNORMAL) };
      // ShellExecuteW returns a value > 32 on success.
      if result.0 as isize <= 32 { return Err(format!("Could not open '{path}'.")); }
      Ok(format!("Opened {path}."))
    }
  }

  fn wide(s: &str) -> Vec<u16> { s.encode_utf16().chain(std::iter::once(0)).collect() }

  struct LockScreen;

  #[async_trait]
  impl AgentTool for LockScreen {
    fn name(&self) -> String { "lock_screen".into() }
    fn description(&self) -> String { "Lock the workstation.".into() }
    fn params_schema(&self) -> serde_json::Value { serde_json::json!({ "type": "object", "properties": {}, "required": [] }) }
    async fn run(&self, _args: &serde_json::Value) -> Result<String, String> {
      unsafe { LockWorkStation() }.map_err(|e| e.to_string())?;
      Ok("Screen locked.".into())
    }
  }

  pub(crate) fn desktop_tools() -> Vec<Box<dyn AgentTool>> {
    vec![
      Box::new(ScreenCapture),
      Box::new(ListWindows),
      Box::new(FocusWindow),
      Box::new(MinimizeWindow),
      Box::new(MaximizeWindow),
      Box::new(CloseWindow),
      Box::new(MouseMove),
      Box::new(MouseClick),
      Box::new(MouseDrag),
      Box::new(Scroll),
      Box::new(TypeText),
      Box::new(PressKeys),
      Box::new(ReadClipboard),
      Box::new(WriteClipboard),
      Box::new(LaunchApp),
      Box::new(OpenPath),
      Box::new(LockScreen),
    ]
  }
}
