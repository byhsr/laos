// Tasks: persisted background work — a delegated agent run, or one captured slow
// tool call — plus the runs feed. A task remembers the chat it was requested from
// so its result can be pushed back there when it finishes.

use rusqlite::{params, Connection};
use tauri::{AppHandle, Emitter, Manager};

use crate::agents::{build_tools, run_agent_once_structured};
use crate::db::db;
use crate::models::*;
use crate::storage::parse_json_vec;

// A unit of background work. `kind` is 'agent' (run the assigned agent on `input`)
// or 'tool' (run the one captured `tool` call). `session_id`/`chat_agent_id` name
// the chat to push the result back into.
pub(crate) struct NewTask {
  pub requester: String,
  pub assigned_agent: String,
  pub input: String,
  pub context: String,
  pub session_id: String,
  pub chat_agent_id: String,
  pub kind: String,
  pub tool: String,
  pub tool_args: String,
}

impl NewTask {
  // A delegated agent run — what the Manager's `delegate_task` creates.
  pub(crate) fn agent(requester: &str, assigned_agent: &str, input: &str, context: &str, session_id: Option<&str>, chat_agent_id: &str) -> Self {
    NewTask {
      requester: requester.into(), assigned_agent: assigned_agent.into(), input: input.into(), context: context.into(),
      session_id: session_id.unwrap_or("").into(), chat_agent_id: chat_agent_id.into(),
      kind: "agent".into(), tool: String::new(), tool_args: String::new(),
    }
  }

  // One slow tool call, backgrounded.
  pub(crate) fn tool(agent_id: &str, tool: &str, args: &serde_json::Value, session_id: Option<&str>) -> Self {
    NewTask {
      requester: agent_id.into(), assigned_agent: agent_id.into(), input: format!("{tool} {args}"), context: String::new(),
      session_id: session_id.unwrap_or("").into(), chat_agent_id: agent_id.into(),
      kind: "tool".into(), tool: tool.into(), tool_args: args.to_string(),
    }
  }
}

pub(crate) fn create_task(conn: &Connection, t: &NewTask) -> Result<String, String> {
  let id = format!("task-{}", chrono::Utc::now().timestamp_millis());
  conn.execute(
    "INSERT INTO tasks (id, requester, assigned_agent, status, input, context, session_id, chat_agent_id, kind, tool, tool_args, created_at) VALUES (?1,?2,?3,'pending',?4,?5,?6,?7,?8,?9,?10,?11)",
    params![id, t.requester, t.assigned_agent, t.input, t.context, t.session_id, t.chat_agent_id, t.kind, t.tool, t.tool_args, chrono::Utc::now().to_rfc3339()],
  ).map_err(|e| e.to_string())?;
  Ok(id)
}

