// Single source of truth for provider dispatch.
//
// A provider is data, not a match arm: a `ProviderDef` describes its endpoint
// root, wire dialect, auth, capabilities and reasoning style. Built-ins are
// defined below; custom ones live in the `providers` table (Settings →
// Providers). Everything model-facing — chat, agents, manager, memory, media,
// classification, embeddings, transcription, speech — goes through `resolve`,
// so adding a provider no longer means touching every call path.

use rusqlite::{params, Connection};
use serde_json::{json, Value};

use crate::models::{default_reasoning, ProviderRecord};

pub(crate) const OLLAMA_DEFAULT_HOST: &str = "http://127.0.0.1:11434";

// API roots (the part before `/chat/completions`), so endpoints are composed
// rather than stripped back out of a full URL.
const GROQ_BASE: &str = "https://api.groq.com/openai/v1";
const OPENROUTER_BASE: &str = "https://openrouter.ai/api/v1";
const OPENAI_BASE: &str = "https://api.openai.com/v1";
const GOOGLE_BASE: &str = "https://generativelanguage.googleapis.com/v1beta/openai";
const XAI_BASE: &str = "https://api.x.ai/v1";
const ANTHROPIC_BASE: &str = "https://api.anthropic.com/v1";

// Ollama unloads an idle model after a few minutes, so without this every turn
// after a pause pays a multi-second reload.
pub(crate) const OLLAMA_KEEP_ALIVE: &str = "30m";

/// How a provider speaks on the wire. Everything except Ollama and Anthropic is
/// OpenAI-compatible.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Dialect { Ollama, OpenAiCompat, Anthropic }

/// Which request parameter carries the reasoning setting.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReasoningStyle { None, OpenAiEffort, OpenRouterEffort, OllamaThink, AnthropicThinking }

/// How the API key travels on a request.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Auth { Bearer, XApiKey, None }

/// What a provider can do. Kept explicit (not inferred) so a role that needs a
/// capability can be gated before the request is made.
#[derive(Clone, Copy, Default)]
pub(crate) struct Caps { pub chat: bool, pub vision: bool, pub embeddings: bool, pub transcription: bool, pub speech: bool }

/// The adapter. `base` is the API root; URLs are composed from it.
#[derive(Clone)]
pub(crate) struct ProviderDef {
  pub id: String,
  pub label: String,
  pub dialect: Dialect,
  pub base: String,
  pub auth: Auth,
  /// Header name used by `Auth::XApiKey` (default `x-api-key`).
  pub auth_header: String,
  pub extra_headers: Vec<(String, String)>,
  pub requires_key: bool,
  pub reasoning: ReasoningStyle,
  pub provider_routing: bool,
  pub caps: Caps,
  pub builtin: bool,
}

