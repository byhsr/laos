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

// Reads the Telegram bot token from the telegram integration config, if enabled.
fn telegram_token(conn: &Connection) -> Result<Option<String>, String> {
  let mut stmt = conn.prepare("SELECT config_json FROM integration_configs WHERE id='telegram'").map_err(|e| e.to_string())?;
  let mut rows = stmt.query_map([], |row| row.get::<_, String>(0)).map_err(|e| e.to_string())?;
  match rows.next().transpose().map_err(|e| e.to_string())? {
    Some(cfg_json) => {
      let cfg: serde_json::Value = serde_json::from_str(&cfg_json).map_err(|e| e.to_string())?;
      Ok(cfg.get("token").and_then(|t| t.as_str()).filter(|s| !s.is_empty()).map(|s| s.to_string()))
    }
    None => Ok(None),
  }
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

// Reads the persisted tunnel URL from the telegram integration config (survives
// app restarts where the in-memory state is lost).
fn get_persisted_tunnel_url(conn: &Connection) -> Option<String> {
  let mut stmt = conn.prepare("SELECT config_json FROM integration_configs WHERE id='telegram'").ok()?;
  let mut rows = stmt.query_map([], |row| row.get::<_, String>(0)).ok()?;
  let cfg = rows.next().transpose().ok()??;
  let v: serde_json::Value = serde_json::from_str(&cfg).ok()?;
  v.get("tunnelUrl").and_then(|t| t.as_str()).map(|s| s.to_string())
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
  let existing: serde_json::Value = {
    let mut stmt = conn.prepare("SELECT config_json FROM integration_configs WHERE id='telegram'").map_err(|e| e.to_string())?;
    let mut rows = stmt.query_map([], |row| row.get::<_, String>(0)).map_err(|e| e.to_string())?;
    match rows.next().transpose().map_err(|e| e.to_string())? {
      Some(cfg) => serde_json::from_str(&cfg).unwrap_or(serde_json::json!({})),
      None => serde_json::json!({}),
    }
  };
  let mut merged = existing;
  if let Some(obj) = merged.as_object_mut() {
    obj.insert("tunnelUrl".into(), serde_json::json!(url));
    obj.insert("tunnelPort".into(), serde_json::json!(port));
  }
  conn.execute(
    "INSERT INTO integration_configs (id, name, provider, config_json, enabled, connected, updated_at) VALUES ('telegram','Telegram','telegram',?1,1,1,?2)
     ON CONFLICT(id) DO UPDATE SET config_json=excluded.config_json, connected=1",
    params![serde_json::to_string(&merged).map_err(|e| e.to_string())?, now()],
  ).map_err(|e| e.to_string())?;

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
  let webhook_url = format!("{url}/webhook/telegram");
  step(&format!("Registering webhook at {webhook_url}…"));
  let resp = http::client()
    .get(format!("https://api.telegram.org/bot{token}/setWebhook?url={webhook_url}"))
    .send().await.map_err(|e| format!("setWebhook request failed: {e}"))?;
  let json: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
  if json.get("ok").and_then(|o| o.as_bool()).unwrap_or(false) {
    // Merge into config, preserving the token.
    let existing: serde_json::Value = {
      let mut stmt = conn.prepare("SELECT config_json FROM integration_configs WHERE id='telegram'").map_err(|e| e.to_string())?;
      let mut rows = stmt.query_map([], |row| row.get::<_, String>(0)).map_err(|e| e.to_string())?;
      match rows.next().transpose().map_err(|e| e.to_string())? {
        Some(cfg) => serde_json::from_str(&cfg).unwrap_or(serde_json::json!({})),
        None => serde_json::json!({}),
      }
    };
    let mut merged = existing;
    if let Some(obj) = merged.as_object_mut() {
      obj.insert("tunnelUrl".into(), serde_json::json!(url));
      obj.insert("webhookRegistered".into(), serde_json::json!(true));
    }
    conn.execute(
      "INSERT INTO integration_configs (id, name, provider, config_json, enabled, connected, updated_at) VALUES ('telegram','Telegram','telegram',?1,1,1,?2)
       ON CONFLICT(id) DO UPDATE SET config_json=excluded.config_json, connected=1",
      params![serde_json::to_string(&merged).map_err(|e| e.to_string())?, now()],
    ).map_err(|e| e.to_string())?;
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
  let webhook_url = format!("{url}/webhook/telegram");
  step(&format!("Registering webhook at {webhook_url}…"));
  let resp = http::client()
    .get(format!("https://api.telegram.org/bot{token}/setWebhook?url={webhook_url}"))
    .send().await.map_err(|e| format!("setWebhook request failed: {e}"))?;
  let json: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
  if json.get("ok").and_then(|o| o.as_bool()).unwrap_or(false) {
    // Merge webhook status into config, preserving the stored token.
    let existing: serde_json::Value = {
      let mut stmt = conn.prepare("SELECT config_json FROM integration_configs WHERE id='telegram'").map_err(|e| e.to_string())?;
      let mut rows = stmt.query_map([], |row| row.get::<_, String>(0)).map_err(|e| e.to_string())?;
      match rows.next().transpose().map_err(|e| e.to_string())? {
        Some(cfg) => serde_json::from_str(&cfg).unwrap_or(serde_json::json!({})),
        None => serde_json::json!({}),
      }
    };
    let mut merged = existing;
    if let Some(obj) = merged.as_object_mut() {
      obj.insert("tunnelUrl".into(), serde_json::json!(url));
      obj.insert("webhookRegistered".into(), serde_json::json!(true));
    }
    conn.execute(
      "INSERT INTO integration_configs (id, name, provider, config_json, enabled, connected, updated_at) VALUES ('telegram','Telegram','telegram',?1,1,1,?2)
       ON CONFLICT(id) DO UPDATE SET config_json=excluded.config_json, connected=1",
      params![serde_json::to_string(&merged).map_err(|e| e.to_string())?, now()],
    ).map_err(|e| e.to_string())?;
    step("Webhook registered ✓");
    Ok(format!("Webhook registered at {webhook_url}"))
  } else {
    let desc = json.get("description").and_then(|d| d.as_str()).unwrap_or("setWebhook failed").to_string();
    step(&format!("setWebhook failed: {desc}"));
    Err(desc)
  }
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
  let _ = conn.execute("UPDATE integration_configs SET connected=0 WHERE id='telegram'", []);
  Ok(())
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
    match client.post(format!("{telegram_url}/webhook/telegram")).json(&probe).send().await {
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
      // Only handle POST /webhook/telegram.
      if !req.starts_with("POST /webhook/telegram") { return; }
      // Extract the JSON body (after the blank line).
      let body = req.split("\r\n\r\n").nth(1).unwrap_or("");
      if let Ok(update) = serde_json::from_str::<serde_json::Value>(body.trim_end_matches('\0')) {
        let text = update.get("message").and_then(|m| m.get("text")).and_then(|t| t.as_str()).map(|s| s.to_string());
        let chat_id = update.get("message").and_then(|m| m.get("chat")).and_then(|c| c.get("id")).and_then(|c| c.as_i64());
        // Health self-test probe — acknowledge without routing to the LLM.
        if text.as_deref() == Some("__health_probe__") {
          let _ = socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nOK").await;
          return;
        }
        if let (Some(text), Some(chat_id)) = (text, chat_id) {
          if let Ok(c) = db(&app) { telegram_log(&c, "in", &chat_id.to_string(), &text, "", "received", "webhook"); }
          let reply = manager_turn(&app, &text).await.unwrap_or_else(|e| format!("Manager error: {e}"));
          if let Ok(token) = db(&app).and_then(|c| telegram_token(&c)) {
            if let Some(t) = token {
              let send_url = format!("https://api.telegram.org/bot{t}/sendMessage");
              let send_result = http::client().post(&send_url).json(&serde_json::json!({ "chat_id": chat_id, "text": reply, "parse_mode": "Markdown" })).send().await;
              if let Ok(c) = db(&app) {
                match send_result {
                  Ok(r) if r.status().is_success() => telegram_log(&c, "out", &chat_id.to_string(), &text, &reply, "sent", "webhook"),
                  Ok(r) => telegram_log(&c, "out", &chat_id.to_string(), &text, &reply, "error", &format!("Telegram returned {}", r.status())),
                  Err(e) => telegram_log(&c, "out", &chat_id.to_string(), &text, &reply, "error", &e.to_string()),
                }
              }
            }
          }
        }
      }
      let _ = socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nOK").await;
    });
  }
}

