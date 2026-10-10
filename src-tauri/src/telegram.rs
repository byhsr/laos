// Telegram integration: bot token + activity log, cloudflared tunnel and
// webhook registration, the local webhook receiver, and the long-poll adapter
// that routes incoming messages to the Manager.

use rusqlite::{params, Connection};
use tauri::{AppHandle, Manager};

use crate::agents::build_workspace_context;
use crate::db::{db, now};
use crate::http;
use crate::manager::manager_turn;
use crate::tasks::list_tasks;

// Reads the telegram integration config blob (empty object when unset).
fn telegram_config(conn: &Connection) -> serde_json::Value {
  let Ok(mut stmt) = conn.prepare("SELECT config_json FROM integration_configs WHERE id='telegram'") else {
    return serde_json::json!({});
  };
  let Ok(mut rows) = stmt.query_map([], |row| row.get::<_, String>(0)) else {
    return serde_json::json!({});
  };
  match rows.next().transpose() {
    Ok(Some(cfg)) => serde_json::from_str(&cfg).unwrap_or_else(|_| serde_json::json!({})),
    _ => serde_json::json!({}),
  }
}

fn cfg_str(cfg: &serde_json::Value, key: &str) -> Option<String> {
  cfg.get(key).and_then(|v| v.as_str()).filter(|s| !s.is_empty()).map(|s| s.to_string())
}

// Merges `mutate` into the telegram config and writes it back, never clobbering
// the stored bot token. `connected` tracks whether a tunnel/webhook is live.
fn write_telegram_config(conn: &Connection, connected: bool, mutate: impl FnOnce(&mut serde_json::Map<String, serde_json::Value>)) -> Result<(), String> {
  let mut merged = telegram_config(conn);
  if !merged.is_object() { merged = serde_json::json!({}); }
  mutate(merged.as_object_mut().unwrap());
  conn.execute(
    "INSERT INTO integration_configs (id, name, provider, config_json, enabled, connected, updated_at) VALUES ('telegram','Telegram','telegram',?1,1,?2,?3)
     ON CONFLICT(id) DO UPDATE SET config_json=excluded.config_json, connected=excluded.connected",
    params![serde_json::to_string(&merged).map_err(|e| e.to_string())?, if connected { 1 } else { 0 }, now()],
  ).map_err(|e| e.to_string())?;
  Ok(())
}

// Reads the Telegram bot token from the telegram integration config, if enabled.
fn telegram_token(conn: &Connection) -> Result<Option<String>, String> {
  Ok(cfg_str(&telegram_config(conn), "token"))
}

fn webhook_secret(conn: &Connection) -> Option<String> {
  cfg_str(&telegram_config(conn), "webhookSecret")
}

// Telegram constrains secret_token to 1-256 chars of A-Za-z0-9_-
fn generate_webhook_secret() -> String {
  use std::hash::{BuildHasher, Hasher};
  let mut out = String::new();
  for i in 0..2u64 {
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u64(i);
    h.write_u64(u64::from(std::process::id()));
    h.write_u128(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0));
    out.push_str(&format!("{:016x}", h.finish()));
  }
  out
}

// Returns the stored webhook secret, minting one on first use.
fn ensure_webhook_secret(conn: &Connection) -> Result<String, String> {
  if let Some(s) = webhook_secret(conn) { return Ok(s); }
  let secret = generate_webhook_secret();
  let to_store = secret.clone();
  write_telegram_config(conn, true, move |o| { o.insert("webhookSecret".into(), serde_json::json!(to_store)); })?;
  Ok(secret)
}

// ---------------------------------------------------------------------------
// Bots (multiple bots, each long-polled and routed to an agent)
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct TelegramBot { id: String, name: String, token: String, agent_id: String, terminal_id: String, allowed_users: String, enabled: bool, webhook_secret: String, webhook_registered: bool }

fn load_bots(conn: &Connection) -> Vec<TelegramBot> {
  let mut out = Vec::new();
  if let Ok(mut stmt) = conn.prepare("SELECT id, name, token, agent_id, terminal_id, allowed_users, enabled, webhook_secret, webhook_registered FROM telegram_bots ORDER BY name") {
    if let Ok(rows) = stmt.query_map([], |row| Ok(TelegramBot {
      id: row.get(0)?, name: row.get(1)?, token: row.get(2)?, agent_id: row.get(3)?, terminal_id: row.get(4)?, allowed_users: row.get(5)?,
      enabled: row.get::<_, i64>(6)? != 0, webhook_secret: row.get(7)?, webhook_registered: row.get::<_, i64>(8)? != 0,
    })) {
      for r in rows { if let Ok(b) = r { out.push(b); } }
    }
  }
  out
}

fn load_bot(conn: &Connection, id: &str) -> Option<TelegramBot> {
  conn.query_row("SELECT id, name, token, agent_id, terminal_id, allowed_users, enabled, webhook_secret, webhook_registered FROM telegram_bots WHERE id=?1", params![id], |row| Ok(TelegramBot {
    id: row.get(0)?, name: row.get(1)?, token: row.get(2)?, agent_id: row.get(3)?, terminal_id: row.get(4)?, allowed_users: row.get(5)?,
    enabled: row.get::<_, i64>(6)? != 0, webhook_secret: row.get(7)?, webhook_registered: row.get::<_, i64>(8)? != 0,
  })).ok()
}

// A bot is private when it lists allowed Telegram user ids; empty = open.
fn is_authorized(bot: &TelegramBot, user_id: Option<i64>) -> bool {
  let list: Vec<String> = serde_json::from_str(&bot.allowed_users).unwrap_or_default();
  if list.is_empty() { return true; }
  matches!(user_id, Some(id) if list.iter().any(|x| x == &id.to_string()))
}

// Basic auth with trust-on-first-use: when a bot has no allowlist, the first
// sender is bound as its owner (persisted), and every later message is locked to
// that id. Once bound it behaves exactly like an explicit allowlist.
fn authorize_and_bind(app: &AppHandle, bot: &TelegramBot, user_id: Option<i64>) -> bool {
  let list: Vec<String> = serde_json::from_str(&bot.allowed_users).unwrap_or_default();
  if !list.is_empty() {
    return matches!(user_id, Some(id) if list.iter().any(|x| x == &id.to_string()));
  }
  let Some(id) = user_id else { return false };
  if let Ok(conn) = db(app) {
    let arr = serde_json::json!([id.to_string()]).to_string();
    let _ = conn.execute("UPDATE telegram_bots SET allowed_users=?1, updated_at=?2 WHERE id=?3", params![arr, now(), bot.id]);
  }
  true
}

// One-time migration: fold the legacy single telegram token (integration_configs
// row) into a bot row so existing setups keep working.
fn migrate_legacy_bot(conn: &Connection) {
  let count: i64 = conn.query_row("SELECT COUNT(*) FROM telegram_bots", [], |r| r.get(0)).unwrap_or(0);
  if count > 0 { return; }
  if let Some(token) = cfg_str(&telegram_config(conn), "token") {
    let _ = conn.execute(
      "INSERT INTO telegram_bots (id, name, token, agent_id, enabled, updated_at) VALUES ('bot-default','Telegram',?1,'manager',1,?2) ON CONFLICT(id) DO NOTHING",
      params![token, now()],
    );
  }
}

fn mask_token(t: &str) -> String { if t.is_empty() { String::new() } else { "••••••••".to_string() } }

#[tauri::command]
pub fn list_telegram_bots(app: AppHandle) -> Result<Vec<serde_json::Value>, String> {
  let conn = db(&app)?;
  Ok(load_bots(&conn).into_iter().map(|b| serde_json::json!({
    "id": b.id, "name": b.name, "agentId": b.agent_id, "terminalId": b.terminal_id, "enabled": b.enabled, "token": mask_token(&b.token),
    "webhookRegistered": b.webhook_registered,
    "allowedUsers": serde_json::from_str::<serde_json::Value>(&b.allowed_users).unwrap_or_else(|_| serde_json::json!([])),
  })).collect())
}

