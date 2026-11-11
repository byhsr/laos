// MCP connector: speaks JSON-RPC 2.0 to a Model Context Protocol server.
//
// Two transports, one interface:
//   • stdio — a local server process, kept alive per configured server and reused
//             across calls so stateful servers keep their state (the original).
//   • http  — a hosted server reached over Streamable HTTP (a single JSON-RPC
//             message POSTed to the server URL and answered with JSON or an SSE
//             stream), authenticated with OAuth 2.1 (PKCE + dynamic client
//             registration) — the way Monid, Treg, Hermes and Claude Code connect.
//
// Callers only ever touch load_server / list_tools / call_tool, so the transport
// stays isolated here.

use rusqlite::{params, Connection};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;
use tauri::{AppHandle, Emitter};

use base64::Engine as _;
use rand::RngCore;
use sha2::{Digest, Sha256};

use crate::db::{db, now};
use crate::http;
use crate::models::*;

// Hard cap on a single stdio MCP request; the watchdog kills a hung server, which
// ends the read and lets the next call respawn it cleanly.
const RPC_TIMEOUT: Duration = Duration::from_secs(45);
const PROTOCOL_VERSION: &str = "2024-11-05";
// Streamable HTTP arrived in the 2025-03-26 revision; ask for a modern version and
// accept whatever the server negotiates back.
const HTTP_PROTOCOL_VERSION: &str = "2025-06-18";
const MASK: &str = "••••••••";
// Loopback port the remote OAuth redirect lands on. Distinct from the integrations
// OAuth port (14852) so the two flows never collide.
const REMOTE_OAUTH_PORT: u16 = 14853;

#[derive(Clone)]
pub(crate) struct McpServer {
  pub(crate) id: String,
  pub(crate) name: String,
  // stdio transport
  pub(crate) command: String,
  pub(crate) args: Vec<String>,
  pub(crate) env: Vec<(String, String)>,
  // http transport
  pub(crate) transport: String,
  pub(crate) url: String,
  /// Resolved OAuth state (access/refresh tokens, endpoints, client id). Its
  /// secret fields come from the credential store, exactly like `env`.
  pub(crate) oauth: serde_json::Value,
}

impl McpServer {
  fn is_http(&self) -> bool { self.transport == "http" }
  /// A string field of the OAuth blob (empty when absent).
  fn field(&self, k: &str) -> String { self.oauth.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string() }
  /// The bearer token an HTTP request carries (an OAuth access token, or a
  /// manually-pasted one — both live under `accessToken`).
  fn bearer(&self) -> String { self.field("accessToken") }
}

fn str_vec(v: &serde_json::Value) -> Vec<String> {
  v.as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect()).unwrap_or_default()
}

fn env_pairs(id: &str, v: &serde_json::Value) -> Vec<(String, String)> {
  v.as_object().map(|o| o.iter().filter_map(|(k, v)| {
    let s = v.as_str()?;
    // A stored secret holds only the sentinel; resolve it from the credential
    // store, and drop the variable if it is no longer there.
    let resolved = if crate::vault::is_ref(s) { crate::vault::get(&format!("mcp/{id}/{k}")) } else { Some(s.to_string()) };
    resolved.map(|s| (k.clone(), s))
  }).collect()).unwrap_or_default()
}

fn parse_args(s: &str) -> serde_json::Value {
  serde_json::from_str(s).unwrap_or_else(|_| serde_json::json!([]))
}

fn parse_env(s: &str) -> serde_json::Value {
  serde_json::from_str(s).unwrap_or_else(|_| serde_json::json!({}))
}

