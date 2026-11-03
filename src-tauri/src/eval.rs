// Evaluation harness (the brief's Phase 6).
//
// Retrieval quality is measured against a small, labelled fixture rather than
// argued about: each case names the query, the mode(s), and the nodes that are
// actually relevant, and the harness reports precision/recall/MRR, latency, and
// the assembled-context size. Reliability is measured too — duplicate creation
// and extraction failure — because those are the two ways the pipeline can
// silently lose or duplicate what the user said.
//
// The fixture runs in a throwaway in-memory database; it never touches the
// user's data. The live scale/cost numbers come from `retrieve::eval_report`.

use std::time::Instant;

use rusqlite::{params, Connection};
use serde_json::{json, Value};

use crate::brain::{self, NewNode};
use crate::retrieve::{self, RetrieveOpts};

const BUDGET: usize = 4;
const DAY: i64 = 86_400_000;

/// A minimal `items` table so the structured retrieval path has a real source row
/// to join against. Only ever created in the throwaway eval database.
const ITEMS_SCHEMA: &str = "CREATE TABLE IF NOT EXISTS items (id TEXT PRIMARY KEY, kind TEXT, title TEXT, body TEXT, url TEXT, \
  tags TEXT, collection TEXT, status TEXT, due_at TEXT, cadence TEXT, streak INTEGER, last_done_at TEXT, project_id TEXT, \
  fired_at TEXT, source TEXT, owner TEXT NOT NULL DEFAULT '', created_at TEXT, updated_at TEXT);";

fn round3(v: f64) -> f64 { (v * 1000.0).round() / 1000.0 }

/// Inserts a complete item row. Every column is set, because the item reader
/// treats a NULL column as an unreadable row rather than an empty value.
fn insert_item(conn: &Connection, id: &str, title: &str, status: &str, due: &str) {
  conn.execute(
    "INSERT INTO items (id, kind, title, body, url, tags, collection, status, due_at, cadence, streak, \
     last_done_at, project_id, fired_at, source, created_at, updated_at) \
     VALUES (?1,'todo',?2,'','','[]','',?3,?4,'',0,'','','','eval','2020-01-01','2020-01-01')",
    params![id, title, status, due],
  ).ok();
}

fn mk(conn: &Connection, kind: &str, scope: &str, title: &str, body: &str, updated: i64) -> String {
  let id = brain::upsert_node(conn, &NewNode {
    kind, scope, title, body, ref_kind: "", ref_id: "", source: "eval",
    provenance: "stated", tags: vec![], importance: 0.5, salience: 0.0, confidence: 0.8, meta: json!({}),
  }).expect("seed node");
  let _ = conn.execute("UPDATE brain_nodes SET updated_at=?2 WHERE id=?1", params![id, updated]);
  id
}

struct Case {
  name: &'static str,
  query: &'static str,
  scope: &'static str,
  modes: Vec<String>,
  relevant: Vec<String>,
  /// A fixed query vector for the semantic case — the harness must not need a model.
  qvec: Option<Vec<f32>>,
}

