// Manager (Laos): the root system agent. Owns its toolset, system prompt,
// the turn loop and tool dispatch, plus the confirmation gate for mutating
// tool calls (approvals).

use async_trait::async_trait;
use rusqlite::{params, Connection};
use tauri::{AppHandle, Manager};

use crate::agents::{api_key_for, build_workspace_context, skills_prompt, tool_schemas};
use crate::db::{db, now};
use crate::http;
use crate::integrations::{integration_definitions, save_integration_config};
use crate::memory::{build_context_bundle, load_memory, MEMORY_SUMMARY_KEY};
use crate::models::*;
use crate::storage::{manager_default_model, parse_json_vec, save_agent, save_workflow};
use crate::tasks::{create_task, delegate_task, list_tasks, task_from_row};
use crate::tools::{AgentTool, ReadAnyFileTool, RunCommandTool, SearchFilesTool, MAX_TOOL_ROUNDS};
use crate::workflows::execute_workflow;

struct ManagerListAgents;
struct ManagerDelegate;
struct ManagerTaskStatus;
struct ManagerListTasks;
struct ManagerSwitch;

#[async_trait]
impl AgentTool for ManagerListAgents {
  fn name(&self) -> String { "list_agents".into() }
  fn description(&self) -> String { "List all available agents with their ids, descriptions and capabilities.".into() }
  fn params_schema(&self) -> serde_json::Value { serde_json::json!({ "type": "object", "properties": {}, "required": [] }) }
  async fn run(&self, _args: &serde_json::Value) -> Result<String, String> { Ok("list_agents".into()) } // replaced at runtime
}

#[async_trait]
impl AgentTool for ManagerDelegate {
  fn name(&self) -> String { "delegate_task".into() }
  fn description(&self) -> String { "Delegate a task to an agent. Params: agentId (string), task (string), context (string, optional). Creates a task and runs the agent.".into() }
  fn params_schema(&self) -> serde_json::Value {
    serde_json::json!({ "type": "object", "properties": { "agentId": { "type": "string" }, "task": { "type": "string" }, "context": { "type": "string" } }, "required": ["agentId", "task"] })
  }
  async fn run(&self, _args: &serde_json::Value) -> Result<String, String> { Ok("delegate_task".into()) } // replaced at runtime
}

#[async_trait]
impl AgentTool for ManagerTaskStatus {
  fn name(&self) -> String { "get_task_status".into() }
  fn description(&self) -> String { "Get the status and result of a task. Params: taskId (string).".into() }
  fn params_schema(&self) -> serde_json::Value {
    serde_json::json!({ "type": "object", "properties": { "taskId": { "type": "string" } }, "required": ["taskId"] })
  }
  async fn run(&self, _args: &serde_json::Value) -> Result<String, String> { Ok("get_task_status".into()) } // replaced at runtime
}

#[async_trait]
impl AgentTool for ManagerListTasks {
  fn name(&self) -> String { "list_tasks".into() }
  fn description(&self) -> String { "List all tasks and their statuses.".into() }
  fn params_schema(&self) -> serde_json::Value { serde_json::json!({ "type": "object", "properties": {}, "required": [] }) }
  async fn run(&self, _args: &serde_json::Value) -> Result<String, String> { Ok("list_tasks".into()) } // replaced at runtime
}

#[async_trait]
impl AgentTool for ManagerSwitch {
  fn name(&self) -> String { "switch_agent".into() }
  fn description(&self) -> String { "Switch the current conversation to another agent. Params: agentId (string).".into() }
  fn params_schema(&self) -> serde_json::Value {
    serde_json::json!({ "type": "object", "properties": { "agentId": { "type": "string" } }, "required": ["agentId"] })
  }
  async fn run(&self, _args: &serde_json::Value) -> Result<String, String> { Ok("switch_agent".into()) } // replaced at runtime
}

// App-control tools: the Manager can create/update/delete agents and workflows,
// configure integrations, run tasks, and inspect the workspace.
macro_rules! manager_tool {
  ($struct:ident, $name:literal, $desc:literal, $props:expr, $req:expr) => {
    struct $struct;
    #[async_trait]
    impl AgentTool for $struct {
      fn name(&self) -> String { $name.into() }
      fn description(&self) -> String { $desc.into() }
      fn params_schema(&self) -> serde_json::Value {
        serde_json::json!({ "type": "object", "properties": $props, "required": $req })
      }
      async fn run(&self, _args: &serde_json::Value) -> Result<String, String> { Ok($name.into()) }
    }
  };
}

