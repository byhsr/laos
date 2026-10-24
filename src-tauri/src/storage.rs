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
    // The Manager seeds its reasoning from its default model, like any agent.
    let reasoning = if model.is_empty() { default_reasoning() } else {
      conn.query_row("SELECT reasoning FROM model_configs WHERE id=?1", params![model], |r| r.get::<_, String>(0)).unwrap_or_else(|_| default_reasoning())
    };
    conn.execute(
      "INSERT INTO agents (id, name, objective, model, tool_ids, integrations, memory, permissions, home_path, color, is_manager, description, reasoning) VALUES ('manager','Laos','You are Laos, the workspace orchestrator. Control the workspace and coordinate work.',?1,'[]','[]',1,'[\"network\"]','agents/manager','#22c55e',1,'',?2)",
      params![model, reasoning],
    ).map_err(|e| e.to_string())?;
  }
  Ok(())
}

// ---------------------------------------------------------------------------
// Built-in memory (native — was the fox/`agent-memory` MCP server)
// ---------------------------------------------------------------------------

// Retires the legacy in-process-bridge MCP server: memory is native now, so drop
// the seeded `memory` server row and any tools imported from it.
fn retire_memory_mcp(conn: &Connection) {
  let _ = conn.execute("DELETE FROM tools WHERE kind='mcp' AND integration_id='memory'", []);
  let _ = conn.execute("DELETE FROM mcp_servers WHERE id='memory'", []);
}

// ---------------------------------------------------------------------------
// Built-in browser connector (native embedded mini-browser)
// ---------------------------------------------------------------------------

// Browser control ships built in as a native tool (`kind='browser'`), backed by
// the embedded mini-browser webview (see crate::browser). It replaced the old
// `@playwright/mcp` server, which needed Node/npx, couldn't show the page and
// dropped screenshots.
const BROWSER_ID: &str = "browser";

// Removes the retired Playwright MCP server and any tools imported from it, so an
// upgraded install doesn't keep a broken, headless browser path around.
fn retire_playwright_browser(conn: &Connection) {
  let _ = conn.execute(
    "DELETE FROM tools WHERE kind='mcp' AND integration_id IN (SELECT id FROM mcp_servers WHERE id=?1 OR lower(args) LIKE '%playwright%')",
    params![BROWSER_ID],
  );
  let _ = conn.execute(
    "DELETE FROM mcp_servers WHERE id=?1 OR lower(args) LIKE '%playwright%'",
    params![BROWSER_ID],
  );
}

// Seeds the single native browser tool row so it appears in the picker and can be
// attached to an agent (or granted to the Manager).
fn ensure_browser_tool(conn: &Connection) -> Result<(), String> {
  conn.execute(
    "INSERT INTO tools (id, name, kind, integration_id, description, enabled, config_json) VALUES (?1,'Browser','browser','builtin',?2,1,'{}')
     ON CONFLICT(id) DO NOTHING",
    params![BROWSER_ID, "Open a mini browser panel and let the agent navigate, click, type and read pages."],
  ).map_err(|e| e.to_string())?;
  Ok(())
}

#[tauri::command]
pub async fn initialize_storage(app: AppHandle) -> Result<(), String> {
  {
    let conn = db(&app)?;
    ensure_manager(&conn)?;
    retire_memory_mcp(&conn);
    retire_playwright_browser(&conn);
    ensure_browser_tool(&conn)?;
    // Before anything can read a secret: move any plaintext left over from an
    // older build into the OS credential store.
    crate::vault::migrate(&conn)?;
  }
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
  let mut stmt = conn.prepare("SELECT id, provider, label, model, host, api_key, enabled, reasoning FROM model_configs ORDER BY id").map_err(|e| e.to_string())?;
  let rows = stmt.query_map([], |row| {
    // The key itself never leaves the backend — only whether one is stored.
    let stored: Option<String> = row.get(5)?;
    Ok(ModelConfigRecord {
      id: row.get(0)?, provider: row.get(1)?, label: row.get(2)?, model: row.get(3)?, host: row.get(4)?,
      has_key: stored.map(|s| !s.trim().is_empty()).unwrap_or(false),
      enabled: row.get::<_, i64>(6)? != 0, reasoning: row.get(7)?,
    })
  }).map_err(|e| e.to_string())?;
  let mut out = Vec::new();
  for row in rows { out.push(row.map_err(|e| e.to_string())?); }
  Ok(out)
}

