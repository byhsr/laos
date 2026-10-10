// Native agent memory: the durable, searchable memory store that used to be the
// fox/`agent-memory` MCP server, reimplemented in-process on the app's SQLite DB.
//
// Data model:
//   am_events      — raw episodes (what happened)
//   am_memories    — durable typed memories (episodic/semantic/procedural/policy/
//                    preference/reflection), with confidence/importance/tags
//   am_self_model  — a keyed self-model per (agent, scope)
//   am_namespaces  — a registry of tracked scopes (projects)
//
// Everything is scoped: an "agent" scope is the agent's own memory, a "project"
// scope groups a project's memory across conversations.

use rusqlite::{params, Connection};
use serde_json::{json, Value};
use tauri::AppHandle;

use crate::db::db;

const MEMORY_KINDS: [&str; 6] = ["episodic", "semantic", "procedural", "policy", "preference", "reflection"];

fn now_ms() -> i64 { chrono::Utc::now().timestamp_millis() }

fn new_id(prefix: &str) -> String {
  let n = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
  format!("{prefix}-{n:x}")
}

/// `scope` is a compact key: `agent:<id>`, `project:<id>`, or `global`.
pub(crate) fn scope_key(scope_type: &str, scope_id: &str) -> String {
  match scope_type {
    "global" | "" => "global".to_string(),
    t => format!("{t}:{scope_id}"),
  }
}

fn scope_parts(key: &str) -> (String, String) {
  match key.split_once(':') {
    Some((t, id)) => (t.to_string(), id.to_string()),
    None => ("global".to_string(), String::new()),
  }
}

pub(crate) fn ensure_schema(conn: &Connection) -> Result<(), String> {
  conn.execute_batch(
    "CREATE TABLE IF NOT EXISTS am_events (id TEXT PRIMARY KEY, agent_id TEXT NOT NULL, scope TEXT NOT NULL, scope_type TEXT NOT NULL DEFAULT 'agent', type TEXT NOT NULL, role TEXT NOT NULL DEFAULT '', content TEXT NOT NULL DEFAULT '', data TEXT NOT NULL DEFAULT '{}', importance REAL NOT NULL DEFAULT 0.5, occurred_at INTEGER NOT NULL);
     CREATE INDEX IF NOT EXISTS am_events_scope ON am_events(scope, occurred_at DESC);
     CREATE TABLE IF NOT EXISTS am_memories (id TEXT PRIMARY KEY, agent_id TEXT NOT NULL, scope TEXT NOT NULL, scope_type TEXT NOT NULL DEFAULT 'agent', kind TEXT NOT NULL DEFAULT 'semantic', status TEXT NOT NULL DEFAULT 'active', title TEXT NOT NULL DEFAULT '', content TEXT NOT NULL DEFAULT '', tags TEXT NOT NULL DEFAULT '[]', confidence REAL NOT NULL DEFAULT 0.8, importance REAL NOT NULL DEFAULT 0.5, source TEXT, expires_at INTEGER, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL);
     CREATE INDEX IF NOT EXISTS am_memories_scope ON am_memories(scope, updated_at DESC);
     CREATE TABLE IF NOT EXISTS am_self_model (id TEXT PRIMARY KEY, agent_id TEXT NOT NULL, scope TEXT NOT NULL DEFAULT 'global', scope_type TEXT NOT NULL DEFAULT 'global', key TEXT NOT NULL, value TEXT NOT NULL, confidence REAL NOT NULL DEFAULT 0.8, importance REAL NOT NULL DEFAULT 0.7, source TEXT, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL, UNIQUE(agent_id, scope, key));
     CREATE TABLE IF NOT EXISTS am_namespaces (key TEXT PRIMARY KEY, type TEXT NOT NULL, id TEXT, name TEXT NOT NULL, description TEXT, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL, metadata TEXT);",
  ).map_err(|e| e.to_string())
}

fn tags_json(v: &Value) -> String {
  v.as_array().map(|a| {
    let list: Vec<String> = a.iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect();
    serde_json::to_string(&list).unwrap_or_else(|_| "[]".into())
  }).unwrap_or_else(|| "[]".into())
}

