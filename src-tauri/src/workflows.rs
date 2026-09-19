// Workflow rule engine: deterministic checker rules, JSON path/expression
// evaluation, the LLM-judge escape hatch, and linear workflow execution.

use rusqlite::params;
use tauri::AppHandle;

use crate::agents::run_agent_once;
use crate::db::db;
use crate::http;
use crate::integrations::integration_secret;
use crate::models::*;
use crate::storage::{parse_json_vec, stored_api_key};
use crate::tools::{AgentTool, IntegrationTool};

// Evaluate a single deterministic rule against the given value. Returns
// Ok(true/false) or Err for a malformed rule. `path` resolution is done by
// callers via resolve_path.
fn eval_rule(rule: &serde_json::Value, value: &serde_json::Value) -> Result<bool, String> {
  let rtype = rule.get("type").and_then(|v| v.as_str()).unwrap_or("").to_string();
  match rtype.as_str() {
    "contains" => {
      let needle = rule.get("value").and_then(|v| v.as_str()).unwrap_or("").to_string();
      let hay = value.as_str().ok_or("contains: input is not a string")?;
      Ok(hay.contains(&needle))
    }
    "notContains" => {
      let needle = rule.get("value").and_then(|v| v.as_str()).unwrap_or("").to_string();
      let hay = value.as_str().ok_or("notContains: input is not a string")?;
      Ok(!hay.contains(&needle))
    }
    "nonEmpty" => Ok(!(value.as_str().map(|s| s.trim().is_empty()).unwrap_or(value.is_null()))),
    "startsWith" => {
      let needle = rule.get("value").and_then(|v| v.as_str()).unwrap_or("").to_string();
      Ok(value.as_str().map(|s| s.starts_with(&needle)).unwrap_or(false))
    }
    "endsWith" => {
      let needle = rule.get("value").and_then(|v| v.as_str()).unwrap_or("").to_string();
      Ok(value.as_str().map(|s| s.ends_with(&needle)).unwrap_or(false))
    }
    "regex" => {
      let pattern = rule.get("pattern").and_then(|v| v.as_str()).unwrap_or("").to_string();
      let mode = rule.get("mode").and_then(|v| v.as_str()).unwrap_or("mustMatch").to_string();
      let re = regex::Regex::new(&pattern).map_err(|e| format!("regex: bad pattern ({e})"))?;
      let s = value.as_str().ok_or("regex: input is not a string")?;
      let matched = re.is_match(s);
      Ok(if mode == "mustNotMatch" { !matched } else { matched })
    }
    "validJson" => {
      Ok(value.as_str().map(|s| serde_json::from_str::<serde_json::Value>(s).is_ok()).unwrap_or(false))
    }
    "equals" => {
      let expected = rule.get("value").and_then(|v| v.as_str()).unwrap_or("").to_string();
      Ok(value.as_str().map(|s| s == expected).unwrap_or(false))
    }
    "notEquals" => {
      let expected = rule.get("value").and_then(|v| v.as_str()).unwrap_or("").to_string();
      Ok(value.as_str().map(|s| s != expected).unwrap_or(false))
    }
    "lengthRange" => {
      let min = rule.get("min").and_then(|v| v.as_u64()).unwrap_or(0);
      let max = rule.get("max").and_then(|v| v.as_u64()).unwrap_or(u64::MAX);
      let len = value.as_str().map(|s| s.chars().count() as u64).ok_or("lengthRange: input is not a string")?;
      Ok(len >= min && len <= max)
    }
    "numericRange" => {
      let min = rule.get("min").and_then(|v| v.as_f64()).unwrap_or(f64::MIN);
      let max = rule.get("max").and_then(|v| v.as_f64()).unwrap_or(f64::MAX);
      let num = value.as_f64().or_else(|| value.as_str().and_then(|s| s.trim().parse::<f64>().ok()))
        .ok_or("numericRange: input is not a number")?;
      Ok(num >= min && num <= max)
    }
    "hasField" => {
      let path = rule.get("path").and_then(|v| v.as_str()).unwrap_or("").to_string();
      let target = resolve_path(value, &path);
      let present = !target.is_null();
      Ok(if rule.get("min").and_then(|v| v.as_u64()).unwrap_or(0) > 0 {
        present && matches!(target, serde_json::Value::Array(a) if a.len() as u64 >= rule.get("min").and_then(|v| v.as_u64()).unwrap_or(0))
      } else {
        present
      })
    }
    "fieldType" => {
      let path = rule.get("path").and_then(|v| v.as_str()).unwrap_or("").to_string();
      let want = rule.get("type").and_then(|v| v.as_str()).unwrap_or("").to_string();
      let target = resolve_path(value, &path);
      let matches = match want.as_str() {
        "string" => target.is_string(),
        "number" => target.is_number(),
        "bool" => target.is_boolean(),
        "array" => target.is_array(),
        "object" => target.is_object(),
        "null" => target.is_null(),
        _ => false,
      };
      Ok(matches)
    }
    "arrayLength" => {
      let path = rule.get("path").and_then(|v| v.as_str()).unwrap_or("").to_string();
      let target = resolve_path(value, &path);
      let arr = target.as_array().ok_or("arrayLength: target is not an array")?;
      let min = rule.get("min").and_then(|v| v.as_u64()).unwrap_or(0);
      let max = rule.get("max").and_then(|v| v.as_u64()).unwrap_or(u64::MAX);
      Ok(arr.len() as u64 >= min && arr.len() as u64 <= max)
    }
    "inList" => {
      let list = rule.get("values").and_then(|v| v.as_array()).cloned().unwrap_or_default();
      let s = value.as_str().ok_or("inList: input is not a string")?;
      Ok(list.iter().any(|v| v.as_str() == Some(s)))
    }
    "each" => {
      let on = rule.get("on").and_then(|v| v.as_str()).unwrap_or("").to_string();
      let sub_rules = rule.get("rules").and_then(|v| v.as_array()).cloned().unwrap_or_default();
      let target = if on.is_empty() { value.clone() } else { resolve_path(value, &on) };
      let arr = target.as_array().ok_or("each: target is not an array")?;
      for item in arr {
        for sub in &sub_rules {
          if !eval_rule(sub, item)? { return Ok(false); }
        }
      }
      Ok(true)
    }
    "all" => {
      let sub = rule.get("rules").and_then(|v| v.as_array()).cloned().unwrap_or_default();
      for r in &sub { if !eval_rule(r, value)? { return Ok(false); } }
      Ok(true)
    }
    "any" => {
      let sub = rule.get("rules").and_then(|v| v.as_array()).cloned().unwrap_or_default();
      for r in &sub { if eval_rule(r, value)? { return Ok(true); } }
      Ok(false)
    }
    "not" => {
      let sub = rule.get("rule").ok_or("not: missing rule")?;
      Ok(!eval_rule(sub, value)?)
    }
    _ => Err(format!("Unknown rule type: {rtype}")),
  }
}

