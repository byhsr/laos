// Agent execution: assembles an agent's toolset, runs it against its provider
// (Ollama / OpenRouter / Groq, with tool-calling), and records runs.

use rusqlite::{params, Connection};
use std::fs;
use tauri::{AppHandle, Manager};

use crate::db::{db, now};
use crate::http;
use crate::integrations::integration_definitions;
use crate::mcp;
use crate::models::*;
use crate::provider;
use crate::tasks::list_tasks;
use crate::tools::{
  browser_tools, desktop_tools, sanitize, script_tools, unique_name, AgentTool, ApiAuth, ApiParam, ApiTool, Approval,
  HttpTool, IntegrationTool, McpTool, ParamIn, ReadAnyFileTool, ReadFileTool, RunCommandTool, SearchFilesTool,
  WriteFileTool, MAX_TOOL_ROUNDS,
};
use crate::tooltext::parse_text_tool_calls;

/// Action tools for the integrations an agent explicitly holds. Attaching the
/// integration is the opt-in, so no separate `network` permission is required
/// (the same reasoning the MCP arm uses). A saved credential counts as connected
/// — the `connected` column only tracks OAuth handshakes — and the secrets are
/// resolved from the vault so the tool sends the real key, not the `__vault__`
/// sentinel.
pub(crate) fn integration_tools(app: &AppHandle, conn: &Connection, ids: &[String]) -> Vec<Box<dyn AgentTool>> {
  let mut tools: Vec<Box<dyn AgentTool>> = Vec::new();
  for integration_id in ids {
    let Ok(mut stmt) = conn.prepare("SELECT enabled, connected, config_json FROM integration_configs WHERE id=?1") else { continue };
    let Ok(mut rows) = stmt.query_map(params![integration_id], |row| Ok((row.get::<_, i64>(0)? != 0, row.get::<_, i64>(1)? != 0, row.get::<_, String>(2)?))) else { continue };
    let Some(Ok((enabled, connected, cfg_json))) = rows.next() else { continue };
    if !enabled { continue; }
    let stored: serde_json::Value = serde_json::from_str(&cfg_json).unwrap_or_else(|_| serde_json::json!({}));
    if !connected && !crate::integrations::config_has_credential(&stored) { continue; }
    let Some(def) = integration_definitions().into_iter().find(|d| d.id == *integration_id) else { continue };
    let credentials = crate::integrations::integration_secret(conn, integration_id).unwrap_or(stored);
    for action in def.actions {
      tools.push(Box::new(IntegrationTool { action: action.name, credentials: credentials.clone(), id: def.id.clone(), app: Some(app.clone()) }));
    }
  }
  tools
}

pub(crate) fn build_tools(app: &AppHandle, conn: &Connection, agent: &AgentRequest, home: &std::path::Path) -> Result<Vec<Box<dyn AgentTool>>, String> {
  let mut tools: Vec<Box<dyn AgentTool>> = Vec::new();
  let has_network = agent.permissions.iter().any(|p| p == "network");
  let has_files = agent.permissions.iter().any(|p| p == "files");
  let has_host_fs = agent.permissions.iter().any(|p| p == "host_fs");
  let has_pc = agent.permissions.iter().any(|p| p == "pc_control");
  let mut stmt = conn.prepare("SELECT kind, enabled, config_json FROM tools WHERE id=?1").map_err(|e| e.to_string())?;
  // Model-facing names already claimed, so two servers exposing the same tool
  // (e.g. two Notion accounts) get distinct names instead of silently colliding.
  let mut used: std::collections::HashSet<String> = std::collections::HashSet::new();
  for tool_id in &agent.tool_ids {
    let mut rows = stmt.query_map(params![tool_id], |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? != 0, row.get::<_, String>(2)?))).map_err(|e| e.to_string())?;
    let Some(Ok((kind, enabled, config_json))) = rows.next() else { continue }; // unknown or missing â†’ skip
    if !enabled { continue; }
    let config: serde_json::Value = serde_json::from_str(&config_json).unwrap_or_else(|_| serde_json::json!({}));
    match kind.as_str() {
      "http_get" if has_network => tools.push(Box::new(HttpTool)),
      "api" if has_network => tools.extend(api_tools_from_config(tool_id, &config, &mut used)),
      "read_file" if has_files => tools.push(Box::new(ReadFileTool { home: home.to_path_buf() })),
      "write_file" if has_files => tools.push(Box::new(WriteFileTool { home: home.to_path_buf() })),
      // Browser control: attaching the `browser` tool opts the agent in, exactly
      // like the `mcp` arm below.
      "browser" => tools.extend(browser_tools()),
      // MCP tools spawn the configured server's command. A user who attached one
      // has already opted into the capability — the old `network` gate silently
      // dropped them, which is why MCPs appeared unusable in agents.
      "mcp" => {
        if let Some(mut tool) = mcp_tool_from_config(conn, &config) {
          let name = unique_name(tool.name(), &used);
          if name != tool.name() { tool.alias = Some(name); }
          tools.push(Box::new(tool));
        }
      }
      _ => {}
    }
    if let Some(last) = tools.last() { used.insert(last.name()); }
  }
  // Integration actions: granted to an agent that explicitly holds the
  // integration (that grant is the opt-in, so no separate `network` gate).
  tools.extend(integration_tools(app, conn, &agent.integrations));
  // Host-filesystem tools: granted only to agents with the explicit host_fs permission.
  if has_host_fs {
    tools.push(Box::new(SearchFilesTool));
    tools.push(Box::new(ReadAnyFileTool));
    tools.push(Box::new(RunCommandTool));
    tools.extend(script_tools(conn));
    // Monid's catalog is reached through its CLI, so it rides on host_fs.
    tools.push(Box::new(crate::tools::MonidTool));
    // yt-dlp downloads do their work through its CLI too, so it rides on host_fs as well.
    tools.push(Box::new(crate::tools::YtDlpTool));
  }
  // PC control tools: granted only to agents with the explicit pc_control permission.
  if has_pc {
    tools.extend(desktop_tools());
  }
  // Every agent gets the shared knowledge base (one store any agent can read and
  // write), the remember tool (an explicit "remember this" becomes durable memory
  // in the agent's own scope) and the items tool (todos/reminders — the store the
  // knowledge base was standing in for). The scratchpad is the agent's own pad for
  // the chat, so its working notes never land in the user's items.
  tools.push(crate::tools::knowledge_tool(app));
  tools.push(crate::tools::remember_tool(app, &agent.id));
  tools.push(crate::tools::items_tool(app, &agent.id));
  tools.push(crate::tools::scratchpad_tool(app));
  // And the document tool: long-form writing the user can read and download.
  tools.push(crate::tools::document_tool(app));
  // And the clock, so a turn that depends on "now" doesn't have to guess the date.
  tools.push(crate::tools::current_time_tool(app));
  Ok(tools)
}