#[tauri::command]
pub fn save_model_config(app: AppHandle, config: ModelConfigInput) -> Result<(), String> {
  let conn = db(&app)?;
  let account = format!("model/{}", config.id);
  // The key is write-only: a submitted value replaces it, `clearKey` removes it,
  // and neither leaves whatever is already stored untouched.
  let submitted = config.api_key.as_deref().map(str::trim).filter(|k| !k.is_empty());
  let api_key = if config.clear_key {
    crate::vault::delete(&account)?;
    None
  } else if let Some(key) = submitted {
    crate::vault::put(&account, key)?;
    Some(crate::vault::REF.to_string())
  } else {
    conn.query_row("SELECT api_key FROM model_configs WHERE id=?1", params![config.id], |r| r.get::<_, Option<String>>(0)).ok().flatten()
  };
  conn.execute(
    "INSERT INTO model_configs (id, provider, label, model, host, api_key, enabled, reasoning) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)
     ON CONFLICT(id) DO UPDATE SET provider=excluded.provider, label=excluded.label, model=excluded.model, host=excluded.host, api_key=excluded.api_key, enabled=excluded.enabled, reasoning=excluded.reasoning",
    params![config.id, config.provider, config.label, config.model, config.host, api_key, if config.enabled { 1 } else { 0 }, config.reasoning],
  ).map_err(|e| e.to_string())?;
  Ok(())
}

#[tauri::command]
pub fn delete_model_config(app: AppHandle, id: String) -> Result<(), String> {
  let conn = db(&app)?;
  conn.execute("DELETE FROM model_configs WHERE id=?1", params![id]).map_err(|e| e.to_string())?;
  let _ = crate::vault::delete(&format!("model/{id}"));
  Ok(())
}

// Model roles: which model fills `classifier` / `embedding` / `reranker` /
// `vision` / `distillation`. An unset role means "use the deterministic default"
// (see provider::resolve_role), so a fresh app needs no configuration.

#[tauri::command]
pub fn list_model_roles(app: AppHandle) -> Result<serde_json::Value, String> {
  let conn = db(&app)?;
  let mut out = serde_json::Map::new();
  for role in crate::provider::ROLES {
    let explicit = crate::provider::explicit_role(&conn, role);
    // classifier/embedding always resolve to a default; the opt-in roles
    // (reranker/vision/distillation) only take effect when pinned, so the UI can
    // show them as off until chosen.
    let resolved = if matches!(role, "classifier" | "embedding") {
      crate::provider::resolve_role(&conn, role)
    } else {
      explicit.clone()
    };
    out.insert(role.to_string(), serde_json::json!({
      "modelId": explicit.unwrap_or_default(),
      "resolved": resolved,
    }));
  }
  Ok(serde_json::Value::Object(out))
}

#[tauri::command]
pub fn set_model_role(app: AppHandle, role: String, model_id: String) -> Result<(), String> {
  let conn = db(&app)?;
  conn.execute(
    "INSERT INTO model_roles (role, model_id, updated_at) VALUES (?1,?2,?3)
     ON CONFLICT(role) DO UPDATE SET model_id=excluded.model_id, updated_at=excluded.updated_at",
    params![role, model_id, chrono::Local::now().to_rfc3339()],
  ).map_err(|e| e.to_string())?;
  Ok(())
}

// Header values whose name marks them secret live in the credential store, so a
// tool's saved config only ever carries the `REF` sentinel. The UI sees the
// "set" marker instead; the real value is resolved when the request is built.
fn mask_tool_config(config: &serde_json::Value) -> serde_json::Value {
  let mut cfg = config.clone();
  if let Some(headers) = cfg.get_mut("headers").and_then(|h| h.as_array_mut()) {
    for h in headers.iter_mut() {
      let Some(obj) = h.as_object_mut() else { continue };
      let name = obj.get("name").and_then(|n| n.as_str()).unwrap_or("").to_string();
      if !crate::vault::is_secret_name(&name) { continue; }
      if obj.get("value").and_then(|v| v.as_str()).map(|v| !v.is_empty()).unwrap_or(false) {
        obj.insert("value".into(), serde_json::json!(crate::vault::MASK));
      }
    }
  }
  cfg
}