// Resolve a dotted path ("items[0].title") against a JSON value.
fn resolve_path(value: &serde_json::Value, path: &str) -> serde_json::Value {
  let mut cur = value.clone();
  for seg in path.split('.') {
    if seg.is_empty() { continue; }
    // Support array index syntax: items[0]
    if let Some(bracket) = seg.find('[') {
      let name = &seg[..bracket];
      let rest = &seg[bracket..];
      if !name.is_empty() { cur = cur.get(name).cloned().unwrap_or(serde_json::Value::Null); }
      for idx in rest.split('[').filter(|s| !s.is_empty()) {
        let i: usize = idx.trim_end_matches(']').parse().unwrap_or(usize::MAX);
        cur = cur.get(i).cloned().unwrap_or(serde_json::Value::Null);
      }
    } else {
      cur = cur.get(seg).cloned().unwrap_or(serde_json::Value::Null);
    }
  }
  cur
}

// Run the checker: evaluate the rule list against the envelope value, with an
// optional LLM judge as the final rule. Returns the JSON verdict envelope.
async fn run_checker(app: &AppHandle, config: &serde_json::Value, input_value: &serde_json::Value, api_key: Option<&str>) -> Result<serde_json::Value, String> {
  let rules = config.get("rules").and_then(|v| v.as_array()).cloned().unwrap_or_default();
  let mode = config.get("mode").and_then(|v| v.as_str()).unwrap_or("all").to_string();
  let mut failed: Vec<String> = Vec::new();
  let mut judged = false;

  // Deterministic rules first (LLM judge runs last).
  let mut llm_rule: Option<serde_json::Value> = None;
  for rule in &rules {
    if rule.get("type").and_then(|v| v.as_str()) == Some("llmJudge") { llm_rule = Some(rule.clone()); continue; }
    let ok = eval_rule(rule, input_value).map_err(|e| format!("Checker rule error: {e}"))?;
    let label = rule.get("label").and_then(|v| v.as_str()).map(|s| s.to_string()).unwrap_or_else(|| format!("{}", rule.get("type").and_then(|v| v.as_str()).unwrap_or("rule")));
    if !ok {
      failed.push(label);
      if mode == "all" { break; } // short-circuit: all rules must pass
    }
  }

  // LLM judge: only run if deterministic rules passed (or mode is "any" and nothing failed).
  let deterministic_passed = failed.is_empty();
  if let Some(lr) = llm_rule {
    if deterministic_passed {
      let verdict = run_llm_judge(app, &lr, input_value, api_key).await?;
      judged = true;
      if !verdict["pass"].as_bool().unwrap_or(false) {
        failed.push(verdict["reason"].as_str().unwrap_or("LLM judge rejected output").to_string());
      }
    }
  }

  let pass = failed.is_empty();
  Ok(serde_json::json!({
    "pass": pass,
    "failedRules": failed,
    "llmJudged": judged,
    "reason": if pass { "All checks passed".to_string() } else { format!("Failed: {}", failed.join(", ")) },
  }))
}

