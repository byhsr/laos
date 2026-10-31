// The re-file pass: the knowledge base accumulated docs that are really memories
// or todos. `preview` reads every doc and proposes where it should live; `apply`
// moves the accepted ones and prunes them from the knowledge base.
//
// Reads only — nothing moves until the user approves a proposal.

use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use tauri::AppHandle;

use crate::db::db;

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Proposal {
  pub doc_id: String,
  pub title: String,
  pub content: String,
  /// "memory" (a fact or rule about the user) or "items" (something to do).
  pub destination: String,
  pub kind: String,
  pub due_at: Option<String>,
}

struct Doc { id: String, title: String, content: String }

fn load_docs(conn: &Connection) -> Result<Vec<Doc>, String> {
  let mut stmt = conn
    .prepare("SELECT id, title, content FROM knowledge_docs ORDER BY updated_at DESC")
    .map_err(|e| e.to_string())?;
  let rows = stmt
    .query_map([], |r| Ok(Doc { id: r.get(0)?, title: r.get(1)?, content: r.get(2)? }))
    .map_err(|e| e.to_string())?;
  Ok(rows.filter_map(|r| r.ok()).collect())
}

/// Every doc that looks like it belongs somewhere else, with the destination it
/// would move to. Docs that are genuine reference material are left out.
pub(crate) async fn preview(app: &AppHandle) -> Result<Vec<Proposal>, String> {
  let docs = { let conn = db(app)?; load_docs(&conn)? };
  let mut out = Vec::new();
  for doc in docs {
    let text = format!("{}\n{}\n{}", doc.title, doc.title, doc.content);
    let Some((destination, kind)) = crate::routing::classify_text(app, &text).await else { continue };
    let destination = if destination == crate::routing::Destination::Memory { "memory" } else { "items" };
    let due = if destination == "items" {
      let conn = db(app)?;
      crate::routing::resolve_due(&conn, &doc.content)
    } else {
      None
    };
    out.push(Proposal { doc_id: doc.id, title: doc.title, content: doc.content, destination: destination.into(), kind, due_at: due });
  }
  Ok(out)
}

/// Moves the given proposals and removes their docs from the knowledge base.
/// Returns how many moved.
pub(crate) fn apply(app: &AppHandle, proposals: &[Proposal]) -> Result<usize, String> {
  let conn = db(app)?;
  let mut moved = 0;
  for p in proposals {
    match p.destination.as_str() {
      "memory" => {
        // A rule or fact concerns the user, not one agent, so it goes global.
        crate::memory::store_rule(app, "manager", &p.kind, &p.title, &p.content)?;
      }
      "items" => {
        let item = crate::capture::add(&conn, crate::capture::NewItem {
          kind: &p.kind, title: &p.title, body: &p.content, url: "", tags: Vec::new(), collection: "",
          due_at: p.due_at.clone(), cadence: "", project_id: "", source: "app",
        })?;
        crate::brain::ingest_item(app, &conn, &item, &crate::capture::synthesized_class(&p.kind), "life");
      }
      other => return Err(format!("Unknown destination \"{other}\".")),
    }
    conn.execute("DELETE FROM knowledge_docs WHERE id=?1", rusqlite::params![p.doc_id]).map_err(|e| e.to_string())?;
    moved += 1;
  }
  Ok(moved)
}

#[tauri::command]
pub async fn refile_knowledge_preview(app: AppHandle) -> Result<Vec<Proposal>, String> {
  preview(&app).await
}

#[tauri::command]
pub fn refile_knowledge_apply(app: AppHandle, proposals: Vec<Proposal>) -> Result<usize, String> {
  apply(&app, &proposals)
}
