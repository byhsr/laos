// The brain map: the user's life as a graph of connected information, on top of
// the stores that already exist.
//
//   brain_nodes        — the unified node registry. A node either owns its content
//                        (people, decisions, ideas, goals…) or is an *index* row
//                        pointing at an authoritative `items` / `am_memories` /
//                        `chat_messages` record (ref_kind/ref_id), so nothing is
//                        duplicated and edges can anchor to anything.
//   brain_edges        — typed, provenance-carrying relationships between nodes.
//   brain_events       — append-only log of meaningful interactions / state changes.
//   brain_observations — inferred patterns with evidence + confidence (never facts).
//   attention_state    — per-node attention signals, computed from observations only.
//   embeddings         — vectors for a node, ranked in Rust (see embed.rs).
//
// Everything is scoped with the same compact keys as agent memory (`global`,
// `project:<id>`, …); `life` is the scope captured second-brain items land in.

use std::sync::atomic::{AtomicI8, Ordering};

use rusqlite::{params, Connection};
use serde_json::{json, Value};
use tauri::AppHandle;

use crate::db::db;

/// Scopes always visible regardless of the active namespace.
pub(crate) const BASE_SCOPES: [&str; 2] = ["global", "life"];

fn now_ms() -> i64 { chrono::Utc::now().timestamp_millis() }

fn new_id(prefix: &str) -> String {
  let n = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
  static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
  let c = N.fetch_add(1, Ordering::Relaxed);
  format!("{prefix}-{n:x}-{c}")
}

/// The scopes a query in `scope` may read: the scope itself plus the base scopes.
pub(crate) fn scopes_for(scope: &str) -> Vec<String> {
  let mut out = Vec::new();
  if !scope.trim().is_empty() && scope != "global" && scope != "life" { out.push(scope.to_string()); }
  for b in BASE_SCOPES { if !out.iter().any(|s| s == b) { out.push(b.to_string()); } }
  out
}

// ---------------------------------------------------------------------------
// Schema
// ---------------------------------------------------------------------------

static FTS_STATE: AtomicI8 = AtomicI8::new(-1);

const BASE_SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS brain_nodes (id TEXT PRIMARY KEY, kind TEXT NOT NULL, scope TEXT NOT NULL DEFAULT 'global',
  title TEXT NOT NULL DEFAULT '', body TEXT NOT NULL DEFAULT '', ref_kind TEXT NOT NULL DEFAULT '', ref_id TEXT NOT NULL DEFAULT '',
  source TEXT NOT NULL DEFAULT '', provenance TEXT NOT NULL DEFAULT 'stated', status TEXT NOT NULL DEFAULT 'active',
  tags TEXT NOT NULL DEFAULT '[]', importance REAL NOT NULL DEFAULT 0.5, salience REAL NOT NULL DEFAULT 0.0,
  confidence REAL NOT NULL DEFAULT 0.8, meta TEXT NOT NULL DEFAULT '{}', created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL);
CREATE UNIQUE INDEX IF NOT EXISTS brain_nodes_ref ON brain_nodes(ref_kind, ref_id) WHERE ref_id != '';
CREATE INDEX IF NOT EXISTS brain_nodes_scope ON brain_nodes(scope, updated_at DESC);
CREATE INDEX IF NOT EXISTS brain_nodes_kind ON brain_nodes(kind);

CREATE TABLE IF NOT EXISTS brain_edges (id TEXT PRIMARY KEY, src_id TEXT NOT NULL, dst_id TEXT NOT NULL, relation TEXT NOT NULL,
  provenance TEXT NOT NULL DEFAULT 'stated', confidence REAL NOT NULL DEFAULT 0.8, evidence TEXT NOT NULL DEFAULT '',
  created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL, UNIQUE(src_id, dst_id, relation));
CREATE INDEX IF NOT EXISTS brain_edges_src ON brain_edges(src_id);
CREATE INDEX IF NOT EXISTS brain_edges_dst ON brain_edges(dst_id);

CREATE TABLE IF NOT EXISTS brain_events (id TEXT PRIMARY KEY, agent_id TEXT NOT NULL DEFAULT '', scope TEXT NOT NULL DEFAULT 'global',
  kind TEXT NOT NULL, content TEXT NOT NULL DEFAULT '', data TEXT NOT NULL DEFAULT '{}', node_id TEXT NOT NULL DEFAULT '',
  occurred_at INTEGER NOT NULL);
CREATE INDEX IF NOT EXISTS brain_events_scope ON brain_events(scope, occurred_at DESC);
CREATE INDEX IF NOT EXISTS brain_events_node ON brain_events(node_id, occurred_at DESC);

CREATE TABLE IF NOT EXISTS brain_observations (id TEXT PRIMARY KEY, scope TEXT NOT NULL DEFAULT 'global', dimension TEXT NOT NULL,
  kind TEXT NOT NULL DEFAULT 'pattern', title TEXT NOT NULL DEFAULT '', body TEXT NOT NULL DEFAULT '',
  confidence REAL NOT NULL DEFAULT 0.5, evidence TEXT NOT NULL DEFAULT '[]', status TEXT NOT NULL DEFAULT 'new',
  first_seen_at INTEGER NOT NULL, last_seen_at INTEGER NOT NULL, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL);
CREATE INDEX IF NOT EXISTS brain_obs_scope ON brain_observations(scope, updated_at DESC);

CREATE TABLE IF NOT EXISTS attention_state (id TEXT PRIMARY KEY, scope TEXT NOT NULL DEFAULT 'global', node_id TEXT NOT NULL,
  dimension TEXT NOT NULL, value REAL NOT NULL DEFAULT 0.0, evidence TEXT NOT NULL DEFAULT '[]',
  confidence REAL NOT NULL DEFAULT 0.5, observed_at INTEGER NOT NULL, updated_at INTEGER NOT NULL, UNIQUE(node_id, dimension));

CREATE TABLE IF NOT EXISTS embeddings (id TEXT PRIMARY KEY, source_kind TEXT NOT NULL, source_id TEXT NOT NULL,
  model TEXT NOT NULL DEFAULT '', dim INTEGER NOT NULL DEFAULT 0, vector BLOB NOT NULL,
  content_hash TEXT NOT NULL DEFAULT '', created_at INTEGER NOT NULL, UNIQUE(source_kind, source_id));
";

const FTS_SCHEMA: &str = "CREATE VIRTUAL TABLE IF NOT EXISTS brain_fts USING fts5(node_id UNINDEXED, title, body, tags, tokenize='unicode61');";

