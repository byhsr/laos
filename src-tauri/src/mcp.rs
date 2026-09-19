// MCP connector: speaks JSON-RPC 2.0 over stdio to a Model Context Protocol
// server process, so any server (Notion, filesystem, GitHub, …) can expose its
// tools to agents. One process is spawned per operation — simple, and nothing
// leaks between calls. The transport is isolated in spawn()/request(), so a
// remote-HTTP transport can be added here later without touching callers.

use rusqlite::{params, Connection};
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::AppHandle;

use crate::db::{db, now};
use crate::models::*;

// Hard cap on a single MCP operation; the watchdog kills a hung server.
const RPC_TIMEOUT: Duration = Duration::from_secs(45);
const PROTOCOL_VERSION: &str = "2024-11-05";
const MASK: &str = "••••••••";

#[derive(Clone)]
pub(crate) struct McpServer {
  pub(crate) id: String,
  pub(crate) name: String,
  pub(crate) command: String,
  pub(crate) args: Vec<String>,
  pub(crate) env: Vec<(String, String)>,
}

fn str_vec(v: &serde_json::Value) -> Vec<String> {
  v.as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect()).unwrap_or_default()
}

fn env_pairs(v: &serde_json::Value) -> Vec<(String, String)> {
  v.as_object().map(|o| o.iter().filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string()))).collect()).unwrap_or_default()
}

fn parse_args(s: &str) -> serde_json::Value {
  serde_json::from_str(s).unwrap_or_else(|_| serde_json::json!([]))
}

fn parse_env(s: &str) -> serde_json::Value {
  serde_json::from_str(s).unwrap_or_else(|_| serde_json::json!({}))
}

// Environment variables are the usual place for tokens (e.g. NOTION_TOKEN).
fn is_secret_key(k: &str) -> bool {
  let lk = k.to_lowercase();
  lk.contains("token") || lk.contains("key") || lk.contains("secret") || lk.contains("password")
}

// Never hand real secret values to the frontend — only a "set" marker.
fn mask_env(env: &serde_json::Value) -> serde_json::Value {
  let mut out = serde_json::Map::new();
  if let Some(obj) = env.as_object() {
    for (k, v) in obj {
      if is_secret_key(k) && v.as_str().map(|s| !s.is_empty()).unwrap_or(false) {
        out.insert(k.clone(), serde_json::json!(MASK));
      } else {
        out.insert(k.clone(), v.clone());
      }
    }
  }
  serde_json::Value::Object(out)
}

// Keeps stored secrets when the UI echoes the mask back or sends an empty value.
fn merge_env(existing: &serde_json::Value, incoming: &serde_json::Value) -> serde_json::Value {
  let mut out = existing.as_object().cloned().unwrap_or_default();
  if let Some(inc) = incoming.as_object() {
    for (k, v) in inc {
      let keep = v.as_str().map(|s| s == MASK || s.is_empty()).unwrap_or(v.is_null());
      if keep { continue; }
      out.insert(k.clone(), v.clone());
    }
  }
  serde_json::Value::Object(out)
}

pub(crate) fn load_server(conn: &Connection, id: &str) -> Result<McpServer, String> {
  let mut stmt = conn.prepare("SELECT id, name, command, args, env FROM mcp_servers WHERE id=?1").map_err(|e| e.to_string())?;
  let mut rows = stmt.query_map(params![id], |row| {
    let args: String = row.get(3)?;
    let env: String = row.get(4)?;
    Ok(McpServer {
      id: row.get(0)?, name: row.get(1)?, command: row.get(2)?,
      args: str_vec(&parse_args(&args)),
      env: env_pairs(&parse_env(&env)),
    })
  }).map_err(|e| e.to_string())?;
  match rows.next().transpose().map_err(|e| e.to_string())? {
    Some(s) => Ok(s),
    None => Err(format!("MCP server '{id}' does not exist.")),
  }
}

// Spawns the server with a watchdog that kills it after RPC_TIMEOUT. The handles
// are taken out of the Child so reading never contends with the watchdog's lock.
fn spawn(server: &McpServer) -> Result<(Arc<Mutex<Child>>, ChildStdin, BufReader<ChildStdout>), String> {
  // On Windows a bare name only resolves to an .exe, so command shims like
  // `npx` (npx.cmd) or `uvx` need cmd /C — the same approach as run_command.
  #[cfg(windows)]
  let mut cmd = {
    let mut c = Command::new("cmd");
    c.arg("/C").arg(&server.command);
    c
  };
  #[cfg(not(windows))]
  let mut cmd = Command::new(&server.command);
  cmd.args(&server.args).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null());
  for (k, v) in &server.env { cmd.env(k, v); }
  let mut child = cmd.spawn().map_err(|e| format!("Could not start '{}': {e}", server.command))?;
  let stdin = child.stdin.take().ok_or("MCP server has no stdin")?;
  let stdout = child.stdout.take().ok_or("MCP server has no stdout")?;
  let child = Arc::new(Mutex::new(child));
  let killer = child.clone();
  std::thread::spawn(move || {
    std::thread::sleep(RPC_TIMEOUT);
    if let Ok(mut c) = killer.lock() { let _ = c.kill(); }
  });
  Ok((child, stdin, BufReader::new(stdout)))
}

