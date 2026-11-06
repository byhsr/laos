// Manager (Laos): the root system agent. Owns its toolset, system prompt,
// the turn loop and tool dispatch, plus the confirmation gate for mutating
// tool calls (approvals).

use async_trait::async_trait;
use rusqlite::{params, Connection};
use tauri::{AppHandle, Manager};

use crate::agents::{build_workspace_context, mcp_tools_for, skills_prompt, tool_schemas};
use crate::db::{db, now};
use crate::http;
use crate::integrations::{integration_definitions, save_integration_config};
use crate::memory::build_context_bundle;
use crate::models::*;
use crate::provider;
use crate::storage::{manager_default_model, parse_json_vec, save_agent, save_workflow};
use crate::tasks::{list_tasks, spawn_task, task_from_row, NewTask};
use crate::tools::{AgentTool, ReadAnyFileTool, RunCommandTool, SearchFilesTool, MAX_TOOL_ROUNDS};
use crate::tooltext::parse_text_tool_calls;
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
  fn description(&self) -> String { "Delegate a task to an agent. Params: agentId (string), task (string), context (string, optional). Runs the agent in the BACKGROUND and returns at once — you are told the result when it finishes, so never wait for it.".into() }
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
manager_tool!(ManagerCreateTask, "create_task", "Create and run a task on an agent in the BACKGROUND. Params: agentId (string), input (string), context (string, optional). Returns at once; the result comes back to you later.", serde_json::json!({ "agentId": { "type": "string" }, "input": { "type": "string" }, "context": { "type": "string" } }), serde_json::json!(["agentId", "input"]));
manager_tool!(ManagerCancelTask, "cancel_task", "Cancel a task. Params: taskId (string).", serde_json::json!({ "taskId": { "type": "string" } }), serde_json::json!(["taskId"]));
manager_tool!(ManagerListModels, "list_models", "List configured models.", serde_json::json!({}), serde_json::json!([]));
manager_tool!(ManagerListTools, "list_tools", "List configured tools.", serde_json::json!({}), serde_json::json!([]));
manager_tool!(ManagerCreateTool, "create_tool", "Create a custom API tool — how a REST integration is added without native code — and optionally attach it to an agent. Params: name (string), description (string), config (object), agentId (string, optional; attaches it to that agent in the same call). config is either a single endpoint { method, url (with {param} placeholders), body?, params? } or, like an integration, a suite { baseUrl, actions: [ { name, method, path, description?, body?, params? } ] }. Each param is { name, type ('string'|'integer'|'number'|'boolean'), description, required?, in? ('path'|'query'|'body'|'header') }. To have it authenticate, declare auth as { type: 'bearer'|'header'|'query', name? } — but NEVER pass the credential itself: the user enters the key in the workshop, and until they do its calls fail with 'No credential is stored'. approval ('never'|'writes'|'always', default 'writes') decides when it asks the user first. Tell the user when a tool you created still needs its key.", serde_json::json!({ "name": { "type": "string" }, "description": { "type": "string" }, "config": { "type": "object" }, "agentId": { "type": "string" } }), serde_json::json!(["name", "config"]));
manager_tool!(ManagerWorkspaceStatus, "get_workspace_status", "Get a condensed workspace status: agents, integrations, active tasks, recent runs.", serde_json::json!({}), serde_json::json!([]));
manager_tool!(ManagerRecallMemory, "recall_memory", "Recall what was discussed or done recently: long-term facts, the last chat summary, and today's/yesterday's condensed context. Call this when the user asks 'what have we done/talked about', to continue prior work, or to reference past decisions. Does NOT inject history into the conversation — it returns it as tool output.", serde_json::json!({}), serde_json::json!([]));
manager_tool!(ManagerKnowledgeBase, "knowledge_base", "The shared knowledge base — durable REFERENCE material any agent may need later (company wiki, ICP, specs, policies, runbooks, product facts). Params: action ('list' | 'get' | 'search' | 'save'), title (string, for save/search), content (string, for save), id (string, for get/save). NOT for anything to do — tasks, chores, errands, reminders or one-off to-dos go to 'items' — and NOT for personal rules about the user (the 'remember' tool).", serde_json::json!({ "action": { "type": "string" }, "title": { "type": "string" }, "content": { "type": "string" }, "id": { "type": "string" } }), serde_json::json!(["action"]));

manager_tool!(ManagerTidyKnowledge, "tidy_knowledge", "Find knowledge-base docs that are really memories or todos and propose where each should move. Read-only: returns the proposals; the user approves them in the Knowledge tab (or here, if they say yes).", serde_json::json!({}), serde_json::json!([]));

