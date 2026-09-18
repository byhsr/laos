// Agent execution: assembles an agent's toolset, runs it against its provider
// (Ollama / OpenRouter / Groq, with tool-calling), and records runs.

use rusqlite::{params, Connection};
use std::fs;
use tauri::{AppHandle, Manager};

use crate::db::{db, now};
use crate::http;
use crate::integrations::integration_definitions;
use crate::models::*;
use crate::storage::stored_api_key;
use crate::tasks::list_tasks;
use crate::tools::{
  AgentTool, ApiParam, ApiTool, HttpTool, IntegrationTool, ReadAnyFileTool, ReadFileTool,
  RunCommandTool, SearchFilesTool, WriteFileTool, MAX_TOOL_ROUNDS,
};

pub(crate) fn build_tools(conn: &Connection, agent: &AgentRequest, home: &std::path::Path) -> Result<Vec<Box<dyn AgentTool>>, String> {
  let mut tools: Vec<Box<dyn AgentTool>> = Vec::new();
  let has_network = agent.permissions.iter().any(|p| p == "network");
  let has_files = agent.permissions.iter().any(|p| p == "files");
  let has_host_fs = agent.permissions.iter().any(|p| p == "host_fs");
  let mut stmt = conn.prepare("SELECT kind, enabled, config_json FROM tools WHERE id=?1").map_err(|e| e.to_string())?;
  for tool_id in &agent.tool_ids {
    let mut rows = stmt.query_map(params![tool_id], |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? != 0, row.get::<_, String>(2)?))).map_err(|e| e.to_string())?;
    let Some(Ok((kind, enabled, config_json))) = rows.next() else { continue }; // unknown or missing â†’ skip
    if !enabled { continue; }
    let config: serde_json::Value = serde_json::from_str(&config_json).unwrap_or_else(|_| serde_json::json!({}));
    let str_cfg = |k: &str| config.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string();
    match kind.as_str() {
      "http_get" if has_network => tools.push(Box::new(HttpTool)),
      "api" if has_network => {
        let params = config.get("params").and_then(|p| p.as_array()).map(|arr| arr.iter().filter_map(|p| {
          Some(ApiParam {
            name: p.get("name")?.as_str()?.to_string(),
            param_type: p.get("type").and_then(|t| t.as_str()).unwrap_or("string").to_string(),
            description: p.get("description").and_then(|d| d.as_str()).unwrap_or("").to_string(),
            required: p.get("required").and_then(|r| r.as_bool()).unwrap_or(false),
          })
        }).collect::<Vec<_>>()).unwrap_or_default();
        let headers = config.get("headers").and_then(|h| h.as_array()).map(|arr| arr.iter().filter_map(|h| {
          Some((h.get("name")?.as_str()?.to_string(), h.get("value")?.as_str()?.to_string()))
        }).collect::<Vec<_>>()).unwrap_or_default();
        let name = if str_cfg("name").is_empty() { format!("api_{}", tool_id) } else { str_cfg("name") };
        tools.push(Box::new(ApiTool {
          name,
          description: str_cfg("description"),
          method: str_cfg("method").to_uppercase(),
          url_template: str_cfg("url"),
          headers,
          body_template: if str_cfg("body").is_empty() { None } else { Some(str_cfg("body")) },
          params,
        }));
      }
      "read_file" if has_files => tools.push(Box::new(ReadFileTool { home: home.to_path_buf() })),
      "write_file" if has_files => tools.push(Box::new(WriteFileTool { home: home.to_path_buf() })),
      _ => {}
    }
  }
  // Grant integration actions to agents that explicitly have the integration AND
  // the integration is enabled+connected. Explicit per-agent grants only.
  for integration_id in &agent.integrations {
    let mut stmt = conn.prepare("SELECT enabled, connected, config_json FROM integration_configs WHERE id=?1").map_err(|e| e.to_string())?;
    let mut rows = stmt.query_map(params![integration_id], |row| Ok((row.get::<_, i64>(0)? != 0, row.get::<_, i64>(1)? != 0, row.get::<_, String>(2)?))).map_err(|e| e.to_string())?;
    if let Some(Ok((enabled, connected, cfg_json))) = rows.next() {
      if !enabled || !connected { continue; }
      let credentials: serde_json::Value = serde_json::from_str(&cfg_json).unwrap_or_else(|_| serde_json::json!({}));
      let def = integration_definitions().into_iter().find(|d| d.id == *integration_id);
      if let Some(def) = def {
        for action in def.actions {
          if has_network {
            tools.push(Box::new(IntegrationTool { action: action.name, credentials: credentials.clone() }));
          }
        }
      }
    }
  }
  // Host-filesystem tools: granted only to agents with the explicit host_fs permission.
  if has_host_fs {
    tools.push(Box::new(SearchFilesTool));
    tools.push(Box::new(ReadAnyFileTool));
    tools.push(Box::new(RunCommandTool));
  }
  Ok(tools)
}

