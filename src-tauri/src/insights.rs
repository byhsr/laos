// The learning layer (bounded): aggregate what the user files and does, ask a
// model for goals / preferences / patterns, and store them as proposals the user
// accepts or dismisses. It never rewrites habits or reminders on its own.

use std::time::Duration;

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tauri::AppHandle;

use crate::db::db;

#[derive(Serialize, Deserialize, Clone)]
pub struct Insight {
  pub id: String,
  #[serde(default)] pub kind: String,
  #[serde(default)] pub title: String,
  #[serde(default)] pub body: String,
  #[serde(default)] pub confidence: f64,
  #[serde(default)] pub status: String,
  #[serde(default)] pub created_at: String,
  #[serde(default)] pub updated_at: String,
}

const SELECT: &str = "SELECT id, kind, title, body, confidence, status, created_at, updated_at FROM insights";

fn row(r: &rusqlite::Row) -> rusqlite::Result<Insight> {
  Ok(Insight {
    id: r.get(0)?, kind: r.get(1)?, title: r.get(2)?, body: r.get(3)?,
    confidence: r.get(4)?, status: r.get(5)?, created_at: r.get(6)?, updated_at: r.get(7)?,
  })
}

pub(crate) fn list(conn: &Connection, status: Option<&str>) -> Vec<Insight> {
  let mut out = Vec::new();
  let (sql, args): (String, Vec<String>) = match status.filter(|s| !s.trim().is_empty()) {
    Some(s) => (format!("{SELECT} WHERE status=?1 ORDER BY created_at DESC LIMIT 200"), vec![s.to_string()]),
    None => (format!("{SELECT} ORDER BY created_at DESC LIMIT 200"), vec![]),
  };
  if let Ok(mut stmt) = conn.prepare(&sql) {
    if let Ok(rows) = stmt.query_map(rusqlite::params_from_iter(args.iter()), row) {
      for r in rows { if let Ok(i) = r { out.push(i); } }
    }
  }
  out
}

fn insert(conn: &Connection, kind: &str, title: &str, body: &str, confidence: f64) -> Result<(), String> {
  let id = format!("ins-{}", chrono::Utc::now().timestamp_millis());
  let ts = chrono::Utc::now().to_rfc3339();
  conn.execute(
    "INSERT INTO insights (id, kind, title, body, confidence, status, created_at, updated_at) VALUES (?1,?2,?3,?4,?5,'new',?6,?6)",
    params![id, kind, title, body, confidence.clamp(0.0, 1.0), ts],
  ).map_err(|e| e.to_string())?;
  Ok(())
}

fn set_status(conn: &Connection, id: &str, status: &str) -> Result<(), String> {
  conn.execute("UPDATE insights SET status=?2, updated_at=?3 WHERE id=?1",
    params![id, status, chrono::Utc::now().to_rfc3339()]).map_err(|e| e.to_string())?;
  Ok(())
}

pub(crate) fn summary(conn: &Connection) -> String {
  let new = list(conn, Some("new"));
  if new.is_empty() { return "No new insights.".into(); }
  new.iter().take(10).enumerate().map(|(i, ins)| {
    format!("{}. [{}] {} — {}", i + 1, ins.kind, ins.title, ins.body)
  }).collect::<Vec<_>>().join("\n")
}

/// Accepting an insight applies it: a habit suggestion becomes a habit item;
/// goals and preferences are already written to memory when the insight is made.
pub(crate) fn accept(conn: &Connection, rest: &str) -> Result<String, String> {
  let n: usize = rest.trim().parse().unwrap_or(0);
  let new = list(conn, Some("new"));
  let Some(ins) = n.checked_sub(1).and_then(|i| new.get(i)) else {
    return Err("Usage: /accept <number> from /insights.".into());
  };
  if ins.kind == "habit_suggestion" {
    let (tags, collection) = (Vec::new(), String::new());
    crate::capture::add(conn, crate::capture::NewItem {
      kind: "habit", title: &ins.title, body: &ins.body, url: "", tags, collection: &collection,
      due_at: None, cadence: "daily", project_id: "", source: "insight",
    })?;
  }
  set_status(conn, &ins.id, "accepted")?;
  Ok(format!("Accepted — {}", ins.title))
}

