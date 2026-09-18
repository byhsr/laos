// Per-agent memory + context: persisted conversations, rolling-window
// summaries, long-term memory facts, chat sessions, and the context bundle
// used by chat and the Manager.

use rusqlite::{params, Connection};
use tauri::AppHandle;

use crate::db::db;
use crate::http;
use crate::storage::stored_api_key;

pub(crate) const ROLLING_WINDOW: usize = 12; // max messages (user+assistant) sent as context
pub(crate) const MEMORY_SUMMARY_KEY: &str = "__summary__";
const MEMORY_WATERMARK_KEY: &str = "__summary_upto__";
// Cap on the transcript handed to the summarizer, so one call stays bounded.
const MAX_SUMMARY_INPUT_CHARS: usize = 6000;

pub(crate) fn load_memory(conn: &Connection, agent_id: &str) -> Vec<(String, String)> {
  let stmt = conn.prepare("SELECT key, value FROM memory WHERE agent_id=?1 ORDER BY updated_at DESC LIMIT 20").ok();
  let mut out = Vec::new();
  if let Some(mut stmt) = stmt {
    if let Ok(rows) = stmt.query_map(params![agent_id], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))) {
      for r in rows { if let Ok(pair) = r { out.push(pair); } }
    }
  }
  out
}

// Non-streaming single-shot completion used for summarization. Returns the text.
async fn one_shot_completion(app: &AppHandle, model: &str, prompt: &str) -> Result<String, String> {
  let client = http::client();
  if let Some(m) = model.strip_prefix("ollama:") {
    let response = client.post("http://127.0.0.1:11434/api/generate")
      .json(&serde_json::json!({ "model": m, "prompt": prompt, "stream": false }))
      .send().await.map_err(|e| e.to_string())?;
    let json: serde_json::Value = response.json().await.map_err(|e| e.to_string())?;
    Ok(json["response"].as_str().unwrap_or("").to_string())
  } else {
    let conn = db(app)?;
    let (url, api_model, key, is_openrouter) = if let Some(m) = model.strip_prefix("groq:") {
      let k = stored_api_key(&conn, model).ok().flatten().ok_or("Groq requires an API key.")?;
      ("https://api.groq.com/openai/v1/chat/completions".to_string(), m.to_string(), k, false)
    } else if let Some(m) = model.strip_prefix("openrouter:") {
      let k = stored_api_key(&conn, model).ok().flatten().ok_or("OpenRouter requires an API key.")?;
      ("https://openrouter.ai/api/v1/chat/completions".to_string(), m.to_string(), k, true)
    } else {
      return Err(format!("Unsupported model provider for '{model}'."));
    };
    let mut body = serde_json::json!({ "model": api_model, "messages": [{ "role": "user", "content": prompt }] });
    http::apply_openai_defaults(&mut body, is_openrouter, http::SUMMARY_MAX_TOKENS);
    let response = http::send_with_retry(|| {
      client.post(&url).header("Authorization", format!("Bearer {key}")).json(&body)
    }, http::MODEL_ATTEMPTS).await?;
    let json: serde_json::Value = response.json().await.map_err(|e| e.to_string())?;
    Ok(json["choices"][0]["message"]["content"].as_str().unwrap_or("").to_string())
  }
}