// Creates the task and runs it in the background, returning its id immediately —
// the caller never waits for the work.
pub(crate) fn spawn_task(app: &AppHandle, t: NewTask) -> Result<String, String> {
  let conn = db(app)?;
  let id = create_task(&conn, &t)?;
  drop(conn);
  // Tell the UI the moment work is backgrounded, so its activity panel can show
  // the job while the chat carries on.
  let _ = app.emit("lumin-task-started", serde_json::json!({
    "taskId": id, "chatAgentId": t.chat_agent_id, "assignedAgent": t.assigned_agent,
    "sessionId": t.session_id, "kind": t.kind, "tool": t.tool, "label": t.input,
  }));
  let app = app.clone();
  let task_id = id.clone();
  tauri::async_runtime::spawn(async move { run_task_inner(&app, &task_id).await });
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

fn load_agent(conn: &Connection, id: &str) -> Result<Option<AgentRequest>, String> {
  let mut stmt = conn.prepare("SELECT id, name, objective, model, tool_ids, integrations, home_path, permissions, skill_ids, reasoning FROM agents WHERE id=?1").map_err(|e| e.to_string())?;
  let mut rows = stmt.query_map(params![id], |row| {
    let tool_ids: String = row.get(4)?;
    let integrations: String = row.get(5)?;
    let permissions: String = row.get(7)?;
    let skill_ids: String = row.get(8)?;
    Ok(AgentRequest {
      id: row.get(0)?, name: row.get(1)?, objective: row.get(2)?, model: row.get(3)?,
      tool_ids: parse_json_vec(&tool_ids), integrations: parse_json_vec(&integrations),
      home_path: row.get(6)?, permissions: parse_json_vec(&permissions),
      skill_ids: parse_json_vec(&skill_ids),
      reasoning: row.get(9)?,
    })
  }).map_err(|e| e.to_string())?;
  rows.next().transpose().map_err(|e| e.to_string())
}

// Runs a task from its stored row, then files the result. Called on a background
// thread by `spawn_task` and inline by the `run_task` command.
pub(crate) async fn run_task_inner(app: &AppHandle, task_id: &str) {
  let row = { db(app).ok().and_then(|conn| load_task(&conn, task_id)) };
  let Some((assigned_agent, input, context, session_id, chat_agent_id, kind, tool, tool_args)) = row else { return };
  if let Ok(conn) = db(app) { let _ = conn.execute("UPDATE tasks SET status='running' WHERE id=?1", params![task_id]); }

  let outcome = if kind == "tool" {
    run_tool_task(app, &assigned_agent, &tool, &tool_args, &session_id).await
  } else {
    run_agent_task(app, &assigned_agent, &input, &context).await
  };
  finish(app, task_id, &assigned_agent, &input, &session_id, &chat_agent_id, outcome);
}

type TaskRow = (String, String, String, String, String, String, String, String);

fn load_task(conn: &Connection, task_id: &str) -> Option<TaskRow> {
  let mut stmt = conn.prepare("SELECT assigned_agent, input, context, session_id, chat_agent_id, kind, tool, tool_args FROM tasks WHERE id=?1").ok()?;
  let mut rows = stmt.query_map(params![task_id], |r| Ok((
    r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, String>(3)?,
    r.get::<_, String>(4)?, r.get::<_, String>(5)?, r.get::<_, String>(6)?, r.get::<_, String>(7)?,
  ))).ok()?;
  rows.next()?.ok()
}

async fn run_agent_task(app: &AppHandle, assigned_agent: &str, input: &str, context: &str) -> Result<String, String> {
  // Scope the connection so it is never held across the await (the future must
  // stay Send for the background spawn).
  let agent = { let conn = db(app)?; load_agent(&conn, assigned_agent)? };
  let Some(a) = agent else { return Err("Assigned agent not found.".into()) };
  let prompt = if context.is_empty() { input.to_string() } else { format!("Context:\n{context}\n\nTask:\n{input}") };
  let mut events = Vec::new();
  run_agent_once_structured(app, &a, &prompt, None, &mut events, false).await.map(|(out, _, _)| out)
}

// Runs one captured tool call for the agent that asked for it. The originating
// chat is carried through, so a tool that needs its session (the scratchpad) still
// has it when the call happens in the background.
async fn run_tool_task(app: &AppHandle, assigned_agent: &str, tool: &str, tool_args: &str, session_id: &str) -> Result<String, String> {
  let tools = {
    let conn = db(app)?;
    let Some(a) = load_agent(&conn, assigned_agent)? else { return Err("Assigned agent not found.".into()) };
    let home = app.path().app_data_dir().map_err(|e| e.to_string())?.join("agents").join(&a.id);
    build_tools(app, &conn, &a, &home)?
  };
  let args: serde_json::Value = serde_json::from_str(tool_args).unwrap_or_else(|_| serde_json::json!({}));
  let sid = if session_id.is_empty() { None } else { Some(session_id) };
  let out = crate::chat::execute_tool(app, &tools, false, tool, &args, sid, assigned_agent).await;
  Ok(out.text)
}

// Records the outcome, notifies, leaves a durable trace in the chat session, and
// tells the UI so it can run the agent's follow-up summary.
fn finish(app: &AppHandle, task_id: &str, assigned_agent: &str, input: &str, session_id: &str, chat_agent_id: &str, outcome: Result<String, String>) {
  let (status, result) = match outcome {
    Ok(out) => ("completed", out),
    Err(e) => ("failed", format!("Error: {e}")),
  };
  if let Ok(conn) = db(app) {
    let _ = conn.execute("UPDATE tasks SET status=?1, result=?2, completed_at=?3 WHERE id=?4", params![status, result, chrono::Utc::now().to_rfc3339(), task_id]);
    // The chat it came from gets a system turn, so the agent still has the result
    // in context next time even if the window was closed when it finished.
    if !session_id.is_empty() {
      let _ = crate::memory::append_session_message(&conn, session_id, chat_agent_id, "system", &format!("Background task {task_id} {status}:\n{result}"));
    }
  }
  crate::notify::notify_and_push(app, "task", &format!("task {status}"), &format!("{assigned_agent}: {input}"), "task", task_id);
  let _ = app.emit("lumin-task-finished", serde_json::json!({
    "taskId": task_id, "chatAgentId": chat_agent_id, "assignedAgent": assigned_agent,
    "sessionId": session_id, "status": status, "result": result,
  }));
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
  let task_id = {
    let conn = db(&app)?;
    create_task(&conn, &NewTask::agent(&requester, &assigned_agent, &input, &context, None, ""))?
  };
  run_task_inner(&app, &task_id).await;
  get_task(app, task_id)?.ok_or("Task disappeared".into())
}

#[tauri::command]
pub fn cancel_task(app: AppHandle, id: String) -> Result<(), String> {
  let conn = db(&app)?;
  conn.execute("UPDATE tasks SET status='cancelled', completed_at=?1 WHERE id=?2 AND status IN ('pending','running')", params![chrono::Utc::now().to_rfc3339(), id]).map_err(|e| e.to_string())?;
  // Same signal as a normal end, so the activity panel drops the job.
  let _ = app.emit("lumin-task-finished", serde_json::json!({
    "taskId": id, "chatAgentId": "", "sessionId": "", "status": "cancelled", "result": "",
  }));
  Ok(())
}

// Writes an edited result back onto the task, so an edit made in the activity
// panel's text view outlives the session that produced it.
#[tauri::command]
pub fn update_task_result(app: AppHandle, id: String, result: String) -> Result<(), String> {
  let conn = db(&app)?;
  conn.execute("UPDATE tasks SET result=?1 WHERE id=?2", params![result, id]).map_err(|e| e.to_string())?;
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