// One LLM call that returns { pass: bool, reason: string }. Retries once if the
// response isn't parseable.
async fn run_llm_judge(app: &AppHandle, rule: &serde_json::Value, input_value: &serde_json::Value, api_key: Option<&str>) -> Result<serde_json::Value, String> {
  let conn = db(app)?;
  let model = rule.get("model").and_then(|v| v.as_str()).unwrap_or("groq:llama-3.3-70b-versatile").to_string();
  let prompt = rule.get("prompt").and_then(|v| v.as_str()).unwrap_or("Is this output acceptable? Reply with only JSON {\"pass\":true/false,\"reason\":\"...\"}").to_string();
  let user_content = if input_value.is_string() {
    input_value.as_str().unwrap_or("").to_string()
  } else {
    serde_json::to_string_pretty(input_value).unwrap_or_else(|_| "".to_string())
  };
  let system = "You are a strict quality judge. Evaluate the output against the criteria and reply with ONLY a JSON object: {\"pass\": true or false, \"reason\": \"short explanation\"}. No other text.";
  let full_prompt = format!("Criteria:\n{prompt}\n\nOutput to judge:\n{user_content}");

  let mut last_err = "LLM judge returned no text".to_string();
  for attempt in 0..2 {
    let result = if let Some(m) = model.strip_prefix("ollama:") {
      let client = http::stream_client();
      let resp = client.post("http://127.0.0.1:11434/api/chat")
        .json(&serde_json::json!({"model": m, "messages": [{"role":"system","content":system},{"role":"user","content":full_prompt}], "stream": false, "format": "json"}))
        .send().await.map_err(|e| format!("LLM judge: could not reach Ollama ({e})"))?;
      if !resp.status().is_success() { return Err(format!("LLM judge: Ollama returned {}", resp.status())); }
      let json: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
      json["message"]["content"].as_str().unwrap_or("").to_string()
    } else if let Some(m) = model.strip_prefix("groq:") {
      let key = api_key.filter(|k| !k.trim().is_empty()).map(|k| k.to_string()).or(stored_api_key(&conn, &model).ok().flatten())
        .ok_or("LLM judge: Groq requires an API key.")?;
      let client = http::client();
      let mut body = serde_json::json!({"model": m, "messages": [{"role":"system","content":system},{"role":"user","content":full_prompt}], "temperature": 0.1});
      http::apply_openai_defaults(&mut body, false, http::SUMMARY_MAX_TOKENS);
      let resp = client.post("https://api.groq.com/openai/v1/chat/completions")
        .header("Authorization", format!("Bearer {key}"))
        .json(&body)
        .send().await.map_err(|e| format!("LLM judge: could not reach Groq ({e})"))?;
      if !resp.status().is_success() { return Err(format!("LLM judge: Groq returned {}", resp.status())); }
      let json: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
      json["choices"][0]["message"]["content"].as_str().unwrap_or("").to_string()
    } else if let Some(m) = model.strip_prefix("openrouter:") {
      let key = api_key.filter(|k| !k.trim().is_empty()).map(|k| k.to_string()).or(stored_api_key(&conn, &model).ok().flatten())
        .ok_or("LLM judge: OpenRouter requires an API key.")?;
      let client = http::client();
      let mut body = serde_json::json!({"model": m, "messages": [{"role":"system","content":system},{"role":"user","content":full_prompt}], "temperature": 0.1});
      http::apply_openai_defaults(&mut body, true, http::SUMMARY_MAX_TOKENS);
      let resp = client.post("https://openrouter.ai/api/v1/chat/completions")
        .header("Authorization", format!("Bearer {key}"))
        .header("HTTP-Referer", "https://local-agent-os.app")
        .header("X-Title", "Local Agent OS")
        .json(&body)
        .send().await.map_err(|e| format!("LLM judge: could not reach OpenRouter ({e})"))?;
      if !resp.status().is_success() { return Err(format!("LLM judge: OpenRouter returned {}", resp.status())); }
      let json: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
      json["choices"][0]["message"]["content"].as_str().unwrap_or("").to_string()
    } else {
      return Err(format!("LLM judge: unknown model provider for {model}"));
    };

    // Parse the verdict, tolerating markdown fences and stray text.
    let cleaned = strip_code_fences(&result);
    let trimmed = cleaned.trim();
    let parsed: Option<serde_json::Value> = if let Some(start) = trimmed.find('{') {
      if let Some(end) = trimmed.rfind('}') {
        serde_json::from_str(&trimmed[start..=end]).ok()
      } else { None }
    } else { None };
    if let Some(v) = parsed {
      return Ok(v);
    }
    last_err = format!("LLM judge: could not parse response (attempt {}): {}", attempt + 1, result.chars().take(120).collect::<String>());
  }
  // Unparseable after retries: fail closed.
  Ok(serde_json::json!({ "pass": false, "reason": last_err }))
}