fn tags_of(s: &str) -> Value { serde_json::from_str(s).unwrap_or_else(|_| json!([])) }

// Make a scope plus its parent 'global' visible together (inherit).
fn scopes_for(scope: &str) -> Vec<String> {
  if scope == "global" { vec!["global".to_string()] } else { vec![scope.to_string(), "global".to_string()] }
}

// ---------------------------------------------------------------------------
// Events
// ---------------------------------------------------------------------------

pub(crate) fn record_event(conn: &Connection, agent_id: &str, scope: &str, etype: &str, role: &str, content: &str, data: &Value, importance: f64) -> Result<(), String> {
  let (st, _) = scope_parts(scope);
  conn.execute(
    "INSERT INTO am_events (id, agent_id, scope, scope_type, type, role, content, data, importance, occurred_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
    params![new_id("ev"), agent_id, scope, st, etype, role, content, serde_json::to_string(data).unwrap_or_else(|_| "{}".into()), importance.clamp(0.0, 1.0), now_ms()],
  ).map_err(|e| e.to_string())?;
  Ok(())
}

// ---------------------------------------------------------------------------
// Memories
// ---------------------------------------------------------------------------

pub(crate) fn add_memory(conn: &Connection, agent_id: &str, scope: &str, kind: &str, title: &str, content: &str, tags: &Value, confidence: f64, importance: f64) -> Result<String, String> {
  let kind = if MEMORY_KINDS.contains(&kind) { kind } else { "semantic" };
  let (st, _) = scope_parts(scope);
  let id = new_id("mem");
  let ts = now_ms();
  conn.execute(
    "INSERT INTO am_memories (id, agent_id, scope, scope_type, kind, status, title, content, tags, confidence, importance, created_at, updated_at) VALUES (?1,?2,?3,?4,?5,'active',?6,?7,?8,?9,?10,?11,?11)",
    params![id, agent_id, scope, st, kind, title, content, tags_json(tags), confidence.clamp(0.0, 1.0), importance.clamp(0.0, 1.0), ts],
  ).map_err(|e| e.to_string())?;
  Ok(id)
}

fn memory_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Value> {
  let tags: String = row.get("tags")?;
  Ok(json!({
    "id": row.get::<_, String>("id")?, "agentId": row.get::<_, String>("agent_id")?,
    "scope": row.get::<_, String>("scope")?, "kind": row.get::<_, String>("kind")?,
    "status": row.get::<_, String>("status")?, "title": row.get::<_, String>("title")?,
    "content": row.get::<_, String>("content")?, "tags": tags_of(&tags),
    "confidence": row.get::<_, f64>("confidence")?, "importance": row.get::<_, f64>("importance")?,
    "updatedAt": row.get::<_, i64>("updated_at")?,
  }))
}

pub(crate) fn get_memory(conn: &Connection, id: &str) -> Option<Value> {
  conn.query_row("SELECT * FROM am_memories WHERE id=?1", params![id], memory_row).ok()
}

pub(crate) fn list_memories(conn: &Connection, scope: &str, limit: usize) -> Vec<Value> {
  let scopes = scopes_for(scope);
  let mut out = Vec::new();
  let Ok(mut stmt) = conn.prepare("SELECT * FROM am_memories WHERE scope IN (?1, ?2) AND status != 'archived' ORDER BY importance DESC, updated_at DESC LIMIT ?3") else { return out };
  if let Ok(rows) = stmt.query_map(params![scopes[0], scopes[1], limit as i64], memory_row) {
    for r in rows { if let Ok(v) = r { out.push(v); } }
  }
  out
}

