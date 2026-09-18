// Streaming chat: streams a completion from the configured provider as token
// deltas, including tool-call rounds (the Manager's app-control tools, or a
// regular agent's own tools) and confirmation gating.

use std::time::{Duration, Instant};

use futures_util::StreamExt;
use rusqlite::params;
use tauri::{AppHandle, Manager};

use crate::agents::{build_tools, tool_schemas};
use crate::db::db;
use crate::http;
use crate::manager::{
  approvals, build_manager_system_prompt, dispatch_manager_tool, manager_tools,
  requires_confirmation, Approval,
};
use crate::memory::{
  append_session_message, load_conversation, load_memory, save_conversation, summarize_old_turns,
  MEMORY_SUMMARY_KEY, ROLLING_WINDOW,
};
use crate::models::*;
use crate::storage::stored_api_key;
use crate::tools::{AgentTool, MAX_TOOL_ROUNDS};

// Streams a chat completion from the configured provider as token deltas.
// Ollama uses NDJSON; Groq/OpenRouter use SSE `data:` lines. Context is a rolling
// window of the last ROLLING_WINDOW persisted messages.
#[tauri::command]
pub async fn stream_chat(app: AppHandle, agent: AgentRequest, input: String, is_manager: bool, on_event: tauri::ipc::Channel<String>, session_id: Option<String>) -> Result<(), String> {
  let conn = db(&app)?;

  // Rolling window: load history, append the new user turn.
  let mut history = load_conversation(&conn, &agent.id);
  history.push(serde_json::json!({ "role": "user", "content": input }));

  // Fold older turns into memory in the background. It is an extra model call and
  // must never sit in front of the reply the user is waiting for.
  if history.len() > ROLLING_WINDOW + 1 {
    let app_bg = app.clone();
    let agent_id = agent.id.clone();
    let model = agent.model.clone();
    let history_bg = history.clone();
    tauri::async_runtime::spawn(async move {
      let _ = summarize_old_turns(&app_bg, &agent_id, &model, &history_bg).await;
    });
  }

  // Build memory context (summary + facts) for the system prompt. Only compact
  // long-term facts are injected — today's/yesterday's context and last-chat
  // summaries are NOT force-fed (that causes hallucination across new chats).
  // They're available on demand via the recall_memory tool.
  let memory = load_memory(&conn, &agent.id);
  let mut memory_blob = String::new();
  for (k, v) in &memory {
    if k == MEMORY_SUMMARY_KEY {
      memory_blob.push_str(&format!("[Memory summary: {v}]\n"));
    } else if let Some(fact) = k.strip_prefix("fact:") {
      memory_blob.push_str(&format!("- {fact}: {v}\n"));
    }
  }
  let full_memory = memory_blob;

  // Tools: the Manager gets its app-control tools; a regular agent gets its own.
  let home = app.path().app_data_dir().map_err(|e| e.to_string())?.join("agents").join(&agent.id);
  let tools: Vec<Box<dyn AgentTool>> = if is_manager { manager_tools() } else { build_tools(&conn, &agent, &home)? };

  let system = if is_manager {
    build_manager_system_prompt(&conn, &full_memory)?
  } else {
    let tool_note = if tools.is_empty() { String::new() } else { "\nYou have tools available: call one when it helps, wait for the result, then continue.".to_string() };
    format!("You are {}. Objective: {}\n\n{full_memory}{tool_note}\nReturn a helpful, direct answer.", agent.name, agent.objective)
  };

  let window: Vec<serde_json::Value> = history.iter().rev().take(ROLLING_WINDOW).cloned().collect::<Vec<_>>().into_iter().rev().collect();
  let mut messages: Vec<serde_json::Value> = vec![serde_json::json!({ "role": "system", "content": system })];
  messages.extend(window);

  let model = agent.model.clone();
  let (url, auth, key, is_openrouter) = if let Some(m) = model.strip_prefix("ollama:") {
    ("http://127.0.0.1:11434/api/chat".to_string(), None::<String>, m.to_string(), false)
  } else if let Some(m) = model.strip_prefix("groq:") {
    let k = stored_api_key(&conn, &model).ok().flatten().ok_or("Groq requires an API key.")?;
    ("https://api.groq.com/openai/v1/chat/completions".into(), Some(k), m.to_string(), false)
  } else if let Some(m) = model.strip_prefix("openrouter:") {
    let k = stored_api_key(&conn, &model).ok().flatten().ok_or("OpenRouter requires an API key.")?;
    ("https://openrouter.ai/api/v1/chat/completions".into(), Some(k), m.to_string(), true)
  } else {
    return Err("Unknown model provider.".into());
  };
  let is_ollama = url.contains("11434");

  // Tool-call rounds (non-streaming) run first, then the final reply streams.
  if !tools.is_empty() {
    let client = http::client();
    for _ in 0..MAX_TOOL_ROUNDS {
      let mut body = serde_json::json!({
        "model": key,
        "messages": messages,
        "tools": tool_schemas(&tools),
        "stream": false,
      });
      if !is_ollama { http::apply_openai_defaults(&mut body, is_openrouter, http::CHAT_MAX_TOKENS); }
      let mut req = client.post(&url).json(&body);
      if let Some(auth) = &auth { req = req.header("Authorization", format!("Bearer {auth}")); }
      let response = req.send().await.map_err(|e| format!("Could not reach the model provider: {e}"))?;
      if !response.status().is_success() { return Err(format!("Model provider returned {}", response.status())); }
      let parsed: serde_json::Value = response.json().await.map_err(|e| e.to_string())?;

      // Support both Ollama (message.tool_calls) and OpenAI-compatible
      // (choices[0].message.tool_calls) response shapes.
      let calls_arr = parsed["message"]["tool_calls"].as_array()
        .or_else(|| parsed["choices"][0]["message"]["tool_calls"].as_array())
        .cloned()
        .unwrap_or_default();
      let mut tool_calls: Vec<(String, String, serde_json::Value)> = Vec::new();
      for c in calls_arr {
        let name = c["function"]["name"].as_str().unwrap_or("").to_string();
        // Ollama gives arguments as an object; OpenAI-compatible as a JSON string.
        let args = match c["function"]["arguments"].as_str() {
          Some(s) => serde_json::from_str(s).unwrap_or_else(|_| serde_json::json!({})),
          None => c["function"]["arguments"].clone(),
        };
        if !name.is_empty() { tool_calls.push((c["id"].as_str().unwrap_or("").to_string(), name, args)); }
      }
      if tool_calls.is_empty() { break; }

      // Append the assistant tool-call message (correct shape for the provider),
      // then one tool result per call, carrying the call id (required by the
      // OpenAI-compatible providers).
      let assistant_msg = parsed["message"].clone();
      let assistant_msg = if assistant_msg.is_null() { parsed["choices"][0]["message"].clone() } else { assistant_msg };
      messages.push(assistant_msg);
      for (call_id, name, args) in tool_calls {
        let result = run_tool(&app, &tools, is_manager, &name, &args, &on_event).await?;
        let mut tool_msg = serde_json::json!({ "role": "tool", "content": result });
        if !call_id.is_empty() { tool_msg["tool_call_id"] = serde_json::json!(call_id); }
        messages.push(tool_msg);
      }
    }
  }

  let client = http::stream_client();
  let mut body = serde_json::json!({ "model": key, "messages": messages, "stream": true });
  if !is_ollama {
    http::apply_openai_defaults(&mut body, is_openrouter, http::CHAT_MAX_TOKENS);
    // Ask for usage on the final chunk so the recorded run has real token counts.
    if is_openrouter { body["usage"] = serde_json::json!({ "include": true }); }
    else { body["stream_options"] = serde_json::json!({ "include_usage": true }); }
  }
  let mut req = client.post(&url).json(&body);
  if let Some(auth) = &auth { req = req.header("Authorization", format!("Bearer {auth}")); }

  let response = req.send().await.map_err(|e| format!("Could not reach the model provider: {e}"))?;
  if !response.status().is_success() {
    return Err(format!("Model provider returned {}", response.status()));
  }
  let mut stream = response.bytes_stream();
  let mut buffer = String::new();
  let mut delta = String::new();
  let mut prompt_tokens = 0u64;
  let mut completion_tokens = 0u64;
  loop {
    // A stalled upstream (OpenRouter switching providers, a dropped connection)
    // must surface as an error rather than an indefinite wait.
    let chunk = match tokio::time::timeout(http::STREAM_IDLE_TIMEOUT, stream.next()).await {
      Err(_) => return Err(format!("The model provider stopped responding (no data for {}s).", http::STREAM_IDLE_TIMEOUT.as_secs())),
      Ok(None) => break,
      Ok(Some(chunk)) => chunk.map_err(|e| e.to_string())?,
    };
    buffer.push_str(&String::from_utf8_lossy(&chunk));
    // Ollama NDJSON: one JSON object per line. OpenAI-compatible: SSE `data:` lines.
    while let Some(pos) = buffer.find('\n') {
      let line: String = buffer.drain(..=pos).collect();
      let line = line.trim();
      if line.is_empty() { continue; }
      if is_ollama {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(line) {
          if let Some(d) = v["message"]["content"].as_str() { delta.push_str(d); on_event.send(d.to_string()).map_err(|e| e.to_string())?; }
          prompt_tokens = v["prompt_eval_count"].as_u64().unwrap_or(prompt_tokens);
          completion_tokens = v["eval_count"].as_u64().unwrap_or(completion_tokens);
        }
      } else {
        // OpenRouter sends `: OPENROUTER PROCESSING` keep-alive comments; ignore
        // anything that isn't a data line or the [DONE] sentinel.
        let Some(data) = line.strip_prefix("data:").map(|s| s.trim()) else { continue };
        if data == "[DONE]" { continue; }
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(data) {
          if let Some(d) = v["choices"][0]["delta"]["content"].as_str() { delta.push_str(d); on_event.send(d.to_string()).map_err(|e| e.to_string())?; }
          prompt_tokens = v["usage"]["prompt_tokens"].as_u64().unwrap_or(prompt_tokens);
          completion_tokens = v["usage"]["completion_tokens"].as_u64().unwrap_or(completion_tokens);
        }
      }
    }
  }
  // Persist the run and the conversation.
  let run_id = format!("{}-{}", agent.id, chrono::Utc::now().timestamp_millis());
  let started = chrono::Utc::now().to_rfc3339();
  let _ = conn.execute(
    "INSERT INTO runs (id,agent_id,started_at,status,model,input,output,prompt_tokens,completion_tokens) VALUES (?1,?2,?3,'completed',?4,?5,?6,?7,?8)",
    params![run_id, agent.id, started, agent.model, input, delta, prompt_tokens as i64, completion_tokens as i64],
  );
  if !delta.is_empty() {
    history.push(serde_json::json!({ "role": "assistant", "content": delta }));
    let _ = save_conversation(&conn, &agent.id, &history);
    // Also append to the chat session (bifurcated history) if one is active.
    if let Some(sess) = &session_id {
      let _ = append_session_message(&conn, sess, &agent.id, "user", &input);
      let _ = append_session_message(&conn, sess, &agent.id, "assistant", &delta);
    }
  }
  Ok(())
}