manager_tool!(ManagerCreateAgent, "create_agent", "Create a new agent. Params: name (string), objective (string), model (string, e.g. 'groq:llama-3.3-70b-versatile'), toolIds (array, optional), integrations (array, optional), permissions (array, optional: 'network', 'files', or 'host_fs' for host file access). No credentials are needed to create an agent â€” host file access is granted via the permissions array ('host_fs').", serde_json::json!({ "name": { "type": "string" }, "objective": { "type": "string" }, "model": { "type": "string" }, "toolIds": { "type": "array", "items": { "type": "string" } }, "integrations": { "type": "array", "items": { "type": "string" } }, "permissions": { "type": "array", "items": { "type": "string" } } }), serde_json::json!(["name", "objective", "model"]));
manager_tool!(ManagerUpdateAgent, "update_agent", "Update fields on an existing agent. Params: agentId (string), name?, objective?, model?, toolIds?, integrations?, permissions?.", serde_json::json!({ "agentId": { "type": "string" }, "name": { "type": "string" }, "objective": { "type": "string" }, "model": { "type": "string" }, "toolIds": { "type": "array" }, "integrations": { "type": "array" }, "permissions": { "type": "array" } }), serde_json::json!(["agentId"]));
manager_tool!(ManagerDeleteAgent, "delete_agent", "Delete an agent. Params: agentId (string).", serde_json::json!({ "agentId": { "type": "string" } }), serde_json::json!(["agentId"]));
manager_tool!(ManagerCreateWorkflow, "create_workflow", "Create a new workflow. Params: name (string), nodes (string, optional JSON), edges (string, optional JSON).", serde_json::json!({ "name": { "type": "string" }, "nodes": { "type": "string" }, "edges": { "type": "string" } }), serde_json::json!(["name"]));
manager_tool!(ManagerListWorkflows, "list_workflows", "List all workflows and their names/ids.", serde_json::json!({}), serde_json::json!([]));
manager_tool!(ManagerSearchWorkflows, "search_workflows", "Search workflows by partial name match. Params: query (string). Returns matching workflows with their names and ids — use this to find a workflow when you only remember part of its name.", serde_json::json!({ "query": { "type": "string" } }), serde_json::json!(["query"]));
manager_tool!(ManagerUpdateWorkflow, "update_workflow", "Update a workflow's nodes/edges. Params: workflowId (string), nodes (string JSON), edges (string JSON).", serde_json::json!({ "workflowId": { "type": "string" }, "nodes": { "type": "string" }, "edges": { "type": "string" } }), serde_json::json!(["workflowId"]));
manager_tool!(ManagerDeleteWorkflow, "delete_workflow", "Delete a workflow. Params: workflowId (string).", serde_json::json!({ "workflowId": { "type": "string" } }), serde_json::json!(["workflowId"]));
manager_tool!(ManagerRunWorkflow, "run_workflow", "Run a workflow with input. Params: workflowId (string), input (string).", serde_json::json!({ "workflowId": { "type": "string" }, "input": { "type": "string" } }), serde_json::json!(["workflowId", "input"]));
manager_tool!(ManagerConfigureIntegration, "configure_integration", "Configure an integration with credentials. Params: id (string, e.g. notion/airtable/sheets/docs/telegram), config (object of keys e.g. token/clientId/clientSecret). Confirm with the user before storing secrets.", serde_json::json!({ "id": { "type": "string" }, "config": { "type": "object" } }), serde_json::json!(["id", "config"]));
manager_tool!(ManagerListIntegrations, "list_integrations", "List integrations and their connected status.", serde_json::json!({}), serde_json::json!([]));
manager_tool!(ManagerTestIntegration, "test_integration", "Test an integration connection. Params: id (string).", serde_json::json!({ "id": { "type": "string" } }), serde_json::json!(["id"]));
manager_tool!(ManagerCreateTask, "create_task", "Create and run a task on an agent. Params: agentId (string), input (string), context (string, optional).", serde_json::json!({ "agentId": { "type": "string" }, "input": { "type": "string" }, "context": { "type": "string" } }), serde_json::json!(["agentId", "input"]));
manager_tool!(ManagerCancelTask, "cancel_task", "Cancel a task. Params: taskId (string).", serde_json::json!({ "taskId": { "type": "string" } }), serde_json::json!(["taskId"]));
manager_tool!(ManagerListModels, "list_models", "List configured models.", serde_json::json!({}), serde_json::json!([]));
manager_tool!(ManagerListTools, "list_tools", "List configured tools.", serde_json::json!({}), serde_json::json!([]));
manager_tool!(ManagerWorkspaceStatus, "get_workspace_status", "Get a condensed workspace status: agents, integrations, active tasks, recent runs.", serde_json::json!({}), serde_json::json!([]));
manager_tool!(ManagerRecallMemory, "recall_memory", "Recall what was discussed or done recently: long-term facts, the last chat summary, and today's/yesterday's condensed context. Call this when the user asks 'what have we done/talked about', to continue prior work, or to reference past decisions. Does NOT inject history into the conversation — it returns it as tool output.", serde_json::json!({}), serde_json::json!([]));
manager_tool!(ManagerKnowledgeBase, "knowledge_base", "Read and write the shared knowledge base. Params: action ('list' | 'get' | 'search' | 'save'), title (string, for save/search), content (string, for save), id (string, for get/save). Use it to store durable reference material (company wiki, ICP notes, decisions) that any agent can consult later.", serde_json::json!({ "action": { "type": "string" }, "title": { "type": "string" }, "content": { "type": "string" }, "id": { "type": "string" } }), serde_json::json!(["action"]));

