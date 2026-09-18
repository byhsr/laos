// Tool abstraction: the AgentTool trait every agent-callable capability
// implements, plus shared helpers and the concrete tool implementations.

pub(crate) mod api;
pub(crate) mod filesystem;
pub(crate) mod integration;
pub(crate) mod web;

pub(crate) use api::{ApiParam, ApiTool};
pub(crate) use filesystem::{ReadAnyFileTool, ReadFileTool, RunCommandTool, SearchFilesTool, WriteFileTool};
pub(crate) use integration::IntegrationTool;
pub(crate) use web::HttpTool;

use async_trait::async_trait;

const MAX_TOOL_CHARS: usize = 8000;
pub(crate) const MAX_TOOL_ROUNDS: usize = 5;

pub(crate) fn clip(s: &str) -> String {
  let chars: Vec<char> = s.chars().collect();
  if chars.len() <= MAX_TOOL_CHARS { s.to_string() }
  else { chars[..MAX_TOOL_CHARS].iter().collect::<String>() + "\nâ€¦[truncated]" }
}

#[async_trait]
pub(crate) trait AgentTool: Send + Sync {
  fn name(&self) -> String;
  fn description(&self) -> String;
  fn params_schema(&self) -> serde_json::Value;
  async fn run(&self, args: &serde_json::Value) -> Result<String, String>;
}

pub(crate) fn str_arg(args: &serde_json::Value, key: &str) -> Option<String> {
  args.get(key).and_then(|v| v.as_str()).map(|s| s.to_string())
}
