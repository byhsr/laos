#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use async_trait::async_trait;
use futures_util::StreamExt;
use rusqlite::{params, Connection};
use std::fs;
use tauri::{AppHandle, Manager};

mod db;
mod models;
use db::{db, now};
use models::*;

// ---------------------------------------------------------------------------
// Web tools (configurable: base URL + API key come from the tool's config)
// ---------------------------------------------------------------------------

const MAX_TOOL_CHARS: usize = 8000;
const MAX_TOOL_ROUNDS: usize = 5;

fn clip(s: &str) -> String {
  let chars: Vec<char> = s.chars().collect();
  if chars.len() <= MAX_TOOL_CHARS { s.to_string() }
  else { chars[..MAX_TOOL_CHARS].iter().collect::<String>() + "\nâ€¦[truncated]" }
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

// Host-filesystem tools (gated behind the agent's `host_fs` permission).
// These deliberately escape the per-agent sandbox â€” treat as powerful.

struct SearchFilesTool;   // recursive filename search from a root path
struct ReadAnyFileTool;   // read any file by absolute path (size-capped)
struct RunCommandTool;    // execute a shell command, return stdout+stderr

#[async_trait]
impl AgentTool for SearchFilesTool {
  fn name(&self) -> String { "search_files".into() }
  fn description(&self) -> String { "Recursively search a directory for files whose name contains a substring. Params: pattern (string, e.g. 'resume'), startPath (string, optional, e.g. 'A:/'), maxResults (integer, optional, default 20).".into() }
  fn params_schema(&self) -> serde_json::Value {
    serde_json::json!({ "type": "object", "properties": { "pattern": { "type": "string" }, "startPath": { "type": "string" }, "maxResults": { "type": "integer" } }, "required": ["pattern"] })
  }
  async fn run(&self, args: &serde_json::Value) -> Result<String, String> {
    let pattern = str_arg(args, "pattern").ok_or("search_files requires a 'pattern' argument.")?;
    let start = str_arg(args, "startPath").unwrap_or_else(|| "A:/".into());
    let max = args.get("maxResults").and_then(|v| v.as_u64()).unwrap_or(20) as usize;
    let root = std::path::Path::new(&start);
    if !root.exists() { return Ok(format!("Path '{start}' does not exist.")); }
    let lower = pattern.to_lowercase();
    let mut found: Vec<String> = Vec::new();
    let mut dirs = vec![root.to_path_buf()];
    let mut visited: std::collections::HashSet<std::path::PathBuf> = std::collections::HashSet::new();
    while let Some(dir) = dirs.pop() {
      if !visited.insert(dir.clone()) { continue; }
      let Ok(entries) = fs::read_dir(&dir) else { continue };
      for entry in entries.flatten() {
        let path = entry.path();
        let Ok(meta) = entry.metadata() else { continue };
        if meta.is_dir() {
          dirs.push(path);
        } else if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
          if name.to_lowercase().contains(&lower) {
            found.push(path.to_string_lossy().to_string());
            if found.len() >= max { return Ok(found.join("\n")); }
          }
        }
      }
    }
    Ok(if found.is_empty() { "No files found matching that name." .to_string() } else { found.join("\n") })
  }
}

#[async_trait]
impl AgentTool for ReadAnyFileTool {
  fn name(&self) -> String { "read_file_any".into() }
  fn description(&self) -> String { "Read a file by absolute path (anywhere on the device). Params: path (string), maxBytes (integer, optional, default 20000).".into() }
  fn params_schema(&self) -> serde_json::Value {
    serde_json::json!({ "type": "object", "properties": { "path": { "type": "string" }, "maxBytes": { "type": "integer" } }, "required": ["path"] })
  }
  async fn run(&self, args: &serde_json::Value) -> Result<String, String> {
    let path = str_arg(args, "path").ok_or("read_file_any requires a 'path' argument.")?;
    let max = args.get("maxBytes").and_then(|v| v.as_u64()).unwrap_or(20000) as usize;
    let p = std::path::Path::new(&path);
    if !p.exists() { return Ok(format!("File '{path}' does not exist.")); }
    let bytes = fs::read(p).map_err(|e| e.to_string())?;
    if bytes.len() > max { return Ok(format!("File is {} bytes (over the {} byte cap). Showing the first {}:\n{}", bytes.len(), max, max, String::from_utf8_lossy(&bytes[..max]))); }
    Ok(String::from_utf8_lossy(&bytes).to_string())
  }
}