// Never hand real secret values to the frontend — only a "set" marker.
fn mask_env(env: &serde_json::Value) -> serde_json::Value {
  let mut out = serde_json::Map::new();
  if let Some(obj) = env.as_object() {
    for (k, v) in obj {
      if crate::vault::is_secret_name(k) && v.as_str().map(|s| !s.is_empty()).unwrap_or(false) {
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

// ---------------------------------------------------------------------------
// OAuth secret handling
//
// The OAuth blob holds a mix of secrets (tokens, client secret) and plain
// metadata (endpoints, client id, the PKCE verifier). Sealing is explicit here
// rather than via `vault::protect_object`'s name heuristic, which would mistake a
// field like `tokenEndpoint` for a secret.
// ---------------------------------------------------------------------------

const OAUTH_SECRET_KEYS: [&str; 3] = ["accessToken", "refreshToken", "clientSecret"];

fn oauth_object(json: &str) -> serde_json::Map<String, serde_json::Value> {
  serde_json::from_str::<serde_json::Value>(json).ok().and_then(|v| v.as_object().cloned()).unwrap_or_default()
}

/// Moves the secret fields into the credential store (base `mcp-oauth/<id>`),
/// leaving `__vault__`; empty secrets are dropped.
fn seal_oauth(id: &str, obj: &mut serde_json::Map<String, serde_json::Value>) -> Result<(), String> {
  for k in OAUTH_SECRET_KEYS {
    match obj.get(k).and_then(|v| v.as_str()).map(|s| s.to_string()) {
      Some(s) if !s.is_empty() && !crate::vault::is_ref(&s) => {
        crate::vault::put(&format!("mcp-oauth/{id}/{k}"), &s)?;
        obj.insert(k.into(), serde_json::Value::String(crate::vault::REF.into()));
      }
      Some(_) => { obj.remove(k); }
      None => {}
    }
  }
  Ok(())
}

/// Replaces `__vault__` secret fields with the real value; drops a secret that is
/// no longer in the store.
fn resolve_oauth(id: &str, obj: &mut serde_json::Map<String, serde_json::Value>) {
  for k in OAUTH_SECRET_KEYS {
    let is_ref = obj.get(k).and_then(|v| v.as_str()).map(crate::vault::is_ref).unwrap_or(false);
    if !is_ref { continue; }
    match crate::vault::get(&format!("mcp-oauth/{id}/{k}")) {
      Some(s) => { obj.insert(k.into(), serde_json::Value::String(s)); }
      None => { obj.remove(k); }
    }
  }
}

pub(crate) fn load_server(conn: &Connection, id: &str) -> Result<McpServer, String> {
  let mut stmt = conn.prepare("SELECT id, name, command, args, env, transport, url, oauth FROM mcp_servers WHERE id=?1").map_err(|e| e.to_string())?;
  let mut rows = stmt.query_map(params![id], |row| {
    let args: String = row.get(3)?;
    let env: String = row.get(4)?;
    let transport: String = row.get(5)?;
    let url: String = row.get(6)?;
    let oauth: String = row.get(7)?;
    let mut oauth_obj = oauth_object(&oauth);
    resolve_oauth(id, &mut oauth_obj);
    Ok(McpServer {
      id: row.get(0)?, name: row.get(1)?, command: row.get(2)?,
      args: str_vec(&parse_args(&args)),
      env: env_pairs(id, &parse_env(&env)),
      transport: if transport.trim().is_empty() { "stdio".into() } else { transport },
      url,
      oauth: serde_json::Value::Object(oauth_obj),
    })
  }).map_err(|e| e.to_string())?;
  match rows.next().transpose().map_err(|e| e.to_string())? {
    Some(s) => Ok(s),
    None => Err(format!("MCP server '{id}' does not exist.")),
  }
}

// ---------------------------------------------------------------------------
// stdio transport — one long-lived process per server
// ---------------------------------------------------------------------------

// One live MCP server process. stdio carries a single request/response stream, so
// calls to the same server are serialized through the session lock rather than
// interleaved. The child is behind its own lock so the watchdog can kill it
// without touching the I/O handles.
struct Session {
  fingerprint: String,
  kill: Arc<Mutex<Child>>,
  stdin: ChildStdin,
  reader: BufReader<ChildStdout>,
  stderr: Arc<Mutex<String>>,
  timed_out: Arc<AtomicBool>,
  next_id: u64,
}

impl Session {
  fn is_dead(&self) -> bool {
    match self.kill.lock() {
      Ok(mut c) => matches!(c.try_wait(), Ok(Some(_))),
      Err(_) => true,
    }
  }

  fn kill_child(&mut self) {
    if let Ok(mut c) = self.kill.lock() { kill_tree(&mut c); }
  }

  // Appends the timeout / stderr detail to an error so the reason is visible
  // instead of a bare "the server closed the connection".
  fn explain(&self, base: String) -> String {
    let mut msg = base;
    if self.timed_out.load(Ordering::SeqCst) {
      msg.push_str(&format!("\nIt didn't respond within {}s — it may still be installing or starting up.", RPC_TIMEOUT.as_secs()));
    }
    // Let the stderr reader drain now the process is gone.
    std::thread::sleep(Duration::from_millis(200));
    if let Ok(err) = self.stderr.lock() {
      let err = err.trim();
      if !err.is_empty() {
        let tail: String = err.chars().rev().take(600).collect::<Vec<_>>().into_iter().rev().collect();
        msg.push_str(&format!("\nServer said: {}", tail.trim()));
      }
    }
    msg
  }
}

// Identifies the exact configuration a session was spawned with, so editing a
// server's command/args/env transparently restarts it on the next call.
fn fingerprint(server: &McpServer) -> String {
  format!("{}\u{1}{:?}\u{1}{:?}", server.command, server.args, server.env)
}

fn sessions() -> &'static Mutex<HashMap<String, Arc<Mutex<Session>>>> {
  static SESSIONS: OnceLock<Mutex<HashMap<String, Arc<Mutex<Session>>>>> = OnceLock::new();
  SESSIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

// Kills and forgets one server's session (called when the server is deleted).
fn remove(id: &str) {
  let arc = match sessions().lock() { Ok(mut m) => m.remove(id), Err(_) => None };
  if let Some(arc) = arc {
    if let Ok(mut s) = arc.lock() { s.kill_child(); }
  }
  drop_http_state(id);
  if let Ok(mut m) = http_tokens().lock() { m.remove(id); }
}

// Kills every live session — called on app exit so no server process is orphaned.
pub(crate) fn shutdown_all() {
  let arcs: Vec<Arc<Mutex<Session>>> = match sessions().lock() {
    Ok(mut m) => m.drain().map(|(_, v)| v).collect(),
    Err(_) => Vec::new(),
  };
  for arc in arcs {
    if let Ok(mut s) = arc.lock() { s.kill_child(); }
  }
  if let Ok(mut m) = http_states().lock() { m.clear(); }
  if let Ok(mut m) = http_tokens().lock() { m.clear(); }
}

// On Windows a bare name only resolves to an .exe, so command shims like `npx`
// (npx.cmd) or `uvx` need cmd /C — the same approach as run_command. The app is a
// GUI process, so CREATE_NO_WINDOW is required or every spawned server flashes a
// console window.
#[cfg(windows)]
fn build_command(server: &McpServer) -> Command {
  use std::os::windows::process::CommandExt;
  const CREATE_NO_WINDOW: u32 = 0x0800_0000;
  let mut c = Command::new("cmd");
  c.arg("/C").arg(&server.command);
  c.creation_flags(CREATE_NO_WINDOW);
  c
}
#[cfg(not(windows))]
fn build_command(server: &McpServer) -> Command {
  Command::new(&server.command)
}

// Kills a spawned child and its whole process tree. On Windows the direct child
// is the `cmd /C` shim, so a bare `kill()` orphans the real server (npx → node):
// the grandchild keeps the stdout pipe open, the blocking read in `rpc` never
// returns, and a "test connection" spins forever even after the watchdog fires.
// `taskkill /T` walks the tree so the pipe actually closes.
#[cfg(windows)]
fn kill_tree(child: &mut Child) {
  use std::os::windows::process::CommandExt;
  const CREATE_NO_WINDOW: u32 = 0x0800_0000;
  let pid = child.id().to_string();
  let _ = Command::new("taskkill")
    .args(["/F", "/T", "/PID", &pid])
    .creation_flags(CREATE_NO_WINDOW)
    .stdout(Stdio::null())
    .stderr(Stdio::null())
    .status();
  let _ = child.kill();
  let _ = child.wait();
}

#[cfg(not(windows))]
fn kill_tree(child: &mut Child) {
  let _ = child.kill();
  let _ = child.wait();
}

fn spawn_session(server: &McpServer) -> Result<Session, String> {
  let mut cmd = build_command(server);
  cmd.args(&server.args).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
  for (k, v) in &server.env { cmd.env(k, v); }
  let mut child = cmd.spawn().map_err(|e| format!("Could not start '{}': {e}", server.command))?;
  let stdin = child.stdin.take().ok_or("MCP server has no stdin")?;
  let stdout = child.stdout.take().ok_or("MCP server has no stdout")?;
  let stderr = child.stderr.take();
  let kill = Arc::new(Mutex::new(child));
  let err_buf = Arc::new(Mutex::new(String::new()));
  if let Some(mut handle) = stderr {
    let buf = err_buf.clone();
    std::thread::spawn(move || {
      let mut text = String::new();
      let _ = handle.read_to_string(&mut text);
      if let Ok(mut b) = buf.lock() { *b = text; }
    });
  }
  Ok(Session {
    fingerprint: fingerprint(server),
    kill, stdin, reader: BufReader::new(stdout), stderr: err_buf,
    timed_out: Arc::new(AtomicBool::new(false)), next_id: 1,
  })
}

// Returns the live session for a server, spawning (and handshaking) one when
// none exists, when the config changed, or when the previous process died.
fn get_session(server: &McpServer) -> Result<Arc<Mutex<Session>>, String> {
  let fp = fingerprint(server);
  {
    let map = sessions().lock().map_err(|_| "MCP session registry is poisoned.".to_string())?;
    if let Some(existing) = map.get(&server.id).cloned() {
      let fresh = match existing.lock() { Ok(s) => s.fingerprint == fp && !s.is_dead(), Err(_) => false };
      if fresh { return Ok(existing); }
    }
  }
  // Spawn outside the registry lock so a slow cold start (npx) never blocks other
  // servers while it installs or boots.
  let mut session = spawn_session(server)?;
  if let Err(e) = handshake(&mut session) { session.kill_child(); return Err(e); }
  let arc = Arc::new(Mutex::new(session));

  let mut map = sessions().lock().map_err(|_| "MCP session registry is poisoned.".to_string())?;
  // A concurrent call may have started a good session in the meantime; prefer it
  // and drop the one we just built.
  if let Some(existing) = map.get(&server.id).cloned() {
    let fresh = match existing.lock() { Ok(s) => s.fingerprint == fp && !s.is_dead(), Err(_) => false };
    if fresh {
      if let Ok(mut s) = arc.lock() { s.kill_child(); }
      return Ok(existing);
    }
  }
  if let Some(old) = map.remove(&server.id) {
    if let Ok(mut s) = old.lock() { s.kill_child(); }
  }
  map.insert(server.id.clone(), arc.clone());
  Ok(arc)
}

// Runs `f` against the server's session, dropping a session whose process died so
// the next call respawns instead of reusing a corpse. A tool that merely returns
// an error keeps its session (the process is still healthy).
fn with_session<T>(server: &McpServer, f: impl FnOnce(&mut Session) -> Result<T, String>) -> Result<T, String> {
  let arc = get_session(server)?;
  let mut session = arc.lock().map_err(|_| "MCP session is poisoned.".to_string())?;
  let result = f(&mut session);
  if result.is_err() && session.is_dead() {
    drop(session);
    remove(&server.id);
  }
  result
}

fn write_line(session: &mut Session, msg: &serde_json::Value) -> Result<(), String> {
  let mut line = serde_json::to_string(msg).map_err(|e| e.to_string())?;
  line.push('\n');
  session.stdin.write_all(line.as_bytes()).map_err(|e| format!("MCP write failed: {e}"))?;
  session.stdin.flush().map_err(|e| e.to_string())
}

// initialize + notifications/initialized — required once before list or call.
fn handshake(session: &mut Session) -> Result<(), String> {
  rpc(session, "initialize", serde_json::json!({
    "protocolVersion": PROTOCOL_VERSION,
    "capabilities": {},
    "clientInfo": { "name": "local-agent-os", "version": env!("CARGO_PKG_VERSION") },
  }))?;
  write_line(session, &serde_json::json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }))
}

// Writes one request and reads until its response id arrives, skipping
// notifications and any unrelated traffic.
fn rpc(session: &mut Session, method: &str, params: serde_json::Value) -> Result<serde_json::Value, String> {
  let id = session.next_id;
  session.next_id += 1;
  let mut msg = serde_json::json!({ "jsonrpc": "2.0", "id": id, "method": method });
  if !params.is_null() { msg["params"] = params; }
  write_line(session, &msg)?;

  // Per-request watchdog: a hung server is killed, which ends the read below and
  // lets the next call respawn it. A healthy idle session is never killed.
  let done = Arc::new(AtomicBool::new(false));
  session.timed_out.store(false, Ordering::SeqCst);
  {
    let kill = session.kill.clone();
    let flag = session.timed_out.clone();
    let done = done.clone();
    std::thread::spawn(move || {
      std::thread::sleep(RPC_TIMEOUT);
      if !done.load(Ordering::SeqCst) {
        flag.store(true, Ordering::SeqCst);
        if let Ok(mut c) = kill.lock() { kill_tree(&mut c); }
      }
    });
  }

  let mut buf = String::new();
  let outcome: Result<serde_json::Value, String> = loop {
    buf.clear();
    let read = session.reader.read_line(&mut buf);
    let n = match read {
      Ok(n) => n,
      Err(e) => break Err(session.explain(format!("MCP read failed: {e}"))),
    };
    if n == 0 { break Err(session.explain(format!("The MCP server closed the connection during '{method}'."))); }
    let trimmed = buf.trim();
    if trimmed.is_empty() { continue; }
    let Ok(v) = serde_json::from_str::<serde_json::Value>(trimmed) else { continue };
    if v.get("id").and_then(|i| i.as_u64()) != Some(id) { continue; }
    if let Some(err) = v.get("error") {
      let m = err.get("message").and_then(|m| m.as_str()).unwrap_or("unknown error");
      break Err(format!("MCP '{method}' failed: {m}"));
    }
    break Ok(v.get("result").cloned().unwrap_or(serde_json::Value::Null));
  };
  done.store(true, Ordering::SeqCst);
  outcome
}

// ---------------------------------------------------------------------------
// Shared response parsing (used by both transports)
// ---------------------------------------------------------------------------

/// Reads a `tools/list` result into the tool list the UI/registry consumes.
fn parse_tool_list(res: &serde_json::Value) -> Result<Vec<McpToolInfo>, String> {
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
}

/// Flattens a `tools/call` result's content blocks into text.
fn flatten_content(tool: &str, res: &serde_json::Value) -> Result<String, String> {
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
}

// ---------------------------------------------------------------------------
// Public interface — branches on transport
// ---------------------------------------------------------------------------

// Lists the tools a server advertises.
pub(crate) async fn list_tools(server: &McpServer) -> Result<Vec<McpToolInfo>, String> {
  if server.is_http() {
    http_list_tools(server).await
  } else {
    let server = server.clone();
    tokio::task::spawn_blocking(move || stdio_list_tools(&server)).await.map_err(|e| e.to_string())?
  }
}

// Invokes a tool and flattens its content blocks into text.
pub(crate) async fn call_tool(server: &McpServer, tool: &str, arguments: serde_json::Value) -> Result<String, String> {
  if server.is_http() {
    http_call_tool(server, tool, arguments).await
  } else {
    let server = server.clone();
    let tool = tool.to_string();
    tokio::task::spawn_blocking(move || stdio_call_tool(&server, &tool, arguments)).await.map_err(|e| e.to_string())?
  }
}

fn stdio_list_tools(server: &McpServer) -> Result<Vec<McpToolInfo>, String> {
  with_session(server, |session| {
    let res = rpc(session, "tools/list", serde_json::json!({}))?;
    parse_tool_list(&res)
  })
}

fn stdio_call_tool(server: &McpServer, tool: &str, arguments: serde_json::Value) -> Result<String, String> {
  with_session(server, |session| {
    let res = rpc(session, "tools/call", serde_json::json!({ "name": tool, "arguments": arguments }))?;
    flatten_content(tool, &res)
  })
}

// ---------------------------------------------------------------------------
// HTTP transport — Streamable HTTP
// ---------------------------------------------------------------------------

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

// The negotiated session (a server may hand back an `Mcp-Session-Id` to reuse) and
// the protocol version, keyed by server id. Never held across an await.
struct HttpState {
  fingerprint: String,
  session_id: Option<String>,
  protocol: String,
}

fn http_states() -> &'static Mutex<HashMap<String, HttpState>> {
  static STATES: OnceLock<Mutex<HashMap<String, HttpState>>> = OnceLock::new();
  STATES.get_or_init(|| Mutex::new(HashMap::new()))
}

// A bearer token refreshed mid-session, so the next call uses it without a reload.
fn http_tokens() -> &'static Mutex<HashMap<String, String>> {
  static TOKENS: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();
  TOKENS.get_or_init(|| Mutex::new(HashMap::new()))
}