pub(crate) fn update_memory(conn: &Connection, id: &str, patch: &Value) -> Result<(), String> {
  let mut sets: Vec<String> = vec!["updated_at = ?1".into()];
  let mut vals: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(now_ms())];
  let push = |sets: &mut Vec<String>, vals: &mut Vec<Box<dyn rusqlite::ToSql>>, col: &str, v: Box<dyn rusqlite::ToSql>| {
    sets.push(format!("{col} = ?{}", vals.len() + 1));
    vals.push(v);
  };
  if let Some(t) = patch.get("title").and_then(|v| v.as_str()) { push(&mut sets, &mut vals, "title", Box::new(t.to_string())); }
  if let Some(c) = patch.get("content").and_then(|v| v.as_str()) { push(&mut sets, &mut vals, "content", Box::new(c.to_string())); }
  if let Some(k) = patch.get("kind").and_then(|v| v.as_str()) { push(&mut sets, &mut vals, "kind", Box::new(k.to_string())); }
  if let Some(s) = patch.get("status").and_then(|v| v.as_str()) { push(&mut sets, &mut vals, "status", Box::new(s.to_string())); }
  if let Some(t) = patch.get("tags") { push(&mut sets, &mut vals, "tags", Box::new(tags_json(t))); }
  if let Some(c) = patch.get("confidence").and_then(|v| v.as_f64()) { push(&mut sets, &mut vals, "confidence", Box::new(c.clamp(0.0, 1.0))); }
  if let Some(i) = patch.get("importance").and_then(|v| v.as_f64()) { push(&mut sets, &mut vals, "importance", Box::new(i.clamp(0.0, 1.0))); }
  let sql = format!("UPDATE am_memories SET {} WHERE id = ?{}", sets.join(", "), vals.len() + 1);
  vals.push(Box::new(id.to_string()));
  let refs: Vec<&dyn rusqlite::ToSql> = vals.iter().map(|b| b.as_ref()).collect();
  conn.execute(&sql, refs.as_slice()).map_err(|e| e.to_string())?;
  Ok(())
}

pub(crate) fn forget_memory(conn: &Connection, id: &str) -> Result<(), String> {
  conn.execute("DELETE FROM am_memories WHERE id=?1", params![id]).map_err(|e| e.to_string())?;
  Ok(())
}

// Lexical + importance + recency blend (no embeddings).
pub(crate) fn search(conn: &Connection, scope: &str, query: &str, limit: usize) -> Vec<Value> {
  let q = query.trim().to_lowercase();
  let tokens: Vec<&str> = q.split_whitespace().filter(|t| !t.is_empty()).collect();
  let now = now_ms();
  let scopes = scopes_for(scope);
  let mut out: Vec<Value> = Vec::new();
  let Ok(mut stmt) = conn.prepare("SELECT * FROM am_memories WHERE scope IN (?1, ?2) AND status != 'archived'") else { return out };
  let rows = match stmt.query_map(params![scopes[0], scopes[1]], memory_row) { Ok(r) => r, Err(_) => return out };
  for r in rows {
    let Ok(m) = r else { continue };
    let hay = format!("{} {}", m["title"].as_str().unwrap_or(""), m["content"].as_str().unwrap_or("")).to_lowercase();
    let hits = tokens.iter().filter(|t| hay.contains(*t)).count() as f64;
    if hits == 0.0 && !q.is_empty() { continue; }
    let lexical = if tokens.is_empty() { 1.0 } else { hits / tokens.len() as f64 };
    let importance = m["importance"].as_f64().unwrap_or(0.5);
    let updated = m["updatedAt"].as_i64().unwrap_or(now);
    let days = ((now - updated) as f64 / 86_400_000.0).max(0.0);
    let recency = 1.0 / (1.0 + days / 14.0);
    let score = 0.5 * lexical + 0.3 * importance + 0.2 * recency;
    out.push(json!({ "memory": m, "score": score }));
  }
  out.sort_by(|a, b| b["score"].as_f64().partial_cmp(&a["score"].as_f64()).unwrap_or(std::cmp::Ordering::Equal));
  out.truncate(limit);
  out
}

// ---------------------------------------------------------------------------
// Self-model
// ---------------------------------------------------------------------------