// Folds the turns that have scrolled out of the rolling window into the agent's
// long-term memory. Only the turns added since the last pass are sent (tracked by
// a watermark) and the previous summary is carried forward, so the work per turn
// stays bounded instead of re-summarizing the whole conversation every time.
pub(crate) async fn summarize_old_turns(app: &AppHandle, agent_id: &str, model: &str, history: &[serde_json::Value]) -> Result<(), String> {
  let conn = db(app)?;
  let old_end = history.len().saturating_sub(ROLLING_WINDOW);
  let upto = load_summary_watermark(&conn, agent_id).min(old_end);
  // Nothing new has left the window (or only a single message has) — not worth a call.
  if old_end <= upto + 1 { return Ok(()); }

  let mut transcript = String::new();
  for m in &history[upto..old_end] {
    let role = m.get("role").and_then(|r| r.as_str()).unwrap_or("");
    let content = m.get("content").and_then(|c| c.as_str()).unwrap_or("");
    if content.is_empty() { continue; }
    transcript.push_str(&format!("{role}: {content}\n"));
    if transcript.len() >= MAX_SUMMARY_INPUT_CHARS { break; }
  }
  transcript.truncate(MAX_SUMMARY_INPUT_CHARS);
  if transcript.trim().is_empty() { return Ok(()); }

  let previous: String = conn
    .query_row("SELECT value FROM memory WHERE agent_id=?1 AND key=?2", params![agent_id, MEMORY_SUMMARY_KEY], |r| r.get(0))
    .unwrap_or_default();

  let prompt = format!(
    "You maintain a running memory summary for an assistant.\n\nExisting summary:\n{previous}\n\nNew turns to fold in:\n{transcript}\n\nReturn an updated summary (max 150 words) capturing decisions, requests, results and anything important to remember, then 2-5 durable facts.\nReply as:\nSUMMARY: <summary>\nFACTS:\n- fact1\n- fact2"
  );
  if let Ok(reply) = one_shot_completion(app, model, &prompt).await {
    let (summary, facts) = match reply.split_once("FACTS:") {
      Some((s, f)) => (s.trim().trim_start_matches("SUMMARY:").trim().to_string(), f.to_string()),
      None => (reply.trim().to_string(), String::new()),
    };
    let ts = chrono::Utc::now().to_rfc3339();
    if !summary.is_empty() {
      let _ = conn.execute(
        "INSERT INTO memory (agent_id, key, value, updated_at) VALUES (?1,?2,?3,?4)
         ON CONFLICT(agent_id,key) DO UPDATE SET value=excluded.value, updated_at=excluded.updated_at",
        params![agent_id, MEMORY_SUMMARY_KEY, summary, ts],
      );
    }
    for line in facts.lines() {
      let fact = line.trim().trim_start_matches('-').trim();
      if fact.is_empty() { continue; }
      let key = format!("fact:{}", fact.chars().take(40).collect::<String>());
      let _ = conn.execute(
        "INSERT INTO memory (agent_id, key, value, updated_at) VALUES (?1,?2,?3,?4)
         ON CONFLICT(agent_id,key) DO UPDATE SET value=excluded.value, updated_at=excluded.updated_at",
        params![agent_id, key, fact, ts],
      );
    }
    save_summary_watermark(&conn, agent_id, old_end);
  }
  Ok(())
}

// Number of leading messages already folded into the memory summary.
fn load_summary_watermark(conn: &Connection, agent_id: &str) -> usize {
  conn.query_row("SELECT value FROM memory WHERE agent_id=?1 AND key=?2", params![agent_id, MEMORY_WATERMARK_KEY], |r| r.get::<_, String>(0))
    .ok()
    .and_then(|v| v.parse::<usize>().ok())
    .unwrap_or(0)
}

fn save_summary_watermark(conn: &Connection, agent_id: &str, upto: usize) {
  let _ = conn.execute(
    "INSERT INTO memory (agent_id, key, value, updated_at) VALUES (?1,?2,?3,?4)
     ON CONFLICT(agent_id,key) DO UPDATE SET value=excluded.value, updated_at=excluded.updated_at",
    params![agent_id, MEMORY_WATERMARK_KEY, upto.to_string(), chrono::Utc::now().to_rfc3339()],
  );
}

pub(crate) fn load_conversation(conn: &Connection, agent_id: &str) -> Vec<serde_json::Value> {
  let stmt = conn.prepare("SELECT messages FROM agent_conversations WHERE agent_id=?1").ok();
  if let Some(mut stmt) = stmt {
    let rows = stmt.query_map(params![agent_id], |row| row.get::<_, String>(0)).ok();
    if let Some(mut rows) = rows {
      if let Some(Ok(s)) = rows.next() {
        if let Ok(v) = serde_json::from_str::<Vec<serde_json::Value>>(&s) { return v; }
      }
    }
  }
  Vec::new()
}

pub(crate) fn save_conversation(conn: &Connection, agent_id: &str, messages: &[serde_json::Value]) -> Result<(), String> {
  conn.execute(
    "INSERT INTO agent_conversations (agent_id, messages) VALUES (?1,?2)
     ON CONFLICT(agent_id) DO UPDATE SET messages=excluded.messages",
    params![agent_id, serde_json::to_string(messages).map_err(|e| e.to_string())?],
  ).map_err(|e| e.to_string())?;
  Ok(())
}

#[tauri::command]
pub fn get_conversation(app: AppHandle, agent_id: String) -> Result<Vec<serde_json::Value>, String> {
  let conn = db(&app)?;
  Ok(load_conversation(&conn, &agent_id))
}