// The AppHandle, so a mid-session token refresh can be persisted. Set once at
// startup (same pattern as browser::init).
fn handle_cell() -> &'static OnceLock<AppHandle> {
  static HANDLE: OnceLock<AppHandle> = OnceLock::new();
  &HANDLE
}

pub(crate) fn init(app: AppHandle) { let _ = handle_cell().set(app); }

fn app_handle() -> Option<AppHandle> { handle_cell().get().cloned() }

fn http_fingerprint(server: &McpServer) -> String { format!("{}\u{1}{}", server.url, server.bearer()) }

fn drop_http_state(id: &str) { if let Ok(mut m) = http_states().lock() { m.remove(id); } }

/// The bearer to send: a refreshed one if we have it, else the stored token.
fn effective_token(server: &McpServer) -> String {
  http_tokens().lock().ok().and_then(|m| m.get(&server.id).cloned()).unwrap_or_else(|| server.bearer())
}

struct HttpReply { session_id: Option<String>, result: serde_json::Value }

/// POSTs one JSON-RPC message and reads the reply. Handles both a plain JSON body
/// and an SSE stream, and captures a session id from the response headers.
async fn http_send(server: &McpServer, token: &str, session_id: Option<&str>, protocol: &str, method: &str, params: serde_json::Value, notify: bool) -> Result<HttpReply, String> {
  let id = if notify { None } else { Some(NEXT_ID.fetch_add(1, Ordering::Relaxed)) };
  let mut body = serde_json::json!({ "jsonrpc": "2.0", "method": method, "params": params });
  if let Some(id) = id { body["id"] = serde_json::json!(id); }

  let mut req = http::client().post(&server.url)
    .header("Accept", "application/json, text/event-stream")
    .header("Content-Type", "application/json")
    .json(&body);
  // The negotiated protocol version is sent on requests after initialize, not on
  // initialize itself.
  if method != "initialize" && !protocol.is_empty() {
    req = req.header("MCP-Protocol-Version", protocol);
  }
  if let Some(sid) = session_id { req = req.header("Mcp-Session-Id", sid); }
  if !token.is_empty() { req = req.bearer_auth(token); }

  let resp = req.send().await.map_err(|e| format!("MCP request to '{}' failed: {e}", server.url))?;
  let status = resp.status();
  let session_id = resp.headers().get("mcp-session-id").and_then(|v| v.to_str().ok()).map(|s| s.to_string());

  if status.as_u16() == 401 {
    return Err(format!("UNAUTHENTICATED: '{}' rejected the request — connect this server.", server.name));
  }
  if status.as_u16() == 202 || status.as_u16() == 204 {
    return Ok(HttpReply { session_id, result: serde_json::Value::Null });
  }
  if !status.is_success() {
    let text = resp.text().await.unwrap_or_default();
    let snippet: String = text.trim().chars().take(500).collect();
    return Err(format!("MCP '{method}' failed ({}): {}", status.as_u16(), snippet));
  }
  if notify { return Ok(HttpReply { session_id, result: serde_json::Value::Null }); }

  let message = read_mcp_reply(resp, method).await?;
  if let Some(err) = message.get("error") {
    let m = err.get("message").and_then(|m| m.as_str()).unwrap_or("unknown error");
    return Err(format!("MCP '{method}' failed: {m}"));
  }
  Ok(HttpReply { session_id, result: message.get("result").cloned().unwrap_or(serde_json::Value::Null) })
}

/// Reads a JSON-RPC reply out of an MCP response: a plain JSON body, or an SSE
/// stream whose first reply we take and then stop reading.
///
/// Draining the whole stream is what produced a bare reqwest "error decoding
/// response body": a streamable-HTTP server may hold the SSE connection open (so
/// the read never completes) or close it the moment it has written the reply (so
/// the read fails as it hits the closed socket) — either way, after the reply was
/// already fully received. Stopping at the first reply sidesteps both.
async fn read_mcp_reply(resp: reqwest::Response, method: &str) -> Result<serde_json::Value, String> {
  use futures_util::StreamExt;
  let ctype = resp.headers().get("content-type").and_then(|v| v.to_str().ok()).unwrap_or("").to_string();
  if ctype.contains("text/event-stream") {
    let mut stream = resp.bytes_stream();
    let mut buf = String::new();
    while let Some(chunk) = stream.next().await {
      let chunk = chunk.map_err(|e| format!("MCP '{method}' stream ended before a reply: {e}"))?;
      buf.push_str(&String::from_utf8_lossy(&chunk));
      if let Some(v) = sse_result(&buf) { return Ok(v); }
    }
    return Err(format!("MCP '{method}' returned an event stream without a result."));
  }
  let text = resp.text().await.map_err(|e| format!("MCP '{method}' response could not be read: {e}"))?;
  serde_json::from_str::<serde_json::Value>(&text).map_err(|e| format!("MCP '{method}' returned invalid JSON: {e}"))
}