fn stop(child: &Arc<Mutex<Child>>) {
  if let Ok(mut c) = child.lock() { let _ = c.kill(); let _ = c.wait(); }
}

// Writes one request and reads until its response id arrives, skipping
// notifications and any unrelated traffic.
fn request(stdin: &mut ChildStdin, reader: &mut BufReader<ChildStdout>, id: u64, method: &str, params: serde_json::Value) -> Result<serde_json::Value, String> {
  let mut msg = serde_json::json!({ "jsonrpc": "2.0", "id": id, "method": method });
  if !params.is_null() { msg["params"] = params; }
  let mut line = serde_json::to_string(&msg).map_err(|e| e.to_string())?;
  line.push('\n');
  stdin.write_all(line.as_bytes()).map_err(|e| format!("MCP write failed: {e}"))?;
  stdin.flush().map_err(|e| e.to_string())?;

  let mut buf = String::new();
  loop {
    buf.clear();
    let n = reader.read_line(&mut buf).map_err(|e| format!("MCP read failed: {e}"))?;
    if n == 0 { return Err(format!("The MCP server closed the connection during '{method}'.")); }
    let trimmed = buf.trim();
    if trimmed.is_empty() { continue; }
    let Ok(v) = serde_json::from_str::<serde_json::Value>(trimmed) else { continue };
    if v.get("id").and_then(|i| i.as_u64()) != Some(id) { continue; }
    if let Some(err) = v.get("error") {
      let m = err.get("message").and_then(|m| m.as_str()).unwrap_or("unknown error");
      return Err(format!("MCP '{method}' failed: {m}"));
    }
    return Ok(v.get("result").cloned().unwrap_or(serde_json::Value::Null));
  }
}

fn notify(stdin: &mut ChildStdin, method: &str) -> Result<(), String> {
  let mut line = serde_json::to_string(&serde_json::json!({ "jsonrpc": "2.0", "method": method })).map_err(|e| e.to_string())?;
  line.push('\n');
  stdin.write_all(line.as_bytes()).map_err(|e| e.to_string())?;
  stdin.flush().map_err(|e| e.to_string())
}

// initialize + notifications/initialized — required before list or call.
fn handshake(stdin: &mut ChildStdin, reader: &mut BufReader<ChildStdout>) -> Result<(), String> {
  request(stdin, reader, 1, "initialize", serde_json::json!({
    "protocolVersion": PROTOCOL_VERSION,
    "capabilities": {},
    "clientInfo": { "name": "local-agent-os", "version": env!("CARGO_PKG_VERSION") },
  }))?;
  notify(stdin, "notifications/initialized")
}

// Lists the tools a server advertises.
pub(crate) fn list_tools(server: &McpServer) -> Result<Vec<McpToolInfo>, String> {
  let (child, mut stdin, mut reader) = spawn(server)?;
  let result = (|| {
    handshake(&mut stdin, &mut reader)?;
    let res = request(&mut stdin, &mut reader, 2, "tools/list", serde_json::json!({}))?;
    let mut out = Vec::new();
    for t in res.get("tools").and_then(|t| t.as_array()).cloned().unwrap_or_default() {
      let name = t.get("name").and_then(|n| n.as_str()).unwrap_or("").to_string();
      if name.is_empty() { continue; }
      out.push(McpToolInfo {
        name,
        description: t.get("description").and_then(|d| d.as_str()).unwrap_or("").to_string(),
        schema: t.get("inputSchema").cloned().unwrap_or_else(|| serde_json::json!({ "type": "object", "properties": {} })),
      });
    }
    Ok(out)
  })();
  stop(&child);
  result
}

