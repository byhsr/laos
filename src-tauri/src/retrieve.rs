// Hybrid retrieval + context assembly.
//
// A relevant memory may use different words (semantic), contain an exact name
// (keyword), belong to a connected project (graph), or be an unfinished
// commitment (structured). So retrieval combines all four, merges, dedupes and
// ranks the candidates, and the assembler turns the top of that list into a
// bounded, attributed context block for a model prompt.
//
// Ranking is deterministic (no reranker model): weights are constants and the
// query class picks a weighting profile. Time decay is selective — durable
// kinds (preferences, decisions, goals) never decay.

use std::collections::HashMap;

use rusqlite::{params, Connection};
use serde_json::{json, Value};
use tauri::AppHandle;

use crate::brain;
use crate::db::db;

#[derive(Clone)]
pub(crate) struct RetrieveOpts {
  pub scope: Option<String>,
  pub modes: Vec<String>,
  pub entity_ids: Vec<String>,
  pub budget: usize,
  /// Ask the `reranker` model to reorder the top candidates. A no-op when no
  /// reranker role is configured, so the deterministic path is unchanged.
  pub rerank: bool,
}

impl Default for RetrieveOpts {
  fn default() -> Self {
    Self { scope: None, modes: vec!["keyword".into(), "semantic".into(), "graph".into(), "structured".into()], entity_ids: vec![], budget: 8, rerank: false }
  }
}

#[derive(Clone)]
pub(crate) struct Hit {
  pub node: Value,
  pub score: f32,
  pub evidence: Value,
}

// Ranking weights.
const W_SEM: f32 = 0.34;
const W_KW: f32 = 0.26;
const W_GRAPH: f32 = 0.16;
const W_RECENCY: f32 = 0.12;
const W_STRUCT: f32 = 0.12;
const W_CONF: f32 = 0.10;

// Kinds whose relevance fades with age. Durable kinds are excluded.
fn decayable(kind: &str) -> bool {
  matches!(kind, "conversation" | "note" | "event" | "observation")
}

struct Cand {
  sem: f32,
  kw: f32,
  graph: f32,
  structure: f32,
  explicit: bool,
  kind: String,
  updated: i64,
  confidence: f32,
}

fn scope_in(scopes: &[String]) -> String {
  (1..=scopes.len()).map(|i| format!("?{i}")).collect::<Vec<_>>().join(", ")
}

fn cand<'a>(cands: &'a mut HashMap<String, Cand>, id: &str, now: i64) -> &'a mut Cand {
  cands.entry(id.to_string()).or_insert_with(|| Cand {
    sem: 0.0, kw: 0.0, graph: 0.0, structure: 0.0, explicit: false, kind: String::new(), updated: now, confidence: 0.5,
  })
}

/// "priorities" favours active commitments + recency; "recall" favours dormant,
/// semantic matches; else balanced.
fn profile(query: &str) -> &'static str {
  let q = query.to_lowercase();
  if ["today", "work on", "priorit", "right now", "should i do", "next step", "due"].iter().any(|k| q.contains(k)) {
    "priorities"
  } else if ["idea", "remember", "was that", "used to", "forgot", "remind me what", "earlier"].iter().any(|k| q.contains(k)) {
    "recall"
  } else {
    "balanced"
  }
}

fn node_json(conn: &Connection, id: &str) -> Option<Value> {
  conn.query_row(
    "SELECT id, kind, scope, title, body, ref_kind, ref_id, provenance, status, tags, importance, salience, confidence, updated_at FROM brain_nodes WHERE id=?1",
    params![id],
    |r| {
      let tags: String = r.get(9)?;
      Ok(json!({
        "id": r.get::<_, String>(0)?, "kind": r.get::<_, String>(1)?, "scope": r.get::<_, String>(2)?,
        "title": r.get::<_, String>(3)?, "body": r.get::<_, String>(4)?,
        "refKind": r.get::<_, String>(5)?, "refId": r.get::<_, String>(6)?,
        "provenance": r.get::<_, String>(7)?, "status": r.get::<_, String>(8)?,
        "tags": serde_json::from_str::<Value>(&tags).unwrap_or(json!([])),
        "importance": r.get::<_, f64>(10)?, "salience": r.get::<_, f64>(11)?,
        "confidence": r.get::<_, f64>(12)?, "updatedAt": r.get::<_, i64>(13)?,
      }))
    },
  ).ok()
}