/// Pulls the first JSON-RPC message carrying a result/error out of an SSE body,
/// skipping notifications (which have no id).
fn sse_result(body: &str) -> Option<serde_json::Value> {
  let mut data = String::new();
  for line in body.lines() {
    if let Some(rest) = line.strip_prefix("data:") {
      if !data.is_empty() { data.push('\n'); }
      data.push_str(rest.trim_start());
    } else if line.trim().is_empty() {
      if let Some(v) = take_message(&data) { return Some(v); }
      data.clear();
    }
  }
  take_message(&data)
}

fn take_message(data: &str) -> Option<serde_json::Value> {
  if data.trim().is_empty() { return None; }
  let v: serde_json::Value = serde_json::from_str(data.trim()).ok()?;
  if v.get("result").is_some() || v.get("error").is_some() { Some(v) } else { None }
}

/// Ensures an HTTP server is initialized, returning its (session id, protocol).
async fn http_context(server: &McpServer) -> Result<(Option<String>, String), String> {
  let fp = http_fingerprint(server);
  {
    let states = http_states().lock().map_err(|_| "MCP state is poisoned.".to_string())?;
    if let Some(s) = states.get(&server.id) {
      if s.fingerprint == fp { return Ok((s.session_id.clone(), s.protocol.clone())); }
    }
  }
  let token = effective_token(server);
  let init = http_send(server, &token, None, HTTP_PROTOCOL_VERSION, "initialize", serde_json::json!({
    "protocolVersion": HTTP_PROTOCOL_VERSION,
    "capabilities": {},
    "clientInfo": { "name": "local-agent-os", "version": env!("CARGO_PKG_VERSION") },
  }), false).await?;
  let protocol = init.result.get("protocolVersion").and_then(|v| v.as_str()).unwrap_or(HTTP_PROTOCOL_VERSION).to_string();
  // Best-effort: a server that doesn't accept the notification just ignores it.
  let _ = http_send(server, &token, init.session_id.as_deref(), &protocol, "notifications/initialized", serde_json::json!({}), true).await;
  let mut states = http_states().lock().map_err(|_| "MCP state is poisoned.".to_string())?;
  states.insert(server.id.clone(), HttpState { fingerprint: fp, session_id: init.session_id.clone(), protocol: protocol.clone() });
  Ok((init.session_id, protocol))
}

async fn http_rpc_once(server: &McpServer, method: &str, params: serde_json::Value) -> Result<serde_json::Value, String> {
  let (sid, protocol) = http_context(server).await?;
  let token = effective_token(server);
  let reply = http_send(server, &token, sid.as_deref(), &protocol, method, params, false).await?;
  if let Some(session_id) = reply.session_id {
    if let Ok(mut states) = http_states().lock() {
      if let Some(s) = states.get_mut(&server.id) { s.session_id = Some(session_id); }
    }
  }
  Ok(reply.result)
}

/// One JSON-RPC round-trip; on a 401 it refreshes the token once, then retries.
async fn http_rpc(server: &McpServer, method: &str, params: serde_json::Value) -> Result<serde_json::Value, String> {
  match http_rpc_once(server, method, params.clone()).await {
    Err(e) if e.starts_with("UNAUTHENTICATED") => {
      drop_http_state(&server.id);
      if refresh_remote_token(server).await.is_ok() {
        return http_rpc_once(server, method, params).await;
      }
      Err(e)
    }
    other => other,
  }
}

async fn http_list_tools(server: &McpServer) -> Result<Vec<McpToolInfo>, String> {
  let res = http_rpc(server, "tools/list", serde_json::json!({})).await?;
  parse_tool_list(&res)
}

async fn http_call_tool(server: &McpServer, tool: &str, arguments: serde_json::Value) -> Result<String, String> {
  let res = http_rpc(server, "tools/call", serde_json::json!({ "name": tool, "arguments": arguments })).await?;
  flatten_content(tool, &res)
}

// ---------------------------------------------------------------------------
// OAuth 2.1 (PKCE + dynamic client registration)
// ---------------------------------------------------------------------------

fn b64url(bytes: &[u8]) -> String { base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes) }

/// A PKCE verifier and its S256 challenge (RFC 7636).
fn pkce_pair() -> (String, String) {
  let mut buf = [0u8; 32];
  rand::thread_rng().fill_bytes(&mut buf);
  let verifier = b64url(&buf);
  let challenge = b64url(&Sha256::digest(verifier.as_bytes()));
  (verifier, challenge)
}

fn random_state() -> String {
  let mut buf = [0u8; 16];
  rand::thread_rng().fill_bytes(&mut buf);
  b64url(&buf)
}

fn remote_redirect_uri() -> String { format!("http://127.0.0.1:{REMOTE_OAUTH_PORT}/callback") }

/// Splits a URL into (scheme://host[:port], path-without-trailing-slash).
fn origin_and_path(url: &str) -> Result<(String, String), String> {
  let u = reqwest::Url::parse(url).map_err(|e| format!("Not a valid URL: {e}"))?;
  let host = u.host_str().ok_or("URL has no host")?;
  let origin = match u.port() { Some(p) => format!("{}://{}:{}", u.scheme(), host, p), None => format!("{}://{}", u.scheme(), host) };
  Ok((origin, u.path().trim_end_matches('/').to_string()))
}

async fn get_json(url: &str) -> Result<serde_json::Value, String> {
  let resp = http::client().get(url).header("Accept", "application/json").send().await.map_err(|e| e.to_string())?;
  if !resp.status().is_success() { return Err(format!("{url} → {}", resp.status().as_u16())); }
  crate::http::json_body(resp).await
}

struct AuthServer { authorize: String, token: String, register: Option<String>, scopes: Vec<String>, resource: String }

/// The RFC 9728 protected-resource document, however a server announces it: the
/// conventional well-known paths first, then the `resource_metadata` link a 401's
/// `WWW-Authenticate` header points at. Some hosts (Treg, for one) only advertise
/// it the second way, so trying one and not the other would work for some servers
/// and silently fail for others.
async fn fetch_resource_metadata(url: &str) -> Option<serde_json::Value> {
  let (origin, path) = origin_and_path(url).ok()?;
  let mut candidates: Vec<String> = Vec::new();
  if path.is_empty() {
    candidates.push(format!("{origin}/.well-known/oauth-protected-resource"));
  } else {
    candidates.push(format!("{origin}/.well-known/oauth-protected-resource{path}"));
    candidates.push(format!("{origin}/.well-known/oauth-protected-resource"));
  }
  for c in &candidates {
    if let Ok(v) = get_json(c).await { return Some(v); }
  }
  // Fall back to the resource's own 401 challenge.
  let meta_url = challenge_resource_metadata(url).await?;
  get_json(&meta_url).await.ok()
}

/// Reaches the MCP resource unauthenticated and reads the `resource_metadata` URL
/// out of its `WWW-Authenticate: Bearer …, resource_metadata="…"` challenge.
async fn challenge_resource_metadata(url: &str) -> Option<String> {
  let body = serde_json::json!({
    "jsonrpc": "2.0", "id": 1, "method": "initialize",
    "params": { "protocolVersion": HTTP_PROTOCOL_VERSION, "capabilities": {},
      "clientInfo": { "name": "local-agent-os", "version": env!("CARGO_PKG_VERSION") } }
  });
  let resp = http::client().post(url)
    .header("Accept", "application/json, text/event-stream")
    .header("Content-Type", "application/json")
    .json(&body).send().await.ok()?;
  let header = resp.headers().get("www-authenticate").and_then(|v| v.to_str().ok())?.to_string();
  parse_resource_metadata(&header)
}

/// Pulls the `resource_metadata="…"` value out of a `WWW-Authenticate` header.
fn parse_resource_metadata(header: &str) -> Option<String> {
  let idx = header.find("resource_metadata")?;
  let rest = header[idx + "resource_metadata".len()..].trim_start();
  let rest = rest.strip_prefix('=')?.trim_start();
  let quoted = rest.starts_with('"');
  let rest = rest.strip_prefix('"').unwrap_or(rest);
  let end = if quoted {
    rest.find('"').unwrap_or(rest.len())
  } else {
    rest.find(|c: char| c == ',' || c.is_whitespace()).unwrap_or(rest.len())
  };
  let val = rest[..end].trim();
  if val.is_empty() { None } else { Some(val.to_string()) }
}

