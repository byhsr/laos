// Persistent app data: SQLite-backed CRUD for knowledge docs, model configs,
// tools, agents and workflows, plus the Manager bootstrap and default model.

use rusqlite::{params, Connection};
use tauri::AppHandle;

use crate::db::db;
use crate::models::*;

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
pub fn initialize_storage(app: AppHandle) -> Result<(), String> {
  let conn = db(&app)?;
  ensure_manager(&conn)?;
  Ok(())
}

// ---------------------------------------------------------------------------
// Knowledge base (shared, user-writable persistent docs)
// ---------------------------------------------------------------------------

#[derive(serde::Serialize, serde::Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeDoc {
  id: String, title: String, content: String,
  #[serde(default)] tags: Vec<String>, updated_at: String,
}

#[tauri::command]
pub fn list_knowledge_docs(app: AppHandle) -> Result<Vec<serde_json::Value>, String> {
  let conn = db(&app)?;
  let mut stmt = conn.prepare("SELECT id, title, content, tags, updated_at FROM knowledge_docs ORDER BY updated_at DESC").map_err(|e| e.to_string())?;
  let rows = stmt.query_map([], |row| {
    let tags: String = row.get(3)?;
    Ok(serde_json::json!({
      "id": row.get::<_, String>(0)?, "title": row.get::<_, String>(1)?,
      "content": row.get::<_, String>(2)?, "tags": parse_json_vec(&tags),
      "updatedAt": row.get::<_, String>(4)?,
    }))
  }).map_err(|e| e.to_string())?;
  let mut out = Vec::new();
  for row in rows { out.push(row.map_err(|e| e.to_string())?); }
  Ok(out)
}

#[tauri::command]
pub fn get_knowledge_doc(app: AppHandle, id: String) -> Result<serde_json::Value, String> {
  let conn = db(&app)?;
  let mut stmt = conn.prepare("SELECT id, title, content, tags, updated_at FROM knowledge_docs WHERE id=?1").map_err(|e| e.to_string())?;
  let mut rows = stmt.query_map(params![id], |row| {
    let tags: String = row.get(3)?;
    Ok(serde_json::json!({
      "id": row.get::<_, String>(0)?, "title": row.get::<_, String>(1)?,
      "content": row.get::<_, String>(2)?, "tags": parse_json_vec(&tags),
      "updatedAt": row.get::<_, String>(4)?,
    }))
  }).map_err(|e| e.to_string())?;
  match rows.next().transpose().map_err(|e| e.to_string())? {
    Some(v) => Ok(v),
    None => Err("Document not found".into()),
  }
}

#[tauri::command]
pub fn save_knowledge_doc(app: AppHandle, doc: KnowledgeDoc) -> Result<(), String> {
  let conn = db(&app)?;
  let tags = serde_json::to_string(&doc.tags).unwrap_or_else(|_| "[]".to_string());
  conn.execute(
    "INSERT INTO knowledge_docs (id, title, content, tags, updated_at) VALUES (?1,?2,?3,?4,?5)
     ON CONFLICT(id) DO UPDATE SET title=excluded.title, content=excluded.content, tags=excluded.tags, updated_at=excluded.updated_at",
    params![doc.id, doc.title, doc.content, tags, doc.updated_at],
  ).map_err(|e| e.to_string())?;
  Ok(())
}

#[tauri::command]
pub fn delete_knowledge_doc(app: AppHandle, id: String) -> Result<(), String> {
  let conn = db(&app)?;
  conn.execute("DELETE FROM knowledge_docs WHERE id=?1", params![id]).map_err(|e| e.to_string())?;
  Ok(())
}