pub(crate) fn manager_tools() -> Vec<Box<dyn AgentTool>> {
  vec![
    Box::new(ManagerListAgents),
    Box::new(ManagerDelegate),
    Box::new(ManagerTaskStatus),
    Box::new(ManagerListTasks),
    Box::new(ManagerSwitch),
    Box::new(ManagerCreateAgent),
    Box::new(ManagerUpdateAgent),
    Box::new(ManagerDeleteAgent),
    Box::new(ManagerCreateWorkflow),
    Box::new(ManagerListWorkflows),
    Box::new(ManagerSearchWorkflows),
    Box::new(ManagerUpdateWorkflow),
    Box::new(ManagerDeleteWorkflow),
    Box::new(ManagerRunWorkflow),
    Box::new(ManagerConfigureIntegration),
    Box::new(ManagerListIntegrations),
    Box::new(ManagerTestIntegration),
    Box::new(ManagerCreateTask),
    Box::new(ManagerCancelTask),
    Box::new(ManagerListModels),
    Box::new(ManagerListTools),
    Box::new(ManagerWorkspaceStatus),
    Box::new(ManagerRecallMemory),
    Box::new(ManagerKnowledgeBase),
    Box::new(SearchFilesTool),
    Box::new(ReadAnyFileTool),
    Box::new(RunCommandTool),
  ]
}

// Builds the Manager's system prompt by enumerating its actual tools, so its
// capabilities are always in sync with the code and never need hand-writing.
pub(crate) fn build_manager_system_prompt(conn: &Connection, memory_blob: &str) -> Result<String, String> {
  let context = build_workspace_context(conn)?;
  let mut tool_list = String::new();
  for t in manager_tools() {
    tool_list.push_str(&format!("- {}: {}\n", t.name(), t.description()));
  }
  // Skills attached to the Manager itself (same mechanism as regular agents).
  let skill_ids: Vec<String> = conn
    .query_row("SELECT skill_ids FROM agents WHERE is_manager=1 LIMIT 1", [], |r| r.get::<_, String>(0))
    .ok()
    .map(|s| parse_json_vec(&s))
    .unwrap_or_default();
  let skills = skills_prompt(conn, &skill_ids);

  Ok(format!(
    "You are the Manager of a real, running agent workspace application. You are NOT a simulated or virtual entity â€” you have real tools and real effects on the user's machine.\n\n\
     You can actually do these things right now (do not claim you cannot):\n{tool_list}\n\
     When a tool returns a result, that result is real. When you create an agent or run a task, it really happens on the user's device.\n\n\
     Rules:\n\
     - Never say you are 'just a language model' or that you lack the ability to do something that is in your tool list. If a user asks for something you can do with your tools, do it.\n\
     - Inspect the workspace freely with read-only tools.\n\
     - When you need to change state (creating/deleting agents or workflows, configuring integrations, running tasks), CALL THE TOOL IN THIS TURN. You must emit the tool call now â€” never ask the user to type 'yes', never ask them to 'provide permissions', never request credentials in your reply, and never describe the tool you would use. The application intercepts your tool call and shows the user a confirmation popup automatically; they approve or reject there. After the tool executes, report its result. If you are not sure you are allowed to do something, call the tool anyway â€” the popup is the permission gate.\n\
     - Creating an agent requires NO credentials. Do not ask the user for API keys or 'file access credentials' when creating an agent â€” file access is just a permission value in the create_agent call.\n\
     - Never store secrets (API keys/tokens) without the user's explicit approval in the confirmation popup.\n\
     - Delegate domain work to agents rather than doing it inline.\n\n\
     Workspace context:\n{context}\n\n{memory_blob}\n{skills}\
     Return a concise, helpful reply to the user."
  ))
}