#[tauri::command]
pub fn list_tools(app: AppHandle) -> Result<Vec<ToolRecord>, String> {
  let conn = db(&app)?;
  let mut stmt = conn.prepare("SELECT id, name, kind, integration_id, description, enabled, config_json FROM tools ORDER BY name").map_err(|e| e.to_string())?;
  let rows = stmt.query_map([], |row| {
    let config_json: String = row.get(6)?;
    let config = serde_json::from_str(&config_json).unwrap_or_else(|_| serde_json::json!({}));
    Ok(ToolRecord {
      id: row.get(0)?, name: row.get(1)?, kind: row.get(2)?, integration_id: row.get(3)?,
      description: row.get(4)?, enabled: row.get::<_, i64>(5)? != 0,
      config: mask_tool_config(&config),
    })
  }).map_err(|e| e.to_string())?;
  let mut out = Vec::new();
  for row in rows { out.push(row.map_err(|e| e.to_string())?); }
  Ok(out)
}

#[tauri::command]
pub fn save_tool(app: AppHandle, tool: ToolRecord) -> Result<(), String> {
  let conn = db(&app)?;
  let mut config = tool.config.clone();
  if let Some(headers) = config.get_mut("headers").and_then(|h| h.as_array_mut()) {
    for h in headers.iter_mut() {
      let Some(obj) = h.as_object_mut() else { continue };
      let name = obj.get("name").and_then(|n| n.as_str()).unwrap_or("").to_string();
      if !crate::vault::is_secret_name(&name) { continue; }
      let value = obj.get("value").and_then(|v| v.as_str()).unwrap_or("").to_string();
      let account = format!("tool/{}/{}", tool.id, name);
      if value == crate::vault::MASK {
        // Untouched: keep the reference only if a secret is actually stored.
        obj.insert("value".into(), serde_json::json!(if crate::vault::get(&account).is_some() { crate::vault::REF } else { "" }));
      } else if !value.is_empty() {
        crate::vault::put(&account, &value)?;
        obj.insert("value".into(), serde_json::json!(crate::vault::REF));
      }
    }
  }
  let config_json = serde_json::to_string(&config).unwrap_or_else(|_| "{}".into());
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
  let mut stmt = conn.prepare("SELECT id, name, description, content, docs, script_ids FROM skills ORDER BY name").map_err(|e| e.to_string())?;
  let rows = stmt.query_map([], |row| {
    let script_ids: String = row.get(5)?;
    Ok(SkillRecord { id: row.get(0)?, name: row.get(1)?, description: row.get(2)?, content: row.get(3)?, docs: row.get(4)?, script_ids: parse_json_vec(&script_ids) })
  }).map_err(|e| e.to_string())?;
  let mut out = Vec::new();
  for row in rows { out.push(row.map_err(|e| e.to_string())?); }
  Ok(out)
}

#[tauri::command]
pub fn save_skill(app: AppHandle, skill: SkillRecord) -> Result<(), String> {
  let conn = db(&app)?;
  conn.execute(
    "INSERT INTO skills (id, name, description, content, docs, script_ids, updated_at) VALUES (?1,?2,?3,?4,?5,?6,?7)
     ON CONFLICT(id) DO UPDATE SET name=excluded.name, description=excluded.description, content=excluded.content, docs=excluded.docs, script_ids=excluded.script_ids, updated_at=excluded.updated_at",
    params![skill.id, skill.name, skill.description, skill.content, skill.docs, serde_json::to_string(&skill.script_ids).unwrap_or_else(|_| "[]".into()), chrono::Utc::now().to_rfc3339()],
  ).map_err(|e| e.to_string())?;
  Ok(())
}

