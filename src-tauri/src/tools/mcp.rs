// MCP tool: wraps one tool advertised by a configured MCP server so agents can
// call it like any other AgentTool. The blocking stdio round-trip runs on a
// blocking thread so it never stalls the async runtime.

use async_trait::async_trait;

use super::{clip, AgentTool};
use crate::mcp::{self, McpServer};

pub(crate) struct McpTool {
  pub(crate) server: McpServer,
  pub(crate) tool_name: String,
  pub(crate) tool_description: String,
  pub(crate) schema: serde_json::Value,
  // Set only when another attached server exposes the same model-facing name
  // (e.g. two servers both named "Notion"); the suffix keeps both addressable.
  pub(crate) alias: Option<String>,
}

// Providers only accept [a-zA-Z0-9_-] function names, but MCP servers advertise
// names like "API.post-search". Sanitize the name the model sees, keep the real
// name for the actual tools/call, and prefix with the server so two servers can
// both expose e.g. "search" without colliding.
fn sanitize(s: &str) -> String {
  let mut out: String = s.chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '_' }).collect();
  if out.is_empty() { out.push_str("tool"); }
  out.truncate(56);
  out
}

// The name this tool would take if nothing else claimed it.
pub(crate) fn default_name(server_name: &str, tool_name: &str) -> String {
  format!("{}_{}", sanitize(server_name), sanitize(tool_name)).chars().take(64).collect()
}

// Disambiguates a model-facing name against the ones already taken by appending
// `_2`, `_3`, … (kept inside the 64-char provider limit).
pub(crate) fn unique_name(base: String, used: &std::collections::HashSet<String>) -> String {
  if !used.contains(&base) { return base; }
  let stem: String = base.chars().take(60).collect();
  for n in 2..1000 {
    let candidate = format!("{stem}_{n}");
    if !used.contains(&candidate) { return candidate; }
  }
  base
}

#[async_trait]
impl AgentTool for McpTool {
  fn name(&self) -> String {
    if let Some(alias) = &self.alias { return alias.clone(); }
    default_name(&self.server.name, &self.tool_name)
  }

  fn description(&self) -> String {
    if self.tool_description.trim().is_empty() {
      format!("{} (MCP tool on '{}')", self.tool_name, self.server.name)
    } else {
      self.tool_description.clone()
    }
  }

  fn params_schema(&self) -> serde_json::Value { self.schema.clone() }

  async fn run(&self, args: &serde_json::Value) -> Result<String, String> {
    let server = self.server.clone();
    let tool = self.tool_name.clone();
    let args = args.clone();
    tokio::task::spawn_blocking(move || mcp::call_tool(&server, &tool, args))
      .await
      .map_err(|e| e.to_string())?
      .map(|text| clip(&text))
  }
}