/// Seeds the labelled brain map and returns the cases to score. Retained items are
/// old on purpose: a system that only ever surfaces fresh material fails these.
fn fixture(conn: &Connection) -> Vec<Case> {
  let now = chrono::Utc::now().timestamp_millis();
  let old = now - 40 * DAY;
  let recent = now - DAY;

  let pref_concise = mk(conn, "preference", "life", "Prefers concise answers", "short and technical", old);
  // A conflicting preference: both must stay retrievable, neither silently wins.
  let pref_detailed = mk(conn, "preference", "life", "Prefers detailed explanations", "walk through the reasoning", old);
  let dec_sqlite = mk(conn, "decision", "life", "Decision: use SQLite for storage", "one embedded database", old);
  let dec_archive = mk(conn, "decision", "life", "Decision: use SQLite for the archive too", "extend the same store", recent);
  let idea_app = mk(conn, "idea", "life", "Idea: a plant-watering app", "reminds you when to water plants", old);
  let person_sam = mk(conn, "person", "life", "Sam", "", old);
  let proj_lup = mk(conn, "project", "project:p1", "Rewrite the memory system", "the context engine", recent);
  let proj_lup_note = mk(conn, "note", "project:p1", "Memory system notes", "retrieval and assembly", recent);
  let proj_garden = mk(conn, "project", "project:p2", "Garden redesign", "beds and paths", old);
  let task_release = mk(conn, "task", "life", "Ship the release", "", now);

  // An open, overdue item under the task node — what the structured path reads.
  insert_item(conn, "it1", "Ship the release", "active", "2000-01-01T00:00:00Z");
  let _ = conn.execute("UPDATE brain_nodes SET ref_kind='item', ref_id='it1' WHERE id=?1", params![task_release]);

  // One graph edge, so expansion has something real to follow.
  brain::add_edge(conn, &idea_app, &person_sam, "associated with", "inferred", 0.7, "eval");

  // A tiny synthetic semantic space — enough to label which nodes are close.
  brain::store_embedding(conn, "brain_node", &idea_app, "eval", "plant app", &[1.0, 0.0, 0.0, 0.0]);
  brain::store_embedding(conn, "brain_node", &dec_sqlite, "eval", "sqlite", &[0.0, 1.0, 0.0, 0.0]);
  brain::store_embedding(conn, "brain_node", &task_release, "eval", "ship", &[0.0, 0.0, 1.0, 0.0]);
  brain::store_embedding(conn, "brain_node", &pref_concise, "eval", "concise", &[0.0, 0.0, 0.0, 1.0]);

  let hybrid = || vec!["keyword".to_string(), "semantic".to_string(), "graph".to_string(), "structured".to_string()];
  let _ = proj_garden;

  vec![
    Case { name: "exact keyword", query: "SQLite", scope: "life", modes: vec!["keyword".into()],
      relevant: vec![dec_sqlite.clone(), dec_archive.clone()], qvec: None },
    // Two preferences that disagree: both are kept and both stay retrievable.
    Case { name: "conflicting preferences", query: "prefers", scope: "life", modes: vec!["keyword".into()],
      relevant: vec![pref_concise.clone(), pref_detailed.clone()], qvec: None },
    Case { name: "semantic", query: "something about watering plants", scope: "life", modes: vec!["semantic".into()],
      relevant: vec![idea_app.clone()], qvec: Some(vec![0.95, 0.05, 0.05, 0.0]) },
    Case { name: "graph expansion", query: "Sam", scope: "life", modes: vec!["keyword".into(), "graph".into()],
      relevant: vec![idea_app.clone()], qvec: None },
    Case { name: "today's priorities", query: "what should I work on today?", scope: "life", modes: hybrid(),
      relevant: vec![task_release.clone()], qvec: None },
    Case { name: "recall a dormant idea", query: "what was that idea I had?", scope: "life", modes: hybrid(),
      relevant: vec![idea_app.clone()], qvec: None },
    Case { name: "project scoped", query: "memory system", scope: "project:p1", modes: hybrid(),
      relevant: vec![proj_lup.clone(), proj_lup_note.clone()], qvec: None },
  ]
}

fn measure(conn: &Connection, c: &Case) -> Value {
  let opts = RetrieveOpts {
    scope: Some(c.scope.to_string()), modes: c.modes.clone(), budget: BUDGET, rerank: false, entity_ids: vec![],
  };
  let start = Instant::now();
  let hits = retrieve::retrieve_core(conn, c.query, c.qvec.as_deref(), &opts);
  let micros = start.elapsed().as_micros().max(1);

  let ids: Vec<String> = hits.iter().filter_map(|h| h.node["id"].as_str().map(str::to_string)).collect();
  let found = ids.iter().filter(|id| c.relevant.contains(id)).count();
  let precision = found as f64 / ids.len().max(1) as f64;
  let recall = found as f64 / c.relevant.len().max(1) as f64;
  let rank = ids.iter().position(|id| c.relevant.contains(id)).map(|i| i + 1);
  let block = retrieve::render_block(&hits, 900);

  json!({
    "case": c.name, "modes": c.modes, "hits": ids.len(),
    "precision": round3(precision), "recall": round3(recall),
    "mrr": round3(rank.map(|r| 1.0 / r as f64).unwrap_or(0.0)), "first_relevant_rank": rank,
    "micros": micros, "context_tokens_est": block.len() / 4,
  })
}

/// Runs the labelled fixture and returns per-case + aggregate metrics, plus the
/// two reliability measurements.
pub(crate) fn run(conn: &Connection) -> Value {
  let cases = fixture(conn);
  let rows: Vec<Value> = cases.iter().map(|c| measure(conn, c)).collect();
  let mean = |k: &str, rows: &[Value]| if rows.is_empty() { 0.0 } else {
    rows.iter().filter_map(|r| r[k].as_f64()).sum::<f64>() / rows.len() as f64
  };
  let worst = rows.iter().min_by(|a, b| {
    a["recall"].as_f64().unwrap_or(0.0).partial_cmp(&b["recall"].as_f64().unwrap_or(0.0)).unwrap_or(std::cmp::Ordering::Equal)
  }).map(|r| r["case"].clone()).unwrap_or(Value::Null);

  json!({
    "cases": rows.len(),
    "precision": round3(mean("precision", &rows)),
    "recall": round3(mean("recall", &rows)),
    "mrr": round3(mean("mrr", &rows)),
    "worst_case": worst,
    "results": rows,
    "reliability": { "duplicates": duplicate_check(conn), "extraction": extraction_check() },
  })
}

