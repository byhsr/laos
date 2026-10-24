// Capture — the Lumin core. Every message is an anchor: short commands file text
// exactly, free text goes through an extractor that turns messy language into a
// structured intent (feature + action + params, with times resolved to an
// absolute instant), and the intent is applied to the `items` store.

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use tauri::AppHandle;

use crate::db::db;

pub(crate) const KINDS: [&str; 6] = ["note", "todo", "reminder", "promise", "link", "habit"];
const CADENCES: [&str; 3] = ["daily", "weekly", "monthly"];

#[derive(Serialize, Deserialize, Clone)]
pub struct Item {
  pub id: String,
  #[serde(default = "default_kind")] pub kind: String,
  #[serde(default)] pub title: String,
  #[serde(default)] pub body: String,
  #[serde(default)] pub url: String,
  #[serde(default)] pub tags: Vec<String>,
  #[serde(default)] pub collection: String,
  #[serde(default = "default_status")] pub status: String,
  #[serde(default)] pub due_at: Option<String>,
  #[serde(default)] pub cadence: String,
  #[serde(default)] pub streak: i64,
  #[serde(default)] pub last_done_at: Option<String>,
  #[serde(default)] pub project_id: String,
  #[serde(default)] pub fired_at: Option<String>,
  #[serde(default)] pub source: String,
  #[serde(default)] pub created_at: String,
  #[serde(default)] pub updated_at: String,
}

fn default_kind() -> String { "note".into() }
fn default_status() -> String { "active".into() }
fn now() -> String { chrono::Utc::now().to_rfc3339() }

const SELECT: &str = "SELECT id, kind, title, body, url, tags, collection, status, due_at, cadence, streak, last_done_at, project_id, fired_at, source, created_at, updated_at FROM items";

pub(crate) fn row_to_item(row: &rusqlite::Row) -> rusqlite::Result<Item> {
  let tags: String = row.get(5)?;
  Ok(Item {
    id: row.get(0)?, kind: row.get(1)?, title: row.get(2)?, body: row.get(3)?, url: row.get(4)?,
    tags: serde_json::from_str(&tags).unwrap_or_default(),
    collection: row.get(6)?, status: row.get(7)?, due_at: row.get(8)?, cadence: row.get(9)?,
    streak: row.get(10)?, last_done_at: row.get(11)?, project_id: row.get(12)?, fired_at: row.get(13)?,
    source: row.get(14)?, created_at: row.get(15)?, updated_at: row.get(16)?,
  })
}

pub(crate) fn list(conn: &Connection, kind: Option<&str>) -> Vec<Item> {
  let mut out = Vec::new();
  let (sql, args): (String, Vec<String>) = match kind.filter(|k| !k.trim().is_empty()) {
    Some(k) => (format!("{SELECT} WHERE kind=?1 ORDER BY created_at DESC LIMIT 500"), vec![k.to_string()]),
    None => (format!("{SELECT} ORDER BY created_at DESC LIMIT 500"), vec![]),
  };
  if let Ok(mut stmt) = conn.prepare(&sql) {
    if let Ok(rows) = stmt.query_map(rusqlite::params_from_iter(args.iter()), row_to_item) {
      for r in rows { if let Ok(i) = r { out.push(i); } }
    }
  }
  out
}

fn find(conn: &Connection, query: &str) -> Option<Item> {
  let q = query.trim().to_lowercase();
  if q.is_empty() { return None; }
  list(conn, None).into_iter().find(|i| {
    i.title.to_lowercase().contains(&q) || i.body.to_lowercase().contains(&q)
  })
}

/// An open item whose title is exactly this text (case-insensitive). Used to make
/// free-text ingestion idempotent: filing the same thought twice updates one item.
pub(crate) fn find_exact_active(conn: &Connection, title: &str) -> Option<Item> {
  let t = title.trim().to_lowercase();
  if t.is_empty() { return None; }
  list(conn, None).into_iter().find(|i| i.status == "active" && i.title.trim().to_lowercase() == t)
}

