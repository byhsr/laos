// Chat documents ("artifacts"): a self-contained file an agent writes in a chat —
// markdown, plain text or html — kept so the user can read and download it. It
// belongs to the chat that produced it (agent + session); it is never mixed with
// the user's own items or the shared knowledge base.

use rusqlite::{params, Connection};
use tauri::{AppHandle, Emitter};

use crate::db::db;

const KINDS: &[&str] = &["markdown", "text", "html"];

#[derive(serde::Serialize, serde::Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Document {
  pub id: String,
  pub agent_id: String,
  pub session_id: String,
  pub title: String,
  pub kind: String,
  pub content: String,
  pub created_at: String,
  pub updated_at: String,
}

fn row(r: &rusqlite::Row) -> rusqlite::Result<Document> {
  Ok(Document {
    id: r.get(0)?, agent_id: r.get(1)?, session_id: r.get(2)?, title: r.get(3)?,
    kind: r.get(4)?, content: r.get(5)?, created_at: r.get(6)?, updated_at: r.get(7)?,
  })
}

// Anything unrecognised reads as markdown, which is the common case.
pub(crate) fn normalized_kind(raw: &str) -> String {
  let k = raw.trim().to_ascii_lowercase();
  if KINDS.contains(&k.as_str()) { k } else { "markdown".into() }
}

pub(crate) fn create(conn: &Connection, agent_id: &str, session_id: &str, title: &str, kind: &str, content: &str) -> Result<Document, String> {
  let id = format!("doc-{}", chrono::Utc::now().timestamp_millis());
  let ts = chrono::Utc::now().to_rfc3339();
  conn.execute(
    "INSERT INTO documents (id, agent_id, session_id, title, kind, content, created_at, updated_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?7)",
    params![id, agent_id, session_id, title, normalized_kind(kind), content, ts],
  ).map_err(|e| e.to_string())?;
  get(conn, &id)?.ok_or_else(|| "The document vanished after saving.".into())
}

pub(crate) fn update(conn: &Connection, id: &str, title: Option<&str>, content: Option<&str>) -> Result<Document, String> {
  let Some(mut doc) = get(conn, id)? else { return Err(format!("No document with id '{id}'.")) };
  if let Some(t) = title.map(str::trim).filter(|t| !t.is_empty()) { doc.title = t.to_string(); }
  if let Some(c) = content { doc.content = c.to_string(); }
  doc.updated_at = chrono::Utc::now().to_rfc3339();
  conn.execute("UPDATE documents SET title=?2, content=?3, updated_at=?4 WHERE id=?1", params![doc.id, doc.title, doc.content, doc.updated_at]).map_err(|e| e.to_string())?;
  Ok(doc)
}

pub(crate) fn get(conn: &Connection, id: &str) -> Result<Option<Document>, String> {
  let mut stmt = conn.prepare("SELECT id, agent_id, session_id, title, kind, content, created_at, updated_at FROM documents WHERE id=?1").map_err(|e| e.to_string())?;
  let mut rows = stmt.query_map(params![id], row).map_err(|e| e.to_string())?;
  rows.next().transpose().map_err(|e| e.to_string())
}

pub(crate) fn list(conn: &Connection, agent_id: &str, session_id: &str) -> Vec<Document> {
  let Ok(mut stmt) = conn.prepare("SELECT id, agent_id, session_id, title, kind, content, created_at, updated_at FROM documents WHERE agent_id=?1 AND session_id=?2 ORDER BY created_at ASC") else { return Vec::new() };
  let Ok(rows) = stmt.query_map(params![agent_id, session_id], row) else { return Vec::new() };
  rows.filter_map(|r| r.ok()).collect()
}

/// Names a document by exact id, or the first whose title contains `query`.
pub(crate) fn find(conn: &Connection, agent_id: &str, session_id: &str, query: &str) -> Option<Document> {
  let docs = list(conn, agent_id, session_id);
  if let Some(exact) = docs.iter().find(|d| d.id == query) { return Some(exact.clone()); }
  let q = query.trim().to_ascii_lowercase();
  if q.is_empty() { return None; }
  docs.into_iter().find(|d| d.title.to_ascii_lowercase().contains(&q))
}

pub(crate) fn delete(conn: &Connection, id: &str) -> Result<(), String> {
  conn.execute("DELETE FROM documents WHERE id=?1", params![id]).map(|_| ()).map_err(|e| e.to_string())
}

// Tells the UI a chat's documents changed, so its cards refresh live.
pub(crate) fn emit_changed(app: &AppHandle, doc: &Document, deleted: bool) {
  let _ = app.emit("lumin-document-changed", serde_json::json!({
    "id": doc.id, "agentId": doc.agent_id, "sessionId": doc.session_id, "deleted": deleted,
  }));
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn list_documents(app: AppHandle, agent_id: String, session_id: String) -> Result<Vec<Document>, String> {
  let conn = db(&app)?;
  Ok(list(&conn, &agent_id, &session_id))
}

#[tauri::command]
pub fn get_document(app: AppHandle, id: String) -> Result<Option<Document>, String> {
  let conn = db(&app)?;
  get(&conn, &id)
}

#[tauri::command]
pub fn delete_document(app: AppHandle, id: String) -> Result<(), String> {
  let conn = db(&app)?;
  let doc = get(&conn, &id)?;
  delete(&conn, &id)?;
  if let Some(d) = doc { emit_changed(&app, &d, true); }
  Ok(())
}

// Saves a chat reply as an artifact, from the message's "keep" action. Same store
// the `document` tool writes to, so it appears in the artifacts menu at once.
#[tauri::command]
pub fn create_document(app: AppHandle, agent_id: String, session_id: String, title: String, kind: String, content: String) -> Result<Document, String> {
  if content.trim().is_empty() { return Err("Nothing to keep.".into()); }
  let conn = db(&app)?;
  let title = if title.trim().is_empty() { "Kept reply".to_string() } else { title.trim().to_string() };
  let doc = create(&conn, &agent_id, &session_id, &title, &kind, &content)?;
  emit_changed(&app, &doc, false);
  Ok(doc)
}