// Builds the runtime tools for one `api` tool row.
//
// A row carrying `actions` is an API *suite*: a base URL, shared headers and auth,
// and one callable tool per action — which is how a user-defined tool stands in
// for what used to require a native integration. A row without `actions` is the
// original single-endpoint tool, and behaves exactly as it always has.
pub(crate) fn api_tools_from_config(tool_id: &str, config: &serde_json::Value, used: &mut std::collections::HashSet<String>) -> Vec<Box<dyn AgentTool>> {
  let str_cfg = |k: &str| config.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string();
  let headers = api_headers(config, tool_id);
  let auth = api_auth(config, tool_id);
  let approval = Approval::parse(&str_cfg("approval"));
  let label = str_cfg("name");
  let slug = if label.is_empty() { format!("api_{tool_id}") } else { label };

  if let Some(actions) = config.get("actions").and_then(|a| a.as_array()) {
    let base = str_cfg("baseUrl");
    // The model-facing name must survive the provider's [a-zA-Z0-9_-] rule, so the
    // display name the user typed is sanitised rather than trusted.
    let safe_slug = sanitize(&slug);
    return actions.iter().filter_map(|a| {
      let raw = a.get("name")?.as_str()?.trim();
      if raw.is_empty() { return None; }
      // Unique across every tool an agent holds, so two suites exposing the same
      // action name (two Notion accounts) can't silently collide.
      let candidate: String = format!("{safe_slug}_{}", sanitize(raw)).chars().take(64).collect();
      let name = unique_name(candidate, used);
      used.insert(name.clone());
      Some(Box::new(ApiTool {
        name,
        description: a.get("description").and_then(|d| d.as_str()).unwrap_or("").to_string(),
        method: a.get("method").and_then(|m| m.as_str()).unwrap_or("GET").to_uppercase(),
        url_template: format!("{base}{}", a.get("path").and_then(|p| p.as_str()).unwrap_or("")),
        headers: headers.clone(),
        body_template: a.get("body").and_then(|b| b.as_str()).filter(|b| !b.trim().is_empty()).map(str::to_string),
        params: api_params(a),
        auth: auth.clone(),
        approval,
      }) as Box<dyn AgentTool>)
    }).collect();
  }

  vec![Box::new(ApiTool {
    name: slug,
    description: str_cfg("description"),
    method: str_cfg("method").to_uppercase(),
    url_template: str_cfg("url"),
    headers,
    body_template: if str_cfg("body").is_empty() { None } else { Some(str_cfg("body")) },
    params: api_params(config),
    auth,
    approval,
  })]
}