pub(crate) fn set_self_model(conn: &Connection, agent_id: &str, scope: &str, key: &str, value: &str, confidence: f64, importance: f64) -> Result<(), String> {
  let (st, _) = scope_parts(scope);
  let ts = now_ms();
  conn.execute(
    "INSERT INTO am_self_model (id, agent_id, scope, scope_type, key, value, confidence, importance, created_at, updated_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?9)
     ON CONFLICT(agent_id, scope, key) DO UPDATE SET value=excluded.value, confidence=excluded.confidence, importance=excluded.importance, updated_at=excluded.updated_at",
    params![new_id("self"), agent_id, scope, st, key, value, confidence.clamp(0.0, 1.0), importance.clamp(0.0, 1.0), ts],
  ).map_err(|e| e.to_string())?;
  Ok(())
}

pub(crate) fn self_model_entries(conn: &Connection, agent_id: &str, scope: Option<&str>) -> Vec<Value> {
  let mut out = Vec::new();
  let Ok(mut stmt) = conn.prepare("SELECT * FROM am_self_model WHERE agent_id=?1 AND (?2 IS NULL OR scope=?2) ORDER BY importance DESC, key") else { return out };
  let scope_owned: Option<String> = scope.map(|s| s.to_string());
  let rows = match stmt.query_map(params![agent_id, scope_owned], |row| Ok(json!({
    "id": row.get::<_, String>("id")?, "key": row.get::<_, String>("key")?,
    "value": row.get::<_, String>("value")?, "confidence": row.get::<_, f64>("confidence")?,
    "importance": row.get::<_, f64>("importance")?, "updatedAt": row.get::<_, i64>("updated_at")?,
  }))) { Ok(r) => r, Err(_) => return out };
  for r in rows { if let Ok(v) = r { out.push(v); } }
  out
}

pub(crate) fn remove_self_model(conn: &Connection, agent_id: &str, scope: &str, key: &str) -> Result<(), String> {
  conn.execute("DELETE FROM am_self_model WHERE agent_id=?1 AND scope=?2 AND key=?3", params![agent_id, scope, key]).map_err(|e| e.to_string())?;
  Ok(())
}

// ---------------------------------------------------------------------------
// Context compilation (token-budgeted markdown)
// ---------------------------------------------------------------------------

pub(crate) fn compile_context(conn: &Connection, agent_id: &str, scope: &str, token_budget: usize) -> String {
  let char_budget = token_budget * 4;
  let mut out = String::new();
  let selfm = self_model_entries(conn, agent_id, Some(scope));
  if !selfm.is_empty() {
    out.push_str("## Self\n");
    for e in selfm.iter().take(8) {
      out.push_str(&format!("- {}: {}\n", e["key"].as_str().unwrap_or(""), e["value"].as_str().unwrap_or("")));
    }
  }
  for (kind, label) in [("policy", "Policies"), ("procedural", "Procedures"), ("preference", "Preferences"), ("semantic", "Facts")] {
    let mems: Vec<Value> = list_memories(conn, scope, 200).into_iter().filter(|m| m["kind"].as_str() == Some(kind)).take(6).collect();
    if mems.is_empty() { continue; }
    out.push_str(&format!("## {label}\n"));
    for m in mems { out.push_str(&format!("- {}\n", m["title"].as_str().unwrap_or(""))); }
  }
  if out.len() > char_budget { out.truncate(char_budget); out.push_str("\n…"); }
  out
}

// ---------------------------------------------------------------------------
// Reflection (heuristic: repeated negative outcomes → a reflection memory)
// ---------------------------------------------------------------------------