pub(crate) fn ensure_schema(conn: &Connection) -> Result<(), String> {
  conn.execute_batch(BASE_SCHEMA).map_err(|e| e.to_string())?;
  // Every connection needs the FTS table (a fresh connection — e.g. an in-memory
  // test DB — does not inherit another's). It's an idempotent DDL no-op when it
  // already exists; a failure means SQLite was built without FTS5, and the
  // keyword path falls back to a lexical scan.
  let ok = conn.execute_batch(FTS_SCHEMA).is_ok();
  FTS_STATE.store(if ok { 1 } else { 0 }, Ordering::Relaxed);
  Ok(())
}

pub(crate) fn fts_enabled() -> bool { FTS_STATE.load(Ordering::Relaxed) == 1 }

// ---------------------------------------------------------------------------
// Full-text index (best effort — a no-op when FTS5 is unavailable)
// ---------------------------------------------------------------------------

fn tags_json(tags: &[String]) -> String { serde_json::to_string(tags).unwrap_or_else(|_| "[]".into()) }

fn fts_put(conn: &Connection, node_id: &str, title: &str, body: &str, tags: &[String]) {
  if !fts_enabled() { return; }
  let _ = conn.execute("DELETE FROM brain_fts WHERE node_id=?1", params![node_id]);
  let _ = conn.execute(
    "INSERT INTO brain_fts (node_id, title, body, tags) VALUES (?1,?2,?3,?4)",
    params![node_id, title, body, tags.join(" ")],
  );
}

fn fts_del(conn: &Connection, node_id: &str) {
  if !fts_enabled() { return; }
  let _ = conn.execute("DELETE FROM brain_fts WHERE node_id=?1", params![node_id]);
}

/// Turns a free-text query into an FTS5 MATCH expression (quoted tokens OR'd).
pub(crate) fn to_match(query: &str) -> String {
  query.split_whitespace()
    .map(|t| t.chars().filter(|c| c.is_alphanumeric() || *c == '_' || *c == '-').collect::<String>())
    .filter(|t| !t.is_empty())
    .map(|t| format!("\"{t}\""))
    .collect::<Vec<_>>().join(" OR ")
}

// ---------------------------------------------------------------------------
// Nodes
// ---------------------------------------------------------------------------

pub(crate) struct NewNode<'a> {
  pub kind: &'a str,
  pub scope: &'a str,
  pub title: &'a str,
  pub body: &'a str,
  /// `""` = this node owns its content; anything else references that store row.
  pub ref_kind: &'a str,
  pub ref_id: &'a str,
  pub source: &'a str,
  pub provenance: &'a str,
  pub tags: Vec<String>,
  pub importance: f64,
  pub salience: f64,
  pub confidence: f64,
  pub meta: Value,
}

/// Upserts a node. An index node (non-empty `ref_id`) reuses its existing id, so
/// re-filing the same item never creates a duplicate.
pub(crate) fn upsert_node(conn: &Connection, n: &NewNode) -> Result<String, String> {
  let ts = now_ms();
  let id = if !n.ref_id.trim().is_empty() {
    conn.query_row("SELECT id FROM brain_nodes WHERE ref_kind=?1 AND ref_id=?2", params![n.ref_kind, n.ref_id], |r| r.get::<_, String>(0))
      .ok()
      .unwrap_or_else(|| new_id("bn"))
  } else {
    new_id("bn")
  };
  conn.execute(
    "INSERT INTO brain_nodes (id, kind, scope, title, body, ref_kind, ref_id, source, provenance, status, tags, importance, salience, confidence, meta, created_at, updated_at) \
     VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,'active',?10,?11,?12,?13,?14,?15,?15) \
     ON CONFLICT(id) DO UPDATE SET kind=excluded.kind, scope=excluded.scope, title=excluded.title, body=excluded.body, \
     source=excluded.source, provenance=excluded.provenance, tags=excluded.tags, importance=excluded.importance, \
     salience=excluded.salience, confidence=excluded.confidence, meta=excluded.meta, updated_at=excluded.updated_at",
    params![
      id, n.kind, n.scope, n.title, n.body, n.ref_kind, n.ref_id, n.source, n.provenance,
      tags_json(&n.tags), n.importance.clamp(0.0, 1.0), n.salience.clamp(0.0, 1.0), n.confidence.clamp(0.0, 1.0),
      n.meta.to_string(), ts
    ],
  ).map_err(|e| e.to_string())?;
  fts_put(conn, &id, n.title, n.body, &n.tags);
  Ok(id)
}

pub(crate) fn record_brain_event(conn: &Connection, agent_id: &str, scope: &str, kind: &str, content: &str, data: &Value, node_id: &str) {
  let _ = conn.execute(
    "INSERT INTO brain_events (id, agent_id, scope, kind, content, data, node_id, occurred_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
    params![new_id("bev"), agent_id, scope, kind, content, data.to_string(), node_id, now_ms()],
  );
}

// ---------------------------------------------------------------------------
// Edges
// ---------------------------------------------------------------------------

pub(crate) fn add_edge(conn: &Connection, src: &str, dst: &str, relation: &str, provenance: &str, confidence: f64, evidence: &str) {
  if src.is_empty() || dst.is_empty() || src == dst { return; }
  let ts = now_ms();
  let _ = conn.execute(
    "INSERT INTO brain_edges (id, src_id, dst_id, relation, provenance, confidence, evidence, created_at, updated_at) \
     VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?8) \
     ON CONFLICT(src_id, dst_id, relation) DO UPDATE SET confidence=MAX(brain_edges.confidence, excluded.confidence), updated_at=excluded.updated_at",
    params![new_id("bed"), src, dst, relation, provenance, confidence.clamp(0.0, 1.0), evidence, ts],
  );
}

// ---------------------------------------------------------------------------
// Entity resolution
// ---------------------------------------------------------------------------

/// Score below which a name is treated as referring to nothing. Above it, a
/// match is good enough to link without asking; two near-equal candidates are
/// what `resolve_ambiguous` catches.
const RESOLVE_MIN: f64 = 0.5;

/// How confident we are that a node is the one `name` refers to, from its title
/// and alias tags: exact 1.0, alias 0.95, kind + contains 0.75, a bare contains
/// (only for names of 4+ characters) 0.55. Same-scope matches edge out base ones.
fn name_score(title: &str, tags_json: &str, kind: &str, kind_hint: &str, name: &str, node_scope: &str, scope: &str) -> f64 {
  let t = title.trim().to_lowercase();
  if t.is_empty() { return 0.0; }
  let aliases: Vec<String> = serde_json::from_str::<Vec<String>>(tags_json).unwrap_or_default()
    .into_iter().map(|s| s.trim().to_lowercase()).filter(|s| !s.is_empty()).collect();
  let mut score = if t == name {
    1.0
  } else if aliases.iter().any(|a| a == name) {
    0.95
  } else if !kind_hint.trim().is_empty() && kind == kind_hint && t.contains(name) {
    0.75
  } else if name.chars().count() >= 4 && t.contains(name) {
    0.55
  } else {
    0.0
  };
  if score > 0.0 {
    if node_scope == scope { score += 0.05; } else if BASE_SCOPES.contains(&node_scope) { score += 0.02; }
  }
  score
}