#[tauri::command]
pub fn save_telegram_bot(app: AppHandle, bot: serde_json::Value) -> Result<String, String> {
  let conn = db(&app)?;
  let id = bot.get("id").and_then(|v| v.as_str()).filter(|s| !s.is_empty()).map(|s| s.to_string())
    .unwrap_or_else(|| format!("bot-{}", chrono::Utc::now().timestamp_millis()));
  let name = bot.get("name").and_then(|v| v.as_str()).unwrap_or("Telegram").to_string();
  let agent_id = bot.get("agentId").and_then(|v| v.as_str()).unwrap_or("").to_string();
  let terminal_id = bot.get("terminalId").and_then(|v| v.as_str()).unwrap_or("").to_string();
  // Allowed Telegram user ids (empty = open).
  let allowed_users = bot.get("allowedUsers").and_then(|v| v.as_array()).map(|a| {
    let list: Vec<String> = a.iter().filter_map(|x| {
      x.as_i64().map(|n| n.to_string()).or_else(|| x.as_str().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()))
    }).collect();
    serde_json::to_string(&list).unwrap_or_else(|_| "[]".into())
  });
  let enabled = bot.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true);
  // Preserve the stored token when the UI echoes the mask or sends nothing.
  let incoming = bot.get("token").and_then(|v| v.as_str()).unwrap_or("");
  let token = if incoming.is_empty() || incoming == "••••••••" {
    load_bot(&conn, &id).map(|b| b.token).unwrap_or_default()
  } else { incoming.to_string() };
  let allowed_users = allowed_users.unwrap_or_else(|| load_bot(&conn, &id).map(|b| b.allowed_users).unwrap_or_else(|| "[]".into()));
  conn.execute(
    "INSERT INTO telegram_bots (id, name, token, agent_id, terminal_id, allowed_users, enabled, updated_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)
     ON CONFLICT(id) DO UPDATE SET name=excluded.name, token=excluded.token, agent_id=excluded.agent_id, terminal_id=excluded.terminal_id, allowed_users=excluded.allowed_users, enabled=excluded.enabled, updated_at=excluded.updated_at",
    params![id, name, token, agent_id, terminal_id, allowed_users, if enabled { 1 } else { 0 }, now()],
  ).map_err(|e| e.to_string())?;
  Ok(id)
}

#[tauri::command]
pub fn delete_telegram_bot(app: AppHandle, id: String) -> Result<(), String> {
  let conn = db(&app)?;
  conn.execute("DELETE FROM telegram_bots WHERE id=?1", params![id]).map_err(|e| e.to_string())?;
  Ok(())
}

// ---------------------------------------------------------------------------
// Ask the user (inline-keyboard buttons + free-text input)
// ---------------------------------------------------------------------------

enum Pending {
  Choice { options: Vec<String>, tx: tokio::sync::oneshot::Sender<String> },
  Text(tokio::sync::oneshot::Sender<String>),
}
static PENDING: std::sync::OnceLock<std::sync::Mutex<std::collections::HashMap<String, Pending>>> = std::sync::OnceLock::new();
fn pending() -> &'static std::sync::Mutex<std::collections::HashMap<String, Pending>> {
  PENDING.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}

// The chat an in-flight Telegram turn came from, so an agent tool can prompt it.
static ACTIVE_CHAT: std::sync::OnceLock<std::sync::Mutex<Option<(String, i64)>>> = std::sync::OnceLock::new();
fn active_chat_store() -> &'static std::sync::Mutex<Option<(String, i64)>> { ACTIVE_CHAT.get_or_init(|| std::sync::Mutex::new(None)) }
pub(crate) fn set_active_chat(v: Option<(String, i64)>) { if let Ok(mut s) = active_chat_store().lock() { *s = v; } }
pub(crate) fn active_chat() -> Option<(String, i64)> { active_chat_store().lock().ok().and_then(|s| s.clone()) }

async fn telegram_send_markup(token: &str, chat_id: i64, text: &str, reply_markup: serde_json::Value) -> Result<(), String> {
  let url = format!("https://api.telegram.org/bot{token}/sendMessage");
  let body = serde_json::json!({ "chat_id": chat_id, "text": text, "reply_markup": reply_markup });
  let resp = http::client().post(&url).json(&body).send().await.map_err(|e| format!("Could not reach Telegram: {e}"))?;
  if resp.status().is_success() { Ok(()) } else { Err(telegram_error(&resp.text().await.unwrap_or_default())) }
}

async fn telegram_answer_callback(token: &str, callback_id: &str, text: &str) {
  let url = format!("https://api.telegram.org/bot{token}/answerCallbackQuery");
  let _ = http::client().post(&url).json(&serde_json::json!({ "callback_query_id": callback_id, "text": text })).send().await;
}

// Sends a question with buttons and awaits the tap (or times out).
pub(crate) async fn ask_choice(app: &AppHandle, bot_id: &str, chat_id: i64, question: &str, options: &[String], timeout_secs: u64) -> Result<String, String> {
  let token = { let conn = db(app)?; load_bot(&conn, bot_id).map(|b| b.token).ok_or("Bot not found.")? };
  let token_id = generate_webhook_secret();
  let keyboard: Vec<serde_json::Value> = options.iter().enumerate()
    .map(|(i, o)| serde_json::json!([{ "text": o, "callback_data": format!("ask:{token_id}:{i}") }]))
    .collect();
  let (tx, rx) = tokio::sync::oneshot::channel();
  pending().lock().map_err(|_| "pending poisoned".to_string())?.insert(token_id.clone(), Pending::Choice { options: options.to_vec(), tx });
  telegram_send_markup(&token, chat_id, question, serde_json::json!({ "inline_keyboard": keyboard })).await?;
  match tokio::time::timeout(std::time::Duration::from_secs(timeout_secs), rx).await {
    Ok(Ok(v)) => Ok(v),
    _ => { if let Ok(mut m) = pending().lock() { m.remove(&token_id); } Err("No answer (timed out).".into()) }
  }
}

// Sends a question and resolves with the next text message from that chat.
pub(crate) async fn ask_text(app: &AppHandle, bot_id: &str, chat_id: i64, question: &str, timeout_secs: u64) -> Result<String, String> {
  let token = { let conn = db(app)?; load_bot(&conn, bot_id).map(|b| b.token).ok_or("Bot not found.")? };
  let key = format!("text:{}:{}", bot_id, chat_id);
  let (tx, rx) = tokio::sync::oneshot::channel();
  pending().lock().map_err(|_| "pending poisoned".to_string())?.insert(key.clone(), Pending::Text(tx));
  telegram_send_message(&token, chat_id, question).await?;
  match tokio::time::timeout(std::time::Duration::from_secs(timeout_secs), rx).await {
    Ok(Ok(v)) => Ok(v),
    _ => { if let Ok(mut m) = pending().lock() { m.remove(&key); } Err("No answer (timed out).".into()) }
  }
}

// Resolves a pending text input for a chat; true when consumed (don't route on).
fn resolve_text_input(bot_id: &str, chat_id: i64, text: &str) -> bool {
  let key = format!("text:{}:{}", bot_id, chat_id);
  let taken = pending().lock().ok().and_then(|mut m| m.remove(&key));
  if let Some(Pending::Text(tx)) = taken { let _ = tx.send(text.to_string()); true } else { false }
}

// Resolves a button tap. Returns (callback_query_id, chosen value) when consumed.
fn handle_callback(cq: &serde_json::Value) -> Option<(String, String)> {
  let data = cq.get("data").and_then(|d| d.as_str()).unwrap_or("");
  let cq_id = cq.get("id").and_then(|d| d.as_str()).unwrap_or("").to_string();
  let rest = data.strip_prefix("ask:")?;
  let mut parts = rest.splitn(2, ':');
  let token_id = parts.next()?;
  let idx: usize = parts.next().and_then(|s| s.parse().ok())?;
  let taken = pending().lock().ok().and_then(|mut m| m.remove(token_id))?;
  match taken {
    Pending::Choice { options, tx } => { let v = options.get(idx).cloned().unwrap_or_default(); let _ = tx.send(v.clone()); Some((cq_id, v)) }
    _ => None,
  }
}

// Command surface for the panel/future use.
#[tauri::command]
pub async fn telegram_ask(app: AppHandle, bot_id: String, chat_id: i64, question: String, options: Vec<String>, timeout_secs: Option<u64>) -> Result<String, String> {
  let t = timeout_secs.unwrap_or(300);
  if options.is_empty() { ask_text(&app, &bot_id, chat_id, &question, t).await }
  else { ask_choice(&app, &bot_id, chat_id, &question, &options, t).await }
}

// ---------------------------------------------------------------------------
// Telegram tunnel + webhook (one-click expose)
// ---------------------------------------------------------------------------

