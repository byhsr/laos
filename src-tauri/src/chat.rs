// Streaming chat: streams a completion from the configured provider as token
// deltas, including tool-call rounds (the Manager's app-control tools, or a
// regular agent's own tools) and confirmation gating.

use std::time::{Duration, Instant};

use futures_util::StreamExt;
use rusqlite::params;
use tauri::{AppHandle, Manager};

use crate::agents::{all_mcp_tools, build_tools, tool_schemas};
use crate::db::db;
use crate::http;
use crate::manager::{
  approvals, build_manager_system_prompt, manager_tools, requires_confirmation,
  run_manager_tool, Approval,
};
use crate::memory::{
  append_session_message, build_day_context, load_conversation, load_memory, save_conversation,
  summarize_old_turns, MEMORY_SUMMARY_KEY, ROLLING_WINDOW,
};
use crate::models::*;
use crate::provider;
use crate::tools::{AgentTool, MAX_TOOL_ROUNDS};
use crate::tooltext::{parse_text_tool_calls, ToolCallLeakFilter};

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
  let tools: Vec<Box<dyn AgentTool>> = if is_manager {
    // The Manager keeps its app-control tools and also gets every imported MCP
    // tool, so a configured MCP server is usable from the lead agent too.
    let mut manager = manager_tools();
    manager.extend(all_mcp_tools(&conn));
    manager
  } else {
    build_tools(&conn, &agent, &home)?
  };

  let system = if is_manager {
    build_manager_system_prompt(&conn, &agent.name, &full_memory)?
  } else {
    // Compact long-term memory plus the time-based context (last chat summary,
    // today, yesterday) go in on every turn, so the agent starts each call
    // already aware of recent work.
    let day_context = build_day_context(&conn, &agent.id).unwrap_or_default();
    let tool_note = if tools.is_empty() { String::new() } else { "\nYou have tools available: call one when it helps, wait for the result, then continue.".to_string() };
    format!("You are {}. Objective: {}\n\n{full_memory}{day_context}{tool_note}\nReturn a helpful, direct answer.", agent.name, agent.objective)
  };

  let window: Vec<serde_json::Value> = history.iter().rev().take(ROLLING_WINDOW).cloned().collect::<Vec<_>>().into_iter().rev().collect();
  let mut messages: Vec<serde_json::Value> = vec![serde_json::json!({ "role": "system", "content": system })];
  messages.extend(window);

  // Provider, endpoint, key and reasoning setting all resolve in one place now.
  let resolved = provider::resolve(&conn, &agent.model, None)?;
  let is_ollama = resolved.kind == provider::Kind::Ollama;

  // Tool-call rounds (non-streaming) run first, then the final reply streams.
  if !tools.is_empty() {
    let client = http::client();
    for _ in 0..MAX_TOOL_ROUNDS {
      emit_status(&on_event, "think", "Thinking…");
      let mut body = serde_json::json!({
        "model": resolved.model,
        "messages": messages,
        "tools": tool_schemas(&tools),
        "stream": false,
      });
      let budget = provider::apply_reasoning(&mut body, &resolved, http::CHAT_MAX_TOKENS);
      if is_ollama {
        provider::apply_ollama_options(&mut body, budget);
      } else {
        http::apply_openai_defaults(&mut body, resolved.kind == provider::Kind::OpenRouter, budget);
      }
      let response = http::send_model_request(&client, &resolved, &resolved.chat_url(), &body, http::MODEL_ATTEMPTS).await?;
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
      if tool_calls.is_empty() {
        // A text-protocol model (DeepSeek DSML) puts the call in `content` rather
        // than `tool_calls`. Recover it so the call actually runs, instead of
        // dropping it and streaming the raw markup into the chat.
        let content = parsed["message"]["content"].as_str()
          .or_else(|| parsed["choices"][0]["message"]["content"].as_str())
          .unwrap_or("");
        let (_, text_calls) = parse_text_tool_calls(content);
        if text_calls.is_empty() { break; }
        // Text-protocol models have no `tool` role or call ids: hand the model its
        // own call back, then each result as a user turn.
        messages.push(serde_json::json!({ "role": "assistant", "content": content }));
        for call in text_calls {
          emit_status(&on_event, "tool", &format!("Calling {}…", call.name));
          let result = run_tool(&app, &tools, is_manager, &call.name, &call.args, &on_event).await?;
          messages.push(serde_json::json!({ "role": "user", "content": format!("<tool_result name=\"{}\">\n{}\n</tool_result>", call.name, result) }));
        }
        continue;
      }

      // Append the assistant tool-call message (correct shape for the provider),
      // then one tool result per call, carrying the call id (required by the
      // OpenAI-compatible providers).
      let assistant_msg = parsed["message"].clone();
      let assistant_msg = if assistant_msg.is_null() { parsed["choices"][0]["message"].clone() } else { assistant_msg };
      messages.push(assistant_msg);
      for (call_id, name, args) in tool_calls {
        emit_status(&on_event, "tool", &format!("Calling {name}…"));
        let result = run_tool(&app, &tools, is_manager, &name, &args, &on_event).await?;
        let mut tool_msg = serde_json::json!({ "role": "tool", "content": result });
        if !call_id.is_empty() { tool_msg["tool_call_id"] = serde_json::json!(call_id); }
        messages.push(tool_msg);
      }
    }
  }

  emit_status(&on_event, "write", "Writing the reply…");
  let client = http::stream_client();
  let mut body = serde_json::json!({ "model": resolved.model, "messages": messages, "stream": true });
  let budget = provider::apply_reasoning(&mut body, &resolved, http::CHAT_MAX_TOKENS);
  if is_ollama {
    provider::apply_ollama_options(&mut body, budget);
  } else {
    let is_openrouter = resolved.kind == provider::Kind::OpenRouter;
    http::apply_openai_defaults(&mut body, is_openrouter, budget);
    // Ask for usage on the final chunk so the recorded run has real token counts.
    if is_openrouter { body["usage"] = serde_json::json!({ "include": true }); }
    else { body["stream_options"] = serde_json::json!({ "include_usage": true }); }
  }
  let response = http::send_model_request(&client, &resolved, &resolved.chat_url(), &body, http::MODEL_ATTEMPTS).await?;
  let mut stream = response.bytes_stream();
  let mut buffer = String::new();
  let mut delta = String::new();
  // Even the final (tool-less) stream can carry text-protocol markup; never let
  // it reach the chat bubble.
  let mut leak = ToolCallLeakFilter::new();
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
          // Ollama reports the model's reasoning on its own channel.
          if let Some(t) = v["message"]["thinking"].as_str() { if !t.is_empty() { emit_reasoning(&on_event, t); } }
          if let Some(d) = v["message"]["content"].as_str() {
            let safe = leak.feed(d);
            if !safe.is_empty() { delta.push_str(&safe); on_event.send(safe).map_err(|e| e.to_string())?; }
          }
          prompt_tokens = v["prompt_eval_count"].as_u64().unwrap_or(prompt_tokens);
          completion_tokens = v["eval_count"].as_u64().unwrap_or(completion_tokens);
        }
      } else {
        // OpenRouter sends `: OPENROUTER PROCESSING` keep-alive comments; ignore
        // anything that isn't a data line or the [DONE] sentinel.
        let Some(data) = line.strip_prefix("data:").map(|s| s.trim()) else { continue };
        if data == "[DONE]" { continue; }
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(data) {
          // OpenAI-compatible providers stream reasoning separately from content.
          if let Some(t) = v["choices"][0]["delta"]["reasoning_content"].as_str() { if !t.is_empty() { emit_reasoning(&on_event, t); } }
          if let Some(d) = v["choices"][0]["delta"]["content"].as_str() {
            let safe = leak.feed(d);
            if !safe.is_empty() { delta.push_str(&safe); on_event.send(safe).map_err(|e| e.to_string())?; }
          }
          prompt_tokens = v["usage"]["prompt_tokens"].as_u64().unwrap_or(prompt_tokens);
          completion_tokens = v["usage"]["completion_tokens"].as_u64().unwrap_or(completion_tokens);
        }
      }
    }
  }
  // Flush whatever the leak filter was still holding back.
  let tail = leak.finish();
  if !tail.is_empty() { delta.push_str(&tail); on_event.send(tail).map_err(|e| e.to_string())?; }

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