/// Every node whose title/alias plausibly matches `name`, best first.
fn resolve_candidates(conn: &Connection, name: &str, kind_hint: &str, scope: &str) -> Vec<(String, f64)> {
  let q = name.trim().to_lowercase();
  if q.is_empty() { return Vec::new(); }
  let mut out: Vec<(String, f64)> = Vec::new();
  if let Ok(mut stmt) = conn.prepare("SELECT id, title, tags, kind, scope FROM brain_nodes WHERE status != 'archived'") {
    if let Ok(rows) = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, String>(3)?, r.get::<_, String>(4)?))) {
      for (id, title, tags, kind, node_scope) in rows.filter_map(|r| r.ok()) {
        let s = name_score(&title, &tags, &kind, kind_hint, &q, &node_scope, scope);
        if s > 0.0 { out.push((id, s)); }
      }
    }
  }
  out.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
  out
}

/// Finds the node an entity name refers to, so "the portfolio" links to the
/// existing portfolio node instead of spawning an unrelated duplicate.
pub(crate) fn resolve_node(conn: &Connection, name: &str, kind_hint: &str, scope: &str) -> Option<String> {
  resolve_candidates(conn, name, kind_hint, scope).into_iter().next()
    .filter(|(_, s)| *s >= RESOLVE_MIN).map(|(id, _)| id)
}

/// The two close contenders for a name, when guessing between them would be
/// wrong. The caller records a question rather than silently picking one.
pub(crate) fn resolve_ambiguous(conn: &Connection, name: &str, kind_hint: &str, scope: &str) -> Option<(String, String)> {
  let c = resolve_candidates(conn, name, kind_hint, scope);
  if c.len() < 2 { return None; }
  if c[1].1 >= RESOLVE_MIN && (c[0].1 - c[1].1).abs() < 0.15 { Some((c[0].0.clone(), c[1].0.clone())) } else { None }
}

// ---------------------------------------------------------------------------
// Ingest
// ---------------------------------------------------------------------------

fn item_kind_to_node(kind: &str) -> &'static str {
  match kind {
    "todo" | "reminder" => "task",
    "goal" => "goal",
    "habit" => "habit",
    "resource" => "note",
    _ => "note",
  }
}

/// Keeps the graph in step with a newly filed item: an index node, resolved
/// entities + typed edges, and an event. No-op-safe / idempotent.
pub(crate) fn ingest_item(app: &AppHandle, conn: &Connection, item: &crate::capture::Item, c: &crate::classify::Classification, scope: &str) {
  let node_kind = item_kind_to_node(&item.kind);
  let n = NewNode {
    kind: node_kind, scope, title: &item.title, body: &item.body,
    ref_kind: "item", ref_id: &item.id, source: &item.source, provenance: "stated",
    tags: item.tags.clone(), importance: 0.5, salience: c.salience, confidence: c.confidence.max(0.5),
    meta: json!({
      "content_type": c.content_type, "item_kind": item.kind,
      "salience_signals": c.salience_signals, "should_act": c.should_act,
    }),
  };
  let node_id = match upsert_node(conn, &n) { Ok(id) => id, Err(_) => return };
  link_entities(conn, &node_id, c, scope);
  link_supersession(conn, &node_id, node_kind, &item.title, scope);
  record_brain_event(conn, "", scope, "node_created", &item.title, &json!({ "content_type": c.content_type, "kind": item.kind }), &node_id);
  spawn_embed(app, &node_id, &format!("{} {}", item.title, item.body));
}

/// Resolves the classifier's entities to nodes (creating missing people/projects)
/// and records typed edges from `from_node` to each.
pub(crate) fn link_entities(conn: &Connection, from_node: &str, c: &crate::classify::Classification, scope: &str) {
  for e in &c.entities {
    let name = e.name.trim();
    if name.is_empty() { continue; }
    let kind = if ["person", "project", "organization", "tool", "place"].contains(&e.kind.as_str()) { e.kind.as_str() } else { "entity" };
    // Two equally good matches: ask rather than guess, and link nothing yet.
    if let Some((a, b)) = resolve_ambiguous(conn, name, kind, scope) {
      record_clarification(conn, scope, name, &a, &b);
      continue;
    }
    let target = resolve_node(conn, name, kind, scope).or_else(|| {
      let n = NewNode {
        kind, scope, title: name, body: "", ref_kind: "", ref_id: "", source: "classify",
        provenance: "stated", tags: vec![], importance: 0.5, salience: 0.0, confidence: 0.6,
        meta: json!({ "entity": true }),
      };
      upsert_node(conn, &n).ok()
    });
    if let Some(t) = target {
      add_edge(conn, from_node, &t, "associated with", "inferred", c.confidence.max(0.5), "classification");
    }
  }
  for r in &c.relationships {
    let target_name = r.target.trim();
    let relation = if r.relation.trim().is_empty() { "related to" } else { r.relation.trim() };
    if target_name.is_empty() { continue; }
    if let Some((a, b)) = resolve_ambiguous(conn, target_name, "", scope) {
      record_clarification(conn, scope, target_name, &a, &b);
      continue;
    }
    if let Some(t) = resolve_node(conn, target_name, "", scope) {
      add_edge(conn, from_node, &t, relation, "inferred", c.confidence.max(0.5), "classification");
    }
  }
}

fn node_title(conn: &Connection, id: &str) -> String {
  conn.query_row("SELECT title FROM brain_nodes WHERE id=?1", params![id], |r| r.get::<_, String>(0))
    .unwrap_or_else(|_| id.to_string())
}

/// Records a question the system can't answer on its own — a name that matches two
/// nodes equally — as an observation, so it surfaces in the mind tab for the user
/// to settle rather than being silently guessed.
pub(crate) fn record_clarification(conn: &Connection, scope: &str, name: &str, a: &str, b: &str) {
  let title = format!("Which “{name}” did you mean?");
  let body = format!("“{name}” could be {} or {}. Nothing was linked until you choose.", node_title(conn, a), node_title(conn, b));
  let _ = insert_observation(conn, scope, "entity_resolution", "question", &title, &body, 0.4, &json!({ "name": name, "candidates": [a, b] }));
}