// Records a telegram message event (inbound or outbound) for the laos log feed.
fn telegram_log(conn: &Connection, direction: &str, chat_id: &str, text: &str, reply: &str, status: &str, detail: &str) {
  let _ = conn.execute(
    "INSERT INTO telegram_logs (direction, chat_id, text, reply, status, detail, created_at) VALUES (?1,?2,?3,?4,?5,?6,?7)",
    params![direction, chat_id, text, reply, status, detail, chrono::Utc::now().to_rfc3339()],
  );
  // Cap the log to the most recent 500 rows.
  let _ = conn.execute("DELETE FROM telegram_logs WHERE id NOT IN (SELECT id FROM telegram_logs ORDER BY id DESC LIMIT 500)", []);
}

// Telegram caps sendMessage at 4096 characters; anything longer is rejected whole.
fn split_for_telegram(text: &str) -> Vec<String> {
  const LIMIT: usize = 4000; // stay clear of the 4096 ceiling
  let mut out: Vec<String> = Vec::new();
  let mut cur = String::new();
  let mut len = 0usize;
  for line in text.split_inclusive('\n') {
    let line_len = line.chars().count();
    if len > 0 && len + line_len > LIMIT {
      out.push(std::mem::take(&mut cur));
      len = 0;
    }
    if line_len > LIMIT {
      for ch in line.chars() {
        if len >= LIMIT { out.push(std::mem::take(&mut cur)); len = 0; }
        cur.push(ch);
        len += 1;
      }
    } else {
      cur.push_str(line);
      len += line_len;
    }
  }
  if !cur.is_empty() { out.push(cur); }
  if out.is_empty() { out.push(String::new()); }
  out
}

// Pulls Telegram's own explanation out of an error body so the log says what
// actually went wrong instead of a bare status line.
fn telegram_error(body: &str) -> String {
  serde_json::from_str::<serde_json::Value>(body).ok()
    .and_then(|v| v.get("description").and_then(|d| d.as_str()).map(|s| s.to_string()))
    .unwrap_or_else(|| body.chars().take(200).collect())
}

// The model answers in Markdown, which Telegram's legacy `Markdown` mode barely
// accepts — one unbalanced `*`, `_` or `[` and it rejects the whole message with
// 400. Its HTML mode covers more ground and escapes cleanly, so convert to that
// first and fall back to the raw text if even that is refused. Formatting is
// optional here; delivery is not.
async fn telegram_send_chunk(url: &str, chat_id: i64, markdown: &str) -> Result<(), String> {
  let client = http::client();
  let send = |body: serde_json::Value| client.post(url).json(&body).send();

  let formatted = serde_json::json!({
    "chat_id": chat_id,
    "text": crate::tg_markdown::to_telegram_html(markdown),
    "parse_mode": "HTML",
  });
  let resp = send(formatted).await.map_err(|e| format!("Could not reach Telegram: {e}"))?;
  if resp.status().is_success() { return Ok(()); }
  let status = resp.status();
  let detail = telegram_error(&resp.text().await.unwrap_or_default());

  let plain = serde_json::json!({ "chat_id": chat_id, "text": markdown });
  let retry = send(plain).await.map_err(|e| format!("Could not reach Telegram: {e}"))?;
  if retry.status().is_success() { return Ok(()); }
  let retry_status = retry.status();
  let retry_detail = telegram_error(&retry.text().await.unwrap_or_default());
  Err(format!("{retry_status} {retry_detail} (formatted send failed: {status} {detail})"))
}

// Sends a reply, splitting anything over the message-size limit.
async fn telegram_send_message(token: &str, chat_id: i64, text: &str) -> Result<(), String> {
  let url = format!("https://api.telegram.org/bot{token}/sendMessage");
  for chunk in split_for_telegram(text) {
    telegram_send_chunk(&url, chat_id, &chunk).await?;
  }
  Ok(())
}

#[tauri::command]
pub fn list_telegram_logs(app: AppHandle) -> Result<Vec<serde_json::Value>, String> {
  let conn = db(&app)?;
  let mut stmt = conn.prepare("SELECT direction, chat_id, text, reply, status, detail, created_at FROM telegram_logs ORDER BY id DESC LIMIT 100").map_err(|e| e.to_string())?;
  let rows = stmt.query_map([], |row| {
    Ok(serde_json::json!({
      "direction": row.get::<_, String>(0)?, "chatId": row.get::<_, String>(1)?,
      "text": row.get::<_, String>(2)?, "reply": row.get::<_, String>(3)?,
      "status": row.get::<_, String>(4)?, "detail": row.get::<_, String>(5)?,
      "createdAt": row.get::<_, String>(6)?,
    }))
  }).map_err(|e| e.to_string())?;
  let mut out = Vec::new();
  for row in rows { out.push(row.map_err(|e| e.to_string())?); }
  Ok(out)
}

// Global state: current tunnel URL + whether a webhook is registered.
// Kept in a OnceLock so the polling loop and commands share it.
static TELEGRAM_TUNNEL_URL: std::sync::OnceLock<std::sync::Mutex<Option<String>>> = std::sync::OnceLock::new();
fn tunnel_url_store() -> &'static std::sync::Mutex<Option<String>> {
  TELEGRAM_TUNNEL_URL.get_or_init(|| std::sync::Mutex::new(None))
}

fn set_tunnel_url(u: Option<String>) { *tunnel_url_store().lock().unwrap() = u; }
fn get_tunnel_url() -> Option<String> { tunnel_url_store().lock().unwrap().clone() }

// Telegram turns run one at a time. The webhook server spawns a task per delivery
// and the polling loop runs its own, so a few quick messages would otherwise fire
// concurrent provider calls and trip the provider's rate limit — a burst is fine,
// a stampede is not.
static TELEGRAM_TURN_LOCK: std::sync::OnceLock<tokio::sync::Mutex<()>> = std::sync::OnceLock::new();

async fn telegram_manager_turn(app: &AppHandle, text: &str) -> String {
  let lock = TELEGRAM_TURN_LOCK.get_or_init(|| tokio::sync::Mutex::new(()));
  let _guard = lock.lock().await;
  manager_turn(app, text).await.unwrap_or_else(|e| format!("Manager error: {e}"))
}

// Telegram re-delivers a webhook update when our response is slow. Remember what
// we have already handled so a retry doesn't run the Manager a second time.
static SEEN_UPDATES: std::sync::OnceLock<std::sync::Mutex<std::collections::VecDeque<i64>>> = std::sync::OnceLock::new();

fn first_time_update(id: i64) -> bool {
  let store = SEEN_UPDATES.get_or_init(|| std::sync::Mutex::new(std::collections::VecDeque::new()));
  let Ok(mut q) = store.lock() else { return true };
  if q.contains(&id) { return false; }
  q.push_back(id);
  while q.len() > 256 { q.pop_front(); }
  true
}

// Reads the persisted tunnel URL from the telegram integration config (survives
// app restarts where the in-memory state is lost).
fn get_persisted_tunnel_url(conn: &Connection) -> Option<String> {
  cfg_str(&telegram_config(conn), "tunnelUrl")
}

