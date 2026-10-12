// Reminders + habits — the app's only time-based machinery. A 30 s tick finds due
// items, notifies the bot that captured them, emits an in-app event, and either
// marks the item fired or rolls it forward on its cadence.

use std::time::Duration;

use rusqlite::{params, Connection};
use tauri::{AppHandle, Emitter};

use crate::capture::Item;
use crate::db::db;

fn parse(iso: &str) -> Option<chrono::DateTime<chrono::FixedOffset>> {
  chrono::DateTime::parse_from_rfc3339(iso).ok()
}

fn is_due(item: &Item, now: chrono::DateTime<chrono::FixedOffset>) -> bool {
  item.status == "active"
    && item.fired_at.is_none()
    && item.due_at.as_deref().and_then(parse).map(|d| d <= now).unwrap_or(false)
}

/// The next occurrence for a recurring item, or None when it doesn't repeat.
pub(crate) fn next_due(current: &Option<String>, cadence: &str) -> Option<String> {
  let cadence = cadence.trim();
  if cadence.is_empty() { return None; }
  let base = current.as_deref().and_then(parse)
    .map(|d| d.with_timezone(&chrono::Local))
    .unwrap_or_else(chrono::Local::now);
  let next = match cadence {
    "weekly" => base + chrono::Duration::days(7),
    "monthly" => base + chrono::Duration::days(30),
    _ => base + chrono::Duration::days(1),
  };
  Some(next.to_rfc3339())
}

fn line(item: &Item) -> String {
  let title = if item.title.trim().is_empty() { &item.body } else { &item.title };
  match item.kind.as_str() {
    "habit" => format!("⏰ habit: {title}"),
    _ => format!("⏰ reminder: {title}"),
  }
}

pub(crate) fn due_summary(conn: &Connection) -> String {
  let now = chrono::Local::now().fixed_offset();
  let mut due: Vec<Item> = crate::capture::list(conn, None).into_iter().filter(|i| is_due(i, now)).collect();
  due.sort_by(|a, b| a.due_at.cmp(&b.due_at));
  if due.is_empty() { return "Nothing due right now.".into(); }
  due.iter().map(|i| {
    let title = if i.title.trim().is_empty() { &i.body } else { &i.title };
    let when = i.due_at.as_deref().and_then(parse)
      .map(|d| d.with_timezone(&chrono::Local).format("%b %d %H:%M").to_string())
      .unwrap_or_default();
    format!("• [{}] {title} · {when}", i.kind)
  }).collect::<Vec<_>>().join("\n")
}

struct Fired {
  item: Item,
  token: String,
  chat_id: String,
}

// Collects everything due. Takes a connection and returns owned data — the caller
// must not hold the connection across the network awaits below.
fn collect_due(conn: &Connection) -> Vec<Fired> {
  let now = chrono::Local::now().fixed_offset();
  crate::capture::list(conn, None).into_iter()
    .filter(|i| i.due_at.is_some() && is_due(i, now))
    .map(|item| {
      let (token, chat_id) = crate::telegram::owner_target(conn, &item.source).unwrap_or_default();
      Fired { item, token, chat_id }
    })
    .collect()
}

pub(crate) async fn reminder_loop(app: AppHandle) {
  loop {
    let fired = match db(&app) { Ok(conn) => collect_due(&conn), Err(_) => Vec::new() };

    for f in fired {
      if !f.token.is_empty() && !f.chat_id.is_empty() {
        let _ = crate::telegram::notify(&f.token, &f.chat_id, &line(&f.item)).await;
      }
      // The app shows it too (App.tsx listens and toasts).
      let _ = app.emit("lumin-reminder", serde_json::json!({
        "id": f.item.id, "kind": f.item.kind,
        "title": if f.item.title.trim().is_empty() { f.item.body.clone() } else { f.item.title.clone() },
      }));

      if let Ok(conn) = db(&app) {
        let ts = chrono::Utc::now().to_rfc3339();
        match next_due(&f.item.due_at, &f.item.cadence) {
          // Recurring: roll to the next occurrence rather than clearing it.
          Some(next) => { let _ = conn.execute("UPDATE items SET due_at=?2, fired_at=NULL, updated_at=?3 WHERE id=?1", params![f.item.id, next, ts]); }
          None => { let _ = conn.execute("UPDATE items SET fired_at=?2, updated_at=?2 WHERE id=?1", params![f.item.id, ts]); }
        }
      }
    }

    tokio::time::sleep(Duration::from_secs(30)).await;
  }
}

#[tauri::command]
pub fn item_due(app: AppHandle, upcoming: Option<bool>) -> Result<Vec<Item>, String> {
  let conn = db(&app)?;
  let now = chrono::Local::now().fixed_offset();
  let mut items: Vec<Item> = crate::capture::list(&conn, None).into_iter()
    .filter(|i| i.status == "active" && i.due_at.is_some())
    .filter(|i| if upcoming.unwrap_or(false) { !is_due(i, now) } else { is_due(i, now) })
    .collect();
  items.sort_by(|a, b| a.due_at.cmp(&b.due_at));
  Ok(items)
}

#[tauri::command]
pub fn reminder_snooze(app: AppHandle, id: String, minutes: Option<i64>) -> Result<(), String> {
  let conn = db(&app)?;
  let next = chrono::Local::now() + chrono::Duration::minutes(minutes.unwrap_or(10));
  conn.execute("UPDATE items SET due_at=?2, fired_at=NULL, updated_at=?3 WHERE id=?1",
    params![id, next.to_rfc3339(), chrono::Utc::now().to_rfc3339()]).map_err(|e| e.to_string())?;
  Ok(())
}
