// Long-lived PTY sessions: run a real CLI (e.g. `cmdc`) and drive it from the
// panel (xterm) or, later, from a bound Telegram bot. One process per session,
// a rolling output ring, and a broadcast to attached channels.

use std::collections::{HashMap, VecDeque};
use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use portable_pty::{native_pty_system, CommandBuilder, MasterPty, PtySize};
use tauri::AppHandle;

const RING_CAP: usize = 64 * 1024;

struct TermSession {
  id: String,
  name: String,
  command: String,
  alive: Arc<AtomicBool>,
  ring: Arc<Mutex<VecDeque<u8>>>,
  total: Arc<Mutex<u64>>,
  master: Mutex<Box<dyn MasterPty + Send>>,
  writer: Mutex<Box<dyn Write + Send>>,
  child: Mutex<Box<dyn portable_pty::Child + Send + Sync>>,
  subscribers: Arc<Mutex<Vec<tauri::ipc::Channel<String>>>>,
  // Renders the screen from the raw byte stream, so the relay can read the visible
  // text instead of reconstructing it from cursor-positioning escapes.
  parser: Arc<Mutex<vt100::Parser>>,
}

fn registry() -> &'static Mutex<HashMap<String, Arc<TermSession>>> {
  static REG: OnceLock<Mutex<HashMap<String, Arc<TermSession>>>> = OnceLock::new();
  REG.get_or_init(|| Mutex::new(HashMap::new()))
}

fn get_session(id: &str) -> Result<Arc<TermSession>, String> {
  registry().lock().map_err(|_| "terminal registry poisoned".to_string())?
    .get(id).cloned().ok_or_else(|| format!("No terminal session '{id}'."))
}

// Strips ANSI escape sequences and control chars for a plain-text excerpt.
// Handles CSI (`ESC [ … final`), OSC (`ESC ] … BEL` / `ESC \`, e.g. window titles)
// and single-char escapes — otherwise OSC payloads leak in as text.
fn strip_ansi(s: &str) -> String {
  let mut out = String::with_capacity(s.len());
  let mut chars = s.chars().peekable();
  while let Some(c) = chars.next() {
    if c == '\u{1b}' {
      match chars.peek() {
        Some('[') => { chars.next(); while let Some(&n) = chars.peek() { chars.next(); if ('@'..='~').contains(&n) { break; } } }
        Some(']') => {
          chars.next();
          while let Some(&n) = chars.peek() {
            chars.next();
            if n == '\u{7}' { break; }
            if n == '\u{1b}' { if chars.peek() == Some(&'\\') { chars.next(); } break; }
          }
        }
        Some('(') | Some(')') | Some('*') | Some('+') => { chars.next(); chars.next(); }
        _ => { let _ = chars.next(); }
      }
    } else if c == '\r' { /* drop CR */ }
    else if c == '\n' || c == '\t' || !c.is_control() { out.push(c); }
  }
  out
}

// Leading UI glyphs a TUI puts before prompts/spinners/answers (·, ○, ✻, ❯, and
// braille spinners). Stripped so we judge the text itself, not the marker — an
// answer like "⠶ Hey! …" must survive.
fn strip_lead_glyphs(l: &str) -> String {
  l.trim_start_matches(|c: char| matches!(c,
    '·' | '○' | '◇' | '◆' | '✻' | '✳' | '✽' | '✦' | '⏺' | '❯' | '➜' | '›' | '»' | '•' |
    '⠋' | '⠙' | '⠹' | '⠸' | '⠼' | '⠴' | '⠦' | '⠧' | '⠇' | '⠏' | '⠶' | '⠷' | '⠿'
  )).trim_start().to_string()
}

// UI chrome a TUI redraws constantly — prompts, spinner status, hints, the banner.
// Judged on the glyph-stripped text, so a real answer carrying a spinner marker is kept.
// Deliberately narrow: only fixed UI strings and status lines, so file bodies, TODOs,
// diffs and code (which may contain words like "models:" or "exit") are never dropped.
fn is_chrome(t: &str) -> bool {
  let l = t.trim();
  if l.is_empty() { return true; }
  // rule/separator lines
  if l.chars().count() >= 3 && l.chars().all(|c| matches!(c, '─' | '═' | '━' | '—' | '-' | '_' | '=' | '·' | '~' | ' ')) { return true; }
  // ASCII-art / logo blocks
  let total = l.chars().count();
  let blocks = l.chars().filter(|c| matches!(c, '█' | '▓' | '▒' | '░')).count();
  if total > 0 && blocks * 2 >= total { return true; }
  let low = l.to_lowercase();
  if low.contains("esc to interrupt") || low.contains("for shortcuts") || low.contains("ask your question")
    || low.contains("pastes images") || low.contains("pastes text only") || low.contains("invalid json")
    || low.contains("command code v") || low.contains("alt+v") || low.contains("ctrl+o to expand") {
    return true;
  }
  // status lines: "Worked for 5s" / "Thought for 1 second" (short, after glyph strip)
  if (low.starts_with("worked for") || low.starts_with("thought for")) && low.len() < 40 { return true; }
  false
}

