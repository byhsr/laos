// Destinations: named places an item can be pushed to — an MCP server plus one of
// its tools, filled from the item through a JSON arg template. This is what lets
// several Notion accounts (one MCP server each) receive captured ideas directly.

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::AppHandle;

use crate::capture::Item;
use crate::db::db;

fn yes() -> bool { true }

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Destination {
  #[serde(default)] pub id: String,
  #[serde(default)] pub name: String,
  #[serde(default)] pub server_id: String,
  #[serde(default)] pub tool: String,
  #[serde(default)] pub arg_template: String,
  #[serde(default = "yes")] pub enabled: bool,
  #[serde(default)] pub created_at: String,
  #[serde(default)] pub updated_at: String,
}

const SELECT: &str = "SELECT id, name, server_id, tool, arg_template, enabled, created_at, updated_at FROM destinations";

fn row(r: &rusqlite::Row) -> rusqlite::Result<Destination> {
  Ok(Destination {
    id: r.get(0)?, name: r.get(1)?, server_id: r.get(2)?, tool: r.get(3)?,
    arg_template: r.get(4)?, enabled: r.get::<_, i64>(5)? != 0, created_at: r.get(6)?, updated_at: r.get(7)?,
  })
}

pub(crate) fn list(conn: &Connection) -> Vec<Destination> {
  let mut out = Vec::new();
  if let Ok(mut stmt) = conn.prepare(&format!("{SELECT} ORDER BY name")) {
    if let Ok(rows) = stmt.query_map([], row) {
      for r in rows { if let Ok(d) = r { out.push(d); } }
    }
  }
  out
}

pub(crate) fn get(conn: &Connection, id: &str) -> Option<Destination> {
  conn.query_row(&format!("{SELECT} WHERE id=?1"), params![id], row).ok()
}

pub(crate) fn upsert(conn: &Connection, d: &Destination) -> Result<(), String> {
  let ts = chrono::Utc::now().to_rfc3339();
  let created = if d.created_at.trim().is_empty() { ts.clone() } else { d.created_at.clone() };
  conn.execute(
    "INSERT INTO destinations (id, name, server_id, tool, arg_template, enabled, created_at, updated_at) \
     VALUES (?1,?2,?3,?4,?5,?6,?7,?8) \
     ON CONFLICT(id) DO UPDATE SET name=excluded.name, server_id=excluded.server_id, tool=excluded.tool, \
     arg_template=excluded.arg_template, enabled=excluded.enabled, updated_at=excluded.updated_at",
    params![d.id, d.name, d.server_id, d.tool, d.arg_template, if d.enabled { 1 } else { 0 }, created, ts],
  ).map_err(|e| e.to_string())?;
  Ok(())
}

/// The destination a capture bot pushes to, if it has one.
pub(crate) fn bot_destination(conn: &Connection, bot_name: &str) -> Option<String> {
  conn.query_row("SELECT destination_id FROM telegram_bots WHERE name=?1", params![bot_name], |r| r.get::<_, String>(0))
    .ok().filter(|s| !s.trim().is_empty())
}

fn default_template() -> String {
  "{\n  \"title\": \"{{title}}\",\n  \"content\": \"{{body}}\"\n}".to_string()
}

// Escapes a value for embedding inside the template's JSON string literals.
fn esc(s: &str) -> String {
  let quoted = serde_json::to_string(s).unwrap_or_else(|_| "\"\"".into());
  quoted[1..quoted.len().saturating_sub(1)].to_string()
}

/// Fills the arg template from an item. `{{title}} {{body}} {{url}} {{tags}}
/// {{kind}}` are replaced with JSON-escaped values, then the whole thing must
/// parse as JSON — so a broken template fails loudly instead of sending garbage.
pub(crate) fn render_args(item: &Item, template: &str) -> Result<Value, String> {
  let template = if template.trim().is_empty() { default_template() } else { template.to_string() };
  let title = if item.title.trim().is_empty() { item.body.clone() } else { item.title.clone() };
  let mut out = template;
  for (token, value) in [
    ("{{title}}", title),
    ("{{body}}", item.body.clone()),
    ("{{url}}", item.url.clone()),
    ("{{tags}}", item.tags.join(", ")),
    ("{{kind}}", item.kind.clone()),
  ] {
    out = out.replace(token, &esc(&value));
  }
  serde_json::from_str(&out).map_err(|e| format!("arg template isn't valid JSON once filled: {e}"))
}