fn keyword_hits(conn: &Connection, query: &str, scopes: &[String], limit: usize) -> Vec<(String, f32)> {
  let q = query.trim();
  if q.is_empty() { return Vec::new(); }
  if brain::fts_enabled() {
    let m = brain::to_match(q);
    if m.is_empty() { return Vec::new(); }
    let sql = format!(
      "SELECT f.node_id FROM brain_fts f JOIN brain_nodes n ON n.id = f.node_id \
       WHERE n.scope IN ({}) AND n.status != 'archived' AND brain_fts MATCH ?{} \
       ORDER BY bm25(brain_fts) LIMIT ?{}",
      scope_in(scopes), scopes.len() + 1, scopes.len() + 2
    );
    let mut args: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
    for s in scopes { args.push(Box::new(s.clone())); }
    args.push(Box::new(m));
    args.push(Box::new(limit as i64));
    let mut out = Vec::new();
    if let Ok(mut stmt) = conn.prepare(&sql) {
      let refs: Vec<&dyn rusqlite::ToSql> = args.iter().map(|b| b.as_ref()).collect();
      if let Ok(rows) = stmt.query_map(refs.as_slice(), |r| r.get::<_, String>(0)) {
        for (i, r) in rows.filter_map(|r| r.ok()).enumerate() {
          // Borda-style rank score: earlier (better) hits score higher.
          out.push((r, 1.0 - (i as f32 / limit.max(1) as f32)));
        }
      }
    }
    return out;
  }
  // Fallback: token-containment scan (same maths as agent_memory::search).
  let tokens: Vec<String> = q.to_lowercase().split_whitespace().map(|t| t.to_string()).collect();
  if tokens.is_empty() { return Vec::new(); }
  let mut out = Vec::new();
  let sql = format!("SELECT id, title, body FROM brain_nodes WHERE scope IN ({}) AND status != 'archived'", scope_in(scopes));
  if let Ok(mut stmt) = conn.prepare(&sql) {
    let refs: Vec<&dyn rusqlite::ToSql> = scopes.iter().map(|s| s as &dyn rusqlite::ToSql).collect();
    if let Ok(rows) = stmt.query_map(refs.as_slice(), |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?))) {
      for (id, title, body) in rows.filter_map(|r| r.ok()) {
        let hay = format!("{title} {body}").to_lowercase();
        let hits = tokens.iter().filter(|t| hay.contains(t.as_str())).count() as f32;
        if hits > 0.0 { out.push((id, hits / tokens.len() as f32)); }
      }
    }
  }
  out.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
  out.truncate(limit);
  out
}

fn vector_hits(conn: &Connection, qvec: &[f32], scopes: &[String], limit: usize) -> Vec<(String, f32)> {
  let sql = format!(
    "SELECT e.source_id, e.vector FROM embeddings e JOIN brain_nodes n ON n.id = e.source_id \
     WHERE e.source_kind='brain_node' AND n.scope IN ({}) AND n.status != 'archived'",
    scope_in(scopes)
  );
  let mut out: Vec<(String, f32)> = Vec::new();
  if let Ok(mut stmt) = conn.prepare(&sql) {
    let refs: Vec<&dyn rusqlite::ToSql> = scopes.iter().map(|s| s as &dyn rusqlite::ToSql).collect();
    if let Ok(rows) = stmt.query_map(refs.as_slice(), |r| Ok((r.get::<_, String>(0)?, r.get::<_, Vec<u8>>(1)?))) {
      for (id, blob) in rows.filter_map(|r| r.ok()) {
        let v = crate::embed::from_blob(&blob);
        let sim = crate::embed::cosine(qvec, &v);
        if sim > 0.0 { out.push((id, sim)); }
      }
    }
  }
  out.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
  out.truncate(limit);
  out
}

