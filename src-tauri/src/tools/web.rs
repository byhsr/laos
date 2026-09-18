// Web tool: a basic permission-scoped HTTP GET.

use async_trait::async_trait;

use super::{clip, str_arg, AgentTool};
use crate::http;

pub(crate) struct HttpTool;

#[async_trait]
impl AgentTool for HttpTool {
  fn name(&self) -> String { "http_get".into() }
  fn description(&self) -> String { "Make a permission-scoped GET request and return the response body as text.".into() }
  fn params_schema(&self) -> serde_json::Value {
    serde_json::json!({ "type": "object", "properties": { "url": { "type": "string", "description": "The URL to GET" } }, "required": ["url"] })
  }
  async fn run(&self, args: &serde_json::Value) -> Result<String, String> {
    let url = str_arg(args, "url").ok_or("http_get requires a 'url' argument.")?;
    let response = http::client().get(&url).send().await.map_err(|e| format!("GET {url} failed: {e}"))?;
    let text = response.text().await.map_err(|e| e.to_string())?;
    Ok(clip(&text))
  }
}