#[async_trait]
impl AgentTool for RunCommandTool {
  fn name(&self) -> String { "run_command".into() }
  fn description(&self) -> String { "Run a shell command on the host machine and return stdout + stderr. Params: command (string). Requires explicit user approval â€” the user confirms before it executes.".into() }
  fn params_schema(&self) -> serde_json::Value {
    serde_json::json!({ "type": "object", "properties": { "command": { "type": "string" } }, "required": ["command"] })
  }
  async fn run(&self, args: &serde_json::Value) -> Result<String, String> {
    let command = str_arg(args, "command").ok_or("run_command requires a 'command' argument.")?;
    // Runs through the system shell; on Windows this is cmd /C.
    #[cfg(windows)] let output = std::process::Command::new("cmd").args(["/C", &command]).output();
    #[cfg(not(windows))] let output = std::process::Command::new("sh").args(["-c", &command]).output();
    let output = output.map_err(|e| format!("Failed to run command: {e}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let status = output.status;
    let mut out = String::new();
    if !status.success() { out.push_str(&format!("Exit code: {}\n", status.code().unwrap_or(-1))); }
    out.push_str(&stdout);
    if !stderr.is_empty() { out.push_str(&format!("\n[stderr]\n{stderr}")); }
    Ok(clip(&out))
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

// Generic integration tool: executes a provider REST action using the stored
// integration credentials. Data-driven â€” new actions are added to the provider match.
struct IntegrationTool {
  action: String,
  credentials: serde_json::Value,
}

#[async_trait]
impl AgentTool for IntegrationTool {
  fn name(&self) -> String { self.action.clone() }
  fn description(&self) -> String {
    match self.action.as_str() {
      "notion_search" => "Search Notion pages and databases. Params: query (string).".into(),
      "notion_create_page" => "Create a Notion page. Params: parentId (string, database or page id), title (string).".into(),
      "notion_get_page" => "Fetch a Notion page's content. Params: pageId (string).".into(),
      "airtable_list_records" => "List records from an Airtable table. Params: baseId (string), tableName (string), maxRecords (integer, optional).".into(),
      "airtable_create_record" => "Create a record in an Airtable table. Params: baseId (string), tableName (string), fields (object of field values).".into(),
      "airtable_update_record" => "Update a record in an Airtable table. Params: baseId (string), tableName (string), recordId (string), fields (object).".into(),
      "sheets_read" => "Read rows from a Google Sheet. Params: spreadsheetId (string), range (string, e.g. 'Sheet1!A1:C10').".into(),
      "sheets_append" => "Append rows to a Google Sheet. Params: spreadsheetId (string), range (string), values (array of arrays).".into(),
      "sheets_update" => "Update cells in a Google Sheet. Params: spreadsheetId (string), range (string), values (array of arrays).".into(),
      "docs_create" => "Create a Google Doc with content. Params: title (string), content (string).".into(),
      "docs_get" => "Read a Google Doc's content. Params: documentId (string).".into(),
      _ => "Integration action".into(),
    }
  }
  fn params_schema(&self) -> serde_json::Value {
    serde_json::json!({ "type": "object", "properties": { "query": { "type": "string" }, "parentId": { "type": "string" }, "title": { "type": "string" }, "pageId": { "type": "string" }, "baseId": { "type": "string" }, "tableName": { "type": "string" }, "maxRecords": { "type": "integer" }, "fields": { "type": "object" }, "recordId": { "type": "string" }, "spreadsheetId": { "type": "string" }, "range": { "type": "string" }, "values": { "type": "array" }, "documentId": { "type": "string" }, "content": { "type": "string" } }, "required": [] })
  }
  async fn run(&self, args: &serde_json::Value) -> Result<String, String> {
    let token = self.credentials.get("token").and_then(|t| t.as_str()).or_else(|| self.credentials.get("apiKey").and_then(|t| t.as_str())).or_else(|| self.credentials.get("accessToken").and_then(|t| t.as_str())).unwrap_or("");
    let client = reqwest::Client::new();
    let arg = |k: &str| args.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string();

    match self.action.as_str() {
      "notion_search" => {
        let body = serde_json::json!({ "query": arg("query") });
        let resp = client.post("https://api.notion.com/v1/search")
          .header("Authorization", format!("Bearer {token}"))
          .header("Notion-Version", "2022-06-28")
          .json(&body).send().await.map_err(|e| e.to_string())?;
        let text = resp.text().await.map_err(|e| e.to_string())?;
        Ok(clip(&text))
      }
      "notion_create_page" => {
        let body = serde_json::json!({
          "parent": { "type": "page_id", "page_id": arg("parentId") },
          "properties": { "title": { "title": [{ "text": { "content": arg("title") } }] } }
        });
        let resp = client.post("https://api.notion.com/v1/pages")
          .header("Authorization", format!("Bearer {token}"))
          .header("Notion-Version", "2022-06-28")
          .json(&body).send().await.map_err(|e| e.to_string())?;
        let text = resp.text().await.map_err(|e| e.to_string())?;
        Ok(clip(&text))
      }
      "notion_get_page" => {
        let resp = client.get(format!("https://api.notion.com/v1/pages/{}", arg("pageId")))
          .header("Authorization", format!("Bearer {token}"))
          .header("Notion-Version", "2022-06-28")
          .send().await.map_err(|e| e.to_string())?;
        let text = resp.text().await.map_err(|e| e.to_string())?;
        Ok(clip(&text))
      }
      "airtable_list_records" => {
        let resp = client.get(format!("https://api.airtable.com/v0/{}/{}/listRecords", arg("baseId"), arg("tableName")))
          .header("Authorization", format!("Bearer {token}"))
          .send().await.map_err(|e| e.to_string())?;
        let text = resp.text().await.map_err(|e| e.to_string())?;
        Ok(clip(&text))
      }
      "airtable_create_record" => {
        let body = serde_json::json!({ "fields": args.get("fields").cloned().unwrap_or(serde_json::json!({})) });
        let resp = client.post(format!("https://api.airtable.com/v0/{}/{}", arg("baseId"), arg("tableName")))
          .header("Authorization", format!("Bearer {token}"))
          .json(&body).send().await.map_err(|e| e.to_string())?;
        let text = resp.text().await.map_err(|e| e.to_string())?;
        Ok(clip(&text))
      }
      "airtable_update_record" => {
        let body = serde_json::json!({ "fields": args.get("fields").cloned().unwrap_or(serde_json::json!({})) });
        let resp = client.patch(format!("https://api.airtable.com/v0/{}/{}/{}", arg("baseId"), arg("tableName"), arg("recordId")))
          .header("Authorization", format!("Bearer {token}"))
          .json(&body).send().await.map_err(|e| e.to_string())?;
        let text = resp.text().await.map_err(|e| e.to_string())?;
        Ok(clip(&text))
      }
      "sheets_read" => {
        let resp = client.get(format!("https://sheets.googleapis.com/v4/spreadsheets/{}/values/{}", arg("spreadsheetId"), arg("range")))
          .header("Authorization", format!("Bearer {token}"))
          .send().await.map_err(|e| e.to_string())?;
        let text = resp.text().await.map_err(|e| e.to_string())?;
        Ok(clip(&text))
      }
      "sheets_append" => {
        let body = serde_json::json!({ "values": args.get("values").cloned().unwrap_or(serde_json::json!([])) });
        let resp = client.post(format!("https://sheets.googleapis.com/v4/spreadsheets/{}/values/{}:append?valueInputOption=RAW", arg("spreadsheetId"), arg("range")))
          .header("Authorization", format!("Bearer {token}"))
          .json(&body).send().await.map_err(|e| e.to_string())?;
        let text = resp.text().await.map_err(|e| e.to_string())?;
        Ok(clip(&text))
      }
      "sheets_update" => {
        let body = serde_json::json!({ "values": args.get("values").cloned().unwrap_or(serde_json::json!([])) });
        let resp = client.put(format!("https://sheets.googleapis.com/v4/spreadsheets/{}/values/{}?valueInputOption=RAW", arg("spreadsheetId"), arg("range")))
          .header("Authorization", format!("Bearer {token}"))
          .json(&body).send().await.map_err(|e| e.to_string())?;
        let text = resp.text().await.map_err(|e| e.to_string())?;
        Ok(clip(&text))
      }
      "docs_create" => {
        let body = serde_json::json!({ "title": arg("title") });
        let resp = client.post("https://docs.googleapis.com/v1/documents")
          .header("Authorization", format!("Bearer {token}"))
          .json(&body).send().await.map_err(|e| e.to_string())?;
        let text = resp.text().await.map_err(|e| e.to_string())?;
        Ok(clip(&text))
      }
      "docs_get" => {
        let resp = client.get(format!("https://docs.googleapis.com/v1/documents/{}", arg("documentId")))
          .header("Authorization", format!("Bearer {token}"))
          .send().await.map_err(|e| e.to_string())?;
        let text = resp.text().await.map_err(|e| e.to_string())?;
        Ok(clip(&text))
      }
      _ => Err(format!("Unknown integration action: {}", self.action)),
    }
  }
}


// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

// Ensures the Manager system agent exists; creates it if missing.
fn ensure_manager(conn: &Connection) -> Result<(), String> {
  let exists: bool = conn.query_row("SELECT COUNT(*) FROM agents WHERE id='manager'", [], |r| r.get::<_, i64>(0)).map(|c| c > 0).unwrap_or(false);
  if !exists {
    let model = manager_default_model(conn);
    let model = if model.is_empty() { "".into() } else { model };
    conn.execute(
      "INSERT INTO agents (id, name, objective, model, tool_ids, integrations, memory, permissions, home_path, color, is_manager, description) VALUES ('manager','Laos','You are Laos, the workspace orchestrator. Control the workspace and coordinate work.',?1,'[]','[]',1,'[\"network\"]','agents/manager','#22c55e',1,'')",
      params![model],
    ).map_err(|e| e.to_string())?;
  }
  Ok(())
}

#[tauri::command]
fn initialize_storage(app: AppHandle) -> Result<(), String> {
  let conn = db(&app)?;
  ensure_manager(&conn)?;
  Ok(())
}

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
  let mut stmt = conn.prepare("SELECT id, name, objective, model, tool_ids, integrations, memory, permissions, home_path, color, x, y, is_manager, description, persona FROM agents ORDER BY name").map_err(|e| e.to_string())?;
  let rows = stmt.query_map([], |row| {
    let tool_ids: String = row.get(4)?;
    let integrations: String = row.get(5)?;
    let permissions: String = row.get(7)?;
    Ok(AgentRecord {
      id: row.get(0)?, name: row.get(1)?, objective: row.get(2)?, model: row.get(3)?,
      tool_ids: parse_json_vec(&tool_ids), integrations: parse_json_vec(&integrations),
      memory: row.get::<_, i64>(6)? != 0, permissions: parse_json_vec(&permissions),
      home_path: row.get(8)?, color: row.get(9)?, x: row.get(10)?, y: row.get(11)?,
      is_manager: row.get::<_, i64>(12)? != 0, description: row.get(13)?, persona: row.get(14)?,
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
    "INSERT INTO agents (id, name, objective, model, tool_ids, integrations, memory, permissions, home_path, color, x, y, is_manager, description, persona) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15)
     ON CONFLICT(id) DO UPDATE SET name=excluded.name, objective=excluded.objective, model=excluded.model, tool_ids=excluded.tool_ids, integrations=excluded.integrations, memory=excluded.memory, permissions=excluded.permissions, home_path=excluded.home_path, color=excluded.color, x=excluded.x, y=excluded.y, is_manager=excluded.is_manager, description=excluded.description, persona=excluded.persona",
    params![agent.id, agent.name, agent.objective, agent.model,
      serde_json::to_string(&agent.tool_ids).unwrap_or_else(|_| "[]".into()),
      serde_json::to_string(&agent.integrations).unwrap_or_else(|_| "[]".into()),
      if agent.memory { 1 } else { 0 },
      serde_json::to_string(&agent.permissions).unwrap_or_else(|_| "[]".into()),
      agent.home_path, agent.color, agent.x, agent.y,
      if agent.is_manager { 1 } else { 0 }, agent.description, agent.persona],
  ).map_err(|e| e.to_string())?;
  Ok(())
}

#[tauri::command]
fn delete_agent(app: AppHandle, id: String) -> Result<(), String> {
  let conn = db(&app)?;
  // The Manager is a system agent and can never be deleted.
  let is_manager: bool = conn.query_row("SELECT is_manager FROM agents WHERE id=?1", params![id], |r| r.get::<_, i64>(0)).map(|v| v != 0).unwrap_or(false);
  if is_manager {
    return Err("The Manager is a system agent and cannot be deleted.".into());
  }
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
// Integrations
// ---------------------------------------------------------------------------

fn integration_definitions() -> Vec<IntegrationRecord> {
  vec![
    IntegrationRecord { id: "notion".into(), name: "Notion".into(), provider: "notion".into(), enabled: false, connected: false, config: serde_json::json!({}), actions: vec![
      IntegrationAction { name: "notion_search".into(), description: "Search Notion pages and databases".into() },
      IntegrationAction { name: "notion_create_page".into(), description: "Create a page in a Notion database or parent page".into() },
      IntegrationAction { name: "notion_get_page".into(), description: "Fetch a Notion page's content".into() },
    ] },
    IntegrationRecord { id: "airtable".into(), name: "Airtable".into(), provider: "airtable".into(), enabled: false, connected: false, config: serde_json::json!({}), actions: vec![
      IntegrationAction { name: "airtable_list_records".into(), description: "List records from an Airtable table".into() },
      IntegrationAction { name: "airtable_create_record".into(), description: "Create a record in an Airtable table".into() },
      IntegrationAction { name: "airtable_update_record".into(), description: "Update a record in an Airtable table".into() },
    ] },
    IntegrationRecord { id: "sheets".into(), name: "Google Sheets".into(), provider: "google".into(), enabled: false, connected: false, config: serde_json::json!({}), actions: vec![
      IntegrationAction { name: "sheets_read".into(), description: "Read rows from a Google Sheet".into() },
      IntegrationAction { name: "sheets_append".into(), description: "Append rows to a Google Sheet".into() },
      IntegrationAction { name: "sheets_update".into(), description: "Update cells in a Google Sheet".into() },
    ] },
    IntegrationRecord { id: "docs".into(), name: "Google Docs".into(), provider: "google".into(), enabled: false, connected: false, config: serde_json::json!({}), actions: vec![
      IntegrationAction { name: "docs_create".into(), description: "Create a Google Doc with content".into() },
      IntegrationAction { name: "docs_get".into(), description: "Read a Google Doc's content".into() },
    ] },
    IntegrationRecord { id: "telegram".into(), name: "Telegram".into(), provider: "telegram".into(), enabled: false, connected: false, config: serde_json::json!({}), actions: vec![
      IntegrationAction { name: "telegram_send".into(), description: "Send a Telegram message".into() },
    ] },
  ]
}

fn integration_secret(conn: &Connection, id: &str) -> Result<serde_json::Value, String> {
  let mut stmt = conn.prepare("SELECT config_json FROM integration_configs WHERE id=?1").map_err(|e| e.to_string())?;
  let mut rows = stmt.query_map(params![id], |row| row.get::<_, String>(0)).map_err(|e| e.to_string())?;
  let cfg: serde_json::Value = match rows.next() {
    Some(Ok(s)) => serde_json::from_str(&s).map_err(|e| e.to_string())?,
    Some(Err(e)) => return Err(e.to_string()),
    None => serde_json::json!({}),
  };
  // Sanitize: string-only fields must never be numbers (e.g. a bad clientId: 0).
  let mut out = cfg;
  if let Some(obj) = out.as_object_mut() {
    for k in ["clientId", "clientSecret", "token", "apiKey", "accessToken", "refreshToken"] {
      if let Some(v) = obj.get_mut(k) {
        if !v.is_string() { *v = serde_json::Value::Null; }
      }
    }
  }
  Ok(out)
}

fn mask_config(cfg: &serde_json::Value) -> serde_json::Value {
  let mut out = serde_json::Map::new();
  if let Some(obj) = cfg.as_object() {
    for (k, v) in obj {
      // Never expose secrets to the frontend; show a "set" marker instead.
      if k.to_lowercase().contains("token") || k.to_lowercase().contains("key") || k.to_lowercase().contains("secret") {
        if v.as_str().map(|s| !s.is_empty()).unwrap_or(false) { out.insert(k.clone(), serde_json::json!("â€¢â€¢â€¢â€¢â€¢â€¢â€¢â€¢")); }
        else { out.insert(k.clone(), serde_json::Value::Null); }
      } else {
        out.insert(k.clone(), v.clone());
      }
    }
  }
  serde_json::Value::Object(out)
}

#[tauri::command]
fn list_integrations(app: AppHandle) -> Result<Vec<IntegrationRecord>, String> {
  let conn = db(&app)?;
  let defs = integration_definitions();
  let mut out = Vec::new();
  for mut d in defs {
    let mut stmt = conn.prepare("SELECT config_json, enabled, connected FROM integration_configs WHERE id=?1").map_err(|e| e.to_string())?;
    let mut rows = stmt.query_map(params![d.id], |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? != 0, row.get::<_, i64>(2)? != 0))).map_err(|e| e.to_string())?;
    if let Some(Ok((cfg, enabled, connected))) = rows.next() {
      d.config = mask_config(&serde_json::from_str::<serde_json::Value>(&cfg).unwrap_or_else(|_| serde_json::json!({})));
      d.enabled = enabled; d.connected = connected;
    }
    out.push(d);
  }
  Ok(out)
}

#[tauri::command]
fn save_integration_config(app: AppHandle, id: String, config: serde_json::Value) -> Result<(), String> {
  let conn = db(&app)?;
  let def = integration_definitions().into_iter().find(|d| d.id == id).ok_or("Unknown integration")?;
  // Merge with any existing secret config so the frontend's masked values don't clobber real tokens.
  let existing = integration_secret(&conn, &id)?;
  let merged = merge_config(&existing, &config)?;
  conn.execute(
    "INSERT INTO integration_configs (id, name, provider, config_json, enabled, connected, updated_at) VALUES (?1,?2,?3,?4,?5,?6,?7)
     ON CONFLICT(id) DO UPDATE SET name=excluded.name, provider=excluded.provider, config_json=excluded.config_json, enabled=excluded.enabled, connected=excluded.connected, updated_at=excluded.updated_at",
    params![id, def.name, def.provider, serde_json::to_string(&merged).map_err(|e| e.to_string())?, 1, 0, now()],
  ).map_err(|e| e.to_string())?;
  Ok(())
}

fn merge_config(existing: &serde_json::Value, incoming: &serde_json::Value) -> Result<serde_json::Value, String> {
  let mut out = existing.clone();
  let obj = out.as_object_mut().ok_or("config must be an object")?;
  if let Some(inc) = incoming.as_object() {
    for (k, v) in inc {
      // Only merge strings, booleans, arrays, objects â€” never numbers that
      // sneak in as bad values (e.g. clientId: 0).
      if !v.is_string() && !v.is_boolean() && !v.is_array() && !v.is_object() && !v.is_null() {
        continue;
      }
      // Keep the stored secret if the frontend sent the masked placeholder or null.
      let masked = v.as_str().map(|s| s == "â€¢â€¢â€¢â€¢â€¢â€¢â€¢â€¢" || s.is_empty()).unwrap_or(v.is_null());
      if masked { continue; }
      obj.insert(k.clone(), v.clone());
    }
  }
  Ok(out)
}

#[tauri::command]
async fn test_integration(app: AppHandle, id: String) -> Result<bool, String> {
  let conn = db(&app)?;
  let cfg = integration_secret(&conn, &id)?;
  let token = cfg.get("token").and_then(|t| t.as_str()).or_else(|| cfg.get("apiKey").and_then(|t| t.as_str())).unwrap_or("");
  if token.is_empty() { return Ok(false); }
  let url: String = match id.as_str() {
    "notion" => "https://api.notion.com/v1/users/me".into(),
    "airtable" => "https://api.airtable.com/v0/meta/whoami".into(),
    "telegram" => format!("https://api.telegram.org/bot{token}/getMe"),
    _ => return Ok(true), // Google OAuth tokens: assume connected until a call fails
  };
  let response = reqwest::Client::new().get(url)
    .header("Authorization", format!("Bearer {token}"))
    .header("Notion-Version", "2022-06-28")
    .send().await.map_err(|e| e.to_string())?;
  let ok = response.status().is_success();
  conn.execute("UPDATE integration_configs SET connected=?1 WHERE id=?2", params![if ok { 1 } else { 0 }, id]).map_err(|e| e.to_string())?;
  Ok(ok)
}

// ---------------------------------------------------------------------------
// OAuth (loopback)
// ---------------------------------------------------------------------------

const OAUTH_REDIRECT_PORT: u16 = 14852;

fn oauth_redirect_uri() -> String { format!("http://127.0.0.1:{OAUTH_REDIRECT_PORT}/callback") }

// Builds the provider authorize URL. Client IDs come from the integration config
// (set in the UI), scopes are per provider.
#[tauri::command]
fn start_oauth(app: AppHandle, id: String) -> Result<String, String> {
  let conn = db(&app)?;
  let cfg = integration_secret(&conn, &id)?;
  let client_id = cfg.get("clientId").and_then(|c| c.as_str()).unwrap_or("").to_string();
  if client_id.is_empty() {
    return Err(format!("OAuth for '{id}' needs a clientId. Set it in the integration config first."));
  }
  let redirect = oauth_redirect_uri();
  let url = match id.as_str() {
    "sheets" | "docs" => format!(
      "https://accounts.google.com/o/oauth2/v2/auth?client_id={client_id}&redirect_uri={redirect}&response_type=code&scope=https://www.googleapis.com/auth/spreadsheets%20https://www.googleapis.com/auth/documents&access_type=offline&prompt=consent"
    ),
    "notion" => format!(
      "https://api.notion.com/v1/oauth/authorize?client_id={client_id}&redirect_uri={redirect}&response_type=code&owner=user"
    ),
    _ => return Err(format!("OAuth is not available for '{id}'. Configure it with an API token instead.")),
  };
  let _ = app; // app kept for signature consistency with future token refresh
  Ok(url)
}

// Opens the provider authorize URL in the system browser and starts a loopback
// listener on a background task to capture the redirect and exchange the code.
// Returns immediately so the UI isn't blocked; the browser does the waiting.
#[tauri::command]
async fn connect_oauth(app: AppHandle, id: String) -> Result<String, String> {
  let url = start_oauth(app.clone(), id.clone())?;
  tauri_plugin_opener::open_url(&url, None::<&str>).map_err(|e| e.to_string())?;

  let handle = app.clone();
  tauri::async_runtime::spawn(async move {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    let listener = match TcpListener::bind(("127.0.0.1", OAUTH_REDIRECT_PORT)) {
      Ok(l) => l,
      Err(e) => { eprintln!("OAuth loopback bind failed: {e}"); return; }
    };
    let (mut stream, _) = match listener.accept() {
      Ok(pair) => pair,
      Err(e) => { eprintln!("OAuth loopback accept failed: {e}"); return; }
    };
    let mut buf = [0u8; 8192];
    let n = match stream.read(&mut buf) {
      Ok(n) => n,
      Err(_) => return,
    };
    let request = String::from_utf8_lossy(&buf[..n]).to_string();
    let _ = stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: 58\r\n\r\n<html><body>Connected! You can close this tab.</body></html>");
    let _ = stream.flush();

    let first_line = request.lines().next().unwrap_or("");
    let code = first_line.split('?').nth(1).and_then(|q| q.split('&').find_map(|kv| {
      let mut it = kv.split('=');
      let k = it.next()?; let v = it.next()?;
      if k == "code" { Some(v.to_string()) } else { None }
    }));
    if let Some(code) = code {
      let _ = complete_oauth_inner(&handle, &id, &code).await;
    }
  });

  Ok(url)
}

async fn complete_oauth_inner(app: &AppHandle, id: &str, code: &str) -> Result<(), String> {
  let conn = db(app)?;
  let cfg = integration_secret(&conn, id)?;
  let client_id = cfg.get("clientId").and_then(|c| c.as_str()).unwrap_or("").to_string();
  let client_secret = cfg.get("clientSecret").and_then(|c| c.as_str()).unwrap_or("").to_string();
  if client_id.is_empty() || client_secret.is_empty() {
    return Err("OAuth client ID/secret not configured for this integration.".into());
  }
  let token_url = match id {
    "sheets" | "docs" => "https://oauth2.googleapis.com/token",
    "notion" => "https://api.notion.com/v1/oauth/token",
    _ => return Err(format!("OAuth is not available for '{id}'.")),
  };
  let body = serde_json::json!({
    "code": code,
    "client_id": client_id,
    "client_secret": client_secret,
    "redirect_uri": oauth_redirect_uri(),
    "grant_type": "authorization_code",
  });
  let response = reqwest::Client::new().post(token_url).header("Content-Type", "application/json").json(&body).send().await.map_err(|e| e.to_string())?;
  if !response.status().is_success() {
    let text = response.text().await.unwrap_or_default();
    return Err(format!("OAuth token exchange failed: {text}"));
  }
  let tokens: serde_json::Value = response.json().await.map_err(|e| e.to_string())?;
  let mut stored = cfg.clone();
  if let Some(obj) = stored.as_object_mut() {
    obj.insert("accessToken".into(), tokens.get("access_token").cloned().unwrap_or(serde_json::Value::Null));
    obj.insert("refreshToken".into(), tokens.get("refresh_token").cloned().unwrap_or(serde_json::Value::Null));
  }
  conn.execute(
    "INSERT INTO integration_configs (id, name, provider, config_json, enabled, connected, updated_at) VALUES (?1,?2,?3,?4,?5,?6,?7)
     ON CONFLICT(id) DO UPDATE SET config_json=excluded.config_json, enabled=excluded.enabled, connected=excluded.connected, updated_at=excluded.updated_at",
    params![id.to_string(), id.to_string(), id.to_string(), serde_json::to_string(&stored).map_err(|e| e.to_string())?, 1, 1, now()],
  ).map_err(|e| e.to_string())?;
  Ok(())
}

// Exchanges an OAuth code (provided by the frontend, e.g. pasted) for tokens.
#[tauri::command]
async fn complete_oauth(app: AppHandle, id: String, code: String) -> Result<(), String> {
  complete_oauth_inner(&app, &id, &code).await
}

// ---------------------------------------------------------------------------
// Agent execution
// ---------------------------------------------------------------------------

fn build_tools(conn: &Connection, agent: &AgentRequest, home: &std::path::Path) -> Result<Vec<Box<dyn AgentTool>>, String> {
  let mut tools: Vec<Box<dyn AgentTool>> = Vec::new();
  let has_network = agent.permissions.iter().any(|p| p == "network");
  let has_files = agent.permissions.iter().any(|p| p == "files");
  let has_host_fs = agent.permissions.iter().any(|p| p == "host_fs");
  let mut stmt = conn.prepare("SELECT kind, enabled, config_json FROM tools WHERE id=?1").map_err(|e| e.to_string())?;
  for tool_id in &agent.tool_ids {
    let mut rows = stmt.query_map(params![tool_id], |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? != 0, row.get::<_, String>(2)?))).map_err(|e| e.to_string())?;
    let Some(Ok((kind, enabled, config_json))) = rows.next() else { continue }; // unknown or missing â†’ skip
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
  // Grant integration actions to agents that explicitly have the integration AND
  // the integration is enabled+connected. Explicit per-agent grants only.
  for integration_id in &agent.integrations {
    let mut stmt = conn.prepare("SELECT enabled, connected, config_json FROM integration_configs WHERE id=?1").map_err(|e| e.to_string())?;
    let mut rows = stmt.query_map(params![integration_id], |row| Ok((row.get::<_, i64>(0)? != 0, row.get::<_, i64>(1)? != 0, row.get::<_, String>(2)?))).map_err(|e| e.to_string())?;
    if let Some(Ok((enabled, connected, cfg_json))) = rows.next() {
      if !enabled || !connected { continue; }
      let credentials: serde_json::Value = serde_json::from_str(&cfg_json).unwrap_or_else(|_| serde_json::json!({}));
      let def = integration_definitions().into_iter().find(|d| d.id == *integration_id);
      if let Some(def) = def {
        for action in def.actions {
          if has_network {
            tools.push(Box::new(IntegrationTool { action: action.name, credentials: credentials.clone() }));
          }
        }
      }
    }
  }
  // Host-filesystem tools: granted only to agents with the explicit host_fs permission.
  if has_host_fs {
    tools.push(Box::new(SearchFilesTool));
    tools.push(Box::new(ReadAnyFileTool));
    tools.push(Box::new(RunCommandTool));
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
async fn run_agent_once(app: &AppHandle, agent: &AgentRequest, input: &str, api_key: Option<&str>, events: &mut Vec<ExecutionEvent>) -> Result<(String, u64, u64), String> {
  run_agent_once_structured(app, agent, input, api_key, events, true).await
}

// `structured` controls whether the prompt forces the summary/result JSON envelope.
async fn run_agent_once_structured(app: &AppHandle, agent: &AgentRequest, input: &str, api_key: Option<&str>, events: &mut Vec<ExecutionEvent>, structured: bool) -> Result<(String, u64, u64), String> {
  let conn = db(app)?;
  let home = app.path().app_data_dir().map_err(|e| e.to_string())?.join("agents").join(&agent.id);
  for folder in ["files", "memory", "runs", "outputs"] { fs::create_dir_all(home.join(folder)).map_err(|e| e.to_string())?; }
  fs::write(home.join("config.json"), serde_json::to_string_pretty(&serde_json::json!({"id":agent.id,"name":agent.name,"objective":agent.objective,"model":agent.model,"tools":agent.tool_ids})).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
  let prompt = if structured {
    format!("You are {}. Objective: {}\n\nTask: {}\n\nReturn a concise structured result in JSON with keys summary and result.", agent.name, agent.objective, input)
  } else {
    format!("You are {}. Objective: {}\n\nTask: {}\n\nReturn a helpful, direct answer.", agent.name, agent.objective, input)
  };
  if let Some(model) = agent.model.strip_prefix("ollama:") {
    let tools = build_tools(&conn, agent, &home)?;
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
      .or(stored_api_key(&conn, &agent.model).ok().flatten())
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
  } else if let Some(model) = agent.model.strip_prefix("groq:") {
    let key = api_key
      .filter(|k| !k.trim().is_empty())
      .map(|k| k.to_string())
      .or(stored_api_key(&conn, &agent.model).ok().flatten())
      .ok_or("Groq requires an API key. Add it in Models first.")?;
    events.push(ExecutionEvent { time: now(), kind: "thought".into(), title: "Asking Groq".into(), detail: Some(model.into()) });
    let response = reqwest::Client::new().post("https://api.groq.com/openai/v1/chat/completions")
      .header("Authorization", format!("Bearer {key}"))
      .json(&serde_json::json!({"model":model,"messages":[{"role":"user","content":prompt}]})).send().await.map_err(|e| format!("Could not reach Groq: {e}"))?;
    if !response.status().is_success() { return Err(format!("Groq returned {}", response.status())); }
    let json: serde_json::Value = response.json().await.map_err(|e| e.to_string())?;
    let output = json["choices"][0]["message"]["content"].as_str().unwrap_or("Groq returned no text.").to_string();
    let pt = json["usage"]["prompt_tokens"].as_u64().unwrap_or(0);
    let ct = json["usage"]["completion_tokens"].as_u64().unwrap_or(0);
    events.push(ExecutionEvent { time: now(), kind: "result".into(), title: "Generated final result".into(), detail: None });
    Ok((output, pt, ct))
  } else { Err("Unknown model provider. Select Ollama, OpenRouter or Groq in the agent Model tab.".into()) }
}

// ---------------------------------------------------------------------------
// Tasks + conversations
// ---------------------------------------------------------------------------

fn create_task(conn: &Connection, requester: &str, assigned_agent: &str, input: &str, context: &str) -> Result<String, String> {
  let id = format!("task-{}", chrono::Utc::now().timestamp_millis());
  conn.execute("INSERT INTO tasks (id, requester, assigned_agent, status, input, context, created_at) VALUES (?1,?2,?3,'pending',?4,?5,?6)",
    params![id, requester, assigned_agent, input, context, chrono::Utc::now().to_rfc3339()]).map_err(|e| e.to_string())?;
  Ok(id)
}

fn task_from_row(row: &rusqlite::Row) -> rusqlite::Result<TaskRecord> {
  Ok(TaskRecord {
    id: row.get(0)?, requester: row.get(1)?, assigned_agent: row.get(2)?, status: row.get(3)?,
    input: row.get(4)?, context: row.get(5)?, result: row.get(6)?,
    created_at: row.get(7)?, completed_at: row.get(8)?,
  })
}

fn list_tasks(conn: &Connection) -> Result<Vec<TaskRecord>, String> {
  let mut stmt = conn.prepare("SELECT id, requester, assigned_agent, status, input, context, result, created_at, completed_at FROM tasks ORDER BY created_at DESC").map_err(|e| e.to_string())?;
  let rows = stmt.query_map([], task_from_row).map_err(|e| e.to_string())?;
  let mut out = Vec::new();
  for row in rows { out.push(row.map_err(|e| e.to_string())?); }
  Ok(out)
}

// Runs a domain agent for a delegated task, storing the result.
async fn delegate_task(app: &AppHandle, task_id: &str, assigned_agent: &str, input: &str, context: &str) -> Result<String, String> {
  let conn = db(app)?;
  let agent = {
    let mut stmt = conn.prepare("SELECT id, name, objective, model, tool_ids, integrations, home_path, permissions FROM agents WHERE id=?1").map_err(|e| e.to_string())?;
    let mut rows = stmt.query_map(params![assigned_agent], |row| {
      let tool_ids: String = row.get(4)?;
      let integrations: String = row.get(5)?;
      let permissions: String = row.get(7)?;
      Ok(AgentRequest {
        id: row.get(0)?, name: row.get(1)?, objective: row.get(2)?, model: row.get(3)?,
        tool_ids: parse_json_vec(&tool_ids), integrations: parse_json_vec(&integrations),
        home_path: row.get(6)?, permissions: parse_json_vec(&permissions),
      })
    }).map_err(|e| e.to_string())?;
    rows.next().transpose().map_err(|e| e.to_string())?
  };
  conn.execute("UPDATE tasks SET status='running' WHERE id=?1", params![task_id]).map_err(|e| e.to_string())?;
  let prompt = if context.is_empty() { input.to_string() } else { format!("Context:\n{context}\n\nTask:\n{input}") };
  let mut events = Vec::new();
  let result = match agent {
    Some(a) => run_agent_once_structured(app, &a, &prompt, None, &mut events, false).await.map(|(out, _, _)| out),
    None => Err("Assigned agent not found.".into()),
  };
  match result {
    Ok(out) => {
      conn.execute("UPDATE tasks SET status='completed', result=?1, completed_at=?2 WHERE id=?3", params![out, chrono::Utc::now().to_rfc3339(), task_id]).map_err(|e| e.to_string())?;
      Ok(out)
    }
    Err(e) => {
      conn.execute("UPDATE tasks SET status='failed', result=?1, completed_at=?2 WHERE id=?3", params![format!("Error: {e}"), chrono::Utc::now().to_rfc3339(), task_id]).map_err(|e| e.to_string())?;
      Err(e)
    }
  }
}

// Builds the Manager's workspace context: agents, integrations, active tasks.
fn build_workspace_context(conn: &Connection) -> Result<String, String> {
  let mut agents = String::new();
  {
    let mut stmt = conn.prepare("SELECT id, name, description, objective, model, integrations, permissions FROM agents WHERE is_manager=0 ORDER BY name").map_err(|e| e.to_string())?;
    let rows = stmt.query_map([], |row| {
      let integrations: String = row.get(5)?;
      let permissions: String = row.get(6)?;
      Ok(format!("- {} (id: {}): {}. Model: {}. Integrations: {}. Permissions: {}.",
        row.get::<_, String>(1)?, row.get::<_, String>(0)?,
        row.get::<_, String>(2)?, row.get::<_, String>(3)?,
        integrations, permissions))
    }).map_err(|e| e.to_string())?;
    for row in rows { agents.push_str(&row.map_err(|e| e.to_string())?); agents.push('\n'); }
  }
  let mut ints = String::new();
  {
    let defs = integration_definitions();
    for d in defs {
      let mut stmt = conn.prepare("SELECT enabled, connected FROM integration_configs WHERE id=?1").map_err(|e| e.to_string())?;
      let mut rows = stmt.query_map(params![d.id], |row| Ok((row.get::<_, i64>(0)? != 0, row.get::<_, i64>(1)? != 0))).map_err(|e| e.to_string())?;
      let (enabled, connected) = rows.next().transpose().map_err(|e| e.to_string())?.unwrap_or((false, false));
      let actions = d.actions.iter().map(|a| a.name.clone()).collect::<Vec<_>>().join(", ");
      ints.push_str(&format!("- {} (id: {}): enabled={} connected={}. Actions: {}\n", d.name, d.id, enabled, connected, actions));
    }
  }
  let tasks = list_tasks(conn)?;
  let active: Vec<String> = tasks.iter().filter(|t| t.status == "pending" || t.status == "running").map(|t| format!("- {} â†’ {}: {} ({})", t.id, t.assigned_agent, t.input, t.status)).collect();
  let recent: Vec<String> = tasks.iter().take(5).filter(|t| t.status == "completed").map(|t| format!("- {} â†’ {}: result: {}", t.id, t.assigned_agent, t.result.as_deref().unwrap_or(""))).collect();
  Ok(format!(
    "## Available Agents\n{}\n## Available Integrations\n{}\n## Active Tasks\n{}\n## Recent Task Results\n{}",
    if agents.is_empty() { "- none".to_string() } else { agents },
    if ints.is_empty() { "- none".to_string() } else { ints },
    if active.is_empty() { "- none".to_string() } else { active.join("\n") },
    if recent.is_empty() { "- none".to_string() } else { recent.join("\n") },
  ))
}

#[tauri::command]
async fn execute_agent(app: AppHandle, agent: AgentRequest, input: String, api_key: Option<String>) -> Result<Execution, String> {
  let started = chrono::Utc::now().to_rfc3339();
  let run_id = format!("{}-{}", agent.id, chrono::Utc::now().timestamp_millis());
  let conn = db(&app)?;
  conn.execute("INSERT INTO runs (id,agent_id,started_at,status,model,input) VALUES (?1,?2,?3,'running',?4,?5)", params![run_id, agent.id, started, agent.model, input]).map_err(|e| e.to_string())?;
  let mut events = vec![ExecutionEvent { time: now(), kind: "thought".into(), title: "Loaded isolated agent context".into(), detail: Some(format!("Home: {} Â· tools: {}", agent.home_path, agent.tool_ids.join(", "))) }];
  let home = app.path().app_data_dir().map_err(|e| e.to_string())?.join("agents").join(&agent.id);
  for folder in ["files", "memory", "runs", "outputs"] { fs::create_dir_all(home.join(folder)).map_err(|e| e.to_string())?; }
  let (output, prompt_tokens, completion_tokens) = match run_agent_once(&app, &agent, &input, api_key.as_deref(), &mut events).await {
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

// ---------------------------------------------------------------------------
// Workflow rule engine (checker nodes)
// ---------------------------------------------------------------------------

// Evaluate a single deterministic rule against the given value. Returns
// Ok(true/false) or Err for a malformed rule. `path` resolution is done by
// callers via resolve_path.
fn eval_rule(rule: &serde_json::Value, value: &serde_json::Value) -> Result<bool, String> {
  let rtype = rule.get("type").and_then(|v| v.as_str()).unwrap_or("").to_string();
  match rtype.as_str() {
    "contains" => {
      let needle = rule.get("value").and_then(|v| v.as_str()).unwrap_or("").to_string();
      let hay = value.as_str().ok_or("contains: input is not a string")?;
      Ok(hay.contains(&needle))
    }
    "notContains" => {
      let needle = rule.get("value").and_then(|v| v.as_str()).unwrap_or("").to_string();
      let hay = value.as_str().ok_or("notContains: input is not a string")?;
      Ok(!hay.contains(&needle))
    }
    "nonEmpty" => Ok(!(value.as_str().map(|s| s.trim().is_empty()).unwrap_or(value.is_null()))),
    "startsWith" => {
      let needle = rule.get("value").and_then(|v| v.as_str()).unwrap_or("").to_string();
      Ok(value.as_str().map(|s| s.starts_with(&needle)).unwrap_or(false))
    }
    "endsWith" => {
      let needle = rule.get("value").and_then(|v| v.as_str()).unwrap_or("").to_string();
      Ok(value.as_str().map(|s| s.ends_with(&needle)).unwrap_or(false))
    }
    "regex" => {
      let pattern = rule.get("pattern").and_then(|v| v.as_str()).unwrap_or("").to_string();
      let mode = rule.get("mode").and_then(|v| v.as_str()).unwrap_or("mustMatch").to_string();
      let re = regex::Regex::new(&pattern).map_err(|e| format!("regex: bad pattern ({e})"))?;
      let s = value.as_str().ok_or("regex: input is not a string")?;
      let matched = re.is_match(s);
      Ok(if mode == "mustNotMatch" { !matched } else { matched })
    }
    "validJson" => {
      Ok(value.as_str().map(|s| serde_json::from_str::<serde_json::Value>(s).is_ok()).unwrap_or(false))
    }
    "equals" => {
      let expected = rule.get("value").and_then(|v| v.as_str()).unwrap_or("").to_string();
      Ok(value.as_str().map(|s| s == expected).unwrap_or(false))
    }
    "notEquals" => {
      let expected = rule.get("value").and_then(|v| v.as_str()).unwrap_or("").to_string();
      Ok(value.as_str().map(|s| s != expected).unwrap_or(false))
    }
    "lengthRange" => {
      let min = rule.get("min").and_then(|v| v.as_u64()).unwrap_or(0);
      let max = rule.get("max").and_then(|v| v.as_u64()).unwrap_or(u64::MAX);
      let len = value.as_str().map(|s| s.chars().count() as u64).ok_or("lengthRange: input is not a string")?;
      Ok(len >= min && len <= max)
    }
    "numericRange" => {
      let min = rule.get("min").and_then(|v| v.as_f64()).unwrap_or(f64::MIN);
      let max = rule.get("max").and_then(|v| v.as_f64()).unwrap_or(f64::MAX);
      let num = value.as_f64().or_else(|| value.as_str().and_then(|s| s.trim().parse::<f64>().ok()))
        .ok_or("numericRange: input is not a number")?;
      Ok(num >= min && num <= max)
    }
    "hasField" => {
      let path = rule.get("path").and_then(|v| v.as_str()).unwrap_or("").to_string();
      let target = resolve_path(value, &path);
      let present = !target.is_null();
      Ok(if rule.get("min").and_then(|v| v.as_u64()).unwrap_or(0) > 0 {
        present && matches!(target, serde_json::Value::Array(a) if a.len() as u64 >= rule.get("min").and_then(|v| v.as_u64()).unwrap_or(0))
      } else {
        present
      })
    }
    "fieldType" => {
      let path = rule.get("path").and_then(|v| v.as_str()).unwrap_or("").to_string();
      let want = rule.get("type").and_then(|v| v.as_str()).unwrap_or("").to_string();
      let target = resolve_path(value, &path);
      let matches = match want.as_str() {
        "string" => target.is_string(),
        "number" => target.is_number(),
        "bool" => target.is_boolean(),
        "array" => target.is_array(),
        "object" => target.is_object(),
        "null" => target.is_null(),
        _ => false,
      };
      Ok(matches)
    }
    "arrayLength" => {
      let path = rule.get("path").and_then(|v| v.as_str()).unwrap_or("").to_string();
      let target = resolve_path(value, &path);
      let arr = target.as_array().ok_or("arrayLength: target is not an array")?;
      let min = rule.get("min").and_then(|v| v.as_u64()).unwrap_or(0);
      let max = rule.get("max").and_then(|v| v.as_u64()).unwrap_or(u64::MAX);
      Ok(arr.len() as u64 >= min && arr.len() as u64 <= max)
    }
    "inList" => {
      let list = rule.get("values").and_then(|v| v.as_array()).cloned().unwrap_or_default();
      let s = value.as_str().ok_or("inList: input is not a string")?;
      Ok(list.iter().any(|v| v.as_str() == Some(s)))
    }
    "each" => {
      let on = rule.get("on").and_then(|v| v.as_str()).unwrap_or("").to_string();
      let sub_rules = rule.get("rules").and_then(|v| v.as_array()).cloned().unwrap_or_default();
      let target = if on.is_empty() { value.clone() } else { resolve_path(value, &on) };
      let arr = target.as_array().ok_or("each: target is not an array")?;
      for item in arr {
        for sub in &sub_rules {
          if !eval_rule(sub, item)? { return Ok(false); }
        }
      }
      Ok(true)
    }
    "all" => {
      let sub = rule.get("rules").and_then(|v| v.as_array()).cloned().unwrap_or_default();
      for r in &sub { if !eval_rule(r, value)? { return Ok(false); } }
      Ok(true)
    }
    "any" => {
      let sub = rule.get("rules").and_then(|v| v.as_array()).cloned().unwrap_or_default();
      for r in &sub { if eval_rule(r, value)? { return Ok(true); } }
      Ok(false)
    }
    "not" => {
      let sub = rule.get("rule").ok_or("not: missing rule")?;
      Ok(!eval_rule(sub, value)?)
    }
    _ => Err(format!("Unknown rule type: {rtype}")),
  }
}

// Resolve a dotted path ("items[0].title") against a JSON value.
fn resolve_path(value: &serde_json::Value, path: &str) -> serde_json::Value {
  let mut cur = value.clone();
  for seg in path.split('.') {
    if seg.is_empty() { continue; }
    // Support array index syntax: items[0]
    if let Some(bracket) = seg.find('[') {
      let name = &seg[..bracket];
      let rest = &seg[bracket..];
      if !name.is_empty() { cur = cur.get(name).cloned().unwrap_or(serde_json::Value::Null); }
      for idx in rest.split('[').filter(|s| !s.is_empty()) {
        let i: usize = idx.trim_end_matches(']').parse().unwrap_or(usize::MAX);
        cur = cur.get(i).cloned().unwrap_or(serde_json::Value::Null);
      }
    } else {
      cur = cur.get(seg).cloned().unwrap_or(serde_json::Value::Null);
    }
  }
  cur
}

// Run the checker: evaluate the rule list against the envelope value, with an
// optional LLM judge as the final rule. Returns the JSON verdict envelope.
async fn run_checker(app: &AppHandle, config: &serde_json::Value, input_value: &serde_json::Value, api_key: Option<&str>) -> Result<serde_json::Value, String> {
  let rules = config.get("rules").and_then(|v| v.as_array()).cloned().unwrap_or_default();
  let mode = config.get("mode").and_then(|v| v.as_str()).unwrap_or("all").to_string();
  let mut failed: Vec<String> = Vec::new();
  let mut judged = false;

  // Deterministic rules first (LLM judge runs last).
  let mut llm_rule: Option<serde_json::Value> = None;
  for rule in &rules {
    if rule.get("type").and_then(|v| v.as_str()) == Some("llmJudge") { llm_rule = Some(rule.clone()); continue; }
    let ok = eval_rule(rule, input_value).map_err(|e| format!("Checker rule error: {e}"))?;
    let label = rule.get("label").and_then(|v| v.as_str()).map(|s| s.to_string()).unwrap_or_else(|| format!("{}", rule.get("type").and_then(|v| v.as_str()).unwrap_or("rule")));
    if !ok {
      failed.push(label);
      if mode == "all" { break; } // short-circuit: all rules must pass
    }
  }

  // LLM judge: only run if deterministic rules passed (or mode is "any" and nothing failed).
  let deterministic_passed = failed.is_empty();
  if let Some(lr) = llm_rule {
    if deterministic_passed {
      let verdict = run_llm_judge(app, &lr, input_value, api_key).await?;
      judged = true;
      if !verdict["pass"].as_bool().unwrap_or(false) {
        failed.push(verdict["reason"].as_str().unwrap_or("LLM judge rejected output").to_string());
      }
    }
  }

  let pass = failed.is_empty();
  Ok(serde_json::json!({
    "pass": pass,
    "failedRules": failed,
    "llmJudged": judged,
    "reason": if pass { "All checks passed".to_string() } else { format!("Failed: {}", failed.join(", ")) },
  }))
}

// One LLM call that returns { pass: bool, reason: string }. Retries once if the
// response isn't parseable.
async fn run_llm_judge(app: &AppHandle, rule: &serde_json::Value, input_value: &serde_json::Value, api_key: Option<&str>) -> Result<serde_json::Value, String> {
  let conn = db(app)?;
  let model = rule.get("model").and_then(|v| v.as_str()).unwrap_or("groq:llama-3.3-70b-versatile").to_string();
  let prompt = rule.get("prompt").and_then(|v| v.as_str()).unwrap_or("Is this output acceptable? Reply with only JSON {\"pass\":true/false,\"reason\":\"...\"}").to_string();
  let user_content = if input_value.is_string() {
    input_value.as_str().unwrap_or("").to_string()
  } else {
    serde_json::to_string_pretty(input_value).unwrap_or_else(|_| "".to_string())
  };
  let system = "You are a strict quality judge. Evaluate the output against the criteria and reply with ONLY a JSON object: {\"pass\": true or false, \"reason\": \"short explanation\"}. No other text.";
  let full_prompt = format!("Criteria:\n{prompt}\n\nOutput to judge:\n{user_content}");

  let mut last_err = "LLM judge returned no text".to_string();
  for attempt in 0..2 {
    let result = if let Some(m) = model.strip_prefix("ollama:") {
      let client = reqwest::Client::new();
      let resp = client.post("http://127.0.0.1:11434/api/chat")
        .json(&serde_json::json!({"model": m, "messages": [{"role":"system","content":system},{"role":"user","content":full_prompt}], "stream": false, "format": "json"}))
        .send().await.map_err(|e| format!("LLM judge: could not reach Ollama ({e})"))?;
      if !resp.status().is_success() { return Err(format!("LLM judge: Ollama returned {}", resp.status())); }
      let json: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
      json["message"]["content"].as_str().unwrap_or("").to_string()
    } else if let Some(m) = model.strip_prefix("groq:") {
      let key = api_key.filter(|k| !k.trim().is_empty()).map(|k| k.to_string()).or(stored_api_key(&conn, &model).ok().flatten())
        .ok_or("LLM judge: Groq requires an API key.")?;
      let client = reqwest::Client::new();
      let resp = client.post("https://api.groq.com/openai/v1/chat/completions")
        .header("Authorization", format!("Bearer {key}"))
        .json(&serde_json::json!({"model": m, "messages": [{"role":"system","content":system},{"role":"user","content":full_prompt}], "temperature": 0.1}))
        .send().await.map_err(|e| format!("LLM judge: could not reach Groq ({e})"))?;
      if !resp.status().is_success() { return Err(format!("LLM judge: Groq returned {}", resp.status())); }
      let json: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
      json["choices"][0]["message"]["content"].as_str().unwrap_or("").to_string()
    } else if let Some(m) = model.strip_prefix("openrouter:") {
      let key = api_key.filter(|k| !k.trim().is_empty()).map(|k| k.to_string()).or(stored_api_key(&conn, &model).ok().flatten())
        .ok_or("LLM judge: OpenRouter requires an API key.")?;
      let client = reqwest::Client::new();
      let resp = client.post("https://openrouter.ai/api/v1/chat/completions")
        .header("Authorization", format!("Bearer {key}"))
        .header("HTTP-Referer", "https://local-agent-os.app")
        .header("X-Title", "Local Agent OS")
        .json(&serde_json::json!({"model": m, "messages": [{"role":"system","content":system},{"role":"user","content":full_prompt}], "temperature": 0.1}))
        .send().await.map_err(|e| format!("LLM judge: could not reach OpenRouter ({e})"))?;
      if !resp.status().is_success() { return Err(format!("LLM judge: OpenRouter returned {}", resp.status())); }
      let json: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
      json["choices"][0]["message"]["content"].as_str().unwrap_or("").to_string()
    } else {
      return Err(format!("LLM judge: unknown model provider for {model}"));
    };

    // Parse the verdict, tolerating markdown fences and stray text.
    let cleaned = strip_code_fences(&result);
    let trimmed = cleaned.trim();
    let parsed: Option<serde_json::Value> = if let Some(start) = trimmed.find('{') {
      if let Some(end) = trimmed.rfind('}') {
        serde_json::from_str(&trimmed[start..=end]).ok()
      } else { None }
    } else { None };
    if let Some(v) = parsed {
      return Ok(v);
    }
    last_err = format!("LLM judge: could not parse response (attempt {}): {}", attempt + 1, result.chars().take(120).collect::<String>());
  }
  // Unparseable after retries: fail closed.
  Ok(serde_json::json!({ "pass": false, "reason": last_err }))
}

fn strip_code_fences(s: &str) -> String {
  let t = s.trim();
  if t.starts_with("```") {
    let mut lines = t.lines();
    let _ = lines.next();
    lines.collect::<Vec<_>>().join("\n").trim_end_matches("```").trim().to_string()
  } else {
    t.to_string()
  }
}

// Evaluate a simple gate condition against the incoming value. Supports:
//   $pass == true | $result.pass == false | contains("error") | $len > 5
// This is a tiny safe evaluator — no eval, just patterns.
fn eval_condition(condition: &str, value: &serde_json::Value) -> bool {
  let c = condition.trim();
  // Tokenize around a comparison operator.
  let (left, right, op) = if let Some(i) = c.find("==") {
    (c[..i].trim(), c[i + 2..].trim(), "eq")
  } else if let Some(i) = c.find("!=") {
    (c[..i].trim(), c[i + 2..].trim(), "ne")
  } else if let Some(i) = c.find(">=") {
    (c[..i].trim(), c[i + 2..].trim(), "ge")
  } else if let Some(i) = c.find("<=") {
    (c[..i].trim(), c[i + 2..].trim(), "le")
  } else if let Some(i) = c.find('>') {
    (c[..i].trim(), c[i + 1..].trim(), "gt")
  } else if let Some(i) = c.find('<') {
    (c[..i].trim(), c[i + 1..].trim(), "lt")
  } else {
    // No operator: treat as truthy expression (contains("...") or $field).
    return eval_truthy(c, value);
  };

  let lv = resolve_expr(left, value);
  let rv = resolve_expr(right, value);
  match op {
    "eq" => json_eq(&lv, &rv),
    "ne" => !json_eq(&lv, &rv),
    "gt" => cmp_f64(&lv, &rv).map(|o| o.is_gt()).unwrap_or(false),
    "lt" => cmp_f64(&lv, &rv).map(|o| o.is_lt()).unwrap_or(false),
    "ge" => cmp_f64(&lv, &rv).map(|o| o.is_ge()).unwrap_or(false),
    "le" => cmp_f64(&lv, &rv).map(|o| o.is_le()).unwrap_or(false),
    _ => false,
  }
}

// Resolve a "$path", "contains(...)", "$len", literal, or quoted string.
fn resolve_expr(expr: &str, value: &serde_json::Value) -> serde_json::Value {
  let e = expr.trim();
  if e.is_empty() { return serde_json::Value::Null; }
  // contains("text") / not contains
  if let Some(rest) = e.strip_prefix("contains(") {
    if let Some(arg) = rest.strip_suffix(')') {
      let needle = arg.trim().trim_matches('"').trim_matches('\'');
      let hay = value.as_str().unwrap_or("");
      return serde_json::json!(hay.contains(needle));
    }
  }
  if let Some(rest) = e.strip_prefix("not contains(") {
    if let Some(arg) = rest.strip_suffix(')') {
      let needle = arg.trim().trim_matches('"').trim_matches('\'');
      let hay = value.as_str().unwrap_or("");
      return serde_json::json!(!hay.contains(needle));
    }
  }
  // $len — string/array length
  if e == "$len" {
    let len = match value {
      serde_json::Value::String(s) => s.chars().count(),
      serde_json::Value::Array(a) => a.len(),
      _ => 0,
    };
    return serde_json::json!(len as i64);
  }
  // $pass or $result.pass — resolved from the envelope
  if let Some(path) = e.strip_prefix('$') {
    let p = path.trim_start_matches("result.");
    return resolve_path(value, p);
  }
  // Literal: number, true/false, quoted string
  if let Ok(n) = e.parse::<f64>() { return serde_json::json!(n); }
  if e == "true" { return serde_json::json!(true); }
  if e == "false" { return serde_json::json!(false); }
  let q = e.trim_matches('"').trim_matches('\'');
  serde_json::json!(q)
}

fn eval_truthy(c: &str, value: &serde_json::Value) -> bool {
  let e = c.trim();
  if e.is_empty() { return false; }
  // contains("...")
  if e.starts_with("contains(") && e.ends_with(')') {
    let arg = e["contains(".len()..e.len() - 1].trim().trim_matches('"').trim_matches('\'');
    return value.as_str().map(|s| s.contains(arg)).unwrap_or(false);
  }
  // bare $path -> truthiness
  if let Some(path) = e.strip_prefix('$') {
    let p = path.trim_start_matches("result.");
    let v = resolve_path(value, p);
    return match v {
      serde_json::Value::Bool(b) => b,
      serde_json::Value::String(s) => !s.is_empty(),
      serde_json::Value::Number(n) => n.as_f64().unwrap_or(0.0) != 0.0,
      serde_json::Value::Array(a) => !a.is_empty(),
      serde_json::Value::Object(o) => !o.is_empty(),
      serde_json::Value::Null => false,
    };
  }
  false
}

fn json_eq(a: &serde_json::Value, b: &serde_json::Value) -> bool {
  match (a, b) {
    (serde_json::Value::String(x), serde_json::Value::String(y)) => x == y,
    (serde_json::Value::Bool(x), serde_json::Value::Bool(y)) => x == y,
    (serde_json::Value::Number(x), serde_json::Value::Number(y)) => x.as_f64() == y.as_f64(),
    (serde_json::Value::String(x), serde_json::Value::Bool(y)) => x == &y.to_string(),
    (serde_json::Value::Bool(y), serde_json::Value::String(x)) => x == &y.to_string(),
    (serde_json::Value::String(x), serde_json::Value::Number(y)) => x.parse::<f64>().ok() == y.as_f64(),
    (serde_json::Value::Number(y), serde_json::Value::String(x)) => x.parse::<f64>().ok() == y.as_f64(),
    _ => a == b,
  }
}

fn cmp_f64(a: &serde_json::Value, b: &serde_json::Value) -> Option<std::cmp::Ordering> {
  let an = a.as_f64().or_else(|| a.as_str().and_then(|s| s.parse().ok()))?;
  let bn = b.as_f64().or_else(|| b.as_str().and_then(|s| s.parse().ok()))?;
  an.partial_cmp(&bn)
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
    let config = node.get("config").cloned().unwrap_or(serde_json::json!({}));
    let mut events = Vec::new();
    let output = match ntype.as_str() {
      "agent" | "subagent" => {
        let agent_id = node.get("agentId").and_then(|v| v.as_str()).unwrap_or("").to_string();
        if agent_id.is_empty() {
          current_input.clone()
        } else {
          let agent_opt = {
            let mut stmt = conn.prepare("SELECT id, name, objective, model, tool_ids, integrations, home_path, permissions FROM agents WHERE id=?1").map_err(|e| e.to_string())?;
            let mut rows = stmt.query_map(params![agent_id], |row| {
              let tool_ids: String = row.get(4)?;
              let integrations: String = row.get(5)?;
              let permissions: String = row.get(7)?;
              Ok(AgentRequest {
                id: row.get(0)?, name: row.get(1)?, objective: row.get(2)?, model: row.get(3)?,
                tool_ids: parse_json_vec(&tool_ids), integrations: parse_json_vec(&integrations),
                home_path: row.get(6)?, permissions: parse_json_vec(&permissions),
              })
            }).map_err(|e| e.to_string())?;
            rows.next().transpose().map_err(|e| e.to_string())?
          };
          let (out, pt, ct) = match agent_opt {
            Some(agent) => run_agent_once(&app, &agent, &current_input, api_key.as_deref(), &mut events).await?,
            None => (current_input.clone(), 0, 0),
          };
          total_prompt += pt; total_completion += ct;
          out
        }
      }
      "checker" => {
        // Try to parse the incoming text as JSON; if it is, rules run against
        // the parsed object, otherwise against the raw string.
        let value: serde_json::Value = serde_json::from_str(&current_input).unwrap_or(serde_json::Value::String(current_input.clone()));
        let verdict = run_checker(&app, &config, &value, api_key.as_deref()).await?;
        // The verdict envelope becomes the input for downstream nodes.
        serde_json::to_string(&verdict).unwrap_or_else(|_| r#"{"pass":false,"reason":"serialize error"}"#.to_string())
      }
      "gate" => {
        // Evaluate the condition against $result (the previous node's output
        // parsed as JSON, or a plain string fallback). True = continue down
        // this edge; false = skip (output is passed through but flagged).
        let condition = config.get("condition").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let parsed: serde_json::Value = serde_json::from_str(&current_input).unwrap_or(serde_json::Value::String(current_input.clone()));
        let decision = if condition.is_empty() { true } else { eval_condition(&condition, &parsed) };
        serde_json::to_string(&serde_json::json!({ "gate": condition, "decision": decision, "input": if parsed.is_string() { parsed.as_str().unwrap_or("").to_string() } else { serde_json::to_string(&parsed).unwrap_or_default() } }))
          .unwrap_or_else(|_| current_input.clone())
      }
      "integration" => {
        // Run an integration action with the accumulated workflow data.
        let integration_id = config.get("integrationId").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let action = config.get("action").and_then(|v| v.as_str()).unwrap_or("").to_string();
        if integration_id.is_empty() || action.is_empty() {
          serde_json::json!({ "error": "integration node requires integrationId and action" }).to_string()
        } else {
          let creds = integration_secret(&conn, &integration_id).unwrap_or_else(|_| serde_json::json!({}));
          let tool = IntegrationTool { action: action.clone(), credentials: creds };
          // Build args from the node config, injecting the accumulated output
          // as the primary payload (title/content/fields) where applicable.
          let mut args = config.get("args").cloned().unwrap_or(serde_json::json!({}));
          if let Some(obj) = args.as_object_mut() {
            let out_str = &current_input;
            // Default injection: map workflow output into the most common
            // payload fields if the user hasn't overridden them.
            if !obj.contains_key("content") && (action.contains("docs_create") || action.contains("notion_create_page")) {
              obj.insert("content".into(), serde_json::json!(out_str));
            }
            if !obj.contains_key("title") && action.contains("notion_create_page") {
              obj.insert("title".into(), serde_json::json!(out_str.chars().take(120).collect::<String>()));
            }
          }
          match tool.run(&args).await {
            Ok(res) => res,
            Err(e) => serde_json::json!({ "error": e }).to_string(),
          }
        }
      }
      _ => current_input.clone(), // trigger/loop: pass through for now
    };
    steps.push(WorkflowStep { node_id: node_id.clone(), node_label: label, output: output.clone(), prompt_tokens: 0, completion_tokens: 0 });
    current_input = output;
  }

  let final_output = current_input;
  Ok(WorkflowExecution { steps, final_output, total_prompt_tokens: total_prompt, total_completion_tokens: total_completion })
}

// ---------------------------------------------------------------------------
// Manager Agent
// ---------------------------------------------------------------------------

struct ManagerListAgents;
struct ManagerDelegate;
struct ManagerTaskStatus;
struct ManagerListTasks;
struct ManagerSwitch;

#[async_trait]
impl AgentTool for ManagerListAgents {
  fn name(&self) -> String { "list_agents".into() }
  fn description(&self) -> String { "List all available agents with their ids, descriptions and capabilities.".into() }
  fn params_schema(&self) -> serde_json::Value { serde_json::json!({ "type": "object", "properties": {}, "required": [] }) }
  async fn run(&self, _args: &serde_json::Value) -> Result<String, String> { Ok("list_agents".into()) } // replaced at runtime
}

#[async_trait]
impl AgentTool for ManagerDelegate {
  fn name(&self) -> String { "delegate_task".into() }
  fn description(&self) -> String { "Delegate a task to an agent. Params: agentId (string), task (string), context (string, optional). Creates a task and runs the agent.".into() }
  fn params_schema(&self) -> serde_json::Value {
    serde_json::json!({ "type": "object", "properties": { "agentId": { "type": "string" }, "task": { "type": "string" }, "context": { "type": "string" } }, "required": ["agentId", "task"] })
  }
  async fn run(&self, _args: &serde_json::Value) -> Result<String, String> { Ok("delegate_task".into()) } // replaced at runtime
}

#[async_trait]
impl AgentTool for ManagerTaskStatus {
  fn name(&self) -> String { "get_task_status".into() }
  fn description(&self) -> String { "Get the status and result of a task. Params: taskId (string).".into() }
  fn params_schema(&self) -> serde_json::Value {
    serde_json::json!({ "type": "object", "properties": { "taskId": { "type": "string" } }, "required": ["taskId"] })
  }
  async fn run(&self, _args: &serde_json::Value) -> Result<String, String> { Ok("get_task_status".into()) } // replaced at runtime
}

#[async_trait]
impl AgentTool for ManagerListTasks {
  fn name(&self) -> String { "list_tasks".into() }
  fn description(&self) -> String { "List all tasks and their statuses.".into() }
  fn params_schema(&self) -> serde_json::Value { serde_json::json!({ "type": "object", "properties": {}, "required": [] }) }
  async fn run(&self, _args: &serde_json::Value) -> Result<String, String> { Ok("list_tasks".into()) } // replaced at runtime
}

#[async_trait]
impl AgentTool for ManagerSwitch {
  fn name(&self) -> String { "switch_agent".into() }
  fn description(&self) -> String { "Switch the current conversation to another agent. Params: agentId (string).".into() }
  fn params_schema(&self) -> serde_json::Value {
    serde_json::json!({ "type": "object", "properties": { "agentId": { "type": "string" } }, "required": ["agentId"] })
  }
  async fn run(&self, _args: &serde_json::Value) -> Result<String, String> { Ok("switch_agent".into()) } // replaced at runtime
}

// App-control tools: the Manager can create/update/delete agents and workflows,
// configure integrations, run tasks, and inspect the workspace.
macro_rules! manager_tool {
  ($struct:ident, $name:literal, $desc:literal, $props:expr, $req:expr) => {
    struct $struct;
    #[async_trait]
    impl AgentTool for $struct {
      fn name(&self) -> String { $name.into() }
      fn description(&self) -> String { $desc.into() }
      fn params_schema(&self) -> serde_json::Value {
        serde_json::json!({ "type": "object", "properties": $props, "required": $req })
      }
      async fn run(&self, _args: &serde_json::Value) -> Result<String, String> { Ok($name.into()) }
    }
  };
}

manager_tool!(ManagerCreateAgent, "create_agent", "Create a new agent. Params: name (string), objective (string), model (string, e.g. 'groq:llama-3.3-70b-versatile'), toolIds (array, optional), integrations (array, optional), permissions (array, optional: 'network', 'files', or 'host_fs' for host file access). No credentials are needed to create an agent â€” host file access is granted via the permissions array ('host_fs').", serde_json::json!({ "name": { "type": "string" }, "objective": { "type": "string" }, "model": { "type": "string" }, "toolIds": { "type": "array", "items": { "type": "string" } }, "integrations": { "type": "array", "items": { "type": "string" } }, "permissions": { "type": "array", "items": { "type": "string" } } }), serde_json::json!(["name", "objective", "model"]));
manager_tool!(ManagerUpdateAgent, "update_agent", "Update fields on an existing agent. Params: agentId (string), name?, objective?, model?, toolIds?, integrations?, permissions?.", serde_json::json!({ "agentId": { "type": "string" }, "name": { "type": "string" }, "objective": { "type": "string" }, "model": { "type": "string" }, "toolIds": { "type": "array" }, "integrations": { "type": "array" }, "permissions": { "type": "array" } }), serde_json::json!(["agentId"]));
manager_tool!(ManagerDeleteAgent, "delete_agent", "Delete an agent. Params: agentId (string).", serde_json::json!({ "agentId": { "type": "string" } }), serde_json::json!(["agentId"]));
manager_tool!(ManagerCreateWorkflow, "create_workflow", "Create a new workflow. Params: name (string), nodes (string, optional JSON), edges (string, optional JSON).", serde_json::json!({ "name": { "type": "string" }, "nodes": { "type": "string" }, "edges": { "type": "string" } }), serde_json::json!(["name"]));
manager_tool!(ManagerListWorkflows, "list_workflows", "List all workflows and their names/ids.", serde_json::json!({}), serde_json::json!([]));
manager_tool!(ManagerSearchWorkflows, "search_workflows", "Search workflows by partial name match. Params: query (string). Returns matching workflows with their names and ids — use this to find a workflow when you only remember part of its name.", serde_json::json!({ "query": { "type": "string" } }), serde_json::json!(["query"]));
manager_tool!(ManagerUpdateWorkflow, "update_workflow", "Update a workflow's nodes/edges. Params: workflowId (string), nodes (string JSON), edges (string JSON).", serde_json::json!({ "workflowId": { "type": "string" }, "nodes": { "type": "string" }, "edges": { "type": "string" } }), serde_json::json!(["workflowId"]));
manager_tool!(ManagerDeleteWorkflow, "delete_workflow", "Delete a workflow. Params: workflowId (string).", serde_json::json!({ "workflowId": { "type": "string" } }), serde_json::json!(["workflowId"]));
manager_tool!(ManagerRunWorkflow, "run_workflow", "Run a workflow with input. Params: workflowId (string), input (string).", serde_json::json!({ "workflowId": { "type": "string" }, "input": { "type": "string" } }), serde_json::json!(["workflowId", "input"]));
manager_tool!(ManagerConfigureIntegration, "configure_integration", "Configure an integration with credentials. Params: id (string, e.g. notion/airtable/sheets/docs/telegram), config (object of keys e.g. token/clientId/clientSecret). Confirm with the user before storing secrets.", serde_json::json!({ "id": { "type": "string" }, "config": { "type": "object" } }), serde_json::json!(["id", "config"]));
manager_tool!(ManagerListIntegrations, "list_integrations", "List integrations and their connected status.", serde_json::json!({}), serde_json::json!([]));
manager_tool!(ManagerTestIntegration, "test_integration", "Test an integration connection. Params: id (string).", serde_json::json!({ "id": { "type": "string" } }), serde_json::json!(["id"]));
manager_tool!(ManagerCreateTask, "create_task", "Create and run a task on an agent. Params: agentId (string), input (string), context (string, optional).", serde_json::json!({ "agentId": { "type": "string" }, "input": { "type": "string" }, "context": { "type": "string" } }), serde_json::json!(["agentId", "input"]));
manager_tool!(ManagerCancelTask, "cancel_task", "Cancel a task. Params: taskId (string).", serde_json::json!({ "taskId": { "type": "string" } }), serde_json::json!(["taskId"]));
manager_tool!(ManagerListModels, "list_models", "List configured models.", serde_json::json!({}), serde_json::json!([]));
manager_tool!(ManagerListTools, "list_tools", "List configured tools.", serde_json::json!({}), serde_json::json!([]));
manager_tool!(ManagerWorkspaceStatus, "get_workspace_status", "Get a condensed workspace status: agents, integrations, active tasks, recent runs.", serde_json::json!({}), serde_json::json!([]));

fn manager_tools() -> Vec<Box<dyn AgentTool>> {
  vec![
    Box::new(ManagerListAgents),
    Box::new(ManagerDelegate),
    Box::new(ManagerTaskStatus),
    Box::new(ManagerListTasks),
    Box::new(ManagerSwitch),
    Box::new(ManagerCreateAgent),
    Box::new(ManagerUpdateAgent),
    Box::new(ManagerDeleteAgent),
    Box::new(ManagerCreateWorkflow),
    Box::new(ManagerListWorkflows),
    Box::new(ManagerSearchWorkflows),
    Box::new(ManagerUpdateWorkflow),
    Box::new(ManagerDeleteWorkflow),
    Box::new(ManagerRunWorkflow),
    Box::new(ManagerConfigureIntegration),
    Box::new(ManagerListIntegrations),
    Box::new(ManagerTestIntegration),
    Box::new(ManagerCreateTask),
    Box::new(ManagerCancelTask),
    Box::new(ManagerListModels),
    Box::new(ManagerListTools),
    Box::new(ManagerWorkspaceStatus),
    Box::new(SearchFilesTool),
    Box::new(ReadAnyFileTool),
    Box::new(RunCommandTool),
  ]
}

// Builds the Manager's system prompt by enumerating its actual tools, so its
// capabilities are always in sync with the code and never need hand-writing.
fn build_manager_system_prompt(conn: &Connection, memory_blob: &str) -> Result<String, String> {
  let context = build_workspace_context(conn)?;
  let mut tool_list = String::new();
  for t in manager_tools() {
    tool_list.push_str(&format!("- {}: {}\n", t.name(), t.description()));
  }
  Ok(format!(
    "You are the Manager of a real, running agent workspace application. You are NOT a simulated or virtual entity â€” you have real tools and real effects on the user's machine.\n\n\
     You can actually do these things right now (do not claim you cannot):\n{tool_list}\n\
     When a tool returns a result, that result is real. When you create an agent or run a task, it really happens on the user's device.\n\n\
     Rules:\n\
     - Never say you are 'just a language model' or that you lack the ability to do something that is in your tool list. If a user asks for something you can do with your tools, do it.\n\
     - Inspect the workspace freely with read-only tools.\n\
     - When you need to change state (creating/deleting agents or workflows, configuring integrations, running tasks), CALL THE TOOL IN THIS TURN. You must emit the tool call now â€” never ask the user to type 'yes', never ask them to 'provide permissions', never request credentials in your reply, and never describe the tool you would use. The application intercepts your tool call and shows the user a confirmation popup automatically; they approve or reject there. After the tool executes, report its result. If you are not sure you are allowed to do something, call the tool anyway â€” the popup is the permission gate.\n\
     - Creating an agent requires NO credentials. Do not ask the user for API keys or 'file access credentials' when creating an agent â€” file access is just a permission value in the create_agent call.\n\
     - Never store secrets (API keys/tokens) without the user's explicit approval in the confirmation popup.\n\
     - Delegate domain work to agents rather than doing it inline.\n\n\
     Workspace context:\n{context}\n\n{memory_blob}\n\
     Return a concise, helpful reply to the user."
  ))
}

// Picks a reachable model for the Manager: prefer an enabled cloud model
// (Groq/OpenRouter, which only needs a key) before falling back to Ollama.
fn manager_default_model(conn: &Connection) -> String {
  let stmt = conn.prepare("SELECT id FROM model_configs WHERE enabled=1 ORDER BY CASE WHEN provider='ollama' THEN 1 ELSE 0 END, id LIMIT 1").ok();
  if let Some(mut stmt) = stmt {
    let rows = stmt.query_map([], |row| row.get::<_, String>(0)).ok();
    if let Some(mut rows) = rows {
      if let Some(Ok(id)) = rows.next() { return id; }
    }
  }
  // No configured models â€” return empty so callers can surface a clear error.
  String::new()
}

// Runs a Manager conversation turn: loads the Manager agent, injects workspace
// context, attaches manager tools, executes tool-calls against real backend functions.
async fn manager_turn(app: &AppHandle, message: &str) -> Result<String, String> {
  let conn = db(app)?;
  // Find the manager agent (is_manager=1); create a default if missing.
  let manager = {
    let mut stmt = conn.prepare("SELECT id, name, objective, model, tool_ids, integrations, home_path, permissions FROM agents WHERE is_manager=1 LIMIT 1").map_err(|e| e.to_string())?;
    let mut rows = stmt.query_map([], |row| {
      let tool_ids: String = row.get(4)?;
      let integrations: String = row.get(5)?;
      let permissions: String = row.get(7)?;
      Ok(AgentRequest {
        id: row.get(0)?, name: row.get(1)?, objective: row.get(2)?, model: row.get(3)?,
        tool_ids: parse_json_vec(&tool_ids), integrations: parse_json_vec(&integrations),
        home_path: row.get(6)?, permissions: parse_json_vec(&permissions),
      })
    }).map_err(|e| e.to_string())?;
    rows.next().transpose().map_err(|e| e.to_string())?
  };
  let mut manager = match manager {
    Some(m) => m,
    None => {
      let model = manager_default_model(&conn);
      if model.is_empty() {
        return Err("No model configured. Add an enabled model in the Models tab first.".into());
      }
      let m = AgentRequest { id: "manager".into(), name: "Laos".into(), objective: "You are Laos, the workspace orchestrator. Control the workspace and coordinate work.".into(), model: model.clone(), tool_ids: vec![], integrations: vec![], home_path: "agents/manager".into(), permissions: vec!["network".into()] };
      conn.execute("INSERT INTO agents (id, name, objective, model, tool_ids, integrations, memory, permissions, home_path, color, is_manager) VALUES (?1,?2,?3,?4,'[]','[]',1,?5,?6,'#22c55e',1)", params![m.id, m.name, m.objective, m.model, serde_json::to_string(&m.permissions).unwrap_or_else(|_| "[]".into()), m.home_path]).map_err(|e| e.to_string())?;
      m
    }
  };
  // Ensure the Manager uses a configured model. If the stored value isn't one
  // of the enabled model configs, pick the best enabled one (cloud before Ollama)
  // so the Manager works with whatever the user configured.
  {
    let configured = {
      let stmt = conn.prepare("SELECT id FROM model_configs WHERE enabled=1").ok();
      let mut ids = Vec::new();
      if let Some(mut stmt) = stmt {
        if let Ok(rows) = stmt.query_map([], |row| row.get::<_, String>(0)) {
          for r in rows { if let Ok(id) = r { ids.push(id); } }
        }
      }
      ids
    };
    if !configured.contains(&manager.model) {
      let best = manager_default_model(&conn);
      if best.is_empty() {
        return Err("No model configured. Add an enabled model in the Models tab first.".into());
      }
      let _ = conn.execute("UPDATE agents SET model=?1 WHERE id='manager'", params![best]);
      manager.model = best;
    }
  }

  let context = build_manager_system_prompt(&conn, "")?;
  let prompt = format!("{context}\n\nUser message: {message}");

  // Manager tools execute against real backend functions.
  let tools = manager_tools();

  let home = app.path().app_data_dir().map_err(|e| e.to_string())?.join("agents").join(&manager.id);
  let mut messages = vec![
    ChatMessage { role: "system".into(), content: Some(prompt), tool_calls: None, tool_call_id: None },
  ];
  let client = reqwest::Client::new();
  let mut final_output = String::new();
  for _round in 0..MAX_TOOL_ROUNDS {
    let body = ChatRequest { model: manager.model.clone(), messages: messages.clone(), tools: Some(tool_schemas(&tools)), stream: false };
    let response = if manager.model.starts_with("ollama:") {
      client.post("http://127.0.0.1:11434/api/chat").json(&body).send().await
    } else if manager.model.starts_with("groq:") {
      let key = api_key_for(&conn, &manager.model)?;
      client.post("https://api.groq.com/openai/v1/chat/completions")
        .header("Authorization", format!("Bearer {key}"))
        .json(&serde_json::json!({ "model": manager.model, "messages": messages, "tools": tool_schemas(&tools) })).send().await
    } else {
      let key = api_key_for(&conn, &manager.model)?;
      client.post("https://openrouter.ai/api/v1/chat/completions")
        .header("Authorization", format!("Bearer {key}"))
        .json(&serde_json::json!({ "model": manager.model, "messages": messages, "tools": tool_schemas(&tools) })).send().await
    }.map_err(|e| format!("Could not reach the model provider: {e}"))?;
    if !response.status().is_success() { return Err(format!("Model provider returned {}", response.status())); }
    let parsed: serde_json::Value = response.json().await.map_err(|e| e.to_string())?;
    // Support both Ollama (message.tool_calls) and OpenAI-compatible shapes.
    let calls_arr = parsed["message"]["tool_calls"].as_array()
      .or_else(|| parsed["choices"][0]["message"]["tool_calls"].as_array())
      .cloned()
      .unwrap_or_default();
    let assistant_msg = parsed["message"].clone();
    let assistant_msg = if assistant_msg.is_null() { parsed["choices"][0]["message"].clone() } else { assistant_msg };
    if !calls_arr.is_empty() {
      if let Ok(am) = serde_json::from_value::<ChatMessage>(assistant_msg.clone()) {
        messages.push(am);
      }
      for c in calls_arr {
        let name = c["function"]["name"].as_str().unwrap_or("").to_string();
        let args: serde_json::Value = c["function"]["arguments"].as_str().and_then(|s| serde_json::from_str(s).ok()).unwrap_or_else(|| serde_json::json!({}));
        let result = match name.as_str() {
          "list_agents" => {
            let mut out = String::new();
            let mut stmt = conn.prepare("SELECT id, name, description, objective, model, integrations FROM agents WHERE is_manager=0 ORDER BY name").map_err(|e| e.to_string())?;
            let rows = stmt.query_map([], |row| {
              let integrations: String = row.get(5)?;
              Ok(format!("- {} (id: {}): {}. Model: {}. Integrations: {}", row.get::<_, String>(1)?, row.get::<_, String>(0)?, row.get::<_, String>(2)?, row.get::<_, String>(3)?, integrations))
            }).map_err(|e| e.to_string())?;
            for row in rows { out.push_str(&row.map_err(|e| e.to_string())?); out.push('\n'); }
            Ok(if out.is_empty() { "No agents yet.".into() } else { out })
          }
          "list_tasks" => {
            let tasks = list_tasks(&conn)?;
            Ok(if tasks.is_empty() { "No tasks.".into() } else { tasks.iter().map(|t| format!("- {} â†’ {}: {} ({})", t.id, t.assigned_agent, t.input, t.status)).collect::<Vec<_>>().join("\n") })
          }
          "get_task_status" => {
            let task_id = args.get("taskId").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let mut stmt = conn.prepare("SELECT id, requester, assigned_agent, status, input, context, result, created_at, completed_at FROM tasks WHERE id=?1").map_err(|e| e.to_string())?;
            let mut rows = stmt.query_map(params![task_id], task_from_row).map_err(|e| e.to_string())?;
            match rows.next().transpose().map_err(|e| e.to_string())? {
              Some(t) => Ok(format!("{} â†’ {}: {} â€” result: {}", t.id, t.assigned_agent, t.status, t.result.unwrap_or_default())),
              None => Ok(format!("Task {task_id} not found.")),
            }
          }
          "delegate_task" => {
            let agent_id = args.get("agentId").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let task = args.get("task").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let context = args.get("context").and_then(|v| v.as_str()).unwrap_or("").to_string();
            if agent_id.is_empty() || task.is_empty() { return Err("delegate_task requires agentId and task.".into()); }
            let task_id = create_task(&conn, "manager", &agent_id, &task, &context)?;
            match delegate_task(app, &task_id, &agent_id, &task, &context).await {
              Ok(out) => Ok(format!("Task {task_id} completed. Result:\n{out}")),
              Err(e) => Ok(format!("Task {task_id} failed: {e}")),
            }
          }
          "switch_agent" => {
            let agent_id = args.get("agentId").and_then(|v| v.as_str()).unwrap_or("").to_string();
            Ok(format!("Switching conversation to agent {agent_id}. The user can now talk to that agent directly."))
          }
          "create_agent" => {
            let name = args.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let objective = args.get("objective").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let model = args.get("model").and_then(|v| v.as_str()).unwrap_or("").to_string();
            if name.is_empty() || objective.is_empty() || model.is_empty() { return Err("create_agent requires name, objective and model.".into()); }
            let arr = |k: &str| args.get(k).and_then(|v| v.as_array()).map(|a| a.iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect::<Vec<_>>()).unwrap_or_default();
            let id = format!("agent-{}", chrono::Utc::now().timestamp_millis());
            let agent = AgentRecord {
              id: id.clone(), name: name.clone(), objective, model,
              tool_ids: arr("toolIds"), integrations: arr("integrations"), memory: true,
              permissions: arr("permissions").into_iter().filter(|p| p == "network" || p == "files").collect(),
              home_path: format!("agents/{id}"), color: "#22c55e".into(), x: 100.0, y: 100.0, is_manager: false, description: "".into(), persona: "ai-orb".into(),
            };
            save_agent(app.clone(), agent)?;
            Ok(format!("Created agent '{name}' (id: {id})."))
          }
          "update_agent" => {
            let agent_id = args.get("agentId").and_then(|v| v.as_str()).unwrap_or("").to_string();
            if agent_id.is_empty() { return Err("update_agent requires agentId.".into()); }
            let existing = {
              let mut stmt = conn.prepare("SELECT id, name, objective, model, tool_ids, integrations, memory, permissions, home_path, color, x, y, is_manager, description, persona FROM agents WHERE id=?1").map_err(|e| e.to_string())?;
              let mut rows = stmt.query_map(params![agent_id], |row| {
                let tool_ids: String = row.get(4)?;
                let integrations: String = row.get(5)?;
                let permissions: String = row.get(7)?;
                Ok(AgentRecord {
                  id: row.get(0)?, name: row.get(1)?, objective: row.get(2)?, model: row.get(3)?,
                  tool_ids: parse_json_vec(&tool_ids), integrations: parse_json_vec(&integrations),
                  memory: row.get::<_, i64>(6)? != 0, permissions: parse_json_vec(&permissions),
                  home_path: row.get(8)?, color: row.get(9)?, x: row.get(10)?, y: row.get(11)?,
                  is_manager: row.get::<_, i64>(12)? != 0, description: row.get(13)?, persona: row.get(14)?,
                })
              }).map_err(|e| e.to_string())?;
              rows.next().transpose().map_err(|e| e.to_string())?
            };
            let Some(mut a) = existing else { return Err(format!("Agent {agent_id} not found.")); };
            if let Some(v) = args.get("name").and_then(|v| v.as_str()) { a.name = v.to_string(); }
            if let Some(v) = args.get("objective").and_then(|v| v.as_str()) { a.objective = v.to_string(); }
            if let Some(v) = args.get("model").and_then(|v| v.as_str()) { a.model = v.to_string(); }
            if let Some(v) = args.get("toolIds").and_then(|v| v.as_array()) { a.tool_ids = v.iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect(); }
            if let Some(v) = args.get("integrations").and_then(|v| v.as_array()) { a.integrations = v.iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect(); }
            if let Some(v) = args.get("permissions").and_then(|v| v.as_array()) { a.permissions = v.iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect(); }
            save_agent(app.clone(), a)?;
            Ok(format!("Updated agent {agent_id}."))
          }
          "delete_agent" => {
            let agent_id = args.get("agentId").and_then(|v| v.as_str()).unwrap_or("").to_string();
            conn.execute("DELETE FROM agents WHERE id=?1 AND is_manager=0", params![agent_id]).map_err(|e| e.to_string())?;
            Ok(format!("Deleted agent {agent_id}."))
          }
          "create_workflow" => {
            let name = args.get("name").and_then(|v| v.as_str()).unwrap_or("Untitled workflow").to_string();
            let nodes = args.get("nodes").and_then(|v| v.as_str()).and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok()).unwrap_or(serde_json::json!([]));
            let edges = args.get("edges").and_then(|v| v.as_str()).and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok()).unwrap_or(serde_json::json!([]));
            let wf = WorkflowRecord { id: format!("wf-{}", chrono::Utc::now().timestamp_millis()), name, nodes, edges, updated_at: now() };
            save_workflow(app.clone(), wf.clone())?;
            Ok(format!("Created workflow '{}' (id: {}).", wf.name, wf.id))
          }
          "list_workflows" => {
            let wfs = {
              let mut stmt = conn.prepare("SELECT id, name, nodes, edges, updated_at FROM workflows ORDER BY updated_at DESC").map_err(|e| e.to_string())?;
              let rows = stmt.query_map([], |row| Ok(format!("- {} (id: {})", row.get::<_, String>(1)?, row.get::<_, String>(0)?))).map_err(|e| e.to_string())?;
              let mut out = Vec::new();
              for r in rows { out.push(r.map_err(|e| e.to_string())?); }
              out
            };
            Ok(if wfs.is_empty() { "No workflows.".into() } else { wfs.join("\n") })
          }
          "search_workflows" => {
            let query = args.get("query").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let like = format!("%{}%", query);
            let wfs = {
              let mut stmt = conn.prepare("SELECT id, name, nodes, edges, updated_at FROM workflows WHERE name LIKE ?1 ORDER BY updated_at DESC").map_err(|e| e.to_string())?;
              let rows = stmt.query_map(params![like], |row| Ok(format!("- {} (id: {})", row.get::<_, String>(1)?, row.get::<_, String>(0)?))).map_err(|e| e.to_string())?;
              let mut out = Vec::new();
              for r in rows { out.push(r.map_err(|e| e.to_string())?); }
              out
            };
            Ok(if wfs.is_empty() { format!("No workflows matching \"{}\".", query) } else { wfs.join("\n") })
          }
          "update_workflow" => {
            let wf_id = args.get("workflowId").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let nodes = args.get("nodes").and_then(|v| v.as_str()).and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok()).unwrap_or(serde_json::json!([]));
            let edges = args.get("edges").and_then(|v| v.as_str()).and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok()).unwrap_or(serde_json::json!([]));
            let existing = {
              let mut stmt = conn.prepare("SELECT name FROM workflows WHERE id=?1").map_err(|e| e.to_string())?;
              let mut rows = stmt.query_map(params![wf_id], |row| row.get::<_, String>(0)).map_err(|e| e.to_string())?;
              rows.next().transpose().map_err(|e| e.to_string())?
            };
            let Some(name) = existing else { return Err(format!("Workflow {wf_id} not found.")); };
            save_workflow(app.clone(), WorkflowRecord { id: wf_id.clone(), name, nodes, edges, updated_at: now() })?;
            Ok(format!("Updated workflow {wf_id}."))
          }
          "delete_workflow" => {
            let wf_id = args.get("workflowId").and_then(|v| v.as_str()).unwrap_or("").to_string();
            conn.execute("DELETE FROM workflows WHERE id=?1", params![wf_id]).map_err(|e| e.to_string())?;
            Ok(format!("Deleted workflow {wf_id}."))
          }
          "run_workflow" => {
            let wf_id = args.get("workflowId").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let input = args.get("input").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let wf = {
              let mut stmt = conn.prepare("SELECT id, name, nodes, edges, updated_at FROM workflows WHERE id=?1").map_err(|e| e.to_string())?;
              let mut rows = stmt.query_map(params![wf_id], |row| {
                let nodes: String = row.get(2)?;
                let edges: String = row.get(3)?;
                Ok(WorkflowRecord {
                  id: row.get(0)?, name: row.get(1)?,
                  nodes: serde_json::from_str(&nodes).unwrap_or_else(|_| serde_json::json!([])),
                  edges: serde_json::from_str(&edges).unwrap_or_else(|_| serde_json::json!([])),
                  updated_at: row.get(4)?,
                })
              }).map_err(|e| e.to_string())?;
              rows.next().transpose().map_err(|e| e.to_string())?
            };
            let Some(wf) = wf else { return Err(format!("Workflow {wf_id} not found.")); };
            let exec = execute_workflow(app.clone(), wf, input, None).await?;
            Ok(format!("Workflow ran. Steps: {}; final output: {}", exec.steps.len(), exec.final_output))
          }
          "configure_integration" => {
            let id = args.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let config = args.get("config").cloned().unwrap_or(serde_json::json!({}));
            save_integration_config(app.clone(), id.clone(), config)?;
            Ok(format!("Configured integration '{id}'. It may need a test to confirm the connection."))
          }
          "list_integrations" => {
            let defs = integration_definitions();
            let mut out = Vec::new();
            for d in defs {
              let mut stmt = conn.prepare("SELECT enabled, connected FROM integration_configs WHERE id=?1").map_err(|e| e.to_string())?;
              let mut rows = stmt.query_map(params![d.id], |row| Ok((row.get::<_, i64>(0)? != 0, row.get::<_, i64>(1)? != 0))).map_err(|e| e.to_string())?;
              let (enabled, connected) = rows.next().transpose().map_err(|e| e.to_string())?.unwrap_or((false, false));
              out.push(format!("- {} (id: {}): enabled={} connected={}", d.name, d.id, enabled, connected));
            }
            Ok(out.join("\n"))
          }
          "test_integration" => {
            let id = args.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
            Ok(format!("Testing integration {id}... use the Integrations tab to see the result."))
          }
          "create_task" => {
            let agent_id = args.get("agentId").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let input = args.get("input").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let context = args.get("context").and_then(|v| v.as_str()).unwrap_or("").to_string();
            if agent_id.is_empty() || input.is_empty() { return Err("create_task requires agentId and input.".into()); }
            let task_id = create_task(&conn, "manager", &agent_id, &input, &context)?;
            match delegate_task(&app, &task_id, &agent_id, &input, &context).await {
              Ok(out) => Ok(format!("Task {task_id} completed. Result:\n{out}")),
              Err(e) => Ok(format!("Task {task_id} failed: {e}")),
            }
          }
          "cancel_task" => {
            let task_id = args.get("taskId").and_then(|v| v.as_str()).unwrap_or("").to_string();
            conn.execute("UPDATE tasks SET status='cancelled', completed_at=?1 WHERE id=?2", params![chrono::Utc::now().to_rfc3339(), task_id]).map_err(|e| e.to_string())?;
            Ok(format!("Cancelled task {task_id}."))
          }
          "list_models" => {
            let mut stmt = conn.prepare("SELECT id, provider, label FROM model_configs ORDER BY id").map_err(|e| e.to_string())?;
            let rows = stmt.query_map([], |row| Ok(format!("- {} ({})", row.get::<_, String>(2)?, row.get::<_, String>(1)?))).map_err(|e| e.to_string())?;
            let mut out = Vec::new();
            for r in rows { out.push(r.map_err(|e| e.to_string())?); }
            Ok(if out.is_empty() { "No models configured.".into() } else { out.join("\n") })
          }
          "list_tools" => {
            let mut stmt = conn.prepare("SELECT name, kind FROM tools WHERE enabled=1 ORDER BY name").map_err(|e| e.to_string())?;
            let rows = stmt.query_map([], |row| Ok(format!("- {} ({})", row.get::<_, String>(0)?, row.get::<_, String>(1)?))).map_err(|e| e.to_string())?;
            let mut out = Vec::new();
            for r in rows { out.push(r.map_err(|e| e.to_string())?); }
            Ok(if out.is_empty() { "No tools configured.".into() } else { out.join("\n") })
          }
          "get_workspace_status" => {
            let agents_n: i64 = conn.query_row("SELECT COUNT(*) FROM agents WHERE is_manager=0", [], |r| r.get(0)).unwrap_or(0);
            let tasks_n: i64 = conn.query_row("SELECT COUNT(*) FROM tasks WHERE status IN ('pending','running')", [], |r| r.get(0)).unwrap_or(0);
            let runs_n: i64 = conn.query_row("SELECT COUNT(*) FROM runs", [], |r| r.get(0)).unwrap_or(0);
            let conn_n: i64 = conn.query_row("SELECT COUNT(*) FROM integration_configs WHERE connected=1", [], |r| r.get(0)).unwrap_or(0);
            Ok(format!("Agents: {agents_n}. Connected integrations: {conn_n}. Active tasks: {tasks_n}. Total runs: {runs_n}."))
          }
          _ => Err(format!("Unknown manager tool {name}.")),
        };
        let call_id = c["id"].as_str().unwrap_or("").to_string();
        messages.push(ChatMessage { role: "assistant".into(), content: None, tool_calls: Some(vec![ToolCall { id: call_id.clone(), kind: "function".into(), function: ToolCallFunction { name: name.clone(), arguments: args.clone() } }]), tool_call_id: None });
        messages.push(ChatMessage { role: "tool".into(), content: Some(result.map_err(|e| e.to_string())?), tool_calls: None, tool_call_id: Some(call_id) });
      }
    } else {
      final_output = parsed["message"]["content"].as_str().unwrap_or("").to_string();
      break;
    }
  }
  let _ = home;
  Ok(final_output)
}

// Shared dispatch for manager tools; used by both the non-streaming manager_turn
// and the streaming stream_chat manager path.
async fn dispatch_manager_tool(app: &AppHandle, name: &str, args: &serde_json::Value) -> Result<String, String> {
  let conn = db(app)?;
  match name {
    "list_agents" => {
      let mut out = String::new();
      let mut stmt = conn.prepare("SELECT id, name, description, objective, model, integrations FROM agents WHERE is_manager=0 ORDER BY name").map_err(|e| e.to_string())?;
      let rows = stmt.query_map([], |row| {
        let integrations: String = row.get(5)?;
        Ok(format!("- {} (id: {}): {}. Model: {}. Integrations: {}", row.get::<_, String>(1)?, row.get::<_, String>(0)?, row.get::<_, String>(2)?, row.get::<_, String>(3)?, integrations))
      }).map_err(|e| e.to_string())?;
      for row in rows { out.push_str(&row.map_err(|e| e.to_string())?); out.push('\n'); }
      Ok(if out.is_empty() { "No agents yet.".into() } else { out })
    }
    "list_tasks" => {
      let tasks = list_tasks(&conn)?;
      Ok(if tasks.is_empty() { "No tasks.".into() } else { tasks.iter().map(|t| format!("- {} â†’ {}: {} ({})", t.id, t.assigned_agent, t.input, t.status)).collect::<Vec<_>>().join("\n") })
    }
    "get_task_status" => {
      let task_id = args.get("taskId").and_then(|v| v.as_str()).unwrap_or("").to_string();
      let mut stmt = conn.prepare("SELECT id, requester, assigned_agent, status, input, context, result, created_at, completed_at FROM tasks WHERE id=?1").map_err(|e| e.to_string())?;
      let mut rows = stmt.query_map(params![task_id], task_from_row).map_err(|e| e.to_string())?;
      match rows.next().transpose().map_err(|e| e.to_string())? {
        Some(t) => Ok(format!("{} â†’ {}: {} â€” result: {}", t.id, t.assigned_agent, t.status, t.result.unwrap_or_default())),
        None => Ok(format!("Task {task_id} not found.")),
      }
    }
    "delegate_task" | "create_task" => {
      let agent_id = args.get("agentId").and_then(|v| v.as_str()).unwrap_or("").to_string();
      let task = args.get("task").and_then(|v| v.as_str()).or_else(|| args.get("input").and_then(|v| v.as_str())).unwrap_or("").to_string();
      let context = args.get("context").and_then(|v| v.as_str()).unwrap_or("").to_string();
      if agent_id.is_empty() || task.is_empty() { return Err("delegate_task/create_task requires agentId and input.".into()); }
      let task_id = create_task(&conn, "manager", &agent_id, &task, &context)?;
      match delegate_task(app, &task_id, &agent_id, &task, &context).await {
        Ok(out) => Ok(format!("Task {task_id} completed. Result:\n{out}")),
        Err(e) => Ok(format!("Task {task_id} failed: {e}")),
      }
    }
    "switch_agent" => {
      let agent_id = args.get("agentId").and_then(|v| v.as_str()).unwrap_or("").to_string();
      Ok(format!("Switching conversation to agent {agent_id}. The user can now talk to that agent directly."))
    }
    "create_agent" => {
      let name = args.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
      let objective = args.get("objective").and_then(|v| v.as_str()).unwrap_or("").to_string();
      let model = args.get("model").and_then(|v| v.as_str()).unwrap_or("").to_string();
      if name.is_empty() || objective.is_empty() || model.is_empty() { return Err("create_agent requires name, objective and model.".into()); }
      let arr = |k: &str| args.get(k).and_then(|v| v.as_array()).map(|a| a.iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect::<Vec<_>>()).unwrap_or_default();
      let id = format!("agent-{}", chrono::Utc::now().timestamp_millis());
      let agent = AgentRecord {
        id: id.clone(), name: name.clone(), objective, model,
        tool_ids: arr("toolIds"), integrations: arr("integrations"), memory: true,
        permissions: arr("permissions").into_iter().filter(|p| p == "network" || p == "files").collect(),
        home_path: format!("agents/{id}"), color: "#22c55e".into(), x: 100.0, y: 100.0, is_manager: false, description: "".into(), persona: "ai-orb".into(),
      };
      save_agent(app.clone(), agent)?;
      Ok(format!("Created agent '{name}' (id: {id})."))
    }
    "delete_agent" => {
      let agent_id = args.get("agentId").and_then(|v| v.as_str()).unwrap_or("").to_string();
      conn.execute("DELETE FROM agents WHERE id=?1 AND is_manager=0", params![agent_id]).map_err(|e| e.to_string())?;
      Ok(format!("Deleted agent {agent_id}."))
    }
    "list_workflows" => {
      let mut stmt = conn.prepare("SELECT id, name FROM workflows ORDER BY updated_at DESC").map_err(|e| e.to_string())?;
      let rows = stmt.query_map([], |row| Ok(format!("- {} (id: {})", row.get::<_, String>(1)?, row.get::<_, String>(0)?))).map_err(|e| e.to_string())?;
      let mut out = Vec::new();
      for r in rows { out.push(r.map_err(|e| e.to_string())?); }
      Ok(if out.is_empty() { "No workflows.".into() } else { out.join("\n") })
    }
    "search_workflows" => {
      let query = args.get("query").and_then(|v| v.as_str()).unwrap_or("").to_string();
      let like = format!("%{}%", query);
      let mut stmt = conn.prepare("SELECT id, name FROM workflows WHERE name LIKE ?1 ORDER BY updated_at DESC").map_err(|e| e.to_string())?;
      let rows = stmt.query_map(params![like], |row| Ok(format!("- {} (id: {})", row.get::<_, String>(1)?, row.get::<_, String>(0)?))).map_err(|e| e.to_string())?;
      let mut out = Vec::new();
      for r in rows { out.push(r.map_err(|e| e.to_string())?); }
      Ok(if out.is_empty() { format!("No workflows matching \"{}\".", query) } else { out.join("\n") })
    }
    "create_workflow" => {
      let name = args.get("name").and_then(|v| v.as_str()).unwrap_or("Untitled workflow").to_string();
      let nodes = args.get("nodes").and_then(|v| v.as_str()).and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok()).unwrap_or(serde_json::json!([]));
      let edges = args.get("edges").and_then(|v| v.as_str()).and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok()).unwrap_or(serde_json::json!([]));
      let wf = WorkflowRecord { id: format!("wf-{}", chrono::Utc::now().timestamp_millis()), name, nodes, edges, updated_at: now() };
      save_workflow(app.clone(), wf.clone())?;
      Ok(format!("Created workflow '{}' (id: {}).", wf.name, wf.id))
    }
    "delete_workflow" => {
      let wf_id = args.get("workflowId").and_then(|v| v.as_str()).unwrap_or("").to_string();
      conn.execute("DELETE FROM workflows WHERE id=?1", params![wf_id]).map_err(|e| e.to_string())?;
      Ok(format!("Deleted workflow {wf_id}."))
    }
    "configure_integration" => {
      let id = args.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
      let config = args.get("config").cloned().unwrap_or(serde_json::json!({}));
      save_integration_config(app.clone(), id.clone(), config)?;
      Ok(format!("Configured integration '{id}'. It may need a test to confirm the connection."))
    }
    "list_integrations" => {
      let defs = integration_definitions();
      let mut out = Vec::new();
      for d in defs {
        let mut stmt = conn.prepare("SELECT enabled, connected FROM integration_configs WHERE id=?1").map_err(|e| e.to_string())?;
        let mut rows = stmt.query_map(params![d.id], |row| Ok((row.get::<_, i64>(0)? != 0, row.get::<_, i64>(1)? != 0))).map_err(|e| e.to_string())?;
        let (enabled, connected) = rows.next().transpose().map_err(|e| e.to_string())?.unwrap_or((false, false));
        out.push(format!("- {} (id: {}): enabled={} connected={}", d.name, d.id, enabled, connected));
      }
      Ok(out.join("\n"))
    }
    "cancel_task" => {
      let task_id = args.get("taskId").and_then(|v| v.as_str()).unwrap_or("").to_string();
      conn.execute("UPDATE tasks SET status='cancelled', completed_at=?1 WHERE id=?2", params![chrono::Utc::now().to_rfc3339(), task_id]).map_err(|e| e.to_string())?;
      Ok(format!("Cancelled task {task_id}."))
    }
    "list_models" => {
      let mut stmt = conn.prepare("SELECT id, provider, label FROM model_configs ORDER BY id").map_err(|e| e.to_string())?;
      let rows = stmt.query_map([], |row| Ok(format!("- {} ({})", row.get::<_, String>(2)?, row.get::<_, String>(1)?))).map_err(|e| e.to_string())?;
      let mut out = Vec::new();
      for r in rows { out.push(r.map_err(|e| e.to_string())?); }
      Ok(if out.is_empty() { "No models configured.".into() } else { out.join("\n") })
    }
    "get_workspace_status" => {
      let agents_n: i64 = conn.query_row("SELECT COUNT(*) FROM agents WHERE is_manager=0", [], |r| r.get(0)).unwrap_or(0);
      let tasks_n: i64 = conn.query_row("SELECT COUNT(*) FROM tasks WHERE status IN ('pending','running')", [], |r| r.get(0)).unwrap_or(0);
      let runs_n: i64 = conn.query_row("SELECT COUNT(*) FROM runs", [], |r| r.get(0)).unwrap_or(0);
      let conn_n: i64 = conn.query_row("SELECT COUNT(*) FROM integration_configs WHERE connected=1", [], |r| r.get(0)).unwrap_or(0);
      Ok(format!("Agents: {agents_n}. Connected integrations: {conn_n}. Active tasks: {tasks_n}. Total runs: {runs_n}."))
    }
    "search_files" => SearchFilesTool.run(args).await,
    "read_file_any" => ReadAnyFileTool.run(args).await,
    "run_command" => RunCommandTool.run(args).await,
    _ => Err(format!("Manager tool '{name}' not implemented.")),
  }
}

#[tauri::command]
async fn manager_message(app: AppHandle, message: String) -> Result<String, String> {
  manager_turn(&app, &message).await
}

// ---------------------------------------------------------------------------
// Telegram adapter (long-poll)
// ---------------------------------------------------------------------------

// Reads the Telegram bot token from the telegram integration config, if enabled.
fn telegram_token(conn: &Connection) -> Result<Option<String>, String> {
  let mut stmt = conn.prepare("SELECT enabled, connected, config_json FROM integration_configs WHERE id='telegram'").map_err(|e| e.to_string())?;
  let mut rows = stmt.query_map([], |row| Ok((row.get::<_, i64>(0)? != 0, row.get::<_, i64>(1)? != 0, row.get::<_, String>(2)?))).map_err(|e| e.to_string())?;
  match rows.next().transpose().map_err(|e| e.to_string())? {
    Some((enabled, connected, cfg_json)) => {
      if !enabled || !connected { return Ok(None); }
      let cfg: serde_json::Value = serde_json::from_str(&cfg_json).map_err(|e| e.to_string())?;
      Ok(cfg.get("token").and_then(|t| t.as_str()).map(|s| s.to_string()))
    }
    None => Ok(None),
  }
}

// Long-polls the Telegram Bot API; every incoming text message is routed to the
// Manager (same logic as the UI chat) and the reply is sent back. Thin channel:
// no business logic lives here.
async fn telegram_loop(app: AppHandle) {
  loop {
    let token = match db(&app).and_then(|c| telegram_token(&c)) {
      Ok(Some(t)) => t,
      _ => { std::thread::sleep(std::time::Duration::from_secs(5)); continue; }
    };
    let mut offset: i64 = 0;
    loop {
      let url = format!("https://api.telegram.org/bot{token}/getUpdates?timeout=30&offset={offset}");
      let response = match reqwest::Client::new().get(&url).send().await {
        Ok(r) => r,
        Err(_) => { std::thread::sleep(std::time::Duration::from_secs(5)); continue; }
      };
      let json: serde_json::Value = match response.json().await {
        Ok(v) => v,
        Err(_) => { std::thread::sleep(std::time::Duration::from_secs(5)); continue; }
      };
      let updates = json.get("result").and_then(|r| r.as_array()).cloned().unwrap_or_default();
      for update in updates {
        let update_id = update.get("update_id").and_then(|u| u.as_i64()).unwrap_or(0);
        offset = update_id + 1;
        let Some(text) = update.get("message").and_then(|m| m.get("text")).and_then(|t| t.as_str()).map(|s| s.to_string()) else { continue };
        let Some(chat_id) = update.get("message").and_then(|m| m.get("chat")).and_then(|c| c.get("id")).and_then(|c| c.as_i64()) else { continue };
        // Handle /agents and /tasks locally for snappy replies; everything else â†’ Manager.
        let reply = match text.trim() {
          "/agents" | "/agents@" => {
            let conn = db(&app).ok();
            match conn {
              Some(c) => {
                let ctx = build_workspace_context(&c).unwrap_or_default();
                ctx.lines().take_while(|l| !l.starts_with("## Available Integrations")).collect::<Vec<_>>().join("\n")
              }
              None => "No agents.".into(),
            }
          }
          "/tasks" | "/tasks@" => {
            let conn = db(&app).ok();
            match conn {
              Some(c) => list_tasks(&c).map(|ts| if ts.is_empty() { "No tasks.".into() } else { ts.iter().map(|t| format!("- {} â†’ {}: {} ({})", t.id, t.assigned_agent, t.input, t.status)).collect::<Vec<_>>().join("\n") }).unwrap_or_default(),
              None => "No tasks.".into(),
            }
          }
          _ => manager_turn(&app, &text).await.unwrap_or_else(|e| format!("Manager error: {e}")),
        };
        let send_url = format!("https://api.telegram.org/bot{token}/sendMessage");
        let _ = reqwest::Client::new().post(&send_url).json(&serde_json::json!({ "chat_id": chat_id, "text": reply, "parse_mode": "Markdown" })).send().await;
      }
    }
  }
}

fn api_key_for(conn: &Connection, model: &str) -> Result<String, String> {
  stored_api_key(conn, model).ok().flatten().filter(|k| !k.is_empty()).ok_or("OpenRouter requires an API key. Add it in Models first.".into())
}

// ---------------------------------------------------------------------------
// Tasks
// ---------------------------------------------------------------------------

#[tauri::command]
fn list_all_tasks(app: AppHandle) -> Result<Vec<TaskRecord>, String> {
  let conn = db(&app)?;
  list_tasks(&conn)
}

#[tauri::command]
fn get_task(app: AppHandle, id: String) -> Result<Option<TaskRecord>, String> {
  let conn = db(&app)?;
  let mut stmt = conn.prepare("SELECT id, requester, assigned_agent, status, input, context, result, created_at, completed_at FROM tasks WHERE id=?1").map_err(|e| e.to_string())?;
  let mut rows = stmt.query_map(params![id], task_from_row).map_err(|e| e.to_string())?;
  rows.next().transpose().map_err(|e| e.to_string())
}

#[tauri::command]
async fn run_task(app: AppHandle, requester: String, assigned_agent: String, input: String, context: String) -> Result<TaskRecord, String> {
  let conn = db(&app)?;
  let task_id = create_task(&conn, &requester, &assigned_agent, &input, &context)?;
  let _ = delegate_task(&app, &task_id, &assigned_agent, &input, &context).await;
  get_task(app, task_id)?.ok_or("Task disappeared".into())
}

#[tauri::command]
fn cancel_task(app: AppHandle, id: String) -> Result<(), String> {
  let conn = db(&app)?;
  conn.execute("UPDATE tasks SET status='cancelled', completed_at=?1 WHERE id=?2 AND status IN ('pending','running')", params![chrono::Utc::now().to_rfc3339(), id]).map_err(|e| e.to_string())?;
  Ok(())
}

#[tauri::command]
fn list_runs(app: AppHandle) -> Result<Vec<serde_json::Value>, String> {
  let conn = db(&app)?;
  let mut stmt = conn.prepare("SELECT id, agent_id, started_at, status, model, input, output, prompt_tokens, completion_tokens FROM runs ORDER BY started_at DESC LIMIT 200").map_err(|e| e.to_string())?;
  let rows = stmt.query_map([], |row| {
    Ok(serde_json::json!({
      "id": row.get::<_, String>(0)?,
      "agentId": row.get::<_, String>(1)?,
      "startedAt": row.get::<_, String>(2)?,
      "status": row.get::<_, String>(3)?,
      "model": row.get::<_, String>(4)?,
      "input": row.get::<_, String>(5)?,
      "output": row.get::<_, Option<String>>(6)?,
      "promptTokens": row.get::<_, i64>(7)?,
      "completionTokens": row.get::<_, i64>(8)?,
    }))
  }).map_err(|e| e.to_string())?;
  let mut out = Vec::new();
  for row in rows { out.push(row.map_err(|e| e.to_string())?); }
  Ok(out)
}

// ---------------------------------------------------------------------------
// Streaming chat
// ---------------------------------------------------------------------------

const ROLLING_WINDOW: usize = 12; // max messages (user+assistant) sent as context
const MEMORY_SUMMARY_KEY: &str = "__summary__";

// Confirmation gating for mutating manager tools: stream_chat emits a confirm
// request, and confirm_manager_tool (from the UI) records the user's decision.
struct Approval { approved: bool, edited_args: serde_json::Value }
static PENDING_APPROVALS: std::sync::OnceLock<std::sync::Mutex<std::collections::HashMap<String, Approval>>> = std::sync::OnceLock::new();
fn approvals() -> &'static std::sync::Mutex<std::collections::HashMap<String, Approval>> {
  PENDING_APPROVALS.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}

// Tools that mutate state and therefore need user confirmation.
fn requires_confirmation(tool: &str) -> bool {
  matches!(tool,
    "create_agent" | "update_agent" | "delete_agent"
    | "create_workflow" | "update_workflow" | "delete_workflow" | "run_workflow"
    | "configure_integration" | "create_task" | "cancel_task" | "delegate_task"
    | "run_command")
}

#[tauri::command]
async fn confirm_manager_tool(app: AppHandle, request_id: String, approved: bool, args: Option<serde_json::Value>, tool: String) -> Result<Option<String>, String> {
  // Record the decision; if approved, execute the tool now and return the result.
  let edited = args.unwrap_or_else(|| serde_json::json!({}));
  if approved {
    let result = dispatch_manager_tool(&app, &tool, &edited).await?;
    approvals().lock().map_err(|e| e.to_string())?.insert(request_id.clone(), Approval { approved: true, edited_args: edited });
    Ok(Some(result))
  } else {
    approvals().lock().map_err(|e| e.to_string())?.insert(request_id.clone(), Approval { approved: false, edited_args: edited });
    Ok(None)
  }
}

fn load_memory(conn: &Connection, agent_id: &str) -> Vec<(String, String)> {
  let stmt = conn.prepare("SELECT key, value FROM memory WHERE agent_id=?1 ORDER BY updated_at DESC LIMIT 20").ok();
  let mut out = Vec::new();
  if let Some(mut stmt) = stmt {
    if let Ok(rows) = stmt.query_map(params![agent_id], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))) {
      for r in rows { if let Ok(pair) = r { out.push(pair); } }
    }
  }
  out
}

