// Every chat gets its own pad: the agent's private working area for thinking,
// drafts and intermediate findings, so its own notes never land in the user's
// notes. One row per scope — `session:<id>` for a chat, `agent:<id>` for a turn
// that has no session (one-shot runs, internal loops).

use rusqlite::{params, Connection};
use tauri::AppHandle;

use crate::db::db;

// Past this the pad is trimmed from the front: it is a working area, not an archive.
const MAX_PAD: usize = 24_000;
// How much of the pad a turn's system prompt carries. `read` returns all of it.
const PROMPT_CHARS: usize = 4_000;

pub(crate) fn scope_for(session_id: Option<&str>, agent_id: &str) -> String {
  match session_id.map(str::trim).filter(|s| !s.is_empty()) {
    Some(sid) => format!("session:{sid}"),
    None => format!("agent:{agent_id}"),
  }
}

pub(crate) fn read(conn: &Connection, scope: &str) -> String {
  conn.query_row("SELECT content FROM scratchpads WHERE scope=?1", params![scope], |r| r.get::<_, String>(0))
    .unwrap_or_default()
}

// The last `max` characters, with a marker when something was dropped from the front.
fn tail(content: &str, max: usize) -> String {
  let len = content.chars().count();
  if len <= max { return content.to_string(); }
  let cut: String = content.chars().skip(len - max).collect();
  format!("…(earlier entries trimmed)\n{cut}")
}

fn save(conn: &Connection, scope: &str, content: &str) -> Result<(), String> {
  conn.execute(
    "INSERT INTO scratchpads (scope, content, updated_at) VALUES (?1,?2,?3) ON CONFLICT(scope) DO UPDATE SET content=?2, updated_at=?3",
    params![scope, content, chrono::Utc::now().to_rfc3339()],
  ).map_err(|e| e.to_string())?;
  Ok(())
}

pub(crate) fn append(conn: &Connection, scope: &str, text: &str) -> Result<String, String> {
  let existing = read(conn, scope);
  let stamp = chrono::Local::now().format("%H:%M");
  let body = if existing.trim().is_empty() { format!("[{stamp}] {text}") } else { format!("{existing}\n\n[{stamp}] {text}") };
  let next = tail(&body, MAX_PAD);
  save(conn, scope, &next)?;
  Ok(next)
}

pub(crate) fn write(conn: &Connection, scope: &str, text: &str) -> Result<(), String> {
  save(conn, scope, &tail(text, MAX_PAD))
}

pub(crate) fn clear(conn: &Connection, scope: &str) -> Result<(), String> {
  save(conn, scope, "")
}

// The section added to a turn's system prompt, so the agent starts out already
// seeing what it has written down in this chat.
pub(crate) fn block(conn: &Connection, session_id: Option<&str>, agent_id: &str) -> String {
  let pad = read(conn, &scope_for(session_id, agent_id));
  let shown = tail(&pad, PROMPT_CHARS);
  let body = if shown.trim().is_empty() { "(empty)".to_string() } else { shown };
  format!("\n## Your scratchpad (private to this chat)\nKeep working notes, drafts and intermediate findings here — never in the user's notes. Use the scratchpad tool to read, append, write or clear it.\n{body}\n")
}

// The UI's peek at the chat on screen.
#[tauri::command]
pub fn get_scratchpad(app: AppHandle, session_id: Option<String>, agent_id: String) -> Result<String, String> {
  let conn = db(&app)?;
  Ok(read(&conn, &scope_for(session_id.as_deref(), &agent_id)))
}