// The `api` tool rows an agent holds. The Manager does not go through
// `build_tools`, so without this an API tool attached to the lead agent was
// silently dead — the row existed and nothing callable came of it.
pub(crate) fn custom_tools_for(conn: &Connection, tool_ids: &[String], has_network: bool) -> Vec<Box<dyn AgentTool>> {
  let mut out: Vec<Box<dyn AgentTool>> = Vec::new();
  if !has_network { return out; }
  let mut used: std::collections::HashSet<String> = std::collections::HashSet::new();
  let Ok(mut stmt) = conn.prepare("SELECT config_json FROM tools WHERE id=?1 AND kind='api' AND enabled=1") else { return out };
  for id in tool_ids {
    let Ok(mut rows) = stmt.query_map(params![id], |row| row.get::<_, String>(0)) else { continue };
    let Some(Ok(config_json)) = rows.next() else { continue };
    let config: serde_json::Value = serde_json::from_str(&config_json).unwrap_or_else(|_| serde_json::json!({}));
    out.extend(api_tools_from_config(id, &config, &mut used));
  }
  out
}

// Static headers for a tool. A secret header stores only the sentinel; resolve it
// here, and drop the header if the credential store no longer holds it.
fn api_headers(config: &serde_json::Value, tool_id: &str) -> Vec<(String, String)> {
  config.get("headers").and_then(|h| h.as_array()).map(|arr| arr.iter().filter_map(|h| {
    let name = h.get("name")?.as_str()?.to_string();
    let value = h.get("value")?.as_str()?.to_string();
    // The sentinel when the config came from the store, the mask when it came back
    // through the UI: both mean "look the real secret up".
    let value = if crate::vault::is_ref(&value) || value == crate::vault::MASK {
      crate::vault::get(&format!("tool/{tool_id}/{name}"))?
    } else { value };
    Some((name, value))
  }).collect::<Vec<_>>()).unwrap_or_default()
}

fn api_params(config: &serde_json::Value) -> Vec<ApiParam> {
  config.get("params").and_then(|p| p.as_array()).map(|arr| arr.iter().filter_map(|p| {
    Some(ApiParam {
      name: p.get("name")?.as_str()?.to_string(),
      param_type: p.get("type").and_then(|t| t.as_str()).unwrap_or("string").to_string(),
      description: p.get("description").and_then(|d| d.as_str()).unwrap_or("").to_string(),
      required: p.get("required").and_then(|r| r.as_bool()).unwrap_or(false),
      // An absent `in` is the legacy behaviour: a raw {name} substitution.
      in_: ParamIn::parse(p.get("in").and_then(|i| i.as_str()).unwrap_or("")),
    })
  }).collect::<Vec<_>>()).unwrap_or_default()
}

// A tool's auth, with the secret resolved from the credential store. A credential
// that is configured but no longer stored becomes `Missing`, so the call fails
// with something the user can act on rather than being sent unauthenticated.
fn api_auth(config: &serde_json::Value, tool_id: &str) -> ApiAuth {
  let Some(auth) = config.get("auth") else { return ApiAuth::None };
  let kind = auth.get("type").and_then(|t| t.as_str()).unwrap_or("none");
  if kind.is_empty() || kind == "none" { return ApiAuth::None; }
  let raw = auth.get("value").and_then(|v| v.as_str()).unwrap_or("").to_string();
  // The sentinel when the config came from the store, the mask when it came back
  // through the UI: both mean "look the real secret up".
  let secret = if crate::vault::is_ref(&raw) || raw == crate::vault::MASK {
    crate::vault::get(&format!("tool/{tool_id}/auth"))
  } else if raw.trim().is_empty() {
    None
  } else {
    Some(raw)
  };
  let Some(secret) = secret else {
    let label = config.get("name").and_then(|v| v.as_str()).filter(|n| !n.trim().is_empty()).unwrap_or(tool_id);
    return ApiAuth::Missing(format!("the {label} tool"));
  };
  let named = |fallback: &str| auth.get("name").and_then(|n| n.as_str()).filter(|n| !n.trim().is_empty()).unwrap_or(fallback).to_string();
  match kind {
    "bearer" => ApiAuth::Bearer(secret),
    "header" => ApiAuth::Header(named("x-api-key"), secret),
    "query" => ApiAuth::Query(named("api_key"), secret),
    _ => ApiAuth::None,
  }
}

#[cfg(test)]
mod api_tool_tests {
  use super::*;

  fn fresh() -> std::collections::HashSet<String> { std::collections::HashSet::new() }