/// Links a new goal/decision to the earlier one it appears to replace, when the
/// two titles share a meaningful word. A deterministic, explainable *inferred*
/// edge — the old node is kept and shown as superseded, never overwritten.
fn link_supersession(conn: &Connection, new_id: &str, node_kind: &str, title: &str, scope: &str) {
  if !matches!(node_kind, "goal" | "decision") { return; }
  let words: Vec<String> = title.to_lowercase()
    .split(|c: char| !c.is_alphanumeric())
    .filter(|w| w.chars().count() >= 4)
    .map(|w| w.to_string())
    .collect();
  if words.is_empty() { return; }
  let mut rows: Vec<(String, String)> = Vec::new();
  if let Ok(mut stmt) = conn.prepare("SELECT id, title FROM brain_nodes WHERE kind=?1 AND id != ?2 AND status='active' AND scope=?3 ORDER BY updated_at DESC LIMIT 50") {
    if let Ok(mapped) = stmt.query_map(params![node_kind, new_id, scope], |r| Ok((r.get(0)?, r.get(1)?))) {
      for r in mapped { if let Ok(v) = r { rows.push(v); } }
    }
  }
  for (id, other) in rows {
    let t = other.to_lowercase();
    if words.iter().any(|w| t.contains(w.as_str())) {
      add_edge(conn, new_id, &id, "supersedes", "inferred", 0.5, "shares subject");
      return;
    }
  }
}

/// Indexes a distilled memory so it participates in the graph + retrieval.
#[allow(clippy::too_many_arguments)]
pub(crate) fn index_memory(app: &AppHandle, conn: &Connection, scope: &str, mem_id: &str, kind: &str, title: &str, content: &str, confidence: f64, importance: f64) {
  let node_kind = match kind {
    "preference" => "preference",
    "policy" => "decision",
    "procedural" => "procedure",
    "reflection" => "observation",
    _ => "note",
  };
  let n = NewNode {
    kind: node_kind, scope, title, body: content, ref_kind: "memory", ref_id: mem_id, source: "distill",
    provenance: "inferred", tags: vec![], importance, salience: 0.0, confidence, meta: json!({ "memory_kind": kind }),
  };
  if let Ok(node_id) = upsert_node(conn, &n) {
    spawn_embed(app, &node_id, &format!("{title} {content}"));
  }
}

/// Indexes a finished turn as a conversation node (searchable, embeddable).
pub(crate) fn index_conversation(app: &AppHandle, conn: &Connection, agent_id: &str, scope: &str, user: &str, assistant: &str) {
  if user.trim().is_empty() && assistant.trim().is_empty() { return; }
  let title: String = user.trim().chars().take(80).collect();
  let body = format!("User: {user}\nAssistant: {assistant}");
  let ref_id = new_id("turn");
  let n = NewNode {
    kind: "conversation", scope, title: &title, body: &body, ref_kind: "turn", ref_id: &ref_id, source: "chat",
    provenance: "stated", tags: vec![], importance: 0.5, salience: 0.0, confidence: 0.9, meta: json!({ "agent_id": agent_id }),
  };
  if let Ok(node_id) = upsert_node(conn, &n) {
    spawn_embed(app, &node_id, &body);
  }
}

// ---------------------------------------------------------------------------
// Embeddings
// ---------------------------------------------------------------------------

pub(crate) fn store_embedding(conn: &Connection, source_kind: &str, source_id: &str, model: &str, text: &str, vector: &[f32]) {
  let hash = crate::embed::content_hash(text);
  let existing: Option<String> = conn.query_row(
    "SELECT content_hash FROM embeddings WHERE source_kind=?1 AND source_id=?2", params![source_kind, source_id], |r| r.get(0),
  ).ok();
  if existing.as_deref() == Some(hash.as_str()) { return; }
  let blob = crate::embed::to_blob(vector);
  let _ = conn.execute(
    "INSERT INTO embeddings (id, source_kind, source_id, model, dim, vector, content_hash, created_at) \
     VALUES (?1,?2,?3,?4,?5,?6,?7,?8) \
     ON CONFLICT(source_kind, source_id) DO UPDATE SET model=excluded.model, dim=excluded.dim, vector=excluded.vector, content_hash=excluded.content_hash, created_at=excluded.created_at",
    params![new_id("emb"), source_kind, source_id, model, vector.len() as i64, blob, hash, now_ms()],
  );
}

/// Embeds a node in the background; a failure (no role, no network) is silent —
/// the node stays keyword-searchable.
pub(crate) fn spawn_embed(app: &AppHandle, node_id: &str, text: &str) {
  let app = app.clone();
  let node_id = node_id.to_string();
  let text = text.to_string();
  tauri::async_runtime::spawn(async move {
    if let Ok(v) = crate::embed::embed_text(&app, &text).await {
      if let Ok(conn) = db(&app) {
        store_embedding(&conn, "brain_node", &node_id, "", &text, &v);
      }
    }
  });
}

// ---------------------------------------------------------------------------
// Attention (deterministic — no model; evidence + confidence only)
// ---------------------------------------------------------------------------

pub(crate) fn upsert_attention(conn: &Connection, scope: &str, node_id: &str, dimension: &str, value: f64, evidence: &Value, confidence: f64) {
  let ts = now_ms();
  let _ = conn.execute(
    "INSERT INTO attention_state (id, scope, node_id, dimension, value, evidence, confidence, observed_at, updated_at) \
     VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?8) \
     ON CONFLICT(node_id, dimension) DO UPDATE SET value=excluded.value, evidence=excluded.evidence, confidence=excluded.confidence, updated_at=excluded.updated_at",
    params![new_id("att"), scope, node_id, dimension, value, evidence.to_string(), confidence.clamp(0.0, 1.0), ts],
  );
}

/// Dimensions tracked per node. Salience is captured with the node; the rest are
/// computed here. None of these is a moral score — they record what happened.
pub(crate) const ATTENTION_DIMS: [&str; 8] = ["allocation", "dormancy", "intention", "persistence", "momentum", "displacement", "drift", "reentry"];

fn events_since(conn: &Connection, node_id: &str, since: i64) -> i64 {
  conn.query_row("SELECT COUNT(*) FROM brain_events WHERE node_id=?1 AND occurred_at >= ?2", params![node_id, since], |r| r.get(0)).unwrap_or(0)
}