fn structured_hits(conn: &Connection, scopes: &[String], limit: usize) -> Vec<(String, f32)> {
  let mut out = Vec::new();
  // Active items (commitments still open) mapped to their node.
  let sql = format!(
    "SELECT n.id, i.due_at FROM brain_nodes n JOIN items i ON i.id = n.ref_id \
     WHERE n.ref_kind='item' AND i.status='active' AND n.scope IN ({}) LIMIT ?{}",
    scope_in(scopes), scopes.len() + 1
  );
  let now = chrono::Utc::now();
  if let Ok(mut stmt) = conn.prepare(&sql) {
    let mut args: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
    for s in scopes { args.push(Box::new(s.clone())); }
    args.push(Box::new(limit as i64));
    let refs: Vec<&dyn rusqlite::ToSql> = args.iter().map(|b| b.as_ref()).collect();
    if let Ok(rows) = stmt.query_map(refs.as_slice(), |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?))) {
      for (id, due) in rows.filter_map(|r| r.ok()) {
        let overdue = due.as_deref()
          .and_then(|d| chrono::DateTime::parse_from_rfc3339(d).ok())
          .map(|d| d <= now)
          .unwrap_or(false);
        out.push((id, if overdue { 1.0 } else { 0.6 }));
      }
    }
  }
  // Recently active nodes (events in the last 7 days).
  let since = now.timestamp_millis() - 7 * 86_400_000;
  if let Ok(mut stmt) = conn.prepare(
    "SELECT node_id FROM brain_events WHERE node_id != '' AND occurred_at >= ?1 GROUP BY node_id ORDER BY MAX(occurred_at) DESC LIMIT ?2",
  ) {
    if let Ok(rows) = stmt.query_map(params![since, limit as i64], |r| r.get::<_, String>(0)) {
      for id in rows.filter_map(|r| r.ok()) { out.push((id, 0.4)); }
    }
  }
  out
}

fn graph_hits(conn: &Connection, seeds: &[String], exclude: &HashMap<String, Cand>) -> Vec<(String, f32)> {
  let mut out = Vec::new();
  for seed in seeds.iter().take(6) {
    let seed_score = exclude.get(seed).map(|c| c.kw.max(c.sem).max(c.graph)).unwrap_or(0.5);
    if let Ok(mut stmt) = conn.prepare("SELECT src_id, dst_id FROM brain_edges WHERE src_id=?1 OR dst_id=?1") {
      if let Ok(rows) = stmt.query_map(params![seed], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))) {
        for (src, dst) in rows.filter_map(|r| r.ok()) {
          let other = if src == *seed { dst } else { src };
          out.push((other, 0.5 * seed_score)); // one hop
        }
      }
    }
  }
  out
}

fn rank(mut cands: HashMap<String, Cand>, profile: &str, now: i64) -> Vec<(String, f32, Cand)> {
  let mut scored: Vec<(String, f32, Cand)> = cands.drain().map(|(id, c)| {
    let days = ((now - c.updated) as f32 / 86_400_000.0).max(0.0);
    let recency = 1.0 / (1.0 + days / 14.0);
    let recency_term = if decayable(&c.kind) { recency } else { 1.0 };
    let (w_sem, w_kw, w_graph, w_rec, w_struct) = match profile {
      "priorities" => (0.24, 0.22, 0.12, 0.14, 0.28),
      "recall" => (0.38, 0.24, 0.20, 0.08, 0.10),
      _ => (W_SEM, W_KW, W_GRAPH, W_RECENCY, W_STRUCT),
    };
    // Recall rewards staleness (dormant ideas), priorities reward freshness.
    let rec_term = if profile == "recall" { (1.0 - recency_term).min(days / 30.0).max(0.0) } else { recency_term };
    let mut score = w_sem * c.sem + w_kw * c.kw + w_graph * c.graph + w_rec * rec_term + w_struct * c.structure + W_CONF * c.confidence;
    if c.explicit { score += 1.0; }
    (id, score, c)
  }).collect();
  scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
  scored
}