  #[test]
  fn a_legacy_row_builds_one_tool_as_before() {
    let config = serde_json::json!({
      "name": "gh", "description": "GitHub", "method": "get",
      "url": "https://api.github.com/users/{user}",
      "headers": [{ "name": "Accept", "value": "application/vnd.github+json" }],
      "params": [{ "name": "user", "type": "string", "description": "who", "required": true }]
    });
    let tools = api_tools_from_config("tool-1", &config, &mut fresh());
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0].name(), "gh");
    assert_eq!(tools[0].description(), "GitHub");
    // The legacy shape has no approval setting, so nothing new starts asking.
    assert!(!tools[0].requires_approval());
  }

  #[test]
  fn a_suite_expands_into_one_tool_per_action() {
    let config = serde_json::json!({
      "name": "notion", "baseUrl": "https://api.notion.com/v1", "approval": "writes",
      "auth": { "type": "bearer", "value": "plaintext-for-test" },
      "actions": [
        { "name": "search", "description": "Search pages", "method": "POST", "path": "/search" },
        { "name": "get_page", "method": "GET", "path": "/pages/{pageId}",
          "params": [{ "name": "pageId", "in": "path", "required": true }] }
      ]
    });
    let tools = api_tools_from_config("tool-1", &config, &mut fresh());
    let names: Vec<String> = tools.iter().map(|t| t.name()).collect();
    assert_eq!(names, vec!["notion_search", "notion_get_page"]);
    // `writes` asks for the POST and lets the GET run.
    assert!(tools[0].requires_approval());
    assert!(!tools[1].requires_approval());
    assert!(tools[1].is_long_running());
  }

  #[test]
  fn suite_names_do_not_collide_across_tools() {
    let config = serde_json::json!({ "name": "notion", "baseUrl": "https://x.dev", "actions": [{ "name": "search" }] });
    let mut used = fresh();
    let first = api_tools_from_config("tool-1", &config, &mut used);
    let second = api_tools_from_config("tool-2", &config, &mut used);
    assert_eq!(first[0].name(), "notion_search");
    assert_eq!(second[0].name(), "notion_search_2");
  }

  #[test]
  fn a_credential_that_vanished_is_reported() {
    let configured = serde_json::json!({ "name": "notion", "auth": { "type": "bearer", "value": crate::vault::REF } });
    assert!(matches!(api_auth(&configured, "tool-nothing-stored"), ApiAuth::Missing(_)));
    // An unconfigured or explicitly disabled auth is not an error.
    assert!(matches!(api_auth(&serde_json::json!({}), "t"), ApiAuth::None));
    assert!(matches!(api_auth(&serde_json::json!({ "auth": { "type": "none" } }), "t"), ApiAuth::None));
  }
}

// Builds one MCP tool from a registry row's config. None when the row is missing
// its server/tool name or the server has since been deleted.
fn mcp_tool_from_config(conn: &Connection, config: &serde_json::Value) -> Option<McpTool> {
  let server_id = config.get("serverId").and_then(|v| v.as_str()).unwrap_or("");
  let tool_name = config.get("toolName").and_then(|v| v.as_str()).unwrap_or("");
  if server_id.is_empty() || tool_name.is_empty() { return None; }
  let server = mcp::load_server(conn, server_id).ok()?;
  Some(McpTool {
    server,
    tool_name: tool_name.to_string(),
    tool_description: config.get("description").and_then(|v| v.as_str()).unwrap_or("").to_string(),
    schema: config.get("schema").cloned().unwrap_or_else(|| serde_json::json!({ "type": "object", "properties": {} })),
    alias: None,
  })
}

// The MCP tools an agent explicitly attached, resolved from its tool ids. The
// Manager uses this so it holds only the MCP tools it was opted into — not every
// enabled server's tools, which bloated every tool round with dozens of unused
// schemas and was the main token cost.
pub(crate) fn mcp_tools_for(conn: &Connection, tool_ids: &[String]) -> Vec<Box<dyn AgentTool>> {
  let mut out: Vec<Box<dyn AgentTool>> = Vec::new();
  let mut used: std::collections::HashSet<String> = std::collections::HashSet::new();
  let Ok(mut stmt) = conn.prepare("SELECT config_json FROM tools WHERE id=?1 AND kind='mcp' AND enabled=1") else { return out };
  for id in tool_ids {
    if let Ok(config_json) = stmt.query_row(params![id], |row| row.get::<_, String>(0)) {
      if let Ok(value) = serde_json::from_str::<serde_json::Value>(&config_json) {
        if let Some(mut tool) = mcp_tool_from_config(conn, &value) {
          let name = unique_name(tool.name(), &used);
          if name != tool.name() { tool.alias = Some(name.clone()); }
          used.insert(name);
          out.push(Box::new(tool));
        }
      }
    }
  }
  out
}

pub(crate) fn tool_schemas(tools: &[Box<dyn AgentTool>]) -> Vec<serde_json::Value> {
  tools.iter().map(|t| serde_json::json!({ "type": "function", "function": { "name": t.name(), "description": t.description(), "parameters": t.params_schema() } })).collect()
}

