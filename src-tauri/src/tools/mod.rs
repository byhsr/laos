// Tool abstraction: the AgentTool trait every agent-callable capability
// implements, plus shared helpers and the concrete tool implementations.

pub(crate) mod api;
pub(crate) mod desktop;
pub(crate) mod filesystem;
pub(crate) mod integration;
pub(crate) mod mcp;
pub(crate) mod script;
pub(crate) mod web;

pub(crate) use api::{ApiParam, ApiTool};
pub(crate) use desktop::desktop_tools;
pub(crate) use filesystem::{run_shell_in, ReadAnyFileTool, ReadFileTool, RunCommandTool, SearchFilesTool, WriteFileTool};
pub(crate) use integration::IntegrationTool;
pub(crate) use mcp::McpTool;
pub(crate) use script::script_tools;
pub(crate) use web::HttpTool;

use async_trait::async_trait;

const MAX_TOOL_CHARS: usize = 8000;
pub(crate) const MAX_TOOL_ROUNDS: usize = 5;

pub(crate) fn clip(s: &str) -> String {
  let chars: Vec<char> = s.chars().collect();
  if chars.len() <= MAX_TOOL_CHARS { s.to_string() }
  else { chars[..MAX_TOOL_CHARS].iter().collect::<String>() + "\nâ€¦[truncated]" }
}

// A tool's result: text plus optional images (base64 PNG, no `data:` prefix).
// Vision tools (screen capture) use the images; everything else leaves them empty.
pub(crate) struct ToolOutput {
  pub text: String,
  pub images: Vec<String>,
}

impl ToolOutput {
  pub(crate) fn text(text: impl Into<String>) -> Self {
    Self { text: text.into(), images: Vec::new() }
  }
}

#[async_trait]
pub(crate) trait AgentTool: Send + Sync {
  fn name(&self) -> String;
  fn description(&self) -> String;
  fn params_schema(&self) -> serde_json::Value;
  async fn run(&self, args: &serde_json::Value) -> Result<String, String>;
  /// Like `run`, but may also return images for the model to see. The default
  /// wraps `run`'s text with no images, so most tools need not implement it.
  async fn run_with_images(&self, args: &serde_json::Value) -> Result<ToolOutput, String> {
    Ok(ToolOutput { text: self.run(args).await?, images: Vec::new() })
  }
}

pub(crate) fn str_arg(args: &serde_json::Value, key: &str) -> Option<String> {
  args.get(key).and_then(|v| v.as_str()).map(|s| s.to_string())
}
