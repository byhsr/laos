// Tools every agent holds: the shared knowledge base (durable REFERENCE material
// any agent can consult), `remember` (facts and standing rules about the user) and
// `items` (todos/reminders — see items.rs).

use async_trait::async_trait;
use tauri::AppHandle;

use super::{str_arg, AgentTool};

pub(crate) struct KnowledgeBaseTool { pub(crate) app: AppHandle }

#[async_trait]
impl AgentTool for KnowledgeBaseTool {
  fn name(&self) -> String { "knowledge_base".into() }
  fn description(&self) -> String {
    "The shared knowledge base — durable REFERENCE material any agent may need later (company wiki, ICP, \
specs, policies, runbooks, product facts). Call 'search' or 'get' before answering a question about the \
user's world; call 'save' to store reference material worth keeping. NOT for anything to do — tasks, chores, \
errands, reminders or one-off to-dos belong in the 'items' tool — and NOT for personal rules about the user \
(the 'remember' tool). Only durable reference material belongs here. Params: action ('list' | 'get' | \
'search' | 'save'), title (string; for save/search), content (string; for save), id (string; for get/save).".into()
  }
  fn params_schema(&self) -> serde_json::Value {
    serde_json::json!({ "type": "object", "properties": { "action": { "type": "string" }, "title": { "type": "string" }, "content": { "type": "string" }, "id": { "type": "string" } }, "required": ["action"] })
  }
  async fn run(&self, args: &serde_json::Value) -> Result<String, String> {
    let conn = crate::db::db(&self.app)?;
    crate::knowledge::run(&conn, args)
  }
}

pub(crate) struct RememberTool { pub(crate) app: AppHandle, pub(crate) agent_id: String }

#[async_trait]
impl AgentTool for RememberTool {
  fn name(&self) -> String { "remember".into() }
  fn description(&self) -> String {
    "Save a durable FACT or standing RULE about the user (\"never use emoji or em dashes\", \"I prefer IST\", \
\"my name is Ranas\") — remembered across every future chat. Use kind 'policy' for a never/always rule, \
'preference' for a taste, 'semantic' for a plain fact; policy and preference apply to every agent. Do NOT use \
this for things to do — a todo or reminder goes to the 'items' tool. Params: content (string — what to \
remember, with enough context to be useful later), title (string, optional — a short label), kind (string, \
optional).".into()
  }
  fn params_schema(&self) -> serde_json::Value {
    serde_json::json!({ "type": "object", "properties": { "content": { "type": "string" }, "title": { "type": "string" }, "kind": { "type": "string" } }, "required": ["content"] })
  }
  async fn run(&self, args: &serde_json::Value) -> Result<String, String> {
    let content = str_arg(args, "content").unwrap_or_default();
    if content.trim().is_empty() { return Err("remember requires a 'content' argument.".into()); }
    let title = str_arg(args, "title").filter(|t| !t.trim().is_empty()).unwrap_or_else(|| content.chars().take(80).collect::<String>());
    let kind = str_arg(args, "kind").unwrap_or_default();
    let kind = if kind.trim().is_empty() { "semantic".to_string() } else { kind };
    // A rule concerns the user, not one agent, so it is stored globally and
    // applies everywhere; a plain fact stays in the agent's own scope.
    let (id, created) = if kind.trim() == "policy" || kind.trim() == "preference" {
      crate::memory::store_rule(&self.app, &self.agent_id, kind.trim(), &title, &content)?
    } else {
      crate::memory::store_remembered(&self.app, &self.agent_id, &title, &content, kind.trim())?
    };
    Ok(if created { format!("Remembered \"{title}\" (id: {id}).") } else { format!("Already remembered \"{title}\".") })
  }
}

/// The shared knowledge base tool, which every agent holds.
pub(crate) fn knowledge_tool(app: &AppHandle) -> Box<dyn AgentTool> {
  Box::new(KnowledgeBaseTool { app: app.clone() })
}

/// The explicit-remember tool, filed under the agent's own scope (global for rules).
pub(crate) fn remember_tool(app: &AppHandle, agent_id: &str) -> Box<dyn AgentTool> {
  Box::new(RememberTool { app: app.clone(), agent_id: agent_id.to_string() })
}