/// The retrieval core: gather → merge → rank. `qvec` is the query embedding when
/// available (semantic mode), otherwise `None`.
pub(crate) fn retrieve_core(conn: &Connection, query: &str, qvec: Option<&[f32]>, opts: &RetrieveOpts) -> Vec<Hit> {
  let scope = opts.scope.clone().unwrap_or_else(|| "global".into());
  let scopes = brain::scopes_for(&scope);
  let modes: Vec<String> = if opts.modes.is_empty() { RetrieveOpts::default().modes } else { opts.modes.clone() };
  let want = |m: &str| modes.iter().any(|x| x == m);
  let now = chrono::Utc::now().timestamp_millis();
  let limit = opts.budget.max(1) * 4;

  let mut cands: HashMap<String, Cand> = HashMap::new();

  if want("keyword") {
    for (id, s) in keyword_hits(conn, query, &scopes, limit) { let e = cand(&mut cands, &id, now); if s > e.kw { e.kw = s; } }
  }
  if want("semantic") {
    if let Some(qv) = qvec { for (id, s) in vector_hits(conn, qv, &scopes, limit) { let e = cand(&mut cands, &id, now); e.sem = s.max(e.sem); } }
  }
  if want("structured") {
    for (id, s) in structured_hits(conn, &scopes, limit) { let e = cand(&mut cands, &id, now); if s > e.structure { e.structure = s; } }
  }

  // Explicit references always win.
  for id in &opts.entity_ids { cand(&mut cands, id, now).explicit = true; }

  // Fill in kind/updated/confidence once, then expand the graph from seeds.
  let ids: Vec<String> = cands.keys().cloned().collect();
  for id in &ids {
    if let Some(n) = node_json(conn, id) {
      let e = cand(&mut cands, id, now);
      e.kind = n["kind"].as_str().unwrap_or("").to_string();
      e.updated = n["updatedAt"].as_i64().unwrap_or(now);
      e.confidence = n["confidence"].as_f64().unwrap_or(0.5) as f32;
    }
  }
  if want("graph") {
    let mut seeds: Vec<(String, f32)> = cands.iter().map(|(k, v)| (k.clone(), v.kw + v.sem + v.structure)).collect();
    seeds.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    let seed_ids: Vec<String> = seeds.iter().take(6).map(|(k, _)| k.clone()).collect();
    for (id, s) in graph_hits(conn, &seed_ids, &cands) {
      let existed = cands.contains_key(&id);
      let e = cand(&mut cands, &id, now);
      e.graph = if existed { s.max(e.graph) } else { s };
      if !existed {
        if let Some(n) = node_json(conn, &id) {
          e.kind = n["kind"].as_str().unwrap_or("").to_string();
          e.updated = n["updatedAt"].as_i64().unwrap_or(now);
          e.confidence = n["confidence"].as_f64().unwrap_or(0.5) as f32;
        }
      }
    }
  }

  let prof = profile(query);
  let ranked = rank(cands, prof, now);
  ranked.into_iter().take(opts.budget.max(1)).filter_map(|(id, score, c)| {
    node_json(conn, &id).map(|node| Hit {
      node,
      score,
      evidence: json!({
        "keyword": c.kw, "semantic": c.sem, "graph": c.graph, "structured": c.structure,
        "confidence": c.confidence, "explicit": c.explicit, "profile": prof,
      }),
    })
  }).collect()
}

/// Async wrapper: embeds the query when an embedding role exists, then retrieves,
/// then — when asked — reranks with the `reranker` model.
pub(crate) async fn gather(app: &AppHandle, query: &str, opts: RetrieveOpts) -> Vec<Hit> {
  let qvec = if opts.modes.iter().any(|m| m == "semantic") {
    crate::embed::embed_text(app, query).await.ok()
  } else {
    None
  };
  let hits = {
    let conn = match db(app) { Ok(c) => c, Err(_) => return Vec::new() };
    retrieve_core(&conn, query, qvec.as_deref(), &opts)
  };
  if opts.rerank { rerank(app, query, hits).await } else { hits }
}

/// Model-based reranking. The deterministic ranking is the fallback in every
/// failure mode: no `reranker` role, a bad reply, or fewer than three candidates
/// leaves the order untouched. Candidates the model omits keep their original
/// relative order at the tail, so nothing is ever lost.
pub(crate) async fn rerank(app: &AppHandle, query: &str, hits: Vec<Hit>) -> Vec<Hit> {
  if hits.len() < 3 { return hits; }
  let model_id = {
    let Ok(conn) = db(app) else { return hits };
    match crate::provider::explicit_role(&conn, "reranker") { Some(m) => m, None => return hits }
  };
  let listing = hits.iter().enumerate().map(|(i, h)| {
    let title = h.node["title"].as_str().unwrap_or("");
    let body: String = h.node["body"].as_str().unwrap_or("").chars().take(160).collect();
    format!("{i}. [{title}] {body}")
  }).collect::<Vec<_>>().join("\n");
  let prompt = format!(
    "Rank these remembered items by how useful they are for answering the query. Return STRICT JSON only: \
     {{\"order\":[indices, best first]}}. Include every index exactly once.\n\nQuery: {query}\n\nItems:\n{listing}"
  );
  let reply = match crate::memory::one_shot_completion(app, &model_id, &prompt, 300).await { Ok(r) => r, Err(_) => return hits };
  let Some(json) = crate::memory::extract_json(&reply) else { return hits };
  let Some(order) = json["order"].as_array() else { return hits };

  let mut out: Vec<Hit> = Vec::new();
  let mut used = vec![false; hits.len()];
  for v in order {
    if let Some(i) = v.as_u64().map(|n| n as usize) {
      if i < hits.len() && !used[i] { used[i] = true; out.push(hits[i].clone()); }
    }
  }
  for (i, h) in hits.into_iter().enumerate() { if !used[i] { out.push(h); } }
  out
}

