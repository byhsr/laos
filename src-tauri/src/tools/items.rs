// The `items` tool — the todo list, reminders and quick notes every agent holds.
//
// This is the destination the knowledge base was standing in for: before it
// existed an agent asked to "remind me tomorrow" had only knowledge_base or
// remember, so reminders and todos were filed as reference docs.

use async_trait::async_trait;
use rusqlite::Connection;
use tauri::AppHandle;

use super::{str_arg, AgentTool};

pub(crate) struct ItemsTool { pub(crate) app: AppHandle, pub(crate) owner: String }

#[async_trait]
impl AgentTool for ItemsTool {
  fn name(&self) -> String { "items".into() }

  fn description(&self) -> String {
    "The USER's own todo list, reminders and quick notes — anything with a when or a do. Use this (NOT the \
knowledge base) for their notes, todos, reminders, goals, resources and habits; reminders are scheduled \
and pushed to the user on Telegram. This is their store — your own working notes, drafts and things you \
are keeping track of while you work belong in the scratchpad tool, never here. Full CRUD: add, list, update, \
complete, delete. Params: action ('add' | \
'list' | 'update' | 'complete' | 'delete'), kind ('note' | 'todo' | 'reminder' | 'goal' | 'resource' | 'habit', \
default 'note'), title (string), body (string, optional), due (string — a time like 'tomorrow 9am' or an \
ISO-8601 instant; for reminders), cadence ('daily' | 'weekly' | 'monthly'; for habits), status ('active' | \
'done', for 'update'), query (string — which item, for 'update' / 'complete' / 'delete'). Items you file are \
tagged with your agent id, so they are listed apart from the user's own todos.".into()
  }

  fn params_schema(&self) -> serde_json::Value {
    serde_json::json!({ "type": "object", "properties": {
      "action": { "type": "string" },
      "kind": { "type": "string" },
      "title": { "type": "string" },
      "body": { "type": "string" },
      "due": { "type": "string" },
      "cadence": { "type": "string" },
      "status": { "type": "string" },
      "query": { "type": "string" }
    }, "required": ["action"] })
  }

  async fn run(&self, args: &serde_json::Value) -> Result<String, String> {
    let conn = crate::db::db(&self.app)?;
    match str_arg(args, "action").unwrap_or_else(|| "list".into()).trim() {
      "add" => add(&self.app, &conn, args, &self.owner),
      "list" => Ok(list(&conn, str_arg(args, "kind").as_deref())),
      "update" => update(&self.app, &conn, args),
      "complete" => complete_item(&conn, args),
      "delete" => delete(&conn, args),
      other => Err(format!("Unknown items action \"{other}\". Use add | list | update | complete | delete.")),
    }
  }
}

fn add(app: &AppHandle, conn: &Connection, args: &serde_json::Value, owner: &str) -> Result<String, String> {
  let title = str_arg(args, "title").unwrap_or_default();
  let mut body = str_arg(args, "body").unwrap_or_default();
  let seed = if title.trim().is_empty() { body.clone() } else { title.clone() };
  if seed.trim().is_empty() { return Err("items add requires a 'title' (or 'body').".into()); }
  if body.trim().is_empty() { body = seed.clone(); }

  let raw_kind = str_arg(args, "kind").unwrap_or_default();
  let kind = if crate::capture::KINDS.contains(&raw_kind.trim()) { raw_kind.trim() } else { "note" };
  let (tags, collection) = crate::capture::tags_in(&seed);
  let url = crate::capture::url_in(&body);
  let title = if title.trim().is_empty() { crate::capture::title_from(&body) } else { title };
  // The due text is resolved in the user's configured zone (default IST).
  let due = str_arg(args, "due").and_then(|d| crate::routing::resolve_due(conn, &d));
  let cadence = str_arg(args, "cadence").unwrap_or_default();

  let item = crate::capture::add(conn, crate::capture::NewItem {
    kind, title: &title, body: &body, url: &url, tags, collection: &collection,
    due_at: due.clone(), cadence: &cadence, project_id: "", source: "app", owner,
  })?;
  // Mirror it into the brain map, exactly as the capture fast paths do.
  crate::brain::ingest_item(app, conn, &item, &crate::capture::synthesized_class(kind), "life");

  let when = due.as_deref().map(|d| format!(" · due {}", crate::tz::format(conn, d))).unwrap_or_default();
  Ok(format!("Added {kind} \"{title}\"{when} (id: {}).", item.id))
}

