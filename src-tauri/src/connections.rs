// The resource / connection / adapter layer.
//
// Three things, one vocabulary:
//   • ADAPTER   — a descriptor: how to talk to a kind of external service (endpoint,
//                 auth scheme, the config fields the UI renders, the actions it
//                 exposes). Built-ins are seeded from code; the `adapters` table is
//                 the catalog (built-in rows + any user-added adapter).
//   • CONNECTION — a configured instance of an adapter with stored credentials.
//                 Integrations live in `integration_configs`; LLM endpoints in
//                 `providers`. Both are surfaced here as one set.
//   • RESOURCE  — a concrete external object a connection exposes (a Sheet, a
//                 calendar, a doc, a channel). `resources` stores them.
//
// Reads go through this module so the UI sees one Connections surface; writes
// still target the owning table, so nothing that already works is disturbed.

use rusqlite::{params, Connection as Sql};
use serde::{Deserialize, Serialize};
use tauri::AppHandle;

use crate::db::db;

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Adapter {
  pub id: String,
  pub name: String,
  /// integration | llm | search | router | mcp
  pub category: String,
  /// The transport the adapter speaks (notion, airtable, google, openai_compat…).
  pub provider: String,
  /// api_key | oauth | none
  pub auth: String,
  pub base_url: String,
  pub caps: serde_json::Value,
  /// The credential fields the UI renders: [{ key, label, secret }].
  pub fields: serde_json::Value,
  /// The actions a connection exposes: [{ name, description }].
  pub actions: serde_json::Value,
  pub builtin: bool,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Connection {
  pub id: String,
  pub adapter_id: String,
  pub name: String,
  pub category: String,
  pub enabled: bool,
  pub connected: bool,
  pub builtin: bool,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Resource {
  #[serde(default)] pub id: String,
  #[serde(default)] pub connection_id: String,
  #[serde(default)] pub kind: String,
  #[serde(default)] pub name: String,
  #[serde(default)] pub external_id: String,
  #[serde(default)] pub url: String,
  #[serde(default)] pub meta: serde_json::Value,
  #[serde(default)] pub created_at: String,
  #[serde(default)] pub updated_at: String,
}

fn field(key: &str, label: &str, secret: bool) -> serde_json::Value {
  serde_json::json!({ "key": key, "label": label, "secret": secret })
}

/// The credential fields an adapter's connection form renders, by auth scheme.
fn fields_for(auth: &str) -> serde_json::Value {
  match auth {
    "oauth" => serde_json::json!([
      field("clientId", "Client ID", false),
      field("clientSecret", "Client secret", true),
    ]),
    "none" => serde_json::json!([]),
    _ => serde_json::json!([field("token", "API key", true)]),
  }
}

/// The auth scheme a built-in integration uses.
fn integration_auth(id: &str) -> &'static str {
  match id {
    "notion" | "sheets" | "docs" | "calendar" | "gmail" => "oauth",
    _ => "api_key",
  }
}

/// Seeds the `adapters` catalog from code (integration definitions + provider
/// built-ins). Idempotent: a re-run refreshes built-in rows and never touches a
/// user-added adapter.
pub(crate) fn seed_adapters(conn: &Sql) -> Result<(), String> {
  for d in crate::integrations::integration_definitions() {
    let auth = integration_auth(&d.id);
    let actions = serde_json::to_value(&d.actions).unwrap_or_else(|_| serde_json::json!([]));
    conn.execute(
      "INSERT INTO adapters (id, name, category, provider, auth, base_url, caps, fields, actions, builtin, created_at) \
       VALUES (?1,?2,'integration',?3,?4,'',?5,?6,?7,1,?8) \
       ON CONFLICT(id) DO UPDATE SET name=excluded.name, provider=excluded.provider, auth=excluded.auth, \
         fields=excluded.fields, actions=excluded.actions, builtin=1",
      params![d.id, d.name, d.provider, auth, "{}", fields_for(auth).to_string(), actions.to_string(), now()],
    ).map_err(|e| e.to_string())?;
  }
  for p in crate::provider::builtins() {
    // `Caps` is a plain struct (not Serialize), so build the JSON by hand.
    let caps = serde_json::json!({
      "chat": p.caps.chat, "vision": p.caps.vision, "embeddings": p.caps.embeddings,
      "transcription": p.caps.transcription, "speech": p.caps.speech,
    });
    let auth = match p.auth {
      crate::provider::Auth::None => "none",
      _ => "api_key",
    };
    conn.execute(
      "INSERT INTO adapters (id, name, category, provider, auth, base_url, caps, fields, actions, builtin, created_at) \
       VALUES (?1,?2,'llm',?1,?3,?4,?5,?6,'[]',1,?7) \
       ON CONFLICT(id) DO UPDATE SET name=excluded.name, auth=excluded.auth, base_url=excluded.base_url, \
         caps=excluded.caps, fields=excluded.fields, builtin=1",
      params![p.id, p.label, auth, p.base, caps.to_string(), fields_for(auth).to_string(), now()],
    ).map_err(|e| e.to_string())?;
  }
  // Monid is a tool router: one key reaches its whole catalog. It is driven
  // through its CLI (see tools/monid.rs), so it has no static actions.
  conn.execute(
    "INSERT INTO adapters (id, name, category, provider, auth, base_url, caps, fields, actions, builtin, created_at) \
     VALUES ('monid','Monid','router','monid','api_key','https://api.monid.ai','{}',?1,'[]',1,?2) \
     ON CONFLICT(id) DO UPDATE SET name=excluded.name, category=excluded.category, base_url=excluded.base_url, fields=excluded.fields, builtin=1",
    params![fields_for("api_key").to_string(), now()],
  ).map_err(|e| e.to_string())?;
  Ok(())
}

fn row_to_adapter(r: &rusqlite::Row) -> rusqlite::Result<Adapter> {
  let caps: String = r.get(6)?;
  let fields: String = r.get(7)?;
  let actions: String = r.get(8)?;
  Ok(Adapter {
    id: r.get(0)?, name: r.get(1)?, category: r.get(2)?, provider: r.get(3)?, auth: r.get(4)?,
    base_url: r.get(5)?,
    caps: serde_json::from_str(&caps).unwrap_or_else(|_| serde_json::json!({})),
    fields: serde_json::from_str(&fields).unwrap_or_else(|_| serde_json::json!([])),
    actions: serde_json::from_str(&actions).unwrap_or_else(|_| serde_json::json!([])),
    builtin: r.get::<_, i64>(9)? != 0,
  })
}

/// Every adapter in the catalog, ordered by category then name.
pub(crate) fn list_adapters_inner(conn: &Sql) -> Vec<Adapter> {
  let mut out = Vec::new();
  if let Ok(mut stmt) = conn.prepare(
    "SELECT id, name, category, provider, auth, base_url, caps, fields, actions, builtin FROM adapters ORDER BY category, name",
  ) {
    if let Ok(rows) = stmt.query_map([], row_to_adapter) {
      for r in rows { if let Ok(a) = r { out.push(a); } }
    }
  }
  out
}

/// The connections the app holds, as one set: integrations (from
/// `integration_configs`) plus LLM endpoints (built-ins + custom `providers`).
pub(crate) fn list_connections_inner(conn: &Sql) -> Vec<Connection> {
  let mut out = Vec::new();
  // Integrations.
  if let Ok(mut stmt) = conn.prepare("SELECT id, name, enabled, connected FROM integration_configs") {
    if let Ok(rows) = stmt.query_map([], |r| Ok(Connection {
      id: r.get::<_, String>(0)?,
      adapter_id: r.get::<_, String>(0)?,
      name: r.get::<_, String>(1)?,
      category: "integration".into(),
      enabled: r.get::<_, i64>(2)? != 0,
      connected: r.get::<_, i64>(3)? != 0,
      builtin: true,
    })) {
      for r in rows { if let Ok(c) = r { out.push(c); } }
    }
  }
  // LLM endpoints: built-ins are always present; custom rows carry their own key.
  for d in crate::provider::builtins() {
    let has_key = crate::vault::get(&format!("provider/{}", d.id)).is_some();
    out.push(Connection {
      id: d.id.clone(), adapter_id: d.id.clone(), name: d.label.clone(),
      category: "llm".into(), enabled: true,
      connected: has_key || !d.requires_key, builtin: true,
    });
  }
  if let Ok(mut stmt) = conn.prepare("SELECT id, label, api_key FROM providers") {
    if let Ok(rows) = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, Option<String>>(2)?))) {
      for r in rows.flatten() {
        let connected = r.2.is_some() || crate::vault::get(&format!("provider/{}", r.0)).is_some();
        out.push(Connection { id: r.0.clone(), adapter_id: r.0, name: r.1, category: "llm".into(), enabled: true, connected, builtin: false });
      }
    }
  }
  out
}