fn strip_code_fences(s: &str) -> String {
  let t = s.trim();
  if t.starts_with("```") {
    let mut lines = t.lines();
    let _ = lines.next();
    lines.collect::<Vec<_>>().join("\n").trim_end_matches("```").trim().to_string()
  } else {
    t.to_string()
  }
}

// Evaluate a simple gate condition against the incoming value. Supports:
//   $pass == true | $result.pass == false | contains("error") | $len > 5
// This is a tiny safe evaluator — no eval, just patterns.
fn eval_condition(condition: &str, value: &serde_json::Value) -> bool {
  let c = condition.trim();
  // Tokenize around a comparison operator.
  let (left, right, op) = if let Some(i) = c.find("==") {
    (c[..i].trim(), c[i + 2..].trim(), "eq")
  } else if let Some(i) = c.find("!=") {
    (c[..i].trim(), c[i + 2..].trim(), "ne")
  } else if let Some(i) = c.find(">=") {
    (c[..i].trim(), c[i + 2..].trim(), "ge")
  } else if let Some(i) = c.find("<=") {
    (c[..i].trim(), c[i + 2..].trim(), "le")
  } else if let Some(i) = c.find('>') {
    (c[..i].trim(), c[i + 1..].trim(), "gt")
  } else if let Some(i) = c.find('<') {
    (c[..i].trim(), c[i + 1..].trim(), "lt")
  } else {
    // No operator: treat as truthy expression (contains("...") or $field).
    return eval_truthy(c, value);
  };

  let lv = resolve_expr(left, value);
  let rv = resolve_expr(right, value);
  match op {
    "eq" => json_eq(&lv, &rv),
    "ne" => !json_eq(&lv, &rv),
    "gt" => cmp_f64(&lv, &rv).map(|o| o.is_gt()).unwrap_or(false),
    "lt" => cmp_f64(&lv, &rv).map(|o| o.is_lt()).unwrap_or(false),
    "ge" => cmp_f64(&lv, &rv).map(|o| o.is_ge()).unwrap_or(false),
    "le" => cmp_f64(&lv, &rv).map(|o| o.is_le()).unwrap_or(false),
    _ => false,
  }
}