/// Recomputes attention signals from observable evidence only. Returns how many
/// rows were written. Interprets nothing — it records what happened.
pub(crate) fn compute_attention(conn: &Connection) -> usize {
  let now = now_ms();
  let day = 86_400_000i64;
  let w7 = now - 7 * day;
  let w14 = now - 14 * day;

  // Total recent activity, so a stalled commitment can be read against how busy
  // the whole system actually is.
  let total_recent: i64 = conn.query_row("SELECT COUNT(*) FROM brain_events WHERE occurred_at >= ?1", params![w14], |r| r.get(0)).unwrap_or(0);

  let mut rows: Vec<(String, String, String, i64, f64)> = Vec::new();
  if let Ok(mut stmt) = conn.prepare("SELECT id, kind, scope, updated_at, salience FROM brain_nodes WHERE status='active' AND kind IN ('project','goal','idea','decision','habit','task','preference','person')") {
    if let Ok(mapped) = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))) {
      for r in mapped { if let Ok(v) = r { rows.push(v); } }
    }
  }

  let mut written = 0usize;
  for (id, kind, scope, updated, salience) in rows {
    let c14 = events_since(conn, &id, w14);
    let c7 = events_since(conn, &id, w7);
    let prev = (c14 - c7).max(0);
    let last: i64 = conn.query_row("SELECT COALESCE(MAX(occurred_at), 0) FROM brain_events WHERE node_id=?1", params![id], |r| r.get(0)).unwrap_or(0);
    let last_activity = updated.max(last);
    let dormancy = ((now - last_activity) as f64 / day as f64).max(0.0);
    // Confidence grows with evidence (event count); a node with no signal still
    // carries a low-confidence reading rather than none.
    let conf = (c14 as f64 / 6.0).clamp(0.2, 1.0);

    let is_commitment = matches!(kind.as_str(), "goal" | "decision" | "task");
    let declares_intention = matches!(kind.as_str(), "goal" | "decision" | "preference");

    let ev = json!({ "events_14d": c14, "events_7d": c7, "last_activity_at": last_activity });

    upsert_attention(conn, &scope, &id, "allocation", c14 as f64, &ev, conf);
    upsert_attention(conn, &scope, &id, "dormancy", dormancy, &ev, if c14 > 0 { conf } else { 0.4 });
    if declares_intention { upsert_attention(conn, &scope, &id, "intention", 1.0, &json!({ "declared": kind }), conf.max(0.5)); }
    if is_commitment { upsert_attention(conn, &scope, &id, "persistence", (dormancy / 30.0).clamp(0.0, 1.0), &ev, conf); }

    // Momentum: rising (c7 > prev) reads > 0.5, falling < 0.5.
    let momentum = (0.5 + (c7 - prev) as f64 / 6.0).clamp(0.0, 1.0);
    upsert_attention(conn, &scope, &id, "momentum", momentum, &json!({ "events_7d": c7, "previous_7d": prev }), conf);
    // Re-entry: a burst this week after a quieter stretch.
    if c7 > 0 && c7 > prev && dormancy <= 7.0 {
      upsert_attention(conn, &scope, &id, "reentry", 1.0, &json!({ "events_7d": c7, "previous_7d": prev }), conf);
    }
    // Displacement / drift: a declared commitment nothing has served.
    if is_commitment && c14 == 0 && dormancy >= 7.0 {
      upsert_attention(conn, &scope, &id, "displacement", 1.0, &json!({ "dormant_days": dormancy.round() }), 0.5);
      if declares_intention && total_recent > 0 {
        upsert_attention(conn, &scope, &id, "drift", 1.0, &json!({ "system_events_14d": total_recent, "dormant_days": dormancy.round() }), 0.5);
      }
    }
    // Salience: what the capture said was likely to grab awareness.
    if salience > 0.0 { upsert_attention(conn, &scope, &id, "salience", salience, &json!({ "from": "capture" }), 0.6); }
    written += 1;
  }
  written
}

// ---------------------------------------------------------------------------
// Observations
// ---------------------------------------------------------------------------

fn insert_observation(conn: &Connection, scope: &str, dimension: &str, kind: &str, title: &str, body: &str, confidence: f64, evidence: &Value) -> Result<(), String> {
  let existing: Option<String> = conn.query_row("SELECT id FROM brain_observations WHERE lower(title)=?1", params![title.to_lowercase()], |r| r.get(0)).ok();
  let ts = now_ms();
  match existing {
    Some(id) => {
      conn.execute("UPDATE brain_observations SET last_seen_at=?2, updated_at=?2, confidence=MAX(confidence, ?3) WHERE id=?1",
        params![id, ts, confidence.clamp(0.0, 1.0)]).map_err(|e| e.to_string())?;
    }
    None => {
      conn.execute(
        "INSERT INTO brain_observations (id, scope, dimension, kind, title, body, confidence, evidence, status, first_seen_at, last_seen_at, created_at, updated_at) \
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,'new',?9,?9,?9,?9)",
        params![new_id("obs"), scope, dimension, kind, title, body, confidence.clamp(0.0, 1.0), evidence.to_string(), ts],
      ).map_err(|e| e.to_string())?;
    }
  }
  Ok(())
}

/// Turns the computed attention signals into a few explainable proposals. The
/// phrasing is deterministic and the evidence is attached — never a judgment.
pub(crate) fn draft_observations(conn: &Connection) -> usize {
  let mut written = 0usize;
  // Dormant goals / decisions with a displacement signal.
  let mut rows: Vec<(String, String, String, f64)> = Vec::new();
  if let Ok(mut stmt) = conn.prepare(
    "SELECT a.node_id, n.title, n.scope, a.value FROM attention_state a JOIN brain_nodes n ON n.id=a.node_id \
     WHERE a.dimension='displacement' AND n.status='active'") {
    if let Ok(mapped) = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))) {
      for r in mapped { if let Ok(v) = r { rows.push(v); } }
    }
  }
  for (node_id, title, scope, dormant) in rows {
    let weeks = (dormant / 7.0).round().max(1.0) as i64;
    let body = format!("You named “{title}” but nothing has touched it for about {weeks} week(s).");
    if insert_observation(conn, &scope, "displacement", "pattern", &format!("{title} may have been displaced"), &body, 0.5, &json!({ "node_id": node_id, "dormant_days": dormant.round() })).is_ok() {
      written += 1;
    }
  }
  written
}

