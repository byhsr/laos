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
struct Execution { output: String, events: Vec<ExecutionEvent>, run_id: String, prompt_tokens: u64, completion_tokens: u64 }
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

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct WorkflowRecord { id: String, name: String, nodes: serde_json::Value, edges: serde_json::Value, updated_at: String }

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WorkflowStep { node_id: String, node_label: String, output: String, prompt_tokens: u64, completion_tokens: u64 }

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WorkflowExecution { steps: Vec<WorkflowStep>, final_output: String, total_prompt_tokens: u64, total_completion_tokens: u64 }

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
struct ChatResponse { message: ChatMessage, #[serde(default)] prompt_eval_count: u64, #[serde(default)] eval_count: u64 }

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
  conn.execute_batch("CREATE TABLE IF NOT EXISTS runs (id TEXT PRIMARY KEY, agent_id TEXT NOT NULL, started_at TEXT NOT NULL, status TEXT NOT NULL, model TEXT NOT NULL, input TEXT NOT NULL, output TEXT, prompt_tokens INTEGER NOT NULL DEFAULT 0, completion_tokens INTEGER NOT NULL DEFAULT 0); CREATE TABLE IF NOT EXISTS memory (agent_id TEXT NOT NULL, key TEXT NOT NULL, value TEXT NOT NULL, updated_at TEXT NOT NULL, PRIMARY KEY(agent_id,key)); CREATE TABLE IF NOT EXISTS model_configs (id TEXT PRIMARY KEY, provider TEXT NOT NULL, label TEXT NOT NULL, model TEXT NOT NULL, host TEXT, api_key TEXT, enabled INTEGER NOT NULL DEFAULT 1); CREATE TABLE IF NOT EXISTS tools (id TEXT PRIMARY KEY, name TEXT NOT NULL, kind TEXT NOT NULL, integration_id TEXT NOT NULL, description TEXT, enabled INTEGER NOT NULL DEFAULT 1, config_json TEXT NOT NULL DEFAULT '{}'); CREATE TABLE IF NOT EXISTS agents (id TEXT PRIMARY KEY, name TEXT NOT NULL, objective TEXT NOT NULL, model TEXT NOT NULL, tool_ids TEXT NOT NULL DEFAULT '[]', integrations TEXT NOT NULL DEFAULT '[]', memory INTEGER NOT NULL DEFAULT 1, permissions TEXT NOT NULL DEFAULT '[]', home_path TEXT NOT NULL, color TEXT NOT NULL DEFAULT '#8b5cf6', x REAL NOT NULL DEFAULT 0, y REAL NOT NULL DEFAULT 0); CREATE TABLE IF NOT EXISTS workflows (id TEXT PRIMARY KEY, name TEXT NOT NULL, nodes TEXT NOT NULL DEFAULT '[]', edges TEXT NOT NULL DEFAULT '[]', updated_at TEXT NOT NULL);") .map_err(|e| e.to_string())?;
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
// Workflows
// ---------------------------------------------------------------------------

#[tauri::command]
fn list_workflows(app: AppHandle) -> Result<Vec<WorkflowRecord>, String> {
  let conn = db(&app)?;
  let mut stmt = conn.prepare("SELECT id, name, nodes, edges, updated_at FROM workflows ORDER BY updated_at DESC").map_err(|e| e.to_string())?;
  let rows = stmt.query_map([], |row| {
    let nodes: String = row.get(2)?;
    let edges: String = row.get(3)?;
    Ok(WorkflowRecord {
      id: row.get(0)?, name: row.get(1)?,
      nodes: serde_json::from_str(&nodes).unwrap_or_else(|_| serde_json::json!([])),
      edges: serde_json::from_str(&edges).unwrap_or_else(|_| serde_json::json!([])),
      updated_at: row.get(4)?,
    })
  }).map_err(|e| e.to_string())?;
  let mut out = Vec::new();
  for row in rows { out.push(row.map_err(|e| e.to_string())?); }
  Ok(out)
}

#[tauri::command]
fn save_workflow(app: AppHandle, workflow: WorkflowRecord) -> Result<(), String> {
  let conn = db(&app)?;
  conn.execute(
    "INSERT INTO workflows (id, name, nodes, edges, updated_at) VALUES (?1,?2,?3,?4,?5)
     ON CONFLICT(id) DO UPDATE SET name=excluded.name, nodes=excluded.nodes, edges=excluded.edges, updated_at=excluded.updated_at",
    params![
      workflow.id, workflow.name,
      serde_json::to_string(&workflow.nodes).map_err(|e| e.to_string())?,
      serde_json::to_string(&workflow.edges).map_err(|e| e.to_string())?,
      workflow.updated_at,
    ],
  ).map_err(|e| e.to_string())?;
  Ok(())
}

#[tauri::command]
fn delete_workflow(app: AppHandle, id: String) -> Result<(), String> {
  let conn = db(&app)?;
  conn.execute("DELETE FROM workflows WHERE id=?1", params![id]).map_err(|e| e.to_string())?;
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

// Returns (final_text, events, used_tools, prompt_tokens, completion_tokens).
async fn run_ollama_chat(model: &str, prompt: &str, agent: &AgentRequest, tools: &[Box<dyn AgentTool>]) -> Result<(String, Vec<ExecutionEvent>, bool, u64, u64), String> {
  let mut events = Vec::new();
  let mut messages = vec![
    ChatMessage { role: "system".into(), content: Some(format!("You are {}. Objective: {}\n\nUse tools when you need current or external information. Call a tool, wait for its result, then continue. Cite URLs you use.", agent.name, agent.objective)), tool_calls: None, tool_call_id: None },
    ChatMessage { role: "user".into(), content: Some(prompt.into()), tool_calls: None, tool_call_id: None },
  ];
  let client = reqwest::Client::new();
  let mut used_tools = false;
  let mut prompt_tokens = 0u64;
  let mut completion_tokens = 0u64;
  for _round in 0..MAX_TOOL_ROUNDS {
    let body = ChatRequest { model: model.into(), messages: messages.clone(), tools: Some(tool_schemas(tools)), stream: false };
    let response = client.post("http://127.0.0.1:11434/api/chat").json(&body).send().await.map_err(|e| format!("Could not reach Ollama. Start it at http://127.0.0.1:11434 ({e})"))?;
    if !response.status().is_success() { return Err(format!("Ollama returned {}", response.status())); }
    let parsed: ChatResponse = response.json().await.map_err(|e| format!("Could not parse Ollama tool response: {e}"))?;
    prompt_tokens += parsed.prompt_eval_count;
    completion_tokens += parsed.eval_count;
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
      return Ok((content, events, used_tools, prompt_tokens, completion_tokens));
    }
  }
  Err("Tool loop exceeded the maximum number of rounds.".into())
}

// Runs a single agent against its provider. Returns (output, prompt_tokens, completion_tokens).
async fn run_agent_once(app: &AppHandle, conn: &Connection, agent: &AgentRequest, input: &str, api_key: Option<&str>, events: &mut Vec<ExecutionEvent>) -> Result<(String, u64, u64), String> {
  let home = app.path().app_data_dir().map_err(|e| e.to_string())?.join("agents").join(&agent.id);
  for folder in ["files", "memory", "runs", "outputs"] { fs::create_dir_all(home.join(folder)).map_err(|e| e.to_string())?; }
  fs::write(home.join("config.json"), serde_json::to_string_pretty(&serde_json::json!({"id":agent.id,"name":agent.name,"objective":agent.objective,"model":agent.model,"tools":agent.tool_ids})).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
  let prompt = format!("You are {}. Objective: {}\n\nTask: {}\n\nReturn a concise structured result in JSON with keys summary and result.", agent.name, agent.objective, input);
  if let Some(model) = agent.model.strip_prefix("ollama:") {
    let tools = build_tools(conn, agent, &home)?;
    if !tools.is_empty() {
      events.push(ExecutionEvent { time: now(), kind: "thought".into(), title: "Asking Ollama".into(), detail: Some(format!("{model} with {} tool(s)", tools.len())) });
      let (output, tool_events, _, pt, ct) = run_ollama_chat(model, &prompt, agent, &tools).await?;
      events.extend(tool_events);
      Ok((output, pt, ct))
    } else {
      events.push(ExecutionEvent { time: now(), kind: "thought".into(), title: "Asking Ollama".into(), detail: Some(model.into()) });
      let response = reqwest::Client::new().post("http://127.0.0.1:11434/api/generate").json(&serde_json::json!({"model":model,"prompt":prompt,"stream":false})).send().await.map_err(|e| format!("Could not reach Ollama. Start it at http://127.0.0.1:11434 ({e})"))?;
      if !response.status().is_success() { return Err(format!("Ollama returned {}", response.status())); }
      let json: serde_json::Value = response.json().await.map_err(|e| e.to_string())?;
      let output = json["response"].as_str().unwrap_or("No response from Ollama.").to_string();
      let pt = json["prompt_eval_count"].as_u64().unwrap_or(0);
      let ct = json["eval_count"].as_u64().unwrap_or(0);
      events.push(ExecutionEvent { time: now(), kind: "result".into(), title: "Generated final result".into(), detail: None });
      Ok((output, pt, ct))
    }
  } else if let Some(model) = agent.model.strip_prefix("openrouter:") {
    let key = api_key
      .filter(|k| !k.trim().is_empty())
      .map(|k| k.to_string())
      .or(stored_api_key(conn, &agent.model).ok().flatten())
      .ok_or("OpenRouter requires an API key. Add it in Models first.")?;
    events.push(ExecutionEvent { time: now(), kind: "thought".into(), title: "Asking OpenRouter".into(), detail: Some(model.into()) });
    let response = reqwest::Client::new().post("https://openrouter.ai/api/v1/chat/completions")
      .header("Authorization", format!("Bearer {key}"))
      .header("HTTP-Referer", "https://local-agent-os.app")
      .header("X-Title", "Local Agent OS")
      .json(&serde_json::json!({"model":model,"messages":[{"role":"user","content":prompt}]})).send().await.map_err(|e| format!("Could not reach OpenRouter: {e}"))?;
    if !response.status().is_success() { return Err(format!("OpenRouter returned {}", response.status())); }
    let json: serde_json::Value = response.json().await.map_err(|e| e.to_string())?;
    let output = json["choices"][0]["message"]["content"].as_str().unwrap_or("OpenRouter returned no text.").to_string();
    let pt = json["usage"]["prompt_tokens"].as_u64().unwrap_or(0);
    let ct = json["usage"]["completion_tokens"].as_u64().unwrap_or(0);
    events.push(ExecutionEvent { time: now(), kind: "result".into(), title: "Generated final result".into(), detail: None });
    Ok((output, pt, ct))
  } else { Err("Unknown model provider. Select Ollama or OpenRouter in the agent Model tab.".into()) }
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
  let (output, prompt_tokens, completion_tokens) = match run_agent_once(&app, &conn, &agent, &input, api_key.as_deref(), &mut events).await {
    Ok(v) => v,
    Err(e) => {
      let _ = conn.execute("UPDATE runs SET status='failed' WHERE id=?1", params![run_id]);
      return Err(e);
    }
  };
  fs::write(home.join("outputs").join(format!("{run_id}.txt")), &output).map_err(|e| e.to_string())?;
  conn.execute("UPDATE runs SET status='completed', output=?1, prompt_tokens=?2, completion_tokens=?3 WHERE id=?4", params![output, prompt_tokens, completion_tokens, run_id]).map_err(|e| e.to_string())?;
  Ok(Execution { output, events, run_id, prompt_tokens, completion_tokens })
}

// Executes a workflow linearly: orders nodes by edges (BFS from start nodes), runs each
// agent/subagent/checker node with the previous node's output as input. Loop/checker/gate
// config is stored but phase 1 passes input through.
#[tauri::command]
async fn execute_workflow(app: AppHandle, workflow: WorkflowRecord, input: String, api_key: Option<String>) -> Result<WorkflowExecution, String> {
  let conn = db(&app)?;
  let nodes: Vec<serde_json::Value> = workflow.nodes.as_array().cloned().unwrap_or_default();
  let edges: Vec<serde_json::Value> = workflow.edges.as_array().cloned().unwrap_or_default();

  // Build node map + adjacency from edges.
  let mut node_map: std::collections::HashMap<String, serde_json::Value> = std::collections::HashMap::new();
  let mut start_ids: Vec<String> = Vec::new();
  let mut incoming: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
  let mut outgoing: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();
  for n in &nodes {
    let id = n.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
    if id.is_empty() { continue; }
    node_map.insert(id.clone(), n.clone());
    incoming.entry(id.clone()).or_insert(0);
  }
  for e in &edges {
    let from = e.get("from").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let to = e.get("to").and_then(|v| v.as_str()).unwrap_or("").to_string();
    if from.is_empty() || to.is_empty() { continue; }
    if !node_map.contains_key(&from) || !node_map.contains_key(&to) { continue; }
    *incoming.entry(to.clone()).or_insert(0) += 1;
    outgoing.entry(from.clone()).or_default().push(to);
  }
  for (id, deg) in &incoming {
    if *deg == 0 { start_ids.push(id.clone()); }
  }
  if start_ids.is_empty() && !nodes.is_empty() {
    start_ids = nodes.iter().filter_map(|n| n.get("id").and_then(|v| v.as_str()).map(|s| s.to_string())).collect();
  }

  // BFS topological order.
  let mut order: Vec<String> = Vec::new();
  let mut queue: Vec<String> = start_ids;
  while let Some(id) = queue.pop() {
    order.push(id.clone());
    if let Some(children) = outgoing.get(&id) {
      for c in children {
        let deg = incoming.entry(c.clone()).or_insert(0);
        *deg = deg.saturating_sub(1);
        if *deg == 0 { queue.push(c.clone()); }
      }
    }
  }

  let mut steps: Vec<WorkflowStep> = Vec::new();
  let mut current_input = input;
  let mut total_prompt = 0u64;
  let mut total_completion = 0u64;

  for node_id in order {
    let Some(node) = node_map.get(&node_id) else { continue };
    let ntype = node.get("type").and_then(|v| v.as_str()).unwrap_or("agent").to_string();
    let label = node.get("label").and_then(|v| v.as_str()).unwrap_or(&node_id).to_string();
    let mut events = Vec::new();
    let output = match ntype.as_str() {
      "agent" | "subagent" | "checker" => {
        let agent_id = node.get("agentId").and_then(|v| v.as_str()).unwrap_or("").to_string();
        if agent_id.is_empty() {
          current_input.clone()
        } else {
          let mut stmt = conn.prepare("SELECT id, name, objective, model, tool_ids, home_path, permissions FROM agents WHERE id=?1").map_err(|e| e.to_string())?;
          let mut rows = stmt.query_map(params![agent_id], |row| {
            let tool_ids: String = row.get(4)?;
            let permissions: String = row.get(6)?;
            Ok(AgentRequest {
              id: row.get(0)?, name: row.get(1)?, objective: row.get(2)?, model: row.get(3)?,
              tool_ids: parse_json_vec(&tool_ids), home_path: row.get(5)?, permissions: parse_json_vec(&permissions),
            })
          }).map_err(|e| e.to_string())?;
          let Some(agent) = rows.next().transpose().map_err(|e| e.to_string())? else {
            current_input.clone()
          };
          let (out, pt, ct) = run_agent_once(&app, &conn, &agent, &current_input, api_key.as_deref(), &mut events).await?;
          total_prompt += pt; total_completion += ct;
          out
        }
      }
      _ => current_input.clone(), // trigger/loop/integration/gate: pass through in phase 1
    };
    steps.push(WorkflowStep { node_id: node_id.clone(), node_label: label, output: output.clone(), prompt_tokens: 0, completion_tokens: 0 });
    current_input = output;
  }

  let final_output = current_input;
  Ok(WorkflowExecution { steps, final_output, total_prompt_tokens: total_prompt, total_completion_tokens: total_completion })
}

fn main() {
  tauri::Builder::default()
    .plugin(tauri_plugin_opener::init())
    .invoke_handler(tauri::generate_handler![initialize_storage, list_model_configs, save_model_config, delete_model_config, list_tools, save_tool, delete_tool, list_agents, save_agent, delete_agent, list_workflows, save_workflow, delete_workflow, execute_agent, execute_workflow])
    .run(tauri::generate_context!())
    .expect("error while running Local Agent OS");
}
