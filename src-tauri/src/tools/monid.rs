// The `monid` tool — Monid ("OpenRouter for agent tools") through its CLI.
//
// Monid exposes hundreds of endpoints (web search and scraping, people/company
// enrichment, social, market data, media) behind one key. Its CLI is the
// documented interface for agents: `discover` ranks the catalog for a need,
// `inspect` returns an endpoint's input schema, and `run` executes it. This tool
// wraps those three verbs so an agent reaches the whole catalog dynamically —
// no fixed, hand-imported tool set.

use async_trait::async_trait;

use super::{clip, str_arg, AgentTool};

pub(crate) struct MonidTool;

#[async_trait]
impl AgentTool for MonidTool {
  fn name(&self) -> String { "monid".into() }

  fn description(&self) -> String {
    "Reach hundreds of external data tools (web search/scraping, enrichment, social, market data, media) \
through Monid's catalog with one key. Workflow: 'discover' to find endpoints for a need, 'inspect' to read an \
endpoint's input schema, then 'run' to execute it. Params: command ('discover' | 'inspect' | 'run'), query \
(string; discover), provider (string; inspect/run), endpoint (string; inspect/run), input (object or JSON \
string; run — the endpoint's input, from inspect). Needs the Monid CLI (`npm i -g @monid-ai/cli`) with a saved \
key (`monid keys add`); runs may cost money and take 1-120s.".into()
  }

  fn params_schema(&self) -> serde_json::Value {
    serde_json::json!({ "type": "object", "properties": {
      "command": { "type": "string" },
      "query": { "type": "string" },
      "provider": { "type": "string" },
      "endpoint": { "type": "string" },
      "input": { "type": ["object", "string"] }
    }, "required": ["command"] })
  }

  async fn run(&self, args: &serde_json::Value) -> Result<String, String> {
    let command = str_arg(args, "command").unwrap_or_default();
    let line = match command.trim() {
      "discover" => {
        let q = str_arg(args, "query").unwrap_or_default();
        if q.trim().is_empty() { return Err("monid discover needs a 'query'.".into()); }
        format!("monid discover -q {} -j", quote(&q)?)
      }
      "inspect" => {
        let p = str_arg(args, "provider").unwrap_or_default();
        let e = str_arg(args, "endpoint").unwrap_or_default();
        if p.trim().is_empty() || e.trim().is_empty() { return Err("monid inspect needs 'provider' and 'endpoint'.".into()); }
        format!("monid inspect -p {} -e {} -j", bare(&p)?, bare(&e)?)
      }
      "run" => {
        let p = str_arg(args, "provider").unwrap_or_default();
        let e = str_arg(args, "endpoint").unwrap_or_default();
        if p.trim().is_empty() || e.trim().is_empty() { return Err("monid run needs 'provider' and 'endpoint'.".into()); }
        // The input can be large JSON, so hand it to the CLI through a temp file
        // rather than fighting shell quoting.
        let input = match args.get("input") {
          Some(serde_json::Value::String(s)) => s.clone(),
          Some(v) => serde_json::to_string(v).unwrap_or_else(|_| "{}".into()),
          None => "{}".into(),
        };
        let path = std::env::temp_dir().join(format!("monid-input-{}.json", chrono::Utc::now().timestamp_millis()));
        std::fs::write(&path, input).map_err(|e| format!("could not stage monid input: {e}"))?;
        format!("monid run -p {} -e {} -f {} -w 60 -j", bare(&p)?, bare(&e)?, quote(&path.to_string_lossy())?)
      }
      other => return Err(format!("Unknown monid command \"{other}\". Use discover | inspect | run.")),
    };
    tokio::task::spawn_blocking(move || crate::tools::run_shell_in(&line, "")).await.map_err(|e| e.to_string())?
  }
}

/// A shell-safe bare token (used for provider / endpoint ids).
fn bare(s: &str) -> Result<String, String> {
  let t = s.trim();
  if t.is_empty() || t.chars().any(|c| "\"&|<>^%".contains(c)) {
    return Err(format!("\"{s}\" contains characters a shell would misread."));
  }
  Ok(t.to_string())
}

/// A double-quoted argument (used for a query or a file path).
fn quote(s: &str) -> Result<String, String> {
  if s.contains('"') || s.contains('\n') {
    return Err("drop double quotes and newlines from this value.".into());
  }
  Ok(format!("\"{}\"", s.trim()))
}
