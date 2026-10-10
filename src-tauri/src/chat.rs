// Streaming chat: streams a completion from the configured provider as token
// deltas, including tool-call rounds (the Manager's app-control tools, or a
// regular agent's own tools) and confirmation gating.

use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use futures_util::StreamExt;
use rusqlite::params;
use tauri::{AppHandle, Manager};

use crate::agents::{build_tools, mcp_tools_for, skills_prompt, tool_schemas};
use crate::db::db;
use crate::http;
use crate::manager::{
  approvals, build_manager_system_prompt, manager_tools, requires_confirmation,
  run_manager_tool, Approval,
};
use crate::memory::{
  append_session_message, load_conversation, load_session_messages, save_conversation,
  summarize_old_turns, ROLLING_WINDOW,
};
use crate::models::*;
use crate::provider;
use crate::tools::{AgentTool, ToolOutput, MAX_TOOL_ROUNDS};
use crate::tooltext::{parse_text_tool_calls, ToolCallLeakFilter};

// Turns the user asked to cancel, keyed by a per-turn id the frontend generates,
// so a cancel can only ever affect the turn it belongs to. The entry is removed
// the moment that turn ends, however it ends.
static CANCELLED_STREAMS: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
fn cancelled_streams() -> &'static Mutex<HashSet<String>> {
  CANCELLED_STREAMS.get_or_init(|| Mutex::new(HashSet::new()))
}
fn is_cancelled(stream_id: Option<&str>) -> bool {
  match stream_id {
    Some(id) => cancelled_streams().lock().map(|set| set.contains(id)).unwrap_or(false),
    None => false,
  }
}

// Marks a turn for cancellation. The running turn notices on its next check.
#[tauri::command]
pub fn cancel_chat(stream_id: String) {
  if let Ok(mut set) = cancelled_streams().lock() { set.insert(stream_id); }
}

// Resolves as soon as the turn is cancelled. With no id to watch it pends
// forever, so it is safe to race inside a select! either way.
async fn wait_for_cancel(stream_id: Option<String>) {
  match stream_id {
    None => std::future::pending::<()>().await,
    Some(id) => loop {
      if is_cancelled(Some(&id)) { return; }
      tokio::time::sleep(Duration::from_millis(120)).await;
    },
  }
}

// Streams a chat completion from the configured provider as token deltas.
// Ollama uses NDJSON; Groq/OpenRouter use SSE `data:` lines. Context is a rolling
// window of the last ROLLING_WINDOW persisted messages.
#[tauri::command]
pub async fn stream_chat(app: AppHandle, agent: AgentRequest, input: String, is_manager: bool, on_event: tauri::ipc::Channel<String>, session_id: Option<String>, stream_id: Option<String>) -> Result<(), String> {
  let result = stream_chat_inner(app, agent, input, is_manager, on_event, session_id, stream_id.clone()).await;
  // Clear the cancel flag for this turn so the registry can't grow unbounded.
  if let Some(id) = &stream_id {
    if let Ok(mut set) = cancelled_streams().lock() { set.remove(id); }
  }
  result
}