// Non-streaming single-shot completion used for summarization. Returns the text.
async fn one_shot_completion(app: &AppHandle, model: &str, prompt: &str) -> Result<String, String> {
  let client = reqwest::Client::new();
  if let Some(m) = model.strip_prefix("ollama:") {
    let response = client.post("http://127.0.0.1:11434/api/generate")
      .json(&serde_json::json!({ "model": m, "prompt": prompt, "stream": false }))
      .send().await.map_err(|e| e.to_string())?;
    let json: serde_json::Value = response.json().await.map_err(|e| e.to_string())?;
    Ok(json["response"].as_str().unwrap_or("").to_string())
  } else {
    let conn = db(app)?;
    let (url, api_model, key) = if model.starts_with("groq:") {
      let k = stored_api_key(&conn, model).ok().flatten().ok_or("Groq requires an API key.")?;
      ("https://api.groq.com/openai/v1/chat/completions".to_string(), model.trim_start_matches("groq:").to_string(), k)
    } else {
      let k = stored_api_key(&conn, model).ok().flatten().ok_or("OpenRouter requires an API key.")?;
      ("https://openrouter.ai/api/v1/chat/completions".to_string(), model.trim_start_matches("openrouter:").to_string(), k)
    };
    let response = client.post(&url)
      .header("Authorization", format!("Bearer {key}"))
      .json(&serde_json::json!({ "model": api_model, "messages": [{ "role": "user", "content": prompt }] }))
      .send().await.map_err(|e| e.to_string())?;
    let json: serde_json::Value = response.json().await.map_err(|e| e.to_string())?;
    Ok(json["choices"][0]["message"]["content"].as_str().unwrap_or("").to_string())
  }
}

