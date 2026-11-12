// Every secret the app holds — provider API keys, tool headers, MCP server env,
// integration credentials, Telegram tokens — lives in the OS credential store
// (Windows Credential Manager, macOS Keychain, Secret Service on Linux), never in
// the database and never in the renderer.
//
// A secret-bearing column or JSON field stores the `REF` sentinel instead, and
// every read path resolves it through here. Nothing falls back to writing
// plaintext: if the store is unreachable the write fails and the caller surfaces
// the error, because a silent plaintext fallback is exactly the bug this module
// exists to remove.

use rusqlite::{params, Connection};
use serde_json::Value;

pub const SERVICE: &str = "local-agent-os";
/// Stands in for a secret that lives in the credential store.
pub const REF: &str = "__vault__";
/// Reserved for the frontend only: what a set-but-hidden secret renders as.
pub const MASK: &str = "••••••••";

fn entry(account: &str) -> Result<keyring::Entry, String> {
  keyring::Entry::new(SERVICE, account).map_err(|e| format!("No system credential store available: {e}"))
}

pub fn put(account: &str, secret: &str) -> Result<(), String> {
  entry(account)?
    .set_password(secret)
    .map_err(|e| format!("Could not save to the system credential store: {e}"))
}

pub fn get(account: &str) -> Option<String> {
  entry(account).ok()?.get_password().ok().filter(|s| !s.is_empty())
}

pub fn delete(account: &str) -> Result<(), String> {
  match entry(account)?.delete_credential() {
    Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
    Err(e) => Err(e.to_string()),
  }
}

pub fn is_ref(v: &str) -> bool {
  v == REF
}

/// Whether a field / header / env name holds a secret. Deliberately generous:
/// a name we guess wrong on is a secret left in the open.
pub fn is_secret_name(name: &str) -> bool {
  let n = name.to_ascii_lowercase();
  n.contains("token") || n.contains("key") || n.contains("secret") || n.contains("password")
    || n.contains("auth") || n.contains("bearer")
}

// ---------------------------------------------------------------------------
// JSON objects (MCP env, integration config)
// ---------------------------------------------------------------------------

/// Moves every secret-named string in `obj` into the credential store under
/// `base/<key>`, leaving `REF` behind. Values already at `REF` (an untouched
/// secret echoed back from the UI) pass through.
pub fn protect_object(base: &str, obj: &mut serde_json::Map<String, Value>) -> Result<(), String> {
  for (k, v) in obj.iter_mut() {
    if !is_secret_name(k) { continue; }
    let Some(s) = v.as_str().map(|s| s.to_string()) else { continue };
    if s.is_empty() || is_ref(&s) { continue; }
    put(&format!("{base}/{k}"), &s)?;
    *v = Value::String(REF.to_string());
  }
  Ok(())
}

/// Replaces every `REF` in `obj` with the real secret from the credential store.
pub fn resolve_object(base: &str, obj: &mut serde_json::Map<String, Value>) {
  for (k, v) in obj.iter_mut() {
    let Some(s) = v.as_str() else { continue };
    if !is_ref(s) { continue; }
    if let Some(secret) = get(&format!("{base}/{k}")) {
      *v = Value::String(secret);
    }
  }
}

// ---------------------------------------------------------------------------
// Migration
// ---------------------------------------------------------------------------

fn object_of(json: &str) -> serde_json::Map<String, Value> {
  serde_json::from_str::<Value>(json).ok().and_then(|v| v.as_object().cloned()).unwrap_or_default()
}