/// Discovers the authorization server (RFC 9728 protected-resource metadata, then
/// RFC 8414 authorization-server metadata). The protected-resource document also
/// names the `resource` value (RFC 8707) and the scopes to request for it.
async fn discover_auth_server(url: &str) -> Result<AuthServer, String> {
  let (origin, _path) = origin_and_path(url)?;
  let mut as_url = String::new();
  let mut resource = String::new();
  let mut prm_scopes: Vec<String> = Vec::new();
  if let Some(v) = fetch_resource_metadata(url).await {
    as_url = v.get("authorization_servers").and_then(|a| a.as_array()).and_then(|a| a.first()).and_then(|s| s.as_str()).unwrap_or("").to_string();
    resource = v.get("resource").and_then(|r| r.as_str()).unwrap_or("").to_string();
    prm_scopes = v.get("scopes_supported").and_then(|s| s.as_array()).map(|a| a.iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect()).unwrap_or_default();
  }
  // No PRM: assume the resource's own origin runs the authorization server.
  if as_url.is_empty() { as_url = origin.clone(); }
  // Fall back to the MCP URL as the resource indicator when the metadata omits it.
  if resource.is_empty() { resource = url.to_string(); }

  let (as_origin, as_path) = origin_and_path(&as_url).unwrap_or_else(|_| (origin.clone(), String::new()));
  let meta_urls = if as_path.is_empty() {
    vec![format!("{as_origin}/.well-known/oauth-authorization-server"), format!("{as_origin}/.well-known/openid-configuration")]
  } else {
    vec![
      format!("{as_origin}/.well-known/oauth-authorization-server{as_path}"),
      format!("{as_origin}/.well-known/openid-configuration{as_path}"),
      format!("{as_origin}/.well-known/oauth-authorization-server"),
      format!("{as_origin}/.well-known/openid-configuration"),
    ]
  };
  let mut meta = None;
  for m in &meta_urls { if let Ok(v) = get_json(m).await { meta = Some(v); break; } }
  let meta = meta.ok_or_else(|| format!("Could not discover an OAuth authorization server for {url}."))?;

  let authorize = meta.get("authorization_endpoint").and_then(|v| v.as_str()).ok_or("The authorization server advertises no authorization_endpoint.")?.to_string();
  let token = meta.get("token_endpoint").and_then(|v| v.as_str()).ok_or("The authorization server advertises no token_endpoint.")?.to_string();
  let register = meta.get("registration_endpoint").and_then(|v| v.as_str()).map(|s| s.to_string());
  // The resource's own scopes win over the authorization server's generic list.
  let scopes = if !prm_scopes.is_empty() {
    prm_scopes
  } else {
    meta.get("scopes_supported").and_then(|v| v.as_array()).map(|a| a.iter().filter_map(|s| s.as_str().map(|s| s.to_string())).collect()).unwrap_or_default()
  };
  Ok(AuthServer { authorize, token, register, scopes, resource })
}

async fn register_client(reg: &str, redirect_uri: &str) -> Result<String, String> {
  let body = serde_json::json!({
    "client_name": "local-agent-os",
    "redirect_uris": [redirect_uri],
    "grant_types": ["authorization_code", "refresh_token"],
    "response_types": ["code"],
    "token_endpoint_auth_method": "none",
  });
  let resp = http::client().post(reg).json(&body).send().await.map_err(|e| e.to_string())?;
  if !resp.status().is_success() {
    let text = resp.text().await.unwrap_or_default();
    return Err(format!("Dynamic client registration failed: {}", text.chars().take(500).collect::<String>()));
  }
  let v: serde_json::Value = crate::http::json_body(resp).await?;
  v.get("client_id").and_then(|c| c.as_str()).map(|s| s.to_string()).ok_or_else(|| "Registration returned no client_id.".to_string())
}

fn build_authorize(auth: &AuthServer, client_id: &str, redirect: &str, state: &str, challenge: &str, scope: &str) -> Result<String, String> {
  let mut u = reqwest::Url::parse(&auth.authorize).map_err(|e| e.to_string())?;
  {
    let mut q = u.query_pairs_mut();
    q.append_pair("response_type", "code");
    q.append_pair("client_id", client_id);
    q.append_pair("redirect_uri", redirect);
    if !scope.is_empty() { q.append_pair("scope", scope); }
    // RFC 8707 resource indicator — MCP requires the request name the resource it
    // is for; a strict authorization server rejects the grant without it.
    if !auth.resource.is_empty() { q.append_pair("resource", &auth.resource); }
    q.append_pair("state", state);
    q.append_pair("code_challenge", challenge);
    q.append_pair("code_challenge_method", "S256");
  }
  Ok(u.to_string())
}

/// Merges a patch into a server's OAuth blob, re-seals the secret fields, and
/// persists. A null patch value removes the key.
fn merge_and_seal_oauth(app: &AppHandle, id: &str, patch: serde_json::Value) -> Result<(), String> {
  let conn = db(app)?;
  let existing: String = conn.query_row("SELECT oauth FROM mcp_servers WHERE id=?1", params![id], |r| r.get(0)).map_err(|e| e.to_string())?;
  let mut obj = oauth_object(&existing);
  resolve_oauth(id, &mut obj);
  if let Some(p) = patch.as_object() {
    for (k, v) in p {
      if v.is_null() { obj.remove(k); } else { obj.insert(k.clone(), v.clone()); }
    }
  }
  seal_oauth(id, &mut obj)?;
  conn.execute("UPDATE mcp_servers SET oauth=?2, updated_at=?3 WHERE id=?1", params![id, serde_json::Value::Object(obj).to_string(), now()]).map_err(|e| e.to_string())?;
  Ok(())
}

/// Refreshes a remote server's token from its stored refresh token, caching the
/// new one and persisting it when the app handle is available.
async fn refresh_remote_token(server: &McpServer) -> Result<String, String> {
  let refresh = server.field("refreshToken");
  let endpoint = server.field("tokenEndpoint");
  if refresh.is_empty() || endpoint.is_empty() {
    return Err("This server has no refresh token — connect it again.".into());
  }
  // Form-encoded, like the code exchange (RFC 6749), not JSON.
  let mut form: Vec<(&str, String)> = vec![
    ("grant_type", "refresh_token".into()),
    ("refresh_token", refresh),
    ("client_id", server.field("clientId")),
  ];
  let resource = server.field("resource");
  if !resource.is_empty() { form.push(("resource", resource)); }
  let secret = server.field("clientSecret");
  if !secret.is_empty() { form.push(("client_secret", secret)); }
  let resp = http::client().post(&endpoint).form(&form).send().await.map_err(|e| e.to_string())?;
  if !resp.status().is_success() {
    return Err(format!("MCP token refresh failed: {}", resp.text().await.unwrap_or_default()));
  }
  let tokens: serde_json::Value = crate::http::json_body(resp).await?;
  let access = tokens.get("access_token").and_then(|v| v.as_str()).unwrap_or("").to_string();
  if access.is_empty() { return Err("MCP token refresh returned no access token.".into()); }
  if let Ok(mut m) = http_tokens().lock() { m.insert(server.id.clone(), access.clone()); }
  if let Some(app) = app_handle() {
    let mut patch = serde_json::Map::new();
    patch.insert("accessToken".into(), serde_json::json!(access));
    if let Some(r) = tokens.get("refresh_token").and_then(|v| v.as_str()) { patch.insert("refreshToken".into(), serde_json::json!(r)); }
    if let Some(s) = tokens.get("expires_in").and_then(|v| v.as_i64()) { patch.insert("expiresAt".into(), serde_json::json!((chrono::Utc::now() + chrono::Duration::seconds(s)).to_rfc3339())); }
    let _ = merge_and_seal_oauth(&app, &server.id, serde_json::Value::Object(patch));
  }
  Ok(access)
}

/// Exchanges an authorization code for tokens at the discovered token endpoint.
async fn exchange_code(server: &McpServer, redirect: &str, code: &str) -> Result<serde_json::Value, String> {
  let endpoint = server.field("tokenEndpoint");
  if endpoint.is_empty() { return Err("No token endpoint — run the connect flow first.".into()); }
  // The token endpoint is form-encoded (RFC 6749), NOT JSON — a JSON body is read
  // as an empty form, so `grant_type` looks missing and the server answers
  // "unsupported_grant_type".
  let mut form: Vec<(&str, String)> = vec![
    ("grant_type", "authorization_code".into()),
    ("code", code.to_string()),
    ("redirect_uri", redirect.to_string()),
    ("client_id", server.field("clientId")),
    ("code_verifier", server.field("codeVerifier")),
  ];
  let resource = server.field("resource");
  if !resource.is_empty() { form.push(("resource", resource)); }
  let secret = server.field("clientSecret");
  if !secret.is_empty() { form.push(("client_secret", secret)); }
  let resp = http::client().post(&endpoint).form(&form).send().await.map_err(|e| e.to_string())?;
  if !resp.status().is_success() {
    let text = resp.text().await.unwrap_or_default();
    return Err(format!("MCP token exchange failed: {}", text.chars().take(600).collect::<String>()));
  }
  crate::http::json_body(resp).await
}