pub(crate) fn upsert(conn: &Connection, item: &Item) -> Result<(), String> {
  let tags = serde_json::to_string(&item.tags).unwrap_or_else(|_| "[]".into());
  let created = if item.created_at.trim().is_empty() { now() } else { item.created_at.clone() };
  conn.execute(
    "INSERT INTO items (id, kind, title, body, url, tags, collection, status, due_at, cadence, streak, last_done_at, project_id, fired_at, source, created_at, updated_at) \
     VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17) \
     ON CONFLICT(id) DO UPDATE SET kind=excluded.kind, title=excluded.title, body=excluded.body, url=excluded.url, \
     tags=excluded.tags, collection=excluded.collection, status=excluded.status, due_at=excluded.due_at, \
     cadence=excluded.cadence, streak=excluded.streak, last_done_at=excluded.last_done_at, project_id=excluded.project_id, \
     fired_at=excluded.fired_at, updated_at=excluded.updated_at",
    params![
      item.id, item.kind, item.title, item.body, item.url, tags, item.collection, item.status,
      item.due_at, item.cadence, item.streak, item.last_done_at, item.project_id, item.fired_at,
      item.source, created, now()
    ],
  ).map_err(|e| e.to_string())?;
  Ok(())
}

// What a new item is built from — a struct rather than eight positional args.
pub(crate) struct NewItem<'a> {
  pub kind: &'a str,
  pub title: &'a str,
  pub body: &'a str,
  pub url: &'a str,
  pub tags: Vec<String>,
  pub collection: &'a str,
  pub due_at: Option<String>,
  pub cadence: &'a str,
  pub project_id: &'a str,
  pub source: &'a str,
}

pub(crate) fn add(conn: &Connection, new: NewItem) -> Result<Item, String> {
  let kind = if KINDS.contains(&new.kind) { new.kind } else { "note" };
  let item = Item {
    id: format!("item-{}-{}", chrono::Utc::now().timestamp_millis(), rand_suffix()),
    kind: kind.to_string(), title: new.title.to_string(), body: new.body.to_string(), url: new.url.to_string(),
    tags: new.tags, collection: new.collection.to_string(), status: "active".into(),
    due_at: new.due_at, cadence: norm_cadence(new.cadence), streak: 0, last_done_at: None,
    project_id: new.project_id.to_string(), fired_at: None, source: new.source.to_string(),
    created_at: now(), updated_at: now(),
  };
  upsert(conn, &item)?;
  Ok(item)
}

fn rand_suffix() -> String {
  // Cheap uniqueness so two items filed in the same millisecond don't collide.
  use std::sync::atomic::{AtomicU32, Ordering};
  static N: AtomicU32 = AtomicU32::new(0);
  N.fetch_add(1, Ordering::Relaxed).to_string()
}

fn norm_cadence(c: &str) -> String {
  let c = c.trim().to_ascii_lowercase();
  if CADENCES.contains(&c.as_str()) { c } else { String::new() }
}

fn title_from(text: &str) -> String {
  let words: Vec<&str> = text.split_whitespace().collect();
  if words.len() > 7 { format!("{}…", words[..7].join(" ")) } else { text.trim().to_string() }
}

fn url_in(text: &str) -> String {
  text.split_whitespace()
    .find(|w| w.starts_with("http://") || w.starts_with("https://"))
    .map(|w| w.trim_end_matches(['.', ',', ')']).to_string())
    .unwrap_or_default()
}

fn tags_in(text: &str) -> (Vec<String>, String) {
  let tags: Vec<String> = text.split_whitespace()
    .filter(|w| w.starts_with('#') && w.len() > 1)
    .map(|w| w.trim_start_matches('#').to_string())
    .collect();
  let collection = tags.first().cloned().unwrap_or_default();
  (tags, collection)
}