manager_tool!(ManagerAskUser, "ask_user", "Ask the user a question and wait for the answer. Over Telegram this shows buttons when options are given, or waits for their next message otherwise. Params: question (string), options (array of strings, optional).", serde_json::json!({ "question": { "type": "string" }, "options": { "type": "array", "items": { "type": "string" } } }), serde_json::json!(["question"]));

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
    Box::new(ManagerCreateTool),
    Box::new(ManagerWorkspaceStatus),
    Box::new(ManagerRecallMemory),
    Box::new(ManagerKnowledgeBase),
    Box::new(ManagerTidyKnowledge),
    Box::new(ManagerAskUser),
    Box::new(SearchFilesTool),
    Box::new(ReadAnyFileTool),
    Box::new(RunCommandTool),
  ]
}

// True for the Manager's own tools — the ones whose `run` is a placeholder and
// which dispatch_manager_tool executes by name.
pub(crate) fn is_manager_builtin(name: &str) -> bool {
  manager_tools().iter().any(|t| t.name() == name)
}

// Executes one Manager tool call. Its own tools dispatch by name; anything else
// in `tools` — the imported MCP tools it also holds — runs as a normal AgentTool.
pub(crate) async fn run_manager_tool(app: &AppHandle, tools: &[Box<dyn AgentTool>], name: &str, args: &serde_json::Value, session_id: Option<&str>, chat_agent_id: &str) -> Result<String, String> {
  if !is_manager_builtin(name) {
    if let Some(t) = tools.iter().find(|t| t.name() == name) {
      return t.run(args).await;
    }
  }
  dispatch_manager_tool(app, name, args, session_id, chat_agent_id).await
}

