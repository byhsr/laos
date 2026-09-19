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
}

#[async_trait]
impl AgentTool for McpTool {
  fn name(&self) -> String { self.tool_name.clone() }

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