#[tauri::command]
pub fn delete_skill(app: AppHandle, id: String) -> Result<(), String> {
  let conn = db(&app)?;
  conn.execute("DELETE FROM skills WHERE id=?1", params![id]).map_err(|e| e.to_string())?;
  Ok(())
}

// ---------------------------------------------------------------------------
// Scripts (reusable runnable commands / custom apps)
// ---------------------------------------------------------------------------

pub(crate) fn load_scripts(conn: &Connection) -> Vec<ScriptRecord> {
  let mut out = Vec::new();
  if let Ok(mut stmt) = conn.prepare("SELECT id, name, description, kind, command, cwd, prompt, model, updated_at FROM scripts ORDER BY name") {
    if let Ok(rows) = stmt.query_map([], |row| Ok(ScriptRecord {
      id: row.get(0)?, name: row.get(1)?, description: row.get(2)?, kind: row.get(3)?,
      command: row.get(4)?, cwd: row.get(5)?, prompt: row.get(6)?, model: row.get(7)?, updated_at: row.get(8)?,
    })) {
      for r in rows { if let Ok(s) = r { out.push(s); } }
    }
  }
  out
}

pub(crate) fn load_script(conn: &Connection, id: &str) -> Option<ScriptRecord> {
  conn.query_row("SELECT id, name, description, kind, command, cwd, prompt, model, updated_at FROM scripts WHERE id=?1", params![id], |row| Ok(ScriptRecord {
    id: row.get(0)?, name: row.get(1)?, description: row.get(2)?, kind: row.get(3)?,
    command: row.get(4)?, cwd: row.get(5)?, prompt: row.get(6)?, model: row.get(7)?, updated_at: row.get(8)?,
  })).ok()
}

#[tauri::command]
pub fn list_scripts(app: AppHandle) -> Result<Vec<ScriptRecord>, String> {
  let conn = db(&app)?;
  Ok(load_scripts(&conn))
}

#[tauri::command]
pub fn save_script(app: AppHandle, script: ScriptRecord) -> Result<String, String> {
  let conn = db(&app)?;
  let id = if script.id.trim().is_empty() { format!("script-{}", chrono::Utc::now().timestamp_millis()) } else { script.id.clone() };
  let kind = if script.kind == "prompt" { "prompt" } else { "command" };
  conn.execute(
    "INSERT INTO scripts (id, name, description, kind, command, cwd, prompt, model, updated_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)
     ON CONFLICT(id) DO UPDATE SET name=excluded.name, description=excluded.description, kind=excluded.kind, command=excluded.command, cwd=excluded.cwd, prompt=excluded.prompt, model=excluded.model, updated_at=excluded.updated_at",
    params![id, script.name, script.description, kind, script.command, script.cwd, script.prompt, script.model, chrono::Utc::now().to_rfc3339()],
  ).map_err(|e| e.to_string())?;
  Ok(id)
}

#[tauri::command]
pub fn delete_script(app: AppHandle, id: String) -> Result<(), String> {
  let conn = db(&app)?;
  conn.execute("DELETE FROM scripts WHERE id=?1", params![id]).map_err(|e| e.to_string())?;
  Ok(())
}

// Runs a saved app on demand (the Apps tab's Test button). A `command` app runs
// through the shell; a `prompt` app runs internally as a single model call.
#[tauri::command]
pub async fn run_script_now(app: AppHandle, id: String, input: Option<String>) -> Result<String, String> {
  let script = {
    let conn = db(&app)?;
    load_script(&conn, &id).ok_or("App not found.")?
  };
  let input = input.unwrap_or_default();
  if script.kind == "prompt" {
    if script.model.trim().is_empty() { return Err("This app needs a model — set one in the app's config.".into()); }
    let prompt = script.prompt.replace("{input}", &input);
    return crate::memory::one_shot_completion(&app, &script.model, &prompt, crate::http::CHAT_MAX_TOKENS).await;
  }
  let command = script.command.replace("{input}", &input);
  let cwd = script.cwd;
  tokio::task::spawn_blocking(move || crate::tools::run_shell_in(&command, &cwd)).await.map_err(|e| e.to_string())?
}