// Resolve a "$path", "contains(...)", "$len", literal, or quoted string.
fn resolve_expr(expr: &str, value: &serde_json::Value) -> serde_json::Value {
  let e = expr.trim();
  if e.is_empty() { return serde_json::Value::Null; }
  // contains("text") / not contains
  if let Some(rest) = e.strip_prefix("contains(") {
    if let Some(arg) = rest.strip_suffix(')') {
      let needle = arg.trim().trim_matches('"').trim_matches('\'');
      let hay = value.as_str().unwrap_or("");
      return serde_json::json!(hay.contains(needle));
    }
  }
  if let Some(rest) = e.strip_prefix("not contains(") {
    if let Some(arg) = rest.strip_suffix(')') {
      let needle = arg.trim().trim_matches('"').trim_matches('\'');
      let hay = value.as_str().unwrap_or("");
      return serde_json::json!(!hay.contains(needle));
    }
  }
  // $len — string/array length
  if e == "$len" {
    let len = match value {
      serde_json::Value::String(s) => s.chars().count(),
      serde_json::Value::Array(a) => a.len(),
      _ => 0,
    };
    return serde_json::json!(len as i64);
  }
  // $pass or $result.pass — resolved from the envelope
  if let Some(path) = e.strip_prefix('$') {
    let p = path.trim_start_matches("result.");
    return resolve_path(value, p);
  }
  // Literal: number, true/false, quoted string
  if let Ok(n) = e.parse::<f64>() { return serde_json::json!(n); }
  if e == "true" { return serde_json::json!(true); }
  if e == "false" { return serde_json::json!(false); }
  let q = e.trim_matches('"').trim_matches('\'');
  serde_json::json!(q)
}

fn eval_truthy(c: &str, value: &serde_json::Value) -> bool {
  let e = c.trim();
  if e.is_empty() { return false; }
  // contains("...")
  if e.starts_with("contains(") && e.ends_with(')') {
    let arg = e["contains(".len()..e.len() - 1].trim().trim_matches('"').trim_matches('\'');
    return value.as_str().map(|s| s.contains(arg)).unwrap_or(false);
  }
  // bare $path -> truthiness
  if let Some(path) = e.strip_prefix('$') {
    let p = path.trim_start_matches("result.");
    let v = resolve_path(value, p);
    return match v {
      serde_json::Value::Bool(b) => b,
      serde_json::Value::String(s) => !s.is_empty(),
      serde_json::Value::Number(n) => n.as_f64().unwrap_or(0.0) != 0.0,
      serde_json::Value::Array(a) => !a.is_empty(),
      serde_json::Value::Object(o) => !o.is_empty(),
      serde_json::Value::Null => false,
    };
  }
  false
}

fn json_eq(a: &serde_json::Value, b: &serde_json::Value) -> bool {
  match (a, b) {
    (serde_json::Value::String(x), serde_json::Value::String(y)) => x == y,
    (serde_json::Value::Bool(x), serde_json::Value::Bool(y)) => x == y,
    (serde_json::Value::Number(x), serde_json::Value::Number(y)) => x.as_f64() == y.as_f64(),
    (serde_json::Value::String(x), serde_json::Value::Bool(y)) => x == &y.to_string(),
    (serde_json::Value::Bool(y), serde_json::Value::String(x)) => x == &y.to_string(),
    (serde_json::Value::String(x), serde_json::Value::Number(y)) => x.parse::<f64>().ok() == y.as_f64(),
    (serde_json::Value::Number(y), serde_json::Value::String(x)) => x.parse::<f64>().ok() == y.as_f64(),
    _ => a == b,
  }
}