// A leading `HH:MM` (or `YYYY-MM-DD HH:MM`) becomes an absolute local RFC3339 time;
// if today's slot has passed it rolls to tomorrow. Returns (due_at, remaining text).
fn parse_time_prefix(rest: &str) -> (Option<String>, String) {
  let tokens: Vec<&str> = rest.split_whitespace().collect();
  if tokens.is_empty() { return (None, String::new()); }
  if let Some(dt) = hhmm_today(tokens[0]) {
    return (Some(dt), tokens[1..].join(" "));
  }
  if tokens.len() >= 2 {
    if let Some(dt) = date_time(tokens[0], tokens[1]) {
      return (Some(dt), tokens[2..].join(" "));
    }
  }
  (None, rest.trim().to_string())
}

fn hhmm_today(token: &str) -> Option<String> {
  let (h, m) = token.split_once(':')?;
  let h: u32 = h.trim().parse().ok()?;
  let m: u32 = m.trim().parse().ok()?;
  if h > 23 || m > 59 { return None; }
  let now = chrono::Local::now();
  let mut dt = now.date_naive().and_hms_opt(h, m, 0)?;
  if dt <= now.naive_local() { dt += chrono::Duration::days(1); }
  dt.and_local_timezone(chrono::Local).single().map(|d| d.to_rfc3339())
}

fn date_time(date: &str, time: &str) -> Option<String> {
  let d = chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()?;
  let (h, m) = time.split_once(':')?;
  let dt = d.and_hms_opt(h.trim().parse().ok()?, m.trim().parse().ok()?, 0)?;
  dt.and_local_timezone(chrono::Local).single().map(|x| x.to_rfc3339())
}

fn kind_for_command(cmd: &str) -> Option<&'static str> {
  match cmd.to_ascii_lowercase().as_str() {
    "/n" | "/note" => Some("note"),
    "/tdl" | "/todo" => Some("todo"),
    "/r" | "/remind" | "/reminder" => Some("reminder"),
    "/p" | "/promise" => Some("promise"),
    "/lib" | "/link" => Some("link"),
    "/habit" => Some("habit"),
    _ => None,
  }
}

/// Maps a Lumin item kind to the brain map's content type.
fn content_type_for(kind: &str) -> &'static str {
  match kind {
    "todo" | "reminder" => "task",
    "promise" => "goal",
    "habit" => "habit",
    _ => "note",
  }
}

// `/items [kind]` numbers a kind-filtered, newest-first, 20-row view. `/done`,
// `/forget` and Telegram's `/send` must index exactly that list, so each source's
// last listed kind is remembered here. Without it they numbered a different list
// (unfiltered, 500 rows) and `n` pointed at an item the user was never shown.
static LAST_ITEMS_KIND: std::sync::OnceLock<std::sync::Mutex<std::collections::HashMap<String, String>>> = std::sync::OnceLock::new();

fn items_kinds() -> &'static std::sync::Mutex<std::collections::HashMap<String, String>> {
  LAST_ITEMS_KIND.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}

fn remember_items_kind(source: &str, kind: Option<&str>) {
  if let Ok(mut m) = items_kinds().lock() {
    match kind {
      Some(k) => { m.insert(source.to_string(), k.to_string()); }
      None => { m.remove(source); }
    }
  }
}

/// The kind `/items` was last asked for by this source, if any.
fn remembered_items_kind(source: &str) -> Option<String> {
  items_kinds().lock().ok().and_then(|m| m.get(source).cloned())
}

/// The items exactly as `/items [kind]` last listed them for this source.
fn listed_items(conn: &Connection, source: &str) -> Vec<Item> {
  let kind = remembered_items_kind(source);
  list(conn, kind.as_deref()).into_iter().take(20).collect()
}

/// The id at 1-based position `n` of the list `/items` displayed for this source.
/// Shared by `/done`, `/forget` and Telegram's `/send`, so one number means one
/// item everywhere.
pub(crate) fn listed_item_id(conn: &Connection, source: &str, n: usize) -> Option<String> {
  let items = listed_items(conn, source);
  n.checked_sub(1).and_then(|i| items.get(i)).map(|it| it.id.clone())
}