#[tauri::command]
pub fn clear_agent_memory(app: AppHandle, agent_id: String) -> Result<(), String> {
  let conn = db(&app)?;
  conn.execute("DELETE FROM memory WHERE agent_id=?1", params![agent_id]).map_err(|e| e.to_string())?;
  conn.execute("DELETE FROM agent_conversations WHERE agent_id=?1", params![agent_id]).map_err(|e| e.to_string())?;
  Ok(())
}

// ---------------------------------------------------------------------------
// Chat sessions (bifurcated history per agent)
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn list_chat_sessions(app: AppHandle, agent_id: String) -> Result<Vec<serde_json::Value>, String> {
  let conn = db(&app)?;
  let mut stmt = conn.prepare("SELECT id, title, created_at, updated_at FROM chat_sessions WHERE agent_id=?1 ORDER BY updated_at DESC LIMIT 100").map_err(|e| e.to_string())?;
  let rows = stmt.query_map(params![agent_id], |row| {
    Ok(serde_json::json!({ "id": row.get::<_, String>(0)?, "title": row.get::<_, String>(1)?, "createdAt": row.get::<_, String>(2)?, "updatedAt": row.get::<_, String>(3)? }))
  }).map_err(|e| e.to_string())?;
  let mut out = Vec::new();
  for row in rows { out.push(row.map_err(|e| e.to_string())?); }
  Ok(out)
}

#[tauri::command]
pub fn get_chat_session(app: AppHandle, session_id: String) -> Result<Vec<serde_json::Value>, String> {
  let conn = db(&app)?;
  let mut stmt = conn.prepare("SELECT role, content, time FROM chat_messages WHERE session_id=?1 ORDER BY id").map_err(|e| e.to_string())?;
  let rows = stmt.query_map(params![session_id], |row| {
    Ok(serde_json::json!({ "role": row.get::<_, String>(0)?, "content": row.get::<_, String>(1)?, "time": row.get::<_, String>(2)? }))
  }).map_err(|e| e.to_string())?;
  let mut out = Vec::new();
  for row in rows { out.push(row.map_err(|e| e.to_string())?); }
  Ok(out)
}

#[tauri::command]
pub fn create_chat_session(app: AppHandle, agent_id: String, title: String) -> Result<serde_json::Value, String> {
  let conn = db(&app)?;
  let id = format!("sess-{}", chrono::Utc::now().timestamp_millis());
  let now = chrono::Utc::now().to_rfc3339();
  let title = if title.trim().is_empty() { "Chat".to_string() } else { title };
  conn.execute("INSERT INTO chat_sessions (id, agent_id, title, created_at, updated_at) VALUES (?1,?2,?3,?4,?4)", params![id, agent_id, title, now]).map_err(|e| e.to_string())?;
  Ok(serde_json::json!({ "id": id, "title": title, "createdAt": now, "updatedAt": now }))
}

#[tauri::command]
pub fn delete_chat_session(app: AppHandle, session_id: String) -> Result<(), String> {
  let conn = db(&app)?;
  conn.execute("DELETE FROM chat_messages WHERE session_id=?1", params![session_id]).map_err(|e| e.to_string())?;
  conn.execute("DELETE FROM chat_sessions WHERE id=?1", params![session_id]).map_err(|e| e.to_string())?;
  Ok(())
}

#[tauri::command]
pub fn rename_chat_session(app: AppHandle, session_id: String, title: String) -> Result<(), String> {
  let conn = db(&app)?;
  conn.execute("UPDATE chat_sessions SET title=?1 WHERE id=?2", params![title, session_id]).map_err(|e| e.to_string())?;
  Ok(())
}

// Appends a message to a session (creates the session if missing).
pub(crate) fn append_session_message(conn: &Connection, session_id: &str, agent_id: &str, role: &str, content: &str) -> Result<(), String> {
  let now = chrono::Utc::now().to_rfc3339();
  conn.execute("INSERT INTO chat_sessions (id, agent_id, title, created_at, updated_at) VALUES (?1,?2,'Chat',?3,?3) ON CONFLICT(id) DO NOTHING", params![session_id, agent_id, now]).map_err(|e| e.to_string())?;
  conn.execute("INSERT INTO chat_messages (session_id, role, content, time) VALUES (?1,?2,?3,'')", params![session_id, role, content]).map_err(|e| e.to_string())?;
  conn.execute("UPDATE chat_sessions SET updated_at=?1 WHERE id=?2", params![now, session_id]).map_err(|e| e.to_string())?;
  Ok(())
}

fn today_key() -> String { chrono::Local::now().format("%Y-%m-%d").to_string() }