/// The built-in providers. Order is the order the UI lists them.
pub(crate) fn builtins() -> Vec<ProviderDef> {
  let ext = |name: &str| name.to_string();
  vec![
    ProviderDef {
      id: "ollama".into(), label: "Ollama".into(), dialect: Dialect::Ollama,
      base: OLLAMA_DEFAULT_HOST.into(), auth: Auth::None, auth_header: String::new(),
      extra_headers: vec![], requires_key: false, reasoning: ReasoningStyle::OllamaThink,
      provider_routing: false, builtin: true,
      caps: Caps { chat: true, vision: true, embeddings: true, transcription: false, speech: false },
    },
    ProviderDef {
      id: "openai".into(), label: "OpenAI".into(), dialect: Dialect::OpenAiCompat,
      base: OPENAI_BASE.into(), auth: Auth::Bearer, auth_header: String::new(),
      extra_headers: vec![], requires_key: true, reasoning: ReasoningStyle::OpenAiEffort,
      provider_routing: false, builtin: true,
      caps: Caps { chat: true, vision: true, embeddings: true, transcription: true, speech: true },
    },
    ProviderDef {
      id: "anthropic".into(), label: "Anthropic".into(), dialect: Dialect::Anthropic,
      base: ANTHROPIC_BASE.into(), auth: Auth::XApiKey, auth_header: ext("x-api-key"),
      extra_headers: vec![], requires_key: true, reasoning: ReasoningStyle::AnthropicThinking,
      provider_routing: false, builtin: true,
      caps: Caps { chat: true, vision: true, embeddings: false, transcription: false, speech: false },
    },
    ProviderDef {
      id: "google".into(), label: "Google (Gemini)".into(), dialect: Dialect::OpenAiCompat,
      base: GOOGLE_BASE.into(), auth: Auth::Bearer, auth_header: String::new(),
      extra_headers: vec![], requires_key: true, reasoning: ReasoningStyle::OpenAiEffort,
      provider_routing: false, builtin: true,
      caps: Caps { chat: true, vision: true, embeddings: true, transcription: false, speech: false },
    },
    ProviderDef {
      id: "groq".into(), label: "Groq".into(), dialect: Dialect::OpenAiCompat,
      base: GROQ_BASE.into(), auth: Auth::Bearer, auth_header: String::new(),
      extra_headers: vec![], requires_key: true, reasoning: ReasoningStyle::OpenAiEffort,
      provider_routing: false, builtin: true,
      // Groq serves whisper transcription but no speech synthesis.
      caps: Caps { chat: true, vision: true, embeddings: false, transcription: true, speech: false },
    },
    ProviderDef {
      id: "openrouter".into(), label: "OpenRouter".into(), dialect: Dialect::OpenAiCompat,
      base: OPENROUTER_BASE.into(), auth: Auth::Bearer, auth_header: String::new(),
      extra_headers: vec![
        ("HTTP-Referer".into(), "https://local-agent-os.app".into()),
        ("X-Title".into(), "Local Agent OS".into()),
      ],
      requires_key: true, reasoning: ReasoningStyle::OpenRouterEffort,
      provider_routing: true, builtin: true,
      // OpenRouter now exposes OpenAI-compatible transcription and speech.
      caps: Caps { chat: true, vision: true, embeddings: false, transcription: true, speech: true },
    },
    ProviderDef {
      id: "xai".into(), label: "xAI (Grok)".into(), dialect: Dialect::OpenAiCompat,
      base: XAI_BASE.into(), auth: Auth::Bearer, auth_header: String::new(),
      extra_headers: vec![], requires_key: true, reasoning: ReasoningStyle::OpenAiEffort,
      provider_routing: false, builtin: true,
      caps: Caps { chat: true, vision: true, embeddings: true, transcription: false, speech: false },
    },
  ]
}

/// The provider id in a stored `"<provider>:<model>"` id.
pub(crate) fn provider_id_of(model_id: &str) -> &str {
  model_id.split(':').next().unwrap_or("")
}

/// The bare model name the provider expects (our prefix stripped).
fn model_of(model_id: &str) -> String {
  model_id.split_once(':').map(|(_, m)| m.to_string()).unwrap_or_default()
}

/// A built-in or custom provider by id.
pub(crate) fn find_provider(conn: &Connection, id: &str) -> Option<ProviderDef> {
  if let Some(def) = builtins().into_iter().find(|d| d.id == id) {
    return Some(def);
  }
  load_custom(conn, id)
}

fn load_custom(conn: &Connection, id: &str) -> Option<ProviderDef> {
  conn.query_row(
    "SELECT label, base_url, auth, auth_header, extra_headers, requires_key, reasoning, provider_routing, caps FROM providers WHERE id=?1",
    params![id],
    |row| Ok((
      row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?, row.get::<_, String>(3)?,
      row.get::<_, String>(4)?, row.get::<_, i64>(5)?, row.get::<_, String>(6)?, row.get::<_, i64>(7)?, row.get::<_, String>(8)?,
    )),
  ).ok().map(|(label, base, auth, auth_header, extra, requires_key, reasoning, routing, caps)| {
    ProviderDef {
      id: id.to_string(),
      label: if label.trim().is_empty() { id.to_string() } else { label },
      // Custom providers are OpenAI-compatible only.
      dialect: Dialect::OpenAiCompat,
      base: base.trim().trim_end_matches('/').to_string(),
      auth: parse_auth(&auth),
      auth_header,
      extra_headers: parse_headers(&extra),
      requires_key: requires_key != 0,
      reasoning: parse_reasoning_style(&reasoning),
      provider_routing: routing != 0,
      caps: parse_caps(&caps),
      builtin: false,
    }
  })
}