#[tauri::command]
pub fn list_model_configs(app: AppHandle) -> Result<Vec<ModelConfigRecord>, String> {
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
pub fn save_model_config(app: AppHandle, config: ModelConfigRecord) -> Result<(), String> {
  let conn = db(&app)?;
  conn.execute(
    "INSERT INTO model_configs (id, provider, label, model, host, api_key, enabled) VALUES (?1,?2,?3,?4,?5,?6,?7)
     ON CONFLICT(id) DO UPDATE SET provider=excluded.provider, label=excluded.label, model=excluded.model, host=excluded.host, api_key=excluded.api_key, enabled=excluded.enabled",
    params![config.id, config.provider, config.label, config.model, config.host, config.api_key, if config.enabled { 1 } else { 0 }],
  ).map_err(|e| e.to_string())?;
  Ok(())
}

#[tauri::command]
pub fn delete_model_config(app: AppHandle, id: String) -> Result<(), String> {
  let conn = db(&app)?;
  conn.execute("DELETE FROM model_configs WHERE id=?1", params![id]).map_err(|e| e.to_string())?;
  Ok(())
}

pub(crate) fn stored_api_key(conn: &Connection, model_id: &str) -> Result<Option<String>, String> {
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
pub fn list_tools(app: AppHandle) -> Result<Vec<ToolRecord>, String> {
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
pub fn save_tool(app: AppHandle, tool: ToolRecord) -> Result<(), String> {
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
pub fn delete_tool(app: AppHandle, id: String) -> Result<(), String> {
  let conn = db(&app)?;
  conn.execute("DELETE FROM tools WHERE id=?1", params![id]).map_err(|e| e.to_string())?;
  Ok(())
}

pub(crate) fn parse_json_vec(s: &str) -> Vec<String> {
  serde_json::from_str(s).unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Skills (reusable instruction packs injected into an agent's prompt)
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn list_skills(app: AppHandle) -> Result<Vec<SkillRecord>, String> {
  let conn = db(&app)?;
  let mut stmt = conn.prepare("SELECT id, name, description, content FROM skills ORDER BY name").map_err(|e| e.to_string())?;
  let rows = stmt.query_map([], |row| Ok(SkillRecord { id: row.get(0)?, name: row.get(1)?, description: row.get(2)?, content: row.get(3)? })).map_err(|e| e.to_string())?;
  let mut out = Vec::new();
  for row in rows { out.push(row.map_err(|e| e.to_string())?); }
  Ok(out)
}

#[tauri::command]
pub fn save_skill(app: AppHandle, skill: SkillRecord) -> Result<(), String> {
  let conn = db(&app)?;
  conn.execute(
    "INSERT INTO skills (id, name, description, content, updated_at) VALUES (?1,?2,?3,?4,?5)
     ON CONFLICT(id) DO UPDATE SET name=excluded.name, description=excluded.description, content=excluded.content, updated_at=excluded.updated_at",
    params![skill.id, skill.name, skill.description, skill.content, chrono::Utc::now().to_rfc3339()],
  ).map_err(|e| e.to_string())?;
  Ok(())
}

#[tauri::command]
pub fn delete_skill(app: AppHandle, id: String) -> Result<(), String> {
  let conn = db(&app)?;
  conn.execute("DELETE FROM skills WHERE id=?1", params![id]).map_err(|e| e.to_string())?;
  Ok(())
}

#[tauri::command]
pub fn list_agents(app: AppHandle) -> Result<Vec<AgentRecord>, String> {
  let conn = db(&app)?;
  let mut stmt = conn.prepare("SELECT id, name, objective, model, tool_ids, integrations, memory, permissions, home_path, color, x, y, is_manager, description, persona, skill_ids FROM agents ORDER BY name").map_err(|e| e.to_string())?;
  let rows = stmt.query_map([], |row| {
    let tool_ids: String = row.get(4)?;
    let integrations: String = row.get(5)?;
    let permissions: String = row.get(7)?;
    let skill_ids: String = row.get(15)?;
    Ok(AgentRecord {
      id: row.get(0)?, name: row.get(1)?, objective: row.get(2)?, model: row.get(3)?,
      tool_ids: parse_json_vec(&tool_ids), integrations: parse_json_vec(&integrations),
      memory: row.get::<_, i64>(6)? != 0, permissions: parse_json_vec(&permissions),
      home_path: row.get(8)?, color: row.get(9)?, x: row.get(10)?, y: row.get(11)?,
      is_manager: row.get::<_, i64>(12)? != 0, description: row.get(13)?, persona: row.get(14)?,
      skill_ids: parse_json_vec(&skill_ids),
    })
  }).map_err(|e| e.to_string())?;
  let mut out = Vec::new();
  for row in rows { out.push(row.map_err(|e| e.to_string())?); }
  Ok(out)
}

#[tauri::command]
pub fn save_agent(app: AppHandle, agent: AgentRecord) -> Result<(), String> {
  let conn = db(&app)?;
  conn.execute(
    "INSERT INTO agents (id, name, objective, model, tool_ids, integrations, memory, permissions, home_path, color, x, y, is_manager, description, persona, skill_ids) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16)
     ON CONFLICT(id) DO UPDATE SET name=excluded.name, objective=excluded.objective, model=excluded.model, tool_ids=excluded.tool_ids, integrations=excluded.integrations, memory=excluded.memory, permissions=excluded.permissions, home_path=excluded.home_path, color=excluded.color, x=excluded.x, y=excluded.y, is_manager=excluded.is_manager, description=excluded.description, persona=excluded.persona, skill_ids=excluded.skill_ids",
    params![agent.id, agent.name, agent.objective, agent.model,
      serde_json::to_string(&agent.tool_ids).unwrap_or_else(|_| "[]".into()),
      serde_json::to_string(&agent.integrations).unwrap_or_else(|_| "[]".into()),
      if agent.memory { 1 } else { 0 },
      serde_json::to_string(&agent.permissions).unwrap_or_else(|_| "[]".into()),
      agent.home_path, agent.color, agent.x, agent.y,
      if agent.is_manager { 1 } else { 0 }, agent.description, agent.persona,
      serde_json::to_string(&agent.skill_ids).unwrap_or_else(|_| "[]".into())],
  ).map_err(|e| e.to_string())?;
  Ok(())
}

#[tauri::command]
pub fn delete_agent(app: AppHandle, id: String) -> Result<(), String> {
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
pub fn list_workflows(app: AppHandle) -> Result<Vec<WorkflowRecord>, String> {
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
pub fn save_workflow(app: AppHandle, workflow: WorkflowRecord) -> Result<(), String> {
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
pub fn delete_workflow(app: AppHandle, id: String) -> Result<(), String> {
  let conn = db(&app)?;
  conn.execute("DELETE FROM workflows WHERE id=?1", params![id]).map_err(|e| e.to_string())?;
  Ok(())
}

// Picks a reachable model for the Manager: prefer an enabled cloud model
// (Groq/OpenRouter, which only needs a key) before falling back to Ollama.
pub(crate) fn manager_default_model(conn: &Connection) -> String {
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