pub(crate) fn list_observations(conn: &Connection, status: Option<&str>) -> Vec<Value> {
  let mut out = Vec::new();
  let sql = "SELECT id, scope, dimension, kind, title, body, confidence, evidence, status, first_seen_at, last_seen_at FROM brain_observations";
  let (sql, args): (String, Vec<String>) = match status.filter(|s| !s.trim().is_empty()) {
    Some(s) => (format!("{sql} WHERE status=?1 ORDER BY updated_at DESC LIMIT 200"), vec![s.to_string()]),
    None => (format!("{sql} ORDER BY updated_at DESC LIMIT 200"), vec![]),
  };
  if let Ok(mut stmt) = conn.prepare(&sql) {
    if let Ok(rows) = stmt.query_map(rusqlite::params_from_iter(args.iter()), |r| {
      let evidence: String = r.get(7)?;
      Ok(json!({
        "id": r.get::<_, String>(0)?, "scope": r.get::<_, String>(1)?, "dimension": r.get::<_, String>(2)?,
        "kind": r.get::<_, String>(3)?, "title": r.get::<_, String>(4)?, "body": r.get::<_, String>(5)?,
        "confidence": r.get::<_, f64>(6)?, "evidence": serde_json::from_str::<Value>(&evidence).unwrap_or(json!([])),
        "status": r.get::<_, String>(8)?, "firstSeenAt": r.get::<_, i64>(9)?, "lastSeenAt": r.get::<_, i64>(10)?,
      }))
    }) {
      for r in rows { if let Ok(v) = r { out.push(v); } }
    }
  }
  out
}

pub(crate) fn set_observation_status(conn: &Connection, id: &str, status: &str) -> Result<(), String> {
  conn.execute("UPDATE brain_observations SET status=?2, updated_at=?3 WHERE id=?1", params![id, status, now_ms()]).map_err(|e| e.to_string())?;
  Ok(())
}

pub(crate) fn correct_observation(conn: &Connection, id: &str, patch: &Value) -> Result<(), String> {
  let mut sets: Vec<String> = vec!["status='corrected'".into(), "updated_at=?1".into()];
  let mut vals: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(now_ms())];
  let push = |sets: &mut Vec<String>, vals: &mut Vec<Box<dyn rusqlite::ToSql>>, col: &str, v: Box<dyn rusqlite::ToSql>| {
    sets.push(format!("{col} = ?{}", vals.len() + 1));
    vals.push(v);
  };
  if let Some(b) = patch.get("body").and_then(|v| v.as_str()) { push(&mut sets, &mut vals, "body", Box::new(b.to_string())); }
  if let Some(c) = patch.get("confidence").and_then(|v| v.as_f64()) { push(&mut sets, &mut vals, "confidence", Box::new(c.clamp(0.0, 1.0))); }
  let sql = format!("UPDATE brain_observations SET {} WHERE id = ?{}", sets.join(", "), vals.len() + 1);
  vals.push(Box::new(id.to_string()));
  let refs: Vec<&dyn rusqlite::ToSql> = vals.iter().map(|b| b.as_ref()).collect();
  conn.execute(&sql, refs.as_slice()).map_err(|e| e.to_string())?;
  Ok(())
}

/// Observations still carrying their deterministic (template) wording, so the
/// phrasing pass can rewrite just those. `(id, title, body)`.
pub(crate) fn unphrased_observations(conn: &Connection) -> Vec<(String, String, String)> {
  let mut out = Vec::new();
  if let Ok(mut stmt) = conn.prepare("SELECT id, title, body, evidence FROM brain_observations WHERE status='new' ORDER BY updated_at DESC LIMIT 12") {
    if let Ok(rows) = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, String>(3)?))) {
      for (id, title, body, evidence) in rows.filter_map(|r| r.ok()) {
        let phrased = serde_json::from_str::<Value>(&evidence).ok()
          .and_then(|v| v.get("phrased").and_then(|p| p.as_bool())).unwrap_or(false);
        if !phrased { out.push((id, title, body)); }
      }
    }
  }
  out
}

/// Replaces an observation's body with the model's phrasing, flagging it in the
/// evidence so it isn't rewritten again.
pub(crate) fn mark_observation_phrased(conn: &Connection, id: &str, body: &str) {
  let mut ev = conn.query_row("SELECT evidence FROM brain_observations WHERE id=?1", params![id], |r| r.get::<_, String>(0))
    .ok().and_then(|s| serde_json::from_str::<Value>(&s).ok()).unwrap_or_else(|| json!({}));
  if !ev.is_object() { ev = json!({ "detail": ev }); }
  ev["phrased"] = json!(true);
  let _ = conn.execute("UPDATE brain_observations SET body=?2, evidence=?3, updated_at=?4 WHERE id=?1",
    params![id, body, ev.to_string(), now_ms()]);
}

// ---------------------------------------------------------------------------
// Graph / nodes
// ---------------------------------------------------------------------------

pub(crate) fn graph(conn: &Connection) -> Value {
  // Pull the raw rows first, then decorate each with its attention signals — a
  // nested query inside the row closure would borrow the connection twice.
  let mut raw: Vec<(String, String, String, String, String, String, String, String, f64)> = Vec::new();
  if let Ok(mut stmt) = conn.prepare("SELECT id, kind, scope, title, ref_kind, ref_id, provenance, status, salience FROM brain_nodes WHERE status != 'archived' ORDER BY updated_at DESC LIMIT 1000") {
    if let Ok(mapped) = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?, r.get(8)?))) {
      for r in mapped { if let Ok(v) = r { raw.push(v); } }
    }
  }
  let mut nodes = Vec::new();
  for (id, kind, scope, label, ref_kind, ref_id, provenance, status, salience) in raw {
    // Per-node attention signals, keyed by dimension (allocation, dormancy,
    // displacement…), so the graph can size/colour by what consumes attention.
    let mut attention = serde_json::Map::new();
    if let Ok(mut s2) = conn.prepare("SELECT dimension, value FROM attention_state WHERE node_id=?1") {
      if let Ok(rows2) = s2.query_map(params![id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, f64>(1)?))) {
        for (d, v) in rows2.filter_map(|x| x.ok()) { attention.insert(d, json!(v)); }
      }
    }
    nodes.push(json!({
      "id": id, "kind": kind, "scope": scope, "label": label,
      "refKind": ref_kind, "refId": ref_id, "provenance": provenance, "status": status,
      "salience": salience, "attention": Value::Object(attention),
    }));
  }
  let mut edges = Vec::new();
  if let Ok(mut stmt) = conn.prepare("SELECT id, src_id, dst_id, relation, provenance, confidence FROM brain_edges") {
    if let Ok(rows) = stmt.query_map([], |r| Ok(json!({
      "id": r.get::<_, String>(0)?, "from": r.get::<_, String>(1)?, "to": r.get::<_, String>(2)?,
      "relation": r.get::<_, String>(3)?, "provenance": r.get::<_, String>(4)?, "confidence": r.get::<_, f64>(5)?,
    }))) {
      for r in rows { if let Ok(v) = r { edges.push(v); } }
    }
  }
  json!({ "nodes": nodes, "edges": edges })
}

