// Integrations: provider catalog, stored credentials (masked on read),
// connection testing, and the OAuth loopback flow.

use rusqlite::{params, Connection};
use tauri::AppHandle;

use crate::db::{db, now};
use crate::http;
use crate::models::*;

pub(crate) fn integration_definitions() -> Vec<IntegrationRecord> {
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

pub(crate) fn integration_secret(conn: &Connection, id: &str) -> Result<serde_json::Value, String> {
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
pub fn list_integrations(app: AppHandle) -> Result<Vec<IntegrationRecord>, String> {
  let conn = db(&app)?;
  let defs = integration_definitions();
  let mut out = Vec::new();
  for mut d in defs {
    let mut stmt = conn.prepare("SELECT config_json, enabled, connected FROM integration_configs WHERE id=?1").map_err(|e| e.to_string())?;
    let mut rows = stmt.query_map(params![d.id], |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? != 0, row.get::<_, i64>(2)? != 0))).map_err(|e| e.to_string())?;
    if let Some(Ok((cfg, enabled, connected))) = rows.next() {
      let parsed = serde_json::from_str::<serde_json::Value>(&cfg).unwrap_or_else(|_| serde_json::json!({}));
      d.config = mask_config(&parsed);
      d.enabled = enabled;
      // Token-based integrations count as connected when a token/key is stored
      // (the `connected` flag is really about OAuth handshakes).
      let has_token = parsed.get("token").and_then(|t| t.as_str()).map(|s| !s.is_empty()).unwrap_or(false)
        || parsed.get("apiKey").and_then(|t| t.as_str()).map(|s| !s.is_empty()).unwrap_or(false);
      d.connected = connected || (has_token && d.provider != "google");
    }
    out.push(d);
  }
  Ok(out)
}

#[tauri::command]
pub fn save_integration_config(app: AppHandle, id: String, config: serde_json::Value) -> Result<(), String> {
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
pub async fn test_integration(app: AppHandle, id: String) -> Result<bool, String> {
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
  let response = http::client().get(url)
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
pub fn start_oauth(app: AppHandle, id: String) -> Result<String, String> {
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
pub async fn connect_oauth(app: AppHandle, id: String) -> Result<String, String> {
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
  let response = http::client().post(token_url).header("Content-Type", "application/json").json(&body).send().await.map_err(|e| e.to_string())?;
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
pub async fn complete_oauth(app: AppHandle, id: String, code: String) -> Result<(), String> {
  complete_oauth_inner(&app, &id, &code).await
}