// Compresses turns older than the window into a memory summary and stores any
// notable facts. Called once when history first exceeds the window.
async fn summarize_old_turns(app: &AppHandle, agent: &AgentRequest, history: &[serde_json::Value]) -> Result<(), String> {
  let conn = db(app)?;
  // Only summarize the turns older than the window.
  if history.len() <= ROLLING_WINDOW { return Ok(()); }
  let old: Vec<&serde_json::Value> = history.iter().take(history.len() - ROLLING_WINDOW).collect();
  let transcript: String = old.iter().filter_map(|m| {
    let role = m.get("role").and_then(|r| r.as_str()).unwrap_or("");
    let content = m.get("content").and_then(|c| c.as_str()).unwrap_or("");
    if content.is_empty() { None } else { Some(format!("{role}: {content}\n")) }
  }).collect();
  if transcript.trim().is_empty() { return Ok(()); }

  let prompt = format!(
    "Summarize this conversation into a short memory summary (max 150 words) capturing decisions, requests, results, and anything important to remember. Also list 2-5 key facts as bullet points.\n\nConversation:\n{transcript}\n\nReply as:\nSUMMARY: <summary>\nFACTS:\n- fact1\n- fact2"
  );
  if let Ok(reply) = one_shot_completion(app, &agent.model, &prompt).await {
    let (summary, facts) = match reply.split_once("FACTS:") {
      Some((s, f)) => (s.trim().trim_start_matches("SUMMARY:").trim().to_string(), f.to_string()),
      None => (reply.trim().to_string(), String::new()),
    };
    let ts = chrono::Utc::now().to_rfc3339();
    let _ = conn.execute(
      "INSERT INTO memory (agent_id, key, value, updated_at) VALUES (?1,?2,?3,?4)
       ON CONFLICT(agent_id,key) DO UPDATE SET value=excluded.value, updated_at=excluded.updated_at",
      params![agent.id, MEMORY_SUMMARY_KEY, summary, ts],
    );
    for line in facts.lines() {
      let fact = line.trim().trim_start_matches('-').trim();
      if fact.is_empty() { continue; }
      let key = format!("fact:{}", fact.chars().take(40).collect::<String>());
      let _ = conn.execute(
        "INSERT INTO memory (agent_id, key, value, updated_at) VALUES (?1,?2,?3,?4)
         ON CONFLICT(agent_id,key) DO UPDATE SET value=excluded.value, updated_at=excluded.updated_at",
        params![agent.id, key, fact, ts],
      );
    }
  }
  Ok(())
}