pub(crate) fn node_detail(conn: &Connection, id: &str) -> Value {
  let node = conn.query_row("SELECT * FROM brain_nodes WHERE id=?1", params![id], |r| {
    let tags: String = r.get("tags")?;
    let meta: String = r.get("meta")?;
    Ok(json!({
      "id": r.get::<_, String>("id")?, "kind": r.get::<_, String>("kind")?, "scope": r.get::<_, String>("scope")?,
      "title": r.get::<_, String>("title")?, "body": r.get::<_, String>("body")?,
      "refKind": r.get::<_, String>("ref_kind")?, "refId": r.get::<_, String>("ref_id")?,
      "source": r.get::<_, String>("source")?, "provenance": r.get::<_, String>("provenance")?, "status": r.get::<_, String>("status")?,
      "tags": serde_json::from_str::<Value>(&tags).unwrap_or(json!([])),
      "importance": r.get::<_, f64>("importance")?, "salience": r.get::<_, f64>("salience")?,
      "confidence": r.get::<_, f64>("confidence")?, "meta": serde_json::from_str::<Value>(&meta).unwrap_or(json!({})),
      "createdAt": r.get::<_, i64>("created_at")?, "updatedAt": r.get::<_, i64>("updated_at")?,
    }))
  }).unwrap_or(Value::Null);
  if node.is_null() { return node; }
  let mut edges = Vec::new();
  if let Ok(mut stmt) = conn.prepare("SELECT id, src_id, dst_id, relation, provenance, confidence FROM brain_edges WHERE src_id=?1 OR dst_id=?1") {
    if let Ok(rows) = stmt.query_map(params![id], |r| Ok(json!({
      "id": r.get::<_, String>(0)?, "from": r.get::<_, String>(1)?, "to": r.get::<_, String>(2)?,
      "relation": r.get::<_, String>(3)?, "provenance": r.get::<_, String>(4)?, "confidence": r.get::<_, f64>(5)?,
    }))) {
      for r in rows { if let Ok(v) = r { edges.push(v); } }
    }
  }
  json!({ "node": node, "edges": edges })
}

/// Removes a node and its edges/FTS row. Never deletes the referenced store row.
pub(crate) fn forget_node(conn: &Connection, id: &str) -> Result<(), String> {
  conn.execute("DELETE FROM brain_edges WHERE src_id=?1 OR dst_id=?1", params![id]).map_err(|e| e.to_string())?;
  conn.execute("DELETE FROM attention_state WHERE node_id=?1", params![id]).map_err(|e| e.to_string())?;
  conn.execute("DELETE FROM embeddings WHERE source_kind='brain_node' AND source_id=?1", params![id]).map_err(|e| e.to_string())?;
  fts_del(conn, id);
  conn.execute("DELETE FROM brain_nodes WHERE id=?1", params![id]).map_err(|e| e.to_string())?;
  Ok(())
}

pub(crate) fn stats(conn: &Connection) -> Value {
  let count = |t: &str| conn.query_row(&format!("SELECT COUNT(*) FROM {t}"), [], |r| r.get::<_, i64>(0)).unwrap_or(0);
  json!({
    "nodes": count("brain_nodes"), "edges": count("brain_edges"), "events": count("brain_events"),
    "observations": count("brain_observations"), "embeddings": count("embeddings"), "fts": fts_enabled(),
    "dimensions": ATTENTION_DIMS,
  })
}

/// Backfills index nodes for existing items + memories (idempotent).
pub(crate) fn reindex(app: &AppHandle, conn: &Connection) -> usize {
  let mut n = 0usize;
  for item in crate::capture::list(conn, None) {
    let node = NewNode {
      kind: item_kind_to_node(&item.kind), scope: "life", title: &item.title, body: &item.body,
      ref_kind: "item", ref_id: &item.id, source: if item.source.is_empty() { "reindex" } else { &item.source },
      provenance: "stated", tags: item.tags.clone(), importance: 0.5, salience: 0.0, confidence: 0.6,
      meta: json!({ "item_kind": item.kind, "reindexed": true }),
    };
    if upsert_node(conn, &node).is_ok() { n += 1; }
  }
  // Memories across every scope.
  let mut mems: Vec<(String, String, String, String, String, f64, f64)> = Vec::new();
  if let Ok(mut stmt) = conn.prepare("SELECT id, scope, kind, title, content, confidence, importance FROM am_memories WHERE status != 'archived'") {
    if let Ok(mapped) = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?))) {
      for r in mapped { if let Ok(v) = r { mems.push(v); } }
    }
  }
  for (id, scope, kind, title, content, confidence, importance) in mems {
    index_memory(app, conn, &scope, &id, &kind, &title, &content, confidence, importance);
    n += 1;
  }
  n
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn brain_graph(app: AppHandle) -> Result<Value, String> {
  let conn = db(&app)?;
  // Attention is normally refreshed by the reflect cycle; if the graph is opened
  // before any pass has run, compute it once here so the lens isn't empty.
  let seen: i64 = conn.query_row("SELECT COUNT(*) FROM attention_state", [], |r| r.get(0)).unwrap_or(0);
  if seen == 0 { let _ = compute_attention(&conn); }
  Ok(graph(&conn))
}

#[tauri::command]
pub fn brain_node(app: AppHandle, id: String) -> Result<Value, String> {
  let conn = db(&app)?;
  Ok(node_detail(&conn, &id))
}

#[tauri::command]
pub fn brain_stats(app: AppHandle) -> Result<Value, String> {
  let conn = db(&app)?;
  Ok(stats(&conn))
}

#[tauri::command]
pub fn brain_forget_node(app: AppHandle, id: String) -> Result<(), String> {
  let conn = db(&app)?;
  forget_node(&conn, &id)
}

#[tauri::command]
pub fn brain_list_observations(app: AppHandle, status: Option<String>) -> Result<Vec<Value>, String> {
  let conn = db(&app)?;
  Ok(list_observations(&conn, status.as_deref()))
}

#[tauri::command]
pub fn brain_correct_observation(app: AppHandle, id: String, patch: Value) -> Result<(), String> {
  let conn = db(&app)?;
  correct_observation(&conn, &id, &patch)
}

#[tauri::command]
pub fn brain_dismiss_observation(app: AppHandle, id: String) -> Result<(), String> {
  let conn = db(&app)?;
  set_observation_status(&conn, &id, "dismissed")
}

