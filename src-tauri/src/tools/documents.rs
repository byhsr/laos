// The `document` tool — an artifact an agent writes for the user to keep. It
// saves a titled markdown/text/html file and the chat shows it as a card with a
// copy and download button, so long-form output doesn't have to be pasted into
// the reply. Needs the turn's context to know which chat the document belongs to.

use async_trait::async_trait;

use super::{clip, str_arg, AgentTool, ToolCtx, ToolOutput};
use crate::documents;

pub(crate) struct DocumentTool { app: tauri::AppHandle }

pub(crate) fn document_tool(app: &tauri::AppHandle) -> Box<dyn AgentTool> {
  Box::new(DocumentTool { app: app.clone() })
}

#[async_trait]
impl AgentTool for DocumentTool {
  fn name(&self) -> String { "document".into() }

  fn description(&self) -> String {
    "Write a document — an artifact the user can read and download in this chat. Use it when a reply is really a piece \
of writing worth keeping (a report, spec, plan, README, article, script or snippet): put the FULL content here instead \
of pasting it into the chat. The document shows up as a card with a copy and download button. Params: action ('create' \
— write a new one, 'update' — change an existing one, 'read', 'list', 'delete'), title (string), content (the whole \
markdown/text/html body), kind ('markdown' default | 'text' | 'html'), id (the document id, or part of its title, for \
update/read/delete).".into()
  }

  fn params_schema(&self) -> serde_json::Value {
    serde_json::json!({ "type": "object", "properties": {
      "action": { "type": "string", "enum": ["create", "update", "read", "list", "delete"], "description": "What to do." },
      "title": { "type": "string", "description": "The document's title / filename." },
      "content": { "type": "string", "description": "The full body (markdown by default)." },
      "kind": { "type": "string", "enum": ["markdown", "text", "html"], "description": "The body format; default markdown." },
      "id": { "type": "string", "description": "The document id (or part of its title) for update/read/delete." }
    }, "required": ["action"] })
  }

  // Reached only by a call with no turn context; it lands on the agent-scoped chat.
  async fn run(&self, args: &serde_json::Value) -> Result<String, String> {
    self.run_ctx(args, &ToolCtx { agent_id: "", session_id: None }).await.map(|o| o.text)
  }

  async fn run_ctx(&self, args: &serde_json::Value, ctx: &ToolCtx<'_>) -> Result<ToolOutput, String> {
    let conn = crate::db::db(&self.app)?;
    let agent_id = ctx.agent_id;
    let session_id = ctx.session_id.unwrap_or("");
    match str_arg(args, "action").unwrap_or_else(|| "list".into()).trim() {
      "create" => {
        let content = str_arg(args, "content").unwrap_or_default();
        if content.trim().is_empty() { return Err("The document 'create' action needs 'content'.".into()); }
        let raw_title = str_arg(args, "title").unwrap_or_default();
        let title = if raw_title.trim().is_empty() { title_from(&content) } else { raw_title.trim().to_string() };
        let kind = str_arg(args, "kind").unwrap_or_else(|| "markdown".into());
        let doc = documents::create(&conn, agent_id, session_id, &title, &kind, &content)?;
        documents::emit_changed(&self.app, &doc, false);
        Ok(ToolOutput::text(format!(
          "Created the {} document \"{}\" (id: {}). It is now visible in the chat as a card the user can read and download — do not paste its contents into your reply.",
          doc.kind, doc.title, doc.id
        )))
      }
      "update" => {
        let id = str_arg(args, "id").unwrap_or_default();
        if id.trim().is_empty() { return Err("The document 'update' action needs 'id'.".into()); }
        let title = str_arg(args, "title");
        let content = str_arg(args, "content");
        let doc = documents::update(&conn, id.trim(), title.as_deref(), content.as_deref())?;
        documents::emit_changed(&self.app, &doc, false);
        Ok(ToolOutput::text(format!("Updated the document \"{}\" (id: {}).", doc.title, doc.id)))
      }
      "read" => {
        let id = str_arg(args, "id").unwrap_or_default();
        let doc = documents::find(&conn, agent_id, session_id, &id).ok_or_else(|| format!("No document matching \"{id}\"."))?;
        Ok(ToolOutput::text(format!("# {}\n({}, id {})\n\n{}", doc.title, doc.kind, doc.id, clip(&doc.content))))
      }
      "delete" => {
        let id = str_arg(args, "id").unwrap_or_default();
        let doc = documents::find(&conn, agent_id, session_id, &id).ok_or_else(|| format!("No document matching \"{id}\"."))?;
        documents::delete(&conn, &doc.id)?;
        documents::emit_changed(&self.app, &doc, true);
        Ok(ToolOutput::text(format!("Deleted the document \"{}\".", doc.title)))
      }
      "list" => {
        let docs = documents::list(&conn, agent_id, session_id);
        if docs.is_empty() { return Ok(ToolOutput::text("No documents in this chat yet.")); }
        Ok(ToolOutput::text(docs.iter().map(|d| format!("- {} ({}, id {})", d.title, d.kind, d.id)).collect::<Vec<_>>().join("\n")))
      }
      other => Err(format!("Unknown document action \"{other}\". Use create | update | read | list | delete.")),
    }
  }
}

// A title from the body's first meaningful line, with any heading markers stripped.
fn title_from(content: &str) -> String {
  let first = content.lines().find(|l| !l.trim().is_empty()).unwrap_or("Document").trim();
  let t: String = first.trim_start_matches('#').trim().chars().take(60).collect();
  if t.is_empty() { "Document".into() } else { t }
}
