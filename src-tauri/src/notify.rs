// Desktop (OS-level) notifications.
//
// The app already shows an in-app toast for the same events, so a system toast is
// suppressed while the window is focused to avoid double-alerting. Every failure
// is swallowed — a notification is never worth breaking the work behind it.

use rusqlite::Connection;
use tauri::{AppHandle, Manager};
use tauri_plugin_notification::NotificationExt;

use crate::db::db;

/// Whether OS notifications are enabled (default on).
pub(crate) fn enabled(conn: &Connection) -> bool {
  crate::app_settings::get(conn, "desktop_notifications")
    .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
    .unwrap_or(true)
}

/// Raises a desktop notification, unless they're disabled or the app is already
/// in front (where the in-app toast has it covered).
pub(crate) fn desktop(app: &AppHandle, title: &str, body: &str) {
  let Ok(conn) = db(app) else { return };
  if !enabled(&conn) {
    return;
  }
  let focused = app
    .get_webview_window("main")
    .map(|w| w.is_focused().unwrap_or(false) && w.is_visible().unwrap_or(false))
    .unwrap_or(false);
  if focused {
    return;
  }
  let body: String = body.chars().take(180).collect();
  #[cfg(windows)]
  windows_toast(app, title, &body);
  #[cfg(not(windows))]
  {
    let _ = app.notification().builder().title(title).body(body).show();
  }
}

// Windows takes its own path. Tauri's notification plugin leaves the toast's
// AppUserModelID unset for dev builds (anything run from `target\debug` or
// `target\release`), and notify-rust then falls back to PowerShell's own id — so
// the toast is attributed to "Windows PowerShell". Showing it ourselves, with the
// app's id registered first, keeps the attribution on the app and lets a click
// bring the window forward.
#[cfg(windows)]
fn windows_toast(app: &AppHandle, title: &str, body: &str) {
  use tauri_winrt_notification::{Duration as ToastDuration, Toast};
  let (id, name) = app_identity(app);
  register_app_id(&id, &name);
  let handle = app.clone();
  let shown = Toast::new(&id)
    .title(title)
    .text1(body)
    .duration(ToastDuration::Short)
    // Windows reports a click only while the toast is still on screen, not once it
    // has moved to the Action Center.
    .on_activated(move |_action| {
      crate::show_main(&handle);
      Ok(())
    })
    .show();
  // If the toast could not be shown under our own identity, fall back to the
  // plugin so the alert still appears.
  if shown.is_err() {
    let _ = app.notification().builder().title(title).body(body).show();
  }
}

#[cfg(windows)]
fn app_identity(app: &AppHandle) -> (String, String) {
  let id = app.config().identifier.clone();
  let name = app.config().product_name.clone().unwrap_or_else(|| "Local Agent OS".into());
  (id, name)
}

/// Registers the AppUserModelID under HKCU once, so Windows shows the app's own
/// name on its toasts with no Start Menu shortcut, and delivers a toast for an id
/// it would otherwise refuse. Per-user and idempotent, so it needs no admin rights.
#[cfg(windows)]
fn register_app_id(id: &str, name: &str) {
  use std::sync::OnceLock;
  static ONCE: OnceLock<()> = OnceLock::new();
  ONCE.get_or_init(|| {
    let _ = write_app_id(id, name);
  });
}

#[cfg(windows)]
fn write_app_id(id: &str, name: &str) -> Result<(), String> {
  use windows::core::PCWSTR;
  use windows::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegSetValueExW, HKEY, HKEY_CURRENT_USER, KEY_WRITE, REG_OPTION_NON_VOLATILE, REG_SZ,
  };
  let wide = |s: &str| s.encode_utf16().chain(std::iter::once(0u16)).collect::<Vec<u16>>();
  let path = wide(&format!("Software\\Classes\\AppUserModelId\\{id}"));
  let value_name = wide("DisplayName");
  let value = wide(name);
  unsafe {
    let mut key = HKEY::default();
    RegCreateKeyExW(HKEY_CURRENT_USER, PCWSTR(path.as_ptr()), None, PCWSTR::null(), REG_OPTION_NON_VOLATILE, KEY_WRITE, None, &mut key, None)
      .ok()
      .map_err(|e| e.to_string())?;
    let bytes = std::slice::from_raw_parts(value.as_ptr() as *const u8, value.len() * std::mem::size_of::<u16>());
    let written = RegSetValueExW(key, PCWSTR(value_name.as_ptr()), None, REG_SZ, Some(bytes)).ok().map_err(|e| e.to_string());
    let _ = RegCloseKey(key);
    written
  }
}

/// Enqueue a durable notification AND raise the OS toast, in one call. The single
/// entry point for a real alert — a reminder, an insight, finished work — so the
/// in-app inbox, the badge and the toast never drift apart.
pub(crate) fn notify_and_push(app: &AppHandle, kind: &str, title: &str, body: &str, ref_kind: &str, ref_id: &str) {
  if let Ok(conn) = db(app) {
    crate::notifications::push_event(app, &conn, kind, title, body, ref_kind, ref_id);
  }
  desktop(app, title, body);
}
