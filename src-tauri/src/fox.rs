// Host-side driver for the built-in memory connector (fox / `agent-memory`).
//
// fox is the durable layer: a keyed self-model plus a searchable store of typed
// memories. By design it does NOT push anything on its own — recall is pull, not
// push — so lup, as the host, drives it:
//
//   * capture  — every turn becomes experience (events), with no tool call and no
//                instruction from the user;
//   * active    — a lean, token-budgeted context block is compiled and injected
//     context     into the prompt each turn, so memory is present without the
//                model having to ask for it;
//   * distill  — a finished chat is distilled into durable typed memories and
//                self-model entries (done in `memory.rs`), so the agent's sense
//                of self evolves from use.
//
// Keeping all durable state here (not inline in the window) is what lets active
// context stay lean and survive the host's own compaction.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

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

// The memory ids that made it into the last injected context, per agent, so a
// good/bad signal can reinforce or dampen exactly what informed the reply.
fn last_context() -> &'static Mutex<HashMap<String, Vec<String>>> {
  static LAST: OnceLock<Mutex<HashMap<String, Vec<String>>>> = OnceLock::new();
  LAST.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Compiles a lean active-context block for a turn and remembers which memories
/// fed it. Returns the markdown to inject (empty when the connector is absent).
pub(crate) fn active_context(app: &AppHandle, agent_id: &str) -> String {
  let args = serde_json::json!({
    "agentId": agent_id,
    "scope": scope(agent_id),
    "scopeMode": "inherit",
    "tokenBudget": 600,
    "includeSelf": true,
    "includePolicies": true,
    "includeProcedures": true,
    "includeSemantic": true,
    "includePreferences": true,
    "includeRecentEpisodes": true,
  });
  let Ok(value) = call(app, "compile_context", args) else { return String::new() };

  let mut ids = Vec::new();
  if let Some(sections) = value.get("sections").and_then(|s| s.as_array()) {
    for section in sections {
      if let Some(items) = section.get("items").and_then(|i| i.as_array()) {
        for item in items {
          // Only real memories can be reinforced; events/self/task items have ids
          // that `update_memory` can't address.
          let kind = item.get("kind").and_then(|k| k.as_str()).unwrap_or("");
          let is_memory = matches!(kind, "episodic" | "semantic" | "procedural" | "policy" | "preference" | "reflection");
          if is_memory {
            if let Some(id) = item.get("id").and_then(|i| i.as_str()) {
              ids.push(id.to_string());
            }
          }
        }
      }
    }
  }
  if let Ok(mut map) = last_context().lock() { map.insert(agent_id.to_string(), ids); }

  value.get("markdown").and_then(|m| m.as_str()).unwrap_or("").to_string()
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

/// A good/bad signal on a reply: reinforce or dampen the memories that informed
/// it, record the outcome as experience, and reflect on a bad one.
pub(crate) fn record_feedback(app: &AppHandle, agent_id: &str, signal: &str, content: &str) {
  let s = scope(agent_id);
  let important = signal == "down";
  let _ = call(app, "record_event", serde_json::json!({
    "agentId": agent_id, "scope": s, "type": "outcome", "role": "user",
    "content": if content.trim().is_empty() { format!("user feedback: {signal}") } else { content.to_string() },
    "data": { "signal": signal },
    "importance": if important { 0.85 } else { 0.55 },
  }));

  // Nudge the memories that fed the reply: up → a little more important, down →
  // less. Values are absolute (fox patches are absolute); a move toward 0.95/0.1
  // is enough to shift ranking without knowing the prior value.
  let ids = last_context().lock().ok().and_then(|m| m.get(agent_id).cloned()).unwrap_or_default();
  let bump = if signal == "down" { 0.1 } else { 0.95 };
  for id in ids {
    let _ = call(app, "update_memory", serde_json::json!({ "id": id, "patch": { "importance": bump } }));
  }

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
