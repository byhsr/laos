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
  let _ = app.notification().builder().title(title).body(body).show();
}
