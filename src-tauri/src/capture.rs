// Capture — a Telegram "second brain" (ported from fren). Short commands
// (/n, /tdl, /r, /p, /lib) file text exactly; free text on a capture bot is
// classified into one of those kinds by a model. Everything lands in `items`.

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tauri::AppHandle;

use crate::db::db;

pub(crate) const KINDS: [&str; 5] = ["note", "todo", "reminder", "promise", "link"];

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
  #[serde(default)] pub source: String,
  #[serde(default)] pub created_at: String,
  #[serde(default)] pub updated_at: String,
}

fn default_kind() -> String { "note".into() }
fn default_status() -> String { "active".into() }
fn now() -> String { chrono::Utc::now().to_rfc3339() }

const SELECT: &str = "SELECT id, kind, title, body, url, tags, collection, status, due_at, source, created_at, updated_at FROM items";

fn row_to_item(row: &rusqlite::Row) -> rusqlite::Result<Item> {
  let tags: String = row.get(5)?;
  Ok(Item {
    id: row.get(0)?, kind: row.get(1)?, title: row.get(2)?, body: row.get(3)?, url: row.get(4)?,
    tags: serde_json::from_str(&tags).unwrap_or_default(),
    collection: row.get(6)?, status: row.get(7)?, due_at: row.get(8)?, source: row.get(9)?,
    created_at: row.get(10)?, updated_at: row.get(11)?,
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

pub(crate) fn upsert(conn: &Connection, item: &Item) -> Result<(), String> {
  let tags = serde_json::to_string(&item.tags).unwrap_or_else(|_| "[]".into());
  let created = if item.created_at.trim().is_empty() { now() } else { item.created_at.clone() };
  conn.execute(
    "INSERT INTO items (id, kind, title, body, url, tags, collection, status, due_at, source, created_at, updated_at) \
     VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12) \
     ON CONFLICT(id) DO UPDATE SET kind=excluded.kind, title=excluded.title, body=excluded.body, url=excluded.url, \
     tags=excluded.tags, collection=excluded.collection, status=excluded.status, due_at=excluded.due_at, updated_at=excluded.updated_at",
    params![item.id, item.kind, item.title, item.body, item.url, tags, item.collection, item.status, item.due_at, item.source, created, now()],
  ).map_err(|e| e.to_string())?;
  Ok(())
}

pub(crate) fn add(conn: &Connection, kind: &str, title: &str, body: &str, url: &str, source: &str) -> Result<Item, String> {
  let item = Item {
    id: format!("item-{}", chrono::Utc::now().timestamp_millis()),
    kind: kind.to_string(), title: title.to_string(), body: body.to_string(), url: url.to_string(),
    tags: Vec::new(), collection: String::new(), status: "active".into(), due_at: None,
    source: source.to_string(), created_at: now(), updated_at: now(),
  };
  upsert(conn, &item)?;
  Ok(item)
}

fn confirm(item: &Item) -> String {
  let title = if item.title.trim().is_empty() { &item.body } else { &item.title };
  format!("Saved {} — {title}", item.kind)
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

fn kind_for_command(cmd: &str) -> Option<&'static str> {
  match cmd.to_ascii_lowercase().as_str() {
    "/n" | "/note" => Some("note"),
    "/tdl" | "/todo" => Some("todo"),
    "/r" | "/remind" | "/reminder" => Some("reminder"),
    "/p" | "/promise" => Some("promise"),
    "/lib" | "/link" => Some("link"),
    _ => None,
  }
}

fn summary(conn: &Connection, kind: Option<&str>) -> String {
  let items = list(conn, kind);
  if items.is_empty() { return "Nothing saved yet.".into(); }
  items.iter().take(20).enumerate().map(|(i, it)| {
    let mark = if it.status == "done" { "☑" } else { "☐" };
    let title = if it.title.trim().is_empty() { &it.body } else { &it.title };
    format!("{}{}. [{}] {}", mark, i + 1, it.kind, title)
  }).collect::<Vec<_>>().join("\n")
}

pub(crate) fn help() -> String {
  "capture: /n <note> · /tdl <todo> · /r <reminder> · /p <promise> · /lib <url> · /items [kind] · /done <n>".into()
}

/// Handles an explicit capture command. Returns None when `text` isn't one, so
/// the caller can fall through (free text is classified separately).
pub(crate) fn command(app: &AppHandle, text: &str, source: &str) -> Option<Result<String, String>> {
  let trimmed = text.trim();
  let (head, rest) = match trimmed.split_once(char::is_whitespace) { Some((a, b)) => (a, b.trim()), None => (trimmed, "") };
  let conn = match db(app) { Ok(c) => c, Err(e) => return Some(Err(e)) };

  if let Some(kind) = kind_for_command(head) {
    if rest.is_empty() { return Some(Err(format!("Use {head} <text>."))); }
    return Some(match add(&conn, kind, &title_from(rest), rest, &url_in(rest), source) {
      Ok(item) => Ok(confirm(&item)),
      Err(e) => Err(e),
    });
  }

  match head.to_ascii_lowercase().as_str() {
    "/items" | "/saved" => {
      let kind = if rest.is_empty() { None } else { Some(rest) };
      Some(Ok(summary(&conn, kind)))
    }
    "/done" => {
      let n: usize = rest.parse().unwrap_or(0);
      let items = list(&conn, None);
      let Some(item) = n.checked_sub(1).and_then(|i| items.get(i)) else {
        return Some(Err("Usage: /done <number> from /items.".into()));
      };
      let id = item.id.clone();
      Some(conn.execute(
        "UPDATE items SET status = CASE WHEN status='done' THEN 'active' ELSE 'done' END, updated_at=?2 WHERE id=?1",
        params![id, now()],
      ).map(|_| "Toggled.".to_string()).map_err(|e| e.to_string()))
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
    _ => None,
  }
}

fn pick_model(conn: &Connection) -> Option<String> {
  conn.query_row("SELECT id FROM model_configs WHERE enabled=1 ORDER BY id LIMIT 1", [], |r| r.get(0)).ok()
}

// The extractor prompt (fren's LuminCore, trimmed to the classification we use).
const EXTRACTOR: &str = "You are a JSON extraction engine. Output strict JSON only, no prose, no code fences.\n\
\n\
SCHEMA: {\"type\":\"chat|feature\",\"feature\":\"note|todo|reminder|promise|collection|null\",\"reasoning\":\"one line\"}\n\
\n\
Rules:\n\
- reminder: a task with a specific time (\"remind me to ... at 6pm\").\n\
- todo: something the user intends to do, no time (\"I need to call mom\").\n\
- promise: an explicit commitment to future behaviour (\"I will ...\", \"I'll start ...\").\n\
- collection: the message contains a URL / link to save.\n\
- note: an idea, thought, fact or snippet worth keeping.\n\
- chat: anything that is just conversation, not worth filing.\n\
Choose exactly one feature. Unknown -> note.";

/// Classifies free text into a kind with the first enabled model, falling back to
/// "note" when there's no model or the reply can't be parsed.
pub(crate) async fn classify(app: &AppHandle, text: &str) -> String {
  match classify_inner(app, text).await {
    Ok(kind) => kind,
    Err(_) => "note".to_string(),
  }
}

async fn classify_inner(app: &AppHandle, text: &str) -> Result<String, String> {
  let conn = db(app)?;
  let model_id = pick_model(&conn).ok_or("No model configured.")?;
  let resolved = crate::provider::resolve(&conn, &model_id, None)?;

  let mut body = json!({
    "model": resolved.model,
    "messages": [
      { "role": "system", "content": EXTRACTOR },
      { "role": "user", "content": text }
    ],
    "temperature": 0.2,
    "stream": false
  });
  crate::http::apply_openai_defaults(&mut body, resolved.kind == crate::provider::Kind::OpenRouter, 200);
  crate::provider::apply_reasoning(&mut body, &resolved, 200);
  if resolved.kind == crate::provider::Kind::Anthropic { body = crate::anthropic::to_wire(&body); }

  let client = crate::http::client();
  let resp = crate::http::send_model_request(&client, &resolved, &resolved.chat_url(), &body, crate::http::MODEL_ATTEMPTS).await?;
  let mut reply: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
  if resolved.kind == crate::provider::Kind::Anthropic { reply = crate::anthropic::normalize_response(&reply); }
  let content = reply["choices"][0]["message"]["content"].as_str().unwrap_or("");
  Ok(parse_kind(content))
}

fn parse_kind(raw: &str) -> String {
  let cleaned = raw.replace("```json", "").replace("```", "");
  let value: serde_json::Value = serde_json::from_str(cleaned.trim()).unwrap_or(serde_json::Value::Null);
  match value["feature"].as_str().unwrap_or("").to_ascii_lowercase().as_str() {
    "todo" => "todo",
    "reminder" => "reminder",
    "promise" => "promise",
    "collection" | "link" => "link",
    _ => "note",
  }
  .to_string()
}

/// Files free text as an item, classifying its kind first.
pub(crate) async fn remember(app: &AppHandle, text: &str, source: &str) -> Result<Item, String> {
  let trimmed = text.trim();
  if trimmed.is_empty() { return Err(help()); }
  let kind = classify(app, trimmed).await;
  let conn = db(app)?;
  add(&conn, &kind, &title_from(trimmed), trimmed, &url_in(trimmed), source)
}

#[tauri::command]
pub fn item_list(app: AppHandle, kind: Option<String>) -> Result<Vec<Item>, String> {
  let conn = db(&app)?;
  Ok(list(&conn, kind.as_deref()))
}

#[tauri::command]
pub fn item_save(app: AppHandle, item: Item) -> Result<Item, String> {
  let conn = db(&app)?;
  let mut it = item;
  if it.id.trim().is_empty() { it.id = format!("item-{}", chrono::Utc::now().timestamp_millis()); }
  if it.created_at.trim().is_empty() { it.created_at = now(); }
  if !KINDS.contains(&it.kind.as_str()) { it.kind = "note".into(); }
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
pub fn item_toggle(app: AppHandle, id: String) -> Result<(), String> {
  let conn = db(&app)?;
  conn.execute(
    "UPDATE items SET status = CASE WHEN status='done' THEN 'active' ELSE 'done' END, updated_at=?2 WHERE id=?1",
    params![id, now()],
  ).map_err(|e| e.to_string())?;
  Ok(())
}