fn load_conversation(conn: &Connection, agent_id: &str) -> Vec<serde_json::Value> {
  let stmt = conn.prepare("SELECT messages FROM agent_conversations WHERE agent_id=?1").ok();
  if let Some(mut stmt) = stmt {
    let rows = stmt.query_map(params![agent_id], |row| row.get::<_, String>(0)).ok();
    if let Some(mut rows) = rows {
      if let Some(Ok(s)) = rows.next() {
        if let Ok(v) = serde_json::from_str::<Vec<serde_json::Value>>(&s) { return v; }
      }
    }
  }
  Vec::new()
}

fn save_conversation(conn: &Connection, agent_id: &str, messages: &[serde_json::Value]) -> Result<(), String> {
  conn.execute(
    "INSERT INTO agent_conversations (agent_id, messages) VALUES (?1,?2)
     ON CONFLICT(agent_id) DO UPDATE SET messages=excluded.messages",
    params![agent_id, serde_json::to_string(messages).map_err(|e| e.to_string())?],
  ).map_err(|e| e.to_string())?;
  Ok(())
}

#[tauri::command]
fn get_conversation(app: AppHandle, agent_id: String) -> Result<Vec<serde_json::Value>, String> {
  let conn = db(&app)?;
  Ok(load_conversation(&conn, &agent_id))
}

