// The agent's own working area for a chat. Unlike every other tool it needs to
// know which chat it is in, so it overrides `run_ctx` and reads the session from
// the turn's context — that is what makes the pad per session.

use async_trait::async_trait;

use super::{clip, str_arg, AgentTool, ToolCtx, ToolOutput};
use crate::scratchpad;

pub(crate) struct ScratchpadTool {
  app: tauri::AppHandle,
}

pub(crate) fn scratchpad_tool(app: &tauri::AppHandle) -> Box<dyn AgentTool> {
  Box::new(ScratchpadTool { app: app.clone() })
}

#[async_trait]
impl AgentTool for ScratchpadTool {
  fn name(&self) -> String { "scratchpad".into() }

  fn description(&self) -> String {
    "Your own private working area for this chat: thinking, drafts, intermediate findings, anything you need to keep track of while you work. This is NOT the user's notes — never file your own working notes with the items tool. Actions: 'read' (default — returns the whole pad), 'append' (add a block), 'write' (replace it entirely), 'clear'. Params: action (string), text (string, for append/write).".into()
  }

  fn params_schema(&self) -> serde_json::Value {
    serde_json::json!({
      "type": "object",
      "properties": {
        "action": { "type": "string", "enum": ["read", "append", "write", "clear"], "description": "What to do with the pad." },
        "text": { "type": "string", "description": "The note to append, or the full contents to write." }
      },
      "required": ["action"]
    })
  }

  // Reached only by a call that never got a turn context; it lands on the
  // agent-scoped pad, which is the same one a session-less turn would use.
  async fn run(&self, args: &serde_json::Value) -> Result<String, String> {
    self.run_ctx(args, &ToolCtx { agent_id: "", session_id: None }).await.map(|o| o.text)
  }

  async fn run_ctx(&self, args: &serde_json::Value, ctx: &ToolCtx<'_>) -> Result<ToolOutput, String> {
    let conn = crate::db::db(&self.app)?;
    let scope = scratchpad::scope_for(ctx.session_id, ctx.agent_id);
    let action = str_arg(args, "action").unwrap_or_else(|| "read".into());
    match action.as_str() {
      "read" | "list" => {
        let pad = scratchpad::read(&conn, &scope);
        Ok(ToolOutput::text(if pad.trim().is_empty() { "Your scratchpad for this chat is empty.".to_string() } else { pad }))
      }
      "append" => {
        let text = str_arg(args, "text").unwrap_or_default();
        if text.trim().is_empty() { return Err("The scratchpad 'append' action needs 'text'.".into()); }
        let pad = scratchpad::append(&conn, &scope, text.trim())?;
        Ok(ToolOutput::text(format!("Added. Your scratchpad now holds:\n\n{}", clip(&pad))))
      }
      "write" => {
        let text = str_arg(args, "text").unwrap_or_default();
        if text.trim().is_empty() { return Err("The scratchpad 'write' action needs 'text'.".into()); }
        scratchpad::write(&conn, &scope, text.trim())?;
        Ok(ToolOutput::text("Scratchpad replaced."))
      }
      "clear" => {
        scratchpad::clear(&conn, &scope)?;
        Ok(ToolOutput::text("Scratchpad cleared."))
      }
      other => Err(format!("Unknown scratchpad action '{other}'. Use read, append, write or clear.")),
    }
  }
}