// A compact view of a session for the Telegram relay: the rendered screen with
// chrome lines dropped. No byte-level guessing — the VT parser already applied
// cursor moves and erases, so there are no partial frames to collapse.
pub(crate) fn compact_text(id: &str, max_lines: usize) -> String {
  let Ok(s) = get_session(id) else { return String::new() };
  let text = match s.parser.lock() { Ok(p) => p.screen().contents(), Err(_) => return String::new() };
  let mut out: Vec<String> = Vec::new();
  for raw in text.lines() {
    let line = raw.trim_end();
    if line.trim().is_empty() { continue; }
    // The input prompt echoes what we sent — not output.
    if line.trim_start().starts_with('❯') { continue; }
    let core = strip_lead_glyphs(line);
    if is_chrome(&core) { continue; }
    out.push(core);
  }
  let start = out.len().saturating_sub(max_lines);
  out[start..].join("\n")
}

/// Runs a command line through the platform shell inside a PTY.
#[tauri::command]
pub fn terminal_start(app: AppHandle, name: String, command: String, cwd: Option<String>, cols: Option<u16>, rows: Option<u16>) -> Result<serde_json::Value, String> {
  let id = format!("term-{}", chrono::Utc::now().timestamp_millis());
  let size = PtySize { rows: rows.unwrap_or(30), cols: cols.unwrap_or(100), pixel_width: 0, pixel_height: 0 };
  let pty = native_pty_system();
  let pair = pty.openpty(size).map_err(|e| format!("Could not open a PTY: {e}"))?;

  // Run through the system shell so `command` is a line, not a bare program.
  #[cfg(windows)]
  let mut cmd = { let mut c = CommandBuilder::new("cmd"); c.arg("/C"); c.arg(&command); c };
  #[cfg(not(windows))]
  let mut cmd = { let mut c = CommandBuilder::new("sh"); c.arg("-c"); c.arg(&command); c };
  if let Some(dir) = cwd.filter(|d| !d.trim().is_empty()) { cmd.cwd(dir); }

  let child = pair.slave.spawn_command(cmd).map_err(|e| format!("Could not start '{command}': {e}"))?;
  drop(pair.slave);

  let mut reader = pair.master.try_clone_reader().map_err(|e| e.to_string())?;
  let writer = pair.master.take_writer().map_err(|e| e.to_string())?;

  let alive = Arc::new(AtomicBool::new(true));
  let ring: Arc<Mutex<VecDeque<u8>>> = Arc::new(Mutex::new(VecDeque::new()));
  let total: Arc<Mutex<u64>> = Arc::new(Mutex::new(0));
  let subscribers: Arc<Mutex<Vec<tauri::ipc::Channel<String>>>> = Arc::new(Mutex::new(Vec::new()));
  let parser = Arc::new(Mutex::new(vt100::Parser::new(size.rows, size.cols, 0)));

  let session = Arc::new(TermSession {
    id: id.clone(), name: name.clone(), command: command.clone(),
    alive: alive.clone(), ring: ring.clone(), total: total.clone(),
    master: Mutex::new(pair.master), writer: Mutex::new(writer), child: Mutex::new(child),
    subscribers: subscribers.clone(), parser: parser.clone(),
  });
  registry().lock().map_err(|_| "terminal registry poisoned".to_string())?.insert(id.clone(), session);

  // Reader thread: append to the ring, broadcast to attached channels. Also lets
  // the Telegram relay (commit 2) observe output via a shared hook.
  let app_bg = app.clone();
  let id_bg = id.clone();
  std::thread::spawn(move || {
    let mut buf = [0u8; 8192];
    loop {
      match reader.read(&mut buf) {
        Ok(0) | Err(_) => break,
        Ok(n) => {
          let chunk = &buf[..n];
          if let Ok(mut r) = ring.lock() {
            r.extend(chunk.iter().copied());
            while r.len() > RING_CAP { r.pop_front(); }
          }
          if let Ok(mut t) = total.lock() { *t += n as u64; }
          if let Ok(mut p) = parser.lock() { p.process(chunk); }
          let text = String::from_utf8_lossy(chunk).to_string();
          if let Ok(mut subs) = subscribers.lock() {
            subs.retain(|ch| ch.send(text.clone()).is_ok());
          }
          crate::telegram::on_terminal_output(&app_bg, &id_bg, &text);
        }
      }
    }
    alive.store(false, Ordering::SeqCst);
    if let Ok(mut subs) = subscribers.lock() {
      for ch in subs.iter() { let _ = ch.send("\r\n[process exited]\r\n".to_string()); }
      subs.clear();
    }
  });

  Ok(serde_json::json!({ "id": id, "name": name }))
}