// ---------------------------------------------------------------------------
// Encoding (DB JSON <-> adapter) and the UI record
// ---------------------------------------------------------------------------

fn parse_auth(s: &str) -> Auth {
  match s.trim().to_ascii_lowercase().as_str() {
    "none" => Auth::None,
    "x-api-key" | "xapikey" => Auth::XApiKey,
    _ => Auth::Bearer,
  }
}

pub(crate) fn auth_str(a: Auth) -> &'static str {
  match a { Auth::None => "none", Auth::XApiKey => "x-api-key", Auth::Bearer => "bearer" }
}

fn parse_reasoning_style(s: &str) -> ReasoningStyle {
  match s.trim().to_ascii_lowercase().as_str() {
    "none" => ReasoningStyle::None,
    "openrouter" => ReasoningStyle::OpenRouterEffort,
    _ => ReasoningStyle::OpenAiEffort,
  }
}

pub(crate) fn reasoning_style_str(s: ReasoningStyle) -> &'static str {
  match s {
    ReasoningStyle::None => "none",
    ReasoningStyle::OpenAiEffort => "openai",
    ReasoningStyle::OpenRouterEffort => "openrouter",
    ReasoningStyle::OllamaThink => "ollama",
    ReasoningStyle::AnthropicThinking => "anthropic",
  }
}

fn parse_caps(raw: &str) -> Caps {
  let v: Value = serde_json::from_str(raw).unwrap_or(json!({}));
  Caps {
    chat: v["chat"].as_bool().unwrap_or(true),
    vision: v["vision"].as_bool().unwrap_or(false),
    embeddings: v["embeddings"].as_bool().unwrap_or(false),
    transcription: v["transcription"].as_bool().unwrap_or(false),
    speech: v["speech"].as_bool().unwrap_or(false),
  }
}

pub(crate) fn caps_json(c: Caps) -> Value {
  json!({ "chat": c.chat, "vision": c.vision, "embeddings": c.embeddings, "transcription": c.transcription, "speech": c.speech })
}

fn parse_headers(raw: &str) -> Vec<(String, String)> {
  let v: Value = serde_json::from_str(raw).unwrap_or(json!({}));
  v.as_object().map(|o| o.iter().filter_map(|(k, val)| val.as_str().map(|s| (k.clone(), s.to_string()))).collect()).unwrap_or_default()
}

pub(crate) fn headers_json(headers: &[(String, String)]) -> Value {
  let mut map = serde_json::Map::new();
  for (k, v) in headers { map.insert(k.clone(), Value::String(v.clone())); }
  Value::Object(map)
}

/// The record the UI reads. Mirrors the shape of `ProviderRecord` in models.rs.
pub(crate) fn def_to_record(def: &ProviderDef, has_key: bool) -> ProviderRecord {
  ProviderRecord {
    id: def.id.clone(), label: def.label.clone(), base_url: def.base.clone(),
    auth: auth_str(def.auth).to_string(), auth_header: def.auth_header.clone(),
    extra_headers: headers_json(&def.extra_headers), requires_key: def.requires_key,
    reasoning: reasoning_style_str(def.reasoning).to_string(),
    provider_routing: def.provider_routing, caps: caps_json(def.caps),
    has_key, builtin: def.builtin,
  }
}

// ---------------------------------------------------------------------------
// Resolution
// ---------------------------------------------------------------------------

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

#[derive(Clone)]
pub(crate) struct Resolved {
  pub(crate) def: ProviderDef,
  /// Provider root: the model's `host` for Ollama, otherwise the adapter's base.
  pub(crate) base: String,
  /// The bare model name the provider expects; our prefix is stripped.
  pub(crate) model: String,
  pub(crate) key: Option<String>,
  pub(crate) reasoning: Reasoning,
}