// Builds the Manager's system prompt by enumerating its actual tools, so its
// capabilities are always in sync with the code and never need hand-writing.
pub(crate) fn build_manager_system_prompt(conn: &Connection, name: &str, mcp_tool_ids: &[String], pc_control: bool, network: bool) -> Result<String, String> {
  let context = build_workspace_context(conn)?;
  // Whether the Manager was opted into browser control (attached the `browser` tool).
  let has_browser = mcp_tool_ids.iter().any(|t| t == "browser");
  let mut tool_list = String::new();
  for t in manager_tools() {
    tool_list.push_str(&format!("- {}: {}\n", t.name(), t.description()));
  }
  // The remember and items tools are provided at runtime (not dispatch-by-name
  // builtins), so list them explicitly here.
  tool_list.push_str("- remember: Save a durable fact or standing rule about the user (\"never use emoji\", \"I prefer IST\") — kind 'policy' for a never/always rule, 'preference' for a taste, 'semantic' for a plain fact.\n");
  tool_list.push_str("- items: The USER's notes, todos, reminders, goals, resources and habits — anything with a when or a do. Full CRUD (add, list, update, complete, delete). Reminders are scheduled and pushed to Telegram. This is their store, not yours.\n");
  tool_list.push_str("- scratchpad: Your own private working area for this chat — thinking, drafts, intermediate findings, anything you need to keep track of while you work. Keep your own working notes here, never in items. Actions: read, append, write, clear.\n");
  tool_list.push_str("- document: Write a document (an artifact) the user can read and download in the chat — a report, spec, plan, README, article or snippet. Put the full content here rather than in your reply. Actions: create, update, read, list, delete.\n");
  tool_list.push_str("- current_datetime: The current date and time in the user's configured timezone (plus tomorrow/yesterday). Use it before scheduling or reasoning about 'now'.\n");
  tool_list.push_str("- monid: Reach hundreds of external data tools (search/scraping/enrichment/media) through Monid. discover → inspect → run. Needs the Monid CLI installed and keyed.\n");
  tool_list.push_str("- yt_dlp: Download a video, or extract its audio (mp3), from a URL with the yt-dlp CLI. Needs yt-dlp on PATH.\n");
  // Only the MCP tools the Manager was explicitly opted into are listed (and
  // offered) — attaching them in its config is the opt-in.
  for t in mcp_tools_for(conn, mcp_tool_ids) {
    tool_list.push_str(&format!("- {}: {}\n", t.name(), t.description()));
  }
  // Custom API tools the Manager holds — the same rows the runtime builds, so the
  // list can never drift from what is actually callable.
  for t in crate::agents::custom_tools_for(conn, mcp_tool_ids, network) {
    tool_list.push_str(&format!("- {}: {}\n", t.name(), t.description()));
  }
  // PC control tools, only when the Manager holds the pc_control permission.
  if pc_control {
    for t in crate::tools::desktop_tools() {
      tool_list.push_str(&format!("- {}: {}\n", t.name(), t.description()));
    }
  }
  // Browser control (the in-app mini browser), only when the Manager attached it.
  if has_browser {
    for t in crate::tools::browser_tools() {
      tool_list.push_str(&format!("- {}: {}\n", t.name(), t.description()));
    }
  }
  // Saved scripts (custom apps), discoverable and runnable via run_script.
  for t in crate::tools::script_tools(conn) {
    tool_list.push_str(&format!("- {}: {}\n", t.name(), t.description()));
  }
  // Skills attached to the Manager itself (same mechanism as regular agents).
  let skill_ids: Vec<String> = conn
    .query_row("SELECT skill_ids FROM agents WHERE is_manager=1 LIMIT 1", [], |r| r.get::<_, String>(0))
    .ok()
    .map(|s| parse_json_vec(&s))
    .unwrap_or_default();
  let skills = skills_prompt(conn, &skill_ids);
  // When the memory connector is present, tell the Manager which identity to file
  // memories under so they land where the Memory view looks.
  let memory_note = if conn
    .query_row("SELECT COUNT(*) FROM tools WHERE kind='mcp' AND integration_id='memory' AND enabled=1", [], |r| r.get::<_, i64>(0))
    .map(|c| c > 0).unwrap_or(false)
  {
    "You have a persistent memory. When you use a memory tool, pass agentId \"manager\" and scope {\"type\":\"agent\",\"id\":\"manager\"}.\n"
  } else {
    ""
  };
  // PC control guidance, only when the Manager can actually control the PC.
  let pc_note = if pc_control {
    "You can see and control the screen. To act on it: call screen_capture first, then pass mouse coordinates in that image's pixel space (origin top-left); use type_text / press_keys for the keyboard.\n"
  } else {
    ""
  };
  // Browser guidance, only when the Manager has browser control: the in-app panel
  // is the web browser, never the user's desktop.
  let browser_note = if has_browser {
    "Browser control (browser_open / browser_navigate / browser_read / browser_click / browser_type) opens pages in the app's own in-app mini-browser panel — it never opens anything on the user's desktop. Use these to browse the web; never use open_path or launch_app to open a web browser.\n"
  } else {
    ""
  };
  // Memory is pull-only: nothing about prior context is injected here. The
  // Manager brings it in on demand via recall_memory (or any memory tool it
  // holds), so a new chat starts clean.

  // Where captured things belong — the same contract every agent gets, so the
  // Manager stops filing preferences and todos as knowledge-base docs.
  let routing = crate::routing::ROUTING_RULES;

  Ok(format!(
    "You are {name}, the lead agent of a real, running agent workspace application. You are NOT a simulated or virtual entity â€” you have real tools and real effects on the user's machine.\n\n\
     You can actually do these things right now (do not claim you cannot):\n{tool_list}\n\
     When a tool returns a result, that result is real. When you create an agent or run a task, it really happens on the user's device.\n\n\
     Rules:\n\
     - Never say you are 'just a language model' or that you lack the ability to do something that is in your tool list. If a user asks for something you can do with your tools, do it.\n\
     - Inspect the workspace freely with read-only tools.\n\
     - When you need to change state (creating/deleting agents or workflows, configuring integrations, running tasks), CALL THE TOOL IN THIS TURN. You must emit the tool call now â€” never ask the user to type 'yes', never ask them to 'provide permissions', never request credentials in your reply, and never describe the tool you would use. The application intercepts your tool call and shows the user a confirmation popup automatically; they approve or reject there. After the tool executes, report its result. If you are not sure you are allowed to do something, call the tool anyway â€” the popup is the permission gate.\n\
     - Creating an agent requires NO credentials. Do not ask the user for API keys or 'file access credentials' when creating an agent â€” file access is just a permission value in the create_agent call.\n\
     - Never store secrets (API keys/tokens) without the user's explicit approval in the confirmation popup.\n\
     - Delegate domain work to agents rather than doing it inline. Anything time-consuming (research, running commands, multi-step work) goes to a task: 'delegate_task' returns immediately and its result is delivered back to you when it finishes — never wait for it, never say you'll 'check' later, and never claim the work is done before you have the result.\n\n\
     Workspace context:\n{context}\n\n{skills}{memory_note}{pc_note}{browser_note}{identity}{routing}\
     Return a concise, helpful reply to the user."
  , identity = crate::app_settings::identity_prompt(conn)))
}