fn cmp_f64(a: &serde_json::Value, b: &serde_json::Value) -> Option<std::cmp::Ordering> {
  let an = a.as_f64().or_else(|| a.as_str().and_then(|s| s.parse().ok()))?;
  let bn = b.as_f64().or_else(|| b.as_str().and_then(|s| s.parse().ok()))?;
  an.partial_cmp(&bn)
}

// Executes a workflow linearly: orders nodes by edges (BFS from start nodes), runs each
// agent/subagent/checker node with the previous node's output as input. Loop/checker/gate
// config is stored but phase 1 passes input through.
#[tauri::command]
pub async fn execute_workflow(app: AppHandle, workflow: WorkflowRecord, input: String, api_key: Option<String>) -> Result<WorkflowExecution, String> {
  let conn = db(&app)?;
  // Persist every execution (with its per-node steps) so results outlive the app
  // session instead of being returned to the UI and discarded.
  let run_id = format!("wfr-{}", chrono::Utc::now().timestamp_millis());
  conn.execute(
    "INSERT INTO workflow_runs (id, workflow_id, workflow_name, started_at, status, input, steps) VALUES (?1,?2,?3,?4,'running',?5,'[]')",
    params![run_id, workflow.id, workflow.name, chrono::Utc::now().to_rfc3339(), input],
  ).map_err(|e| e.to_string())?;
  // The node loop owns its own connection, so this command holds no rusqlite
  // borrow across the await (a Tauri command future must be Send).
  match run_workflow_nodes(&app, &workflow, input, api_key).await {
    Ok((steps, final_output, prompt_tokens, completion_tokens)) => {
      conn.execute(
        "UPDATE workflow_runs SET status='completed', ended_at=?1, final_output=?2, steps=?3, prompt_tokens=?4, completion_tokens=?5 WHERE id=?6",
        params![chrono::Utc::now().to_rfc3339(), final_output, serde_json::to_string(&steps).unwrap_or_else(|_| "[]".to_string()), prompt_tokens as i64, completion_tokens as i64, run_id],
      ).map_err(|e| e.to_string())?;
      Ok(WorkflowExecution { steps, final_output, total_prompt_tokens: prompt_tokens, total_completion_tokens: completion_tokens })
    }
    Err(e) => {
      let _ = conn.execute(
        "UPDATE workflow_runs SET status='failed', ended_at=?1, final_output=?2 WHERE id=?3",
        params![chrono::Utc::now().to_rfc3339(), e, run_id],
      );
      Err(e)
    }
  }
}

