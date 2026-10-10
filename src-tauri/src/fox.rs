// Host-side memory driver. Memory used to be the fox/`agent-memory` MCP server;
// it is now the in-process store in `agent_memory.rs` — no child process, no
// JSON-RPC. This module is the host policy on top of it:
//
//   * capture  — every turn becomes experience (events), with no tool call;
//   * distill  — a finished chat is distilled into durable typed memories and
//                self-model entries (done in `memory.rs`).
//
// Recall is pull: nothing here is injected into the prompt.

use serde_json::Value;
use tauri::AppHandle;

use crate::agent_memory as am;
use crate::db::db;

/// Registers a project as a namespace so its memory is tracked together.
pub(crate) fn register_namespace(app: &AppHandle, id: &str, name: &str, description: &str) {
  if let Ok(conn) = db(app) {
    let _ = am::create_namespace(&conn, &am::scope_key("project", id), name, description);
  }
}

/// Unregisters a project's namespace (its memory is left in place).
pub(crate) fn remove_namespace(app: &AppHandle, id: &str) {
  if let Ok(conn) = db(app) {
    let _ = am::remove_namespace(&conn, &am::scope_key("project", id));
  }
}

/// Records both sides of a finished turn as experience. When the session belongs
/// to a project, it is filed under the project's scope.
pub(crate) fn record_turn(app: &AppHandle, agent_id: &str, user: &str, assistant: &str, project_id: Option<&str>) {
  let scope = match project_id {
    Some(p) if !p.trim().is_empty() => am::scope_key("project", p),
    _ => am::scope_key("agent", agent_id),
  };
  let Ok(conn) = db(app) else { return };
  let empty = serde_json::json!({});
  if !user.trim().is_empty() {
    let _ = am::record_event(&conn, agent_id, &scope, "user_message", "user", user, &empty, 0.5);
  }
  if !assistant.trim().is_empty() {
    let _ = am::record_event(&conn, agent_id, &scope, "assistant_message", "assistant", assistant, &empty, 0.5);
  }
}

/// Writes the memories + self-model entries produced by a distillation pass.
/// Returns how many items were stored.
pub(crate) fn write_distilled(app: &AppHandle, agent_id: &str, distilled: &Value) -> usize {
  let scope = am::scope_key("agent", agent_id);
  let Ok(conn) = db(app) else { return 0 };
  let mut stored = 0;

  if let Some(entries) = distilled.get("self").and_then(|v| v.as_array()) {
    for e in entries {
      let key = e.get("key").and_then(|x| x.as_str()).unwrap_or("").trim();
      let value = e.get("value").and_then(|x| x.as_str()).unwrap_or("").trim();
      if key.is_empty() || value.is_empty() { continue; }
      if am::set_self_model(&conn, agent_id, &scope, key, value, 0.8, 0.7).is_ok() { stored += 1; }
    }
  }

  if let Some(memories) = distilled.get("memories").and_then(|v| v.as_array()) {
    for m in memories {
      let title = m.get("title").and_then(|x| x.as_str()).unwrap_or("").trim();
      let content = m.get("content").and_then(|x| x.as_str()).unwrap_or("").trim();
      if title.is_empty() || content.is_empty() { continue; }
      let kind = m.get("kind").and_then(|x| x.as_str()).unwrap_or("semantic");
      let tags = m.get("tags").cloned().unwrap_or_else(|| serde_json::json!([]));
      let confidence = m.get("confidence").and_then(|x| x.as_f64()).unwrap_or(0.8);
      let importance = m.get("importance").and_then(|x| x.as_f64()).unwrap_or(0.5);
      if am::add_memory(&conn, agent_id, &scope, kind, title, content, &tags, confidence, importance).is_ok() { stored += 1; }
    }
  }

  stored
}

/// A good/bad signal on a reply: record the outcome, and reflect on a bad one.
pub(crate) fn record_feedback(app: &AppHandle, agent_id: &str, signal: &str, content: &str) {
  let scope = am::scope_key("agent", agent_id);
  let Ok(conn) = db(app) else { return };
  let important = signal == "down";
  let text = if content.trim().is_empty() { format!("user feedback: {signal}") } else { content.to_string() };
  let _ = am::record_event(&conn, agent_id, &scope, "outcome", "user", &text, &serde_json::json!({ "signal": signal }), if important { 0.85 } else { 0.55 });
  if important {
    let _ = am::reflect(&conn, agent_id, &scope, true);
  }
}

/// Explicit good/bad signal from the chat UI.
#[tauri::command]
pub async fn rate_turn(app: AppHandle, agent_id: String, signal: String, content: Option<String>) {
  let text = content.unwrap_or_default();
  let _ = tauri::async_runtime::spawn_blocking(move || record_feedback(&app, &agent_id, &signal, &text)).await;
}