/// Moves every secret still stored in plaintext into the credential store,
/// leaving `REF` behind. Runs once at startup; a row whose secret cannot be
/// written is left exactly as it was, so nothing is ever lost.
pub fn migrate(conn: &Connection) -> Result<usize, String> {
  let mut moved = 0;

  // Model keys are gone. The provider owns the key now (`provider/<id>`), so a
  // key still hanging off a model row is stale — and worse, it used to shadow
  // the provider's key, which is why updating a provider's key had no effect.
  // Drop the vault copy and clear the column so the provider's key is the only
  // one a model can ever send.
  let mut stmt = conn.prepare("SELECT id FROM model_configs WHERE api_key IS NOT NULL AND api_key != ''").map_err(|e| e.to_string())?;
  let stale: Vec<String> = stmt.query_map([], |r| r.get::<_, String>(0)).map_err(|e| e.to_string())?
    .filter_map(|r| r.ok()).collect();
  drop(stmt);
  for id in stale {
    let _ = delete(&format!("model/{id}"));
    conn.execute("UPDATE model_configs SET api_key=NULL WHERE id=?1", params![id]).map_err(|e| e.to_string())?;
    moved += 1;
  }

  // Generic tool headers (`config_json.headers[]`).
  let mut stmt = conn.prepare("SELECT id, config_json FROM tools").map_err(|e| e.to_string())?;
  let tool_rows: Vec<(String, String)> = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?))).map_err(|e| e.to_string())?
    .filter_map(|r| r.ok()).collect();
  drop(stmt);
  for (id, json) in tool_rows {
    let mut cfg = object_of(&json);
    let Some(Value::Array(headers)) = cfg.get_mut("headers") else { continue };
    let mut changed = false;
    for h in headers.iter_mut() {
      let Some(obj) = h.as_object_mut() else { continue };
      let name = obj.get("name").and_then(|n| n.as_str()).unwrap_or("").to_string();
      if !is_secret_name(&name) { continue; }
      let Some(value) = obj.get("value").and_then(|v| v.as_str()).map(|s| s.to_string()) else { continue };
      if value.is_empty() || is_ref(&value) { continue; }
      if put(&format!("tool/{id}/{name}"), &value).is_err() { continue; }
      obj.insert("value".into(), Value::String(REF.to_string()));
      changed = true;
    }
    if changed {
      conn.execute("UPDATE tools SET config_json=?1 WHERE id=?2", params![Value::Object(cfg).to_string(), id]).map_err(|e| e.to_string())?;
      moved += 1;
    }
  }

  // MCP server env and integration configs.
  for (table, col, base) in [("mcp_servers", "env", "mcp"), ("integration_configs", "config_json", "integration")] {
    let sql = format!("SELECT id, {col} FROM {table}");
    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let rows: Vec<(String, String)> = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?))).map_err(|e| e.to_string())?
      .filter_map(|r| r.ok()).collect();
    drop(stmt);
    for (id, json) in rows {
      let mut obj = object_of(&json);
      let before = obj.clone();
      if protect_object(&format!("{base}/{id}"), &mut obj).is_err() { continue; }
      if obj != before {
        let sql = format!("UPDATE {table} SET {col}=?1 WHERE id=?2");
        conn.execute(&sql, params![Value::Object(obj).to_string(), id]).map_err(|e| e.to_string())?;
        moved += 1;
      }
    }
  }

  // Telegram bot token + webhook secret (plain columns).
  let mut stmt = conn.prepare("SELECT id, token, webhook_secret FROM telegram_bots").map_err(|e| e.to_string())?;
  let bots: Vec<(String, String, String)> = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).map_err(|e| e.to_string())?
    .filter_map(|r| r.ok()).collect();
  drop(stmt);
  for (id, token, secret) in bots {
    if !token.is_empty() && !is_ref(&token) && put(&format!("telegram/{id}/token"), &token).is_ok() {
      conn.execute("UPDATE telegram_bots SET token=?1 WHERE id=?2", params![REF, id]).map_err(|e| e.to_string())?;
      moved += 1;
    }
    if !secret.is_empty() && !is_ref(&secret) && put(&format!("telegram/{id}/webhook"), &secret).is_ok() {
      conn.execute("UPDATE telegram_bots SET webhook_secret=?1 WHERE id=?2", params![REF, id]).map_err(|e| e.to_string())?;
      moved += 1;
    }
  }

  Ok(moved)
}