// Invokes a tool and flattens its content blocks into text.
pub(crate) fn call_tool(server: &McpServer, tool: &str, arguments: serde_json::Value) -> Result<String, String> {
  let (child, mut stdin, mut reader) = spawn(server)?;
  let result = (|| {
    handshake(&mut stdin, &mut reader)?;
    let res = request(&mut stdin, &mut reader, 2, "tools/call", serde_json::json!({ "name": tool, "arguments": arguments }))?;
    let mut text = String::new();
    for block in res.get("content").and_then(|c| c.as_array()).cloned().unwrap_or_default() {
      match block.get("type").and_then(|t| t.as_str()) {
        Some("text") => { if let Some(t) = block.get("text").and_then(|t| t.as_str()) { text.push_str(t); text.push('\n'); } }
        Some(other) => { text.push_str(&format!("[{other} content]\n")); }
        None => {}
      }
    }
    if res.get("isError").and_then(|e| e.as_bool()).unwrap_or(false) {
      return Err(if text.trim().is_empty() { format!("'{tool}' reported an error.") } else { text.trim().to_string() });
    }
    Ok(if text.trim().is_empty() { "The tool returned no content.".to_string() } else { text.trim().to_string() })
  })();
  stop(&child);
  result
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn list_mcp_servers(app: AppHandle) -> Result<Vec<McpServerRecord>, String> {
  let conn = db(&app)?;
  let mut stmt = conn.prepare("SELECT id, name, command, args, env, enabled FROM mcp_servers ORDER BY name").map_err(|e| e.to_string())?;
  let rows = stmt.query_map([], |row| {
    let args: String = row.get(3)?;
    let env: String = row.get(4)?;
    Ok(McpServerRecord {
      id: row.get(0)?, name: row.get(1)?, command: row.get(2)?,
      args: parse_args(&args),
      env: mask_env(&parse_env(&env)),
      enabled: row.get::<_, i64>(5)? != 0,
    })
  }).map_err(|e| e.to_string())?;
  let mut out = Vec::new();
  for r in rows { out.push(r.map_err(|e| e.to_string())?); }
  Ok(out)
}

#[tauri::command]
pub fn save_mcp_server(app: AppHandle, server: McpServerRecord) -> Result<String, String> {
  let conn = db(&app)?;
  if server.command.trim().is_empty() { return Err("An MCP server needs a command to run.".into()); }
  let existing = conn
    .query_row("SELECT env FROM mcp_servers WHERE id=?1", params![server.id], |r| r.get::<_, String>(0))
    .ok()
    .map(|s| parse_env(&s))
    .unwrap_or_else(|| serde_json::json!({}));
  let merged = merge_env(&existing, &server.env);
  let id = if server.id.trim().is_empty() { format!("mcp-{}", chrono::Utc::now().timestamp_millis()) } else { server.id };
  let name = if server.name.trim().is_empty() { id.clone() } else { server.name };
  conn.execute(
    "INSERT INTO mcp_servers (id, name, command, args, env, enabled, updated_at) VALUES (?1,?2,?3,?4,?5,?6,?7)
     ON CONFLICT(id) DO UPDATE SET name=excluded.name, command=excluded.command, args=excluded.args, env=excluded.env, enabled=excluded.enabled, updated_at=excluded.updated_at",
    params![
      id, name, server.command,
      serde_json::to_string(&server.args).unwrap_or_else(|_| "[]".into()),
      serde_json::to_string(&merged).unwrap_or_else(|_| "{}".into()),
      if server.enabled { 1 } else { 0 },
      now()
    ],
  ).map_err(|e| e.to_string())?;
  Ok(id)
}

#[tauri::command]
pub fn delete_mcp_server(app: AppHandle, id: String) -> Result<(), String> {
  let conn = db(&app)?;
  conn.execute("DELETE FROM mcp_servers WHERE id=?1", params![id]).map_err(|e| e.to_string())?;
  // Its imported tools go with it.
  conn.execute("DELETE FROM tools WHERE kind='mcp' AND integration_id=?1", params![id]).map_err(|e| e.to_string())?;
  Ok(())
}

// Handshake + tools/list against a saved server.
#[tauri::command]
pub fn test_mcp_server(app: AppHandle, id: String) -> Result<Vec<McpToolInfo>, String> {
  let conn = db(&app)?;
  list_tools(&load_server(&conn, &id)?)
}

// Turns a server's advertised tools into registry tools (kind='mcp') so they can
// be attached to agents like any other tool. Returns how many were imported.
#[tauri::command]
pub fn import_mcp_tools(app: AppHandle, id: String) -> Result<usize, String> {
  let conn = db(&app)?;
  let server = load_server(&conn, &id)?;
  let tools = list_tools(&server)?;
  let mut count = 0;
  for t in &tools {
    let tool_id = format!("mcp:{}:{}", server.id, t.name);
    let config = serde_json::json!({
      "serverId": server.id, "toolName": t.name,
      "description": t.description, "schema": t.schema,
    });
    conn.execute(
      "INSERT INTO tools (id, name, kind, integration_id, description, enabled, config_json) VALUES (?1,?2,'mcp',?3,?4,1,?5)
       ON CONFLICT(id) DO UPDATE SET name=excluded.name, description=excluded.description, config_json=excluded.config_json",
      params![tool_id, t.name, server.id, t.description, serde_json::to_string(&config).map_err(|e| e.to_string())?],
    ).map_err(|e| e.to_string())?;
    count += 1;
  }
  Ok(count)
}