#[tauri::command]
fn clear_agent_memory(app: AppHandle, agent_id: String) -> Result<(), String> {
  let conn = db(&app)?;
  conn.execute("DELETE FROM memory WHERE agent_id=?1", params![agent_id]).map_err(|e| e.to_string())?;
  conn.execute("DELETE FROM agent_conversations WHERE agent_id=?1", params![agent_id]).map_err(|e| e.to_string())?;
  Ok(())
}

// ---------------------------------------------------------------------------
// Chat sessions (bifurcated history per agent)
// ---------------------------------------------------------------------------

#[tauri::command]
fn list_chat_sessions(app: AppHandle, agent_id: String) -> Result<Vec<serde_json::Value>, String> {
  let conn = db(&app)?;
  let mut stmt = conn.prepare("SELECT id, title, created_at, updated_at FROM chat_sessions WHERE agent_id=?1 ORDER BY updated_at DESC LIMIT 100").map_err(|e| e.to_string())?;
  let rows = stmt.query_map(params![agent_id], |row| {
    Ok(serde_json::json!({ "id": row.get::<_, String>(0)?, "title": row.get::<_, String>(1)?, "createdAt": row.get::<_, String>(2)?, "updatedAt": row.get::<_, String>(3)? }))
  }).map_err(|e| e.to_string())?;
  let mut out = Vec::new();
  for row in rows { out.push(row.map_err(|e| e.to_string())?); }
  Ok(out)
}