impl Resolved {
  pub(crate) fn dialect(&self) -> Dialect { self.def.dialect }
  pub(crate) fn is_ollama(&self) -> bool { self.def.dialect == Dialect::Ollama }
  pub(crate) fn is_anthropic(&self) -> bool { self.def.dialect == Dialect::Anthropic }
  pub(crate) fn provider_routing(&self) -> bool { self.def.provider_routing }
  pub(crate) fn caps(&self) -> Caps { self.def.caps }
  pub(crate) fn label(&self) -> String { self.def.label.clone() }

  pub(crate) fn chat_url(&self) -> String {
    match self.def.dialect {
      Dialect::Ollama => format!("{}/api/chat", self.base),
      Dialect::Anthropic => format!("{}/messages", self.base),
      Dialect::OpenAiCompat => format!("{}/chat/completions", self.base),
    }
  }

  /// Ollama's single-prompt endpoint. Cloud providers have no equivalent.
  pub(crate) fn generate_url(&self) -> String { format!("{}/api/generate", self.base) }

  pub(crate) fn embeddings_url(&self) -> String {
    match self.def.dialect {
      Dialect::Ollama => format!("{}/api/embeddings", self.base),
      _ => format!("{}/embeddings", self.base),
    }
  }

  pub(crate) fn transcription_url(&self) -> String { format!("{}/audio/transcriptions", self.base) }
  pub(crate) fn speech_url(&self) -> String { format!("{}/audio/speech", self.base) }

  /// Where the provider publishes the models this account can reach. Ollama keeps
  /// its own list at `/api/tags`; every OpenAI-compatible root and Anthropic
  /// expose `/models`.
  pub(crate) fn models_url(&self) -> String {
    match self.def.dialect {
      Dialect::Ollama => format!("{}/api/tags", self.base),
      _ => format!("{}/models", self.base),
    }
  }
}

/// The stored `model` on an agent or role is normally a `"<provider>:<model>"` id,
/// but a label can slip in — e.g. the Manager copying the `list_models` output
/// ("Qwen3 8B (ollama)") straight into `create_agent`. Match it back to a
/// configured model (by exact id, by label, or by the `"<label> (<provider>)"`
/// shape that list prints) so a wrong-but-recognisable value still resolves
/// instead of failing the whole run.
pub(crate) fn normalize_model_id(conn: &Connection, model: &str) -> String {
  let model = model.trim();
  if model.is_empty() { return String::new(); }
  let mut rows: Vec<(String, String, String)> = Vec::new();
  if let Ok(mut stmt) = conn.prepare("SELECT id, label, provider FROM model_configs") {
    if let Ok(iter) = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?))) {
      for r in iter.flatten() { rows.push(r); }
    }
  }
  if rows.iter().any(|(id, _, _)| id.as_str() == model) { return model.to_string(); }
  if let Some((id, _, _)) = rows.iter().find(|(_, label, _)| label.as_str() == model) { return id.clone(); }
  if let Some((id, _, _)) = rows.iter().find(|(_, label, provider)| format!("{label} ({provider})") == model) { return id.clone(); }
  model.to_string()
}

/// Whether a model id's provider is one the app knows — used to reject a bogus
/// model at agent-creation time instead of failing the run later.
pub(crate) fn model_provider_known(conn: &Connection, model: &str) -> bool {
  find_provider(conn, provider_id_of(model)).is_some()
}

/// Resolves a stored `"<provider>:<model>"` id into everything a request needs.
///
/// `override_key` is a caller-supplied API key (the browser preview passes one);
/// it wins over the stored key. Otherwise the key belongs to the **provider** —
/// one key, configured once, shared by every model under it. A model has no key
/// of its own: older installs kept a copy on the row, and that stale copy taking
/// precedence is why updating a provider's key used to change nothing.
pub(crate) fn resolve(conn: &Connection, model_id: &str, override_key: Option<&str>) -> Result<Resolved, String> {
  // A label in place of an id is tolerated rather than fatal.
  let model_id = normalize_model_id(conn, model_id);
  let id = provider_id_of(&model_id);
  let def = find_provider(conn, id)
    .ok_or_else(|| format!("Unknown model provider \"{id}\". Add it in Settings → Providers."))?;
  let (host, reasoning) = model_settings(conn, &model_id)?;
  let key = override_key.filter(|k| !k.trim().is_empty()).map(|k| k.to_string())
    .or_else(|| crate::vault::get(&format!("provider/{}", def.id)));
  if def.requires_key && key.is_none() {
    return Err(format!("{} requires an API key. Add one in Settings → Providers.", def.label));
  }
  let base = base_for(&def, host.as_deref());
  Ok(Resolved { def, base, model: model_of(&model_id), key, reasoning })
}