// Runs a Manager conversation turn: loads the Manager agent, injects workspace
// context, attaches manager tools, executes tool-calls against real backend functions.
pub(crate) async fn manager_turn(app: &AppHandle, message: &str) -> Result<String, String> {
  let conn = db(app)?;
  // Find the manager agent (is_manager=1); create a default if missing.
  let manager = {
    let mut stmt = conn.prepare("SELECT id, name, objective, model, tool_ids, integrations, home_path, permissions, skill_ids FROM agents WHERE is_manager=1 LIMIT 1").map_err(|e| e.to_string())?;
    let mut rows = stmt.query_map([], |row| {
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
  let mut manager = match manager {
    Some(m) => m,
    None => {
      let model = manager_default_model(&conn);
      if model.is_empty() {
        return Err("No model configured. Add an enabled model in the Models tab first.".into());
      }
      let m = AgentRequest { id: "manager".into(), name: "Laos".into(), objective: "You are Laos, the workspace orchestrator. Control the workspace and coordinate work.".into(), model: model.clone(), tool_ids: vec![], integrations: vec![], home_path: "agents/manager".into(), permissions: vec!["network".into()], skill_ids: vec![] };
      conn.execute("INSERT INTO agents (id, name, objective, model, tool_ids, integrations, memory, permissions, home_path, color, is_manager) VALUES (?1,?2,?3,?4,'[]','[]',1,?5,?6,'#22c55e',1)", params![m.id, m.name, m.objective, m.model, serde_json::to_string(&m.permissions).unwrap_or_else(|_| "[]".into()), m.home_path]).map_err(|e| e.to_string())?;
      m
    }
  };
  // Ensure the Manager uses a configured model. If the stored value isn't one
  // of the enabled model configs, pick the best enabled one (cloud before Ollama)
  // so the Manager works with whatever the user configured.
  {
    let configured = {
      let stmt = conn.prepare("SELECT id FROM model_configs WHERE enabled=1").ok();
      let mut ids = Vec::new();
      if let Some(mut stmt) = stmt {
        if let Ok(rows) = stmt.query_map([], |row| row.get::<_, String>(0)) {
          for r in rows { if let Ok(id) = r { ids.push(id); } }
        }
      }
      ids
    };
    if !configured.contains(&manager.model) {
      let best = manager_default_model(&conn);
      if best.is_empty() {
        return Err("No model configured. Add an enabled model in the Models tab first.".into());
      }
      let _ = conn.execute("UPDATE agents SET model=?1 WHERE id='manager'", params![best]);
      manager.model = best;
    }
  }

  // Inject the Manager's compact long-term memory (tier 2), matching the
  // streaming path. Day/last-chat context stays retrieval-only (recall_memory)
  // so it never leaks across chats.
  let memory = load_memory(&conn, &manager.id);
  let mut memory_blob = String::new();
  for (k, v) in &memory {
    if k == MEMORY_SUMMARY_KEY {
      memory_blob.push_str(&format!("[Memory summary: {v}]\n"));
    } else if let Some(fact) = k.strip_prefix("fact:") {
      memory_blob.push_str(&format!("- {fact}: {v}\n"));
    }
  }
  let context = build_manager_system_prompt(&conn, &memory_blob)?;
  let prompt = format!("{context}\n\nUser message: {message}");

  // Manager tools execute against real backend functions.
  let tools = manager_tools();

  let home = app.path().app_data_dir().map_err(|e| e.to_string())?.join("agents").join(&manager.id);
  let mut messages = vec![
    ChatMessage { role: "system".into(), content: Some(prompt), tool_calls: None, tool_call_id: None },
  ];
  let client = http::client();
  // The provider expects the bare model name; only our stored id carries the prefix.
  let api_model = manager.model.split_once(':').map(|(_, m)| m.to_string()).unwrap_or_else(|| manager.model.clone());
  let mut final_output = String::new();
  let is_ollama = manager.model.starts_with("ollama:");
  let is_groq = manager.model.starts_with("groq:");
  let provider_url = if is_ollama {
    "http://127.0.0.1:11434/api/chat".to_string()
  } else if is_groq {
    "https://api.groq.com/openai/v1/chat/completions".to_string()
  } else {
    "https://openrouter.ai/api/v1/chat/completions".to_string()
  };
  let provider_key = if is_ollama { None } else { Some(api_key_for(&conn, &manager.model)?) };

  for _round in 0..MAX_TOOL_ROUNDS {
    let mut body = serde_json::json!({ "model": api_model, "messages": messages, "tools": tool_schemas(&tools), "stream": false });
    if !is_ollama { http::apply_openai_defaults(&mut body, !is_groq, http::CHAT_MAX_TOKENS); }
    let response = http::send_with_retry(|| {
      let mut req = client.post(&provider_url).json(&body);
      if let Some(k) = &provider_key { req = req.header("Authorization", format!("Bearer {k}")); }
      if !is_ollama && !is_groq {
        req = req.header("HTTP-Referer", "https://local-agent-os.app").header("X-Title", "Local Agent OS");
      }
      req
    }, http::MODEL_ATTEMPTS).await?;
    let parsed: serde_json::Value = response.json().await.map_err(|e| e.to_string())?;
    // Support both Ollama (message.tool_calls) and OpenAI-compatible shapes.
    let calls_arr = parsed["message"]["tool_calls"].as_array()
      .or_else(|| parsed["choices"][0]["message"]["tool_calls"].as_array())
      .cloned()
      .unwrap_or_default();
    let assistant_msg = parsed["message"].clone();
    let assistant_msg = if assistant_msg.is_null() { parsed["choices"][0]["message"].clone() } else { assistant_msg };
    if !calls_arr.is_empty() {
      if let Ok(am) = serde_json::from_value::<ChatMessage>(assistant_msg.clone()) {
        messages.push(am);
      }
      for c in calls_arr {
        let name = c["function"]["name"].as_str().unwrap_or("").to_string();
        let args: serde_json::Value = c["function"]["arguments"].as_str().and_then(|s| serde_json::from_str(s).ok()).unwrap_or_else(|| serde_json::json!({}));
        // One dispatcher for every path. This used to be a second,
        // hand-maintained copy of the tool logic, and the two drifted apart —
        // a tool could be offered to the model and then come back "unknown".
        let result = if name == "run_command" {
          // Shell execution is gated behind a confirmation popup in the app.
          // This path has no way to ask, so refuse rather than run it silently.
          Err("run_command needs your approval, which this channel can't ask for. Run it from the Manager chat in the app.".to_string())
        } else {
          dispatch_manager_tool(app, &name, &args).await
        };
        let call_id = c["id"].as_str().unwrap_or("").to_string();
        messages.push(ChatMessage { role: "assistant".into(), content: None, tool_calls: Some(vec![ToolCall { id: call_id.clone(), kind: "function".into(), function: ToolCallFunction { name: name.clone(), arguments: args.clone() } }]), tool_call_id: None });
        messages.push(ChatMessage { role: "tool".into(), content: Some(result.map_err(|e| e.to_string())?), tool_calls: None, tool_call_id: Some(call_id) });
      }
    } else {
      // Ollama puts the text at message.content; the OpenAI-compatible providers
      // (Groq/OpenRouter) put it at choices[0].message.content. Reading only the
      // first shape returned an empty reply, which Telegram then rejected as
      // "message text is empty".
      final_output = parsed["message"]["content"].as_str()
        .or_else(|| parsed["choices"][0]["message"]["content"].as_str())
        .unwrap_or("")
        .to_string();
      break;
    }
  }
  let _ = home;
  if final_output.trim().is_empty() {
    // Every round was a tool call, or the provider returned no text. Never hand
    // back an empty reply.
    final_output = format!("{} didn't return a reply for that. Try again.", manager.name);
  }
  Ok(final_output)
}

// Shared dispatch for manager tools; used by both the non-streaming manager_turn
// and the streaming stream_chat manager path.
pub(crate) async fn dispatch_manager_tool(app: &AppHandle, name: &str, args: &serde_json::Value) -> Result<String, String> {
  let conn = db(app)?;
  match name {
    "list_agents" => {
      let mut out = String::new();
      let mut stmt = conn.prepare("SELECT id, name, description, objective, model, integrations FROM agents WHERE is_manager=0 ORDER BY name").map_err(|e| e.to_string())?;
      let rows = stmt.query_map([], |row| {
        let integrations: String = row.get(5)?;
        Ok(format!("- {} (id: {}): {}. Model: {}. Integrations: {}", row.get::<_, String>(1)?, row.get::<_, String>(0)?, row.get::<_, String>(2)?, row.get::<_, String>(3)?, integrations))
      }).map_err(|e| e.to_string())?;
      for row in rows { out.push_str(&row.map_err(|e| e.to_string())?); out.push('\n'); }
      Ok(if out.is_empty() { "No agents yet.".into() } else { out })
    }
    "list_tasks" => {
      let tasks = list_tasks(&conn)?;
      Ok(if tasks.is_empty() { "No tasks.".into() } else { tasks.iter().map(|t| format!("- {} â†’ {}: {} ({})", t.id, t.assigned_agent, t.input, t.status)).collect::<Vec<_>>().join("\n") })
    }
    "get_task_status" => {
      let task_id = args.get("taskId").and_then(|v| v.as_str()).unwrap_or("").to_string();
      let mut stmt = conn.prepare("SELECT id, requester, assigned_agent, status, input, context, result, created_at, completed_at FROM tasks WHERE id=?1").map_err(|e| e.to_string())?;
      let mut rows = stmt.query_map(params![task_id], task_from_row).map_err(|e| e.to_string())?;
      match rows.next().transpose().map_err(|e| e.to_string())? {
        Some(t) => Ok(format!("{} â†’ {}: {} â€” result: {}", t.id, t.assigned_agent, t.status, t.result.unwrap_or_default())),
        None => Ok(format!("Task {task_id} not found.")),
      }
    }
    "delegate_task" | "create_task" => {
      let agent_id = args.get("agentId").and_then(|v| v.as_str()).unwrap_or("").to_string();
      let task = args.get("task").and_then(|v| v.as_str()).or_else(|| args.get("input").and_then(|v| v.as_str())).unwrap_or("").to_string();
      let context = args.get("context").and_then(|v| v.as_str()).unwrap_or("").to_string();
      if agent_id.is_empty() || task.is_empty() { return Err("delegate_task/create_task requires agentId and input.".into()); }
      let task_id = create_task(&conn, "manager", &agent_id, &task, &context)?;
      match delegate_task(app, &task_id, &agent_id, &task, &context).await {
        Ok(out) => Ok(format!("Task {task_id} completed. Result:\n{out}")),
        Err(e) => Ok(format!("Task {task_id} failed: {e}")),
      }
    }
    "switch_agent" => {
      let agent_id = args.get("agentId").and_then(|v| v.as_str()).unwrap_or("").to_string();
      Ok(format!("Switching conversation to agent {agent_id}. The user can now talk to that agent directly."))
    }
    "create_agent" => {
      let name = args.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
      let objective = args.get("objective").and_then(|v| v.as_str()).unwrap_or("").to_string();
      let model = args.get("model").and_then(|v| v.as_str()).unwrap_or("").to_string();
      if name.is_empty() || objective.is_empty() || model.is_empty() { return Err("create_agent requires name, objective and model.".into()); }
      let arr = |k: &str| args.get(k).and_then(|v| v.as_array()).map(|a| a.iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect::<Vec<_>>()).unwrap_or_default();
      let id = format!("agent-{}", chrono::Utc::now().timestamp_millis());
      let agent = AgentRecord {
        id: id.clone(), name: name.clone(), objective, model,
        tool_ids: arr("toolIds"), integrations: arr("integrations"), memory: true, skill_ids: arr("skillIds"),
        permissions: arr("permissions").into_iter().filter(|p| p == "network" || p == "files").collect(),
        home_path: format!("agents/{id}"), color: "#22c55e".into(), x: 100.0, y: 100.0, is_manager: false, description: "".into(), persona: "ai-orb".into(),
      };
      save_agent(app.clone(), agent)?;
      Ok(format!("Created agent '{name}' (id: {id})."))
    }
    "delete_agent" => {
      let agent_id = args.get("agentId").and_then(|v| v.as_str()).unwrap_or("").to_string();
      conn.execute("DELETE FROM agents WHERE id=?1 AND is_manager=0", params![agent_id]).map_err(|e| e.to_string())?;
      Ok(format!("Deleted agent {agent_id}."))
    }
    "update_agent" => {
      let agent_id = args.get("agentId").and_then(|v| v.as_str()).unwrap_or("").to_string();
      if agent_id.is_empty() { return Err("update_agent requires agentId.".into()); }
      let existing = {
        let mut stmt = conn.prepare("SELECT id, name, objective, model, tool_ids, integrations, memory, permissions, home_path, color, x, y, is_manager, description, persona, skill_ids FROM agents WHERE id=?1").map_err(|e| e.to_string())?;
        let mut rows = stmt.query_map(params![agent_id], |row| {
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
        rows.next().transpose().map_err(|e| e.to_string())?
      };
      let Some(mut a) = existing else { return Err(format!("Agent {agent_id} not found.")); };
      if let Some(v) = args.get("name").and_then(|v| v.as_str()) { a.name = v.to_string(); }
      if let Some(v) = args.get("objective").and_then(|v| v.as_str()) { a.objective = v.to_string(); }
      if let Some(v) = args.get("model").and_then(|v| v.as_str()) { a.model = v.to_string(); }
      if let Some(v) = args.get("toolIds").and_then(|v| v.as_array()) { a.tool_ids = v.iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect(); }
      if let Some(v) = args.get("integrations").and_then(|v| v.as_array()) { a.integrations = v.iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect(); }
      if let Some(v) = args.get("permissions").and_then(|v| v.as_array()) { a.permissions = v.iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect(); }
      save_agent(app.clone(), a)?;
      Ok(format!("Updated agent {agent_id}."))
    }
    "list_workflows" => {
      let mut stmt = conn.prepare("SELECT id, name FROM workflows ORDER BY updated_at DESC").map_err(|e| e.to_string())?;
      let rows = stmt.query_map([], |row| Ok(format!("- {} (id: {})", row.get::<_, String>(1)?, row.get::<_, String>(0)?))).map_err(|e| e.to_string())?;
      let mut out = Vec::new();
      for r in rows { out.push(r.map_err(|e| e.to_string())?); }
      Ok(if out.is_empty() { "No workflows.".into() } else { out.join("\n") })
    }
    "search_workflows" => {
      let query = args.get("query").and_then(|v| v.as_str()).unwrap_or("").to_string();
      let like = format!("%{}%", query);
      let mut stmt = conn.prepare("SELECT id, name FROM workflows WHERE name LIKE ?1 ORDER BY updated_at DESC").map_err(|e| e.to_string())?;
      let rows = stmt.query_map(params![like], |row| Ok(format!("- {} (id: {})", row.get::<_, String>(1)?, row.get::<_, String>(0)?))).map_err(|e| e.to_string())?;
      let mut out = Vec::new();
      for r in rows { out.push(r.map_err(|e| e.to_string())?); }
      Ok(if out.is_empty() { format!("No workflows matching \"{}\".", query) } else { out.join("\n") })
    }
    "update_workflow" => {
      let wf_id = args.get("workflowId").and_then(|v| v.as_str()).unwrap_or("").to_string();
      let nodes = args.get("nodes").and_then(|v| v.as_str()).and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok()).unwrap_or(serde_json::json!([]));
      let edges = args.get("edges").and_then(|v| v.as_str()).and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok()).unwrap_or(serde_json::json!([]));
      let existing = {
        let mut stmt = conn.prepare("SELECT name FROM workflows WHERE id=?1").map_err(|e| e.to_string())?;
        let mut rows = stmt.query_map(params![wf_id], |row| row.get::<_, String>(0)).map_err(|e| e.to_string())?;
        rows.next().transpose().map_err(|e| e.to_string())?
      };
      let Some(name) = existing else { return Err(format!("Workflow {wf_id} not found.")); };
      save_workflow(app.clone(), WorkflowRecord { id: wf_id.clone(), name, nodes, edges, updated_at: now() })?;
      Ok(format!("Updated workflow {wf_id}."))
    }
    "run_workflow" => {
      let wf_id = args.get("workflowId").and_then(|v| v.as_str()).unwrap_or("").to_string();
      let input = args.get("input").and_then(|v| v.as_str()).unwrap_or("").to_string();
      let wf = {
        let mut stmt = conn.prepare("SELECT id, name, nodes, edges, updated_at FROM workflows WHERE id=?1").map_err(|e| e.to_string())?;
        let mut rows = stmt.query_map(params![wf_id], |row| {
          let nodes: String = row.get(2)?;
          let edges: String = row.get(3)?;
          Ok(WorkflowRecord {
            id: row.get(0)?, name: row.get(1)?,
            nodes: serde_json::from_str(&nodes).unwrap_or_else(|_| serde_json::json!([])),
            edges: serde_json::from_str(&edges).unwrap_or_else(|_| serde_json::json!([])),
            updated_at: row.get(4)?,
          })
        }).map_err(|e| e.to_string())?;
        rows.next().transpose().map_err(|e| e.to_string())?
      };
      let Some(wf) = wf else { return Err(format!("Workflow {wf_id} not found.")); };
      let exec = execute_workflow(app.clone(), wf, input, None).await?;
      Ok(format!("Workflow ran. Steps: {}; final output: {}", exec.steps.len(), exec.final_output))
    }
    "create_workflow" => {
      let name = args.get("name").and_then(|v| v.as_str()).unwrap_or("Untitled workflow").to_string();
      let nodes = args.get("nodes").and_then(|v| v.as_str()).and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok()).unwrap_or(serde_json::json!([]));
      let edges = args.get("edges").and_then(|v| v.as_str()).and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok()).unwrap_or(serde_json::json!([]));
      let wf = WorkflowRecord { id: format!("wf-{}", chrono::Utc::now().timestamp_millis()), name, nodes, edges, updated_at: now() };
      save_workflow(app.clone(), wf.clone())?;
      Ok(format!("Created workflow '{}' (id: {}).", wf.name, wf.id))
    }
    "delete_workflow" => {
      let wf_id = args.get("workflowId").and_then(|v| v.as_str()).unwrap_or("").to_string();
      conn.execute("DELETE FROM workflows WHERE id=?1", params![wf_id]).map_err(|e| e.to_string())?;
      Ok(format!("Deleted workflow {wf_id}."))
    }
    "configure_integration" => {
      let id = args.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
      let config = args.get("config").cloned().unwrap_or(serde_json::json!({}));
      save_integration_config(app.clone(), id.clone(), config)?;
      Ok(format!("Configured integration '{id}'. It may need a test to confirm the connection."))
    }
    "list_integrations" => {
      let defs = integration_definitions();
      let mut out = Vec::new();
      for d in defs {
        let mut stmt = conn.prepare("SELECT enabled, connected FROM integration_configs WHERE id=?1").map_err(|e| e.to_string())?;
        let mut rows = stmt.query_map(params![d.id], |row| Ok((row.get::<_, i64>(0)? != 0, row.get::<_, i64>(1)? != 0))).map_err(|e| e.to_string())?;
        let (enabled, connected) = rows.next().transpose().map_err(|e| e.to_string())?.unwrap_or((false, false));
        out.push(format!("- {} (id: {}): enabled={} connected={}", d.name, d.id, enabled, connected));
      }
      Ok(out.join("\n"))
    }
    "test_integration" => {
      let id = args.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
      Ok(format!("Testing integration {id}... use the Integrations tab to see the result."))
    }
    "cancel_task" => {
      let task_id = args.get("taskId").and_then(|v| v.as_str()).unwrap_or("").to_string();
      conn.execute("UPDATE tasks SET status='cancelled', completed_at=?1 WHERE id=?2", params![chrono::Utc::now().to_rfc3339(), task_id]).map_err(|e| e.to_string())?;
      Ok(format!("Cancelled task {task_id}."))
    }
    "list_models" => {
      let mut stmt = conn.prepare("SELECT id, provider, label FROM model_configs ORDER BY id").map_err(|e| e.to_string())?;
      let rows = stmt.query_map([], |row| Ok(format!("- {} ({})", row.get::<_, String>(2)?, row.get::<_, String>(1)?))).map_err(|e| e.to_string())?;
      let mut out = Vec::new();
      for r in rows { out.push(r.map_err(|e| e.to_string())?); }
      Ok(if out.is_empty() { "No models configured.".into() } else { out.join("\n") })
    }
    "list_tools" => {
      let mut stmt = conn.prepare("SELECT name, kind FROM tools WHERE enabled=1 ORDER BY name").map_err(|e| e.to_string())?;
      let rows = stmt.query_map([], |row| Ok(format!("- {} ({})", row.get::<_, String>(0)?, row.get::<_, String>(1)?))).map_err(|e| e.to_string())?;
      let mut out = Vec::new();
      for r in rows { out.push(r.map_err(|e| e.to_string())?); }
      Ok(if out.is_empty() { "No tools configured.".into() } else { out.join("\n") })
    }
    "get_workspace_status" => {
      let agents_n: i64 = conn.query_row("SELECT COUNT(*) FROM agents WHERE is_manager=0", [], |r| r.get(0)).unwrap_or(0);
      let tasks_n: i64 = conn.query_row("SELECT COUNT(*) FROM tasks WHERE status IN ('pending','running')", [], |r| r.get(0)).unwrap_or(0);
      let runs_n: i64 = conn.query_row("SELECT COUNT(*) FROM runs", [], |r| r.get(0)).unwrap_or(0);
      let conn_n: i64 = conn.query_row("SELECT COUNT(*) FROM integration_configs WHERE connected=1", [], |r| r.get(0)).unwrap_or(0);
      Ok(format!("Agents: {agents_n}. Connected integrations: {conn_n}. Active tasks: {tasks_n}. Total runs: {runs_n}."))
    }
    "recall_memory" => {
      let manager_id = {
        let mut stmt = conn.prepare("SELECT id FROM agents WHERE is_manager=1 LIMIT 1").map_err(|e| e.to_string())?;
        let mut rows = stmt.query_map([], |row| row.get::<_, String>(0)).map_err(|e| e.to_string())?;
        rows.next().transpose().map_err(|e| e.to_string())?.unwrap_or_else(|| "manager".to_string())
      };
      let bundle = build_context_bundle(&conn, &manager_id).unwrap_or_default();
      Ok(if bundle.trim().is_empty() { "No stored memory yet.".to_string() } else { bundle })
    }
    "knowledge_base" => {
      let action = args.get("action").and_then(|v| v.as_str()).unwrap_or("list").to_string();
      let title = args.get("title").and_then(|v| v.as_str()).unwrap_or("").to_string();
      let content = args.get("content").and_then(|v| v.as_str()).unwrap_or("").to_string();
      let id = args.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
      match action.as_str() {
        "list" => {
          let mut stmt = conn.prepare("SELECT id, title, updated_at FROM knowledge_docs ORDER BY updated_at DESC").map_err(|e| e.to_string())?;
          let rows = stmt.query_map([], |row| Ok(format!("- {} (id: {})", row.get::<_, String>(1)?, row.get::<_, String>(0)?))).map_err(|e| e.to_string())?;
          let mut out = Vec::new();
          for r in rows { out.push(r.map_err(|e| e.to_string())?); }
          Ok(if out.is_empty() { "Knowledge base is empty.".into() } else { out.join("\n") })
        }
        "search" => {
          let like = format!("%{}%", title);
          let mut stmt = conn.prepare("SELECT id, title, content FROM knowledge_docs WHERE title LIKE ?1 OR content LIKE ?1 ORDER BY updated_at DESC LIMIT 5").map_err(|e| e.to_string())?;
          let rows = stmt.query_map(params![like], |row| Ok(format!("- {} (id: {}): {}", row.get::<_, String>(1)?, row.get::<_, String>(0)?, row.get::<_, String>(2)?.chars().take(150).collect::<String>()))).map_err(|e| e.to_string())?;
          let mut out = Vec::new();
          for r in rows { out.push(r.map_err(|e| e.to_string())?); }
          Ok(if out.is_empty() { format!("No docs matching \"{title}\".") } else { out.join("\n") })
        }
        "get" => {
          let mut stmt = conn.prepare("SELECT id, title, content FROM knowledge_docs WHERE id=?1 OR title=?1").map_err(|e| e.to_string())?;
          let mut rows = stmt.query_map(params![if id.is_empty() { title.clone() } else { id.clone() }], |row| Ok(format!("# {}\n\n{}", row.get::<_, String>(1)?, row.get::<_, String>(2)?))).map_err(|e| e.to_string())?;
          match rows.next().transpose().map_err(|e| e.to_string())? {
            Some(v) => Ok(v),
            None => Ok(format!("No knowledge base doc found for \"{}\".", if id.is_empty() { title } else { id })),
          }
        }
        "save" => {
          if title.is_empty() { return Err("knowledge_base save requires a title.".into()); }
          let doc_id = if id.is_empty() { format!("kb-{}", chrono::Utc::now().timestamp_millis()) } else { id };
          conn.execute(
            "INSERT INTO knowledge_docs (id, title, content, tags, updated_at) VALUES (?1,?2,?3,'[]',?4)
             ON CONFLICT(id) DO UPDATE SET title=excluded.title, content=excluded.content, updated_at=excluded.updated_at",
            params![doc_id, title, content, chrono::Utc::now().to_rfc3339()],
          ).map_err(|e| e.to_string())?;
          Ok(format!("Saved knowledge base doc \"{title}\" (id: {doc_id})."))
        }
        _ => Err(format!("Unknown knowledge_base action {action}.")),
      }
    }
    "search_files" => SearchFilesTool.run(args).await,
    "read_file_any" => ReadAnyFileTool.run(args).await,
    "run_command" => RunCommandTool.run(args).await,
    _ => Err(format!("Manager tool '{name}' not implemented.")),
  }
}

#[tauri::command]
pub async fn manager_message(app: AppHandle, message: String) -> Result<String, String> {
  manager_turn(&app, &message).await
}

// Confirmation gating for mutating manager tools: stream_chat emits a confirm
// request, and confirm_manager_tool (from the UI) records the user's decision.
pub(crate) struct Approval { pub(crate) approved: bool, pub(crate) edited_args: serde_json::Value }
static PENDING_APPROVALS: std::sync::OnceLock<std::sync::Mutex<std::collections::HashMap<String, Approval>>> = std::sync::OnceLock::new();
pub(crate) fn approvals() -> &'static std::sync::Mutex<std::collections::HashMap<String, Approval>> {
  PENDING_APPROVALS.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}

// Tools that mutate state and therefore need user confirmation.
pub(crate) fn requires_confirmation(tool: &str) -> bool {
  matches!(tool,
    "create_agent" | "update_agent" | "delete_agent"
    | "create_workflow" | "update_workflow" | "delete_workflow" | "run_workflow"
    | "configure_integration" | "create_task" | "cancel_task" | "delegate_task"
    | "run_command")
}

#[tauri::command]
pub async fn confirm_manager_tool(app: AppHandle, request_id: String, approved: bool, args: Option<serde_json::Value>, tool: String) -> Result<Option<String>, String> {
  // Record the user's decision only. chat::run_tool performs the single
  // execution once the approval is observed — executing here as well would run
  // every approved tool twice.
  let _ = (&app, &tool);
  let edited = args.unwrap_or_else(|| serde_json::json!({}));
  approvals().lock().map_err(|e| e.to_string())?.insert(request_id, Approval { approved, edited_args: edited });
  Ok(None)
}
