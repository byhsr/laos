// Single source of truth for provider dispatch: which endpoint a stored model id
// maps to, how its API key resolves, and how the per-model reasoning setting is
// translated into each provider's request parameter.
//
// This used to be re-implemented in five places (chat, agents, manager, memory,
// workflows), which is how the Manager path drifted into passing `!is_groq` as
// its "is OpenRouter" flag. Everything model-facing goes through here now.

use rusqlite::{params, Connection};

use crate::models::default_reasoning;

pub(crate) const OLLAMA_DEFAULT_HOST: &str = "http://127.0.0.1:11434";
const GROQ_URL: &str = "https://api.groq.com/openai/v1/chat/completions";
const OPENROUTER_URL: &str = "https://openrouter.ai/api/v1/chat/completions";

// Ollama unloads an idle model after a few minutes, so without this every turn
// after a pause pays a multi-second reload.
pub(crate) const OLLAMA_KEEP_ALIVE: &str = "30m";

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind { Ollama, Groq, OpenRouter }

// Maps to the stored `model_configs.reasoning` value.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Reasoning { Auto, Off, Low, Medium, High }

impl Reasoning {
  pub(crate) fn parse(raw: &str) -> Self {
    match raw.trim().to_ascii_lowercase().as_str() {
      "off" | "none" | "false" => Reasoning::Off,
      "low" => Reasoning::Low,
      "medium" => Reasoning::Medium,
      "high" => Reasoning::High,
      // Anything unrecognised (including "auto" and legacy rows) means "send nothing".
      _ => Reasoning::Auto,
    }
  }

  fn level(self) -> Option<&'static str> {
    match self {
      Reasoning::Low => Some("low"),
      Reasoning::Medium => Some("medium"),
      Reasoning::High => Some("high"),
      _ => None,
    }
  }

  /// Whether the model has been asked to actually reason.
  pub(crate) fn is_enabled(self) -> bool {
    matches!(self, Reasoning::Low | Reasoning::Medium | Reasoning::High)
  }
}

pub(crate) struct Resolved {
  pub(crate) kind: Kind,
  /// Provider root: the stored host for Ollama, the completions URL otherwise.
  pub(crate) base: String,
  /// The bare model name the provider expects; our prefix is stripped.
  pub(crate) model: String,
  pub(crate) key: Option<String>,
  pub(crate) reasoning: Reasoning,
}

impl Resolved {
  pub(crate) fn chat_url(&self) -> String {
    match self.kind {
      Kind::Ollama => format!("{}/api/chat", self.base),
      _ => self.base.clone(),
    }
  }

  /// Ollama's single-prompt endpoint. Cloud providers have no equivalent.
  pub(crate) fn generate_url(&self) -> String {
    format!("{}/api/generate", self.base)
  }
}

/// Resolves a stored `"<provider>:<model>"` id into everything a request needs.
///
/// `override_key` is a caller-supplied API key (the browser preview passes one);
/// it wins over the stored key when present.
pub(crate) fn resolve(conn: &Connection, model_id: &str, override_key: Option<&str>) -> Result<Resolved, String> {
  let (host, stored_key, reasoning) = model_settings(conn, model_id)?;
  let key = override_key.filter(|k| !k.trim().is_empty()).map(|k| k.to_string()).or(stored_key);

  if let Some(model) = model_id.strip_prefix("ollama:") {
    // The host field is editable in the UI, so honour it rather than assuming
    // the default port — people run Ollama on other hosts and ports. Ollama
    // takes no auth, so any supplied key is ignored.
    let base = host.filter(|h| !h.trim().is_empty()).unwrap_or_else(|| OLLAMA_DEFAULT_HOST.to_string());
    return Ok(Resolved { kind: Kind::Ollama, base: base.trim_end_matches('/').to_string(), model: model.to_string(), key: None, reasoning });
  }
  if let Some(model) = model_id.strip_prefix("groq:") {
    let key = key.ok_or("Groq requires an API key. Add one in Settings → Models.")?;
    return Ok(Resolved { kind: Kind::Groq, base: GROQ_URL.to_string(), model: model.to_string(), key: Some(key), reasoning });
  }
  if let Some(model) = model_id.strip_prefix("openrouter:") {
    let key = key.ok_or("OpenRouter requires an API key. Add one in Settings → Models.")?;
    return Ok(Resolved { kind: Kind::OpenRouter, base: OPENROUTER_URL.to_string(), model: model.to_string(), key: Some(key), reasoning });
  }
  Err(format!("Unknown model provider for \"{model_id}\"."))
}

fn model_settings(conn: &Connection, model_id: &str) -> Result<(Option<String>, Option<String>, Reasoning), String> {
  let mut stmt = conn
    .prepare("SELECT host, api_key, reasoning FROM model_configs WHERE id=?1")
    .map_err(|e| e.to_string())?;
  let mut rows = stmt
    .query_map(params![model_id], |row| {
      Ok((
        row.get::<_, Option<String>>(0)?,
        row.get::<_, Option<String>>(1)?,
        row.get::<_, Option<String>>(2)?.unwrap_or_else(default_reasoning),
      ))
    })
    .map_err(|e| e.to_string())?;
  match rows.next() {
    Some(Ok((host, key, reasoning))) => Ok((host, key.filter(|k| !k.trim().is_empty()), Reasoning::parse(&reasoning))),
    Some(Err(e)) => Err(e.to_string()),
    // A model missing from the table (for example a seed the user never saved)
    // still resolves, so the caller reports the real problem — a missing key.
    None => Ok((None, None, Reasoning::Auto)),
  }
}

/// Applies the model's reasoning setting to a request body and returns the output
/// budget to use.
///
/// The budget matters: reasoning tokens are billed against `max_tokens`, so a
/// model that spends its whole allowance thinking returns empty content with
/// `finish_reason: "length"`. Enabling reasoning therefore raises the budget.
pub(crate) fn apply_reasoning(body: &mut serde_json::Value, resolved: &Resolved, base_max_tokens: u64) -> u64 {
  let reasoning = resolved.reasoning;
  // "auto" deliberately writes nothing, so models with no reasoning support are
  // never sent a parameter they would reject.
  if reasoning == Reasoning::Auto {
    return base_max_tokens;
  }

  match resolved.kind {
    Kind::Ollama => {
      // Accepts booleans, or levels for models like GPT-OSS that ignore booleans.
      body["think"] = match reasoning.level() {
        Some(level) => serde_json::json!(level),
        None => serde_json::json!(false),
      };
    }
    Kind::OpenRouter => {
      body["reasoning"] = match reasoning.level() {
        Some(level) => serde_json::json!({ "effort": level, "exclude": true }),
        None => serde_json::json!({ "enabled": false }),
      };
    }
    Kind::Groq => {
      body["reasoning_effort"] = match reasoning.level() {
        Some(level) => serde_json::json!(level),
        None => serde_json::json!("none"),
      };
    }
  }

  if reasoning.is_enabled() { crate::http::REASONING_MAX_TOKENS } else { base_max_tokens }
}

/// Ollama-only tuning, dropped into the body before it is sent.
///
/// `num_predict` is unbounded by default while every cloud path caps output at
/// `max_tokens`, so a chatty local model can otherwise generate thousands of
/// tokens nobody watches.
pub(crate) fn apply_ollama_options(body: &mut serde_json::Value, max_tokens: u64) {
  body["keep_alive"] = serde_json::json!(OLLAMA_KEEP_ALIVE);
  body["options"] = serde_json::json!({ "num_predict": max_tokens });
}