pub(crate) fn reflect(conn: &Connection, agent_id: &str, scope: &str, persist: bool) -> Vec<Value> {
  let scopes = scopes_for(scope);
  let mut negatives: i64 = 0;
  if let Ok(mut stmt) = conn.prepare("SELECT data FROM am_events WHERE scope IN (?1, ?2) AND type='outcome' ORDER BY occurred_at DESC LIMIT 50") {
    if let Ok(rows) = stmt.query_map(params![scopes[0], scopes[1]], |r| r.get::<_, String>(0)) {
      for r in rows {
        if let Ok(d) = r {
          if serde_json::from_str::<Value>(&d).ok().and_then(|v| v.get("signal").and_then(|s| s.as_str()).map(|s| s == "down")).unwrap_or(false) { negatives += 1; }
        }
      }
    }
  }
  if negatives < 2 { return Vec::new(); }
  let draft = json!({ "kind": "reflection", "title": "Recurring negative feedback", "content": format!("{negatives} recent turns were rated poorly — revisit the approach before repeating it.") });
  if persist {
    let _ = add_memory(conn, agent_id, scope, "reflection", draft["title"].as_str().unwrap_or(""), draft["content"].as_str().unwrap_or(""), &json!([]), 0.7, 0.7);
  }
  vec![draft]
}

pub(crate) fn stats(conn: &Connection) -> Value {
  let count = |t: &str| conn.query_row(&format!("SELECT COUNT(*) FROM {t}"), [], |r| r.get::<_, i64>(0)).unwrap_or(0);
  json!({ "events": count("am_events"), "memories": count("am_memories"), "selfModelEntries": count("am_self_model") })
}

// ---------------------------------------------------------------------------
// Namespaces (tracked scopes; a project registers one)
// ---------------------------------------------------------------------------

pub(crate) fn create_namespace(conn: &Connection, scope: &str, name: &str, description: &str) -> Result<(), String> {
  let (st, id) = scope_parts(scope);
  let ts = now_ms();
  let name = if name.trim().is_empty() { scope.to_string() } else { name.to_string() };
  conn.execute(
    "INSERT INTO am_namespaces (key, type, id, name, description, created_at, updated_at, metadata) VALUES (?1,?2,?3,?4,?5,?6,?6,'{}')
     ON CONFLICT(key) DO UPDATE SET name=excluded.name, description=excluded.description, updated_at=excluded.updated_at",
    params![scope, st, id, name, description, ts],
  ).map_err(|e| e.to_string())?;
  Ok(())
}

pub(crate) fn remove_namespace(conn: &Connection, scope: &str) -> Result<(), String> {
  conn.execute("DELETE FROM am_namespaces WHERE key=?1", params![scope]).map_err(|e| e.to_string())?;
  Ok(())
}

fn namespace_counts(conn: &Connection, scope: &str) -> (i64, i64, i64, i64) {
  let c = |t: &str| conn.query_row(&format!("SELECT COUNT(*) FROM {t} WHERE scope=?1"), params![scope], |r| r.get::<_, i64>(0)).unwrap_or(0);
  let last: i64 = conn.query_row(
    "SELECT COALESCE(MAX(x), 0) FROM (SELECT MAX(occurred_at) x FROM am_events WHERE scope=?1 UNION ALL SELECT MAX(updated_at) FROM am_memories WHERE scope=?1 UNION ALL SELECT MAX(updated_at) FROM am_self_model WHERE scope=?1)",
    params![scope], |r| r.get(0)).unwrap_or(0);
  (c("am_events"), c("am_memories"), c("am_self_model"), last)
}

pub(crate) fn list_namespaces(conn: &Connection) -> Vec<Value> {
  let mut out = Vec::new();
  let Ok(mut stmt) = conn.prepare("SELECT key, type, id, name, description FROM am_namespaces ORDER BY updated_at DESC") else { return out };
  let rows = match stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, String>(3)?, r.get::<_, String>(4)?))) { Ok(r) => r, Err(_) => return out };
  for r in rows {
    let Ok((key, ty, id, name, desc)) = r else { continue };
    let (ev, mem, sm, last) = namespace_counts(conn, &key);
    out.push(json!({
      "namespace": { "key": key, "type": ty, "id": id, "name": name, "description": desc },
      "counts": { "events": ev, "memories": mem, "selfModelEntries": sm },
      "lastActivityAt": if last == 0 { Value::Null } else { json!(last) },
    }));
  }
  out
}

// ---------------------------------------------------------------------------
// Commands (the Memory view reads these directly — no MCP)
// ---------------------------------------------------------------------------

