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
    IntegrationRecord { id: "calendar".into(), name: "Google Calendar".into(), provider: "google".into(), enabled: false, connected: false, config: serde_json::json!({}), actions: vec![
      IntegrationAction { name: "calendar_list_events".into(), description: "List upcoming events on a Google Calendar".into() },
      IntegrationAction { name: "calendar_create_event".into(), description: "Create a Google Calendar event".into() },
    ] },
    IntegrationRecord { id: "gmail".into(), name: "Gmail".into(), provider: "google".into(), enabled: false, connected: false, config: serde_json::json!({}), actions: vec![
      IntegrationAction { name: "gmail_send".into(), description: "Send an email through Gmail".into() },
      IntegrationAction { name: "gmail_list".into(), description: "List Gmail messages matching a query".into() },
    ] },
    IntegrationRecord { id: "telegram".into(), name: "Telegram".into(), provider: "telegram".into(), enabled: false, connected: false, config: serde_json::json!({}), actions: vec![
      IntegrationAction { name: "telegram_send".into(), description: "Send a Telegram message".into() },
    ] },
    // Web search connectors: one API key each.
    IntegrationRecord { id: "exa".into(), name: "Exa".into(), provider: "exa".into(), enabled: false, connected: false, config: serde_json::json!({}), actions: vec![
      IntegrationAction { name: "exa_search".into(), description: "Search the web with Exa (neural) and get ranked results".into() },
    ] },
    IntegrationRecord { id: "parallel".into(), name: "Parallel".into(), provider: "parallel".into(), enabled: false, connected: false, config: serde_json::json!({}), actions: vec![
      IntegrationAction { name: "parallel_search".into(), description: "Search the web with Parallel Web Systems".into() },
    ] },
    // Web search connectors: one API key each (kept API-key based, never OAuth).
    IntegrationRecord { id: "tavily".into(), name: "Tavily".into(), provider: "tavily".into(), enabled: false, connected: false, config: serde_json::json!({}), actions: vec![
      IntegrationAction { name: "tavily_search".into(), description: "Search the web with Tavily and get LLM-ready results".into() },
    ] },
    IntegrationRecord { id: "tinyfish".into(), name: "TinyFish".into(), provider: "tinyfish".into(), enabled: false, connected: false, config: serde_json::json!({}), actions: vec![
      IntegrationAction { name: "tinyfish_search".into(), description: "Search the live web with TinyFish (ranked, fresh results)".into() },
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
    // Secrets are stored as a sentinel; resolve them for the caller (a tool or a
    // connection test). The UI never calls this — it reads through mask_config.
    crate::vault::resolve_object(&format!("integration/{id}"), obj);
    for k in ["clientId", "clientSecret", "token", "apiKey", "accessToken", "refreshToken"] {
      if let Some(v) = obj.get_mut(k) {
        if !v.is_string() { *v = serde_json::Value::Null; }
      }
    }
  }
  Ok(out)
}

/// Whether a stored integration config carries a usable credential. The
/// `connected` column only tracks OAuth handshakes, so a saved API key or token
/// must count too — otherwise a configured integration reads as "not connected"
/// everywhere and is never exposed to an agent.
pub(crate) fn config_has_credential(cfg: &serde_json::Value) -> bool {
  ["token", "apiKey", "accessToken", "refreshToken"].iter().any(|k| {
    cfg.get(*k).and_then(|v| v.as_str()).map(|s| !s.trim().is_empty()).unwrap_or(false)
  })
}

/// Whether the integration with `id` is connected: the OAuth flag is set, or a
/// credential is stored. The single source of truth for the UI, the connections
/// list, the workspace context and the agent tool builder.
pub(crate) fn integration_connected(conn: &Connection, id: &str) -> bool {
  conn
    .query_row(
      "SELECT connected, config_json FROM integration_configs WHERE id=?1",
      params![id],
      |r| Ok((r.get::<_, i64>(0)? != 0, r.get::<_, String>(1)?)),
    )
    .ok()
    .map(|(connected, cfg)| {
      connected || config_has_credential(&serde_json::from_str::<serde_json::Value>(&cfg).unwrap_or_else(|_| serde_json::json!({})))
    })
    .unwrap_or(false)
}