#[tauri::command]
fn get_chat_session(app: AppHandle, session_id: String) -> Result<Vec<serde_json::Value>, String> {
  let conn = db(&app)?;
  let mut stmt = conn.prepare("SELECT role, content, time FROM chat_messages WHERE session_id=?1 ORDER BY id").map_err(|e| e.to_string())?;
  let rows = stmt.query_map(params![session_id], |row| {
    Ok(serde_json::json!({ "role": row.get::<_, String>(0)?, "content": row.get::<_, String>(1)?, "time": row.get::<_, String>(2)? }))
  }).map_err(|e| e.to_string())?;
  let mut out = Vec::new();
  for row in rows { out.push(row.map_err(|e| e.to_string())?); }
  Ok(out)
}

#[tauri::command]
fn create_chat_session(app: AppHandle, agent_id: String, title: String) -> Result<serde_json::Value, String> {
  let conn = db(&app)?;
  let id = format!("sess-{}", chrono::Utc::now().timestamp_millis());
  let now = chrono::Utc::now().to_rfc3339();
  let title = if title.trim().is_empty() { "Chat".to_string() } else { title };
  conn.execute("INSERT INTO chat_sessions (id, agent_id, title, created_at, updated_at) VALUES (?1,?2,?3,?4,?4)", params![id, agent_id, title, now]).map_err(|e| e.to_string())?;
  Ok(serde_json::json!({ "id": id, "title": title, "createdAt": now, "updatedAt": now }))
}

