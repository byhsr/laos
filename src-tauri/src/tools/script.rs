// Scripts: run a saved, reusable command ("custom app") by id or name. Granted
// alongside the host-filesystem tools (it executes arbitrary commands).

use async_trait::async_trait;
use rusqlite::Connection;

use super::{str_arg, AgentTool};
use crate::models::ScriptRecord;

pub(crate) struct ListScriptsTool { pub(crate) scripts: Vec<ScriptRecord> }
pub(crate) struct RunScriptTool { pub(crate) scripts: Vec<ScriptRecord> }

#[async_trait]
impl AgentTool for ListScriptsTool {
  fn name(&self) -> String { "list_scripts".into() }
  fn description(&self) -> String { "List the saved scripts (custom apps) that can be run with run_script.".into() }
  fn params_schema(&self) -> serde_json::Value { serde_json::json!({ "type": "object", "properties": {}, "required": [] }) }
  async fn run(&self, _args: &serde_json::Value) -> Result<String, String> {
    if self.scripts.is_empty() { return Ok("No apps saved.".into()); }
    Ok(self.scripts.iter().map(|s| format!("- {} ({}, id: {}): {}", s.name, s.kind, s.id, s.description)).collect::<Vec<_>>().join("\n"))
  }
}

#[async_trait]
impl AgentTool for RunScriptTool {
  fn name(&self) -> String { "run_script".into() }
  fn description(&self) -> String {
    "Run a saved command app. Params: script (string: id or name), input (string, optional; substituted for {input} in the command). Returns the command's stdout + stderr. (Prompt apps run from the Apps tab or a workflow, not here.)".into()
  }
  fn params_schema(&self) -> serde_json::Value {
    serde_json::json!({ "type": "object", "properties": { "script": { "type": "string" }, "input": { "type": "string" } }, "required": ["script"] })
  }
  async fn run(&self, args: &serde_json::Value) -> Result<String, String> {
    let key = str_arg(args, "script").ok_or("run_script requires a 'script' (id or name).")?;
    let input = str_arg(args, "input").unwrap_or_default();
    let script = self.scripts.iter().find(|s| s.id == key || s.name.eq_ignore_ascii_case(&key))
      .ok_or_else(|| format!("No app named '{key}'. Use list_scripts to see them."))?;
    if script.kind != "command" {
      return Err(format!("'{}' is a prompt app — run it from the Apps tab or a workflow.", script.name));
    }
    let command = script.command.replace("{input}", &input);
    let cwd = script.cwd.clone();
    tokio::task::spawn_blocking(move || crate::tools::run_shell_in(&command, &cwd))
      .await
      .map_err(|e| e.to_string())?
  }
}

/// The script tools, built from the current registry. Empty when no scripts are
/// saved, so agents that never use scripts pay nothing.
pub(crate) fn script_tools(conn: &Connection) -> Vec<Box<dyn AgentTool>> {
  let scripts = crate::storage::load_scripts(conn);
  if scripts.is_empty() { return Vec::new(); }
  vec![
    Box::new(ListScriptsTool { scripts: scripts.clone() }),
    Box::new(RunScriptTool { scripts }),
  ]
}
