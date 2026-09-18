// Configurable REST API tool: method + URL template with {param} placeholders,
// headers, optional JSON body, and a manual parameter list.

use async_trait::async_trait;

use super::{clip, AgentTool};
use crate::http;

// Generic configurable API tool: method, URL template with {param} placeholders,
// headers, optional JSON body, and a manual parameter list. Runs any REST API.
pub(crate) struct ApiTool {
  pub(crate) name: String,
  pub(crate) description: String,
  pub(crate) method: String,
  pub(crate) url_template: String,
  pub(crate) headers: Vec<(String, String)>,
  pub(crate) body_template: Option<String>,
  pub(crate) params: Vec<ApiParam>,
}
pub(crate) struct ApiParam { pub(crate) name: String, pub(crate) param_type: String, pub(crate) description: String, pub(crate) required: bool }

fn fill_template(template: &str, args: &serde_json::Value) -> Result<String, String> {
  let mut out = template.to_string();
  // Replace {name} with the string value of args.name (JSON-encoded for body use).
  for (key, value) in args.as_object().ok_or("args must be an object")? {
    let placeholder = format!("{{{}}}", key);
    if out.contains(&placeholder) {
      let text = match value {
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
      };
      out = out.replace(&placeholder, &text);
    }
  }
  // Leftover placeholders are an error (missing required arg).
  if out.contains('{') && out.contains('}') {
    return Err(format!("Missing value for placeholder in template: {template}"));
  }
  Ok(out)
}

#[async_trait]
impl AgentTool for ApiTool {
  fn name(&self) -> String { self.name.clone() }
  fn description(&self) -> String { self.description.clone() }
  fn params_schema(&self) -> serde_json::Value {
    let mut properties = serde_json::Map::new();
    let mut required = Vec::new();
    for p in &self.params {
      properties.insert(p.name.clone(), serde_json::json!({
        "type": p.param_type,
        "description": p.description,
      }));
      if p.required { required.push(p.name.clone()); }
    }
    serde_json::json!({ "type": "object", "properties": properties, "required": required })
  }
  async fn run(&self, args: &serde_json::Value) -> Result<String, String> {
    let url = fill_template(&self.url_template, args)?;
    let mut req = http::client().request(
      reqwest::Method::from_bytes(self.method.as_bytes()).map_err(|e| format!("Invalid HTTP method {}: {e}", self.method))?,
      &url,
    );
    for (k, v) in &self.headers {
      let value = fill_template(v, args)?;
      req = req.header(k, value);
    }
    if let Some(body) = &self.body_template {
      let filled = fill_template(body, args)?;
      req = req.header("Content-Type", "application/json").body(filled);
    }
    let response = req.send().await.map_err(|e| format!("Request to {url} failed: {e}"))?;
    let status = response.status();
    let text = response.text().await.map_err(|e| e.to_string())?;
    // Prefer pretty-printed JSON so the model can read the structure.
    let body = if let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) {
      serde_json::to_string_pretty(&v).unwrap_or(text)
    } else { text };
    if !status.is_success() {
      return Err(format!("API returned {status}: {body}"));
    }
    Ok(clip(&body))
  }
}