// Runs the workflow's nodes. Split from the command so no connection borrow is
// held across an await in a Send future.
async fn run_workflow_nodes(app: &AppHandle, workflow: &WorkflowRecord, input: String, api_key: Option<String>) -> Result<(Vec<WorkflowStep>, String, u64, u64), String> {
  let conn = db(app)?;
  let nodes: Vec<serde_json::Value> = workflow.nodes.as_array().cloned().unwrap_or_default();
  let edges: Vec<serde_json::Value> = workflow.edges.as_array().cloned().unwrap_or_default();

  // Build node map + adjacency from edges.
  let mut node_map: std::collections::HashMap<String, serde_json::Value> = std::collections::HashMap::new();
  let mut start_ids: Vec<String> = Vec::new();
  let mut incoming: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
  let mut outgoing: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();
  for n in &nodes {
    let id = n.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
    if id.is_empty() { continue; }
    node_map.insert(id.clone(), n.clone());
    incoming.entry(id.clone()).or_insert(0);
  }
  for e in &edges {
    let from = e.get("from").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let to = e.get("to").and_then(|v| v.as_str()).unwrap_or("").to_string();
    if from.is_empty() || to.is_empty() { continue; }
    if !node_map.contains_key(&from) || !node_map.contains_key(&to) { continue; }
    *incoming.entry(to.clone()).or_insert(0) += 1;
    outgoing.entry(from.clone()).or_default().push(to);
  }
  for (id, deg) in &incoming {
    if *deg == 0 { start_ids.push(id.clone()); }
  }
  if start_ids.is_empty() && !nodes.is_empty() {
    start_ids = nodes.iter().filter_map(|n| n.get("id").and_then(|v| v.as_str()).map(|s| s.to_string())).collect();
  }

  // BFS topological order.
  let mut order: Vec<String> = Vec::new();
  let mut queue: Vec<String> = start_ids;
  while let Some(id) = queue.pop() {
    order.push(id.clone());
    if let Some(children) = outgoing.get(&id) {
      for c in children {
        let deg = incoming.entry(c.clone()).or_insert(0);
        *deg = deg.saturating_sub(1);
        if *deg == 0 { queue.push(c.clone()); }
      }
    }
  }

  let mut steps: Vec<WorkflowStep> = Vec::new();
  let mut current_input = input;
  let mut total_prompt = 0u64;
  let mut total_completion = 0u64;

  for node_id in order {
    let Some(node) = node_map.get(&node_id) else { continue };
    let ntype = node.get("type").and_then(|v| v.as_str()).unwrap_or("agent").to_string();
    let label = node.get("label").and_then(|v| v.as_str()).unwrap_or(&node_id).to_string();
    let config = node.get("config").cloned().unwrap_or(serde_json::json!({}));
    let mut events = Vec::new();
    let output = match ntype.as_str() {
      "agent" | "subagent" => {
        let agent_id = node.get("agentId").and_then(|v| v.as_str()).unwrap_or("").to_string();
        if agent_id.is_empty() {
          current_input.clone()
        } else {
          let agent_opt = {
            let mut stmt = conn.prepare("SELECT id, name, objective, model, tool_ids, integrations, home_path, permissions, skill_ids FROM agents WHERE id=?1").map_err(|e| e.to_string())?;
            let mut rows = stmt.query_map(params![agent_id], |row| {
              let tool_ids: String = row.get(4)?;
              let integrations: String = row.get(5)?;
              let permissions: String = row.get(7)?;
              let skill_ids: String = row.get(8)?;
              Ok(AgentRequest {
                id: row.get(0)?, name: row.get(1)?, objective: row.get(2)?, model: row.get(3)?,
                tool_ids: parse_json_vec(&tool_ids), integrations: parse_json_vec(&integrations),
                home_path: row.get(6)?, permissions: parse_json_vec(&permissions),
                skill_ids: parse_json_vec(&skill_ids),
              })
            }).map_err(|e| e.to_string())?;
            rows.next().transpose().map_err(|e| e.to_string())?
          };
          let (out, pt, ct) = match agent_opt {
            Some(agent) => run_agent_once(app, &agent, &current_input, api_key.as_deref(), &mut events).await?,
            None => (current_input.clone(), 0, 0),
          };
          total_prompt += pt; total_completion += ct;
          out
        }
      }
      "checker" => {
        // Try to parse the incoming text as JSON; if it is, rules run against
        // the parsed object, otherwise against the raw string.
        let value: serde_json::Value = serde_json::from_str(&current_input).unwrap_or(serde_json::Value::String(current_input.clone()));
        let verdict = run_checker(app, &config, &value, api_key.as_deref()).await?;
        // The verdict envelope becomes the input for downstream nodes.
        serde_json::to_string(&verdict).unwrap_or_else(|_| r#"{"pass":false,"reason":"serialize error"}"#.to_string())
      }
      "gate" => {
        // Evaluate the condition against $result (the previous node's output
        // parsed as JSON, or a plain string fallback). True = continue down
        // this edge; false = skip (output is passed through but flagged).
        let condition = config.get("condition").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let parsed: serde_json::Value = serde_json::from_str(&current_input).unwrap_or(serde_json::Value::String(current_input.clone()));
        let decision = if condition.is_empty() { true } else { eval_condition(&condition, &parsed) };
        serde_json::to_string(&serde_json::json!({ "gate": condition, "decision": decision, "input": if parsed.is_string() { parsed.as_str().unwrap_or("").to_string() } else { serde_json::to_string(&parsed).unwrap_or_default() } }))
          .unwrap_or_else(|_| current_input.clone())
      }
      "integration" => {
        // Run an integration action with the accumulated workflow data.
        let integration_id = config.get("integrationId").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let action = config.get("action").and_then(|v| v.as_str()).unwrap_or("").to_string();
        if integration_id.is_empty() || action.is_empty() {
          serde_json::json!({ "error": "integration node requires integrationId and action" }).to_string()
        } else {
          let creds = integration_secret(&conn, &integration_id).unwrap_or_else(|_| serde_json::json!({}));
          let tool = IntegrationTool { action: action.clone(), credentials: creds };
          // Build args from the node config, injecting the accumulated output
          // as the primary payload (title/content/fields) where applicable.
          let mut args = config.get("args").cloned().unwrap_or(serde_json::json!({}));
          if let Some(obj) = args.as_object_mut() {
            let out_str = &current_input;
            // Default injection: map workflow output into the most common
            // payload fields if the user hasn't overridden them.
            if !obj.contains_key("content") && (action.contains("docs_create") || action.contains("notion_create_page")) {
              obj.insert("content".into(), serde_json::json!(out_str));
            }
            if !obj.contains_key("title") && action.contains("notion_create_page") {
              obj.insert("title".into(), serde_json::json!(out_str.chars().take(120).collect::<String>()));
            }
          }
          match tool.run(&args).await {
            Ok(res) => res,
            Err(e) => serde_json::json!({ "error": e }).to_string(),
          }
        }
      }
      _ => current_input.clone(), // trigger/loop: pass through for now
    };
    steps.push(WorkflowStep { node_id: node_id.clone(), node_label: label, output: output.clone(), prompt_tokens: 0, completion_tokens: 0 });
    current_input = output;
  }

  let final_output = current_input;
  Ok((steps, final_output, total_prompt, total_completion))
}

fn workflow_run_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<WorkflowRunRecord> {
  let steps: String = row.get(8)?;
  Ok(WorkflowRunRecord {
    id: row.get(0)?, workflow_id: row.get(1)?, workflow_name: row.get(2)?,
    started_at: row.get(3)?, ended_at: row.get(4)?, status: row.get(5)?,
    input: row.get(6)?, final_output: row.get(7)?,
    steps: serde_json::from_str(&steps).unwrap_or_else(|_| serde_json::json!([])),
    prompt_tokens: row.get::<_, i64>(9)? as u64,
    completion_tokens: row.get::<_, i64>(10)? as u64,
  })
}

// Stored workflow runs, newest first, optionally filtered to a single workflow.
#[tauri::command]
pub fn list_workflow_runs(app: AppHandle, workflow_id: Option<String>) -> Result<Vec<WorkflowRunRecord>, String> {
  let conn = db(&app)?;
  const COLS: &str = "SELECT id, workflow_id, workflow_name, started_at, ended_at, status, input, final_output, steps, prompt_tokens, completion_tokens FROM workflow_runs";
  let mut out = Vec::new();
  match workflow_id.filter(|s| !s.is_empty()) {
    Some(wf) => {
      let mut stmt = conn.prepare(&format!("{COLS} WHERE workflow_id=?1 ORDER BY started_at DESC LIMIT 100")).map_err(|e| e.to_string())?;
      let rows = stmt.query_map(params![wf], workflow_run_row).map_err(|e| e.to_string())?;
      for r in rows { out.push(r.map_err(|e| e.to_string())?); }
    }
    None => {
      let mut stmt = conn.prepare(&format!("{COLS} ORDER BY started_at DESC LIMIT 100")).map_err(|e| e.to_string())?;
      let rows = stmt.query_map([], workflow_run_row).map_err(|e| e.to_string())?;
      for r in rows { out.push(r.map_err(|e| e.to_string())?); }
    }
  }
  Ok(out)
}