fn scope_or_agent(agent_id: &str, scope: Option<String>) -> String {
  scope.filter(|s| !s.trim().is_empty()).unwrap_or_else(|| scope_key("agent", agent_id))
}

#[tauri::command]
pub fn am_list_memories(app: AppHandle, agent_id: String, scope: Option<String>) -> Result<Vec<Value>, String> {
  let conn = db(&app)?;
  Ok(list_memories(&conn, &scope_or_agent(&agent_id, scope), 200))
}

#[tauri::command]
pub fn am_search(app: AppHandle, agent_id: String, scope: Option<String>, query: String, limit: Option<usize>) -> Result<Vec<Value>, String> {
  let conn = db(&app)?;
  Ok(search(&conn, &scope_or_agent(&agent_id, scope), &query, limit.unwrap_or(30)))
}

#[tauri::command]
pub fn am_get_self_model(app: AppHandle, agent_id: String, scope: Option<String>) -> Result<Vec<Value>, String> {
  let conn = db(&app)?;
  Ok(self_model_entries(&conn, &agent_id, scope.filter(|s| !s.trim().is_empty()).as_deref()))
}

#[tauri::command]
pub fn am_stats(app: AppHandle) -> Result<Value, String> { let conn = db(&app)?; Ok(stats(&conn)) }

#[tauri::command]
pub fn am_list_namespaces(app: AppHandle) -> Result<Vec<Value>, String> { let conn = db(&app)?; Ok(list_namespaces(&conn)) }

#[tauri::command]
pub fn am_add_memory(app: AppHandle, agent_id: String, scope: String, kind: String, title: String, content: String, tags: Option<Value>, confidence: Option<f64>, importance: Option<f64>) -> Result<String, String> {
  let conn = db(&app)?;
  add_memory(&conn, &agent_id, &scope, &kind, &title, &content, &tags.unwrap_or_else(|| json!([])), confidence.unwrap_or(0.8), importance.unwrap_or(0.5))
}

#[tauri::command]
pub fn am_update_memory(app: AppHandle, id: String, patch: Value) -> Result<(), String> { let conn = db(&app)?; update_memory(&conn, &id, &patch) }

#[tauri::command]
pub fn am_forget_memory(app: AppHandle, id: String) -> Result<(), String> { let conn = db(&app)?; forget_memory(&conn, &id) }

#[tauri::command]
pub fn am_get_memory(app: AppHandle, id: String) -> Result<Value, String> {
  let conn = db(&app)?;
  Ok(get_memory(&conn, &id).unwrap_or(Value::Null))
}

#[tauri::command]
pub fn am_remove_self_model(app: AppHandle, agent_id: String, scope: String, key: String) -> Result<(), String> {
  let conn = db(&app)?;
  remove_self_model(&conn, &agent_id, &scope, &key)
}

#[tauri::command]
pub fn am_set_self_model(app: AppHandle, agent_id: String, scope: Option<String>, key: String, value: String, confidence: Option<f64>, importance: Option<f64>) -> Result<(), String> {
  let conn = db(&app)?;
  set_self_model(&conn, &agent_id, &scope_or_agent(&agent_id, scope), &key, &value, confidence.unwrap_or(0.8), importance.unwrap_or(0.7))
}

#[tauri::command]
pub fn am_compile_context(app: AppHandle, agent_id: String, scope: Option<String>, token_budget: Option<usize>) -> Result<String, String> {
  let conn = db(&app)?;
  Ok(compile_context(&conn, &agent_id, &scope_or_agent(&agent_id, scope), token_budget.unwrap_or(600)))
}

#[tauri::command]
pub fn am_create_namespace(app: AppHandle, scope: String, name: String, description: Option<String>) -> Result<(), String> {
  let conn = db(&app)?;
  create_namespace(&conn, &scope, &name, &description.unwrap_or_default())
}

#[tauri::command]
pub fn am_remove_namespace(app: AppHandle, scope: String) -> Result<(), String> { let conn = db(&app)?; remove_namespace(&conn, &scope) }