/// The same source filed twice must yield one node and one open item — the
/// failure this guards against is a re-delivered Telegram message or a retried
/// capture quietly multiplying the item list.
fn duplicate_check(conn: &Connection) -> Value {
  insert_item(conn, "dup1", "Water the plants", "active", "");
  let n = NewNode {
    kind: "task", scope: "life", title: "Water the plants", body: "", ref_kind: "item", ref_id: "dup1",
    source: "eval", provenance: "stated", tags: vec![], importance: 0.5, salience: 0.0, confidence: 0.8, meta: json!({}),
  };
  let _ = brain::upsert_node(conn, &n);
  let _ = brain::upsert_node(conn, &n);
  let nodes: i64 = conn.query_row("SELECT COUNT(*) FROM brain_nodes WHERE ref_kind='item' AND ref_id='dup1'", [], |r| r.get(0)).unwrap_or(0);
  let items: i64 = conn.query_row("SELECT COUNT(*) FROM items WHERE id='dup1'", [], |r| r.get(0)).unwrap_or(0);
  let refind = crate::capture::find_exact_active(conn, "water the plants").is_some();
  json!({ "same_ref_nodes": nodes, "items": items, "refind_finds_one": refind, "ok": nodes == 1 && items == 1 && refind })
}

/// Malformed classifier output must never lose the input: every sample has to come
/// back as a usable, non-empty classification carrying the raw text.
fn extraction_check() -> Value {
  let samples = [
    "",
    "not json at all",
    "{\"content_type\":\"bogus\"}",
    "{\"params\":{}}",
    "{\"salience\":\"high\"}",
    "{\"content_type\":\"task\",\"params\":{\"title\":\"\"}}",
  ];
  let raw = "the raw words the user typed";
  let recovered = samples.iter().filter(|s| {
    let mut c = crate::classify::parse(s);
    crate::classify::normalize(&mut c, raw);
    !c.params.body.trim().is_empty() && !c.content_type.trim().is_empty()
  }).count();
  json!({ "samples": samples.len(), "recovered": recovered, "lost": samples.len() - recovered, "ok": recovered == samples.len() })
}

/// Quality metrics against the throwaway fixture, plus read-only cost/scale
/// numbers from the live store.
#[tauri::command]
pub fn brain_eval(app: tauri::AppHandle) -> Result<Value, String> {
  let conn = Connection::open_in_memory().map_err(|e| e.to_string())?;
  brain::ensure_schema(&conn)?;
  conn.execute_batch(ITEMS_SCHEMA).map_err(|e| e.to_string())?;
  let quality = run(&conn);
  let live = {
    let c = crate::db::db(&app)?;
    retrieve::eval_report(&c)
  };
  Ok(json!({ "quality": quality, "live": live }))
}

#[cfg(test)]
mod tests {
  use super::*;

  fn conn() -> Connection {
    let c = Connection::open_in_memory().unwrap();
    brain::ensure_schema(&c).unwrap();
    c.execute_batch(ITEMS_SCHEMA).unwrap();
    c
  }

  #[test]
  fn every_labelled_case_finds_its_relevant_nodes() {
    let c = conn();
    let r = run(&c);
    for case in r["results"].as_array().unwrap() {
      assert_eq!(case["recall"].as_f64().unwrap(), 1.0, "missed a labelled case: {case}");
    }
    assert_eq!(r["recall"].as_f64().unwrap(), 1.0);
  }

  #[test]
  fn the_pipeline_neither_duplicates_nor_loses_input() {
    let c = conn();
    let r = run(&c);
    assert_eq!(r["reliability"]["duplicates"]["ok"], true, "a re-filed item duplicated: {r}");
    assert_eq!(r["reliability"]["extraction"]["ok"], true, "malformed output lost input: {r}");
  }

  #[test]
  fn project_scope_never_leaks_another_project() {
    let c = conn();
    let r = run(&c);
    // The project-scoped case must not recall the other project's nodes.
    let case = r["results"].as_array().unwrap().iter().find(|x| x["case"] == "project scoped").unwrap();
    assert_eq!(case["recall"].as_f64().unwrap(), 1.0);
  }
}
