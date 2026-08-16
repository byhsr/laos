// Database connection + schema migrations.
use rusqlite::Connection;
use std::fs;
use tauri::{AppHandle, Manager};

pub fn db(app: &AppHandle) -> Result<Connection, String> {
  let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
  fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
  let conn = Connection::open(dir.join("local-agent-os.sqlite3")).map_err(|e| e.to_string())?;
  conn.execute_batch("CREATE TABLE IF NOT EXISTS runs (id TEXT PRIMARY KEY, agent_id TEXT NOT NULL, started_at TEXT NOT NULL, status TEXT NOT NULL, model TEXT NOT NULL, input TEXT NOT NULL, output TEXT, prompt_tokens INTEGER NOT NULL DEFAULT 0, completion_tokens INTEGER NOT NULL DEFAULT 0); CREATE TABLE IF NOT EXISTS memory (agent_id TEXT NOT NULL, key TEXT NOT NULL, value TEXT NOT NULL, updated_at TEXT NOT NULL, PRIMARY KEY(agent_id,key)); CREATE TABLE IF NOT EXISTS model_configs (id TEXT PRIMARY KEY, provider TEXT NOT NULL, label TEXT NOT NULL, model TEXT NOT NULL, host TEXT, api_key TEXT, enabled INTEGER NOT NULL DEFAULT 1); CREATE TABLE IF NOT EXISTS tools (id TEXT PRIMARY KEY, name TEXT NOT NULL, kind TEXT NOT NULL, integration_id TEXT NOT NULL, description TEXT, enabled INTEGER NOT NULL DEFAULT 1, config_json TEXT NOT NULL DEFAULT '{}'); CREATE TABLE IF NOT EXISTS agents (id TEXT PRIMARY KEY, name TEXT NOT NULL, objective TEXT NOT NULL, model TEXT NOT NULL, tool_ids TEXT NOT NULL DEFAULT '[]', integrations TEXT NOT NULL DEFAULT '[]', memory INTEGER NOT NULL DEFAULT 1, permissions TEXT NOT NULL DEFAULT '[]', home_path TEXT NOT NULL, color TEXT NOT NULL DEFAULT '#8b5cf6', x REAL NOT NULL DEFAULT 0, y REAL NOT NULL DEFAULT 0, is_manager INTEGER NOT NULL DEFAULT 0, description TEXT NOT NULL DEFAULT ''); CREATE TABLE IF NOT EXISTS workflows (id TEXT PRIMARY KEY, name TEXT NOT NULL, nodes TEXT NOT NULL DEFAULT '[]', edges TEXT NOT NULL DEFAULT '[]', updated_at TEXT NOT NULL); CREATE TABLE IF NOT EXISTS integration_configs (id TEXT PRIMARY KEY, name TEXT NOT NULL, provider TEXT NOT NULL, config_json TEXT NOT NULL DEFAULT '{}', enabled INTEGER NOT NULL DEFAULT 0, connected INTEGER NOT NULL DEFAULT 0, updated_at TEXT NOT NULL); CREATE TABLE IF NOT EXISTS tasks (id TEXT PRIMARY KEY, requester TEXT NOT NULL, assigned_agent TEXT NOT NULL, status TEXT NOT NULL DEFAULT 'pending', input TEXT NOT NULL, context TEXT NOT NULL DEFAULT '', result TEXT, created_at TEXT NOT NULL, completed_at TEXT); CREATE TABLE IF NOT EXISTS agent_conversations (agent_id TEXT PRIMARY KEY, messages TEXT NOT NULL DEFAULT '[]'); CREATE TABLE IF NOT EXISTS chat_sessions (id TEXT PRIMARY KEY, agent_id TEXT NOT NULL, title TEXT NOT NULL DEFAULT 'Chat', created_at TEXT NOT NULL, updated_at TEXT NOT NULL, summary TEXT NOT NULL DEFAULT ''); CREATE TABLE IF NOT EXISTS chat_messages (id INTEGER PRIMARY KEY AUTOINCREMENT, session_id TEXT NOT NULL, role TEXT NOT NULL, content TEXT NOT NULL, time TEXT NOT NULL DEFAULT ''); CREATE TABLE IF NOT EXISTS day_contexts (agent_id TEXT NOT NULL, day TEXT NOT NULL, summary TEXT NOT NULL, updated_at TEXT NOT NULL, PRIMARY KEY(agent_id, day));") .map_err(|e| e.to_string())?;
  // Migrate older DBs: ensure new columns exist on existing tables.
  let cols = |table: &str| -> Result<Vec<String>, String> {
    conn.prepare(&format!("PRAGMA table_info({table})")).map_err(|e| e.to_string())?
      .query_map([], |row| row.get::<_, String>(1)).map_err(|e| e.to_string())?
      .collect::<Result<_, _>>().map_err(|e| e.to_string())
  };
  let agents_cols = cols("agents")?;
  if !agents_cols.iter().any(|c| c == "is_manager") { conn.execute("ALTER TABLE agents ADD COLUMN is_manager INTEGER NOT NULL DEFAULT 0", []).map_err(|e| e.to_string())?; }
  if !agents_cols.iter().any(|c| c == "description") { conn.execute("ALTER TABLE agents ADD COLUMN description TEXT NOT NULL DEFAULT ''", []).map_err(|e| e.to_string())?; }
  let runs_cols = cols("runs")?;
  if !runs_cols.iter().any(|c| c == "prompt_tokens") { conn.execute("ALTER TABLE runs ADD COLUMN prompt_tokens INTEGER NOT NULL DEFAULT 0", []).map_err(|e| e.to_string())?; }
  if !runs_cols.iter().any(|c| c == "completion_tokens") { conn.execute("ALTER TABLE runs ADD COLUMN completion_tokens INTEGER NOT NULL DEFAULT 0", []).map_err(|e| e.to_string())?; }
  let sess_cols = cols("chat_sessions")?;
  if !sess_cols.iter().any(|c| c == "summary") { conn.execute("ALTER TABLE chat_sessions ADD COLUMN summary TEXT NOT NULL DEFAULT ''", []).map_err(|e| e.to_string())?; }
  Ok(conn)
}

pub fn now() -> String { chrono::Local::now().format("%H:%M:%S").to_string() }