// ---------------------------------------------------------------------------
// Projects (group conversations + scope their memory)
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn list_projects(app: AppHandle) -> Result<Vec<ProjectRecord>, String> {
  let conn = db(&app)?;
  let mut stmt = conn.prepare("SELECT id, name, description, created_at, updated_at FROM projects ORDER BY updated_at DESC").map_err(|e| e.to_string())?;
  let rows = stmt.query_map([], |row| Ok(ProjectRecord {
    id: row.get(0)?, name: row.get(1)?, description: row.get(2)?, created_at: row.get(3)?, updated_at: row.get(4)?,
  })).map_err(|e| e.to_string())?;
  let mut out = Vec::new();
  for row in rows { out.push(row.map_err(|e| e.to_string())?); }
  Ok(out)
}

#[tauri::command]
pub fn save_project(app: AppHandle, project: ProjectRecord) -> Result<String, String> {
  let conn = db(&app)?;
  let now = chrono::Utc::now().to_rfc3339();
  let id = if project.id.trim().is_empty() { format!("proj-{}", chrono::Utc::now().timestamp_millis()) } else { project.id.clone() };
  let created = if project.created_at.trim().is_empty() { now.clone() } else { project.created_at.clone() };
  conn.execute(
    "INSERT INTO projects (id, name, description, created_at, updated_at) VALUES (?1,?2,?3,?4,?5)
     ON CONFLICT(id) DO UPDATE SET name=excluded.name, description=excluded.description, updated_at=excluded.updated_at",
    params![id, project.name, project.description, created, now],
  ).map_err(|e| e.to_string())?;
  // Mirror the project into fox as a namespace so its memory is tracked together.
  crate::fox::register_namespace(&app, &id, &project.name, &project.description);
  Ok(id)
}

#[tauri::command]
pub fn delete_project(app: AppHandle, id: String) -> Result<(), String> {
  let conn = db(&app)?;
  conn.execute("DELETE FROM projects WHERE id=?1", params![id]).map_err(|e| e.to_string())?;
  crate::fox::remove_namespace(&app, &id);
  Ok(())
}

#[tauri::command]
pub fn list_agents(app: AppHandle) -> Result<Vec<AgentRecord>, String> {
  let conn = db(&app)?;
  let mut stmt = conn.prepare("SELECT id, name, objective, model, tool_ids, integrations, memory, permissions, home_path, color, x, y, is_manager, description, persona, skill_ids, pinned, avatar, reasoning FROM agents ORDER BY name").map_err(|e| e.to_string())?;
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
      pinned: row.get::<_, i64>(16)? != 0, avatar: row.get(17)?,
      reasoning: row.get(18)?,
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
    "INSERT INTO agents (id, name, objective, model, tool_ids, integrations, memory, permissions, home_path, color, x, y, is_manager, description, persona, skill_ids, pinned, avatar, reasoning) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19)
     ON CONFLICT(id) DO UPDATE SET name=excluded.name, objective=excluded.objective, model=excluded.model, tool_ids=excluded.tool_ids, integrations=excluded.integrations, memory=excluded.memory, permissions=excluded.permissions, home_path=excluded.home_path, color=excluded.color, x=excluded.x, y=excluded.y, is_manager=excluded.is_manager, description=excluded.description, persona=excluded.persona, skill_ids=excluded.skill_ids, pinned=excluded.pinned, avatar=excluded.avatar, reasoning=excluded.reasoning",
    params![agent.id, agent.name, agent.objective, agent.model,
      serde_json::to_string(&agent.tool_ids).unwrap_or_else(|_| "[]".into()),
      serde_json::to_string(&agent.integrations).unwrap_or_else(|_| "[]".into()),
      if agent.memory { 1 } else { 0 },
      serde_json::to_string(&agent.permissions).unwrap_or_else(|_| "[]".into()),
      agent.home_path, agent.color, agent.x, agent.y,
      if agent.is_manager { 1 } else { 0 }, agent.description, agent.persona,
      serde_json::to_string(&agent.skill_ids).unwrap_or_else(|_| "[]".into()),
      if agent.pinned { 1 } else { 0 }, agent.avatar,
      if agent.reasoning.trim().is_empty() { default_reasoning() } else { agent.reasoning.clone() }],
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