fn summary(conn: &Connection, kind: Option<&str>) -> String {
  let items = list(conn, kind);
  if items.is_empty() { return "Nothing saved yet.".into(); }
  items.iter().take(20).enumerate().map(|(i, it)| {
    let mark = if it.status == "done" { "☑" } else { "☐" };
    let title = if it.title.trim().is_empty() { &it.body } else { &it.title };
    let when = it.due_at.as_deref().map(short_time).map(|t| format!(" · {t}")).unwrap_or_default();
    let streak = if it.kind == "habit" && it.streak > 0 { format!(" · {}🔥", it.streak) } else { String::new() };
    format!("{}{}. [{}] {}{when}{streak}", mark, i + 1, it.kind, title)
  }).collect::<Vec<_>>().join("\n")
}

fn short_time(iso: &str) -> String {
  chrono::DateTime::parse_from_rfc3339(iso)
    .map(|d| d.with_timezone(&chrono::Local).format("%b %d %H:%M").to_string())
    .unwrap_or_else(|_| iso.to_string())
}

pub(crate) fn help() -> String {
  "lumin: /n note · /tdl todo · /r [HH:MM] reminder · /p promise · /lib url · /habit text [daily|weekly|monthly] · /items [kind] · /done n · /forget n".into()
}

/// Marks an item done (toggling it back if it was already done). Completing a
/// habit bumps its streak; completing a recurring item schedules the next due.
pub(crate) fn complete(conn: &Connection, id: &str) -> Result<String, String> {
  let items = list(conn, None);
  let Some(item) = items.into_iter().find(|i| i.id == id) else { return Err("Not found.".into()) };
  if item.status == "done" {
    conn.execute("UPDATE items SET status='active', updated_at=?2 WHERE id=?1", params![id, now()]).map_err(|e| e.to_string())?;
    return Ok("Reopened.".into());
  }
  let streak = if item.kind == "habit" { item.streak + 1 } else { item.streak };
  let next = crate::reminders::next_due(&item.due_at, &item.cadence);
  conn.execute(
    "UPDATE items SET status='done', streak=?2, last_done_at=?3, due_at=?4, fired_at=NULL, updated_at=?3 WHERE id=?1",
    params![id, streak, now(), next],
  ).map_err(|e| e.to_string())?;
  Ok(if item.kind == "habit" { format!("Done — {} day streak.", streak) } else { "Done.".into() })
}

/// Handles an explicit capture command. Returns None when `text` isn't one, so
/// the caller can fall through (free text is extracted separately).
pub(crate) fn command(app: &AppHandle, text: &str, source: &str) -> Option<Result<String, String>> {
  let trimmed = text.trim();
  let (head, rest) = match trimmed.split_once(char::is_whitespace) { Some((a, b)) => (a, b.trim()), None => (trimmed, "") };
  let conn = match db(app) { Ok(c) => c, Err(e) => return Some(Err(e)) };

  if let Some(kind) = kind_for_command(head) {
    if rest.is_empty() { return Some(Err(format!("Use {head} <text>."))); }
    let (due_at, body) = if kind == "reminder" { parse_time_prefix(rest) } else { (None, rest.to_string()) };
    let (tags, collection) = tags_in(&body);
    let cadence = if kind == "habit" {
      CADENCES.iter().find(|c| body.to_ascii_lowercase().contains(&format!(" {c}")) || body.to_ascii_lowercase().ends_with(**c)).copied().unwrap_or("daily")
    } else { "" };
    let title = title_from(&body);
    let url = url_in(&body);
    // The fast path never calls a model, so the reading is synthesized, not inferred.
    let class = crate::classify::Classification {
      content_type: content_type_for(kind).into(),
      intent: "create".into(),
      feature: kind.into(),
      action: "create".into(),
      confidence: 0.9,
      ..Default::default()
    };
    return Some(match add(&conn, NewItem {
      kind, title: &title, body: &body, url: &url, tags, collection: &collection,
      due_at, cadence, project_id: "", source,
    }) {
      Ok(item) => {
        // Mirror it into the brain map (index node + entities + event).
        crate::brain::ingest_item(app, &conn, &item, &class, "life");
        // A capture bot with a destination sends the idea on (fire-and-forget).
        crate::destinations::auto_push(app, &item, source);
        Ok(confirm(&item))
      }
      Err(e) => Err(e),
    });
  }

  match head.to_ascii_lowercase().as_str() {
    "/items" | "/saved" => {
      let kind = if rest.is_empty() { None } else { Some(rest) };
      // Remember the filter so /done, /forget and /send keep numbering it.
      remember_items_kind(source, kind);
      Some(Ok(summary(&conn, kind)))
    }
    "/due" => Some(Ok(crate::reminders::due_summary(&conn))),
    "/done" => {
      let n: usize = rest.parse().unwrap_or(0);
      match listed_item_id(&conn, source, n) {
        Some(id) => Some(complete(&conn, &id)),
        None => Some(Err("Usage: /done <number> from /items.".into())),
      }
    }
    "/forget" => {
      let n: usize = rest.parse().unwrap_or(0);
      let Some(id) = listed_item_id(&conn, source, n) else {
        return Some(Err("Usage: /forget <number> from /items.".into()));
      };
      Some(conn.execute("DELETE FROM items WHERE id=?1", params![id]).map(|_| "Removed.".to_string()).map_err(|e| e.to_string()))
    }
    "/insights" => Some(Ok(crate::insights::summary(&conn))),
    "/accept" => Some(crate::insights::accept(&conn, rest)),
    _ => None,
  }
}

