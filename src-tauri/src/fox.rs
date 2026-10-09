// Host-side driver for the built-in memory connector (fox / `agent-memory`).
//
// fox is the durable layer: a keyed self-model plus a searchable store of typed
// memories. By design it does NOT push anything on its own — recall is pull, not
// push — so lup, as the host, drives it:
//
//   * capture  — every turn becomes experience (events), with no tool call and no
//                instruction from the user;
//   * distill  — a finished chat is distilled into durable typed memories and
//                self-model entries (done in `memory.rs`), so the agent's sense
//                of self evolves from use.
//
// Recall is pull: nothing here is injected into the prompt — the model asks for
// it (a memory tool) when it needs it, and the Memory view reads it directly.

use tauri::AppHandle;

use crate::db::db;
use crate::mcp;

const MEMORY_SERVER_ID: &str = "memory";

fn call(app: &AppHandle, tool: &str, args: serde_json::Value) -> Result<serde_json::Value, String> {
  let conn = db(app)?;
  let server = mcp::load_server(&conn, MEMORY_SERVER_ID)?;
  let text = mcp::call_tool(&server, tool, args)?;
  Ok(serde_json::from_str(&text).unwrap_or(serde_json::Value::String(text)))
}

// Every fox call is scoped to the agent's own namespace, matching the Memory view.
fn scope(agent_id: &str) -> serde_json::Value {
  serde_json::json!({ "type": "agent", "id": agent_id })
}

/// Records both sides of a finished turn as experience. Best-effort.
pub(crate) fn record_turn(app: &AppHandle, agent_id: &str, user: &str, assistant: &str) {
  let s = scope(agent_id);
  if !user.trim().is_empty() {
    let _ = call(app, "record_event", serde_json::json!({
      "agentId": agent_id, "scope": s, "type": "user_message", "role": "user", "content": user,
    }));
  }
  if !assistant.trim().is_empty() {
    let _ = call(app, "record_event", serde_json::json!({
      "agentId": agent_id, "scope": s, "type": "assistant_message", "role": "assistant", "content": assistant,
    }));
  }
}

/// Writes the memories + self-model entries produced by a distillation pass.
/// Returns how many items were stored.
pub(crate) fn write_distilled(app: &AppHandle, agent_id: &str, distilled: &serde_json::Value) -> usize {
  let s = scope(agent_id);
  let mut stored = 0;

  if let Some(entries) = distilled.get("self").and_then(|v| v.as_array()) {
    for e in entries {
      let key = e.get("key").and_then(|x| x.as_str()).unwrap_or("").trim();
      let value = e.get("value").and_then(|x| x.as_str()).unwrap_or("").trim();
      if key.is_empty() || value.is_empty() { continue; }
      if call(app, "set_self_model", serde_json::json!({ "agentId": agent_id, "key": key, "value": value })).is_ok() {
        stored += 1;
      }
    }
  }

  if let Some(memories) = distilled.get("memories").and_then(|v| v.as_array()) {
    for m in memories {
      let title = m.get("title").and_then(|x| x.as_str()).unwrap_or("").trim();
      let content = m.get("content").and_then(|x| x.as_str()).unwrap_or("").trim();
      if title.is_empty() || content.is_empty() { continue; }
      let kind = m.get("kind").and_then(|x| x.as_str()).unwrap_or("semantic");
      let mut args = serde_json::json!({
        "agentId": agent_id, "scope": s, "kind": kind, "title": title, "content": content,
      });
      if let Some(tags) = m.get("tags") { if tags.is_array() { args["tags"] = tags.clone(); } }
      if let Some(c) = m.get("confidence").and_then(|x| x.as_f64()) { args["confidence"] = serde_json::json!(c.clamp(0.0, 1.0)); }
      if let Some(i) = m.get("importance").and_then(|x| x.as_f64()) { args["importance"] = serde_json::json!(i.clamp(0.0, 1.0)); }
      if call(app, "add_memory", args).is_ok() { stored += 1; }
    }
  }

  stored
}

/// A good/bad signal on a reply: record the outcome as experience, and reflect on
/// a bad one (recurring failures become procedures/policies).
pub(crate) fn record_feedback(app: &AppHandle, agent_id: &str, signal: &str, content: &str) {
  let s = scope(agent_id);
  let important = signal == "down";
  let _ = call(app, "record_event", serde_json::json!({
    "agentId": agent_id, "scope": s, "type": "outcome", "role": "user",
    "content": if content.trim().is_empty() { format!("user feedback: {signal}") } else { content.to_string() },
    "data": { "signal": signal },
    "importance": if important { 0.85 } else { 0.55 },
  }));

  if important {
    let _ = call(app, "reflect", serde_json::json!({ "agentId": agent_id, "scope": s, "persist": true }));
  }
}

/// Explicit good/bad signal from the chat UI.
#[tauri::command]
pub async fn rate_turn(app: AppHandle, agent_id: String, signal: String, content: Option<String>) {
  let text = content.unwrap_or_default();
  let _ = tauri::async_runtime::spawn_blocking(move || record_feedback(&app, &agent_id, &signal, &text)).await;
}
