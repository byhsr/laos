// Reminders + habits — the app's only time-based machinery. A 30 s tick finds due
// items, notifies the bot that captured them, emits an in-app event, and either
// marks the item fired or rolls it forward on its cadence.

use std::time::Duration;

use rusqlite::{params, Connection};
use tauri::AppHandle;

use crate::capture::Item;
use crate::db::db;

fn parse(iso: &str) -> Option<chrono::DateTime<chrono::FixedOffset>> {
  let s = iso.trim();
  if s.is_empty() { return None; }
  if let Ok(d) = chrono::DateTime::parse_from_rfc3339(s) { return Some(d); }
  // A time with no offset — the classifier often returns one — is read as local
  // rather than silently failing and making the item inert forever.
  for fmt in ["%Y-%m-%dT%H:%M:%S", "%Y-%m-%dT%H:%M", "%Y-%m-%d %H:%M:%S", "%Y-%m-%d %H:%M"] {
    if let Ok(naive) = chrono::NaiveDateTime::parse_from_str(s, fmt) {
      if let chrono::LocalResult::Single(dt) = chrono::TimeZone::from_local_datetime(&chrono::Local, &naive) {
        return Some(dt.fixed_offset());
      }
    }
  }
  None
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
  // Name the kind so a timed todo/goal doesn't masquerade as a reminder.
  let label = match item.kind.trim() {
    "" | "reminder" => "reminder",
    other => other,
  };
  format!("⏰ {label}: {title}")
}

pub(crate) fn due_summary(conn: &Connection) -> String {
  let now = chrono::Utc::now().fixed_offset();
  let mut due: Vec<Item> = crate::capture::timed_items(conn).into_iter().filter(|i| is_due(i, now)).collect();
  due.sort_by(|a, b| a.due_at.cmp(&b.due_at));
  if due.is_empty() { return "Nothing due right now.".into(); }
  due.iter().map(|i| {
    let title = if i.title.trim().is_empty() { &i.body } else { &i.title };
    let when = i.due_at.as_deref().and_then(|d| parse(d).map(|_| crate::tz::format(conn, d))).unwrap_or_default();
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
  let now = chrono::Utc::now().fixed_offset();
  crate::capture::timed_items(conn).into_iter()
    .filter(|i| is_due(i, now))
    .map(|item| {
      // The bot that captured the item, falling back to the owner bot so a
      // reminder created by an agent (or in the app) still reaches Telegram.
      let (token, chat_id) = crate::telegram::owner_target(conn, &item.source)
        .or_else(|| crate::telegram::default_owner(conn))
        .unwrap_or_default();
      Fired { item, token, chat_id }
    })
    .collect()
}

pub(crate) async fn reminder_loop(app: AppHandle) {
  loop {
    let fired = match db(&app) { Ok(conn) => collect_due(&conn), Err(_) => Vec::new() };

    for f in fired {
      // Notify the capturing bot. A failure must not silently lose the reminder:
      // log it and leave the item due, rescheduled shortly, so it retries.
      let mut failed = false;
      if !f.token.is_empty() && !f.chat_id.is_empty() {
        let send = tokio::time::timeout(
          Duration::from_secs(20),
          crate::telegram::notify(&f.token, &f.chat_id, &line(&f.item)),
        ).await;
        match send {
          Ok(Ok(())) => {}
          Ok(Err(e)) => { failed = true; crate::telegram::log_error(&app, &format!("reminder “{}” not delivered: {e}", line(&f.item))); }
          Err(_) => { failed = true; crate::telegram::log_error(&app, &format!("reminder “{}” not delivered: timed out", line(&f.item))); }
        }
      }
      let title = if f.item.title.trim().is_empty() { f.item.body.clone() } else { f.item.title.clone() };
      // Queue it durably AND toast it. The queue survives a reload (and the window
      // sitting on the tray); the OS toast fires only when the app isn't in front.
      let line = if f.item.kind == "habit" { format!("habit: {title}") } else { format!("reminder: {title}") };
      crate::notify::notify_and_push(&app, &f.item.kind, "⏰ due now", &line, "item", &f.item.id);

      if let Ok(conn) = db(&app) {
        let ts = chrono::Utc::now().to_rfc3339();
        if failed {
          // Retry in five minutes rather than every tick.
          let retry = chrono::Utc::now() + chrono::Duration::minutes(5);
          let _ = conn.execute("UPDATE items SET due_at=?2, updated_at=?3 WHERE id=?1", params![f.item.id, retry.to_rfc3339(), ts]);
          continue;
        }
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
  let now = chrono::Utc::now().fixed_offset();
  let mut items: Vec<Item> = crate::capture::timed_items(&conn).into_iter()
    .filter(|i| i.status == "active")
    .filter(|i| {
      if upcoming.unwrap_or(false) {
        // Strictly ahead of now — not "anything not currently due", which would
        // resurrect an item already fired today.
        i.due_at.as_deref().and_then(parse).map(|d| d > now).unwrap_or(false)
      } else {
        is_due(i, now)
      }
    })
    .collect();
  items.sort_by(|a, b| a.due_at.cmp(&b.due_at));
  Ok(items)
}

#[tauri::command]
pub fn reminder_snooze(app: AppHandle, id: String, minutes: Option<i64>) -> Result<(), String> {
  let conn = db(&app)?;
  let next = chrono::Utc::now() + chrono::Duration::minutes(minutes.unwrap_or(10));
  conn.execute("UPDATE items SET due_at=?2, fired_at=NULL, updated_at=?3 WHERE id=?1",
    params![id, next.to_rfc3339(), chrono::Utc::now().to_rfc3339()]).map_err(|e| e.to_string())?;
  Ok(())
}
