// Capture — the Lumin core. Every message is an anchor: short commands file text
// exactly, free text goes through an extractor that turns messy language into a
// structured intent (feature + action + params, with times resolved to an
// absolute instant), and the intent is applied to the `items` store.

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::json;
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
    return Some(match add(&conn, NewItem {
      kind, title: &title, body: &body, url: &url, tags, collection: &collection,
      due_at, cadence, project_id: "", source,
    }) {
      Ok(item) => {
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
      Some(Ok(summary(&conn, kind)))
    }
    "/due" => Some(Ok(crate::reminders::due_summary(&conn))),
    "/done" => {
      let items = list(&conn, None);
      let n: usize = rest.parse().unwrap_or(0);
      match n.checked_sub(1).and_then(|i| items.get(i)) {
        Some(item) => { let id = item.id.clone(); Some(complete(&conn, &id)) }
        None => Some(Err("Usage: /done <number> from /items.".into())),
      }
    }
    "/forget" => {
      let n: usize = rest.parse().unwrap_or(0);
      let items = list(&conn, None);
      let Some(item) = n.checked_sub(1).and_then(|i| items.get(i)) else {
        return Some(Err("Usage: /forget <number> from /items.".into()));
      };
      let id = item.id.clone();
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
// The extractor: messy text -> structured intent.
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

const EXTRACTOR: &str = "You are the extractor — the anchor every message passes through. Turn messy, \
low-effort language into one structured intent. Output strict JSON only: no prose, no code fences.\n\
\n\
SCHEMA:\n\
{\n\
  \"type\": \"chat|feature\",\n\
  \"feature\": \"note|todo|reminder|promise|link|habit|null\",\n\
  \"action\": \"create|read|update|delete|complete|null\",\n\
  \"params\": {\n\
    \"title\": \"short title, 3-7 words\",\n\
    \"body\": \"the full text as the user said it\",\n\
    \"url\": \"the link, or empty\",\n\
    \"tags\": [\"#tags without #\"],\n\
    \"due_at\": \"absolute local ISO-8601 timestamp, or null\",\n\
    \"cadence\": \"daily|weekly|monthly for habits, else empty\"\n\
  },\n\
  \"mood\": \"neutral|positive|negative\",\n\
  \"intent_shape\": \"clear|vague|mixed\",\n\
  \"needs_guardrail\": false,\n\
  \"confidence\": 0.0\n\
}\n\
\n\
RULES\n\
- feature: reminder = has a time (\"call mom at 6pm\", \"tonight\"); todo = intends to do it, no time; \
promise = an explicit commitment (\"I will\", \"I'll\"); habit = something repeated (\"every day\", \
\"every week\", \"daily\"); link = the message shares a URL; note = an idea/fact/thought worth keeping; \
chat = just conversation, nothing to file (then feature and action are null).\n\
- action: create for new things; complete for \"I did X\" / \"done with X\"; read for \"show me / what \
are my X\"; update for \"change X to Y\"; delete for \"remove X\".\n\
- Resolve EVERY relative time to an absolute local ISO-8601 timestamp in params.due_at. Never leave \
\"tonight\" or \"6pm\" as text.\n\
- Always fill every field; unknown -> null or empty string.";

fn extractor_system() -> String {
  let now = chrono::Local::now();
  format!("{EXTRACTOR}\n\nRight now it is {} (local time, UTC offset {}). Resolve times against this.",
    now.format("%Y-%m-%d %H:%M %A"), now.offset())
}

/// Runs the extractor over free text. Never errors: an unparsable reply becomes a
/// plain `note`, so nothing the user sends is ever lost.
pub(crate) async fn extract(app: &AppHandle, text: &str) -> Intent {
  let mut intent = extract_inner(app, text).await.unwrap_or_default();
  // A failed call (no model, network error) comes back empty — treat it as a note.
  if intent.r#type.trim().is_empty() { intent.r#type = "feature".into(); }
  if !intent_feature_known(&intent.feature) { intent.feature = "note".into(); }
  if intent.action.trim().is_empty() || intent.action == "null" { intent.action = "create".into(); }
  intent
}

fn intent_feature_known(f: &str) -> bool { KINDS.contains(&f.trim()) }

async fn extract_inner(app: &AppHandle, text: &str) -> Result<Intent, String> {
  let (model_id, reply) = {
    let conn = db(app)?;
    let model_id = pick_model(&conn).ok_or("No model configured.")?;
    let resolved = crate::provider::resolve(&conn, &model_id, None)?;

    let mut body = json!({
      "model": resolved.model,
      "messages": [
        { "role": "system", "content": extractor_system() },
        { "role": "user", "content": text }
      ],
      "temperature": 0.1,
      "stream": false
    });
    crate::http::apply_openai_defaults(&mut body, resolved.kind == crate::provider::Kind::OpenRouter, 400);
    crate::provider::apply_reasoning(&mut body, &resolved, 400);
    if resolved.kind == crate::provider::Kind::Anthropic { body = crate::anthropic::to_wire(&body); }

    let client = crate::http::client();
    let resp = crate::http::send_model_request(&client, &resolved, &resolved.chat_url(), &body, crate::http::MODEL_ATTEMPTS).await?;
    let mut reply: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    if resolved.kind == crate::provider::Kind::Anthropic { reply = crate::anthropic::normalize_response(&reply); }
    (model_id, reply["choices"][0]["message"]["content"].as_str().unwrap_or("").to_string())
  };
  let _ = model_id;
  Ok(parse_intent(&reply))
}

fn parse_intent(raw: &str) -> Intent {
  let cleaned = raw.replace("```json", "").replace("```", "");
  let value: serde_json::Value = serde_json::from_str(cleaned.trim()).unwrap_or(serde_json::Value::Null);
  let mut intent: Intent = serde_json::from_value(value.clone()).unwrap_or_default();
  // Models sometimes answer with a bare feature string ("todo").
  if intent.feature.is_empty() {
    if let Some(s) = value.as_str() { intent.feature = s.trim().to_ascii_lowercase(); }
  }
  if intent.action.trim().is_empty() || intent.action == "null" { intent.action = "create".into(); }
  if intent.params.cadence.trim().is_empty() {
    intent.params.cadence = norm_cadence(&intent.params.cadence);
  }
  intent
}

fn pick_model(conn: &Connection) -> Option<String> {
  conn.query_row("SELECT id FROM model_configs WHERE enabled=1 ORDER BY id LIMIT 1", [], |r| r.get(0)).ok()
}

/// Files free text: extract the intent, then apply it to the store.
pub(crate) async fn remember(app: &AppHandle, text: &str, source: &str) -> Result<String, String> {
  let trimmed = text.trim();
  if trimmed.is_empty() { return Err(help()); }
  let intent = extract(app, trimmed).await;
  let conn = db(app)?;
  // The extractor's `type` gate: pure conversation isn't filed. Say so rather
  // than silently swallowing it.
  if intent.r#type.trim() == "chat" && intent.action.trim() != "read" {
    return Ok("Noted — nothing to file (use /n to keep it as a note).".into());
  }
  let (reply, created) = apply(&conn, &intent, trimmed, source)?;
  // A capture bot with a destination sends the new item on (fire-and-forget).
  if let Some(item) = &created { crate::destinations::auto_push(app, item, source); }
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
