# PC Control (computer use)

Native control of the user's machine: mouse and keyboard, screen capture, window and app
control, and clipboard/system actions. Owned by `src-tauri/src/tools/desktop.rs`.

Related: [tool-calls.md](./tool-calls.md) (the registry, permission gating, confirmation),
[harness.md](./harness.md) (image tool results).

## Opt-in

Grant an agent the **`pc_control`** permission (agent Config → permissions, or the Manager's
config) and it receives every desktop tool. Without it, none are offered. The permission is
never granted automatically — not even to the Manager.

> Screen captures are sent to the configured model, so on a cloud provider the screen
> contents leave the machine. Use a local model (Ollama) if that matters. Vision requires a
> vision-capable model.

## Tools

| Tool | Does | Confirms? |
| --- | --- | --- |
| `screen_capture` | Capture a monitor as an image (the model sees it) + its pixel size | no |
| `list_windows` | List visible top-level windows (`<handle>: <title>`) | no |
| `mouse_move` / `mouse_click` / `mouse_drag` / `scroll` | Mouse input | no |
| `type_text` / `press_keys` | Keyboard input / key chords | no |
| `focus_window` / `minimize_window` / `maximize_window` | Window control | no |
| `read_clipboard` | Read clipboard text | no |
| `launch_app` | Launch a program | **yes** |
| `close_window` | Send WM_CLOSE to a window | **yes** |
| `write_clipboard` | Replace the clipboard | **yes** |
| `open_path` | Open a file/folder/URL with the default app | **yes** |
| `lock_screen` | Lock the workstation | **yes** |

The confirm ones route through the same popup as `run_command` (see
[tool-calls.md](./tool-calls.md#the-confirmation-gate)). The low-level input tools do not
confirm individually — a vision-control loop issues many of them, and the permission grant is
the opt-in.

## The see → act loop

1. The model calls `screen_capture`. It receives the image plus text stating the capture's
   pixel size (and the physical screen size/scale).
2. It reasons over the image and calls an input tool with coordinates **in the captured
   image's pixel space** (origin top-left).
3. The loop repeats.

DPI is handled process-wide via `enigo::set_dpi_awareness()`, so the coordinates the model
reads off the image map 1:1 to real screen pixels. Images are downscaled to a max dimension of
1280 px to bound tokens; the text reports the sent size.

Only the most recent screenshot is carried in the context (`strip_old_images`), so a long
control loop doesn't re-send every frame.

## Platform & limits

- **Windows-first.** The desktop crates (`enigo`, `xcap`, `arboard`, `windows`) are
  Windows-target-only dependencies, so the macOS/Linux release builds don't pull X11/EVE system
  libs. On other platforms `desktop_tools()` returns nothing and `pc_control` grants nothing.
- `screen_capture` captures one monitor at a time (`monitor` index; default primary), not a
  stitched multi-monitor image.
- `launch_app` launches a program by name/path with args; it does not run a shell.
- Requires an input-capable desktop session (a normal logged-in Windows desktop). There is no
  "agent is controlling your PC" indicator yet — stop it with the chat's cancel control.