// Renders an agent's attached skills (instruction packs) into prompt text.
// A skill carries instructions (`content`), an optional reference (`docs`) and
// references to saved scripts it can run. Empty when the agent has none, so
// unaffected agents keep their exact prompt.
pub(crate) fn skills_prompt(conn: &Connection, skill_ids: &[String]) -> String {
  if skill_ids.is_empty() { return String::new(); }
  let mut stmt = match conn.prepare("SELECT name, description, content, docs, script_ids FROM skills WHERE id=?1") {
    Ok(s) => s,
    Err(_) => return String::new(),
  };
  let mut out = String::new();
  for id in skill_ids {
    let row = stmt.query_row(params![id], |r| Ok((
      r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?,
      r.get::<_, String>(3)?, r.get::<_, String>(4)?,
    )));
    let Ok((name, description, content, docs, script_ids_json)) = row else { continue };
    if out.is_empty() { out.push_str("\n## Skills\n"); }
    out.push_str(&format!("\n### {name}\n"));
    if !description.trim().is_empty() { out.push_str(&format!("When to use: {}\n", description.trim())); }
    if !content.trim().is_empty() { out.push_str(content.trim()); out.push('\n'); }
    if !docs.trim().is_empty() { out.push_str("\nReference:\n"); out.push_str(docs.trim()); out.push('\n'); }
    let script_ids: Vec<String> = serde_json::from_str(&script_ids_json).unwrap_or_default();
    let scripts: Vec<String> = script_ids.iter()
      .filter_map(|sid| crate::storage::load_script(conn, sid).map(|s| format!("- {}: {}", s.name, s.description)))
      .collect();
    if !scripts.is_empty() {
      out.push_str("Scripts for this skill (run them with the run_script tool, by name):\n");
      out.push_str(&scripts.join("\n"));
      out.push('\n');
    }
  }
  out
}

// Returns (final_text, events, used_tools, prompt_tokens, completion_tokens).
async fn run_ollama_chat(resolved: &provider::Resolved, prompt: &str, agent: &AgentRequest, tools: &[Box<dyn AgentTool>]) -> Result<(String, Vec<ExecutionEvent>, bool, u64, u64), String> {
  let mut events = Vec::new();
  let mut messages = vec![
    ChatMessage { role: "system".into(), content: Some(format!("You are {}. Objective: {}\n\nUse tools when you need current or external information. Call a tool, wait for its result, then continue. Cite URLs you use.", agent.name, agent.objective)), tool_calls: None, tool_call_id: None },
    ChatMessage { role: "user".into(), content: Some(prompt.into()), tool_calls: None, tool_call_id: None },
  ];
  let client = http::stream_client();
  let mut used_tools = false;
  let mut prompt_tokens = 0u64;
  let mut completion_tokens = 0u64;
  for _round in 0..MAX_TOOL_ROUNDS {
    // Built as JSON rather than the typed ChatRequest so the provider layer can
    // drop reasoning parameters in one place if Ollama rejects them.
    let mut body = serde_json::json!({ "model": resolved.model, "messages": messages, "tools": tool_schemas(tools), "stream": false });
    let budget = provider::apply_reasoning(&mut body, resolved, http::CHAT_MAX_TOKENS);
    provider::apply_ollama_options(&mut body, budget);
    let response = http::send_model_request(&client, resolved, &resolved.chat_url(), &body, http::MODEL_ATTEMPTS)
      .await
      .map_err(|e| format!("Could not reach Ollama. Start it at {} ({e})", resolved.base))?;
    let parsed: ChatResponse = response.json().await.map_err(|e| format!("Could not parse Ollama tool response: {e}"))?;
    prompt_tokens += parsed.prompt_eval_count;
    completion_tokens += parsed.eval_count;
    let reply = parsed.message;
    if let Some(calls) = &reply.tool_calls {
      used_tools = true;
      for call in calls {
        let name = call.function.name.clone();
        let args = call.function.arguments.clone();
        let tool = tools.iter().find(|t| t.name() == name);
        let result = match tool {
          Some(t) => t.run(&args).await,
          None => Err(format!("Unknown tool {name}.")),
        };
        let detail = String::new();
        let (title, result_str) = match &result {
          Ok(text) => (format!("Called {name}"), format!("{text}")),
          Err(e) => (format!("Tool {name} failed"), format!("Error: {e}")),
        };
        events.push(ExecutionEvent { time: now(), kind: "tool".into(), title, detail: if detail.is_empty() { None } else { Some(detail) } });
        messages.push(ChatMessage { role: "assistant".into(), content: None, tool_calls: Some(vec![call.clone()]), tool_call_id: None });
        messages.push(ChatMessage { role: "tool".into(), content: Some(result_str), tool_calls: None, tool_call_id: Some(call.id.clone()) });
      }
    } else {
      // Text-protocol fallback: some models emit the call in `content` using
      // special-token markup instead of `tool_calls` (see tooltext.rs).
      let content = reply.content.clone().unwrap_or_default();
      let (cleaned, text_calls) = parse_text_tool_calls(&content);
      if text_calls.is_empty() {
        return Ok((cleaned, events, used_tools, prompt_tokens, completion_tokens));
      }
      used_tools = true;
      messages.push(ChatMessage { role: "assistant".into(), content: Some(content), tool_calls: None, tool_call_id: None });
      for call in text_calls {
        let result = match tools.iter().find(|t| t.name() == call.name) {
          Some(t) => t.run(&call.args).await,
          None => Err(format!("Unknown tool {}.", call.name)),
        };
        let (title, result_str) = match &result {
          Ok(text) => (format!("Called {}", call.name), format!("{text}")),
          Err(e) => (format!("Tool {} failed", call.name), format!("Error: {e}")),
        };
        events.push(ExecutionEvent { time: now(), kind: "tool".into(), title, detail: None });
        messages.push(ChatMessage { role: "user".into(), content: Some(format!("<tool_result name=\"{}\">\n{}\n</tool_result>", call.name, result_str)), tool_calls: None, tool_call_id: None });
      }
    }
  }
  Err("Tool loop exceeded the maximum number of rounds.".into())
}