// A live step for the chat bubble while the reply is being prepared. Tool rounds
// run before any token streams, so without this the UI can only show a generic
// "typing" placeholder. The kind lets the UI group the trace (think / tool /
// write / wait) instead of printing one flat status line.
fn emit_status(on_event: &tauri::ipc::Channel<String>, kind: &str, text: &str) {
  let _ = on_event.send(serde_json::json!({ "type": "status", "kind": kind, "text": text }).to_string());
}

// The model's own reasoning stream, forwarded so the UI can show what it is
// actually thinking. Only providers that expose it send anything.
fn emit_reasoning(on_event: &tauri::ipc::Channel<String>, text: &str) {
  let _ = on_event.send(serde_json::json!({ "type": "reasoning", "text": text }).to_string());
}

// Runs a single tool call. State-changing tools go through the confirmation popup
// first; everything else executes immediately.
async fn run_tool(app: &AppHandle, tools: &[Box<dyn AgentTool>], is_manager: bool, name: &str, args: &serde_json::Value, on_event: &tauri::ipc::Channel<String>) -> Result<String, String> {
  if !requires_confirmation(name) {
    return Ok(execute_tool(app, tools, is_manager, name, args).await);
  }
  let request_id = format!("req-{}", chrono::Utc::now().timestamp_millis());
  emit_status(on_event, "wait", "Waiting for your approval…");
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
    // The Manager's own tools dispatch by name; the MCP tools it also holds run
    // like any other agent tool.
    run_manager_tool(app, tools, name, args).await.unwrap_or_else(|e| e)
  } else {
    match tools.iter().find(|t| t.name() == name) {
      Some(t) => t.run(args).await.unwrap_or_else(|e| format!("Error: {e}")),
      None => format!("Unknown tool '{name}'."),
    }
  }
}
