// Tool abstraction: the AgentTool trait every agent-callable capability
// implements, plus shared helpers and the concrete tool implementations.

pub(crate) mod api;
pub(crate) mod browser;
pub(crate) mod datetime;
pub(crate) mod desktop;
pub(crate) mod filesystem;
pub(crate) mod integration;
pub(crate) mod items;
pub(crate) mod knowledge;
pub(crate) mod mcp;
pub(crate) mod monid;
pub(crate) mod script;
pub(crate) mod scratchpad;
pub(crate) mod web;
pub(crate) mod ytdlp;

pub(crate) use api::{ApiAuth, ApiParam, ApiTool, Approval, ParamIn};
pub(crate) use browser::browser_tools;
pub(crate) use datetime::current_time_tool;
pub(crate) use desktop::desktop_tools;
pub(crate) use filesystem::{run_shell_in, ReadAnyFileTool, ReadFileTool, RunCommandTool, SearchFilesTool, WriteFileTool};
pub(crate) use integration::IntegrationTool;
pub(crate) use items::items_tool;
pub(crate) use knowledge::{knowledge_tool, remember_tool};
pub(crate) use mcp::{sanitize, unique_name, McpTool};
pub(crate) use monid::MonidTool;
pub(crate) use script::script_tools;
pub(crate) use scratchpad::scratchpad_tool;
pub(crate) use web::HttpTool;
pub(crate) use ytdlp::YtDlpTool;

use async_trait::async_trait;

const MAX_TOOL_CHARS: usize = 8000;
// Rounds a single turn may spend calling tools. Research-and-act turns routinely
// need more than a handful; slow calls are backgrounded now, so this is a backstop
// rather than the thing that stops a turn.
pub(crate) const MAX_TOOL_ROUNDS: usize = 12;

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

// What a tool knows about the turn it was called in. Almost every tool ignores
// it; the scratchpad needs the session so its pad is per chat.
pub(crate) struct ToolCtx<'a> {
  pub agent_id: &'a str,
  pub session_id: Option<&'a str>,
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
  /// How a tool is actually invoked: with the turn's context. The default drops
  /// the context and defers to `run_with_images`, so a tool that doesn't need the
  /// session — which is all of them but the scratchpad — is unchanged.
  async fn run_ctx(&self, args: &serde_json::Value, ctx: &ToolCtx<'_>) -> Result<ToolOutput, String> {
    let _ = ctx;
    self.run_with_images(args).await
  }
  /// Whether this tool asks the user before running. Only consulted while
  /// autonomy is off (which overrides everything), so a tool that never wants to
  /// be gated — which is most of them — needs no change.
  fn requires_approval(&self) -> bool { false }
  /// Whether this tool may take long enough to be worth backgrounding in the
  /// interactive chat. Name heuristics cover the builtins; a tool that knows
  /// better says so here.
  fn is_long_running(&self) -> bool { false }
}

pub(crate) fn str_arg(args: &serde_json::Value, key: &str) -> Option<String> {
  args.get(key).and_then(|v| v.as_str()).map(|s| s.to_string())
}