fn yesterday_key() -> String { (chrono::Local::now() - chrono::Duration::days(1)).format("%Y-%m-%d").to_string() }

// Loads the memory facts ("brain") for an agent, excluding the summary pseudo-key.
fn load_memory_facts(conn: &Connection, agent_id: &str) -> Vec<(String, String)> {
  load_memory(conn, agent_id).into_iter().filter(|(k, _)| !k.starts_with("__")).collect()
}

// Summarizes a closed chat session (via the model), stores the summary on the
// session, and folds it into the agent's day context. Returns the summary text.
async fn close_chat_session(app: &AppHandle, session_id: &str, agent_id: &str, model: &str) -> Result<String, String> {
  let conn = db(app)?;
  let transcript: Vec<String> = {
    let mut stmt = conn.prepare("SELECT role, content FROM chat_messages WHERE session_id=?1 ORDER BY id").map_err(|e| e.to_string())?;
    let rows = stmt.query_map(params![session_id], |row| Ok(format!("{}: {}", row.get::<_, String>(0)?, row.get::<_, String>(1)?))).map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for r in rows { out.push(r.map_err(|e| e.to_string())?); }
    out
  };
  if transcript.is_empty() { return Ok(String::new()); }
  let joined = transcript.join("\n");
  let prompt = format!(
    "Summarize this chat conversation as a compact memory for the agent. Capture what was discussed, decided, requested, and any open threads. Max 200 words.\n\nConversation:\n{joined}"
  );
  let summary = one_shot_completion(app, model, &prompt).await.unwrap_or_default();
  if !summary.is_empty() {
    let day = today_key();
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute("UPDATE chat_sessions SET summary=?1 WHERE id=?2", params![summary, session_id]).map_err(|e| e.to_string())?;
    // Fold into today's day-context (append, capped).
    let existing: String = conn.query_row("SELECT summary FROM day_contexts WHERE agent_id=?1 AND day=?2", params![agent_id, day], |r| r.get(0)).unwrap_or_default();
    let merged = if existing.is_empty() { summary.clone() } else { format!("{existing}\n{summary}") };
    let merged = merged.chars().take(3000).collect::<String>();
    conn.execute(
      "INSERT INTO day_contexts (agent_id, day, summary, updated_at) VALUES (?1,?2,?3,?4) ON CONFLICT(agent_id,day) DO UPDATE SET summary=excluded.summary, updated_at=excluded.updated_at",
      params![agent_id, day, merged, now],
    ).map_err(|e| e.to_string())?;
  }
  Ok(summary)
}

#[tauri::command]
pub async fn close_session(app: AppHandle, session_id: String, agent_id: String, model: String) -> Result<(), String> {
  let _ = close_chat_session(&app, &session_id, &agent_id, &model).await;
  Ok(())
}

// Assembles the full context bundle: memory facts, last chat summary, today's
// context, yesterday's context. Injected before the rolling window each turn.
pub(crate) fn build_context_bundle(conn: &Connection, agent_id: &str) -> Result<String, String> {
  let facts = load_memory_facts(conn, agent_id);
  let mut out = String::new();
  if !facts.is_empty() {
    out.push_str("## Long-term memory (facts learned about you):\n");
    for (k, v) in &facts {
      let label = k.strip_prefix("fact:").unwrap_or(k);
      out.push_str(&format!("- {label}: {v}\n"));
    }
  }
  // Last chat's condensed summary.
  let last_chat: Option<String> = conn.query_row(
    "SELECT summary FROM chat_sessions WHERE agent_id=?1 AND summary != '' ORDER BY updated_at DESC LIMIT 1",
    params![agent_id], |r| r.get(0)).ok();
  if let Some(s) = last_chat { if !s.is_empty() { out.push_str(&format!("\n## Last chat summary:\n{s}\n")); } }
  // Today's context.
  let today: Option<String> = conn.query_row("SELECT summary FROM day_contexts WHERE agent_id=?1 AND day=?2", params![agent_id, today_key()], |r| r.get(0)).ok();
  if let Some(s) = today { if !s.is_empty() { out.push_str(&format!("\n## Today's context:\n{s}\n")); } }
  // Yesterday's context.
  let yday: Option<String> = conn.query_row("SELECT summary FROM day_contexts WHERE agent_id=?1 AND day=?2", params![agent_id, yesterday_key()], |r| r.get(0)).ok();
  if let Some(s) = yday { if !s.is_empty() { out.push_str(&format!("\n## Yesterday's context:\n{s}\n")); } }
  Ok(out)
}