pub(crate) fn tool_schemas(tools: &[Box<dyn AgentTool>]) -> Vec<serde_json::Value> {
  tools.iter().map(|t| serde_json::json!({ "type": "function", "function": { "name": t.name(), "description": t.description(), "parameters": t.params_schema() } })).collect()
}

// Renders an agent's attached skills (instruction packs) into prompt text.
// Empty when the agent has none, so unaffected agents keep their exact prompt.
pub(crate) fn skills_prompt(conn: &Connection, skill_ids: &[String]) -> String {
  if skill_ids.is_empty() { return String::new(); }
  let mut stmt = match conn.prepare("SELECT name, description, content FROM skills WHERE id=?1") {
    Ok(s) => s,
    Err(_) => return String::new(),
  };
  let mut out = String::new();
  for id in skill_ids {
    if let Ok((name, description, content)) = stmt.query_row(params![id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?))) {
      if out.is_empty() { out.push_str("\n## Skills\n"); }
      out.push_str(&format!("\n### {name}\n"));
      if !description.trim().is_empty() { out.push_str(&format!("When to use: {}\n", description.trim())); }
      if !content.trim().is_empty() { out.push_str(content.trim()); out.push('\n'); }
    }
  }
  out
}

// Returns (final_text, events, used_tools, prompt_tokens, completion_tokens).
async fn run_ollama_chat(model: &str, prompt: &str, agent: &AgentRequest, tools: &[Box<dyn AgentTool>]) -> Result<(String, Vec<ExecutionEvent>, bool, u64, u64), String> {
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
    let body = ChatRequest { model: model.into(), messages: messages.clone(), tools: Some(tool_schemas(tools)), stream: false };
    let response = client.post("http://127.0.0.1:11434/api/chat").json(&body).send().await.map_err(|e| format!("Could not reach Ollama. Start it at http://127.0.0.1:11434 ({e})"))?;
    if !response.status().is_success() { return Err(format!("Ollama returned {}", response.status())); }
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
      let content = reply.content.unwrap_or_default();
      return Ok((content, events, used_tools, prompt_tokens, completion_tokens));
    }
  }
  Err("Tool loop exceeded the maximum number of rounds.".into())
}

