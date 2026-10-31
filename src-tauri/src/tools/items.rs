// The `items` tool — the todo list, reminders and quick notes every agent holds.
//
// This is the destination the knowledge base was standing in for: before it
// existed an agent asked to "remind me tomorrow" had only knowledge_base or
// remember, so reminders and todos were filed as reference docs.

use async_trait::async_trait;
use rusqlite::Connection;
use tauri::AppHandle;

use super::{str_arg, AgentTool};

pub(crate) struct ItemsTool { pub(crate) app: AppHandle }

#[async_trait]
impl AgentTool for ItemsTool {
  fn name(&self) -> String { "items".into() }

  fn description(&self) -> String {
    "The user's todo list, reminders and quick notes — anything with a when or a do. Use this (NOT the \
knowledge base) for todos, reminders, promises, links, habits and throwaway notes; reminders are scheduled \
and pushed to the user on Telegram. Params: action ('add' | 'list' | 'complete' | 'delete'), kind ('note' | \
'todo' | 'reminder' | 'promise' | 'link' | 'habit', default 'note'), title (string), body (string, optional), \
due (string — a time like 'tomorrow 9am' or an ISO-8601 instant; for reminders), cadence ('daily' | 'weekly' | \
'monthly'; for habits), query (string — which item, for 'complete' / 'delete').".into()
  }

  fn params_schema(&self) -> serde_json::Value {
    serde_json::json!({ "type": "object", "properties": {
      "action": { "type": "string" },
      "kind": { "type": "string" },
      "title": { "type": "string" },
      "body": { "type": "string" },
      "due": { "type": "string" },
      "cadence": { "type": "string" },
      "query": { "type": "string" }
    }, "required": ["action"] })
  }

  async fn run(&self, args: &serde_json::Value) -> Result<String, String> {
    let conn = crate::db::db(&self.app)?;
    match str_arg(args, "action").unwrap_or_else(|| "list".into()).trim() {
      "add" => add(&self.app, &conn, args),
      "list" => Ok(list(&conn, str_arg(args, "kind").as_deref())),
      "complete" => edit(&conn, args, true),
      "delete" => edit(&conn, args, false),
      other => Err(format!("Unknown items action \"{other}\". Use add | list | complete | delete.")),
    }
  }
}

fn add(app: &AppHandle, conn: &Connection, args: &serde_json::Value) -> Result<String, String> {
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
    due_at: due.clone(), cadence: &cadence, project_id: "", source: "app",
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

fn edit(conn: &Connection, args: &serde_json::Value, complete: bool) -> Result<String, String> {
  let who = if complete { "complete" } else { "delete" };
  let query = str_arg(args, "query").or_else(|| str_arg(args, "title")).unwrap_or_default();
  let q = query.trim();
  if q.is_empty() { return Err(format!("items {who} requires a 'query' naming the item.")); }
  let Some(item) = crate::capture::find(conn, q) else { return Err(format!("No item matching \"{q}\".")); };
  if complete {
    let reply = crate::capture::complete(conn, &item.id)?;
    Ok(format!("{reply} ({})", item.title))
  } else {
    conn.execute("DELETE FROM items WHERE id=?1", rusqlite::params![item.id]).map_err(|e| e.to_string())?;
    Ok(format!("Removed \"{}\".", item.title))
  }
}

/// The items tool, held by every agent and the Manager.
pub(crate) fn items_tool(app: &AppHandle) -> Box<dyn AgentTool> {
  Box::new(ItemsTool { app: app.clone() })
}