// Spawns cloudflared (system binary) pointed at a local port. Returns the
// generated trycloudflare.com URL by scraping cloudflared's stdout.
#[tauri::command]
pub async fn telegram_start_tunnel(app: AppHandle, local_port: Option<u16>, on_progress: tauri::ipc::Channel<String>) -> Result<String, String> {
  let port = local_port.unwrap_or(14789);
  let step = |s: &str| { let _ = on_progress.send(s.to_string()); };
  if let Some(existing) = get_tunnel_url() { step("Tunnel already running"); return Ok(existing); }

  step("Looking for cloudflared…");
  let cloudflared = match which_cloudflared().await {
    Some(c) => { step("cloudflared found"); c }
    None => {
      step("cloudflared missing — downloading official release (~50MB)…");
      match which_cloudflared_download(&app).await {
        Some(c) => { step("cloudflared downloaded"); c }
        None => return Err("cloudflared not found and auto-download failed. Install it manually (https://developers.cloudflare.com/cloudflare-one/connections/connect-networks/downloads/) or place cloudflared.exe next to the app.".into()),
      }
    }
  };

  step("Starting tunnel…");
  let mut child = tokio::process::Command::new(&cloudflared)
    .arg("tunnel")
    .arg("--url")
    .arg(format!("http://127.0.0.1:{port}"))
    .arg("--no-autoupdate")
    .stdout(std::process::Stdio::piped())
    .stderr(std::process::Stdio::piped())
    .spawn()
    .map_err(|e| format!("Failed to start cloudflared: {e}"))?;

  step("Waiting for tunnel URL…");
  let stdout = child.stdout.take().ok_or("cloudflared stdout unavailable")?;
  let stderr = child.stderr.take().ok_or("cloudflared stderr unavailable")?;
  use tokio::io::AsyncBufReadExt;
  let mut out_reader = tokio::io::BufReader::new(stdout).lines();
  let mut err_reader = tokio::io::BufReader::new(stderr).lines();
  // cloudflared prints the tunnel URL to stderr (newer versions) or stdout.
  let mut url: Option<String> = None;
  let mut saw_line = false;
  for _ in 0..160 {
    tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    let mut found = false;
    // Read whatever lines are available from both streams.
    loop {
      match tokio::time::timeout(std::time::Duration::from_millis(50), out_reader.next_line()).await {
        Ok(Ok(Some(line))) => {
          saw_line = true;
          if let Some(idx) = line.find("https://") {
            let candidate = line[idx..].split_whitespace().next().unwrap_or("").trim_end_matches('.').to_string();
            if candidate.contains("trycloudflare.com") { url = Some(candidate.clone()); step(&format!("Tunnel URL: {candidate}")); found = true; break; }
          }
        }
        Ok(Ok(None)) | Err(_) => break,
        Ok(Err(_)) => break,
      }
    }
    if found { break; }
    loop {
      match tokio::time::timeout(std::time::Duration::from_millis(50), err_reader.next_line()).await {
        Ok(Ok(Some(line))) => {
          saw_line = true;
          let trimmed = line.trim();
          if let Some(idx) = trimmed.find("https://") {
            let candidate = trimmed[idx..].split_whitespace().next().unwrap_or("").trim_end_matches('.').to_string();
            if candidate.contains("trycloudflare.com") { url = Some(candidate.clone()); step(&format!("Tunnel URL: {candidate}")); found = true; break; }
          }
          // Surface meaningful stderr lines (connection errors, retries) as progress.
          if !trimmed.is_empty() && (trimmed.contains("ERR") || trimmed.contains("error") || trimmed.contains("failed") || trimmed.contains("connect") || trimmed.contains("retry")) {
            step(trimmed);
          }
        }
        Ok(Ok(None)) | Err(_) => break,
        Ok(Err(_)) => break,
      }
    }
    if found { break; }
    // Give up early if the process already exited without printing anything.
    if child.try_wait().map(|s| s.is_some()).unwrap_or(false) && !saw_line {
      break;
    }
  }

  let url = url.ok_or("Timed out waiting for cloudflared tunnel URL. Check that cloudflared can reach Cloudflare's edge (firewall/proxy?), or run cloudflared manually to see the error.")?;
  set_tunnel_url(Some(url.clone()));
  // Keep the child alive for the app lifetime (detached handle).
  let _ = child.id();
  std::mem::forget(child);

  // Persist the local port + tunnel URL, MERGING with any existing config
  // (never clobber the stored bot token).
  let conn = db(&app)?;
  let persisted = url.clone();
  write_telegram_config(&conn, true, move |o| {
    o.insert("tunnelUrl".into(), serde_json::json!(persisted));
    o.insert("tunnelPort".into(), serde_json::json!(port));
  })?;

  Ok(url)
}

// Finds the cloudflared binary: next to the app first, then on PATH.
async fn which_cloudflared() -> Option<std::path::PathBuf> {
  if let Ok(exe) = std::env::current_exe() {
    let sibling = exe.parent()?.join("cloudflared");
    if sibling.exists() { return Some(sibling); }
    #[cfg(windows)]
    {
      let sibling_exe = exe.parent()?.join("cloudflared.exe");
      if sibling_exe.exists() { return Some(sibling_exe); }
    }
  }
  // PATH lookup.
  let out = tokio::process::Command::new(if cfg!(windows) { "where" } else { "which" })
    .arg("cloudflared").output().await.ok()?;
  if out.status.success() {
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if !s.is_empty() { return Some(std::path::PathBuf::from(s.lines().next().unwrap_or(""))); }
  }
  None
}

// Downloads the official cloudflared release into the app data dir (one-shot, cached).
async fn which_cloudflared_download(app: &AppHandle) -> Option<std::path::PathBuf> {
  let dir = app.path().app_data_dir().ok()?;
  let _ = std::fs::create_dir_all(&dir);
  let name = if cfg!(windows) { "cloudflared.exe" } else { "cloudflared" };
  let target = dir.join(name);
  if target.exists() { return Some(target); }

  let url = if cfg!(windows) {
    "https://github.com/cloudflare/cloudflared/releases/latest/download/cloudflared-windows-amd64.exe"
  } else if cfg!(target_os = "macos") {
    "https://github.com/cloudflare/cloudflared/releases/latest/download/cloudflared-darwin-amd64.tgz"
  } else {
    "https://github.com/cloudflare/cloudflared/releases/latest/download/cloudflared-linux-amd64"
  };

  match http::client().get(url).send().await {
    Ok(resp) => {
      if !resp.status().is_success() { return None; }
      let bytes = match resp.bytes().await { Ok(b) => b, Err(_) => return None };
      if cfg!(target_os = "macos") {
        // The macOS release is a .tgz; the binary is cloudflared inside.
        let _ = std::fs::write(dir.join("cloudflared.tgz"), &bytes);
        // Best-effort extract via tar.
        let _ = tokio::process::Command::new("tar").args(["-xzf", dir.join("cloudflared.tgz").to_str().unwrap_or(""), "-C"]).arg(&dir).output().await;
        let extracted = dir.join("cloudflared");
        if extracted.exists() { return Some(extracted); }
        None
      } else {
        let ok = std::fs::write(&target, &bytes).is_ok();
        if ok {
          #[cfg(unix)]
          { use std::os::unix::fs::PermissionsExt; let _ = std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o755)); }
          Some(target)
        } else { None }
      }
    }
    Err(_) => None,
  }
}

// Registers the Telegram webhook to a user-provided public URL (their own
// Cloudflare named tunnel / custom domain). The tunnel itself is managed by
// the user's existing cloudflared setup — we just point Telegram at it.
#[tauri::command]
pub async fn telegram_register_custom_url(app: AppHandle, public_url: String, on_progress: tauri::ipc::Channel<String>) -> Result<String, String> {
  let conn = db(&app)?;
  let step = |s: &str| { let _ = on_progress.send(s.to_string()); };
  let token = telegram_token(&conn)?.ok_or("Telegram token not configured. Save it in the config above first.")?;
  let url = public_url.trim().trim_end_matches('/').to_string();
  if url.is_empty() || (!url.starts_with("https://") && !url.starts_with("http://")) {
    return Err("Public URL must start with http:// or https://".into());
  }
  let secret = ensure_webhook_secret(&conn)?;
  let webhook_url = format!("{url}/webhook/telegram");
  step(&format!("Registering webhook at {webhook_url}…"));
  let resp = http::client()
    .get(format!("https://api.telegram.org/bot{token}/setWebhook?url={webhook_url}&secret_token={secret}"))
    .send().await.map_err(|e| format!("setWebhook request failed: {e}"))?;
  let json: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
  if json.get("ok").and_then(|o| o.as_bool()).unwrap_or(false) {
    write_telegram_config(&conn, true, move |o| {
      o.insert("tunnelUrl".into(), serde_json::json!(url));
      o.insert("webhookRegistered".into(), serde_json::json!(true));
    })?;
    step("Webhook registered ✓");
    Ok(format!("Webhook registered at {webhook_url}"))
  } else {
    let desc = json.get("description").and_then(|d| d.as_str()).unwrap_or("setWebhook failed").to_string();
    step(&format!("setWebhook failed: {desc}"));
    Err(desc)
  }
}

// Registers the Telegram webhook to point at the tunnel URL.
#[tauri::command]
pub async fn telegram_register_webhook(app: AppHandle, on_progress: tauri::ipc::Channel<String>) -> Result<String, String> {
  let conn = db(&app)?;
  let step = |s: &str| { let _ = on_progress.send(s.to_string()); };
  step("Reading bot token…");
  let token = telegram_token(&conn)?.ok_or("Telegram token not configured.")?;
  step("Token OK — checking tunnel…");
  let url = get_tunnel_url().ok_or("Tunnel not running. Start the tunnel first.")?;
  let secret = ensure_webhook_secret(&conn)?;
  let webhook_url = format!("{url}/webhook/telegram");
  step(&format!("Registering webhook at {webhook_url}…"));
  let resp = http::client()
    .get(format!("https://api.telegram.org/bot{token}/setWebhook?url={webhook_url}&secret_token={secret}"))
    .send().await.map_err(|e| format!("setWebhook request failed: {e}"))?;
  let json: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
  if json.get("ok").and_then(|o| o.as_bool()).unwrap_or(false) {
    write_telegram_config(&conn, true, move |o| {
      o.insert("tunnelUrl".into(), serde_json::json!(url));
      o.insert("webhookRegistered".into(), serde_json::json!(true));
    })?;
    step("Webhook registered ✓");
    Ok(format!("Webhook registered at {webhook_url}"))
  } else {
    let desc = json.get("description").and_then(|d| d.as_str()).unwrap_or("setWebhook failed").to_string();
    step(&format!("setWebhook failed: {desc}"));
    Err(desc)
  }
}

