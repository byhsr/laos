// Tasks: persisted work items delegated to agents, plus the runs feed.

use rusqlite::{params, Connection};
use tauri::AppHandle;

use crate::agents::run_agent_once_structured;
use crate::db::db;
use crate::models::*;
use crate::storage::parse_json_vec;

pub(crate) fn create_task(conn: &Connection, requester: &str, assigned_agent: &str, input: &str, context: &str) -> Result<String, String> {
  let id = format!("task-{}", chrono::Utc::now().timestamp_millis());
  conn.execute("INSERT INTO tasks (id, requester, assigned_agent, status, input, context, created_at) VALUES (?1,?2,?3,'pending',?4,?5,?6)",
    params![id, requester, assigned_agent, input, context, chrono::Utc::now().to_rfc3339()]).map_err(|e| e.to_string())?;
  Ok(id)
}

pub(crate) fn task_from_row(row: &rusqlite::Row) -> rusqlite::Result<TaskRecord> {
  Ok(TaskRecord {
    id: row.get(0)?, requester: row.get(1)?, assigned_agent: row.get(2)?, status: row.get(3)?,
    input: row.get(4)?, context: row.get(5)?, result: row.get(6)?,
    created_at: row.get(7)?, completed_at: row.get(8)?,
  })
}

pub(crate) fn list_tasks(conn: &Connection) -> Result<Vec<TaskRecord>, String> {
  let mut stmt = conn.prepare("SELECT id, requester, assigned_agent, status, input, context, result, created_at, completed_at FROM tasks ORDER BY created_at DESC").map_err(|e| e.to_string())?;
  let rows = stmt.query_map([], task_from_row).map_err(|e| e.to_string())?;
  let mut out = Vec::new();
  for row in rows { out.push(row.map_err(|e| e.to_string())?); }
  Ok(out)
}

// Runs a domain agent for a delegated task, storing the result.
pub(crate) async fn delegate_task(app: &AppHandle, task_id: &str, assigned_agent: &str, input: &str, context: &str) -> Result<String, String> {
  let conn = db(app)?;
  let agent = {
    let mut stmt = conn.prepare("SELECT id, name, objective, model, tool_ids, integrations, home_path, permissions, skill_ids FROM agents WHERE id=?1").map_err(|e| e.to_string())?;
    let mut rows = stmt.query_map(params![assigned_agent], |row| {
      let tool_ids: String = row.get(4)?;
      let integrations: String = row.get(5)?;
      let permissions: String = row.get(7)?;
      let skill_ids: String = row.get(8)?;
      Ok(AgentRequest {
        id: row.get(0)?, name: row.get(1)?, objective: row.get(2)?, model: row.get(3)?,
        tool_ids: parse_json_vec(&tool_ids), integrations: parse_json_vec(&integrations),
        home_path: row.get(6)?, permissions: parse_json_vec(&permissions),
        skill_ids: parse_json_vec(&skill_ids),
      })
    }).map_err(|e| e.to_string())?;
    rows.next().transpose().map_err(|e| e.to_string())?
  };
  conn.execute("UPDATE tasks SET status='running' WHERE id=?1", params![task_id]).map_err(|e| e.to_string())?;
  let prompt = if context.is_empty() { input.to_string() } else { format!("Context:\n{context}\n\nTask:\n{input}") };
  let mut events = Vec::new();
  let result = match agent {
    Some(a) => run_agent_once_structured(app, &a, &prompt, None, &mut events, false).await.map(|(out, _, _)| out),
    None => Err("Assigned agent not found.".into()),
  };
  match result {
    Ok(out) => {
      conn.execute("UPDATE tasks SET status='completed', result=?1, completed_at=?2 WHERE id=?3", params![out, chrono::Utc::now().to_rfc3339(), task_id]).map_err(|e| e.to_string())?;
      Ok(out)
    }
    Err(e) => {
      conn.execute("UPDATE tasks SET status='failed', result=?1, completed_at=?2 WHERE id=?3", params![format!("Error: {e}"), chrono::Utc::now().to_rfc3339(), task_id]).map_err(|e| e.to_string())?;
      Err(e)
    }
  }
}

// ---------------------------------------------------------------------------
// Tasks
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn list_all_tasks(app: AppHandle) -> Result<Vec<TaskRecord>, String> {
  let conn = db(&app)?;
  list_tasks(&conn)
}

#[tauri::command]
pub fn get_task(app: AppHandle, id: String) -> Result<Option<TaskRecord>, String> {
  let conn = db(&app)?;
  let mut stmt = conn.prepare("SELECT id, requester, assigned_agent, status, input, context, result, created_at, completed_at FROM tasks WHERE id=?1").map_err(|e| e.to_string())?;
  let mut rows = stmt.query_map(params![id], task_from_row).map_err(|e| e.to_string())?;
  rows.next().transpose().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn run_task(app: AppHandle, requester: String, assigned_agent: String, input: String, context: String) -> Result<TaskRecord, String> {
  let conn = db(&app)?;
  let task_id = create_task(&conn, &requester, &assigned_agent, &input, &context)?;
  let _ = delegate_task(&app, &task_id, &assigned_agent, &input, &context).await;
  get_task(app, task_id)?.ok_or("Task disappeared".into())
}

#[tauri::command]
pub fn cancel_task(app: AppHandle, id: String) -> Result<(), String> {
  let conn = db(&app)?;
  conn.execute("UPDATE tasks SET status='cancelled', completed_at=?1 WHERE id=?2 AND status IN ('pending','running')", params![chrono::Utc::now().to_rfc3339(), id]).map_err(|e| e.to_string())?;
  Ok(())
}

#[tauri::command]
pub fn list_runs(app: AppHandle) -> Result<Vec<serde_json::Value>, String> {
  let conn = db(&app)?;
  let mut stmt = conn.prepare("SELECT id, agent_id, started_at, status, model, input, output, prompt_tokens, completion_tokens FROM runs ORDER BY started_at DESC LIMIT 200").map_err(|e| e.to_string())?;
  let rows = stmt.query_map([], |row| {
    Ok(serde_json::json!({
      "id": row.get::<_, String>(0)?,
      "agentId": row.get::<_, String>(1)?,
      "startedAt": row.get::<_, String>(2)?,
      "status": row.get::<_, String>(3)?,
      "model": row.get::<_, String>(4)?,
      "input": row.get::<_, String>(5)?,
      "output": row.get::<_, Option<String>>(6)?,
      "promptTokens": row.get::<_, i64>(7)?,
      "completionTokens": row.get::<_, i64>(8)?,
    }))
  }).map_err(|e| e.to_string())?;
  let mut out = Vec::new();
  for row in rows { out.push(row.map_err(|e| e.to_string())?); }
  Ok(out)
}