// ---------------------------------------------------------------------------
// Reflection
// ---------------------------------------------------------------------------

/// A compact picture of what the user actually files and finishes — the feedstock
/// nothing else computes.
pub(crate) fn aggregate(conn: &Connection) -> String {
  let items = crate::capture::list(conn, None);
  if items.is_empty() { return String::new(); }

  let mut by_kind: std::collections::BTreeMap<&str, usize> = Default::default();
  let mut by_tag: std::collections::BTreeMap<String, usize> = Default::default();
  let mut titles: std::collections::BTreeMap<String, usize> = Default::default();
  let (mut done, mut active) = (0usize, 0usize);

  for it in &items {
    *by_kind.entry(it.kind.as_str()).or_default() += 1;
    if it.status == "done" { done += 1; } else { active += 1; }
    for t in &it.tags { *by_tag.entry(t.clone()).or_default() += 1; }
    let key = it.title.trim().to_lowercase();
    if !key.is_empty() { *titles.entry(key).or_default() += 1; }
  }

  let repeated: Vec<String> = titles.iter().filter(|(_, n)| **n >= 2).map(|(t, n)| format!("{t} (×{n})")).collect();
  let tags: Vec<String> = by_tag.iter().map(|(t, n)| format!("#{t} (×{n})")).collect();
  let kinds: Vec<String> = by_kind.iter().map(|(k, n)| format!("{k}: {n}")).collect();

  format!(
    "Items filed: {} ({} done, {} active)\nBy kind: {}\nTags: {}\nRepeated titles: {}\nRecent: {}",
    items.len(), done, active,
    kinds.join(", "),
    if tags.is_empty() { "none".into() } else { tags.join(", ") },
    if repeated.is_empty() { "none".into() } else { repeated.join(", ") },
    items.iter().take(12).map(|i| format!("[{}] {}", i.kind, i.title)).collect::<Vec<_>>().join("; "),
  )
}

const REFLECT_PROMPT: &str = "You are the planner of a personal system. Given a summary of what the user \
files and finishes, infer a few durable things about them and propose at most 5 insights. Output strict \
JSON only, no prose, no code fences.\n\
\n\
SCHEMA: {\"insights\":[{\"kind\":\"goal|preference|pattern|habit_suggestion\",\"title\":\"short\",\
\"body\":\"one sentence why\",\"confidence\":0.0}],\"self\":[{\"key\":\"goals|current_focus|preferences\",\
\"value\":\"one line\"}]}\n\
\n\
RULES\n\
- goal: something they are clearly working toward.\n\
- preference: how they like things done (times, phrasing, tools).\n\
- pattern: a behaviour visible in the data (e.g. they file most todos in the evening).\n\
- habit_suggestion: a title that repeats — propose making it a recurring habit.\n\
- Only propose what the data supports. Fewer, sharper insights beat many. \
If there is nothing meaningful, return empty arrays.";

