// Model fallback: when the configured model can't be used — no API key, unknown
// provider, or a request that fails before it starts — fall back to a local
// Ollama model, starting Ollama and pulling the model if necessary, so a chat
// turn still gets a reply.

use rusqlite::Connection;

use crate::http;
use crate::provider::{self, Reasoning, Resolved};

// The smallest sensible built-in local model to reach for last.
const DEFAULT_LOCAL: &str = "ollama:qwen3:8b";

/// Local fallback candidates, best first: enabled Ollama models, then the default.
pub(crate) fn local_fallback_ids(conn: &Connection) -> Vec<String> {
  let mut out = Vec::new();
  if let Ok(mut stmt) = conn.prepare("SELECT id FROM model_configs WHERE enabled=1 AND provider='ollama' ORDER BY id") {
    if let Ok(rows) = stmt.query_map([], |r| r.get::<_, String>(0)) {
      for r in rows { if let Ok(id) = r { out.push(id); } }
    }
  }
  if !out.iter().any(|id| id == DEFAULT_LOCAL) { out.push(DEFAULT_LOCAL.to_string()); }
  out
}

async fn ollama_up(host: &str) -> bool {
  let url = format!("{}/api/tags", host.trim_end_matches('/'));
  matches!(http::client().get(&url).send().await, Ok(r) if r.status().is_success())
}

/// Starts `ollama serve` when it isn't reachable, then waits for it to come up.
async fn ensure_ollama(host: &str) -> Result<(), String> {
  if ollama_up(host).await { return Ok(()); }
  let _ = spawn_ollama();
  for _ in 0..20 {
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    if ollama_up(host).await { return Ok(()); }
  }
  Err("Ollama isn't running and could not be started. Install Ollama, then retry.".into())
}

// Best-effort background start; if Ollama is already starting, the spawn just fails.
fn spawn_ollama() -> std::io::Result<std::process::Child> {
  #[cfg(windows)]
  {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    std::process::Command::new("cmd").args(["/C", "ollama serve"]).creation_flags(CREATE_NO_WINDOW).spawn()
  }
  #[cfg(not(windows))]
  { std::process::Command::new("sh").args(["-c", "ollama serve"]).spawn() }
}

/// Pulls the model if it isn't present (best-effort; drains the pull stream so it
/// actually completes).
async fn ensure_model(host: &str, model: &str) -> Result<(), String> {
  let base = host.trim_end_matches('/');
  let tags: serde_json::Value = match http::client().get(format!("{base}/api/tags")).send().await {
    Ok(r) => r.json().await.unwrap_or(serde_json::Value::Null),
    Err(_) => serde_json::Value::Null,
  };
  let present = tags.get("models").and_then(|m| m.as_array()).map(|a| a.iter().any(|m| {
    m.get("name").and_then(|n| n.as_str()).map(|n| n == model || n.starts_with(&format!("{model}:"))).unwrap_or(false)
  })).unwrap_or(false);
  if present { return Ok(()); }

  let resp = http::stream_client()
    .post(format!("{base}/api/pull"))
    .json(&serde_json::json!({ "name": model, "stream": true }))
    .send().await.map_err(|e| e.to_string())?;
  if !resp.status().is_success() { return Err(format!("Could not pull {model}: {}", resp.status())); }
  use futures_util::StreamExt;
  let mut stream = resp.bytes_stream();
  while let Some(Ok(_)) = stream.next().await {}
  Ok(())
}

// Resolves the local candidates (sync, so callers can do it before any await —
// a rusqlite Connection reference is not Send, so it must not cross an await).
pub(crate) fn local_candidates(conn: &Connection) -> Vec<(Resolved, String)> {
  local_fallback_ids(conn).into_iter()
    .filter_map(|cand| provider::resolve(conn, &cand, None).ok().map(|r| (r, cand)))
    .collect()
}

// Ensures Ollama + the model are available for the first candidate that works.
async fn pick_ready(candidates: Vec<(Resolved, String)>) -> Option<(Resolved, String)> {
  for (mut r, cand) in candidates {
    if ensure_ollama(&r.base).await.is_err() { continue; }
    let _ = ensure_model(&r.base, &r.model).await; // best-effort
    r.reasoning = Reasoning::Auto;
    return Some((r, cand));
  }
  None
}

/// Resolves the agent's model, or a local fallback when it can't be used.
/// Returns the resolved model plus a note when a fallback was taken.
pub(crate) async fn resolve_with_fallback(primary: Result<Resolved, String>, candidates: Vec<(Resolved, String)>, reasoning: &str) -> Result<(Resolved, Option<String>), String> {
  match primary {
    Ok(mut r) => { provider::apply_agent_reasoning(&mut r, reasoning); Ok((r, None)) }
    Err(primary_err) => match pick_ready(candidates).await {
      Some((r, cand)) => Ok((r, Some(format!("Configured model unavailable ({primary_err}); using local {cand}.")))),
      None => Err(primary_err),
    },
  }
}

/// A local fallback to switch to after a request fails at runtime.
pub(crate) async fn fallback_resolved(candidates: Vec<(Resolved, String)>) -> Option<(Resolved, String)> {
  pick_ready(candidates).await
}
