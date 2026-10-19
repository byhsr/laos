// Native browser-control tools backed by the embedded mini-browser webview
// (see crate::browser). These replace the old Playwright MCP server: no Node,
// no cold-start timeout, and the page is visible in the left panel while the
// agent works. Each tool shows the panel first, so browser control always slides
// the drawer open.

use async_trait::async_trait;

use crate::tools::{str_arg, AgentTool};

struct BrowserOpen;

#[async_trait]
impl AgentTool for BrowserOpen {
  fn name(&self) -> String { "browser_open".into() }
  fn description(&self) -> String {
    "Open a URL in the in-app mini browser (a panel on the left of the app) and show it. This is the app's own browser and does NOT open anything on the user's desktop. Use it first, then browser_read to see the page; the user can watch and interact.".into()
  }
  fn params_schema(&self) -> serde_json::Value {
    serde_json::json!({ "type": "object", "properties": { "url": { "type": "string", "description": "The URL to open (a bare domain defaults to https)" } }, "required": ["url"] })
  }
  async fn run(&self, args: &serde_json::Value) -> Result<String, String> {
    let url = str_arg(args, "url").ok_or("browser_open requires a 'url' argument.")?;
    crate::browser::tool_open(&url).await
  }
}

struct BrowserNavigate;

#[async_trait]
impl AgentTool for BrowserNavigate {
  fn name(&self) -> String { "browser_navigate".into() }
  fn description(&self) -> String {
    "Navigate the in-app mini browser to a URL (the app's own browser panel, not the user's desktop). Same as browser_open.".into()
  }
  fn params_schema(&self) -> serde_json::Value {
    serde_json::json!({ "type": "object", "properties": { "url": { "type": "string", "description": "The URL to navigate to" } }, "required": ["url"] })
  }
  async fn run(&self, args: &serde_json::Value) -> Result<String, String> {
    let url = str_arg(args, "url").ok_or("browser_navigate requires a 'url' argument.")?;
    crate::browser::tool_open(&url).await
  }
}

struct BrowserBack;

#[async_trait]
impl AgentTool for BrowserBack {
  fn name(&self) -> String { "browser_back".into() }
  fn description(&self) -> String { "Go back one page in the mini browser's history.".into() }
  fn params_schema(&self) -> serde_json::Value { serde_json::json!({ "type": "object", "properties": {}, "required": [] }) }
  async fn run(&self, _args: &serde_json::Value) -> Result<String, String> { crate::browser::tool_back().await }
}

struct BrowserForward;

#[async_trait]
impl AgentTool for BrowserForward {
  fn name(&self) -> String { "browser_forward".into() }
  fn description(&self) -> String { "Go forward one page in the mini browser's history.".into() }
  fn params_schema(&self) -> serde_json::Value { serde_json::json!({ "type": "object", "properties": {}, "required": [] }) }
  async fn run(&self, _args: &serde_json::Value) -> Result<String, String> { crate::browser::tool_forward().await }
}

struct BrowserReload;

#[async_trait]
impl AgentTool for BrowserReload {
  fn name(&self) -> String { "browser_reload".into() }
  fn description(&self) -> String { "Reload the current page in the mini browser.".into() }
  fn params_schema(&self) -> serde_json::Value { serde_json::json!({ "type": "object", "properties": {}, "required": [] }) }
  async fn run(&self, _args: &serde_json::Value) -> Result<String, String> { crate::browser::tool_reload().await }
}

struct BrowserRead;

#[async_trait]
impl AgentTool for BrowserRead {
  fn name(&self) -> String { "browser_read".into() }
  fn description(&self) -> String {
    "Read the current page: returns its URL, title and visible text. Use it after navigating or clicking to see the result. Long pages are truncated.".into()
  }
  fn params_schema(&self) -> serde_json::Value { serde_json::json!({ "type": "object", "properties": {}, "required": [] }) }
  async fn run(&self, _args: &serde_json::Value) -> Result<String, String> { crate::browser::tool_read().await }
}

struct BrowserClick;

#[async_trait]
impl AgentTool for BrowserClick {
  fn name(&self) -> String { "browser_click".into() }
  fn description(&self) -> String {
    "Click an element in the mini browser. Give a CSS 'selector' OR a short 'text' to match against link/button labels (case-insensitive contains).".into()
  }
  fn params_schema(&self) -> serde_json::Value {
    serde_json::json!({ "type": "object", "properties": {
      "selector": { "type": "string", "description": "CSS selector, e.g. '#submit' or 'a.login'" },
      "text": { "type": "string", "description": "Visible label text to match (used if no selector)" },
    }, "required": [] })
  }
  async fn run(&self, args: &serde_json::Value) -> Result<String, String> {
    let selector = str_arg(args, "selector");
    let text = str_arg(args, "text");
    if selector.is_none() && text.is_none() { return Err("browser_click needs a 'selector' or 'text'.".into()); }
    crate::browser::tool_click(selector.as_deref(), text.as_deref()).await
  }
}

struct BrowserType;

#[async_trait]
impl AgentTool for BrowserType {
  fn name(&self) -> String { "browser_type".into() }
  fn description(&self) -> String {
    "Type text into a field in the mini browser. Identify the field with a CSS 'selector' OR a 'label' matched against its placeholder/name/aria-label. Set submit=true to submit the surrounding form.".into()
  }
  fn params_schema(&self) -> serde_json::Value {
    serde_json::json!({ "type": "object", "properties": {
      "selector": { "type": "string", "description": "CSS selector for the input/textarea" },
      "label": { "type": "string", "description": "Placeholder/name/aria-label text to match (used if no selector)" },
      "text": { "type": "string", "description": "The text to type" },
      "submit": { "type": "boolean", "description": "Submit the form after typing (default false)" },
    }, "required": ["text"] })
  }
  async fn run(&self, args: &serde_json::Value) -> Result<String, String> {
    let selector = str_arg(args, "selector");
    let label = str_arg(args, "label");
    let text = str_arg(args, "text").ok_or("browser_type requires 'text'.")?;
    let submit = args.get("submit").and_then(|v| v.as_bool()).unwrap_or(false);
    crate::browser::tool_type(selector.as_deref(), label.as_deref(), &text, submit).await
  }
}

// The full browser-control set, granted when an agent attaches the `browser` tool.
pub(crate) fn browser_tools() -> Vec<Box<dyn AgentTool>> {
  vec![
    Box::new(BrowserOpen),
    Box::new(BrowserNavigate),
    Box::new(BrowserBack),
    Box::new(BrowserForward),
    Box::new(BrowserReload),
    Box::new(BrowserRead),
    Box::new(BrowserClick),
    Box::new(BrowserType),
  ]
}