// Registers a single bot's webhook at {base}/webhook/telegram/{botId} with a
// per-bot secret, so one tunnel serves many bots (routed by path).
#[tauri::command]
pub async fn telegram_set_bot_webhook(app: AppHandle, bot_id: String, public_url: Option<String>) -> Result<String, String> {
  let (token, existing_secret) = {
    let conn = db(&app)?;
    let bot = load_bot(&conn, &bot_id).ok_or("Bot not found.")?;
    if bot.token.trim().is_empty() { return Err("Bot has no token.".into()); }
    (bot.token, bot.webhook_secret)
  };
  let base = public_url.map(|u| u.trim().trim_end_matches('/').to_string()).filter(|u| !u.is_empty())
    .or_else(get_tunnel_url)
    .ok_or("No public URL — start a tunnel first, or provide one.")?;
  let secret = if existing_secret.trim().is_empty() { generate_webhook_secret() } else { existing_secret };
  let webhook_url = format!("{base}/webhook/telegram/{bot_id}");
  let resp = http::client()
    .get(format!("https://api.telegram.org/bot{token}/setWebhook?url={webhook_url}&secret_token={secret}"))
    .send().await.map_err(|e| format!("setWebhook request failed: {e}"))?;
  let json: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
  if !json.get("ok").and_then(|o| o.as_bool()).unwrap_or(false) {
    return Err(json.get("description").and_then(|d| d.as_str()).unwrap_or("setWebhook failed").to_string());
  }
  {
    let conn = db(&app)?;
    conn.execute("UPDATE telegram_bots SET webhook_secret=?1, webhook_registered=1, updated_at=?2 WHERE id=?3", params![secret, now(), bot_id]).map_err(|e| e.to_string())?;
  }
  Ok(format!("Webhook registered at {webhook_url}"))
}

// Clears a bot's webhook so its long-poll loop resumes.
#[tauri::command]
pub async fn telegram_clear_bot_webhook(app: AppHandle, bot_id: String) -> Result<(), String> {
  let token = { let conn = db(&app)?; load_bot(&conn, &bot_id).map(|b| b.token).unwrap_or_default() };
  if !token.trim().is_empty() {
    let _ = http::client().get(format!("https://api.telegram.org/bot{token}/deleteWebhook")).send().await;
  }
  let conn = db(&app)?;
  conn.execute("UPDATE telegram_bots SET webhook_registered=0, updated_at=?1 WHERE id=?2", params![now(), bot_id]).map_err(|e| e.to_string())?;
  Ok(())
}

// Unregisters the webhook and clears the tunnel state.
#[tauri::command]
pub async fn telegram_stop_tunnel(app: AppHandle) -> Result<(), String> {
  let conn = db(&app)?;
  let token = telegram_token(&conn).ok().flatten();
  if let Some(t) = token {
    let _ = http::client().get(format!("https://api.telegram.org/bot{t}/deleteWebhook")).send().await;
  }
  set_tunnel_url(None);
  // Clear the webhook flags too, otherwise the polling loop would keep pausing
  // for a webhook that no longer exists.
  let _ = write_telegram_config(&conn, false, |o| {
    o.insert("webhookRegistered".into(), serde_json::json!(false));
    o.remove("tunnelUrl");
    o.remove("tunnelPort");
  });
  Ok(())
}

// Called once at startup. Our cloudflared tunnel is a child of this process, so
// it never survives a restart — but Telegram still holds the webhook, which makes
// getUpdates fail and silently stalls message delivery. Clear it so long-polling
// takes over again. A user-managed custom domain has no `tunnelPort` and may still
// be alive, so that registration is left alone.
pub(crate) async fn telegram_reconcile_on_boot(app: AppHandle) {
  let Ok(conn) = db(&app) else { return };
  let cfg = telegram_config(&conn);
  let registered = cfg.get("webhookRegistered").and_then(|v| v.as_bool()).unwrap_or(false);
  let ephemeral = cfg.get("tunnelPort").is_some();

  if !registered {
    if ephemeral {
      let _ = write_telegram_config(&conn, false, |o| { o.remove("tunnelUrl"); o.remove("tunnelPort"); });
    }
    return;
  }

  if !ephemeral {
    if webhook_secret(&conn).is_none() {
      telegram_log(&conn, "sys", "", "", "", "error", "A webhook is registered without a secret token. Re-run the webhook registration to secure it.");
    }
    return;
  }

  if let Ok(Some(token)) = telegram_token(&conn) {
    let _ = http::client().get(format!("https://api.telegram.org/bot{token}/deleteWebhook")).send().await;
  }
  let _ = write_telegram_config(&conn, false, |o| {
    o.insert("webhookRegistered".into(), serde_json::json!(false));
    o.remove("tunnelUrl");
    o.remove("tunnelPort");
  });
  telegram_log(&conn, "sys", "", "", "", "info", "Tunnel from the previous session is gone — webhook cleared, long-polling resumed.");
}

#[tauri::command]
pub async fn telegram_tunnel_status(app: AppHandle) -> Result<serde_json::Value, String> {
  let conn = db(&app)?;
  let live = get_tunnel_url();
  let persisted = get_persisted_tunnel_url(&conn);
  // If the in-memory state is empty (e.g. after restart), fall back to the
  // persisted URL so the UI reflects what Telegram is actually pointed at.
  let url = live.clone().or(persisted);
  let registered: bool = {
    let mut stmt = conn.prepare("SELECT config_json FROM integration_configs WHERE id='telegram'").map_err(|e| e.to_string())?;
    let mut rows = stmt.query_map([], |row| row.get::<_, String>(0)).map_err(|e| e.to_string())?;
    match rows.next().transpose().map_err(|e| e.to_string())? {
      Some(cfg) => serde_json::from_str::<serde_json::Value>(&cfg).map(|v| v.get("webhookRegistered").and_then(|w| w.as_bool()).unwrap_or(false)).unwrap_or(false),
      None => false,
    }
  };
  Ok(serde_json::json!({ "tunnelUrl": url, "webhookRegistered": registered, "liveTunnel": live }))
}

// Full webhook health: local receiver port listening? tunnel up? webhook set
// with Telegram? what does Telegram report?
#[tauri::command]
pub async fn telegram_webhook_health(app: AppHandle) -> Result<serde_json::Value, String> {
  let conn = db(&app)?;
  let live = get_tunnel_url();
  let persisted = get_persisted_tunnel_url(&conn);
  let url = live.clone().or_else(|| persisted.clone());
  let registered: bool = {
    let mut stmt = conn.prepare("SELECT config_json FROM integration_configs WHERE id='telegram'").map_err(|e| e.to_string())?;
    let mut rows = stmt.query_map([], |row| row.get::<_, String>(0)).map_err(|e| e.to_string())?;
    match rows.next().transpose().map_err(|e| e.to_string())? {
      Some(cfg) => serde_json::from_str::<serde_json::Value>(&cfg).map(|v| v.get("webhookRegistered").and_then(|w| w.as_bool()).unwrap_or(false)).unwrap_or(false),
      None => false,
    }
  };

  // Is the local webhook receiver listening on port 14789?
  let receiver_up = tokio::net::TcpStream::connect(("127.0.0.1", 14789)).await.is_ok();

  // What does Telegram report about the webhook?
  let mut telegram_info = serde_json::json!({});
  if let Ok(Some(token)) = telegram_token(&conn) {
    let resp = http::client()
      .get(format!("https://api.telegram.org/bot{token}/getWebhookInfo"))
      .send().await;
    if let Ok(r) = resp {
      if let Ok(j) = r.json::<serde_json::Value>().await {
        telegram_info = j.get("result").cloned().unwrap_or(serde_json::json!({}));
      }
    }
  }

  // Detect a mismatch: Telegram's registered URL differs from the live tunnel.
  let telegram_url = telegram_info.get("url").and_then(|u| u.as_str()).map(|s| s.to_string()).unwrap_or_default();
  let mismatch = !telegram_url.is_empty() && live.is_some() && !telegram_url.contains(live.as_deref().unwrap_or(""));

  // End-to-end self-test: POST a probe to the registered webhook URL (through
  // the tunnel) and see if the local receiver answers. Proves the full chain.
  let mut tunnel_reachable = false;
  let mut probe_error = String::new();
  if !telegram_url.is_empty() {
    let client = reqwest::Client::builder()
      .timeout(std::time::Duration::from_secs(8))
      .build().unwrap_or_else(|_| http::client());
    // Send a minimal update that the receiver ignores but still answers 200 to.
    let probe = serde_json::json!({ "update_id": 0, "message": { "message_id": 0, "chat": { "id": 0, "type": "private" }, "date": 0, "text": "__health_probe__" } });
    let mut probe_req = client.post(format!("{telegram_url}/webhook/telegram")).json(&probe);
    // The receiver enforces the secret token, so the self-test must carry it too.
    if let Some(secret) = webhook_secret(&conn) {
      probe_req = probe_req.header("X-Telegram-Bot-Api-Secret-Token", secret);
    }
    match probe_req.send().await {
      Ok(r) => { tunnel_reachable = r.status().is_success(); if !tunnel_reachable { probe_error = format!("HTTP {}", r.status()); } }
      Err(e) => probe_error = e.to_string(),
    }
  }

  Ok(serde_json::json!({
    "tunnelUrl": url,
    "webhookRegistered": registered,
    "receiverListening": receiver_up,
    "liveTunnel": live,
    "urlMismatch": mismatch,
    "tunnelReachable": tunnel_reachable,
    "probeError": probe_error,
    "telegram": telegram_info,
  }))
}

