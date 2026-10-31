// The shared knowledge base: one durable, user- and agent-writable store of
// reference material (company wiki, ICP notes, decisions, preferences) that ANY
// agent can read and write — not just the Manager. Backed by `knowledge_docs`.
//
// Both the Manager's built-in `knowledge_base` tool and the `knowledge_base` tool
// every other agent holds route through `run`, so there is exactly one
// implementation and one source of truth.

use rusqlite::{params, Connection};

pub(crate) fn run(conn: &Connection, args: &serde_json::Value) -> Result<String, String> {
  let action = args.get("action").and_then(|v| v.as_str()).unwrap_or("list");
  let title = args.get("title").and_then(|v| v.as_str()).unwrap_or("");
  let content = args.get("content").and_then(|v| v.as_str()).unwrap_or("");
  let id = args.get("id").and_then(|v| v.as_str()).unwrap_or("");
  match action {
    "list" => {
      let mut stmt = conn.prepare("SELECT id, title, updated_at FROM knowledge_docs ORDER BY updated_at DESC").map_err(|e| e.to_string())?;
      let rows = stmt.query_map([], |row| Ok(format!("- {} (id: {})", row.get::<_, String>(1)?, row.get::<_, String>(0)?))).map_err(|e| e.to_string())?;
      let mut out = Vec::new();
      for r in rows { out.push(r.map_err(|e| e.to_string())?); }
      Ok(if out.is_empty() { "Knowledge base is empty.".into() } else { out.join("\n") })
    }
    "search" => {
      let like = format!("%{}%", title);
      let mut stmt = conn.prepare("SELECT id, title, content FROM knowledge_docs WHERE title LIKE ?1 OR content LIKE ?1 ORDER BY updated_at DESC LIMIT 5").map_err(|e| e.to_string())?;
      let rows = stmt.query_map(params![like], |row| Ok(format!("- {} (id: {}): {}", row.get::<_, String>(1)?, row.get::<_, String>(0)?, row.get::<_, String>(2)?.chars().take(150).collect::<String>()))).map_err(|e| e.to_string())?;
      let mut out = Vec::new();
      for r in rows { out.push(r.map_err(|e| e.to_string())?); }
      Ok(if out.is_empty() { format!("No docs matching \"{title}\".") } else { out.join("\n") })
    }
    "get" => {
      let mut stmt = conn.prepare("SELECT id, title, content FROM knowledge_docs WHERE id=?1 OR title=?1").map_err(|e| e.to_string())?;
      let mut rows = stmt.query_map(params![if id.is_empty() { title } else { id }], |row| Ok(format!("# {}\n\n{}", row.get::<_, String>(1)?, row.get::<_, String>(2)?))).map_err(|e| e.to_string())?;
      match rows.next().transpose().map_err(|e| e.to_string())? {
        Some(v) => Ok(v),
        None => Ok(format!("No knowledge base doc found for \"{}\".", if id.is_empty() { title } else { id })),
      }
    }
    "save" => {
      if title.is_empty() { return Err("knowledge_base save requires a title.".into()); }
      let doc_id = if id.is_empty() { format!("kb-{}", chrono::Utc::now().timestamp_millis()) } else { id.to_string() };
      conn.execute(
        "INSERT INTO knowledge_docs (id, title, content, tags, updated_at) VALUES (?1,?2,?3,'[]',?4)
         ON CONFLICT(id) DO UPDATE SET title=excluded.title, content=excluded.content, updated_at=excluded.updated_at",
        params![doc_id, title, content, chrono::Utc::now().to_rfc3339()],
      ).map_err(|e| e.to_string())?;
      Ok(format!("Saved knowledge base doc \"{title}\" (id: {doc_id})."))
    }
    _ => Err(format!("Unknown knowledge_base action {action}.")),
  }
}