fn confirm(item: &Item) -> String {
  let title = if item.title.trim().is_empty() { &item.body } else { &item.title };
  let when = item.due_at.as_deref().map(short_time).map(|t| format!(" · {t}")).unwrap_or_default();
  format!("Saved {} — {title}{when}", item.kind)
}

// ---------------------------------------------------------------------------
// The intent (Lumin's view of a classification).
//
// The model that produces it now lives in `classify.rs` (the universal
// classifier); these types are the shape `capture::apply` consumes.
// ---------------------------------------------------------------------------

#[derive(Deserialize, Default)]
pub(crate) struct Intent {
  #[serde(default)] pub r#type: String,
  #[serde(default)] pub feature: String,
  #[serde(default)] pub action: String,
  #[serde(default)] pub params: IntentParams,
  #[serde(default)] pub confidence: f64,
}

#[derive(Deserialize, Default)]
pub(crate) struct IntentParams {
  #[serde(default)] pub title: String,
  #[serde(default)] pub body: String,
  #[serde(default)] pub url: String,
  #[serde(default)] pub tags: Vec<String>,
  #[serde(default)] pub due_at: Option<String>,
  #[serde(default)] pub cadence: String,
}

fn intent_feature_known(f: &str) -> bool { KINDS.contains(&f.trim()) }

/// Files free text: classify it, then apply the intent to the store.
pub(crate) async fn remember(app: &AppHandle, text: &str, source: &str) -> Result<String, String> {
  let trimmed = text.trim();
  if trimmed.is_empty() { return Err(help()); }
  let class = crate::classify::classify(app, trimmed, source).await;
  let intent = crate::classify::to_intent(&class);
  let conn = db(app)?;
  // The classifier's `type` gate: pure conversation isn't filed. Say so rather
  // than silently swallowing it.
  if intent.r#type.trim() == "chat" && intent.action.trim() != "read" {
    return Ok("Noted — nothing to file (use /n to keep it as a note).".into());
  }
  let (reply, created) = apply(&conn, &intent, trimmed, source)?;
  if let Some(item) = &created {
    // Mirror the file into the brain map, then hand it to the bot's destination.
    crate::brain::ingest_item(app, &conn, item, &class, "life");
    crate::destinations::auto_push(app, item, source);
  }
  // A low-confidence read still files (nothing is ever lost) but says so.
  Ok(if intent.confidence > 0.0 && intent.confidence < 0.4 { format!("{reply} · low confidence, check it in Items") } else { reply })
}

fn intent_kind(intent: &Intent) -> &str {
  if intent_feature_known(&intent.feature) { intent.feature.trim() } else { "note" }
}