/// One reflection pass. Returns how many new insights were written.
pub(crate) async fn reflect(app: &AppHandle) -> Result<usize, String> {
  let (aggregates, model_id, reply) = {
    let conn = db(app)?;
    let aggregates = aggregate(&conn);
    if aggregates.is_empty() { return Ok(0); }
    let model_id = conn.query_row("SELECT id FROM model_configs WHERE enabled=1 ORDER BY id LIMIT 1", [], |r| r.get::<_, String>(0))
      .map_err(|_| "No model configured.".to_string())?;
    let resolved = crate::provider::resolve(&conn, &model_id, None)?;

    let mut body = json!({
      "model": resolved.model,
      "messages": [
        { "role": "system", "content": REFLECT_PROMPT },
        { "role": "user", "content": aggregates }
      ],
      "temperature": 0.3,
      "stream": false
    });
    crate::http::apply_openai_defaults(&mut body, resolved.kind == crate::provider::Kind::OpenRouter, 700);
    crate::provider::apply_reasoning(&mut body, &resolved, 700);
    if resolved.kind == crate::provider::Kind::Anthropic { body = crate::anthropic::to_wire(&body); }

    let client = crate::http::client();
    let resp = crate::http::send_model_request(&client, &resolved, &resolved.chat_url(), &body, crate::http::MODEL_ATTEMPTS).await?;
    let mut reply: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    if resolved.kind == crate::provider::Kind::Anthropic { reply = crate::anthropic::normalize_response(&reply); }
    let text = reply["choices"][0]["message"]["content"].as_str().unwrap_or("").to_string();
    let _ = &model_id;
    (aggregates, model_id, text)
  };
  let _ = (aggregates, model_id);

  let cleaned = reply.replace("```json", "").replace("```", "");
  let parsed: serde_json::Value = serde_json::from_str(cleaned.trim()).unwrap_or(serde_json::json!({}));

  let conn = db(app)?;
  let existing: Vec<String> = list(&conn, None).into_iter().map(|i| i.title.to_lowercase()).collect();
  let mut written = 0usize;

  if let Some(items) = parsed["insights"].as_array() {
    for ins in items.iter().take(5) {
      let title = ins["title"].as_str().unwrap_or("").trim().to_string();
      if title.is_empty() || existing.iter().any(|t| t == &title.to_lowercase()) { continue; }
      let kind = match ins["kind"].as_str().unwrap_or("pattern") {
        "goal" | "preference" | "pattern" | "habit_suggestion" => ins["kind"].as_str().unwrap_or("pattern"),
        _ => "pattern",
      };
      let body = ins["body"].as_str().unwrap_or("").to_string();
      let confidence = ins["confidence"].as_f64().unwrap_or(0.5);
      insert(&conn, kind, &title, &body, confidence)?;
      // Goals and preferences are durable — mirror them into native memory.
      if kind == "goal" || kind == "preference" {
        let mem_kind = if kind == "goal" { "reflection" } else { "preference" };
        let _ = crate::agent_memory::add_memory(&conn, "manager", "global", mem_kind, &title, &body, &json!([]), confidence, 0.7);
      }
      written += 1;
    }
  }
  if let Some(entries) = parsed["self"].as_array() {
    for e in entries.iter().take(6) {
      let key = e["key"].as_str().unwrap_or("").trim();
      let value = e["value"].as_str().unwrap_or("").trim();
      if key.is_empty() || value.is_empty() { continue; }
      let _ = crate::agent_memory::set_self_model(&conn, "manager", "global", key, value, 0.7, 0.7);
    }
  }

  Ok(written)
}

/// Reflects in the background every 30 minutes; a no-op pass costs nothing when
/// there is nothing new to think about.
pub(crate) async fn insight_loop(app: AppHandle) {
  loop {
    tokio::time::sleep(Duration::from_secs(30 * 60)).await;
    let _ = reflect(&app).await;
  }
}

#[tauri::command]
pub fn insight_list(app: AppHandle, status: Option<String>) -> Result<Vec<Insight>, String> {
  let conn = db(&app)?;
  Ok(list(&conn, status.as_deref()))
}

#[tauri::command]
pub fn insight_accept(app: AppHandle, id: String) -> Result<(), String> {
  let conn = db(&app)?;
  set_status(&conn, &id, "accepted").map_err(|e| e.to_string())
}

#[tauri::command]
pub fn insight_dismiss(app: AppHandle, id: String) -> Result<(), String> {
  let conn = db(&app)?;
  set_status(&conn, &id, "dismissed")
}

#[tauri::command]
pub async fn reflect_now(app: AppHandle) -> Result<usize, String> {
  reflect(&app).await
}