// Case-insensitive lookup of a header value in a raw HTTP request.
fn header_value(req: &str, name: &str) -> Option<String> {
  let head = req.split("\r\n\r\n").next().unwrap_or("");
  head.lines().skip(1).find_map(|line| {
    let (k, v) = line.split_once(':')?;
    if k.trim().eq_ignore_ascii_case(name) { Some(v.trim().to_string()) } else { None }
  })
}

// Minimal HTTP server that receives Telegram webhook POSTs at /webhook/telegram
// and routes the update to the Manager (same path as the polling loop).
pub(crate) async fn telegram_webhook_server(app: AppHandle, port: u16) {
  use tokio::io::{AsyncReadExt, AsyncWriteExt};
  let listener = match tokio::net::TcpListener::bind(("127.0.0.1", port)).await {
    Ok(l) => l,
    Err(e) => {
      // Record the bind failure so it's visible in the webhook health UI.
      if let Ok(c) = db(&app) {
        telegram_log(&c, "sys", "", "", "", "error", &format!("Webhook receiver failed to bind port {port}: {e}"));
      }
      eprintln!("webhook server bind failed: {e}");
      return;
    }
  };
  loop {
    let Ok((mut socket, _)) = listener.accept().await else { continue };
    let app = app.clone();
    tokio::spawn(async move {
      let mut buf = vec![0u8; 8192];
      let n = match tokio::time::timeout(std::time::Duration::from_secs(5), socket.read(&mut buf)).await {
        Ok(Ok(n)) => n,
        _ => return,
      };
      let req = String::from_utf8_lossy(&buf[..n]).to_string();
      // Route by path: /webhook/telegram/<botId>, or /webhook/telegram (legacy).
      let path = req.split_whitespace().nth(1).unwrap_or("");
      if !path.starts_with("/webhook/telegram") { return; }
      let bot_id = path.strip_prefix("/webhook/telegram/").map(|s| s.trim_matches('/').to_string()).filter(|s| !s.is_empty());

      // Resolve the target bot (by path id), or the legacy single bot.
      let target: Option<(String, String, TelegramBot)> = {
        let conn = db(&app).ok();
        match &bot_id {
          Some(id) => conn.as_ref().and_then(|c| load_bot(c, id)).map(|b| (b.webhook_secret.clone(), b.token.clone(), b)),
          None => {
            let secret = conn.as_ref().and_then(|c| webhook_secret(c)).unwrap_or_default();
            let token = conn.as_ref().and_then(|c| telegram_token(c).ok().flatten());
            token.map(|t| (secret, t.clone(), TelegramBot {
              id: "bot-default".into(), name: "Telegram".into(), token: t, agent_id: "manager".into(), terminal_id: String::new(), allowed_users: "[]".into(),
              enabled: true, webhook_secret: String::new(), webhook_registered: true,
            }))
          }
        }
      };
      let Some((secret, token, bot)) = target else {
        let _ = socket.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 9\r\n\r\nNot Found").await;
        return;
      };
      // Authenticate: Telegram echoes the secret_token registered for this bot.
      if !secret.is_empty() {
        let supplied = header_value(&req, "X-Telegram-Bot-Api-Secret-Token").unwrap_or_default();
        if supplied != secret {
          if let Ok(c) = db(&app) { telegram_log(&c, "sys", "", "", "", "error", "Rejected a webhook POST with a missing or invalid secret token."); }
          let _ = socket.write_all(b"HTTP/1.1 403 Forbidden\r\nContent-Length: 9\r\n\r\nForbidden").await;
          return;
        }
      }
      // Extract the JSON body (after the blank line).
      let body = req.split("\r\n\r\n").nth(1).unwrap_or("");
      if let Ok(update) = serde_json::from_str::<serde_json::Value>(body.trim_end_matches('\0')) {
        let update_id = update.get("update_id").and_then(|u| u.as_i64()).unwrap_or(0);
        // Button taps.
        if let Some(cq) = update.get("callback_query") {
          let uid = cq.get("from").and_then(|f| f.get("id")).and_then(|i| i.as_i64());
          if is_authorized(&bot, uid) {
            if let Some((cq_id, value)) = handle_callback(cq) {
              let t2 = token.clone();
              tokio::spawn(async move { telegram_answer_callback(&t2, &cq_id, &value).await; });
            }
          }
          let _ = socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nOK").await;
          return;
        }
        let text = update.get("message").and_then(|m| m.get("text")).and_then(|t| t.as_str()).map(|s| s.to_string());
        let chat_id = update.get("message").and_then(|m| m.get("chat")).and_then(|c| c.get("id")).and_then(|c| c.as_i64());
        let user_id = update.get("message").and_then(|m| m.get("from")).and_then(|f| f.get("id")).and_then(|i| i.as_i64());
        // Health self-test probe — acknowledge without routing to the LLM.
        if text.as_deref() == Some("__health_probe__") {
          let _ = socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nOK").await;
          return;
        }
        // Anyone may learn their own id; private bots drop everyone else.
        if let (Some(t), Some(cid)) = (text.clone(), chat_id) {
          if matches!(t.trim(), "/id" | "/whoami") {
            let t2 = token.clone();
            let msg = format!("Your Telegram user id is {}.", user_id.map(|u| u.to_string()).unwrap_or_default());
            tokio::spawn(async move { let _ = telegram_send_message(&t2, cid, &msg).await; });
            let _ = socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nOK").await;
            return;
          }
        }
        if !authorize_and_bind(&app, &bot, user_id) {
          if let Ok(c) = db(&app) { telegram_log(&c, "in", &chat_id.map(|c| c.to_string()).unwrap_or_default(), text.as_deref().unwrap_or(""), "", "blocked", &bot.name); }
          let t2 = token.clone();
          if let Some(cid) = chat_id { tokio::spawn(async move { let _ = telegram_send_message(&t2, cid, "This bot is private.").await; }); }
          let _ = socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nOK").await;
          return;
        }
        if let (Some(text), Some(chat_id)) = (text, chat_id) {
          // A pending "next message" input consumes this without routing it on.
          if resolve_text_input(&bot.id, chat_id, &text) {
            if let Ok(c) = db(&app) { telegram_log(&c, "in", &chat_id.to_string(), &text, "", "input", &bot.name); }
            let _ = socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nOK").await;
            return;
          }
          if let Ok(c) = db(&app) { telegram_log(&c, "in", &chat_id.to_string(), &text, "", "received", &bot.name); }
          // Acknowledge before doing the model work, so Telegram doesn't time out
          // and re-deliver the update (which multiplies provider calls).
          let _ = socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nOK").await;
          if first_time_update(update_id) {
            tokio::spawn(async move {
              set_active_chat(Some((bot.id.clone(), chat_id)));
              let reply = route_bot_message(&app, &bot, chat_id, &text).await;
              set_active_chat(None);
              let send_result = telegram_send_message(&token, chat_id, &reply).await;
              if let Ok(c) = db(&app) {
                match send_result {
                  Ok(()) => telegram_log(&c, "out", &chat_id.to_string(), &text, &reply, "sent", &bot.name),
                  Err(e) => telegram_log(&c, "out", &chat_id.to_string(), &text, &reply, "error", &e),
                }
              }
            });
          }
          return;
        }
      }
      let _ = socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nOK").await;
    });
  }
}