// Runs a single tool call. State-changing tools go through the confirmation popup
// first; everything else executes immediately.
async fn run_tool(app: &AppHandle, tools: &[Box<dyn AgentTool>], is_manager: bool, name: &str, args: &serde_json::Value, on_event: &tauri::ipc::Channel<String>) -> Result<String, String> {
  if !requires_confirmation(name) {
    return Ok(execute_tool(app, tools, is_manager, name, args).await);
  }
  let request_id = format!("req-{}", chrono::Utc::now().timestamp_millis());
  let event = serde_json::json!({ "type": "confirm", "requestId": request_id, "tool": name, "args": args });
  on_event.send(event.to_string()).map_err(|e| e.to_string())?;
  let deadline = Instant::now() + Duration::from_secs(120);
  let decision = loop {
    {
      let mut map = approvals().lock().map_err(|e| e.to_string())?;
      if let Some(a) = map.remove(&request_id) { break a; }
    }
    if Instant::now() > deadline { break Approval { approved: false, edited_args: serde_json::json!({}) }; }
    tokio::time::sleep(Duration::from_millis(200)).await;
  };
  if !decision.approved { return Ok("The user declined this action.".to_string()); }
  Ok(execute_tool(app, tools, is_manager, name, &decision.edited_args).await)
}

async fn execute_tool(app: &AppHandle, tools: &[Box<dyn AgentTool>], is_manager: bool, name: &str, args: &serde_json::Value) -> String {
  if is_manager {
    dispatch_manager_tool(app, name, args).await.unwrap_or_else(|e| e)
  } else {
    match tools.iter().find(|t| t.name() == name) {
      Some(t) => t.run(args).await.unwrap_or_else(|e| format!("Error: {e}")),
      None => format!("Unknown tool '{name}'."),
    }
  }
}
