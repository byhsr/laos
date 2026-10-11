// App-level settings (key/value) plus the two "keep it running" switches:
//   run_in_background — closing the window hides to the tray instead of quitting,
//                       so the Telegram bots keep polling;
//   launch_at_login   — start with the OS (tauri-plugin-autostart).

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::AppHandle;
use tauri_plugin_autostart::ManagerExt;

use crate::db::db;

// A named `/cli` shortcut: `name` is the alias, `command` the shell/launch line
// (empty → the default shell) and `cwd` the folder to start in.
#[derive(Serialize, Deserialize, Clone)]
pub struct CliShortcut {
  #[serde(default)] pub name: String,
  #[serde(default)] pub command: String,
  #[serde(default)] pub cwd: String,
}

pub(crate) fn get(conn: &Connection, key: &str) -> Option<String> {
  conn.query_row("SELECT value FROM settings WHERE key=?1", params![key], |r| r.get(0)).ok()
}

pub(crate) fn set(conn: &Connection, key: &str, value: &str) -> Result<(), String> {
  conn.execute(
    "INSERT INTO settings (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
    params![key, value],
  ).map_err(|e| e.to_string())?;
  Ok(())
}

fn flag(v: Option<String>, default: bool) -> bool {
  v.map(|s| s == "1" || s.eq_ignore_ascii_case("true")).unwrap_or(default)
}

/// True when closing the window should hide to the tray rather than quit.
pub(crate) fn run_in_background(app: &AppHandle) -> bool {
  db(app).ok().map(|c| flag(get(&c, "run_in_background"), true)).unwrap_or(true)
}

/// When on, state-changing tools run without the in-app approval popup and the
/// manager doesn't stop to ask the user a question — it proceeds on its own.
pub(crate) fn autonomy(app: &AppHandle) -> bool {
  db(app).ok().map(|c| flag(get(&c, "autonomy"), true)).unwrap_or(true)
}

/// Shell launched when a session starts without an explicit command (`/cli`, new
/// terminals). Empty/unset falls back to the platform default (cmd / sh).
pub(crate) fn default_shell(app: &AppHandle) -> String {
  let fallback = if cfg!(windows) { "cmd" } else { "sh" };
  db(app).ok()
    .and_then(|c| get(&c, "default_shell"))
    .map(|s| s.trim().to_string())
    .filter(|s| !s.is_empty())
    .unwrap_or_else(|| fallback.to_string())
}

pub(crate) fn cli_shortcuts(conn: &Connection) -> Vec<CliShortcut> {
  get(conn, "cli_shortcuts").and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

/// Look up a `/cli <name>` shortcut (case-insensitive); None if there's no match.
pub(crate) fn resolve_cli_shortcut(app: &AppHandle, name: &str) -> Option<CliShortcut> {
  let name = name.trim();
  if name.is_empty() { return None; }
  let conn = db(app).ok()?;
  cli_shortcuts(&conn).into_iter().find(|c| c.name.trim().eq_ignore_ascii_case(name))
}

#[tauri::command]
pub fn get_app_settings(app: AppHandle) -> Result<Value, String> {
  let conn = db(&app)?;
  Ok(json!({
    "runInBackground": flag(get(&conn, "run_in_background"), true),
    "launchAtLogin": flag(get(&conn, "launch_at_login"), false),
    "autostartEnabled": app.autolaunch().is_enabled().unwrap_or(false),
    "defaultShell": default_shell(&app),
    "cliShortcuts": cli_shortcuts(&conn),
    "autonomy": autonomy(&app),
  }))
}

#[tauri::command]
pub fn set_autonomy(app: AppHandle, enabled: bool) -> Result<(), String> {
  let conn = db(&app)?;
  set(&conn, "autonomy", if enabled { "1" } else { "0" })
}

#[tauri::command]
pub fn set_cli_shortcuts(app: AppHandle, shortcuts: Vec<CliShortcut>) -> Result<(), String> {
  let conn = db(&app)?;
  let json = serde_json::to_string(&shortcuts).map_err(|e| e.to_string())?;
  set(&conn, "cli_shortcuts", &json)
}

#[tauri::command]
pub fn set_default_shell(app: AppHandle, shell: String) -> Result<(), String> {
  let conn = db(&app)?;
  set(&conn, "default_shell", shell.trim())
}

#[tauri::command]
pub fn set_run_in_background(app: AppHandle, enabled: bool) -> Result<(), String> {
  let conn = db(&app)?;
  set(&conn, "run_in_background", if enabled { "1" } else { "0" })
}

#[tauri::command]
pub fn set_launch_at_login(app: AppHandle, enabled: bool) -> Result<(), String> {
  {
    let conn = db(&app)?;
    set(&conn, "launch_at_login", if enabled { "1" } else { "0" })?;
  }
  let launcher = app.autolaunch();
  if enabled { launcher.enable().map_err(|e| e.to_string())?; } else { launcher.disable().map_err(|e| e.to_string())?; }
  Ok(())
}