// Runs a Manager conversation turn: loads the Manager agent, injects workspace
// context, attaches manager tools, executes tool-calls against real backend functions.
pub(crate) async fn manager_turn(app: &AppHandle, message: &str) -> Result<String, String> {
  let conn = db(app)?;
  // Find the manager agent (is_manager=1); create a default if missing.
  let manager = {
    let mut stmt = conn.prepare("SELECT id, name, objective, model, tool_ids, integrations, home_path, permissions, skill_ids, reasoning FROM agents WHERE is_manager=1 LIMIT 1").map_err(|e| e.to_string())?;
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
        reasoning: row.get(9)?,
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
      let reasoning = conn.query_row("SELECT reasoning FROM model_configs WHERE id=?1", params![model], |r| r.get::<_, String>(0)).unwrap_or_else(|_| default_reasoning());
      let m = AgentRequest { id: "manager".into(), name: "Laos".into(), objective: "You are Laos, the workspace orchestrator. Control the workspace and coordinate work.".into(), model: model.clone(), tool_ids: vec![], integrations: vec![], home_path: "agents/manager".into(), permissions: vec!["network".into()], skill_ids: vec![], reasoning: reasoning.clone() };
      conn.execute("INSERT INTO agents (id, name, objective, model, tool_ids, integrations, memory, permissions, home_path, color, is_manager, reasoning) VALUES (?1,?2,?3,?4,'[]','[]',1,?5,?6,'#22c55e',1,?7)", params![m.id, m.name, m.objective, m.model, serde_json::to_string(&m.permissions).unwrap_or_else(|_| "[]".into()), m.home_path, reasoning]).map_err(|e| e.to_string())?;
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

  // Durable memory is injected, so the Manager starts each turn already knowing
  // what it has learned (recall_memory still works for a deeper pull).
  let pc_control = manager.permissions.iter().any(|p| p == "pc_control");
  let network = manager.permissions.iter().any(|p| p == "network");
  // Route the turn's content before the model sees it, exactly as the streaming
  // chat path does — a standing rule or a timed reminder is filed, not described.
  let triage = crate::routing::triage(app, &manager.id, message, None).await;
  let scope = crate::agent_memory::scope_key("agent", &manager.id);
  let memory = crate::memory::memory_block(app, &manager.id, &scope, message).await;
  let context = build_manager_system_prompt(&conn, &manager.name, &manager.tool_ids, pc_control, network)?;
  let prompt = format!("{context}{memory}\n\nUser message: {message}");
  // Tell the model what was already filed, so it doesn't file it a second time.
  let prompt = match &triage { Some(t) => format!("{prompt}{}", t.note), None => prompt };

  // The Manager's built-in tools plus only the MCP tools it was opted into.
  let tools: Vec<Box<dyn AgentTool>> = {
    let mut t = manager_tools();
    if pc_control { t.extend(crate::tools::desktop_tools()); }
    t.extend(crate::tools::script_tools(&conn));
    t.extend(mcp_tools_for(&conn, &manager.tool_ids));
    // Custom API tools the Manager holds, matching the streaming chat path.
    t.extend(crate::agents::custom_tools_for(&conn, &manager.tool_ids, network));
    // The integrations the Manager holds are callable, matching the streaming
    // chat path.
    t.extend(crate::agents::integration_tools(app, &conn, &manager.integrations));
    // An explicit "remember this" from any surface becomes durable memory, and a
    // todo/reminder becomes an item rather than a knowledge-base doc.
    t.push(crate::tools::remember_tool(app, &manager.id));
    t.push(crate::tools::items_tool(app, &manager.id));
    t.push(crate::tools::scratchpad_tool(app));
    t.push(crate::tools::document_tool(app));
    t.push(crate::tools::current_time_tool(app));
    t.push(Box::new(crate::tools::MonidTool));
    t.push(Box::new(crate::tools::YtDlpTool));
    t
  };

  let home = app.path().app_data_dir().map_err(|e| e.to_string())?.join("agents").join(&manager.id);
  let mut messages = vec![
    ChatMessage { role: "system".into(), content: Some(prompt), tool_calls: None, tool_call_id: None },
  ];
  let client = http::client();
  // Endpoint, key and reasoning all resolve in one place. This path used to pass
  // `!is_groq` as its "is OpenRouter" flag, which routed an unknown prefix to
  // OpenRouter instead of failing.
  // Resolve with a local fallback so a bot/Telegram turn still replies when the
  // configured model is unusable.
  let (resolved, _fb) = crate::fallback::resolve_with_fallback(
    provider::resolve(&conn, &manager.model, None),
    crate::fallback::local_candidates(&conn),
    &manager.reasoning,
  ).await?;
  let is_ollama = resolved.is_ollama();
  let is_anthropic = resolved.is_anthropic();
  let mut final_output = String::new();

  for _round in 0..MAX_TOOL_ROUNDS {
    let mut body = serde_json::json!({ "model": resolved.model, "messages": messages, "tools": tool_schemas(&tools), "stream": false });
    let budget = provider::apply_reasoning(&mut body, &resolved, http::CHAT_MAX_TOKENS);
    if is_ollama {
      provider::apply_ollama_options(&mut body, budget);
    } else {
      http::apply_openai_defaults(&mut body, resolved.provider_routing(), budget);
    }
    if is_anthropic { body = crate::anthropic::to_wire(&body); }
    let response = http::send_model_request(&client, &resolved, &resolved.chat_url(), &body, http::MODEL_ATTEMPTS).await?;
    let mut parsed: serde_json::Value = response.json().await.map_err(|e| e.to_string())?;
    if is_anthropic { parsed = crate::anthropic::normalize_response(&parsed); }
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
        let result = if name == "run_command" && !crate::app_settings::autonomy(app) {
          // Shell execution is gated behind a confirmation popup in the app.
          // This path has no way to ask, so refuse rather than run it silently.
          Err("run_command needs your approval, which this channel can't ask for. Run it from the Manager chat in the app.".to_string())
        } else {
          run_manager_tool(app, &tools, &name, &args, None, &manager.id).await
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
      let content = parsed["message"]["content"].as_str()
        .or_else(|| parsed["choices"][0]["message"]["content"].as_str())
        .unwrap_or("")
        .to_string();
      // Text-protocol fallback: a model that emits its call as markup in
      // `content` never lands in `tool_calls`, so recover it and run it here
      // instead of showing the raw markup (see tooltext.rs).
      let (cleaned, text_calls) = parse_text_tool_calls(&content);
      if text_calls.is_empty() {
        final_output = cleaned;
        break;
      }
      messages.push(ChatMessage { role: "assistant".into(), content: Some(content), tool_calls: None, tool_call_id: None });
      for call in text_calls {
        let result = if call.name == "run_command" && !crate::app_settings::autonomy(app) {
          Err("run_command needs your approval, which this channel can't ask for. Run it from the Manager chat in the app.".to_string())
        } else {
          run_manager_tool(app, &tools, &call.name, &call.args, None, &manager.id).await
        };
        messages.push(ChatMessage { role: "user".into(), content: Some(format!("<tool_result name=\"{}\">\n{}\n</tool_result>", call.name, result.map_err(|e| e.to_string())?)), tool_calls: None, tool_call_id: None });
      }
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
// Cleans a model-proposed tool config: the shape is kept, the credential is not. A
// declared auth keeps its type, so the tool fails with a clear "no credential"
// message until the key is added in the workshop, while any value the model passed
// is dropped; a secret-looking static header is removed outright. A model must
// never be able to write a plaintext secret into the tool table.
fn model_tool_config(name: &str, config: &serde_json::Value) -> serde_json::Value {
  let mut config = config.clone();
  if let Some(auth) = config.get_mut("auth").and_then(|a| a.as_object_mut()) {
    auth.insert("value".into(), serde_json::json!(""));
  }
  if let Some(headers) = config.get_mut("headers").and_then(|h| h.as_array_mut()) {
    headers.retain(|h| h.get("name").and_then(|n| n.as_str()).map(|n| !crate::vault::is_secret_name(n)).unwrap_or(true));
  }
  // The function name is also the prefix every suite action is named with, so it
  // has to be a usable identifier rather than whatever the model wrote.
  config["name"] = serde_json::json!(crate::tools::sanitize(&name.to_lowercase().replace(' ', "_")));
  config
}

pub(crate) async fn dispatch_manager_tool(app: &AppHandle, name: &str, args: &serde_json::Value, session_id: Option<&str>, chat_agent_id: &str) -> Result<String, String> {
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
      // Fire-and-forget: the task runs in the background and its result is pushed
      // back into this chat when it finishes (see tasks::finish).
      let task_id = spawn_task(app, NewTask::agent("manager", &agent_id, &task, &context, session_id, chat_agent_id))?;
      Ok(format!("Task {task_id} started in the background on {agent_id}. Do not wait for it — tell the user it's running and that you'll report when it finishes."))
    }
    "switch_agent" => {
      let agent_id = args.get("agentId").and_then(|v| v.as_str()).unwrap_or("").to_string();
      Ok(format!("Switching conversation to agent {agent_id}. The user can now talk to that agent directly."))
    }
    "create_agent" => {
      let name = args.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
      let objective = args.get("objective").and_then(|v| v.as_str()).unwrap_or("").to_string();
      let model = args.get("model").and_then(|v| v.as_str()).unwrap_or("").to_string();
      let model = crate::provider::normalize_model_id(&conn, &model);
      if name.is_empty() || objective.is_empty() || model.is_empty() { return Err("create_agent requires name, objective and model.".into()); }
      if !crate::provider::model_provider_known(&conn, &model) {
        return Err(format!("Unknown model '{model}'. Pass a configured model id from list_models."));
      }
      let arr = |k: &str| args.get(k).and_then(|v| v.as_array()).map(|a| a.iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect::<Vec<_>>()).unwrap_or_default();
      let id = format!("agent-{}", chrono::Utc::now().timestamp_millis());
      // New agents seed their reasoning from the chosen model.
      let reasoning = conn.query_row("SELECT reasoning FROM model_configs WHERE id=?1", params![model], |r| r.get::<_, String>(0)).unwrap_or_else(|_| default_reasoning());
      let agent = AgentRecord {
        id: id.clone(), name: name.clone(), objective, model,
        tool_ids: arr("toolIds"), integrations: arr("integrations"), memory: true, skill_ids: arr("skillIds"),
        permissions: arr("permissions").into_iter().filter(|p| matches!(p.as_str(), "network" | "files" | "host_fs" | "pc_control")).collect(),
        home_path: format!("agents/{id}"), color: "#22c55e".into(), x: 100.0, y: 100.0, is_manager: false, description: "".into(), persona: "ai-orb".into(),
        pinned: false, avatar: String::new(), reasoning,
      };
      save_agent(app.clone(), agent)?;
      // The roster only loads once at startup, so a new agent would stay invisible
      // (sidebar, tool pickers, the split view) until a reload without this.
      let _ = tauri::Emitter::emit(app, "lumin-agents-changed", serde_json::json!({ "id": id }));
      Ok(format!("Created agent '{name}' (id: {id})."))
    }
    "delete_agent" => {
      let agent_id = args.get("agentId").and_then(|v| v.as_str()).unwrap_or("").to_string();
      conn.execute("DELETE FROM agents WHERE id=?1 AND is_manager=0", params![agent_id]).map_err(|e| e.to_string())?;
      let _ = tauri::Emitter::emit(app, "lumin-agents-changed", serde_json::json!({ "id": agent_id, "deleted": true }));
      Ok(format!("Deleted agent {agent_id}."))
    }
    "update_agent" => {
      let agent_id = args.get("agentId").and_then(|v| v.as_str()).unwrap_or("").to_string();
      if agent_id.is_empty() { return Err("update_agent requires agentId.".into()); }
      let existing = {
        let mut stmt = conn.prepare("SELECT id, name, objective, model, tool_ids, integrations, memory, permissions, home_path, color, x, y, is_manager, description, persona, skill_ids, pinned, avatar, reasoning FROM agents WHERE id=?1").map_err(|e| e.to_string())?;
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
            pinned: row.get::<_, i64>(16)? != 0, avatar: row.get(17)?,
            reasoning: row.get(18)?,
          })
        }).map_err(|e| e.to_string())?;
        rows.next().transpose().map_err(|e| e.to_string())?
      };
      let Some(mut a) = existing else { return Err(format!("Agent {agent_id} not found.")); };
      if let Some(v) = args.get("name").and_then(|v| v.as_str()) { a.name = v.to_string(); }
      if let Some(v) = args.get("objective").and_then(|v| v.as_str()) { a.objective = v.to_string(); }
      if let Some(v) = args.get("model").and_then(|v| v.as_str()) {
        let v = crate::provider::normalize_model_id(&conn, v);
        if !crate::provider::model_provider_known(&conn, &v) {
          return Err(format!("Unknown model '{v}'. Pass a configured model id from list_models."));
        }
        a.model = v;
      }
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
        let enabled = conn.query_row("SELECT enabled FROM integration_configs WHERE id=?1", params![d.id], |r| r.get::<_, i64>(0)).map(|v| v != 0).unwrap_or(false);
        let connected = crate::integrations::integration_connected(&conn, &d.id);
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
      // Lead with the id — the model copies the first token back into create_agent,
      // so the display label must not be the first thing it sees.
      let rows = stmt.query_map([], |row| Ok(format!("- {} — {} ({})", row.get::<_, String>(0)?, row.get::<_, String>(2)?, row.get::<_, String>(1)?))).map_err(|e| e.to_string())?;
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
    "create_tool" => {
      let name = args.get("name").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
      let description = args.get("description").and_then(|v| v.as_str()).unwrap_or("").to_string();
      if name.is_empty() { return Err("create_tool requires a name.".into()); }
      let Some(config) = args.get("config").filter(|c| c.is_object()) else { return Err("create_tool requires a config object.".into()) };
      let config = model_tool_config(&name, config);
      let id = format!("tool-{}", chrono::Utc::now().timestamp_millis());
      // Build it exactly the way the runtime will, so a definition that yields no
      // callable tool is rejected here rather than saved and inert.
      let mut used = std::collections::HashSet::new();
      let built = crate::agents::api_tools_from_config(&id, &config, &mut used);
      // A single endpoint with no url builds a tool that cannot be called, so the
      // build alone is not enough to call it valid.
      let suite = config.get("actions").and_then(|a| a.as_array()).is_some();
      let has_url = config.get("url").and_then(|u| u.as_str()).map(|u| !u.trim().is_empty()).unwrap_or(false);
      if built.is_empty() || (!suite && !has_url) {
        return Err("That config is not callable. A single endpoint needs a `url`; a suite needs a `baseUrl` and at least one action with a `name`.".into());
      }
      let callable: Vec<String> = built.iter().map(|t| t.name()).collect();
      crate::storage::save_tool(app.clone(), ToolRecord {
        id: id.clone(), name: name.clone(), kind: "api".into(), integration_id: "http".into(),
        description: if description.is_empty() { None } else { Some(description) },
        enabled: true, config: config.clone(),
      })?;
      // Attach in the same call: update_agent replaces toolIds wholesale, and
      // nothing lets the model read an agent's current list back to append to it.
      let mut attached = String::new();
      if let Some(agent_id) = args.get("agentId").and_then(|v| v.as_str()).map(str::trim).filter(|s| !s.is_empty()) {
        let current: Option<String> = conn.query_row("SELECT tool_ids FROM agents WHERE id=?1", params![agent_id], |r| r.get(0)).ok();
        let Some(json) = current else {
          return Err(format!("Created tool '{name}' ({id}), but there is no agent '{agent_id}'. Attach it from an agent's config."));
        };
        let mut ids = parse_json_vec(&json);
        if !ids.contains(&id) { ids.push(id.clone()); }
        conn.execute("UPDATE agents SET tool_ids=?1 WHERE id=?2", params![serde_json::to_string(&ids).unwrap_or_else(|_| "[]".into()), agent_id]).map_err(|e| e.to_string())?;
        attached = format!(" and attached it to {agent_id}");
      }
      let credential = if config.get("auth").is_some() {
        " It declares auth, so tell the user to add its key in the workshop — its calls fail with 'No credential is stored' until they do."
      } else { "" };
      Ok(format!("Created tool '{name}' (id: {id}){attached}. Callable as: {}.{credential}", callable.join(", ")))
    }
    "get_workspace_status" => {
      let agents_n: i64 = conn.query_row("SELECT COUNT(*) FROM agents WHERE is_manager=0", [], |r| r.get(0)).unwrap_or(0);
      let tasks_n: i64 = conn.query_row("SELECT COUNT(*) FROM tasks WHERE status IN ('pending','running')", [], |r| r.get(0)).unwrap_or(0);
      let runs_n: i64 = conn.query_row("SELECT COUNT(*) FROM runs", [], |r| r.get(0)).unwrap_or(0);
      let conn_n = integration_definitions().iter().filter(|d| crate::integrations::integration_connected(&conn, &d.id)).count();
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
    // One shared implementation: the Manager's built-in tool and the
    // knowledge_base tool every other agent holds both route here.
    "knowledge_base" => crate::knowledge::run(&conn, args),
    // Read-only: the user approves the moves in the Knowledge tab.
    "tidy_knowledge" => {
      let proposals = crate::refile::preview(app).await?;
      if proposals.is_empty() { return Ok("Knowledge base looks clean — nothing to move.".into()); }
      Ok(proposals.iter()
        .map(|p| format!("- \"{}\" → {} as a {}", p.title, p.destination, p.kind))
        .collect::<Vec<_>>().join("\n"))
    }
    "search_files" => SearchFilesTool.run(args).await,
    "read_file_any" => ReadAnyFileTool.run(args).await,
    "run_command" => RunCommandTool.run(args).await,
    "ask_user" => {
      if crate::app_settings::autonomy(app) {
        // No one is at the keyboard — don't stall the turn waiting for an answer.
        Ok("Autonomy is on and no one is at the keyboard. Use your best judgment and continue without asking.".into())
      } else {
        let question = args.get("question").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let options: Vec<String> = args.get("options").and_then(|v| v.as_array()).map(|a| a.iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect()).unwrap_or_default();
        match crate::telegram::active_chat() {
          Some((bot_id, chat_id)) => {
            if options.is_empty() {
              crate::telegram::ask_text(app, &bot_id, chat_id, &question, 300).await
            } else {
              crate::telegram::ask_choice(app, &bot_id, chat_id, &question, &options, 300).await
            }
          }
          None => Ok("ask_user works from a Telegram chat; there isn't one in this turn.".into()),
        }
      }
    }
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
    | "create_tool"
    | "run_command" | "run_script"
    // PC control: the coarse/system actions confirm; low-level input and
    // read-only actions do not (a vision loop needs many of them).
    | "launch_app" | "close_window" | "lock_screen" | "write_clipboard" | "open_path"
    // Browser control: acting on a page confirms (only when autonomy is off);
    // reading and pure navigation history do not.
    | "browser_click" | "browser_type")
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

#[cfg(test)]
mod model_tool_tests {
  use super::*;

  #[test]
  fn a_model_proposed_credential_is_never_written() {
    let config = serde_json::json!({
      "name": "Notion API",
      "baseUrl": "https://api.notion.com/v1",
      "auth": { "type": "bearer", "value": "ntn_secret123" },
      "headers": [
        { "name": "Notion-Version", "value": "2022-06-28" },
        { "name": "X-API-Key", "value": "sk-live-123" }
      ],
      "actions": [{ "name": "search", "method": "POST", "path": "/search" }]
    });
    let cleaned = model_tool_config("Notion API", &config);
    // The auth shape survives so the tool fails with a clear message; the value does not.
    assert_eq!(cleaned["auth"]["type"], "bearer");
    assert_eq!(cleaned["auth"]["value"], "");
    // The harmless header survives; the credential-bearing one is dropped.
    let kept: Vec<&str> = cleaned["headers"].as_array().unwrap().iter().filter_map(|h| h["name"].as_str()).collect();
    assert_eq!(kept, vec!["Notion-Version"]);
    // The display name becomes the identifier the actions are prefixed with.
    assert_eq!(cleaned["name"], "notion_api");
  }

  #[test]
  fn a_cleaned_config_still_builds_into_a_callable_tool() {
    let config = serde_json::json!({
      "name": "notion",
      "baseUrl": "https://api.notion.com/v1",
      "auth": { "type": "bearer", "value": "leak" },
      "actions": [
        { "name": "search", "method": "POST", "path": "/search" },
        { "name": "get page", "method": "GET", "path": "/pages/{pageId}" }
      ]
    });
    let cleaned = model_tool_config("Notion", &config);
    let mut used = std::collections::HashSet::new();
    let names: Vec<String> = crate::agents::api_tools_from_config("tool-1", &cleaned, &mut used).iter().map(|t| t.name()).collect();
    assert_eq!(names, vec!["notion_search", "notion_get_page"]);
  }

  #[test]
  fn an_endpoint_with_no_url_is_not_callable() {
    // It still builds, which is why the dispatch arm checks the url as well as the
    // build: a tool with nothing to call must not be saved.
    let cleaned = model_tool_config("Empty", &serde_json::json!({ "method": "GET" }));
    let mut used = std::collections::HashSet::new();
    assert_eq!(crate::agents::api_tools_from_config("tool-1", &cleaned, &mut used).len(), 1);
    let has_url = cleaned.get("url").and_then(|u| u.as_str()).map(|u| !u.trim().is_empty()).unwrap_or(false);
    assert!(!has_url);
  }

  #[test]
  fn a_suite_whose_actions_are_all_unnamed_builds_nothing() {
    let cleaned = model_tool_config("NoActions", &serde_json::json!({ "baseUrl": "https://x.dev", "actions": [{ "name": "  " }] }));
    let mut used = std::collections::HashSet::new();
    assert!(crate::agents::api_tools_from_config("tool-2", &cleaned, &mut used).is_empty());
  }
}