/// The provider root a request goes to: Ollama honours the editable host,
/// everything else uses the adapter's own base.
fn base_for(def: &ProviderDef, host: Option<&str>) -> String {
  if def.dialect == Dialect::Ollama {
    if let Some(h) = host.filter(|h| !h.trim().is_empty()) {
      return h.trim_end_matches('/').to_string();
    }
  }
  def.base.clone()
}

/// A provider with no model attached: its adapter, root and key. Used by
/// provider-level calls — listing the models an account can reach — where there
/// is no `model_configs` row to read a host from.
pub(crate) fn resolve_provider(conn: &Connection, id: &str, host: Option<&str>) -> Result<Resolved, String> {
  let def = find_provider(conn, id).ok_or_else(|| format!("Unknown provider \"{id}\"."))?;
  let key = crate::vault::get(&format!("provider/{}", def.id));
  let base = base_for(&def, host);
  Ok(Resolved { def, base, model: String::new(), key, reasoning: Reasoning::Auto })
}

/// Applies an agent's own reasoning setting over the model's default. An empty
/// agent value leaves the model's setting in place, so an agent that never chose
/// one still follows its model.
pub(crate) fn apply_agent_reasoning(resolved: &mut Resolved, agent_reasoning: &str) {
  if !agent_reasoning.trim().is_empty() {
    resolved.reasoning = Reasoning::parse(agent_reasoning);
  }
}