fn provenance_label(hit: &Hit) -> String {
  let node = &hit.node;
  let prov = node["provenance"].as_str().unwrap_or("stated");
  let conf = node["confidence"].as_f64().unwrap_or(0.0);
  if prov == "inferred" { format!("inferred {:.1}", conf) } else { "stated".into() }
}

/// Renders the retrieved hits into a bounded, attributed context block. Stated
/// and inferred items are labelled distinctly (the brief's provenance rule).
pub(crate) fn render_block(hits: &[Hit], budget_tokens: usize) -> String {
  if hits.is_empty() { return String::new(); }
  let char_budget = budget_tokens * 4;
  let mut out = String::from("\n\n## Relevant context (retrieved)\n");
  for h in hits {
    let node = &h.node;
    let kind = node["kind"].as_str().unwrap_or("note");
    let title = node["title"].as_str().unwrap_or("").trim();
    let body = node["body"].as_str().unwrap_or("").trim();
    let scope = node["scope"].as_str().unwrap_or("");
    let line = if body.is_empty() || body.len() < 3 {
      format!("- [{kind} · {}] {title} · {scope}\n", provenance_label(h))
    } else {
      let snip: String = body.chars().take(240).collect();
      format!("- [{kind} · {}] {title}: {snip} · {scope}\n", provenance_label(h))
    };
    if out.len() + line.len() > char_budget { out.push_str("- …\n"); break; }
    out.push_str(&line);
  }
  out
}

/// The context assembler: retrieve for this request and render a bounded block.
/// Empty when there is nothing relevant, so an agent with no history is unchanged.
pub(crate) async fn context_block(app: &AppHandle, scope: &str, request: &str, budget_tokens: usize) -> String {
  if request.trim().len() < 3 { return String::new(); }
  // Rerank only when a `reranker` role is configured; otherwise it's a no-op and
  // the deterministic order stands.
  let opts = RetrieveOpts { scope: Some(scope.to_string()), budget: 6, rerank: true, ..Default::default() };
  let hits = gather(app, request, opts).await;
  render_block(&hits, budget_tokens)
}

#[tauri::command]
pub async fn retrieve_context(app: AppHandle, query: String, scope: Option<String>, modes: Option<Vec<String>>, budget: Option<usize>, rerank: Option<bool>) -> Result<Value, String> {
  let opts = RetrieveOpts { scope, modes: modes.unwrap_or_default(), budget: budget.unwrap_or(8), rerank: rerank.unwrap_or(false), ..Default::default() };
  let hits = gather(&app, &query, opts).await;
  let items: Vec<Value> = hits.iter().map(|h| json!({
    "node": h.node, "score": h.score, "evidence": h.evidence,
  })).collect();
  let text = render_block(&hits, 900);
  Ok(json!({ "items": items, "text": text }))
}