#[tauri::command]
fn delete_chat_session(app: AppHandle, session_id: String) -> Result<(), String> {
  let conn = db(&app)?;
  conn.execute("DELETE FROM chat_messages WHERE session_id=?1", params![session_id]).map_err(|e| e.to_string())?;
  conn.execute("DELETE FROM chat_sessions WHERE id=?1", params![session_id]).map_err(|e| e.to_string())?;
  Ok(())
}

#[tauri::command]
fn rename_chat_session(app: AppHandle, session_id: String, title: String) -> Result<(), String> {
  let conn = db(&app)?;
  conn.execute("UPDATE chat_sessions SET title=?1 WHERE id=?2", params![title, session_id]).map_err(|e| e.to_string())?;
  Ok(())
}

// Appends a message to a session (creates the session if missing).
fn append_session_message(conn: &Connection, session_id: &str, agent_id: &str, role: &str, content: &str) -> Result<(), String> {
  let now = chrono::Utc::now().to_rfc3339();
  conn.execute("INSERT INTO chat_sessions (id, agent_id, title, created_at, updated_at) VALUES (?1,?2,'Chat',?3,?3) ON CONFLICT(id) DO NOTHING", params![session_id, agent_id, now]).map_err(|e| e.to_string())?;
  conn.execute("INSERT INTO chat_messages (session_id, role, content, time) VALUES (?1,?2,?3,'')", params![session_id, role, content]).map_err(|e| e.to_string())?;
  conn.execute("UPDATE chat_sessions SET updated_at=?1 WHERE id=?2", params![now, session_id]).map_err(|e| e.to_string())?;
  Ok(())
}

fn today_key() -> String { chrono::Local::now().format("%Y-%m-%d").to_string() }

fn yesterday_key() -> String { (chrono::Local::now() - chrono::Duration::days(1)).format("%Y-%m-%d").to_string() }

// Loads the memory facts ("brain") for an agent, excluding the summary pseudo-key.
fn load_memory_facts(conn: &Connection, agent_id: &str) -> Vec<(String, String)> {
  load_memory(conn, agent_id).into_iter().filter(|(k, _)| k != MEMORY_SUMMARY_KEY).collect()
}

// Summarizes a closed chat session (via the model), stores the summary on the
// session, and folds it into the agent's day context. Returns the summary text.
async fn close_chat_session(app: &AppHandle, session_id: &str, agent_id: &str, model: &str) -> Result<String, String> {
  let conn = db(app)?;
  let transcript: Vec<String> = {
    let mut stmt = conn.prepare("SELECT role, content FROM chat_messages WHERE session_id=?1 ORDER BY id").map_err(|e| e.to_string())?;
    let rows = stmt.query_map(params![session_id], |row| Ok(format!("{}: {}", row.get::<_, String>(0)?, row.get::<_, String>(1)?))).map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for r in rows { out.push(r.map_err(|e| e.to_string())?); }
    out
  };
  if transcript.is_empty() { return Ok(String::new()); }
  let joined = transcript.join("\n");
  let prompt = format!(
    "Summarize this chat conversation as a compact memory for the agent. Capture what was discussed, decided, requested, and any open threads. Max 200 words.\n\nConversation:\n{joined}"
  );
  let summary = one_shot_completion(app, model, &prompt).await.unwrap_or_default();
  if !summary.is_empty() {
    let day = today_key();
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute("UPDATE chat_sessions SET summary=?1 WHERE id=?2", params![summary, session_id]).map_err(|e| e.to_string())?;
    // Fold into today's day-context (append, capped).
    let existing: String = conn.query_row("SELECT summary FROM day_contexts WHERE agent_id=?1 AND day=?2", params![agent_id, day], |r| r.get(0)).unwrap_or_default();
    let merged = if existing.is_empty() { summary.clone() } else { format!("{existing}\n{summary}") };
    let merged = merged.chars().take(3000).collect::<String>();
    conn.execute(
      "INSERT INTO day_contexts (agent_id, day, summary, updated_at) VALUES (?1,?2,?3,?4) ON CONFLICT(agent_id,day) DO UPDATE SET summary=excluded.summary, updated_at=excluded.updated_at",
      params![agent_id, day, merged, now],
    ).map_err(|e| e.to_string())?;
  }
  Ok(summary)
}

#[tauri::command]
async fn close_session(app: AppHandle, session_id: String, agent_id: String, model: String) -> Result<(), String> {
  let _ = close_chat_session(&app, &session_id, &agent_id, &model).await;
  Ok(())
}

// Assembles the full context bundle: memory facts, last chat summary, today's
// context, yesterday's context. Injected before the rolling window each turn.
fn build_context_bundle(conn: &Connection, agent_id: &str) -> Result<String, String> {
  let facts = load_memory_facts(conn, agent_id);
  let mut out = String::new();
  if !facts.is_empty() {
    out.push_str("## Long-term memory (facts learned about you):\n");
    for (k, v) in &facts {
      let label = k.strip_prefix("fact:").unwrap_or(k);
      out.push_str(&format!("- {label}: {v}\n"));
    }
  }
  // Last chat's condensed summary.
  let last_chat: Option<String> = conn.query_row(
    "SELECT summary FROM chat_sessions WHERE agent_id=?1 AND summary != '' ORDER BY updated_at DESC LIMIT 1",
    params![agent_id], |r| r.get(0)).ok();
  if let Some(s) = last_chat { if !s.is_empty() { out.push_str(&format!("\n## Last chat summary:\n{s}\n")); } }
  // Today's context.
  let today: Option<String> = conn.query_row("SELECT summary FROM day_contexts WHERE agent_id=?1 AND day=?2", params![agent_id, today_key()], |r| r.get(0)).ok();
  if let Some(s) = today { if !s.is_empty() { out.push_str(&format!("\n## Today's context:\n{s}\n")); } }
  // Yesterday's context.
  let yday: Option<String> = conn.query_row("SELECT summary FROM day_contexts WHERE agent_id=?1 AND day=?2", params![agent_id, yesterday_key()], |r| r.get(0)).ok();
  if let Some(s) = yday { if !s.is_empty() { out.push_str(&format!("\n## Yesterday's context:\n{s}\n")); } }
  Ok(out)
}

// Streams a chat completion from the configured provider as token deltas.
// Supports the Manager (is_manager agent) and regular agents. Ollama uses NDJSON
// (stream:true); Groq/OpenRouter use SSE `data:` lines. Context is a rolling
// window of the last ROLLING_WINDOW persisted messages.
#[tauri::command]
async fn stream_chat(app: AppHandle, agent: AgentRequest, input: String, is_manager: bool, on_event: tauri::ipc::Channel<String>, session_id: Option<String>) -> Result<(), String> {
  let conn = db(&app)?;
  let client = reqwest::Client::new();

  // Rolling window: load history, append the new user turn. If the history has
  // grown past the window, compress the older turns into memory (summary + facts).
  let mut history = load_conversation(&conn, &agent.id);
  history.push(serde_json::json!({ "role": "user", "content": input }));
  if history.len() > ROLLING_WINDOW + 1 {
    let _ = summarize_old_turns(&app, &agent, &history).await;
  }

  // Build memory context (summary + facts) for the system prompt.
  let memory = load_memory(&conn, &agent.id);
  let mut memory_blob = String::new();
  for (k, v) in &memory {
    if k == MEMORY_SUMMARY_KEY {
      memory_blob.push_str(&format!("[Memory summary: {v}]\n"));
    } else if let Some(fact) = k.strip_prefix("fact:") {
      memory_blob.push_str(&format!("- {fact}: {v}\n"));
    }
  }
  // Long-term memory: facts + last chat + today's and yesterday's condensed context.
  let context_bundle = build_context_bundle(&conn, &agent.id).unwrap_or_default();
  let full_memory = format!("{memory_blob}{context_bundle}");

  // Build the system prompt (workspace context for the Manager).
  let system = if is_manager {
    build_manager_system_prompt(&conn, &full_memory)?
  } else {
    format!("You are {}. Objective: {}\n\n{full_memory}\nReturn a helpful, direct answer.", agent.name, agent.objective)
  };

  let window: Vec<serde_json::Value> = history.iter().rev().take(ROLLING_WINDOW).cloned().collect::<Vec<_>>().into_iter().rev().collect();
  let mut messages: Vec<serde_json::Value> = vec![serde_json::json!({ "role": "system", "content": system })];
  messages.extend(window);

  let model = agent.model.clone();

  let (url, auth, key) = if let Some(m) = model.strip_prefix("ollama:") {
    (format!("http://127.0.0.1:11434/api/chat"), None::<String>, m.to_string())
  } else if let Some(m) = model.strip_prefix("groq:") {
    let k = stored_api_key(&conn, &model).ok().flatten().ok_or("Groq requires an API key.")?;
    ("https://api.groq.com/openai/v1/chat/completions".into(), Some(k.clone()), m.to_string())
  } else if let Some(m) = model.strip_prefix("openrouter:") {
    let k = stored_api_key(&conn, &model).ok().flatten().ok_or("OpenRouter requires an API key.")?;
    ("https://openrouter.ai/api/v1/chat/completions".into(), Some(k.clone()), m.to_string())
  } else { return Err("Unknown model provider.".into()); };

  // For the Manager, run tool-call rounds first (app control: create agents,
  // workflows, configure integrations, delegate tasks), then stream the final reply.
  if is_manager {
    let tools = manager_tools();
    for _ in 0..MAX_TOOL_ROUNDS {
      let body = serde_json::json!({
        "model": key,
        "messages": messages,
        "tools": tool_schemas(&tools),
        "stream": false,
      });
      let mut req = client.post(&url).json(&body);
      if let Some(auth) = &auth { req = req.header("Authorization", format!("Bearer {auth}")); }
      let response = req.send().await.map_err(|e| format!("Could not reach the model provider: {e}"))?;
      if !response.status().is_success() { return Err(format!("Model provider returned {}", response.status())); }
      let parsed: serde_json::Value = response.json().await.map_err(|e| e.to_string())?;
      let mut tool_calls: Vec<(String, serde_json::Value)> = Vec::new();
      // Support both Ollama (message.tool_calls) and OpenAI-compatible
      // (choices[0].message.tool_calls) response shapes.
      let calls_arr = parsed["message"]["tool_calls"].as_array()
        .or_else(|| parsed["choices"][0]["message"]["tool_calls"].as_array())
        .cloned()
        .unwrap_or_default();
      for c in calls_arr {
        if let (Some(name), Some(arguments)) = (c["function"]["name"].as_str(), c["function"]["arguments"].as_str()) {
          let args = serde_json::from_str(arguments).unwrap_or_else(|_| serde_json::json!({}));
          tool_calls.push((name.to_string(), args));
        }
      }
      if tool_calls.is_empty() { break; }
      // Append the assistant tool-call message (correct shape for the provider), then tool results.
      let assistant_msg = parsed["message"].clone();
      let assistant_msg = if assistant_msg.is_null() { parsed["choices"][0]["message"].clone() } else { assistant_msg };
      messages.push(assistant_msg);
      for (name, args) in tool_calls {
        if requires_confirmation(&name) {
          // Emit a confirmation request and wait for the user's decision.
          let request_id = format!("req-{}", chrono::Utc::now().timestamp_millis());
          let event = serde_json::json!({ "type": "confirm", "requestId": request_id, "tool": name, "args": args });
          on_event.send(event.to_string()).map_err(|e| e.to_string())?;
          let decision = {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
            loop {
              let mut map = approvals().lock().map_err(|e| e.to_string())?;
              if let Some(a) = map.remove(&request_id) {
                break a;
              }
              drop(map);
              if std::time::Instant::now() > deadline { break Approval { approved: false, edited_args: serde_json::json!({}) }; }
              std::thread::sleep(std::time::Duration::from_millis(200));
            }
          };
          let result = if decision.approved {
            dispatch_manager_tool(&app, &name, &decision.edited_args).await.unwrap_or_else(|e| e)
          } else {
            "The user declined this action.".to_string()
          };
          messages.push(serde_json::json!({ "role": "tool", "content": result }));
        } else {
          let result = dispatch_manager_tool(&app, &name, &args).await.unwrap_or_else(|e| e);
          messages.push(serde_json::json!({ "role": "tool", "content": result }));
        }
      }
    }
  }

  let is_ollama = url.contains("11434");
  let mut req = client.post(&url).json(&serde_json::json!({
    "model": key,
    "messages": messages,
    "stream": true,
  }));
  if let Some(auth) = auth { req = req.header("Authorization", format!("Bearer {auth}")); }

  let response = req.send().await.map_err(|e| format!("Could not reach the model provider: {e}"))?;
  if !response.status().is_success() {
    return Err(format!("Model provider returned {}", response.status()));
  }
  let mut stream = response.bytes_stream();
  let mut buffer = String::new();
  let mut delta = String::new();
  while let Some(chunk) = stream.next().await {
    let chunk = chunk.map_err(|e| e.to_string())?;
    buffer.push_str(&String::from_utf8_lossy(&chunk));
    // Ollama NDJSON: one JSON object per line. OpenAI-compatible: SSE `data:` lines.
    while let Some(pos) = buffer.find('\n') {
      let line: String = buffer.drain(..=pos).collect();
      let line = line.trim();
      if line.is_empty() { continue; }
      if is_ollama {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(line) {
          if let Some(d) = v["message"]["content"].as_str() { delta.push_str(d); on_event.send(d.to_string()).map_err(|e| e.to_string())?; }
        }
      } else {
        let data = line.strip_prefix("data:").map(|s| s.trim()).unwrap_or(line);
        if data == "[DONE]" { break; }
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(data) {
          if let Some(d) = v["choices"][0]["delta"]["content"].as_str() { delta.push_str(d); on_event.send(d.to_string()).map_err(|e| e.to_string())?; }
        }
      }
    }
  }
  // Persist the run (tokens unknown when streaming; store 0) and the conversation.
  let run_id = format!("{}-{}", agent.id, chrono::Utc::now().timestamp_millis());
  let started = chrono::Utc::now().to_rfc3339();
  let _ = conn.execute("INSERT INTO runs (id,agent_id,started_at,status,model,input,output,prompt_tokens,completion_tokens) VALUES (?1,?2,?3,'completed',?4,?5,?6,0,0)", params![run_id, agent.id, started, agent.model, input, delta]);
  if !delta.is_empty() {
    history.push(serde_json::json!({ "role": "assistant", "content": delta }));
    let _ = save_conversation(&conn, &agent.id, &history);
    // Also append to the chat session (bifurcated history) if one is active.
    if let Some(sess) = &session_id {
      let _ = append_session_message(&conn, sess, &agent.id, "user", &input);
      let _ = append_session_message(&conn, sess, &agent.id, "assistant", &delta);
    }
  }
  Ok(())
}

fn main() {
  tauri::Builder::default()
    .plugin(tauri_plugin_opener::init())
    .setup(|app| {
      // Start the Telegram long-poll adapter (no-op until a token is configured).
      let handle = app.handle().clone();
      tauri::async_runtime::spawn(telegram_loop(handle));
      Ok(())
    })
    .invoke_handler(tauri::generate_handler![initialize_storage, list_model_configs, save_model_config, delete_model_config, list_tools, save_tool, delete_tool, list_agents, save_agent, delete_agent, list_workflows, save_workflow, delete_workflow, list_integrations, save_integration_config, test_integration, start_oauth, connect_oauth, complete_oauth, execute_agent, execute_workflow, manager_message, list_all_tasks, get_task, run_task, cancel_task, list_runs, stream_chat, get_conversation, clear_agent_memory, confirm_manager_tool, list_chat_sessions, get_chat_session, create_chat_session, delete_chat_session, rename_chat_session, close_session])
    .run(tauri::generate_context!())
    .expect("error while running Local Agent OS");
}
