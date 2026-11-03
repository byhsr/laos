// Notifications — a durable queue for everything the app tells the user.
//
// Reminders, insights and finished work used to be fire-and-forget: an in-app
// event (lost if the window was closed to the tray), an OS toast (suppressed
// while focused), a Telegram push. Nothing survived a reload. This persists each
// as a row with an unread state, so there is a real inbox the user can revisit.

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

use crate::db::db;

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Notification {
  pub id: i64,
  pub kind: String,
  pub title: String,
  pub body: String,
  pub ref_kind: String,
  pub ref_id: String,
  pub created_at: String,
  pub read_at: Option<String>,
  pub dismissed_at: Option<String>,
}

const SELECT: &str =
  "SELECT id, kind, title, body, ref_kind, ref_id, created_at, read_at, dismissed_at FROM notifications";

fn row(r: &rusqlite::Row) -> rusqlite::Result<Notification> {
  Ok(Notification {
    id: r.get(0)?, kind: r.get(1)?, title: r.get(2)?, body: r.get(3)?,
    ref_kind: r.get(4)?, ref_id: r.get(5)?, created_at: r.get(6)?,
    read_at: r.get(7)?, dismissed_at: r.get(8)?,
  })
}

fn now_iso() -> String { chrono::Utc::now().to_rfc3339() }

/// Inserts a notification and prunes the queue to the newest 500 (like
/// `telegram_logs`). Returns the new row's id.
pub(crate) fn push(conn: &Connection, kind: &str, title: &str, body: &str, ref_kind: &str, ref_id: &str) -> Result<i64, String> {
  conn.execute(
    "INSERT INTO notifications (kind, title, body, ref_kind, ref_id, created_at) VALUES (?1,?2,?3,?4,?5,?6)",
    params![kind, title, body, ref_kind, ref_id, now_iso()],
  ).map_err(|e| e.to_string())?;
  let id = conn.last_insert_rowid();
  let _ = conn.execute(
    "DELETE FROM notifications WHERE id NOT IN (SELECT id FROM notifications ORDER BY id DESC LIMIT 500)",
    [],
  );
  Ok(id)
}

/// Push a notification and tell the frontend, so the badge and toast update live.
pub(crate) fn push_event(app: &AppHandle, conn: &Connection, kind: &str, title: &str, body: &str, ref_kind: &str, ref_id: &str) {
  if let Ok(id) = push(conn, kind, title, body, ref_kind, ref_id) {
    let _ = app.emit("lumin-notification", serde_json::json!({
      "id": id, "kind": kind, "title": title, "body": body,
      "refKind": ref_kind, "refId": ref_id,
    }));
  }
}

#[tauri::command]
pub fn list_notifications(app: AppHandle, unread_only: Option<bool>) -> Result<Vec<Notification>, String> {
  let conn = db(&app)?;
  let sql = if unread_only.unwrap_or(false) {
    format!("{SELECT} WHERE dismissed_at IS NULL AND read_at IS NULL ORDER BY id DESC LIMIT 200")
  } else {
    format!("{SELECT} WHERE dismissed_at IS NULL ORDER BY id DESC LIMIT 200")
  };
  let mut out = Vec::new();
  if let Ok(mut stmt) = conn.prepare(&sql) {
    if let Ok(rows) = stmt.query_map([], row) {
      for r in rows { if let Ok(n) = r { out.push(n); } }
    }
  }
  Ok(out)
}

#[tauri::command]
pub fn notification_unread_count(app: AppHandle) -> Result<i64, String> {
  let conn = db(&app)?;
  conn.query_row(
    "SELECT COUNT(*) FROM notifications WHERE dismissed_at IS NULL AND read_at IS NULL",
    [], |r| r.get::<_, i64>(0),
  ).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn mark_notification_read(app: AppHandle, id: i64) -> Result<(), String> {
  let conn = db(&app)?;
  conn.execute("UPDATE notifications SET read_at=?2 WHERE id=?1 AND read_at IS NULL", params![id, now_iso()])
    .map(|_| ()).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn dismiss_notification(app: AppHandle, id: i64) -> Result<(), String> {
  let conn = db(&app)?;
  conn.execute(
    "UPDATE notifications SET dismissed_at=?2, read_at=COALESCE(read_at, ?2) WHERE id=?1",
    params![id, now_iso()],
  ).map(|_| ()).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn mark_all_notifications_read(app: AppHandle) -> Result<(), String> {
  let conn = db(&app)?;
  conn.execute("UPDATE notifications SET read_at=?1 WHERE read_at IS NULL AND dismissed_at IS NULL", params![now_iso()])
    .map(|_| ()).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn clear_notifications(app: AppHandle) -> Result<(), String> {
  let conn = db(&app)?;
  conn.execute("DELETE FROM notifications", []).map(|_| ()).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
  use super::*;

  fn db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
      "CREATE TABLE notifications (id INTEGER PRIMARY KEY AUTOINCREMENT, kind TEXT NOT NULL, title TEXT NOT NULL, \
       body TEXT NOT NULL DEFAULT '', ref_kind TEXT NOT NULL DEFAULT '', ref_id TEXT NOT NULL DEFAULT '', \
       created_at TEXT NOT NULL, read_at TEXT, dismissed_at TEXT);",
    ).unwrap();
    conn
  }

  fn unread(conn: &Connection) -> i64 {
    conn.query_row("SELECT COUNT(*) FROM notifications WHERE dismissed_at IS NULL AND read_at IS NULL", [], |r| r.get(0)).unwrap()
  }

  /// A pushed notification is unread; reading and dismissing each clear it once.
  #[test]
  fn a_notification_queues_then_clears() {
    let conn = db();
    let id = push(&conn, "reminder", "due now", "call the dentist", "item", "item-1").unwrap();
    assert_eq!(unread(&conn), 1);
    conn.execute("UPDATE notifications SET read_at=?2 WHERE id=?1", rusqlite::params![id, "t"]).unwrap();
    assert_eq!(unread(&conn), 0);
    assert_eq!(row_count(&conn), 1, "reading keeps the row in history");
    conn.execute("UPDATE notifications SET dismissed_at=?2 WHERE id=?1", rusqlite::params![id, "t"]).unwrap();
    assert_eq!(row_count(&conn), 1, "dismissing is a state, not a delete");
  }

  fn row_count(conn: &Connection) -> i64 {
    conn.query_row("SELECT COUNT(*) FROM notifications", [], |r| r.get(0)).unwrap()
  }

  /// The queue is capped, so a long-running install can't grow it without bound.
  #[test]
  fn the_queue_is_pruned_to_the_newest_500() {
    let conn = db();
    for i in 0..510 { push(&conn, "system", "n", &i.to_string(), "", "").unwrap(); }
    assert_eq!(row_count(&conn), 500);
    let newest: String = conn.query_row("SELECT body FROM notifications ORDER BY id DESC LIMIT 1", [], |r| r.get(0)).unwrap();
    assert_eq!(newest, "509", "the newest survive");
  }
}