async fn stream_chat_inner(app: AppHandle, agent: AgentRequest, input: String, is_manager: bool, on_event: tauri::ipc::Channel<String>, session_id: Option<String>, stream_id: Option<String>) -> Result<(), String> {
  let conn = db(&app)?;

  // Rolling window: the CURRENT CHAT's turns. A new session starts empty, so a
  // new chat gets a clean context and only long-term memory carries over. A call
  // with no session (an internal/one-off turn) still falls back to the agent's
  // stored conversation.
  let scoped_to_session = session_id.as_deref().map(|s| !s.is_empty()).unwrap_or(false);
  let mut history = match &session_id {
    Some(sid) if scoped_to_session => load_session_messages(&conn, sid),
    _ => load_conversation(&conn, &agent.id),
  };
  history.push(serde_json::json!({ "role": "user", "content": input }));

  // The session's project (if any) scopes this turn's memory: durable memory is
  // filed under the project so it accumulates across its conversations.
  let project_scope = session_id.as_deref().and_then(|sid| crate::memory::session_project(&conn, sid));

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

  // Memory is pull-only: nothing durable is injected here. A new chat starts
  // clean, and memory only enters the window when the agent asks for it (a
  // recall/search memory tool, or the Manager's recall_memory). This is what
  // keeps a fresh session truly fresh.

  // Tools: the Manager gets its app-control tools plus the MCP tools it was
  // opted into; a regular agent gets its own.
  let home = app.path().app_data_dir().map_err(|e| e.to_string())?.join("agents").join(&agent.id);
  let pc_control = agent.permissions.iter().any(|p| p == "pc_control");
  let tools: Vec<Box<dyn AgentTool>> = if is_manager {
    let mut manager = manager_tools();
    if pc_control { manager.extend(crate::tools::desktop_tools()); }
    manager.extend(crate::tools::script_tools(&conn));
    manager.extend(mcp_tools_for(&conn, &agent.tool_ids));
    manager
  } else {
    build_tools(&conn, &agent, &home)?
  };

  let system = if is_manager {
    build_manager_system_prompt(&conn, &agent.name, &agent.tool_ids, pc_control)?
  } else {
    // The conversation itself is the rolling window; no memory is injected.
    let tool_note = if tools.is_empty() { String::new() } else { "\nYou have tools available: call one when it helps, wait for the result, then continue.".to_string() };
    // When the memory connector is attached, tell the agent which identity to file
    // memories under, so they land in the namespace the Memory view reads.
    let memory_note = if agent.tool_ids.iter().any(|t| t.starts_with("mcp:memory:")) {
      match &project_scope {
        Some(pid) => format!("\nYou have a persistent memory for this project. When you use a memory tool, pass agentId \"{id}\" and scope {{\"type\":\"project\",\"id\":\"{pid}\"}}.", id = agent.id),
        None => format!("\nYou have a persistent memory. When you use a memory tool, pass agentId \"{id}\" and scope {{\"type\":\"agent\",\"id\":\"{id}\"}}.", id = agent.id),
      }
    } else {
      String::new()
    };
    // PC control guidance, only when the agent can actually control the PC.
    let pc_note = if pc_control {
      "\nYou can see and control the screen. To act: call screen_capture first, then give mouse coordinates in that image's pixel space (origin top-left); use type_text / press_keys for the keyboard."
    } else {
      ""
    };
    // Attached skills (instructions + reference + scripts) go into the live chat
    // prompt too, not just one-shot runs.
    let skills = skills_prompt(&conn, &agent.skill_ids);
    format!("You are {}. Objective: {}\n{skills}\n{memory_note}{pc_note}{tool_note}\nReturn a helpful, direct answer.", agent.name, agent.objective)
  };

  let window: Vec<serde_json::Value> = history.iter().rev().take(ROLLING_WINDOW).cloned().collect::<Vec<_>>().into_iter().rev().collect();
  let mut messages: Vec<serde_json::Value> = vec![serde_json::json!({ "role": "system", "content": system })];
  messages.extend(window);

  // Resolve with a local fallback: if the configured model can't be used (no key
  // or unknown provider), fall back to a local Ollama model so the turn still replies.
  let primary = provider::resolve(&conn, &agent.model, None);
  let fallback_candidates = crate::fallback::local_candidates(&conn);
  let (mut resolved, fallback_note) = crate::fallback::resolve_with_fallback(primary, fallback_candidates.clone(), &agent.reasoning).await?;
  if let Some(note) = &fallback_note { emit_status(&on_event, "think", note); }
  let mut is_ollama = resolved.kind == provider::Kind::Ollama;
  let mut is_anthropic = resolved.kind == provider::Kind::Anthropic;

  // Tool-call rounds (non-streaming) run first, then the final reply streams.
  // Usage is accumulated across every round (and the final stream) so a run's
  // recorded token counts reflect the whole turn, not just the last call.
  let mut prompt_tokens = 0u64;
  let mut completion_tokens = 0u64;
  if !tools.is_empty() {
    let client = http::client();
    for _ in 0..MAX_TOOL_ROUNDS {
      if is_cancelled(stream_id.as_deref()) { return Ok(()); }
      emit_status(&on_event, "think", "Thinking…");
      // Keep only the latest screenshot in the request; older ones are dropped so
      // a vision loop doesn't re-send every image on every round.
      strip_old_images(&mut messages);
      let mut body = build_tool_body(&resolved, &messages, &tools);
      let response = match http::send_model_request(&client, &resolved, &resolved.chat_url(), &body, http::MODEL_ATTEMPTS).await {
        Ok(r) => r,
        Err(e) => match crate::fallback::fallback_resolved(fallback_candidates.clone()).await {
          Some((fb, cand)) => {
            emit_status(&on_event, "think", &format!("Model error ({e}); retrying with local {cand}."));
            resolved = fb; is_ollama = resolved.kind == provider::Kind::Ollama; is_anthropic = resolved.kind == provider::Kind::Anthropic;
            body = build_tool_body(&resolved, &messages, &tools);
            http::send_model_request(&client, &resolved, &resolved.chat_url(), &body, http::MODEL_ATTEMPTS).await?
          }
          None => return Err(e),
        },
      };
      let mut parsed: serde_json::Value = response.json().await.map_err(|e| e.to_string())?;
      if is_anthropic { parsed = crate::anthropic::normalize_response(&parsed); }
      // Ollama reports prompt_eval_count/eval_count; OpenAI-compatible usage.{prompt,completion}_tokens.
      prompt_tokens += parsed["prompt_eval_count"].as_u64()
        .or_else(|| parsed["usage"]["prompt_tokens"].as_u64())
        .unwrap_or(0);
      completion_tokens += parsed["eval_count"].as_u64()
        .or_else(|| parsed["usage"]["completion_tokens"].as_u64())
        .unwrap_or(0);

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
          let output = run_tool(&app, &tools, is_manager, &call.name, &call.args, &on_event, stream_id.as_deref()).await?;
          messages.push(serde_json::json!({ "role": "user", "content": format!("<tool_result name=\"{}\">\n{}\n</tool_result>", call.name, output.text) }));
          push_images(&mut messages, is_ollama, &output.images);
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
        let output = run_tool(&app, &tools, is_manager, &name, &args, &on_event, stream_id.as_deref()).await?;
        let mut tool_msg = serde_json::json!({ "role": "tool", "content": output.text });
        if !call_id.is_empty() { tool_msg["tool_call_id"] = serde_json::json!(call_id); }
        messages.push(tool_msg);
        push_images(&mut messages, is_ollama, &output.images);
      }
    }
  }

  // A cancel that landed during a tool round stops here rather than starting the
  // final reply.
  if is_cancelled(stream_id.as_deref()) { return Ok(()); }

  emit_status(&on_event, "write", "Writing the reply…");
  strip_old_images(&mut messages);
  let client = http::stream_client();
  let mut body = build_stream_body(&resolved, &messages);
  let response = match http::send_model_request(&client, &resolved, &resolved.chat_url(), &body, http::MODEL_ATTEMPTS).await {
    Ok(r) => r,
    Err(e) => match crate::fallback::fallback_resolved(fallback_candidates.clone()).await {
      Some((fb, cand)) => {
        emit_status(&on_event, "think", &format!("Model error ({e}); retrying with local {cand}."));
        resolved = fb; is_ollama = resolved.kind == provider::Kind::Ollama; is_anthropic = resolved.kind == provider::Kind::Anthropic;
        body = build_stream_body(&resolved, &messages);
        http::send_model_request(&client, &resolved, &resolved.chat_url(), &body, http::MODEL_ATTEMPTS).await?
      }
      None => return Err(e),
    },
  };
  let mut stream = response.bytes_stream();
  let mut buffer = String::new();
  let mut delta = String::new();
  // Even the final (tool-less) stream can carry text-protocol markup; never let
  // it reach the chat bubble.
  let mut leak = ToolCallLeakFilter::new();
  loop {
    // A stalled upstream (OpenRouter switching providers, a dropped connection)
    // must surface as an error rather than an indefinite wait. The cancel arm
    // races it so a user cancel takes effect at once, not on the next token.
    let ready = tokio::select! {
      r = tokio::time::timeout(http::STREAM_IDLE_TIMEOUT, stream.next()) => Some(r),
      _ = wait_for_cancel(stream_id.clone()) => None,
    };
    let chunk = match ready {
      None => return Ok(()),
      Some(Err(_)) => return Err(format!("The model provider stopped responding (no data for {}s).", http::STREAM_IDLE_TIMEOUT.as_secs())),
      Some(Ok(None)) => break,
      Some(Ok(Some(chunk))) => chunk.map_err(|e| e.to_string())?,
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
          if is_anthropic {
            // Anthropic SSE: named events carry text deltas, thinking, and usage.
            match v["type"].as_str() {
              Some("content_block_delta") => {
                if let Some(t) = v["delta"]["text"].as_str() {
                  let safe = leak.feed(t);
                  if !safe.is_empty() { delta.push_str(&safe); on_event.send(safe).map_err(|e| e.to_string())?; }
                } else if let Some(t) = v["delta"]["thinking"].as_str() {
                  if !t.is_empty() { emit_reasoning(&on_event, t); }
                }
              }
              Some("message_start") => { prompt_tokens = v["message"]["usage"]["input_tokens"].as_u64().unwrap_or(prompt_tokens); }
              Some("message_delta") => { completion_tokens = v["usage"]["output_tokens"].as_u64().unwrap_or(completion_tokens); }
              _ => {}
            }
          } else {
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
  }
  // Flush whatever the leak filter was still holding back.
  let tail = leak.finish();
  if !tail.is_empty() { delta.push_str(&tail); on_event.send(tail).map_err(|e| e.to_string())?; }

  // Passive capture: this exchange becomes experience in the agent's durable
  // memory — no tool call, no instruction from the user.
  if !delta.trim().is_empty() {
    let app_cap = app.clone();
    let id_cap = agent.id.clone();
    let user_cap = input.clone();
    let reply_cap = delta.clone();
    let project_cap = project_scope.clone();
    let _ = tauri::async_runtime::spawn_blocking(move || crate::fox::record_turn(&app_cap, &id_cap, &user_cap, &reply_cap, project_cap.as_deref()));
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
    if scoped_to_session {
      // The session IS the conversation, so record the turns there and leave the
      // agent-level copy alone — it belongs to the session-less path.
      if let Some(sess) = &session_id {
        let _ = append_session_message(&conn, sess, &agent.id, "user", &input);
        let _ = append_session_message(&conn, sess, &agent.id, "assistant", &delta);
      }
    } else {
      let _ = save_conversation(&conn, &agent.id, &history);
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

// Builds a non-streaming tool-round request body for the given provider.
fn build_tool_body(resolved: &provider::Resolved, messages: &[serde_json::Value], tools: &[Box<dyn AgentTool>]) -> serde_json::Value {
  let mut body = serde_json::json!({ "model": resolved.model, "messages": messages, "tools": tool_schemas(tools), "stream": false });
  let budget = provider::apply_reasoning(&mut body, resolved, http::CHAT_MAX_TOKENS);
  if resolved.kind == provider::Kind::Ollama {
    provider::apply_ollama_options(&mut body, budget);
  } else {
    http::apply_openai_defaults(&mut body, resolved.kind == provider::Kind::OpenRouter, budget);
  }
  if resolved.kind == provider::Kind::Anthropic { body = crate::anthropic::to_wire(&body); }
  body
}

// Builds the final streaming request body for the given provider.
fn build_stream_body(resolved: &provider::Resolved, messages: &[serde_json::Value]) -> serde_json::Value {
  let mut body = serde_json::json!({ "model": resolved.model, "messages": messages, "stream": true });
  let budget = provider::apply_reasoning(&mut body, resolved, http::CHAT_MAX_TOKENS);
  if resolved.kind == provider::Kind::Ollama {
    provider::apply_ollama_options(&mut body, budget);
  } else {
    let is_openrouter = resolved.kind == provider::Kind::OpenRouter;
    http::apply_openai_defaults(&mut body, is_openrouter, budget);
    if is_openrouter { body["usage"] = serde_json::json!({ "include": true }); }
    else { body["stream_options"] = serde_json::json!({ "include_usage": true }); }
  }
  if resolved.kind == provider::Kind::Anthropic { body = crate::anthropic::to_wire(&body); }
  body
}

// Runs a single tool call. State-changing tools go through the confirmation popup
// first; everything else executes immediately.
async fn run_tool(app: &AppHandle, tools: &[Box<dyn AgentTool>], is_manager: bool, name: &str, args: &serde_json::Value, on_event: &tauri::ipc::Channel<String>, stream_id: Option<&str>) -> Result<ToolOutput, String> {
  if !requires_confirmation(name) {
    return Ok(execute_tool(app, tools, is_manager, name, args).await);
  }
  let request_id = format!("req-{}", chrono::Utc::now().timestamp_millis());
  emit_status(on_event, "wait", "Waiting for your approval…");
  let event = serde_json::json!({ "type": "confirm", "requestId": request_id, "tool": name, "args": args });
  on_event.send(event.to_string()).map_err(|e| e.to_string())?;
  let deadline = Instant::now() + Duration::from_secs(120);
  let decision = loop {
    // A cancel while the approval popup is open must not leave this waiting the
    // full deadline; bail out and let the turn unwind.
    if is_cancelled(stream_id) { return Ok(ToolOutput::text("")); }
    {
      let mut map = approvals().lock().map_err(|e| e.to_string())?;
      if let Some(a) = map.remove(&request_id) { break a; }
    }
    if Instant::now() > deadline { break Approval { approved: false, edited_args: serde_json::json!({}) }; }
    tokio::time::sleep(Duration::from_millis(200)).await;
  };
  if !decision.approved { return Ok(ToolOutput::text("The user declined this action.")); }
  Ok(execute_tool(app, tools, is_manager, name, &decision.edited_args).await)
}

async fn execute_tool(app: &AppHandle, tools: &[Box<dyn AgentTool>], is_manager: bool, name: &str, args: &serde_json::Value) -> ToolOutput {
  if is_manager {
    // The Manager's own tools dispatch by name; anything else it holds — MCP tools
    // and the PC control tools — runs like any agent tool (and may return images).
    if !crate::manager::is_manager_builtin(name) {
      if let Some(t) = tools.iter().find(|t| t.name() == name) {
        return t.run_with_images(args).await.unwrap_or_else(|e| ToolOutput::text(format!("Error: {e}")));
      }
    }
    ToolOutput::text(run_manager_tool(app, tools, name, args).await.unwrap_or_else(|e| e))
  } else {
    match tools.iter().find(|t| t.name() == name) {
      Some(t) => t.run_with_images(args).await.unwrap_or_else(|e| ToolOutput::text(format!("Error: {e}"))),
      None => ToolOutput::text(format!("Unknown tool '{name}'.")),
    }
  }
}

// Appends the model-visible image turn for a tool that returned images. OpenAI-
// compatible providers take a content array on a user message; Ollama takes an
// `images` array. Uses a user turn because a `tool` role can't carry images.
fn push_images(messages: &mut Vec<serde_json::Value>, is_ollama: bool, images: &[String]) {
  if images.is_empty() { return; }
  if is_ollama {
    messages.push(serde_json::json!({ "role": "user", "content": "[current screen]", "images": images }));
  } else {
    let mut content = vec![serde_json::json!({ "type": "text", "text": "[current screen]" })];
    for b64 in images {
      content.push(serde_json::json!({ "type": "image_url", "image_url": { "url": format!("data:image/png;base64,{b64}") } }));
    }
    messages.push(serde_json::json!({ "role": "user", "content": content }));
  }
}

// Keeps only the last image-carrying message; earlier ones become text so a
// vision loop doesn't re-send every screenshot on every round.
fn strip_old_images(messages: &mut [serde_json::Value]) {
  let mut last = None;
  for (i, m) in messages.iter().enumerate() {
    if message_has_image(m) { last = Some(i); }
  }
  for (i, m) in messages.iter_mut().enumerate() {
    if Some(i) == last || !message_has_image(m) { continue; }
    if m.get("images").is_some() {
      if let Some(obj) = m.as_object_mut() { obj.remove("images"); }
      m["content"] = serde_json::json!("[previous screen]");
    } else if let Some(arr) = m.get_mut("content").and_then(|v| v.as_array_mut()) {
      arr.retain(|p| p.get("type").and_then(|t| t.as_str()) != Some("image_url"));
      if arr.is_empty() { m["content"] = serde_json::json!("[previous screen]"); }
    }
  }
}

fn message_has_image(m: &serde_json::Value) -> bool {
  if let Some(a) = m.get("images").and_then(|v| v.as_array()) {
    if !a.is_empty() { return true; }
  }
  m.get("content").and_then(|v| v.as_array())
    .map(|a| a.iter().any(|p| p.get("type").and_then(|t| t.as_str()) == Some("image_url")))
    .unwrap_or(false)
}