// Output relay: a session bound to a bot relays a debounced tail of its output to
// the chat that last messaged it.
struct TermRelay { token: String, chat_id: i64, last_output: std::time::Instant, dirty: bool, last_sent: String }
static TERM_RELAY: std::sync::OnceLock<std::sync::Mutex<std::collections::HashMap<String, TermRelay>>> = std::sync::OnceLock::new();
fn term_relay() -> &'static std::sync::Mutex<std::collections::HashMap<String, TermRelay>> {
  TERM_RELAY.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}

// Chat-bound CLI sessions started with `/cli`: "{bot_id}:{chat_id}" -> terminal id.
static CHAT_CLI: std::sync::OnceLock<std::sync::Mutex<std::collections::HashMap<String, String>>> = std::sync::OnceLock::new();
fn chat_cli() -> &'static std::sync::Mutex<std::collections::HashMap<String, String>> {
  CHAT_CLI.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}
fn chat_cli_get(key: &str) -> Option<String> { chat_cli().lock().ok().and_then(|m| m.get(key).cloned()) }
fn chat_cli_set(key: &str, id: Option<&str>) {
  if let Ok(mut m) = chat_cli().lock() {
    match id { Some(v) => { m.insert(key.to_string(), v.to_string()); }, None => { m.remove(key); } }
  }
}
fn default_shell() -> String { if cfg!(windows) { "cmd".to_string() } else { "sh".to_string() } }

// Called by the terminal reader when a bound session produces output.
pub(crate) fn on_terminal_output(_app: &AppHandle, session_id: &str, _text: &str) {
  if let Ok(mut m) = term_relay().lock() {
    if let Some(r) = m.get_mut(session_id) { r.last_output = std::time::Instant::now(); r.dirty = true; }
  }
}

// Flushes quiet output to the bound chat (skips when the tail is unchanged, so a
// redrawing TUI doesn't spam).
pub(crate) async fn terminal_relay_loop(_app: AppHandle) {
  loop {
    tokio::time::sleep(std::time::Duration::from_millis(800)).await;
    let due: Vec<(String, String, i64)> = {
      let mut m = match term_relay().lock() { Ok(m) => m, Err(_) => continue };
      let mut out = Vec::new();
      for (sid, r) in m.iter_mut() {
        if r.dirty && r.last_output.elapsed() >= std::time::Duration::from_millis(1200) {
          out.push((sid.clone(), r.token.clone(), r.chat_id));
          r.dirty = false;
        }
      }
      out
    };
    for (sid, token, chat_id) in due {
      let tail = crate::terminal::tail_text(&sid, 30);
      if tail.trim().is_empty() { continue; }
      let changed = term_relay().lock().map(|m| m.get(&sid).map(|r| r.last_sent != tail).unwrap_or(false)).unwrap_or(false);
      if !changed { continue; }
      if let Ok(mut m) = term_relay().lock() { if let Some(r) = m.get_mut(&sid) { r.last_sent = tail.clone(); } }
      let _ = telegram_send_message(&token, chat_id, &tail).await;
    }
  }
}

// One long-poll loop per bot. Returns when the bot is disabled or removed.
async fn poll_bot(app: AppHandle, bot: TelegramBot) {
  let mut offset: i64 = 0;
  loop {
    // Reload the bot each pass; stop once it's disabled or deleted.
    let current = match db(&app).ok().and_then(|c| load_bot(&c, &bot.id)) {
      Some(b) if b.enabled && !b.token.trim().is_empty() => b,
      _ => return,
    };
    // A webhook registered for THIS bot makes Telegram reject its getUpdates, so
    // only this bot's polling pauses — other bots keep long-polling.
    if current.webhook_registered {
      tokio::time::sleep(std::time::Duration::from_secs(5)).await;
      continue;
    }
    let url = format!("https://api.telegram.org/bot{}/getUpdates?timeout=30&offset={offset}", current.token);
    let response = match http::client().get(&url).send().await {
      Ok(r) => r,
      Err(_) => { tokio::time::sleep(std::time::Duration::from_secs(5)).await; continue; }
    };
    let json: serde_json::Value = match response.json().await {
      Ok(v) => v,
      Err(_) => { tokio::time::sleep(std::time::Duration::from_secs(5)).await; continue; }
    };
    if !json.get("ok").and_then(|o| o.as_bool()).unwrap_or(false) {
      tokio::time::sleep(std::time::Duration::from_secs(5)).await;
      continue;
    }
    let updates = json.get("result").and_then(|r| r.as_array()).cloned().unwrap_or_default();
    for update in updates {
      let update_id = update.get("update_id").and_then(|u| u.as_i64()).unwrap_or(0);
      offset = update_id + 1;
      // Button taps (inline keyboard).
      if let Some(cq) = update.get("callback_query") {
        let uid = cq.get("from").and_then(|f| f.get("id")).and_then(|i| i.as_i64());
        if !is_authorized(&current, uid) { continue; }
        if let Some((cq_id, value)) = handle_callback(cq) {
          let _ = telegram_answer_callback(&current.token, &cq_id, &value).await;
          if let Ok(c) = db(&app) { telegram_log(&c, "in", "", &value, "", "callback", &current.name); }
        }
        continue;
      }
      let Some(text) = update.get("message").and_then(|m| m.get("text")).and_then(|t| t.as_str()).map(|s| s.to_string()) else { continue };
      let Some(chat_id) = update.get("message").and_then(|m| m.get("chat")).and_then(|c| c.get("id")).and_then(|c| c.as_i64()) else { continue };
      let user_id = update.get("message").and_then(|m| m.get("from")).and_then(|f| f.get("id")).and_then(|i| i.as_i64());
      // Anyone may learn their own id so it can be added to the allowlist.
      if matches!(text.trim(), "/id" | "/whoami") {
        let _ = telegram_send_message(&current.token, chat_id, &format!("Your Telegram user id is {}.", user_id.map(|u| u.to_string()).unwrap_or_default())).await;
        continue;
      }
      // Auth: an allowlist locks the bot to its ids; empty binds the first
      // sender as owner (trust-on-first-use), then locks to them.
      if !authorize_and_bind(&app, &current, user_id) {
        if let Ok(c) = db(&app) { telegram_log(&c, "in", &chat_id.to_string(), &text, "", "blocked", &current.name); }
        let _ = telegram_send_message(&current.token, chat_id, "This bot is private.").await;
        continue;
      }
      // A pending "next message" input consumes this without routing it on.
      if resolve_text_input(&current.id, chat_id, &text) {
        if let Ok(c) = db(&app) { telegram_log(&c, "in", &chat_id.to_string(), &text, "", "input", &current.name); }
        continue;
      }
      if let Ok(c) = db(&app) { telegram_log(&c, "in", &chat_id.to_string(), &text, "", "received", &current.name); }
      // Let an agent tool (ask_user) prompt this chat for the duration of the turn.
      set_active_chat(Some((current.id.clone(), chat_id)));
      let reply = route_bot_message(&app, &current, chat_id, &text).await;
      set_active_chat(None);
      let send_result = telegram_send_message(&current.token, chat_id, &reply).await;
      if let Ok(c) = db(&app) {
        match send_result {
          Ok(()) => telegram_log(&c, "out", &chat_id.to_string(), &text, &reply, "sent", &current.name),
          Err(e) => telegram_log(&c, "out", &chat_id.to_string(), &text, &reply, "error", &e),
        }
      }
    }
  }
}