fn model_settings(conn: &Connection, model_id: &str) -> Result<(Option<String>, Reasoning), String> {
  let mut stmt = conn
    .prepare("SELECT host, reasoning FROM model_configs WHERE id=?1")
    .map_err(|e| e.to_string())?;
  let mut rows = stmt
    .query_map(params![model_id], |row| {
      Ok((
        row.get::<_, Option<String>>(0)?,
        row.get::<_, Option<String>>(1)?.unwrap_or_else(default_reasoning),
      ))
    })
    .map_err(|e| e.to_string())?;
  match rows.next() {
    Some(Ok((host, reasoning))) => Ok((host, Reasoning::parse(&reasoning))),
    Some(Err(e)) => Err(e.to_string()),
    // A model missing from the table (for example a seed the user never saved)
    // still resolves, so the caller reports the real problem — a missing key.
    None => Ok((None, Reasoning::Auto)),
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

  match resolved.def.reasoning {
    // A provider with no reasoning dialect never receives the setting at all.
    ReasoningStyle::None => return base_max_tokens,
    ReasoningStyle::OllamaThink => {
      // Accepts booleans, or levels for models like GPT-OSS that ignore booleans.
      body["think"] = match reasoning.level() {
        Some(level) => serde_json::json!(level),
        None => serde_json::json!(false),
      };
    }
    ReasoningStyle::OpenRouterEffort => {
      body["reasoning"] = match reasoning.level() {
        Some(level) => serde_json::json!({ "effort": level, "exclude": true }),
        None => serde_json::json!({ "enabled": false }),
      };
    }
    ReasoningStyle::OpenAiEffort => {
      body["reasoning_effort"] = match reasoning.level() {
        Some(level) => serde_json::json!(level),
        None => serde_json::json!("none"),
      };
    }
    ReasoningStyle::AnthropicThinking => {
      // Extended thinking. `max_tokens` must exceed `budget_tokens`, so an
      // enabled level returns a budget large enough to hold the trace plus the
      // answer (apply_openai_defaults writes that into max_tokens).
      let budget = match reasoning.level() {
        Some("low") => 1024u64,
        Some("medium") => 4096,
        Some(_) => 8192,
        None => 0,
      };
      if budget == 0 {
        body["thinking"] = serde_json::json!({ "type": "disabled" });
        return base_max_tokens;
      }
      body["thinking"] = serde_json::json!({ "type": "enabled", "budget_tokens": budget });
      return budget + base_max_tokens;
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

// Endpoint helpers kept as free functions so existing call sites read unchanged.
pub(crate) fn embeddings_url(resolved: &Resolved) -> String { resolved.embeddings_url() }
pub(crate) fn transcription_url(resolved: &Resolved) -> String { resolved.transcription_url() }
pub(crate) fn speech_url(resolved: &Resolved) -> String { resolved.speech_url() }

// ---------------------------------------------------------------------------
// Model roles
//
// One model need not do every job: classification and embeddings can be pinned to
// dedicated models (a small local model, or a hosted embedding API) while chat
// stays on its own. A role is optional — unset means "use the sensible default",
// so an app that never configured roles behaves exactly as before.
// ---------------------------------------------------------------------------

/// The roles a model can fill. The capability roles have hard requirements (see
/// `caps`); the rest fall back to the generic default.
pub(crate) const ROLES: [&str; 6] = ["classifier", "embedding", "reranker", "vision", "transcriber", "distillation"];

/// Whether a model id is likely to be a speech-to-text model. Used to find a
/// usable transcriber without making the user pin one, while refusing to send a
/// clip to an arbitrary chat model (a Groq llama model cannot transcribe).
pub(crate) fn looks_like_transcriber(model_id: &str) -> bool {
  let name = model_id.split_once(':').map(|(_, m)| m).unwrap_or(model_id).to_ascii_lowercase();
  const MARKERS: &[&str] = &["whisper", "transcribe", "speech-to-text", "distil-whisper"];
  MARKERS.iter().any(|m| name.contains(m))
}

/// The model to transcribe with when the caller hasn't named one: an explicitly
/// pinned `transcriber` role wins, then a model typed `stt`, then a whisper-like
/// id. `None` when nothing suitable is configured.
pub(crate) fn resolve_transcriber(conn: &Connection) -> Option<String> {
  if let Some(explicit) = explicit_role(conn, "transcriber") {
    return Some(explicit);
  }
  if let Ok(id) = conn.query_row(
    "SELECT id FROM model_configs WHERE enabled=1 AND kind='stt' ORDER BY id LIMIT 1",
    [], |r| r.get::<_, String>(0),
  ) {
    let id = id.trim().to_string();
    if !id.is_empty() { return Some(id); }
  }
  let mut stmt = conn.prepare("SELECT id FROM model_configs WHERE enabled=1 ORDER BY id").ok()?;
  let rows = stmt.query_map([], |r| r.get::<_, String>(0)).ok()?;
  for row in rows.flatten() {
    if looks_like_transcriber(&row) { return Some(row); }
  }
  None
}

/// Whether a model id is likely to accept images. Providers here publish no
/// capability list, so this is a name read used to decide whether to hand a
/// picture to a model or describe it in words instead. The list is deliberately
/// conservative because the two ways of being wrong are not symmetrical: a
/// capable model that misses the list loses nothing (it gets a description),
/// while a text-only model that hits it receives image parts and fails the turn.
pub(crate) fn looks_like_vision(model_id: &str) -> bool {
  let name = model_id.split_once(':').map(|(_, m)| m).unwrap_or(model_id).to_ascii_lowercase();
  const MARKERS: &[&str] = &[
    "gpt-4o", "gpt-4.1", "gpt-5", "o3", "o4",
    "claude-3", "claude-4", "claude-sonnet", "claude-opus",
    "gemini", "grok-2-vision", "grok-4",
    "llava", "qwen-vl", "qwen2-vl", "llama-3.2-11b", "minicpm-v", "moondream",
  ];
  MARKERS.iter().any(|m| name.contains(m))
}

/// A role that only takes effect when explicitly pinned. Used where a default
/// would silently change existing behaviour (which model edits images, reorders
/// memory, or distils a chat) instead of adding a capability, so an unconfigured
/// app behaves exactly as it did before the role existed.
pub(crate) fn explicit_role(conn: &Connection, role: &str) -> Option<String> {
  conn.query_row("SELECT model_id FROM model_roles WHERE role=?1", params![role], |r| r.get::<_, String>(0))
    .ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

/// Which model fills a role. An explicit `model_roles` row wins; otherwise a
/// deterministic default so the feature works with no configuration:
///
/// * `embedding` — the first enabled model whose provider supports embeddings
///   (preferring a local Ollama one), else `None`.
/// * anything else (`classifier`, `reranker`, `vision`, `distillation`) — the
///   first enabled model, preferring a cloud model over Ollama (structured work
///   on a small local model is unreliable).
pub(crate) fn resolve_role(conn: &Connection, role: &str) -> Option<String> {
  if let Ok(explicit) = conn.query_row("SELECT model_id FROM model_roles WHERE role=?1", params![role], |r| r.get::<_, String>(0)) {
    let explicit = explicit.trim();
    if !explicit.is_empty() { return Some(explicit.to_string()); }
  }
  if role == "embedding" {
    let mut stmt = conn.prepare("SELECT id FROM model_configs WHERE enabled=1 ORDER BY CASE WHEN provider='ollama' THEN 0 ELSE 1 END, id").ok()?;
    let rows = stmt.query_map([], |r| r.get::<_, String>(0)).ok()?;
    for r in rows.flatten() {
      if find_provider(conn, provider_id_of(&r)).map(|d| d.caps.embeddings).unwrap_or(false) {
        return Some(r);
      }
    }
    return None;
  }
  conn.query_row(
    "SELECT id FROM model_configs WHERE enabled=1 ORDER BY CASE WHEN provider='ollama' THEN 1 ELSE 0 END, id LIMIT 1",
    [], |r| r.get(0),
  ).ok()
}

#[cfg(test)]
mod tests {
  use super::*;

  fn def(id: &str) -> ProviderDef { builtins().into_iter().find(|d| d.id == id).unwrap() }

  fn resolved(id: &str, model: &str) -> Resolved {
    let def = def(id);
    Resolved { base: def.base.clone(), def, model: model.into(), key: None, reasoning: Reasoning::Auto }
  }

  #[test]
  fn only_audio_capable_providers_transcribe() {
    let caps = |id: &str| def(id).caps;
    assert!(caps("openai").transcription);
    assert!(caps("groq").transcription);
    assert!(caps("openrouter").transcription, "OpenRouter added a transcription endpoint");
    assert!(!caps("ollama").transcription);
    assert!(!caps("anthropic").transcription);
    assert!(!caps("google").transcription);
  }

  #[test]
  fn openrouter_speaks() {
    assert!(def("openrouter").caps.speech);
    assert!(def("openai").caps.speech);
    assert!(!def("groq").caps.speech);
  }

  /// Endpoints compose from the adapter's base, not by stripping a chat URL.
  #[test]
  fn endpoints_compose_from_the_base() {
    assert_eq!(resolved("openai", "whisper-1").transcription_url(), "https://api.openai.com/v1/audio/transcriptions");
    assert_eq!(resolved("groq", "whisper-large-v3").transcription_url(), "https://api.groq.com/openai/v1/audio/transcriptions");
    assert_eq!(resolved("openrouter", "openai/whisper-1").transcription_url(), "https://openrouter.ai/api/v1/audio/transcriptions");
    assert_eq!(resolved("openrouter", "openai/gpt-4o-mini-tts").speech_url(), "https://openrouter.ai/api/v1/audio/speech");
    assert_eq!(resolved("openai", "gpt-4o-mini").chat_url(), "https://api.openai.com/v1/chat/completions");
    assert_eq!(resolved("anthropic", "claude").chat_url(), "https://api.anthropic.com/v1/messages");
  }

  #[test]
  fn provider_id_is_the_prefix() {
    assert_eq!(provider_id_of("openrouter:openai/whisper-1"), "openrouter");
    assert_eq!(provider_id_of("ollama:qwen3:8b"), "ollama");
    assert_eq!(provider_id_of("bare-model"), "bare-model");
  }
}