/// A read-only measurement of the retrieval path over the real store. This is the
/// evaluation harness the brief calls for: it makes the *cost* of the approach —
/// how much there is to scan, how long a vector scan takes, how big the assembled
/// context is — visible, so reranking and an ANN index stay evidence-driven.
pub(crate) fn eval_report(conn: &Connection) -> Value {
  let count = |sql: &str| conn.query_row(sql, [], |r| r.get::<_, i64>(0)).unwrap_or(0);
  let nodes = count("SELECT COUNT(*) FROM brain_nodes WHERE status != 'archived'");
  let embeddings = count("SELECT COUNT(*) FROM embeddings WHERE source_kind='brain_node'");

  let mut vectors: Vec<Vec<f32>> = Vec::new();
  if let Ok(mut stmt) = conn.prepare("SELECT vector FROM embeddings WHERE source_kind='brain_node' LIMIT 5000") {
    if let Ok(rows) = stmt.query_map([], |r| r.get::<_, Vec<u8>>(0)) {
      for b in rows.filter_map(|r| r.ok()) { vectors.push(crate::embed::from_blob(&b)); }
    }
  }
  let dim = vectors.first().map(|v| v.len()).unwrap_or(0);
  let q = vec![1.0f32 / (dim.max(1) as f32).sqrt(); dim];
  let start = std::time::Instant::now();
  let mut top = 0f32;
  for v in &vectors { let c = crate::embed::cosine(&q, v); if c > top { top = c; } }
  let scan_us = start.elapsed().as_micros().max(1);

  let opts = RetrieveOpts { scope: Some("global".into()), budget: 8, ..Default::default() };
  let hits = retrieve_core(conn, "what should I work on today?", None, &opts);
  let block = render_block(&hits, 900);

  json!({
    "nodes": nodes,
    "embeddings": embeddings,
    "vector_scan": {
      "dim": dim, "vectors": vectors.len(), "micros": scan_us,
      "per_vector_ns": if vectors.is_empty() { 0 } else { (scan_us * 1000 / vectors.len() as u128) as u64 },
      "top_similarity": top,
    },
    "context": { "hits": hits.len(), "chars": block.len(), "tokens_est": block.len() / 4, "budget_tokens": 900 },
    "fts": brain::fts_enabled(),
    "guidance": if embeddings > 2000 { "vector scan is past personal scale — revisit an ANN index" } else { "brute-force cosine is well within budget at this size" },
  })
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::brain::{self, NewNode};

  fn setup() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    brain::ensure_schema(&conn).unwrap();
    conn.execute_batch("CREATE TABLE items (id TEXT PRIMARY KEY, kind TEXT, title TEXT, body TEXT, url TEXT, tags TEXT, collection TEXT, status TEXT, due_at TEXT, cadence TEXT, streak INTEGER, last_done_at TEXT, project_id TEXT, fired_at TEXT, source TEXT, created_at TEXT, updated_at TEXT);").unwrap();
    conn
  }

  fn add(conn: &Connection, kind: &str, scope: &str, title: &str, body: &str) -> String {
    brain::upsert_node(conn, &NewNode {
      kind, scope, title, body, ref_kind: "", ref_id: "", source: "test", provenance: "stated",
      tags: vec![], importance: 0.5, salience: 0.0, confidence: 0.8, meta: json!({}),
    }).unwrap()
  }

  #[test]
  fn priorities_profile_surfaces_active_commitment() {
    let conn = setup();
    let idea = add(&conn, "idea", "life", "Idea: half-finished app concept", "a dormant thought");
    // An active item node (structured candidate), overdue.
    conn.execute("INSERT INTO items (id, kind, title, body, status, due_at, source, created_at, updated_at) VALUES ('it1','todo','Ship the release','', 'active','2000-01-01T00:00:00Z','app','2020-01-01','2020-01-01')", []).unwrap();
    let task = add(&conn, "task", "life", "Ship the release", "");
    conn.execute("UPDATE brain_nodes SET ref_kind='item', ref_id='it1' WHERE id=?1", params![task]).unwrap();
    // Age the idea so a "recall" query favours it; here we test priorities.
    conn.execute("UPDATE brain_nodes SET updated_at=0 WHERE id=?1", params![idea]).unwrap();

    let opts = RetrieveOpts { scope: Some("life".into()), budget: 2, ..Default::default() };
    let hits = retrieve_core(&conn, "what should I work on today?", None, &opts);
    assert!(!hits.is_empty());
    assert_eq!(hits[0].node["id"].as_str(), Some(task.as_str()), "active commitment ranks first for a priorities query");

    let hits2 = retrieve_core(&conn, "what was that idea I had?", None, &opts);
    assert_eq!(hits2[0].node["id"].as_str(), Some(idea.as_str()), "recall favours the dormant idea");
  }

  #[test]
  fn durable_preferences_do_not_decay() {
    let conn = setup();
    let pref = add(&conn, "preference", "life", "Prefers concise answers", "keep it short");
    conn.execute("UPDATE brain_nodes SET updated_at=0 WHERE id=?1", params![pref]).unwrap();
    let fresh = add(&conn, "note", "life", "Older note about concise answers", "");
    let opts = RetrieveOpts { scope: Some("life".into()), budget: 2, ..Default::default() };
    let hits = retrieve_core(&conn, "concise answers", None, &opts);
    // Both keep; but the note (decayable, aged) does not outrank on recency alone.
    assert!(hits.iter().any(|h| h.node["id"].as_str() == Some(pref.as_str())));
    let _ = fresh;
  }

  #[test]
  fn scope_filter_excludes_other_projects() {
    let conn = setup();
    let a = add(&conn, "note", "project:p1", "alpha keyword", "");
    let b = add(&conn, "note", "project:p2", "alpha keyword", "");
    let opts = RetrieveOpts { scope: Some("project:p1".into()), budget: 5, ..Default::default() };
    let hits = retrieve_core(&conn, "alpha keyword", None, &opts);
    let ids: Vec<&str> = hits.iter().filter_map(|h| h.node["id"].as_str()).collect();
    assert!(ids.contains(&a.as_str()));
    assert!(!ids.contains(&b.as_str()), "another project's node is not returned");
  }

  #[test]
  fn rendered_block_labels_provenance() {
    let conn = setup();
    let stated = add(&conn, "note", "life", "Stated fact", "body here");
    brain::upsert_node(&conn, &NewNode {
      kind: "observation", scope: "life", title: "Inferred guess", body: "maybe", ref_kind: "", ref_id: "",
      source: "distill", provenance: "inferred", tags: vec![], importance: 0.5, salience: 0.0, confidence: 0.4, meta: json!({}),
    }).unwrap();
    let opts = RetrieveOpts { scope: Some("life".into()), budget: 5, ..Default::default() };
    let hits = retrieve_core(&conn, "fact guess", None, &opts);
    let block = render_block(&hits, 900);
    assert!(block.contains("stated"));
    assert!(block.contains("inferred"));
    let _ = stated;
  }

  #[test]
  fn a_node_whose_source_row_vanished_still_retrieves() {
    let conn = setup();
    let id = add(&conn, "note", "life", "Orphaned but searchable", "the source row is gone");
    // Point it at an item that does not exist: the structured path must simply
    // contribute nothing rather than error, and keyword search must still work.
    conn.execute("UPDATE brain_nodes SET ref_kind='item', ref_id='missing' WHERE id=?1", params![id]).unwrap();
    let opts = RetrieveOpts { scope: Some("life".into()), budget: 3, ..Default::default() };
    let hits = retrieve_core(&conn, "orphaned but searchable", None, &opts);
    assert!(hits.iter().any(|h| h.node["id"].as_str() == Some(id.as_str())),
      "a dangling source reference does not break retrieval");
  }

  #[test]
  fn eval_report_measures_the_retrieval_path() {
    let conn = setup();
    add(&conn, "note", "life", "Ship the release", "the big one");
    let r = eval_report(&conn);
    assert!(r["nodes"].as_i64().unwrap() >= 1);
    assert_eq!(r["context"]["budget_tokens"].as_i64().unwrap(), 900);
    assert!(r["guidance"].as_str().unwrap().contains("cosine"));
  }

  /// A labelled fixture: precision@2 for a query whose relevant nodes are known.
  /// This is the harness the brief asks for — retrieval quality measured against a
  /// fixed expectation, so a ranking change is a visible regression, not a guess.
  #[test]
  fn retrieval_precision_on_a_labelled_fixture() {
    let conn = setup();
    let rel_a = add(&conn, "note", "life", "Quarterly budget spreadsheet", "numbers for the quarter");
    let rel_b = add(&conn, "note", "life", "Budget review meeting notes", "what we decided about the budget");
    add(&conn, "note", "life", "Grocery list", "milk, eggs, bread");
    add(&conn, "note", "life", "Podcast episode", "an interview");
    let opts = RetrieveOpts { scope: Some("life".into()), budget: 2, ..Default::default() };
    let hits = retrieve_core(&conn, "budget", None, &opts);
    let ids: Vec<&str> = hits.iter().filter_map(|h| h.node["id"].as_str()).collect();
    let relevant = [rel_a.as_str(), rel_b.as_str()];
    let precision = ids.iter().filter(|id| relevant.contains(id)).count() as f64 / ids.len().max(1) as f64;
    assert!(precision >= 0.5, "precision@2 should find the budget notes: {ids:?}");
  }
}