async fn complete_remote_oauth_inner(app: &AppHandle, id: &str, code: &str, state: Option<&str>) -> Result<(), String> {
  let server = { let conn = db(app)?; load_server(&conn, id)? };
  // Verify the CSRF state the connect flow issued, when there was one. A callback
  // carrying a code for a different request is refused before it is exchanged.
  let expected = server.field("state");
  if !expected.is_empty() {
    match state {
      Some(s) if s == expected => {}
      _ => return Err("The sign-in response didn't match this request (state mismatch) — nothing was saved.".into()),
    }
  }
  let redirect = remote_redirect_uri();
  let tokens = exchange_code(&server, &redirect, code).await?;
  let access = tokens.get("access_token").and_then(|v| v.as_str()).unwrap_or("").to_string();
  if access.is_empty() { return Err("The authorization server returned no access token.".into()); }
  let mut patch = serde_json::Map::new();
  patch.insert("accessToken".into(), serde_json::json!(access));
  if let Some(r) = tokens.get("refresh_token").and_then(|v| v.as_str()) { patch.insert("refreshToken".into(), serde_json::json!(r)); }
  if let Some(s) = tokens.get("expires_in").and_then(|v| v.as_i64()) { patch.insert("expiresAt".into(), serde_json::json!((chrono::Utc::now() + chrono::Duration::seconds(s)).to_rfc3339())); }
  // The verifier and state were single-use; drop them.
  patch.insert("codeVerifier".into(), serde_json::Value::Null);
  patch.insert("state".into(), serde_json::Value::Null);
  merge_and_seal_oauth(app, id, serde_json::Value::Object(patch))?;
  drop_http_state(id);
  if let Ok(mut m) = http_tokens().lock() { m.remove(id); }
  Ok(())
}

/// What the loopback redirect turned out to be.
enum Callback { Code { code: String, state: Option<String> }, Denied(String), TimedOut }

const SKIP_PAGE: &[u8] = b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";

// The landing page is styled with the app's own design tokens — the two dark
// planes, hairline borders, and the single accent on the confirm mark — so it
// reads as a continuation of the app rather than a stray web page. Kept as its
// own const so the `format!` that assembles the document never has to escape a
// brace in the stylesheet.
const PAGE_CSS: &str = r#"
:root{--background:#191919;--surface:#202020;--border:#2a2a2a;--foreground:#f0efed;--muted:#8a8a8a;--accent:#d7ff38;--danger:#ef4444;--ease:cubic-bezier(.2,0,0,1);--mono:'DM Mono',ui-monospace,SFMono-Regular,Menlo,Consolas,monospace;--sans:'Manrope',system-ui,-apple-system,'Segoe UI',sans-serif}
*{box-sizing:border-box}
html{height:100%}
body{margin:0;min-height:100%;display:grid;place-items:center;padding:24px;background:var(--background);color:var(--foreground);font-family:var(--sans);-webkit-font-smoothing:antialiased}
.panel{width:100%;max-width:360px;padding:20px;background:var(--surface);border:1px solid var(--border);border-radius:16px;box-shadow:0 16px 40px -12px rgba(0,0,0,.6);opacity:0;transform:translateY(4px) scale(.99);animation:rise .16s var(--ease) forwards}
@keyframes rise{to{opacity:1;transform:none}}
.label{font-family:var(--mono);font-size:10px;letter-spacing:.14em;text-transform:uppercase;color:var(--muted)}
.mark{display:grid;place-items:center;width:38px;height:38px;margin:14px 0 12px;border:1px solid var(--border);border-radius:12px;color:var(--accent)}
.mark svg{width:18px;height:18px;display:block}
.error .mark{color:var(--danger)}
h1{margin:0 0 6px;font-size:19px;font-weight:600;letter-spacing:-.01em;line-height:1.3;overflow-wrap:anywhere}
p{margin:0;font-size:13.5px;line-height:1.65;color:var(--muted);overflow-wrap:anywhere}
.hint{margin-top:16px;padding-top:12px;border-top:1px solid var(--border);font-family:var(--mono);font-size:11px;color:var(--muted)}
@media (prefers-reduced-motion:reduce){.panel{animation:none;opacity:1;transform:none}}
"#;

const PAGE_CHECK: &str = "<svg viewBox=\"0 0 24 24\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"2.5\" stroke-linecap=\"round\" stroke-linejoin=\"round\"><path d=\"M20 6 9 17l-5-5\"/></svg>";
const PAGE_ALERT: &str = "<svg viewBox=\"0 0 24 24\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\"><circle cx=\"12\" cy=\"12\" r=\"9\"/><path d=\"M12 8v5\"/><path d=\"M12 16.5h.01\"/></svg>";
const PAGE_SCRIPT: &str = "<script>setTimeout(function(){try{window.close()}catch(e){}},900)</script>";

/// Escapes any text that reaches the landing page — a server name or an OAuth
/// error detail is attacker-influenced, so it never lands in the markup raw.
fn html_escape(s: &str) -> String {
  s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\'', "&#39;")
}

/// The page the browser lands on after a remote MCP sign-in, in the app's own
/// visual language. One accent (a check), neutral everything else, mono chrome.
fn callback_page(ok: bool, server: &str, detail: &str) -> String {
  let name = html_escape(if server.trim().is_empty() { "The server" } else { server.trim() });
  let (cls, label, icon, heading, body, hint) = if ok {
    ("ok", "mcp connected", PAGE_CHECK, format!("{name} is connected"),
     "Its tools are ready to attach to an agent.".to_string(), "you can close this tab")
  } else {
    ("error", "mcp sign-in failed", PAGE_ALERT, format!("{name} could not be connected"),
     html_escape(detail), "close this tab, then try again in the app")
  };
  format!("<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>Local Agent OS</title><style>{PAGE_CSS}</style></head><body><main class=\"panel {cls}\"><div class=\"label\">{label}</div><div class=\"mark\">{icon}</div><h1>{heading}</h1><p>{body}</p><div class=\"hint\">{hint}</div></main>{PAGE_SCRIPT}</body></html>")
}

fn write_html(stream: &mut impl std::io::Write, html: &str) {
  let response = format!("HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{}", html.as_bytes().len(), html);
  let _ = stream.write_all(response.as_bytes());
  let _ = stream.flush();
}

/// Waits on the loopback redirect for the callback that actually carries a `code`.
///
/// A browser opens speculative connections (preconnect, `/favicon.ico`) that reach
/// this port first; the old code accepted a single connection and read once, so a
/// stray probe could swallow the accept and the real callback was never seen. This
/// loops, ignoring anything without a `code`/`error`, with a per-read timeout so a
/// silent connection can't block the wait.
fn wait_for_callback(listener: std::net::TcpListener, server: &str) -> Callback {
  use std::io::{Read, Write};
  let deadline = std::time::Instant::now() + Duration::from_secs(300);
  loop {
    if std::time::Instant::now() >= deadline { return Callback::TimedOut; }
    let Ok((mut stream, _)) = listener.accept() else { return Callback::TimedOut; };
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(5)));
    let mut buf = [0u8; 8192];
    let n = match stream.read(&mut buf) { Ok(n) => n, Err(_) => continue };
    let request = String::from_utf8_lossy(&buf[..n]).to_string();
    let first_line = request.lines().next().unwrap_or("");
    let target = first_line.split_whitespace().nth(1).unwrap_or("");
    let query = target.splitn(2, '?').nth(1).unwrap_or("");
    let mut code = None;
    let mut state = None;
    let mut error = None;
    let mut desc = None;
    for kv in query.split('&') {
      let mut it = kv.splitn(2, '=');
      let k = it.next().unwrap_or("");
      let v = urldecode(it.next().unwrap_or(""));
      match k {
        "code" => code = Some(v),
        "state" => state = Some(v),
        "error" => error = Some(v),
        "error_description" => desc = Some(v),
        _ => {}
      }
    }
    if let Some(code) = code {
      write_html(&mut stream, &callback_page(true, server, ""));
      return Callback::Code { code, state };
    }
    if error.is_some() {
      let detail = format!("{} {}", error.unwrap_or_default(), desc.unwrap_or_default());
      let detail = detail.trim().to_string();
      write_html(&mut stream, &callback_page(false, server, &detail));
      return Callback::Denied(detail);
    }
    // Not the callback (preconnect, favicon, a bare `/`): answer and keep waiting.
    let _ = stream.write_all(SKIP_PAGE);
    let _ = stream.flush();
  }
}