fn list(conn: &Connection, kind: Option<&str>) -> String {
  let items = crate::capture::list(conn, kind);
  if items.is_empty() { return "No items yet.".into(); }
  items.iter().take(30).map(|i| {
    let title = if i.title.trim().is_empty() { &i.body } else { &i.title };
    let when = i.due_at.as_deref().map(|d| format!(" · due {}", crate::tz::format(conn, d))).unwrap_or_default();
    format!("[{}] {title} ({}){when} · id {}", i.kind, i.status, i.id)
  }).collect::<Vec<_>>().join("\n")
}

/// The item named by `query` / `title` — any text its title or body contains.
fn locate(conn: &Connection, args: &serde_json::Value) -> Result<crate::capture::Item, String> {
  let query = str_arg(args, "query").or_else(|| str_arg(args, "title")).unwrap_or_default();
  let q = query.trim();
  if q.is_empty() { return Err("This action requires a 'query' naming the item.".into()); }
  crate::capture::find(conn, q).ok_or_else(|| format!("No item matching \"{q}\"."))
}

fn complete_item(conn: &Connection, args: &serde_json::Value) -> Result<String, String> {
  let item = locate(conn, args)?;
  let reply = crate::capture::complete(conn, &item.id)?;
  Ok(format!("{reply} ({})", item.title))
}

/// Delete one item. Goes through `capture::forget`, so the brain index node goes
/// with it rather than being left orphaned.
fn delete(conn: &Connection, args: &serde_json::Value) -> Result<String, String> {
  let item = locate(conn, args)?;
  crate::capture::forget(conn, &item.id)?;
  Ok(format!("Removed \"{}\".", item.title))
}

/// Update one item in place. `query` names it; any of title / body / kind / due /
/// cadence / status may be changed. The brain node is refreshed to match.
fn update(app: &AppHandle, conn: &Connection, args: &serde_json::Value) -> Result<String, String> {
  let query = str_arg(args, "query").unwrap_or_default();
  let q = query.trim();
  if q.is_empty() { return Err("items update requires a 'query' naming the item.".into()); }
  let Some(mut item) = crate::capture::find(conn, q) else { return Err(format!("No item matching \"{q}\".")); };

  if let Some(t) = str_arg(args, "title").filter(|t| !t.trim().is_empty()) { item.title = t; }
  if let Some(b) = str_arg(args, "body").filter(|b| !b.trim().is_empty()) { item.body = b; }
  if let Some(k) = str_arg(args, "kind") {
    if crate::capture::KINDS.contains(&k.trim()) { item.kind = k.trim().to_string(); }
  }
  if let Some(d) = str_arg(args, "due").and_then(|d| crate::routing::resolve_due(conn, &d)) { item.due_at = Some(d); }
  if let Some(c) = str_arg(args, "cadence").filter(|c| !c.trim().is_empty()) { item.cadence = c.trim().to_string(); }
  if let Some(s) = str_arg(args, "status").filter(|s| !s.trim().is_empty()) { item.status = s.trim().to_string(); }

  crate::capture::upsert(conn, &item)?;
  crate::brain::ingest_item(app, conn, &item, &crate::capture::synthesized_class(&item.kind), "life");
  Ok(format!("Updated \"{}\" (id: {}).", item.title, item.id))
}

/// The items tool held by every agent and the Manager. `owner` is the filing
/// agent's id, so its items are tagged and the user's own list stays separate.
pub(crate) fn items_tool(app: &AppHandle, agent_id: &str) -> Box<dyn AgentTool> {
  Box::new(ItemsTool { app: app.clone(), owner: agent_id.to_string() })
}
