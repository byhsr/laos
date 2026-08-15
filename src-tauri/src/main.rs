#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use async_trait::async_trait;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::fs;
use tauri::{AppHandle, Manager};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AgentRequest { id: String, name: String, objective: String, model: String, tool_ids: Vec<String>, home_path: String, permissions: Vec<String> }

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Execution { output: String, events: Vec<ExecutionEvent> }
#[derive(Serialize)]
struct ExecutionEvent { time: String, #[serde(rename = "type")] kind: String, title: String, detail: Option<String> }

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ModelConfigRecord { id: String, provider: String, label: String, model: String, host: Option<String>, api_key: Option<String>, enabled: bool }

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ToolRecord { id: String, name: String, kind: String, integration_id: String, description: Option<String>, enabled: bool, config: serde_json::Value }

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct AgentRecord {
  id: String, name: String, objective: String, model: String,
  tool_ids: Vec<String>, integrations: Vec<String>, memory: bool,
  permissions: Vec<String>, home_path: String, color: String,
  #[serde(default)] x: f64, #[serde(default)] y: f64,
}

// Ollama chat (tool-calling) request/response
#[derive(Serialize)]
struct ChatRequest { model: String, messages: Vec<ChatMessage>, tools: Option<Vec<serde_json::Value>>, stream: bool }
#[derive(Serialize, Deserialize, Clone)]
struct ChatMessage { role: String, content: Option<String>, #[serde(skip_serializing_if = "Option::is_none")] tool_calls: Option<Vec<ToolCall>>, #[serde(skip_serializing_if = "Option::is_none")] tool_call_id: Option<String> }
#[derive(Serialize, Deserialize, Clone)]
struct ToolCall { id: String, #[serde(rename = "type")] kind: String, function: ToolCallFunction }
#[derive(Serialize, Deserialize, Clone)]
struct ToolCallFunction { name: String, arguments: serde_json::Value }

#[derive(Deserialize)]
struct ChatResponse { message: ChatMessage }

// Firecrawl response shapes
#[derive(Deserialize)]
struct FirecrawlSearchResponse { data: Option<FirecrawlSearchData> }
#[derive(Deserialize)]
struct FirecrawlSearchData { #[serde(default)] web: Vec<SearchResult> }
#[derive(Deserialize)]
struct SearchResult { title: Option<String>, description: Option<String>, url: Option<String> }
#[derive(Deserialize)]
struct FirecrawlScrapeResponse { data: Option<FirecrawlScrapeData> }
#[derive(Deserialize)]
struct FirecrawlScrapeData { markdown: Option<String> }

// ---------------------------------------------------------------------------
// Web tools (configurable: base URL + API key come from the tool's config)
// ---------------------------------------------------------------------------

const MAX_TOOL_CHARS: usize = 8000;
const MAX_TOOL_ROUNDS: usize = 5;

fn clip(s: &str) -> String {
  let chars: Vec<char> = s.chars().collect();
  if chars.len() <= MAX_TOOL_CHARS { s.to_string() }
  else { chars[..MAX_TOOL_CHARS].iter().collect::<String>() + "\n…[truncated]" }
}

async fn web_api_search(base_url: &str, api_key: &str, query: &str, limit: usize) -> Result<String, String> {
  let body = serde_json::json!({ "query": query, "limit": limit.clamp(1, 10), "scrapeOptions": { "formats": [{ "type": "markdown" }] } });
  let mut req = reqwest::Client::new().post(format!("{}/v2/search", base_url)).header("Content-Type", "application/json");
  if !api_key.is_empty() { req = req.header("Authorization", format!("Bearer {api_key}")); }
  let response = req.json(&body).send().await.map_err(|e| format!("Could not reach the search API at {base_url}: {e}"))?;
  if !response.status().is_success() { return Err(format!("Search returned {}", response.status())); }
  let parsed: FirecrawlSearchResponse = response.json().await.map_err(|e| e.to_string())?;
  let results = parsed.data.map(|d| d.web).unwrap_or_default();
  if results.is_empty() { return Ok("No results found.".into()); }
  Ok(clip(&results.iter().enumerate().map(|(i, r)| format!("{}. {}\n   {}\n   {}", i + 1, r.title.as_deref().unwrap_or("Untitled"), r.url.as_deref().unwrap_or(""), r.description.as_deref().unwrap_or(""))).collect::<Vec<_>>().join("\n")))
}

async fn web_api_scrape(base_url: &str, api_key: &str, url: &str) -> Result<String, String> {
  let body = serde_json::json!({ "url": url, "formats": ["markdown"], "onlyMainContent": true });
  let mut req = reqwest::Client::new().post(format!("{}/v2/scrape", base_url)).header("Content-Type", "application/json");
  if !api_key.is_empty() { req = req.header("Authorization", format!("Bearer {api_key}")); }
  let response = req.json(&body).send().await.map_err(|e| format!("Could not reach the crawl API at {base_url}: {e}"))?;
  if !response.status().is_success() { return Err(format!("Scrape returned {}", response.status())); }
  let parsed: FirecrawlScrapeResponse = response.json().await.map_err(|e| e.to_string())?;
  let data = parsed.data.ok_or("The scrape API returned no data.")?;
  let markdown = data.markdown.unwrap_or_default();
  if markdown.trim().is_empty() { return Ok(format!("Scraped {} but got no readable markdown.", url)); }
  Ok(clip(&markdown))
}

// ---------------------------------------------------------------------------
// Tool layer
// ---------------------------------------------------------------------------

struct WebSearchTool { base_url: String, api_key: String }
struct WebCrawlTool { base_url: String, api_key: String }
struct HttpTool;

// Generic configurable API tool: method, URL template with {param} placeholders,
// headers, optional JSON body, and a manual parameter list. Runs any REST API.
struct ApiTool {
  name: String,
  description: String,
  method: String,
  url_template: String,
  headers: Vec<(String, String)>,
  body_template: Option<String>,
  params: Vec<ApiParam>,
}
struct ApiParam { name: String, param_type: String, description: String, required: bool }

#[async_trait]
trait AgentTool: Send + Sync {
  fn name(&self) -> String;
  fn description(&self) -> String;
  fn params_schema(&self) -> serde_json::Value;
  async fn run(&self, args: &serde_json::Value) -> Result<String, String>;
}

fn str_arg(args: &serde_json::Value, key: &str) -> Option<String> {
  args.get(key).and_then(|v| v.as_str()).map(|s| s.to_string())
}

#[async_trait]
impl AgentTool for WebSearchTool {
  fn name(&self) -> String { "web_search".into() }
  fn description(&self) -> String { "Search the web and return top results (title, URL, description). Use when the task needs current or external information.".into() }
  fn params_schema(&self) -> serde_json::Value {
    serde_json::json!({ "type": "object", "properties": { "query": { "type": "string", "description": "Search query" }, "limit": { "type": "integer", "description": "Max results (1-10)", "default": 5 } }, "required": ["query"] })
  }
  async fn run(&self, args: &serde_json::Value) -> Result<String, String> {
    let query = str_arg(args, "query").ok_or("web_search requires a 'query' argument.")?;
    let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(5) as usize;
    web_api_search(&self.base_url, &self.api_key, &query, limit).await
  }
}

#[async_trait]
impl AgentTool for WebCrawlTool {
  fn name(&self) -> String { "web_crawl".into() }
  fn description(&self) -> String { "Fetch and extract clean markdown content from a single URL. Use to read a page in full.".into() }
  fn params_schema(&self) -> serde_json::Value {
    serde_json::json!({ "type": "object", "properties": { "url": { "type": "string", "description": "The URL to scrape" } }, "required": ["url"] })
  }
  async fn run(&self, args: &serde_json::Value) -> Result<String, String> {
    let url = str_arg(args, "url").ok_or("web_crawl requires a 'url' argument.")?;
    web_api_scrape(&self.base_url, &self.api_key, &url).await
  }
}

#[async_trait]
impl AgentTool for HttpTool {
  fn name(&self) -> String { "http_get".into() }
  fn description(&self) -> String { "Make a permission-scoped GET request and return the response body as text.".into() }
  fn params_schema(&self) -> serde_json::Value {
    serde_json::json!({ "type": "object", "properties": { "url": { "type": "string", "description": "The URL to GET" } }, "required": ["url"] })
  }
  async fn run(&self, args: &serde_json::Value) -> Result<String, String> {
    let url = str_arg(args, "url").ok_or("http_get requires a 'url' argument.")?;
    let response = reqwest::Client::new().get(&url).send().await.map_err(|e| format!("GET {url} failed: {e}"))?;
    let text = response.text().await.map_err(|e| e.to_string())?;
    Ok(clip(&text))
  }
}

struct ReadFileTool { home: std::path::PathBuf }
struct WriteFileTool { home: std::path::PathBuf }

impl ReadFileTool {
  fn resolve(&self, name: &str) -> Result<std::path::PathBuf, String> {
    let p = self.home.join("files").join(name);
    if !p.starts_with(&self.home) { return Err("Path escapes agent home.".into()); }
    Ok(p)
  }
}
impl WriteFileTool {
  fn resolve(&self, name: &str) -> Result<std::path::PathBuf, String> {
    let p = self.home.join("files").join(name);
    if !p.starts_with(&self.home) { return Err("Path escapes agent home.".into()); }
    Ok(p)
  }
}

#[async_trait]
impl AgentTool for ReadFileTool {
  fn name(&self) -> String { "read_file".into() }
  fn description(&self) -> String { "Read a file from the agent's isolated files directory.".into() }
  fn params_schema(&self) -> serde_json::Value {
    serde_json::json!({ "type": "object", "properties": { "name": { "type": "string", "description": "File name relative to the agent files directory" } }, "required": ["name"] })
  }
  async fn run(&self, args: &serde_json::Value) -> Result<String, String> {
    let name = str_arg(args, "name").ok_or("read_file requires a 'name' argument.")?;
    let path = self.resolve(&name)?;
    if !path.exists() { return Ok(format!("File '{}' does not exist in the agent files directory.", name)); }
    Ok(clip(&fs::read_to_string(&path).map_err(|e| e.to_string())?))
  }
}

#[async_trait]
impl AgentTool for WriteFileTool {
  fn name(&self) -> String { "write_file".into() }
  fn description(&self) -> String { "Write text content to a file in the agent's isolated files directory.".into() }
  fn params_schema(&self) -> serde_json::Value {
    serde_json::json!({ "type": "object", "properties": { "name": { "type": "string", "description": "File name relative to the agent files directory" }, "content": { "type": "string", "description": "Full text content to write" } }, "required": ["name", "content"] })
  }
  async fn run(&self, args: &serde_json::Value) -> Result<String, String> {
    let name = str_arg(args, "name").ok_or("write_file requires a 'name' argument.")?;
    let content = str_arg(args, "content").ok_or("write_file requires a 'content' argument.")?;
    let path = self.resolve(&name)?;
    if let Some(parent) = path.parent() { fs::create_dir_all(parent).map_err(|e| e.to_string())?; }
    fs::write(&path, &content).map_err(|e| e.to_string())?;
    Ok(format!("Wrote {} bytes to {}", content.len(), name))
  }
}

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
    let mut req = reqwest::Client::new().request(
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

fn now() -> String { chrono::Local::now().format("%H:%M:%S").to_string() }

// ---------------------------------------------------------------------------
// DB
// ---------------------------------------------------------------------------

fn db(app: &AppHandle) -> Result<Connection, String> {
  let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
  fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
  let conn = Connection::open(dir.join("local-agent-os.sqlite3")).map_err(|e| e.to_string())?;
  conn.execute_batch("CREATE TABLE IF NOT EXISTS runs (id TEXT PRIMARY KEY, agent_id TEXT NOT NULL, started_at TEXT NOT NULL, status TEXT NOT NULL, model TEXT NOT NULL, input TEXT NOT NULL, output TEXT); CREATE TABLE IF NOT EXISTS memory (agent_id TEXT NOT NULL, key TEXT NOT NULL, value TEXT NOT NULL, updated_at TEXT NOT NULL, PRIMARY KEY(agent_id,key)); CREATE TABLE IF NOT EXISTS model_configs (id TEXT PRIMARY KEY, provider TEXT NOT NULL, label TEXT NOT NULL, model TEXT NOT NULL, host TEXT, api_key TEXT, enabled INTEGER NOT NULL DEFAULT 1); CREATE TABLE IF NOT EXISTS tools (id TEXT PRIMARY KEY, name TEXT NOT NULL, kind TEXT NOT NULL, integration_id TEXT NOT NULL, description TEXT, enabled INTEGER NOT NULL DEFAULT 1, config_json TEXT NOT NULL DEFAULT '{}'); CREATE TABLE IF NOT EXISTS agents (id TEXT PRIMARY KEY, name TEXT NOT NULL, objective TEXT NOT NULL, model TEXT NOT NULL, tool_ids TEXT NOT NULL DEFAULT '[]', integrations TEXT NOT NULL DEFAULT '[]', memory INTEGER NOT NULL DEFAULT 1, permissions TEXT NOT NULL DEFAULT '[]', home_path TEXT NOT NULL, color TEXT NOT NULL DEFAULT '#8b5cf6', x REAL NOT NULL DEFAULT 0, y REAL NOT NULL DEFAULT 0);") .map_err(|e| e.to_string())?;
  Ok(conn)
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

#[tauri::command]
fn initialize_storage(app: AppHandle) -> Result<(), String> { db(&app).map(|_| ()) }

#[tauri::command]
fn list_model_configs(app: AppHandle) -> Result<Vec<ModelConfigRecord>, String> {
  let conn = db(&app)?;
  let mut stmt = conn.prepare("SELECT id, provider, label, model, host, api_key, enabled FROM model_configs ORDER BY id").map_err(|e| e.to_string())?;
  let rows = stmt.query_map([], |row| Ok(ModelConfigRecord {
    id: row.get(0)?, provider: row.get(1)?, label: row.get(2)?, model: row.get(3)?, host: row.get(4)?, api_key: row.get(5)?, enabled: row.get::<_, i64>(6)? != 0,
  })).map_err(|e| e.to_string())?;
  let mut out = Vec::new();
  for row in rows { out.push(row.map_err(|e| e.to_string())?); }
  Ok(out)
}

#[tauri::command]
fn save_model_config(app: AppHandle, config: ModelConfigRecord) -> Result<(), String> {
  let conn = db(&app)?;
  conn.execute(
    "INSERT INTO model_configs (id, provider, label, model, host, api_key, enabled) VALUES (?1,?2,?3,?4,?5,?6,?7)
     ON CONFLICT(id) DO UPDATE SET provider=excluded.provider, label=excluded.label, model=excluded.model, host=excluded.host, api_key=excluded.api_key, enabled=excluded.enabled",
    params![config.id, config.provider, config.label, config.model, config.host, config.api_key, if config.enabled { 1 } else { 0 }],
  ).map_err(|e| e.to_string())?;
  Ok(())
}

#[tauri::command]
fn delete_model_config(app: AppHandle, id: String) -> Result<(), String> {
  let conn = db(&app)?;
  conn.execute("DELETE FROM model_configs WHERE id=?1", params![id]).map_err(|e| e.to_string())?;
  Ok(())
}

fn stored_api_key(conn: &Connection, model_id: &str) -> Result<Option<String>, String> {
  let mut stmt = conn.prepare("SELECT api_key FROM model_configs WHERE id=?1 AND api_key IS NOT NULL AND api_key != ''").map_err(|e| e.to_string())?;
  let mut rows = stmt.query_map(params![model_id], |row| row.get::<_, Option<String>>(0)).map_err(|e| e.to_string())?;
  match rows.next() {
    Some(Ok(Some(key))) => Ok(Some(key)),
    Some(Ok(None)) => Ok(None),
    Some(Err(e)) => Err(e.to_string()),
    None => Ok(None),
  }
}

#[tauri::command]
fn list_tools(app: AppHandle) -> Result<Vec<ToolRecord>, String> {
  let conn = db(&app)?;
  let mut stmt = conn.prepare("SELECT id, name, kind, integration_id, description, enabled, config_json FROM tools ORDER BY name").map_err(|e| e.to_string())?;
  let rows = stmt.query_map([], |row| {
    let config_json: String = row.get(6)?;
    Ok(ToolRecord {
      id: row.get(0)?, name: row.get(1)?, kind: row.get(2)?, integration_id: row.get(3)?,
      description: row.get(4)?, enabled: row.get::<_, i64>(5)? != 0,
      config: serde_json::from_str(&config_json).unwrap_or_else(|_| serde_json::json!({})),
    })
  }).map_err(|e| e.to_string())?;
  let mut out = Vec::new();
  for row in rows { out.push(row.map_err(|e| e.to_string())?); }
  Ok(out)
}

#[tauri::command]
fn save_tool(app: AppHandle, tool: ToolRecord) -> Result<(), String> {
  let conn = db(&app)?;
  let config_json = serde_json::to_string(&tool.config).unwrap_or_else(|_| "{}".into());
  conn.execute(
    "INSERT INTO tools (id, name, kind, integration_id, description, enabled, config_json) VALUES (?1,?2,?3,?4,?5,?6,?7)
     ON CONFLICT(id) DO UPDATE SET name=excluded.name, kind=excluded.kind, integration_id=excluded.integration_id, description=excluded.description, enabled=excluded.enabled, config_json=excluded.config_json",
    params![tool.id, tool.name, tool.kind, tool.integration_id, tool.description, if tool.enabled { 1 } else { 0 }, config_json],
  ).map_err(|e| e.to_string())?;
  Ok(())
}

#[tauri::command]
fn delete_tool(app: AppHandle, id: String) -> Result<(), String> {
  let conn = db(&app)?;
  conn.execute("DELETE FROM tools WHERE id=?1", params![id]).map_err(|e| e.to_string())?;
  Ok(())
}

fn parse_json_vec(s: &str) -> Vec<String> {
  serde_json::from_str(s).unwrap_or_default()
}

#[tauri::command]
fn list_agents(app: AppHandle) -> Result<Vec<AgentRecord>, String> {
  let conn = db(&app)?;
  let mut stmt = conn.prepare("SELECT id, name, objective, model, tool_ids, integrations, memory, permissions, home_path, color, x, y FROM agents ORDER BY name").map_err(|e| e.to_string())?;
  let rows = stmt.query_map([], |row| {
    let tool_ids: String = row.get(4)?;
    let integrations: String = row.get(5)?;
    let permissions: String = row.get(7)?;
    Ok(AgentRecord {
      id: row.get(0)?, name: row.get(1)?, objective: row.get(2)?, model: row.get(3)?,
      tool_ids: parse_json_vec(&tool_ids), integrations: parse_json_vec(&integrations),
      memory: row.get::<_, i64>(6)? != 0, permissions: parse_json_vec(&permissions),
      home_path: row.get(8)?, color: row.get(9)?, x: row.get(10)?, y: row.get(11)?,
    })
  }).map_err(|e| e.to_string())?;
  let mut out = Vec::new();
  for row in rows { out.push(row.map_err(|e| e.to_string())?); }
  Ok(out)
}

#[tauri::command]
fn save_agent(app: AppHandle, agent: AgentRecord) -> Result<(), String> {
  let conn = db(&app)?;
  conn.execute(
    "INSERT INTO agents (id, name, objective, model, tool_ids, integrations, memory, permissions, home_path, color, x, y) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)
     ON CONFLICT(id) DO UPDATE SET name=excluded.name, objective=excluded.objective, model=excluded.model, tool_ids=excluded.tool_ids, integrations=excluded.integrations, memory=excluded.memory, permissions=excluded.permissions, home_path=excluded.home_path, color=excluded.color, x=excluded.x, y=excluded.y",
    params![agent.id, agent.name, agent.objective, agent.model,
      serde_json::to_string(&agent.tool_ids).unwrap_or_else(|_| "[]".into()),
      serde_json::to_string(&agent.integrations).unwrap_or_else(|_| "[]".into()),
      if agent.memory { 1 } else { 0 },
      serde_json::to_string(&agent.permissions).unwrap_or_else(|_| "[]".into()),
      agent.home_path, agent.color, agent.x, agent.y],
  ).map_err(|e| e.to_string())?;
  Ok(())
}

#[tauri::command]
fn delete_agent(app: AppHandle, id: String) -> Result<(), String> {
  let conn = db(&app)?;
  conn.execute("DELETE FROM agents WHERE id=?1", params![id]).map_err(|e| e.to_string())?;
  Ok(())
}

// ---------------------------------------------------------------------------
// Agent execution
// ---------------------------------------------------------------------------

fn build_tools(conn: &Connection, agent: &AgentRequest, home: &std::path::Path) -> Result<Vec<Box<dyn AgentTool>>, String> {
  let mut tools: Vec<Box<dyn AgentTool>> = Vec::new();
  let has_network = agent.permissions.iter().any(|p| p == "network");
  let has_files = agent.permissions.iter().any(|p| p == "files");
  let mut stmt = conn.prepare("SELECT kind, enabled, config_json FROM tools WHERE id=?1").map_err(|e| e.to_string())?;
  for tool_id in &agent.tool_ids {
    let mut rows = stmt.query_map(params![tool_id], |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? != 0, row.get::<_, String>(2)?))).map_err(|e| e.to_string())?;
    let Some(Ok((kind, enabled, config_json))) = rows.next() else { continue }; // unknown or missing → skip
    if !enabled { continue; }
    let config: serde_json::Value = serde_json::from_str(&config_json).unwrap_or_else(|_| serde_json::json!({}));
    let str_cfg = |k: &str| config.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string();
    match kind.as_str() {
      "web_search" if has_network => tools.push(Box::new(WebSearchTool { base_url: str_cfg("baseUrl"), api_key: str_cfg("apiKey") })),
      "web_crawl" if has_network => tools.push(Box::new(WebCrawlTool { base_url: str_cfg("baseUrl"), api_key: str_cfg("apiKey") })),
      "http_get" if has_network => tools.push(Box::new(HttpTool)),
      "api" if has_network => {
        let params = config.get("params").and_then(|p| p.as_array()).map(|arr| arr.iter().filter_map(|p| {
          Some(ApiParam {
            name: p.get("name")?.as_str()?.to_string(),
            param_type: p.get("type").and_then(|t| t.as_str()).unwrap_or("string").to_string(),
            description: p.get("description").and_then(|d| d.as_str()).unwrap_or("").to_string(),
            required: p.get("required").and_then(|r| r.as_bool()).unwrap_or(false),
          })
        }).collect::<Vec<_>>()).unwrap_or_default();
        let headers = config.get("headers").and_then(|h| h.as_array()).map(|arr| arr.iter().filter_map(|h| {
          Some((h.get("name")?.as_str()?.to_string(), h.get("value")?.as_str()?.to_string()))
        }).collect::<Vec<_>>()).unwrap_or_default();
        let name = if str_cfg("name").is_empty() { format!("api_{}", tool_id) } else { str_cfg("name") };
        tools.push(Box::new(ApiTool {
          name,
          description: str_cfg("description"),
          method: str_cfg("method").to_uppercase(),
          url_template: str_cfg("url"),
          headers,
          body_template: if str_cfg("body").is_empty() { None } else { Some(str_cfg("body")) },
          params,
        }));
      }
      "read_file" if has_files => tools.push(Box::new(ReadFileTool { home: home.to_path_buf() })),
      "write_file" if has_files => tools.push(Box::new(WriteFileTool { home: home.to_path_buf() })),
      _ => {}
    }
  }
  Ok(tools)
}

fn tool_schemas(tools: &[Box<dyn AgentTool>]) -> Vec<serde_json::Value> {
  tools.iter().map(|t| serde_json::json!({ "type": "function", "function": { "name": t.name(), "description": t.description(), "parameters": t.params_schema() } })).collect()
}

// Returns (final_text, events, used_tools).
async fn run_ollama_chat(model: &str, prompt: &str, agent: &AgentRequest, tools: &[Box<dyn AgentTool>]) -> Result<(String, Vec<ExecutionEvent>, bool), String> {
  let mut events = Vec::new();
  let mut messages = vec![
    ChatMessage { role: "system".into(), content: Some(format!("You are {}. Objective: {}\n\nUse tools when you need current or external information. Call a tool, wait for its result, then continue. Cite URLs you use.", agent.name, agent.objective)), tool_calls: None, tool_call_id: None },
    ChatMessage { role: "user".into(), content: Some(prompt.into()), tool_calls: None, tool_call_id: None },
  ];
  let client = reqwest::Client::new();
  let mut used_tools = false;
  for _round in 0..MAX_TOOL_ROUNDS {
    let body = ChatRequest { model: model.into(), messages: messages.clone(), tools: Some(tool_schemas(tools)), stream: false };
    let response = client.post("http://127.0.0.1:11434/api/chat").json(&body).send().await.map_err(|e| format!("Could not reach Ollama. Start it at http://127.0.0.1:11434 ({e})"))?;
    if !response.status().is_success() { return Err(format!("Ollama returned {}", response.status())); }
    let parsed: ChatResponse = response.json().await.map_err(|e| format!("Could not parse Ollama tool response: {e}"))?;
    let reply = parsed.message;
    if let Some(calls) = &reply.tool_calls {
      used_tools = true;
      for call in calls {
        let name = call.function.name.clone();
        let args = call.function.arguments.clone();
        let tool = tools.iter().find(|t| t.name() == name);
        let result = match tool {
          Some(t) => t.run(&args).await,
          None => Err(format!("Unknown tool {name}.")),
        };
        let detail = match name.as_str() {
          "web_search" => args.get("query").and_then(|q| q.as_str()).map(|q| format!("query: {q}")).unwrap_or_default(),
          "web_crawl" => args.get("url").and_then(|u| u.as_str()).map(|u| format!("url: {u}")).unwrap_or_default(),
          _ => String::new(),
        };
        let (title, result_str) = match &result {
          Ok(text) => (format!("Called {name}"), format!("{text}")),
          Err(e) => (format!("Tool {name} failed"), format!("Error: {e}")),
        };
        events.push(ExecutionEvent { time: now(), kind: "tool".into(), title, detail: if detail.is_empty() { None } else { Some(detail) } });
        messages.push(ChatMessage { role: "assistant".into(), content: None, tool_calls: Some(vec![call.clone()]), tool_call_id: None });
        messages.push(ChatMessage { role: "tool".into(), content: Some(result_str), tool_calls: None, tool_call_id: Some(call.id.clone()) });
      }
    } else {
      let content = reply.content.unwrap_or_default();
      return Ok((content, events, used_tools));
    }
  }
  Err("Tool loop exceeded the maximum number of rounds.".into())
}

#[tauri::command]
async fn execute_agent(app: AppHandle, agent: AgentRequest, input: String, api_key: Option<String>) -> Result<Execution, String> {
  let started = chrono::Utc::now().to_rfc3339();
  let run_id = format!("{}-{}", agent.id, chrono::Utc::now().timestamp_millis());
  let conn = db(&app)?;
  conn.execute("INSERT INTO runs (id,agent_id,started_at,status,model,input) VALUES (?1,?2,?3,'running',?4,?5)", params![run_id, agent.id, started, agent.model, input]).map_err(|e| e.to_string())?;
  let mut events = vec![ExecutionEvent { time: now(), kind: "thought".into(), title: "Loaded isolated agent context".into(), detail: Some(format!("Home: {} · tools: {}", agent.home_path, agent.tool_ids.join(", "))) }];
  let home = app.path().app_data_dir().map_err(|e| e.to_string())?.join("agents").join(&agent.id);
  for folder in ["files", "memory", "runs", "outputs"] { fs::create_dir_all(home.join(folder)).map_err(|e| e.to_string())?; }
  fs::write(home.join("config.json"), serde_json::to_string_pretty(&serde_json::json!({"id":agent.id,"name":agent.name,"objective":agent.objective,"model":agent.model,"tools":agent.tool_ids})).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
  let prompt = format!("You are {}. Objective: {}\n\nTask: {}\n\nReturn a concise structured result in JSON with keys summary and result.", agent.name, agent.objective, input);
  let result = if let Some(model) = agent.model.strip_prefix("ollama:") {
    let tools = build_tools(&conn, &agent, &home)?;
    if !tools.is_empty() {
      events.push(ExecutionEvent { time: now(), kind: "thought".into(), title: "Asking Ollama".into(), detail: Some(format!("{model} with {} tool(s)", tools.len())) });
      run_ollama_chat(model, &prompt, &agent, &tools).await
    } else {
      events.push(ExecutionEvent { time: now(), kind: "thought".into(), title: "Asking Ollama".into(), detail: Some(model.into()) });
      let response = reqwest::Client::new().post("http://127.0.0.1:11434/api/generate").json(&serde_json::json!({"model":model,"prompt":prompt,"stream":false})).send().await.map_err(|e| format!("Could not reach Ollama. Start it at http://127.0.0.1:11434 ({e})"))?;
      if !response.status().is_success() { return Err(format!("Ollama returned {}", response.status())); }
      let output = response.json::<serde_json::Value>().await.map_err(|e| e.to_string())?["response"].as_str().unwrap_or("No response from Ollama.").to_string();
      events.push(ExecutionEvent { time: now(), kind: "result".into(), title: "Generated final result".into(), detail: None });
      Ok((output, Vec::new(), false))
    }
  } else if let Some(model) = agent.model.strip_prefix("openrouter:") {
    let key = api_key
      .filter(|key| !key.trim().is_empty())
      .or(stored_api_key(&conn, &agent.model).ok().flatten())
      .ok_or("OpenRouter requires an API key. Add it in Models first.")?;
    events.push(ExecutionEvent { time: now(), kind: "thought".into(), title: "Asking OpenRouter".into(), detail: Some(model.into()) });
    let response = reqwest::Client::new().post("https://openrouter.ai/api/v1/chat/completions")
      .header("Authorization", format!("Bearer {key}"))
      .header("HTTP-Referer", "https://local-agent-os.app")
      .header("X-Title", "Local Agent OS")
      .json(&serde_json::json!({"model":model,"messages":[{"role":"user","content":prompt}]})).send().await.map_err(|e| format!("Could not reach OpenRouter: {e}"))?;
    if !response.status().is_success() { return Err(format!("OpenRouter returned {}", response.status())); }
    let output = response.json::<serde_json::Value>().await.map_err(|e| e.to_string())?["choices"][0]["message"]["content"].as_str().unwrap_or("OpenRouter returned no text.").to_string();
    events.push(ExecutionEvent { time: now(), kind: "result".into(), title: "Generated final result".into(), detail: None });
    Ok((output, Vec::new(), false))
  } else { return Err("Unknown model provider. Select Ollama or OpenRouter in the agent Model tab.".into()); };
  let output = match result {
    Ok((text, mut tool_events, _used_tools)) => {
      events.append(&mut tool_events);
      text
    }
    Err(e) => {
      let _ = conn.execute("UPDATE runs SET status='failed' WHERE id=?1", params![run_id]);
      return Err(e);
    }
  };
  fs::write(home.join("outputs").join(format!("{run_id}.txt")), &output).map_err(|e| e.to_string())?;
  conn.execute("UPDATE runs SET status='completed', output=?1 WHERE id=?2", params![output, run_id]).map_err(|e| e.to_string())?;
  Ok(Execution { output, events })
}

fn main() {
  tauri::Builder::default()
    .plugin(tauri_plugin_opener::init())
    .invoke_handler(tauri::generate_handler![initialize_storage, list_model_configs, save_model_config, delete_model_config, list_tools, save_tool, delete_tool, list_agents, save_agent, delete_agent, execute_agent])
    .run(tauri::generate_context!())
    .expect("error while running Local Agent OS");
}