fn urldecode(s: &str) -> String {
  let bytes = s.as_bytes();
  let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
  let mut i = 0;
  while i < bytes.len() {
    match bytes[i] {
      b'%' if i + 2 < bytes.len() => {
        match std::str::from_utf8(&bytes[i + 1..i + 3]).ok().and_then(|h| u8::from_str_radix(h, 16).ok()) {
          Some(b) => { out.push(b); i += 3; }
          None => { out.push(bytes[i]); i += 1; }
        }
      }
      b'+' => { out.push(b' '); i += 1; }
      b => { out.push(b); i += 1; }
    }
  }
  String::from_utf8_lossy(&out).to_string()
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn list_mcp_servers(app: AppHandle) -> Result<Vec<McpServerRecord>, String> {
  let conn = db(&app)?;
  let mut stmt = conn.prepare("SELECT id, name, command, args, env, enabled, transport, url, oauth FROM mcp_servers ORDER BY name").map_err(|e| e.to_string())?;
  let rows = stmt.query_map([], |row| {
    let args: String = row.get(3)?;
    let env: String = row.get(4)?;
    let transport: String = row.get(6)?;
    let url: String = row.get(7)?;
    let oauth: String = row.get(8)?;
    let connected = oauth_object(&oauth).get("accessToken").and_then(|v| v.as_str()).map(|s| !s.is_empty()).unwrap_or(false);
    Ok(McpServerRecord {
      id: row.get(0)?, name: row.get(1)?, command: row.get(2)?,
      args: parse_args(&args),
      env: mask_env(&parse_env(&env)),
      enabled: row.get::<_, i64>(5)? != 0,
      transport: if transport.trim().is_empty() { "stdio".into() } else { transport },
      url,
      connected,
    })
  }).map_err(|e| e.to_string())?;
  let mut out = Vec::new();
  for r in rows { out.push(r.map_err(|e| e.to_string())?); }
  Ok(out)
}

#[tauri::command]
pub fn save_mcp_server(app: AppHandle, server: McpServerRecord) -> Result<String, String> {
  let conn = db(&app)?;
  let transport = if server.transport.trim().is_empty() { "stdio".to_string() } else { server.transport.trim().to_string() };
  if transport == "http" {
    if server.url.trim().is_empty() { return Err("A remote MCP server needs a URL.".into()); }
  } else if server.command.trim().is_empty() {
    return Err("An MCP server needs a command to run.".into());
  }
  let id = if server.id.trim().is_empty() { format!("mcp-{}", chrono::Utc::now().timestamp_millis()) } else { server.id.clone() };
  let existing = conn
    .query_row("SELECT env FROM mcp_servers WHERE id=?1", params![id], |r| r.get::<_, String>(0))
    .ok()
    .map(|s| parse_env(&s))
    .unwrap_or_else(|| serde_json::json!({}));
  let mut merged = merge_env(&existing, &server.env);
  // A newly typed credential goes straight into the OS credential store; the row
  // keeps only the sentinel, exactly like any other secret.
  if let Some(obj) = merged.as_object_mut() { crate::vault::protect_object(&format!("mcp/{id}"), obj)?; }
  let name = if server.name.trim().is_empty() { id.clone() } else { server.name };
  // The `oauth` column is never written here — it is owned by the connect flow,
  // and re-saving a server must not drop its tokens.
  conn.execute(
    "INSERT INTO mcp_servers (id, name, command, args, env, enabled, updated_at, transport, url) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)
     ON CONFLICT(id) DO UPDATE SET name=excluded.name, command=excluded.command, args=excluded.args, env=excluded.env, enabled=excluded.enabled, updated_at=excluded.updated_at, transport=excluded.transport, url=excluded.url",
    params![
      id, name, server.command,
      serde_json::to_string(&server.args).unwrap_or_else(|_| "[]".into()),
      serde_json::to_string(&merged).unwrap_or_else(|_| "{}".into()),
      if server.enabled { 1 } else { 0 },
      now(),
      transport,
      server.url.trim(),
    ],
  ).map_err(|e| e.to_string())?;
  // The running process (if any) keeps serving the old config until the next call
  // notices the fingerprint changed and respawns — no restart needed here.
  Ok(id)
}

#[tauri::command]
pub fn delete_mcp_server(app: AppHandle, id: String) -> Result<(), String> {
  let conn = db(&app)?;
  conn.execute("DELETE FROM mcp_servers WHERE id=?1", params![id]).map_err(|e| e.to_string())?;
  // Its imported tools go with it.
  conn.execute("DELETE FROM tools WHERE kind='mcp' AND integration_id=?1", params![id]).map_err(|e| e.to_string())?;
  // Any OAuth secrets it held, and its live process/session, are cleaned up.
  for k in OAUTH_SECRET_KEYS { let _ = crate::vault::delete(&format!("mcp-oauth/{id}/{k}")); }
  remove(&id);
  Ok(())
}

// Handshake + tools/list against a saved server.
#[tauri::command]
pub async fn test_mcp_server(app: AppHandle, id: String) -> Result<Vec<McpToolInfo>, String> {
  let server = { let conn = db(&app)?; load_server(&conn, &id)? };
  list_tools(&server).await
}

// Turns a server's advertised tools into registry tools (kind='mcp') so they can
// be attached to agents like any other tool. Returns how many were imported.
#[tauri::command]
pub async fn import_mcp_tools(app: AppHandle, id: String) -> Result<usize, String> {
  let server = { let conn = db(&app)?; load_server(&conn, &id)? };
  let tools = list_tools(&server).await?;
  let mut conn = db(&app)?;
  let tx = conn.transaction().map_err(|e| e.to_string())?;
  // A server's advertised tools can change between imports, so rebuild this
  // server's rows: the registry never keeps advertising a tool that no longer
  // exists. Ids are deterministic (`mcp:<serverId>:<toolName>`), so an agent
  // that already attached one keeps a valid reference.
  tx.execute("DELETE FROM tools WHERE kind='mcp' AND integration_id=?1", params![server.id]).map_err(|e| e.to_string())?;
  let mut count = 0;
  for t in &tools {
    let tool_id = format!("mcp:{}:{}", server.id, t.name);
    let config = serde_json::json!({
      "serverId": server.id, "serverName": server.name, "toolName": t.name,
      "description": t.description, "schema": t.schema,
    });
    tx.execute(
      "INSERT INTO tools (id, name, kind, integration_id, description, enabled, config_json) VALUES (?1,?2,'mcp',?3,?4,1,?5)
       ON CONFLICT(id) DO UPDATE SET name=excluded.name, description=excluded.description, config_json=excluded.config_json",
      params![tool_id, t.name, server.id, t.description, serde_json::to_string(&config).map_err(|e| e.to_string())?],
    ).map_err(|e| e.to_string())?;
    count += 1;
  }
  tx.commit().map_err(|e| e.to_string())?;
  Ok(count)
}

// Invokes a server's tool directly, for UI surfaces (e.g. the Memory view) that
// read a server's data without routing through an agent. Returns flattened text.
#[tauri::command]
pub async fn call_mcp_tool(app: AppHandle, server_id: String, tool: String, arguments: serde_json::Value) -> Result<String, String> {
  let server = { let conn = db(&app)?; load_server(&conn, &server_id)? };
  call_tool(&server, &tool, arguments).await
}

// Runs discovery + dynamic client registration for a remote server, stores what
// the callback will need, and returns the authorize URL to open.
#[tauri::command]
pub async fn remote_mcp_start(app: AppHandle, id: String) -> Result<String, String> {
  let server = { let conn = db(&app)?; load_server(&conn, &id)? };
  if !server.is_http() { return Err("This server uses the stdio transport, not a URL.".into()); }
  if server.url.trim().is_empty() { return Err("This remote server has no URL.".into()); }

  let redirect = remote_redirect_uri();
  let auth = discover_auth_server(&server.url).await?;
  // Reuse a registered client id from a previous connect; otherwise register one.
  let mut client_id = server.field("clientId");
  if client_id.is_empty() {
    let reg = auth.register.clone().ok_or_else(|| "The authorization server doesn't support dynamic client registration — connect this server in its own app, then paste a token here.".to_string())?;
    client_id = register_client(&reg, &redirect).await?;
  }
  let (verifier, challenge) = pkce_pair();
  let state = random_state();
  let scope = auth.scopes.join(" ");
  let url = build_authorize(&auth, &client_id, &redirect, &state, &challenge, &scope)?;
  merge_and_seal_oauth(&app, &id, serde_json::json!({
    "clientId": client_id,
    "tokenEndpoint": auth.token,
    "authorizationEndpoint": auth.authorize,
    "resource": auth.resource,
    "codeVerifier": verifier,
    "state": state,
    "scope": scope,
  }))?;
  drop_http_state(&id);
  Ok(url)
}

// Opens the authorize URL and waits on the loopback for the redirect, exchanging
// the code for tokens. Returns the URL so the UI can tell the user where to look.
// The terminal outcome (success or a readable failure) is emitted as
// `lumin-mcp-changed` and, on failure, queued as a notification — the flow used
// to fail silently.
#[tauri::command]
pub async fn remote_mcp_connect(app: AppHandle, id: String) -> Result<String, String> {
  let name = { let conn = db(&app)?; load_server(&conn, &id).map(|s| s.name).unwrap_or_else(|_| id.clone()) };
  let url = remote_mcp_start(app.clone(), id.clone()).await?;
  // Bind the callback port before opening the browser, so a fast redirect can't
  // race the listener and land on a closed port.
  let listener = std::net::TcpListener::bind(("127.0.0.1", REMOTE_OAUTH_PORT))
    .map_err(|e| format!("Could not open the OAuth callback port {REMOTE_OAUTH_PORT}: {e}"))?;
  tauri_plugin_opener::open_url(&url, None::<&str>).map_err(|e| e.to_string())?;

  let handle = app.clone();
  let id2 = id.clone();
  let page_name = name.clone();
  tauri::async_runtime::spawn(async move {
    let outcome = tokio::task::spawn_blocking(move || wait_for_callback(listener, &page_name)).await.ok();
    let (ok, message) = match outcome {
      Some(Callback::Code { code, state }) => match complete_remote_oauth_inner(&handle, &id2, &code, state.as_deref()).await {
        Ok(()) => (true, format!("Signed in to {name}. Run sync tools to import its tools.")),
        Err(e) => (false, format!("{name} sign-in could not be completed: {e}")),
      },
      Some(Callback::Denied(e)) => (false, format!("{name} sign-in was refused: {e}")),
      _ => (false, format!("{name} sign-in timed out — no callback arrived. Try again, or paste a token.")),
    };
    drop_http_state(&id2);
    // A successful connect is worth a real alert too: the browser held focus during
    // the sign-in, so the in-app toast alone would go unseen.
    crate::notify::notify_and_push(&handle, "system", if ok { "MCP connected" } else { "MCP sign-in failed" }, &message, "", "");
    let _ = handle.emit("lumin-mcp-changed", serde_json::json!({ "id": id2, "ok": ok, "message": message }));
  });

  Ok(url)
}

// Exchanges a code the user pasted manually (the same flow, without the loopback).
#[tauri::command]
pub async fn remote_mcp_complete(app: AppHandle, id: String, code: String) -> Result<(), String> {
  complete_remote_oauth_inner(&app, &id, &code, None).await
}

// A static bearer token, for a host whose discovery/registration is unavailable.
#[tauri::command]
pub fn remote_mcp_set_token(app: AppHandle, id: String, token: String) -> Result<(), String> {
  if token.trim().is_empty() { return Err("Enter a token.".into()); }
  merge_and_seal_oauth(&app, &id, serde_json::json!({ "accessToken": token.trim() }))?;
  drop_http_state(&id);
  Ok(())
}

// Forgets a remote server's credentials.
#[tauri::command]
pub fn remote_mcp_disconnect(app: AppHandle, id: String) -> Result<(), String> {
  for k in OAUTH_SECRET_KEYS { let _ = crate::vault::delete(&format!("mcp-oauth/{id}/{k}")); }
  let conn = db(&app)?;
  conn.execute("UPDATE mcp_servers SET oauth='{}', updated_at=?2 WHERE id=?1", params![id, now()]).map_err(|e| e.to_string())?;
  drop_http_state(&id);
  if let Ok(mut m) = http_tokens().lock() { m.remove(&id); }
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;

  fn http_server(url: &str) -> McpServer {
    McpServer {
      id: "s".into(), name: "s".into(), command: String::new(), args: vec![], env: vec![],
      transport: "http".into(), url: url.into(), oauth: serde_json::json!({}),
    }
  }

  /// RFC 7636 Appendix B: the S256 challenge of the example verifier.
  #[test]
  fn pkce_challenge_is_the_s256_of_the_verifier() {
    let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
    assert_eq!(b64url(&Sha256::digest(verifier.as_bytes())), "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM");
  }

  /// An SSE body yields the message carrying a result, skipping notifications.
  #[test]
  fn sse_result_skips_notifications_and_returns_the_reply() {
    let body = "event: message\ndata: {\"jsonrpc\":\"2.0\",\"method\":\"notifications/progress\"}\n\nevent: message\ndata: {\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"tools\":[]}}\n\n";
    let v = sse_result(body).expect("a result");
    assert!(v.get("result").is_some());
  }

  /// Metadata bases are derived from the MCP URL's origin + path.
  #[test]
  fn origin_and_path_derive_the_metadata_bases() {
    assert_eq!(origin_and_path("https://mcp.monid.ai/v1").unwrap(), ("https://mcp.monid.ai".into(), "/v1".into()));
    assert_eq!(origin_and_path("https://treg.to/mcp/").unwrap(), ("https://treg.to".into(), "/mcp".into()));
  }

  /// Only an http server takes the HTTP path (the stdio registry is never touched).
  #[test]
  fn transport_is_recognised() {
    assert!(http_server("https://mcp.monid.ai/v1").is_http());
    let stdio = McpServer { transport: "stdio".into(), ..http_server("") };
    assert!(!stdio.is_http());
  }

  /// Percent-decoding covers the escapes a redirect query can carry.
  #[test]
  fn urldecode_handles_percent_and_plus() {
    assert_eq!(urldecode("a%2Fb%20c+d"), "a/b c d");
  }

  /// The 401 challenge names the protected-resource metadata document (quoted or
  /// bare), which discovery follows when the well-known path isn't served.
  #[test]
  fn resource_metadata_is_read_from_a_www_authenticate_challenge() {
    let h = r#"Bearer realm="mcp", resource_metadata="https://treg.to/.well-known/oauth-protected-resource""#;
    assert_eq!(parse_resource_metadata(h).as_deref(), Some("https://treg.to/.well-known/oauth-protected-resource"));
    assert_eq!(parse_resource_metadata("Bearer resource_metadata=https://x.dev/prm"), Some("https://x.dev/prm".to_string()));
    assert_eq!(parse_resource_metadata("Basic realm=\"nope\""), None);
  }

  /// The authorize URL carries the RFC 8707 resource indicator and PKCE (S256).
  #[test]
  fn authorize_url_carries_the_resource_indicator() {
    let auth = AuthServer {
      authorize: "https://treg.to/oauth/authorize".into(), token: "https://treg.to/oauth/token".into(),
      register: None, scopes: vec!["treg:call".into()], resource: "https://treg.to/mcp/".into(),
    };
    let url = build_authorize(&auth, "cid", "http://127.0.0.1:14853/callback", "st", "ch", "treg:call").unwrap();
    assert!(url.contains("resource=https%3A%2F%2Ftreg.to%2Fmcp%2F"));
    assert!(url.contains("code_challenge_method=S256"));
    assert!(url.contains("response_type=code"));
  }

  /// The landing page escapes anything interpolated into it — a server name or an
  /// OAuth error is attacker-influenced text, never markup.
  #[test]
  fn the_landing_page_escapes_hostile_text() {
    let page = callback_page(true, "<script>bad</script>", "");
    assert!(!page.contains("<script>bad"), "the raw tag is not echoed");
    assert!(page.contains("&lt;script&gt;bad&lt;/script&gt; is connected"), "it is shown escaped");
  }

  /// The declared Content-Length matches the bytes actually sent, so a multi-byte
  /// character in the stylesheet or the detail can't truncate the response.
  #[test]
  fn the_landing_page_content_length_matches_its_body() {
    let html = callback_page(false, "treg", "access denied — try again");
    let mut out: Vec<u8> = Vec::new();
    write_html(&mut out, &html);
    let text = String::from_utf8(out).expect("utf-8");
    let (head, body) = text.split_once("\r\n\r\n").expect("a header block");
    let len: usize = head.lines().find_map(|l| l.strip_prefix("Content-Length: ")).expect("a length").parse().expect("a number");
    assert_eq!(len, body.len(), "declared length equals the bytes sent");
  }
}