// Returns (reply, the item just created if this was a create).
fn apply(conn: &Connection, intent: &Intent, raw: &str, source: &str) -> Result<(String, Option<Item>), String> {
  let kind = intent_kind(intent);
  let p = &intent.params;
  let body = if p.body.trim().is_empty() { raw.to_string() } else { p.body.clone() };
  let title = if p.title.trim().is_empty() { title_from(&body) } else { p.title.clone() };
  let (auto_tags, auto_collection) = tags_in(raw);
  let tags = if p.tags.is_empty() { auto_tags } else { p.tags.clone() };
  let collection = if auto_collection.is_empty() { tags.first().cloned().unwrap_or_default() } else { auto_collection };
  let url = if p.url.trim().is_empty() { url_in(raw) } else { p.url.clone() };

  match intent.action.trim() {
    // "I did X" / "done with X" — complete the closest match.
    "complete" => {
      if let Some(item) = find(conn, &title) { complete(conn, &item.id).map(|r| (r, None)) } else { Err(format!("Couldn't find anything matching “{title}”.")) }
    }
    "delete" => {
      if let Some(item) = find(conn, &title) {
        conn.execute("DELETE FROM items WHERE id=?1", params![item.id]).map_err(|e| e.to_string())?;
        Ok((format!("Removed — {}", item.title), None))
      } else { Err(format!("Couldn't find anything matching “{title}”.")) }
    }
    "read" => Ok((summary(conn, if kind == "note" { None } else { Some(kind) }), None)),
    "update" => {
      if let Some(item) = find(conn, &title) {
        conn.execute("UPDATE items SET body=?2, title=?3, due_at=COALESCE(?4, due_at), updated_at=?5 WHERE id=?1",
          params![item.id, body, title, p.due_at, now()]).map_err(|e| e.to_string())?;
        Ok((format!("Updated — {title}"), None))
      } else { Err(format!("Couldn't find anything matching “{title}”.")) }
    }
    _ => {
      // Idempotent ingestion: the same thought filed twice (a retried bot message,
      // a repeated note) updates the open item instead of making a duplicate.
      if let Some(existing) = find_exact_active(conn, &title) {
        conn.execute(
          "UPDATE items SET body=?2, kind=?3, url=?4, tags=?5, due_at=COALESCE(?6, due_at), cadence=?7, updated_at=?8 WHERE id=?1",
          params![existing.id, body, kind, url, serde_json::to_string(&tags).unwrap_or_else(|_| "[]".into()), p.due_at.clone(), norm_cadence(&p.cadence), now()],
        ).map_err(|e| e.to_string())?;
        return Ok((format!("Already saved — {title}"), None));
      }
      let item = add(conn, NewItem {
        kind, title: &title, body: &body, url: &url, tags, collection: &collection,
        due_at: p.due_at.clone(), cadence: &p.cadence, project_id: "", source,
      })?;
      Ok((confirm(&item), Some(item)))
    }
  }
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn item_list(app: AppHandle, kind: Option<String>) -> Result<Vec<Item>, String> {
  let conn = db(&app)?;
  Ok(list(&conn, kind.as_deref()))
}

#[tauri::command]
pub fn item_save(app: AppHandle, item: Item) -> Result<Item, String> {
  let conn = db(&app)?;
  let mut it = item;
  if it.id.trim().is_empty() { it.id = format!("item-{}-{}", chrono::Utc::now().timestamp_millis(), rand_suffix()); }
  if it.created_at.trim().is_empty() { it.created_at = now(); }
  if !KINDS.contains(&it.kind.as_str()) { it.kind = "note".into(); }
  it.cadence = norm_cadence(&it.cadence);
  it.updated_at = now();
  upsert(&conn, &it)?;
  Ok(it)
}

#[tauri::command]
pub fn item_delete(app: AppHandle, id: String) -> Result<(), String> {
  let conn = db(&app)?;
  conn.execute("DELETE FROM items WHERE id=?1", params![id]).map_err(|e| e.to_string())?;
  Ok(())
}

#[tauri::command]
pub fn item_toggle(app: AppHandle, id: String) -> Result<String, String> {
  let conn = db(&app)?;
  complete(&conn, &id)
}

#[cfg(test)]
mod tests {
  use super::*;

  /// `/items [kind]` numbers a filtered view; `/done`, `/forget` and Telegram's
  /// `/send` must index that same list. Remembering the filter is what makes the
  /// number mean one item — before, they numbered a different, unfiltered list.
  #[test]
  fn items_kind_is_remembered_per_source() {
    remember_items_kind("test-chat-a", Some("note"));
    assert_eq!(remembered_items_kind("test-chat-a").as_deref(), Some("note"));

    // A plain `/items` clears the filter.
    remember_items_kind("test-chat-a", None);
    assert_eq!(remembered_items_kind("test-chat-a"), None);

    // One source's filter never leaks into another's numbering.
    remember_items_kind("test-chat-b", Some("todo"));
    assert_eq!(remembered_items_kind("test-chat-a"), None);
    assert_eq!(remembered_items_kind("test-chat-b").as_deref(), Some("todo"));
    remember_items_kind("test-chat-b", None);
  }

  fn items_db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
      "CREATE TABLE items (id TEXT PRIMARY KEY, kind TEXT, title TEXT, body TEXT, url TEXT, tags TEXT, \
       collection TEXT, status TEXT, due_at TEXT, cadence TEXT, streak INTEGER, last_done_at TEXT, \
       project_id TEXT, fired_at TEXT, source TEXT, created_at TEXT, updated_at TEXT);",
    ).unwrap();
    conn
  }

  fn add_item(conn: &Connection, id: &str, kind: &str, created_at: &str) {
    conn.execute(
      "INSERT INTO items (id, kind, title, body, url, tags, collection, status, cadence, streak, project_id, source, created_at, updated_at) \
       VALUES (?1,?2,'','','','[]','','active','',0,'','',?3,?3)",
      params![id, kind, created_at],
    ).unwrap();
  }

  /// Filing the same thought twice must keep one item, not two: the brain node is
  /// idempotent by `ref_id`, so without this the item list would grow a duplicate.
  #[test]
  fn refiling_the_same_title_finds_the_open_item() {
    let conn = items_db();
    let a = add(&conn, NewItem {
      kind: "todo", title: "Water the plants", body: "", url: "", tags: vec![],
      collection: "", due_at: None, cadence: "", project_id: "", source: "app",
    }).unwrap();
    let found = find_exact_active(&conn, "  water the PLANTS ").expect("case/space-insensitive exact match");
    assert_eq!(found.id, a.id);
    assert!(find_exact_active(&conn, "water the ferns").is_none(), "a different title is not the same item");
  }

  /// The bug: `/send 1`, `/done 1` and `/forget 1` indexed an unfiltered list
  /// while `/items note` had numbered a filtered one, so `1` meant two different
  /// items. They now share one numbering.
  #[test]
  fn listed_item_id_follows_the_items_filter() {
    let conn = items_db();
    add_item(&conn, "n1", "note", "2026-01-01T00:00:00Z");
    add_item(&conn, "n2", "note", "2026-01-02T00:00:00Z");
    add_item(&conn, "t1", "todo", "2026-01-03T00:00:00Z");

    // Unfiltered, newest first: t1, n2, n1.
    remember_items_kind("test-list-a", None);
    assert_eq!(listed_item_id(&conn, "test-list-a", 1).as_deref(), Some("t1"));
    assert_eq!(listed_item_id(&conn, "test-list-a", 3).as_deref(), Some("n1"));
    assert_eq!(listed_item_id(&conn, "test-list-a", 4), None, "past the end");
    assert_eq!(listed_item_id(&conn, "test-list-a", 0), None, "numbering is 1-based");

    // After `/items note`, 1 is the newest *note*, not the newest item.
    remember_items_kind("test-list-a", Some("note"));
    assert_eq!(listed_item_id(&conn, "test-list-a", 1).as_deref(), Some("n2"));
    assert_eq!(listed_item_id(&conn, "test-list-a", 2).as_deref(), Some("n1"));
    assert_eq!(listed_item_id(&conn, "test-list-a", 3), None);

    remember_items_kind("test-list-a", None);
  }
}