// Provider-agnostic tool-calling loop for the OpenAI-compatible endpoints
// (OpenRouter / Groq). Mirrors run_ollama_chat but speaks the OpenAI shape:
// arguments arrive as a JSON string, and each tool result must carry its call id.
async fn run_openai_tool_chat(resolved: &provider::Resolved, prompt: &str, agent: &AgentRequest, tools: &[Box<dyn AgentTool>]) -> Result<(String, Vec<ExecutionEvent>, bool, u64, u64), String> {
  let mut events = Vec::new();
  let mut messages = vec![
    serde_json::json!({ "role": "system", "content": format!("You are {}. Objective: {}\n\nUse tools when you need current or external information. Call a tool, wait for its result, then continue. Cite URLs you use.", agent.name, agent.objective) }),
    serde_json::json!({ "role": "user", "content": prompt }),
  ];
  let client = http::client();
  let mut used_tools = false;
  let mut prompt_tokens = 0u64;
  let mut completion_tokens = 0u64;
  for _round in 0..MAX_TOOL_ROUNDS {
    let mut body = serde_json::json!({ "model": resolved.model, "messages": messages, "tools": tool_schemas(tools), "stream": false });
    let budget = provider::apply_reasoning(&mut body, resolved, http::CHAT_MAX_TOKENS);
    http::apply_openai_defaults(&mut body, resolved.provider_routing(), budget);
    if resolved.is_anthropic() { body = crate::anthropic::to_wire(&body); }
    let response = http::send_model_request(&client, resolved, &resolved.chat_url(), &body, http::MODEL_ATTEMPTS).await?;
    let mut parsed: serde_json::Value = response.json().await.map_err(|e| format!("Could not parse the tool response: {e}"))?;
    if resolved.is_anthropic() { parsed = crate::anthropic::normalize_response(&parsed); }
    prompt_tokens += parsed["usage"]["prompt_tokens"].as_u64().unwrap_or(0);
    completion_tokens += parsed["usage"]["completion_tokens"].as_u64().unwrap_or(0);
    let message = parsed["choices"][0]["message"].clone();
    let calls = message["tool_calls"].as_array().cloned().unwrap_or_default();
    if calls.is_empty() {
      let content = message["content"].as_str().unwrap_or("").to_string();
      let (cleaned, text_calls) = parse_text_tool_calls(&content);
      if text_calls.is_empty() {
        return Ok((cleaned, events, used_tools, prompt_tokens, completion_tokens));
      }
      // Text-protocol fallback (see tooltext.rs).
      used_tools = true;
      messages.push(message);
      for call in text_calls {
        let result = match tools.iter().find(|t| t.name() == call.name) {
          Some(t) => t.run(&call.args).await,
          None => Err(format!("Unknown tool {}.", call.name)),
        };
        let (title, result_str) = match &result {
          Ok(text) => (format!("Called {}", call.name), format!("{text}")),
          Err(e) => (format!("Tool {} failed", call.name), format!("Error: {e}")),
        };
        events.push(ExecutionEvent { time: now(), kind: "tool".into(), title, detail: None });
        messages.push(serde_json::json!({ "role": "user", "content": format!("<tool_result name=\"{}\">\n{}\n</tool_result>", call.name, result_str) }));
      }
      continue;
    }
    used_tools = true;
    messages.push(message);
    for call in calls {
      let name = call["function"]["name"].as_str().unwrap_or("").to_string();
      let args = serde_json::from_str(call["function"]["arguments"].as_str().unwrap_or("{}")).unwrap_or_else(|_| serde_json::json!({}));
      let result = match tools.iter().find(|t| t.name() == name) {
        Some(t) => t.run(&args).await,
        None => Err(format!("Unknown tool {name}.")),
      };
      let detail = String::new();
      let (title, result_str) = match &result {
        Ok(text) => (format!("Called {name}"), format!("{text}")),
        Err(e) => (format!("Tool {name} failed"), format!("Error: {e}")),
      };
      events.push(ExecutionEvent { time: now(), kind: "tool".into(), title, detail: if detail.is_empty() { None } else { Some(detail) } });
      messages.push(serde_json::json!({ "role": "tool", "tool_call_id": call["id"].as_str().unwrap_or(""), "content": result_str }));
    }
  }
  Err("Tool loop exceeded the maximum number of rounds.".into())
}

// Runs a single agent against its provider. Returns (output, prompt_tokens, completion_tokens).
pub(crate) async fn run_agent_once(app: &AppHandle, agent: &AgentRequest, input: &str, api_key: Option<&str>, events: &mut Vec<ExecutionEvent>) -> Result<(String, u64, u64), String> {
  run_agent_once_structured(app, agent, input, api_key, events, true).await
}