// Long-polls the Telegram Bot API; every incoming text message is routed to the
// Manager (same logic as the UI chat) and the reply is sent back. Thin channel:
// no business logic lives here.
pub(crate) async fn telegram_loop(app: AppHandle) {
  loop {
    let token = match db(&app).and_then(|c| telegram_token(&c)) {
      Ok(Some(t)) => t,
      _ => { tokio::time::sleep(std::time::Duration::from_secs(5)).await; continue; }
    };
    // If a tunnel webhook is active, long-polling must pause (Telegram rejects
    // getUpdates while a webhook is set).
    if get_tunnel_url().is_some() {
      tokio::time::sleep(std::time::Duration::from_secs(5)).await;
      continue;
    }
    let mut offset: i64 = 0;
    loop {
      let url = format!("https://api.telegram.org/bot{token}/getUpdates?timeout=30&offset={offset}");
      let response = match http::client().get(&url).send().await {
        Ok(r) => r,
        Err(_) => { tokio::time::sleep(std::time::Duration::from_secs(5)).await; continue; }
      };
      let json: serde_json::Value = match response.json().await {
        Ok(v) => v,
        Err(_) => { tokio::time::sleep(std::time::Duration::from_secs(5)).await; continue; }
      };
      let updates = json.get("result").and_then(|r| r.as_array()).cloned().unwrap_or_default();
      for update in updates {
        let update_id = update.get("update_id").and_then(|u| u.as_i64()).unwrap_or(0);
        offset = update_id + 1;
        let Some(text) = update.get("message").and_then(|m| m.get("text")).and_then(|t| t.as_str()).map(|s| s.to_string()) else { continue };
        let Some(chat_id) = update.get("message").and_then(|m| m.get("chat")).and_then(|c| c.get("id")).and_then(|c| c.as_i64()) else { continue };
        if let Ok(c) = db(&app) { telegram_log(&c, "in", &chat_id.to_string(), &text, "", "received", ""); }
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
        let send_result = http::client().post(&send_url).json(&serde_json::json!({ "chat_id": chat_id, "text": reply, "parse_mode": "Markdown" })).send().await;
        if let Ok(c) = db(&app) {
          match send_result {
            Ok(r) if r.status().is_success() => telegram_log(&c, "out", &chat_id.to_string(), &text, &reply, "sent", ""),
            Ok(r) => telegram_log(&c, "out", &chat_id.to_string(), &text, &reply, "error", &format!("Telegram returned {}", r.status())),
            Err(e) => telegram_log(&c, "out", &chat_id.to_string(), &text, &reply, "error", &e.to_string()),
          }
        }
      }
    }
  }
}