fn now() -> String { chrono::Local::now().to_rfc3339() }

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn list_adapters(app: AppHandle) -> Result<Vec<Adapter>, String> {
  let conn = db(&app)?;
  seed_adapters(&conn)?;
  Ok(list_adapters_inner(&conn))
}

#[tauri::command]
pub fn list_connections(app: AppHandle) -> Result<Vec<Connection>, String> {
  let conn = db(&app)?;
  Ok(list_connections_inner(&conn))
}

#[tauri::command]
pub fn list_resources(app: AppHandle) -> Result<Vec<Resource>, String> {
  let conn = db(&app)?;
  let mut out = Vec::new();
  let mut stmt = conn.prepare(
    "SELECT id, connection_id, kind, name, external_id, url, meta, created_at, updated_at FROM resources ORDER BY created_at DESC",
  ).map_err(|e| e.to_string())?;
  let rows = stmt.query_map([], |r| {
    let meta: String = r.get(6)?;
    Ok(Resource {
      id: r.get(0)?, connection_id: r.get(1)?, kind: r.get(2)?, name: r.get(3)?,
      external_id: r.get(4)?, url: r.get(5)?,
      meta: serde_json::from_str(&meta).unwrap_or_else(|_| serde_json::json!({})),
      created_at: r.get(7)?, updated_at: r.get(8)?,
    })
  }).map_err(|e| e.to_string())?;
  for r in rows { out.push(r.map_err(|e| e.to_string())?); }
  Ok(out)
}