fn mask_config(cfg: &serde_json::Value) -> serde_json::Value {
  let mut out = serde_json::Map::new();
  if let Some(obj) = cfg.as_object() {
    for (k, v) in obj {
      // Never expose secrets to the frontend; show a "set" marker instead.
      if k.to_lowercase().contains("token") || k.to_lowercase().contains("key") || k.to_lowercase().contains("secret") {
        if v.as_str().map(|s| !s.is_empty()).unwrap_or(false) { out.insert(k.clone(), serde_json::json!(crate::vault::MASK)); }
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
      // Connected when the OAuth handshake ran or a token/key is stored — a saved
      // API key counts, for every provider (Google included).
      d.connected = connected || config_has_credential(&parsed);
    }
    out.push(d);
  }
  Ok(out)
}

#[tauri::command]
pub fn save_integration_config(app: AppHandle, id: String, config: serde_json::Value) -> Result<(), String> {
  let conn = db(&app)?;
  let def = integration_definitions().into_iter().find(|d| d.id == id).ok_or("Unknown integration")?;
  // Merge with the *stored* config (sentinel and all) so the frontend's masked
  // values don't clobber real tokens, and only a genuinely new secret is written.
  let existing = conn
    .query_row("SELECT config_json FROM integration_configs WHERE id=?1", params![id], |r| r.get::<_, String>(0))
    .ok()
    .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
    .unwrap_or_else(|| serde_json::json!({}));
  let mut merged = merge_config(&existing, &config)?;
  if let Some(obj) = merged.as_object_mut() { crate::vault::protect_object(&format!("integration/{id}"), obj)?; }
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
      let masked = v.as_str().map(|s| s == crate::vault::MASK || s.is_empty()).unwrap_or(v.is_null());
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
  let token = cfg.get("token").and_then(|t| t.as_str())
    .or_else(|| cfg.get("apiKey").and_then(|t| t.as_str()))
    // Google integrations store an OAuth access token here, not a `token`.
    .or_else(|| cfg.get("accessToken").and_then(|t| t.as_str()))
    .unwrap_or("");
  if token.is_empty() { return Ok(false); }
  // Exa and Parallel take the key in an x-api-key header, so they're probed with
  // a tiny search instead of the Bearer GET below.
  if id == "exa" || id == "parallel" {
    let (url, body) = if id == "exa" {
      ("https://api.exa.ai/search", serde_json::json!({ "query": "ping", "numResults": 1, "contents": { "highlights": true } }))
    } else {
      ("https://api.parallel.ai/v1/search", serde_json::json!({ "objective": "ping", "search_queries": ["ping"], "mode": "turbo" }))
    };
    let response = http::client().post(url)
      .header("x-api-key", token)
      .json(&body).send().await.map_err(|e| e.to_string())?;
    let ok = response.status().is_success();
    conn.execute("UPDATE integration_configs SET connected=?1 WHERE id=?2", params![if ok { 1 } else { 0 }, id]).map_err(|e| e.to_string())?;
    return Ok(ok);
  }
  // Tavily posts its key in the JSON body; TinyFish takes it in an X-API-Key header.
  if id == "tavily" {
    let body = serde_json::json!({ "api_key": token, "query": "ping", "max_results": 1 });
    let response = http::client().post("https://api.tavily.com/search").json(&body).send().await.map_err(|e| e.to_string())?;
    let ok = response.status().is_success();
    conn.execute("UPDATE integration_configs SET connected=?1 WHERE id=?2", params![if ok { 1 } else { 0 }, id]).map_err(|e| e.to_string())?;
    return Ok(ok);
  }
  if id == "tinyfish" {
    let response = http::client().get("https://api.search.tinyfish.ai")
      .query(&[("query", "ping")])
      .header("X-API-Key", token)
      .send().await.map_err(|e| e.to_string())?;
    let ok = response.status().is_success();
    conn.execute("UPDATE integration_configs SET connected=?1 WHERE id=?2", params![if ok { 1 } else { 0 }, id]).map_err(|e| e.to_string())?;
    return Ok(ok);
  }
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

// A bundled Google "Desktop" OAuth client, so connecting Google is a single tap
// (no client id to paste). For installed apps Google does not treat the client
// secret as confidential; it still never reaches the renderer. Override per
// integration in its config, or point this at your own Google Cloud project.
const GOOGLE_CLIENT_ID: &str = "";
const GOOGLE_CLIENT_SECRET: &str = "";

fn oauth_redirect_uri() -> String { format!("http://127.0.0.1:{OAUTH_REDIRECT_PORT}/callback") }

fn is_google(id: &str) -> bool { matches!(id, "sheets" | "docs" | "calendar" | "gmail") }

/// The OAuth scopes a Google integration requests (each id asks only for what it
/// needs, so consent is legible).
fn google_scopes(id: &str) -> String {
  let scopes: &[&str] = match id {
    "sheets" => &["https://www.googleapis.com/auth/spreadsheets"],
    "docs" => &["https://www.googleapis.com/auth/documents"],
    "calendar" => &["https://www.googleapis.com/auth/calendar"],
    "gmail" => &["https://www.googleapis.com/auth/gmail.send", "https://www.googleapis.com/auth/gmail.readonly"],
    _ => &[],
  };
  scopes.join("%20")
}

/// The (client id, client secret) to use: the integration's own if set, else the
/// bundled client for a Google integration.
fn client_creds(cfg: &serde_json::Value, id: &str) -> (String, String) {
  let cid = cfg.get("clientId").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
  let csec = cfg.get("clientSecret").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
  if cid.is_empty() && is_google(id) { (GOOGLE_CLIENT_ID.to_string(), GOOGLE_CLIENT_SECRET.to_string()) } else { (cid, csec) }
}

// Builds the provider authorize URL. Client IDs come from the integration config
// (set in the UI), or the bundled Google client for Google integrations.
#[tauri::command]
pub fn start_oauth(app: AppHandle, id: String) -> Result<String, String> {
  let conn = db(&app)?;
  let cfg = integration_secret(&conn, &id)?;
  let (client_id, _) = client_creds(&cfg, &id);
  let redirect = oauth_redirect_uri();
  let url = match id.as_str() {
    "sheets" | "docs" | "calendar" | "gmail" => {
      if client_id.is_empty() {
        return Err(format!("OAuth for '{id}' needs a Google client id — set GOOGLE_CLIENT_ID or a client id in the config."));
      }
      format!(
        "https://accounts.google.com/o/oauth2/v2/auth?client_id={client_id}&redirect_uri={redirect}&response_type=code&scope={}&access_type=offline&prompt=consent",
        google_scopes(&id)
      )
    }
    "notion" => {
      if client_id.is_empty() {
        return Err(format!("OAuth for '{id}' needs a clientId. Set it in the integration config first."));
      }
      format!("https://api.notion.com/v1/oauth/authorize?client_id={client_id}&redirect_uri={redirect}&response_type=code&owner=user")
    }
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
  let (client_id, client_secret) = client_creds(&cfg, id);
  if client_id.is_empty() || client_secret.is_empty() {
    return Err("OAuth client ID/secret not configured for this integration.".into());
  }
  let token_url = match id {
    "sheets" | "docs" | "calendar" | "gmail" => "https://oauth2.googleapis.com/token",
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
  let tokens: serde_json::Value = crate::http::json_body(response).await?;
  let mut stored = cfg.clone();
  if let Some(obj) = stored.as_object_mut() {
    obj.insert("accessToken".into(), tokens.get("access_token").cloned().unwrap_or(serde_json::Value::Null));
    obj.insert("refreshToken".into(), tokens.get("refresh_token").cloned().unwrap_or(serde_json::Value::Null));
    // Google access tokens last ~1h; remember when this one dies so it can be
    // refreshed lazily instead of failing the next call.
    if let Some(secs) = tokens.get("expires_in").and_then(|v| v.as_i64()) {
      obj.insert("expiresAt".into(), serde_json::json!((chrono::Utc::now() + chrono::Duration::seconds(secs)).to_rfc3339()));
    }
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

/// A usable Google access token for a connection. Refreshes from the stored
/// refresh token when the cached one has expired (or is within a minute of it),
/// so a long-lived Google connection keeps working without a re-connect.
pub(crate) async fn google_token(app: &AppHandle, id: &str) -> Result<String, String> {
  let conn = db(app)?;
  let cfg = integration_secret(&conn, id)?;
  let access = cfg.get("accessToken").and_then(|v| v.as_str()).unwrap_or("").to_string();
  let refresh = cfg.get("refreshToken").and_then(|v| v.as_str()).unwrap_or("").to_string();
  let expires = cfg.get("expiresAt").and_then(|v| v.as_str()).and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok());
  let still_valid = expires
    .map(|e| e.with_timezone(&chrono::Utc) > chrono::Utc::now() + chrono::Duration::seconds(60))
    .unwrap_or(!access.is_empty());
  if still_valid && !access.is_empty() { return Ok(access); }
  if refresh.is_empty() {
    return if access.is_empty() { Err("Google connection has no token — connect it again.".into()) } else { Ok(access) };
  }
  let (client_id, client_secret) = client_creds(&cfg, id);
  let body = serde_json::json!({
    "client_id": client_id, "client_secret": client_secret,
    "refresh_token": refresh, "grant_type": "refresh_token",
  });
  let resp = http::client().post("https://oauth2.googleapis.com/token").json(&body).send().await.map_err(|e| e.to_string())?;
  if !resp.status().is_success() {
    return Err(format!("Google token refresh failed: {}", resp.text().await.unwrap_or_default()));
  }
  let tokens: serde_json::Value = crate::http::json_body(resp).await?;
  let new_access = tokens.get("access_token").and_then(|v| v.as_str()).unwrap_or("").to_string();
  if new_access.is_empty() { return Err("Google token refresh returned no access token.".into()); }
  let mut stored = cfg.clone();
  if let Some(obj) = stored.as_object_mut() {
    obj.insert("accessToken".into(), serde_json::json!(new_access));
    if let Some(secs) = tokens.get("expires_in").and_then(|v| v.as_i64()) {
      obj.insert("expiresAt".into(), serde_json::json!((chrono::Utc::now() + chrono::Duration::seconds(secs)).to_rfc3339()));
    }
    // Re-seal the secrets before persisting (the read resolved the sentinels).
    crate::vault::protect_object(&format!("integration/{id}"), obj)?;
  }
  conn.execute(
    "UPDATE integration_configs SET config_json=?2, updated_at=?3 WHERE id=?1",
    params![id, serde_json::to_string(&stored).map_err(|e| e.to_string())?, now()],
  ).map_err(|e| e.to_string())?;
  Ok(new_access)
}

/// Explicit refresh, callable from the UI. The lazy refresh in the Google action
/// path is the norm; this exists for a "reconnect"-style button.
#[tauri::command]
pub async fn refresh_google_token(app: AppHandle, id: String) -> Result<(), String> {
  google_token(&app, &id).await.map(|_| ())
}

#[cfg(test)]
mod tests {
  use super::*;

  /// The marker had been re-encoded twice, so a configured token rendered as
  /// mojibake and never matched the frontend's `MASK` — meaning the UI treated a
  /// set secret as an unknown value.
  #[test]
  fn secrets_are_masked_with_the_shared_marker() {
    let cfg = serde_json::json!({ "botToken": "123:abc", "publicUrl": "https://example.test" });
    let masked = mask_config(&cfg);
    assert_eq!(masked["botToken"], serde_json::json!(crate::vault::MASK));
    // Non-secret fields still pass through untouched.
    assert_eq!(masked["publicUrl"], serde_json::json!("https://example.test"));
    // An unset secret stays null rather than becoming a "set" marker.
    let empty = mask_config(&serde_json::json!({ "botToken": "" }));
    assert_eq!(empty["botToken"], serde_json::Value::Null);
  }
}