/// Pushes one item to one destination: loads the MCP server, renders the args,
/// and calls the tool. Blocking (MCP is stdio), so it runs on a blocking thread.
pub(crate) async fn push(app: AppHandle, item_id: String, destination_id: String) -> Result<String, String> {
  let (server_id, tool, args) = {
    let conn = db(&app)?;
    let dest = get(&conn, &destination_id).ok_or_else(|| "Destination not found.".to_string())?;
    if !dest.enabled { return Err("That destination is turned off.".into()); }
    if dest.server_id.trim().is_empty() || dest.tool.trim().is_empty() {
      return Err("Destination is missing its server or tool.".into());
    }
    let item = crate::capture::list(&conn, None).into_iter().find(|i| i.id == item_id)
      .ok_or_else(|| "Item not found.".to_string())?;
    (dest.server_id, dest.tool, render_args(&item, &dest.arg_template)?)
  };

  let app2 = app.clone();
  tokio::task::spawn_blocking(move || {
    let conn = db(&app2)?;
    let server = crate::mcp::load_server(&conn, &server_id)?;
    crate::mcp::call_tool(&server, &tool, args)
  }).await.map_err(|e| e.to_string())?
}

/// Auto-push for captured items. Fire-and-forget: the Telegram reply never waits
/// on a cold MCP server, and a failure lands in the Telegram log instead.
pub(crate) fn auto_push(app: &AppHandle, item: &Item, source: &str) {
  let Ok(conn) = db(app) else { return };
  let Some(destination_id) = bot_destination(&conn, source) else { return };
  let app2 = app.clone();
  let id = item.id.clone();
  let bot = source.to_string();
  tauri::async_runtime::spawn(async move {
    if let Err(e) = push(app2.clone(), id, destination_id).await {
      crate::telegram::log_destination(&app2, &bot, &e);
    }
  });
}

/// Validates a destination without writing anything to the target account.
fn check(conn: &Connection, d: &Destination) -> Result<String, String> {
  if d.server_id.trim().is_empty() { return Err("Pick a server.".into()); }
  if d.tool.trim().is_empty() { return Err("Pick a tool.".into()); }
  crate::mcp::load_server(conn, &d.server_id).map_err(|e| format!("Server not available: {e}"))?;
  let sample = Item {
    id: "sample".into(), kind: "note".into(), title: "Sample title".into(), body: "Sample body".into(),
    url: "https://example.com".into(), tags: vec!["sample".into()], collection: String::new(),
    status: "active".into(), due_at: None, cadence: String::new(), streak: 0, last_done_at: None,
    project_id: String::new(), fired_at: None, source: "app".into(), created_at: String::new(), updated_at: String::new(),
  };
  render_args(&sample, &d.arg_template)?;
  Ok(format!("ok — {} is ready.", d.tool))
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn list_destinations(app: AppHandle) -> Result<Vec<Destination>, String> {
  let conn = db(&app)?;
  Ok(list(&conn))
}

#[tauri::command]
pub fn save_destination(app: AppHandle, destination: Destination) -> Result<Destination, String> {
  let conn = db(&app)?;
  let mut d = destination;
  if d.id.trim().is_empty() { d.id = format!("dst-{}", chrono::Utc::now().timestamp_millis()); }
  if d.name.trim().is_empty() { d.name = d.tool.clone(); }
  upsert(&conn, &d)?;
  Ok(get(&conn, &d.id).unwrap_or(d))
}

#[tauri::command]
pub fn delete_destination(app: AppHandle, id: String) -> Result<(), String> {
  let conn = db(&app)?;
  conn.execute("DELETE FROM destinations WHERE id=?1", params![id]).map_err(|e| e.to_string())?;
  conn.execute("UPDATE telegram_bots SET destination_id='' WHERE destination_id=?1", params![id]).ok();
  Ok(())
}

/// The tools a server has imported, for the destination's tool picker.
#[tauri::command]
pub fn destination_tools(app: AppHandle, server_id: String) -> Result<Vec<String>, String> {
  let conn = db(&app)?;
  let mut out = Vec::new();
  let mut stmt = conn.prepare("SELECT config_json FROM tools WHERE kind='mcp' AND enabled=1").map_err(|e| e.to_string())?;
  let rows = stmt.query_map([], |r| r.get::<_, String>(0)).map_err(|e| e.to_string())?;
  for cfg in rows.flatten() {
    if let Ok(v) = serde_json::from_str::<Value>(&cfg) {
      if v.get("serverId").and_then(|s| s.as_str()) == Some(server_id.as_str()) {
        if let Some(t) = v.get("toolName").and_then(|s| s.as_str()) { out.push(t.to_string()); }
      }
    }
  }
  out.sort();
  out.dedup();
  Ok(out)
}

#[tauri::command]
pub fn test_destination(app: AppHandle, destination: Destination) -> Result<String, String> {
  let conn = db(&app)?;
  check(&conn, &destination)
}

#[tauri::command]
pub async fn push_item(app: AppHandle, id: String, destination_id: String) -> Result<String, String> {
  push(app, id, destination_id).await
}