#[tauri::command]
pub fn save_resource(app: AppHandle, resource: Resource) -> Result<Resource, String> {
  let conn = db(&app)?;
  let mut res = resource;
  if res.id.trim().is_empty() { res.id = format!("res-{}", chrono::Utc::now().timestamp_millis()); }
  if res.name.trim().is_empty() { res.name = res.external_id.clone(); }
  if res.created_at.trim().is_empty() { res.created_at = now(); }
  res.updated_at = now();
  conn.execute(
    "INSERT INTO resources (id, connection_id, kind, name, external_id, url, meta, created_at, updated_at) \
     VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9) \
     ON CONFLICT(id) DO UPDATE SET connection_id=excluded.connection_id, kind=excluded.kind, name=excluded.name, \
       external_id=excluded.external_id, url=excluded.url, meta=excluded.meta, updated_at=excluded.updated_at",
    params![res.id, res.connection_id, res.kind, res.name, res.external_id, res.url, res.meta.to_string(), res.created_at, res.updated_at],
  ).map_err(|e| e.to_string())?;
  Ok(res)
}

#[tauri::command]
pub fn delete_resource(app: AppHandle, id: String) -> Result<(), String> {
  let conn = db(&app)?;
  conn.execute("DELETE FROM resources WHERE id=?1", params![id]).map(|_| ()).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
  use super::*;

  fn catalog_db() -> Sql {
    let conn = Sql::open_in_memory().unwrap();
    conn.execute_batch(
      "CREATE TABLE adapters (id TEXT PRIMARY KEY, name TEXT NOT NULL, category TEXT NOT NULL, provider TEXT NOT NULL, \
       auth TEXT NOT NULL, base_url TEXT NOT NULL DEFAULT '', caps TEXT NOT NULL DEFAULT '{}', fields TEXT NOT NULL DEFAULT '[]', \
       actions TEXT NOT NULL DEFAULT '[]', builtin INTEGER NOT NULL DEFAULT 0, created_at TEXT NOT NULL);",
    ).unwrap();
    conn
  }

  /// Seeding is idempotent and marks built-ins read-only — a re-run must not
  /// duplicate rows or wipe the `builtin` flag.
  #[test]
  fn seeding_the_catalog_is_idempotent() {
    let conn = catalog_db();
    seed_adapters(&conn).unwrap();
    let first = list_adapters_inner(&conn);
    seed_adapters(&conn).unwrap();
    let second = list_adapters_inner(&conn);
    assert_eq!(first.len(), second.len());
    assert!(second.iter().all(|a| a.builtin), "seeded adapters are built-ins");
    assert!(second.iter().any(|a| a.category == "integration"));
    assert!(second.iter().any(|a| a.category == "llm"));
  }
}