#[tauri::command]
pub fn brain_accept_observation(app: AppHandle, id: String) -> Result<(), String> {
  let conn = db(&app)?;
  set_observation_status(&conn, &id, "accepted")
}

#[tauri::command]
pub fn brain_reindex(app: AppHandle) -> Result<usize, String> {
  let conn = db(&app)?;
  Ok(reindex(&app, &conn))
}

#[tauri::command]
pub fn brain_reflect_now(app: AppHandle) -> Result<usize, String> {
  let conn = db(&app)?;
  let _ = compute_attention(&conn);
  Ok(draft_observations(&conn))
}

#[cfg(test)]
mod tests {
  use super::*;

  fn db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    ensure_schema(&conn).unwrap();
    conn
  }

  fn node(conn: &Connection, ref_id: &str, title: &str) -> String {
    upsert_node(conn, &NewNode {
      kind: "note", scope: "life", title, body: "", ref_kind: "item", ref_id, source: "app",
      provenance: "stated", tags: vec![], importance: 0.5, salience: 0.0, confidence: 0.7, meta: json!({}),
    }).unwrap()
  }

  #[test]
  fn index_node_upsert_is_idempotent_on_ref() {
    let conn = db();
    let a = node(&conn, "item-1", "first title");
    let b = node(&conn, "item-1", "updated title");
    assert_eq!(a, b, "same ref reuses the node id");
    let n: i64 = conn.query_row("SELECT COUNT(*) FROM brain_nodes", [], |r| r.get(0)).unwrap();
    assert_eq!(n, 1);
    let title: String = conn.query_row("SELECT title FROM brain_nodes WHERE id=?1", params![a], |r| r.get(0)).unwrap();
    assert_eq!(title, "updated title");
  }

  #[test]
  fn edge_upsert_raises_confidence() {
    let conn = db();
    let a = node(&conn, "item-1", "a");
    let b = node(&conn, "item-2", "b");
    add_edge(&conn, &a, &b, "related to", "inferred", 0.4, "first");
    add_edge(&conn, &a, &b, "related to", "inferred", 0.8, "second");
    let (n, c): (i64, f64) = conn.query_row("SELECT COUNT(*), MAX(confidence) FROM brain_edges", [], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
    assert_eq!(n, 1);
    assert!((c - 0.8).abs() < 1e-6);
  }

  #[test]
  fn resolve_node_matches_existing_title() {
    let conn = db();
    let a = node(&conn, "item-1", "Portfolio revamp");
    assert_eq!(resolve_node(&conn, "portfolio revamp", "", "life").as_deref(), Some(a.as_str()));
    assert_eq!(resolve_node(&conn, "nothing here", "", "life"), None);
  }

  #[test]
  fn attention_flags_a_dormant_commitment() {
    let conn = db();
    let id = upsert_node(&conn, &NewNode {
      kind: "goal", scope: "life", title: "Ship the app", body: "", ref_kind: "", ref_id: "",
      source: "app", provenance: "stated", tags: vec![], importance: 0.5, salience: 0.0, confidence: 0.7, meta: json!({}),
    }).unwrap();
    conn.execute("UPDATE brain_nodes SET updated_at=0 WHERE id=?1", params![id]).unwrap();
    compute_attention(&conn);
    let mut dims: Vec<String> = Vec::new();
    if let Ok(mut stmt) = conn.prepare("SELECT dimension FROM attention_state WHERE node_id=?1") {
      if let Ok(rows) = stmt.query_map(params![id], |r| r.get::<_, String>(0)) {
        for r in rows { if let Ok(d) = r { dims.push(d); } }
      }
    }
    assert!(dims.iter().any(|d| d == "displacement"), "a dormant goal is flagged displaced: {dims:?}");
    assert!(dims.iter().any(|d| d == "intention"), "a goal declares intention: {dims:?}");
    assert!(dims.iter().any(|d| d == "dormancy"), "dormancy is tracked: {dims:?}");
  }

  #[test]
  fn two_equal_matches_ask_instead_of_guessing() {
    let conn = db();
    let a = node(&conn, "item-1", "Acme Site");
    let b = node(&conn, "item-2", "Acme Store");
    let amb = resolve_ambiguous(&conn, "acme", "", "life");
    assert!(amb.is_some(), "two equally good contains matches are ambiguous");
    record_clarification(&conn, "life", "acme", &a, &b);
    let n: i64 = conn.query_row("SELECT COUNT(*) FROM brain_observations WHERE dimension='entity_resolution'", [], |r| r.get(0)).unwrap();
    assert_eq!(n, 1, "the question is recorded as an observation");
  }

  #[test]
  fn a_new_decision_supersedes_the_one_it_replaces() {
    let conn = db();
    let old = upsert_node(&conn, &NewNode {
      kind: "decision", scope: "life", title: "Use SQLite for storage", body: "", ref_kind: "", ref_id: "",
      source: "app", provenance: "stated", tags: vec![], importance: 0.5, salience: 0.0, confidence: 0.7, meta: json!({}),
    }).unwrap();
    let new = upsert_node(&conn, &NewNode {
      kind: "decision", scope: "life", title: "Use SQLite for the archive too", body: "", ref_kind: "", ref_id: "",
      source: "app", provenance: "stated", tags: vec![], importance: 0.5, salience: 0.0, confidence: 0.7, meta: json!({}),
    }).unwrap();
    link_supersession(&conn, &new, "decision", "Use SQLite for the archive too", "life");
    let rel: Option<String> = conn.query_row(
      "SELECT relation FROM brain_edges WHERE src_id=?1 AND dst_id=?2", params![new, old], |r| r.get(0),
    ).ok();
    assert_eq!(rel.as_deref(), Some("supersedes"));
  }

  #[test]
  fn forget_node_leaves_referenced_row_alone() {
    let conn = db();
    conn.execute_batch("CREATE TABLE items (id TEXT PRIMARY KEY, title TEXT)").unwrap();
    conn.execute("INSERT INTO items (id, title) VALUES ('item-1','keep me')", []).unwrap();
    let a = node(&conn, "item-1", "keep me");
    let b = node(&conn, "item-2", "other");
    add_edge(&conn, &a, &b, "related to", "stated", 0.9, "");
    forget_node(&conn, &a).unwrap();
    let items: i64 = conn.query_row("SELECT COUNT(*) FROM items", [], |r| r.get(0)).unwrap();
    let edges: i64 = conn.query_row("SELECT COUNT(*) FROM brain_edges", [], |r| r.get(0)).unwrap();
    assert_eq!(items, 1, "the referenced item is untouched");
    assert_eq!(edges, 0, "the node's edges are removed");
  }
}