// Routes an inbound bot message: local commands, else the bot's agent (or the
// Manager when the bot has none).
async fn route_bot_message(app: &AppHandle, bot: &TelegramBot, chat_id: i64, text: &str) -> String {
  match text.trim() {
    "/agents" | "/agents@" => match db(app) {
      Ok(c) => {
        let ctx = build_workspace_context(&c).unwrap_or_default();
        ctx.lines().take_while(|l| !l.starts_with("## Available Integrations")).collect::<Vec<_>>().join("\n")
      }
      Err(_) => "No agents.".into(),
    },
    "/tasks" | "/tasks@" => match db(app) {
      Ok(c) => list_tasks(&c).map(|ts| if ts.is_empty() { "No tasks.".into() } else { ts.iter().map(|t| format!("- {} -> {}: {} ({})", t.id, t.assigned_agent, t.input, t.status)).collect::<Vec<_>>().join("\n") }).unwrap_or_default(),
      Err(_) => "No tasks.".into(),
    },
    _ => {
      let key = format!("{}:{}", bot.id, chat_id);
      let cmd = text.trim();
      // `/cli [command]` — start a CLI session bound to this chat and pipe the
      // following messages into its stdin. This is a terminal, not a conversation:
      // it returns before the agent path, so nothing here is ever written to
      // memory (terminal sessions never teach memory).
      if cmd == "/cli" || cmd.starts_with("/cli ") {
        if let Some(prev) = chat_cli_get(&key) { let _ = crate::terminal::terminal_kill(prev); }
        let launch = cmd.strip_prefix("/cli").map(|s| s.trim().to_string()).unwrap_or_default();
        let command = if launch.is_empty() { default_shell() } else { launch };
        return match crate::terminal::terminal_start(app.clone(), format!("tg-{chat_id}"), command.clone(), None, None, None) {
          Ok(v) => {
            let sid = v.get("id").and_then(|x| x.as_str()).unwrap_or("").to_string();
            if sid.is_empty() { "Could not start the session.".into() } else {
              chat_cli_set(&key, Some(&sid));
              if let Ok(mut m) = term_relay().lock() {
                m.insert(sid.clone(), TermRelay { token: bot.token.clone(), chat_id, last_output: std::time::Instant::now(), dirty: false, last_sent: String::new() });
              }
              format!("CLI started (`{command}`). Send a command; /screen to peek, /exit to end.")
            }
          }
          Err(e) => format!("Could not start the CLI: {e}"),
        };
      }
      if matches!(cmd, "/exit" | "/cli-stop") || cmd == "/cli stop" {
        return match chat_cli_get(&key) {
          Some(sid) => { let _ = crate::terminal::terminal_kill(sid); chat_cli_set(&key, None); "CLI session ended.".into() }
          None => "No CLI session in this chat.".into(),
        };
      }
      // A chat-bound CLI session owns the message.
      if let Some(sid) = chat_cli_get(&key) {
        if !crate::terminal::session_alive(&sid) {
          chat_cli_set(&key, None);
          return "CLI session ended. Send /cli to start another.".into();
        }
        if cmd == "/screen" {
          let t = crate::terminal::tail_text(&sid, 40);
          return if t.trim().is_empty() { "(no output yet)".into() } else { t };
        }
        if let Err(e) = crate::terminal::write_line(&sid, text) { return format!("Terminal error: {e}"); }
        return String::new(); // output is relayed
      }
      // A bot bound to a terminal pipes the message into the CLI's stdin. If the
      // session isn't running we say so — never silently fall back to an agent.
      if !bot.terminal_id.is_empty() {
        if !crate::terminal::session_alive(&bot.terminal_id) {
          return format!("This bot is bound to terminal '{}', which isn't running. Start it in the Terminal view.", bot.terminal_id);
        }
        if text.trim() == "/screen" {
          let t = crate::terminal::tail_text(&bot.terminal_id, 40);
          return if t.trim().is_empty() { "(no output yet)".into() } else { t };
        }
        if let Ok(mut m) = term_relay().lock() {
          m.insert(bot.terminal_id.clone(), TermRelay { token: bot.token.clone(), chat_id, last_output: std::time::Instant::now(), dirty: false, last_sent: String::new() });
        }
        if let Err(e) = crate::terminal::write_line(&bot.terminal_id, text) { return format!("Terminal error: {e}"); }
        return String::new(); // no immediate reply — output is relayed
      }
      if bot.agent_id.is_empty() || bot.agent_id == "manager" {
        telegram_manager_turn(app, text).await
      } else {
        run_agent_reply(app, &bot.id, &bot.agent_id, chat_id, text).await
      }
    }
  }
}

// Answers a message with the bot's agent, in a persistent per-(bot, chat)
// conversation so the agent has context and its own memory answers. Serialized
// with Manager turns.
async fn run_agent_reply(app: &AppHandle, bot_id: &str, agent_id: &str, chat_id: i64, text: &str) -> String {
  let session_id = format!("tg-{}-{}", bot_id, chat_id);
  // Build the agent + the rolling conversation, then drop the connection before
  // awaiting (a rusqlite Connection is not held across an await point).
  let (agent, convo) = {
    let conn = match db(app) { Ok(c) => c, Err(e) => return format!("Error: {e}") };
    let agent = {
      let mut stmt = match conn.prepare("SELECT id, name, objective, model, tool_ids, integrations, home_path, permissions, skill_ids, reasoning FROM agents WHERE id=?1") { Ok(s) => s, Err(e) => return format!("Error: {e}") };
      let mut rows = match stmt.query_map(params![agent_id], |row| {
        let tool_ids: String = row.get(4)?; let integrations: String = row.get(5)?; let permissions: String = row.get(7)?; let skill_ids: String = row.get(8)?;
        Ok(crate::models::AgentRequest {
          id: row.get(0)?, name: row.get(1)?, objective: row.get(2)?, model: row.get(3)?,
          tool_ids: crate::storage::parse_json_vec(&tool_ids), integrations: crate::storage::parse_json_vec(&integrations),
          home_path: row.get(6)?, permissions: crate::storage::parse_json_vec(&permissions),
          skill_ids: crate::storage::parse_json_vec(&skill_ids), reasoning: row.get(9)?,
        })
      }) { Ok(r) => r, Err(e) => return format!("Error: {e}") };
      match rows.next().transpose() { Ok(Some(a)) => a, Ok(None) => return format!("Agent {agent_id} not found."), Err(e) => return format!("Error: {e}") }
    };
    // Last 10 turns of this bot+chat conversation, so the agent keeps context.
    let mut convo = String::new();
    let history = crate::memory::load_session_messages(&conn, &session_id);
    for m in history.iter().rev().take(10).collect::<Vec<_>>().into_iter().rev() {
      let role = m.get("role").and_then(|r| r.as_str()).unwrap_or("");
      let content = m.get("content").and_then(|c| c.as_str()).unwrap_or("");
      if !content.is_empty() { convo.push_str(&format!("{role}: {content}\n")); }
    }
    convo.push_str(&format!("user: {text}"));
    (agent, convo)
  };

  let lock = TELEGRAM_TURN_LOCK.get_or_init(|| tokio::sync::Mutex::new(()));
  let _guard = lock.lock().await;
  let mut events = Vec::new();
  match crate::agents::run_agent_once_structured(app, &agent, &convo, None, &mut events, false).await {
    Ok((out, _, _)) => {
      if let Ok(conn) = db(app) {
        let _ = crate::memory::append_session_message(&conn, &session_id, agent_id, "user", text);
        let _ = crate::memory::append_session_message(&conn, &session_id, agent_id, "assistant", &out);
      }
      out
    }
    Err(e) => format!("Agent error: {e}"),
  }
}

// Supervisor: keeps one poll task running per enabled bot, (re)spawning when a
// bot is added or re-enabled and letting tasks exit when one is removed.
static RUNNING_BOTS: std::sync::OnceLock<std::sync::Mutex<std::collections::HashSet<String>>> = std::sync::OnceLock::new();
fn running_bots() -> &'static std::sync::Mutex<std::collections::HashSet<String>> {
  RUNNING_BOTS.get_or_init(|| std::sync::Mutex::new(std::collections::HashSet::new()))
}

pub(crate) async fn telegram_loop(app: AppHandle) {
  loop {
    let bots = db(&app).map(|c| { migrate_legacy_bot(&c); load_bots(&c) }).unwrap_or_default();
    for bot in bots {
      if !bot.enabled || bot.token.trim().is_empty() { continue; }
      let running = running_bots().lock().map(|s| s.contains(&bot.id)).unwrap_or(true);
      if running { continue; }
      if let Ok(mut s) = running_bots().lock() { s.insert(bot.id.clone()); }
      let app_b = app.clone();
      let id = bot.id.clone();
      tauri::async_runtime::spawn(async move {
        poll_bot(app_b, bot).await;
        if let Ok(mut s) = running_bots().lock() { s.remove(&id); }
      });
    }
    tokio::time::sleep(std::time::Duration::from_secs(10)).await;
  }
}