// `structured` controls whether the prompt forces the summary/result JSON envelope.
pub(crate) async fn run_agent_once_structured(app: &AppHandle, agent: &AgentRequest, input: &str, api_key: Option<&str>, events: &mut Vec<ExecutionEvent>, structured: bool) -> Result<(String, u64, u64), String> {
  let conn = db(app)?;
  let home = app.path().app_data_dir().map_err(|e| e.to_string())?.join("agents").join(&agent.id);
  for folder in ["files", "memory", "runs", "outputs"] { fs::create_dir_all(home.join(folder)).map_err(|e| e.to_string())?; }
  fs::write(home.join("config.json"), serde_json::to_string_pretty(&serde_json::json!({"id":agent.id,"name":agent.name,"objective":agent.objective,"model":agent.model,"tools":agent.tool_ids})).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
  let skills = skills_prompt(&conn, &agent.skill_ids);
  // Pull relevant history for this task when the agent keeps memory, so a
  // one-shot run / task / workflow node / Telegram reply is not context-blind.
  let memory_on: bool = conn.query_row("SELECT memory FROM agents WHERE id=?1", params![agent.id], |r| r.get::<_, i64>(0)).map(|v| v != 0).unwrap_or(false);
  let ctx = if memory_on {
    let scope = crate::agent_memory::scope_key("agent", &agent.id);
    crate::retrieve::context_block(app, &scope, input, 300).await
  } else {
    String::new()
  };
  let prompt = if structured {
    format!("You are {}. Objective: {}\n{skills}{ctx}\nTask: {}\n\nReturn a concise structured result in JSON with keys summary and result.", agent.name, agent.objective, input)
  } else {
    format!("You are {}. Objective: {}\n{skills}{ctx}\nTask: {}\n\nReturn a helpful, direct answer.", agent.name, agent.objective, input)
  };
  let mut resolved = provider::resolve(&conn, &agent.model, api_key)?;
  // The agent's own reasoning wins over the model's default.
  provider::apply_agent_reasoning(&mut resolved, &agent.reasoning);
  let label = resolved.label();
  let tools = build_tools(app, &conn, agent, &home)?;
  events.push(ExecutionEvent {
    time: now(),
    kind: "thought".into(),
    title: format!("Asking {label}"),
    detail: Some(if tools.is_empty() { resolved.model.clone() } else { format!("{} with {} tool(s)", resolved.model, tools.len()) }),
  });

  if !tools.is_empty() {
    let (output, tool_events, _, pt, ct) = if resolved.is_ollama() {
      run_ollama_chat(&resolved, &prompt, agent, &tools).await?
    } else {
      run_openai_tool_chat(&resolved, &prompt, agent, &tools).await?
    };
    events.extend(tool_events);
    return Ok((output, pt, ct));
  }

  // No tools to offer, so this is a single call. Ollama answers those on its
  // dedicated generate endpoint rather than the chat one.
  let (output, pt, ct) = if resolved.is_ollama() {
    let mut body = serde_json::json!({ "model": resolved.model, "prompt": prompt, "stream": false });
    let budget = provider::apply_reasoning(&mut body, &resolved, http::CHAT_MAX_TOKENS);
    provider::apply_ollama_options(&mut body, budget);
    let response = http::send_model_request(&http::stream_client(), &resolved, &resolved.generate_url(), &body, http::MODEL_ATTEMPTS)
      .await
      .map_err(|e| format!("Could not reach Ollama. Start it at {} ({e})", resolved.base))?;
    let json: serde_json::Value = response.json().await.map_err(|e| e.to_string())?;
    (
      json["response"].as_str().unwrap_or("No response from Ollama.").to_string(),
      json["prompt_eval_count"].as_u64().unwrap_or(0),
      json["eval_count"].as_u64().unwrap_or(0),
    )
  } else {
    let mut body = serde_json::json!({ "model": resolved.model, "messages": [{ "role": "user", "content": prompt }] });
    let budget = provider::apply_reasoning(&mut body, &resolved, http::CHAT_MAX_TOKENS);
    http::apply_openai_defaults(&mut body, resolved.provider_routing(), budget);
    if resolved.is_anthropic() { body = crate::anthropic::to_wire(&body); }
    let response = http::send_model_request(&http::client(), &resolved, &resolved.chat_url(), &body, http::MODEL_ATTEMPTS).await?;
    let mut json: serde_json::Value = response.json().await.map_err(|e| e.to_string())?;
    if resolved.is_anthropic() { json = crate::anthropic::normalize_response(&json); }
    (
      json["choices"][0]["message"]["content"].as_str().unwrap_or("The provider returned no text.").to_string(),
      json["usage"]["prompt_tokens"].as_u64().unwrap_or(0),
      json["usage"]["completion_tokens"].as_u64().unwrap_or(0),
    )
  };
  events.push(ExecutionEvent { time: now(), kind: "result".into(), title: "Generated final result".into(), detail: None });
  Ok((output, pt, ct))
}

// Builds the Manager's workspace context: agents, integrations, active tasks.
pub(crate) fn build_workspace_context(conn: &Connection) -> Result<String, String> {
  let mut agents = String::new();
  {
    let mut stmt = conn.prepare("SELECT id, name, description, objective, model, integrations, permissions FROM agents WHERE is_manager=0 ORDER BY name").map_err(|e| e.to_string())?;
    let rows = stmt.query_map([], |row| {
      let integrations: String = row.get(5)?;
      let permissions: String = row.get(6)?;
      Ok(format!("- {} (id: {}): {}. Model: {}. Integrations: {}. Permissions: {}.",
        row.get::<_, String>(1)?, row.get::<_, String>(0)?,
        row.get::<_, String>(2)?, row.get::<_, String>(3)?,
        integrations, permissions))
    }).map_err(|e| e.to_string())?;
    for row in rows { agents.push_str(&row.map_err(|e| e.to_string())?); agents.push('\n'); }
  }
  let mut ints = String::new();
  {
    let defs = integration_definitions();
    for d in defs {
      let mut stmt = conn.prepare("SELECT enabled, connected, config_json FROM integration_configs WHERE id=?1").map_err(|e| e.to_string())?;
      let mut rows = stmt.query_map(params![d.id], |row| Ok((row.get::<_, i64>(0)? != 0, row.get::<_, i64>(1)? != 0, row.get::<_, String>(2)?))).map_err(|e| e.to_string())?;
      let (enabled, connected) = match rows.next().transpose().map_err(|e| e.to_string())? {
        Some((enabled, connected, cfg)) => {
          let parsed: serde_json::Value = serde_json::from_str(&cfg).unwrap_or_else(|_| serde_json::json!({}));
          (enabled, connected || crate::integrations::config_has_credential(&parsed))
        }
        None => (false, false),
      };
      let actions = d.actions.iter().map(|a| a.name.clone()).collect::<Vec<_>>().join(", ");
      ints.push_str(&format!("- {} (id: {}): enabled={} connected={}. Actions: {}\n", d.name, d.id, enabled, connected, actions));
    }
  }
  let tasks = list_tasks(conn)?;
  let active: Vec<String> = tasks.iter().filter(|t| t.status == "pending" || t.status == "running").map(|t| format!("- {} â†’ {}: {} ({})", t.id, t.assigned_agent, t.input, t.status)).collect();
  let recent: Vec<String> = tasks.iter().take(5).filter(|t| t.status == "completed").map(|t| format!("- {} â†’ {}: result: {}", t.id, t.assigned_agent, t.result.as_deref().unwrap_or(""))).collect();
  Ok(format!(
    "## Available Agents\n{}\n## Available Integrations\n{}\n## Active Tasks\n{}\n## Recent Task Results\n{}",
    if agents.is_empty() { "- none".to_string() } else { agents },
    if ints.is_empty() { "- none".to_string() } else { ints },
    if active.is_empty() { "- none".to_string() } else { active.join("\n") },
    if recent.is_empty() { "- none".to_string() } else { recent.join("\n") },
  ))
}

#[tauri::command]
pub async fn execute_agent(app: AppHandle, agent: AgentRequest, input: String, api_key: Option<String>) -> Result<Execution, String> {
  let started = chrono::Utc::now().to_rfc3339();
  let run_id = format!("{}-{}", agent.id, chrono::Utc::now().timestamp_millis());
  let conn = db(&app)?;
  conn.execute("INSERT INTO runs (id,agent_id,started_at,status,model,input) VALUES (?1,?2,?3,'running',?4,?5)", params![run_id, agent.id, started, agent.model, input]).map_err(|e| e.to_string())?;
  let mut events = vec![ExecutionEvent { time: now(), kind: "thought".into(), title: "Loaded isolated agent context".into(), detail: Some(format!("Home: {} Â· tools: {}", agent.home_path, agent.tool_ids.join(", "))) }];
  let home = app.path().app_data_dir().map_err(|e| e.to_string())?.join("agents").join(&agent.id);
  for folder in ["files", "memory", "runs", "outputs"] { fs::create_dir_all(home.join(folder)).map_err(|e| e.to_string())?; }
  let (output, prompt_tokens, completion_tokens) = match run_agent_once(&app, &agent, &input, api_key.as_deref(), &mut events).await {
    Ok(v) => v,
    Err(e) => {
      let _ = conn.execute("UPDATE runs SET status='failed' WHERE id=?1", params![run_id]);
      return Err(e);
    }
  };
  fs::write(home.join("outputs").join(format!("{run_id}.txt")), &output).map_err(|e| e.to_string())?;
  conn.execute("UPDATE runs SET status='completed', output=?1, prompt_tokens=?2, completion_tokens=?3 WHERE id=?4", params![output, prompt_tokens, completion_tokens, run_id]).map_err(|e| e.to_string())?;
  Ok(Execution { output, events, run_id, prompt_tokens, completion_tokens })
}