#[tauri::command]
pub fn terminal_write(id: String, data: String) -> Result<(), String> {
  let s = get_session(&id)?;
  let mut w = s.writer.lock().map_err(|_| "session locked".to_string())?;
  w.write_all(data.as_bytes()).map_err(|e| e.to_string())?;
  w.flush().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn terminal_resize(id: String, cols: u16, rows: u16) -> Result<(), String> {
  let s = get_session(&id)?;
  {
    let m = s.master.lock().map_err(|_| "session locked".to_string())?;
    m.resize(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 }).map_err(|e| e.to_string())?;
  }
  if let Ok(mut p) = s.parser.lock() { p.set_size(rows, cols); }
  Ok(())
}

#[tauri::command]
pub fn terminal_read(id: String, from: u64) -> Result<serde_json::Value, String> {
  let s = get_session(&id)?;
  let ring = s.ring.lock().map_err(|_| "session locked".to_string())?;
  let total = *s.total.lock().map_err(|_| "session locked".to_string())?;
  let start = total.saturating_sub(ring.len() as u64);
  let begin = from.max(start);
  let skip = (begin - start) as usize;
  let bytes: Vec<u8> = ring.iter().skip(skip).copied().collect();
  Ok(serde_json::json!({ "data": bytes, "next": total }))
}

#[tauri::command]
pub fn terminal_attach(id: String, on_output: tauri::ipc::Channel<String>) -> Result<(), String> {
  let s = get_session(&id)?;
  // Seed the terminal with the current buffer, then subscribe to new output.
  if let Ok(ring) = s.ring.lock() {
    let text = String::from_utf8_lossy(&ring.iter().copied().collect::<Vec<u8>>()).to_string();
    if !text.is_empty() { let _ = on_output.send(text); }
  }
  s.subscribers.lock().map_err(|_| "session locked".to_string())?.push(on_output);
  Ok(())
}

#[tauri::command]
pub fn terminal_list() -> Result<Vec<serde_json::Value>, String> {
  let reg = registry().lock().map_err(|_| "terminal registry poisoned".to_string())?;
  Ok(reg.values().map(|s| {
    let excerpt = s.ring.lock().ok()
      .map(|r| { let text = String::from_utf8_lossy(&r.iter().copied().collect::<Vec<u8>>()).to_string(); strip_ansi(&text) })
      .unwrap_or_default();
    let tail: String = excerpt.chars().rev().take(200).collect::<Vec<_>>().into_iter().rev().collect();
    serde_json::json!({ "id": s.id, "name": s.name, "command": s.command, "alive": s.alive.load(Ordering::SeqCst), "excerpt": tail })
  }).collect())
}

#[tauri::command]
pub fn terminal_kill(id: String) -> Result<(), String> {
  let s = get_session(&id)?;
  if let Ok(mut c) = s.child.lock() { let _ = c.kill(); }
  s.alive.store(false, Ordering::SeqCst);
  registry().lock().map_err(|_| "terminal registry poisoned".to_string())?.remove(&id);
  Ok(())
}

/// Writes a line to a session's stdin (used by the Telegram bridge). The text is
/// typed one character at a time, then Enter is sent as its own keystroke: a TUI
/// (Ink/readline) sees a fast text-plus-newline burst as a paste and holds it, so
/// the line just sits there and never reaches the tool. Real keystrokes submit.
pub(crate) fn write_line(id: &str, line: &str) -> Result<(), String> {
  let s = get_session(id)?;
  {
    let mut w = s.writer.lock().map_err(|_| "session locked".to_string())?;
    for ch in line.chars() {
      let mut buf = [0u8; 4];
      w.write_all(ch.encode_utf8(&mut buf).as_bytes()).map_err(|e| e.to_string())?;
      w.flush().map_err(|e| e.to_string())?;
      std::thread::sleep(std::time::Duration::from_millis(8));
    }
  }
  if !line.is_empty() { std::thread::sleep(std::time::Duration::from_millis(90)); }
  let mut w = s.writer.lock().map_err(|_| "session locked".to_string())?;
  w.write_all(b"\r").map_err(|e| e.to_string())?;
  w.flush().map_err(|e| e.to_string())
}

/// The plain-text tail of a session's buffer (raw-ish; kept for the panel).
#[allow(dead_code)]
pub(crate) fn tail_text(id: &str, max_lines: usize) -> String {
  let Ok(s) = get_session(id) else { return String::new() };
  let ring = match s.ring.lock() { Ok(r) => r, Err(_) => return String::new() };
  let text = strip_ansi(&String::from_utf8_lossy(&ring.iter().copied().collect::<Vec<u8>>()));
  let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
  lines[lines.len().saturating_sub(max_lines)..].join("\n")
}

/// Whether a live session exists.
pub(crate) fn session_alive(id: &str) -> bool {
  get_session(id).map(|s| s.alive.load(Ordering::SeqCst)).unwrap_or(false)
}