// Provider-agnostic tool-calling loop for the OpenAI-compatible endpoints
// (OpenRouter / Groq). Mirrors run_ollama_chat but speaks the OpenAI shape:
// arguments arrive as a JSON string, and each tool result must carry its call id.
async fn run_openai_tool_chat(url: &str, key: &str, model: &str, is_openrouter: bool, prompt: &str, agent: &AgentRequest, tools: &[Box<dyn AgentTool>]) -> Result<(String, Vec<ExecutionEvent>, bool, u64, u64), String> {
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
    let mut body = serde_json::json!({ "model": model, "messages": messages, "tools": tool_schemas(tools), "stream": false });
    http::apply_openai_defaults(&mut body, is_openrouter, http::CHAT_MAX_TOKENS);
    let response = http::send_with_retry(|| {
      let mut req = client.post(url).header("Authorization", format!("Bearer {key}")).json(&body);
      if is_openrouter { req = req.header("HTTP-Referer", "https://local-agent-os.app").header("X-Title", "Local Agent OS"); }
      req
    }, http::MODEL_ATTEMPTS).await?;
    let parsed: serde_json::Value = response.json().await.map_err(|e| format!("Could not parse the tool response: {e}"))?;
    prompt_tokens += parsed["usage"]["prompt_tokens"].as_u64().unwrap_or(0);
    completion_tokens += parsed["usage"]["completion_tokens"].as_u64().unwrap_or(0);
    let message = parsed["choices"][0]["message"].clone();
    let calls = message["tool_calls"].as_array().cloned().unwrap_or_default();
    if calls.is_empty() {
      let content = message["content"].as_str().unwrap_or("").to_string();
      return Ok((content, events, used_tools, prompt_tokens, completion_tokens));
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
  let prompt = if structured {
    format!("You are {}. Objective: {}\n{skills}\nTask: {}\n\nReturn a concise structured result in JSON with keys summary and result.", agent.name, agent.objective, input)
  } else {
    format!("You are {}. Objective: {}\n{skills}\nTask: {}\n\nReturn a helpful, direct answer.", agent.name, agent.objective, input)
  };
  if let Some(model) = agent.model.strip_prefix("ollama:") {
    let tools = build_tools(&conn, agent, &home)?;
    if !tools.is_empty() {
      events.push(ExecutionEvent { time: now(), kind: "thought".into(), title: "Asking Ollama".into(), detail: Some(format!("{model} with {} tool(s)", tools.len())) });
      let (output, tool_events, _, pt, ct) = run_ollama_chat(model, &prompt, agent, &tools).await?;
      events.extend(tool_events);
      Ok((output, pt, ct))
    } else {
      events.push(ExecutionEvent { time: now(), kind: "thought".into(), title: "Asking Ollama".into(), detail: Some(model.into()) });
      let response = http::stream_client().post("http://127.0.0.1:11434/api/generate").json(&serde_json::json!({"model":model,"prompt":prompt,"stream":false})).send().await.map_err(|e| format!("Could not reach Ollama. Start it at http://127.0.0.1:11434 ({e})"))?;
      if !response.status().is_success() { return Err(format!("Ollama returned {}", response.status())); }
      let json: serde_json::Value = response.json().await.map_err(|e| e.to_string())?;
      let output = json["response"].as_str().unwrap_or("No response from Ollama.").to_string();
      let pt = json["prompt_eval_count"].as_u64().unwrap_or(0);
      let ct = json["eval_count"].as_u64().unwrap_or(0);
      events.push(ExecutionEvent { time: now(), kind: "result".into(), title: "Generated final result".into(), detail: None });
      Ok((output, pt, ct))
    }
  } else if let Some(model) = agent.model.strip_prefix("openrouter:") {
    let key = api_key
      .filter(|k| !k.trim().is_empty())
      .map(|k| k.to_string())
      .or(stored_api_key(&conn, &agent.model).ok().flatten())
      .ok_or("OpenRouter requires an API key. Add it in Models first.")?;
    let tools = build_tools(&conn, agent, &home)?;
    events.push(ExecutionEvent { time: now(), kind: "thought".into(), title: "Asking OpenRouter".into(), detail: Some(if tools.is_empty() { model.into() } else { format!("{model} with {} tool(s)", tools.len()) }) });
    if !tools.is_empty() {
      let (output, tool_events, _, pt, ct) = run_openai_tool_chat("https://openrouter.ai/api/v1/chat/completions", &key, model, true, &prompt, agent, &tools).await?;
      events.extend(tool_events);
      return Ok((output, pt, ct));
    }
    let mut body = serde_json::json!({"model":model,"messages":[{"role":"user","content":prompt}]});
    http::apply_openai_defaults(&mut body, true, http::CHAT_MAX_TOKENS);
    let response = http::send_with_retry(|| {
      http::client().post("https://openrouter.ai/api/v1/chat/completions")
        .header("Authorization", format!("Bearer {key}"))
        .header("HTTP-Referer", "https://local-agent-os.app")
        .header("X-Title", "Local Agent OS")
        .json(&body)
    }, http::MODEL_ATTEMPTS).await?;
    let json: serde_json::Value = response.json().await.map_err(|e| e.to_string())?;
    let output = json["choices"][0]["message"]["content"].as_str().unwrap_or("OpenRouter returned no text.").to_string();
    let pt = json["usage"]["prompt_tokens"].as_u64().unwrap_or(0);
    let ct = json["usage"]["completion_tokens"].as_u64().unwrap_or(0);
    events.push(ExecutionEvent { time: now(), kind: "result".into(), title: "Generated final result".into(), detail: None });
    Ok((output, pt, ct))
  } else if let Some(model) = agent.model.strip_prefix("groq:") {
    let key = api_key
      .filter(|k| !k.trim().is_empty())
      .map(|k| k.to_string())
      .or(stored_api_key(&conn, &agent.model).ok().flatten())
      .ok_or("Groq requires an API key. Add it in Models first.")?;
    let tools = build_tools(&conn, agent, &home)?;
    events.push(ExecutionEvent { time: now(), kind: "thought".into(), title: "Asking Groq".into(), detail: Some(if tools.is_empty() { model.into() } else { format!("{model} with {} tool(s)", tools.len()) }) });
    if !tools.is_empty() {
      let (output, tool_events, _, pt, ct) = run_openai_tool_chat("https://api.groq.com/openai/v1/chat/completions", &key, model, false, &prompt, agent, &tools).await?;
      events.extend(tool_events);
      return Ok((output, pt, ct));
    }
    let mut body = serde_json::json!({"model":model,"messages":[{"role":"user","content":prompt}]});
    http::apply_openai_defaults(&mut body, false, http::CHAT_MAX_TOKENS);
    let response = http::send_with_retry(|| {
      http::client().post("https://api.groq.com/openai/v1/chat/completions")
        .header("Authorization", format!("Bearer {key}"))
        .json(&body)
    }, http::MODEL_ATTEMPTS).await?;
    let json: serde_json::Value = response.json().await.map_err(|e| e.to_string())?;
    let output = json["choices"][0]["message"]["content"].as_str().unwrap_or("Groq returned no text.").to_string();
    let pt = json["usage"]["prompt_tokens"].as_u64().unwrap_or(0);
    let ct = json["usage"]["completion_tokens"].as_u64().unwrap_or(0);
    events.push(ExecutionEvent { time: now(), kind: "result".into(), title: "Generated final result".into(), detail: None });
    Ok((output, pt, ct))
  } else { Err("Unknown model provider. Select Ollama, OpenRouter or Groq in the agent Model tab.".into()) }
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
      let mut stmt = conn.prepare("SELECT enabled, connected FROM integration_configs WHERE id=?1").map_err(|e| e.to_string())?;
      let mut rows = stmt.query_map(params![d.id], |row| Ok((row.get::<_, i64>(0)? != 0, row.get::<_, i64>(1)? != 0))).map_err(|e| e.to_string())?;
      let (enabled, connected) = rows.next().transpose().map_err(|e| e.to_string())?.unwrap_or((false, false));
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

pub(crate) fn api_key_for(conn: &Connection, model: &str) -> Result<String, String> {
  stored_api_key(conn, model).ok().flatten().filter(|k| !k.is_empty()).ok_or("OpenRouter requires an API key. Add it in Models first.".into())
}
