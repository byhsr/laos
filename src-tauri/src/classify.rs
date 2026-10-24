// The universal classifier: one place that interprets an incoming interaction —
// content type, intent, entities, relationships, temporal info, salience — so
// chat, Telegram, capture and workflows all interpret input the same way.
//
// It generalizes the Lumin extractor (`capture.rs`) rather than replacing it: the
// legacy feature/action/params fields are still produced, so `capture::apply`
// keeps working, and the richer fields feed the brain map.
//
// It never fails: an unparsable reply degrades to a plain note/conversation, so
// nothing the user sends is ever lost (the brief's contract).

use serde::Deserialize;
use serde_json::json;
use tauri::AppHandle;

use crate::db::db;

pub(crate) const CONTENT_TYPES: [&str; 13] = [
  "note", "task", "reminder", "habit", "goal", "idea", "project", "decision",
  "person", "preference", "event", "conversation", "other",
];

#[derive(Deserialize, Default, Clone)]
pub(crate) struct Entity {
  #[serde(default)] pub name: String,
  #[serde(default)] pub kind: String,
}

#[derive(Deserialize, Default, Clone)]
pub(crate) struct Relationship {
  #[serde(default)] pub target: String,
  #[serde(default)] pub relation: String,
}

#[derive(Deserialize, Default, Clone)]
pub(crate) struct Params {
  #[serde(default)] pub title: String,
  #[serde(default)] pub body: String,
  #[serde(default)] pub url: String,
  #[serde(default)] pub tags: Vec<String>,
  #[serde(default)] pub due_at: Option<String>,
  #[serde(default)] pub cadence: String,
}

#[derive(Deserialize, Default, Clone)]
pub(crate) struct Classification {
  #[serde(default)] pub content_type: String,
  #[serde(default)] pub intent: String,
  #[serde(default)] pub entities: Vec<Entity>,
  #[serde(default)] pub relationships: Vec<Relationship>,
  #[serde(default)] pub salience: f64,
  #[serde(default)] pub salience_signals: Vec<String>,
  #[serde(default)] pub should_act: bool,
  #[serde(default)] pub confidence: f64,
  // Legacy Lumin fields (so items still fill correctly).
  #[serde(default)] pub feature: String,
  #[serde(default)] pub action: String,
  #[serde(default)] pub params: Params,
}

const CLASSIFIER: &str = "You are the classifier — the single interpreter every interaction passes through. \
Turn messy, low-effort language into one structured reading. Output strict JSON only: no prose, no code fences.\n\
\n\
SCHEMA:\n\
{\n\
  \"content_type\": \"note|task|reminder|habit|goal|idea|project|decision|person|preference|event|conversation|other\",\n\
  \"intent\": \"create|update|complete|search|recall|plan|reflect|act|converse\",\n\
  \"entities\": [{\"name\":\"a person, project, org, tool or place referenced\",\"kind\":\"person|project|organization|tool|place\"}],\n\
  \"relationships\": [{\"target\":\"name of an existing thing\",\"relation\":\"belongs to project|related to|blocks|supports|about\"}],\n\
  \"salience\": 0.0,\n\
  \"salience_signals\": [\"novelty|urgency|social|intensity|repetition\"],\n\
  \"should_act\": false,\n\
  \"confidence\": 0.0,\n\
  \"feature\": \"note|todo|reminder|promise|link|habit|null\",\n\
  \"action\": \"create|read|update|delete|complete|null\",\n\
  \"params\": {\n\
    \"title\": \"short title, 3-7 words\",\n\
    \"body\": \"the full text as the user said it\",\n\
    \"url\": \"the link, or empty\",\n\
    \"tags\": [\"#tags without #\"],\n\
    \"due_at\": \"absolute local ISO-8601 timestamp, or null\",\n\
    \"cadence\": \"daily|weekly|monthly for habits, else empty\"\n\
  }\n\
}\n\
\n\
RULES\n\
- content_type: the general kind of thing. A 'promise'/commitment to future behaviour is a goal; \
an explicit 'I will not' or a firm call is a decision; a captured person is a person; a durable way of \
working is a preference; a bare conversation is conversation.\n\
- feature (Lumin): reminder = has a time; todo = intends to do it, no time; promise = an explicit \
commitment (\"I will\", \"I'll\"); habit = something repeated (\"every day\", \"daily\"); link = the message \
shares a URL; note = an idea/fact worth keeping; chat = just conversation (feature and action null).\n\
- action: create for new things; complete for \"I did X\"; read for \"show me / what are my X\"; update for \
\"change X to Y\"; delete for \"remove X\".\n\
- salience is how likely the thing is to capture attention (novelty, urgency, social pull, intensity, \
repetition) — NOT importance, usefulness or positivity. 0.0 = easy to ignore, 1.0 = very hard to ignore.\n\
- Resolve EVERY relative time to an absolute local ISO-8601 timestamp in params.due_at. Never leave \
\"tonight\" or \"6pm\" as text.\n\
- should_act is true only when the message asks for an action to be taken now.\n\
- Always fill every field; unknown -> null or empty string / empty array.";

fn classifier_system() -> String {
  let now = chrono::Local::now();
  format!("{CLASSIFIER}\n\nRight now it is {} (local time, UTC offset {}). Resolve times against this.",
    now.format("%Y-%m-%d %H:%M %A"), now.offset())
}

pub(crate) fn normalize(c: &mut Classification, raw: &str) {
  if !CONTENT_TYPES.contains(&c.content_type.trim()) {
    c.content_type = match c.feature.trim() {
      "todo" => "task".into(),
      "reminder" => "reminder".into(),
      "habit" => "habit".into(),
      "promise" => "goal".into(),
      _ => "note".into(),
    };
  }
  if c.intent.trim().is_empty() || c.intent == "null" {
    c.intent = if c.content_type == "conversation" { "converse".into() } else { "create".into() };
  }
  if !crate::capture::KINDS.contains(&c.feature.trim()) { c.feature = "note".into(); }
  if c.action.trim().is_empty() || c.action == "null" { c.action = "create".into(); }
  if c.params.body.trim().is_empty() { c.params.body = raw.trim().to_string(); }
  if c.params.title.trim().is_empty() {
    let words: Vec<&str> = c.params.body.split_whitespace().collect();
    c.params.title = if words.len() > 7 { format!("{}…", words[..7].join(" ")) } else { c.params.body.clone() };
  }
  c.salience = c.salience.clamp(0.0, 1.0);
  c.confidence = c.confidence.clamp(0.0, 1.0);
  // Drop nameless entities/relationships early.
  c.entities.retain(|e| !e.name.trim().is_empty());
  c.relationships.retain(|r| !r.target.trim().is_empty());
}

pub(crate) fn parse(raw: &str) -> Classification {
  let cleaned = raw.replace("```json", "").replace("```", "");
  serde_json::from_str::<Classification>(cleaned.trim()).unwrap_or_default()
}

async fn classify_inner(app: &AppHandle, text: &str) -> Result<Classification, String> {
  let reply = {
    let conn = db(app)?;
    let model_id = crate::provider::resolve_role(&conn, "classifier").ok_or("No model configured.")?;
    let resolved = crate::provider::resolve(&conn, &model_id, None)?;
    let mut body = json!({
      "model": resolved.model,
      "messages": [
        { "role": "system", "content": classifier_system() },
        { "role": "user", "content": text }
      ],
      "temperature": 0.1,
      "stream": false
    });
    crate::http::apply_openai_defaults(&mut body, resolved.kind == crate::provider::Kind::OpenRouter, 500);
    crate::provider::apply_reasoning(&mut body, &resolved, 500);
    if resolved.kind == crate::provider::Kind::Anthropic { body = crate::anthropic::to_wire(&body); }
    let client = crate::http::client();
    let resp = crate::http::send_model_request(&client, &resolved, &resolved.chat_url(), &body, crate::http::MODEL_ATTEMPTS).await?;
    let mut reply: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    if resolved.kind == crate::provider::Kind::Anthropic { reply = crate::anthropic::normalize_response(&reply); }
    reply["choices"][0]["message"]["content"].as_str().unwrap_or("").to_string()
  };
  Ok(parse(&reply))
}

/// Classifies an interaction. Never errors: a failed call degrades to a note.
pub(crate) async fn classify(app: &AppHandle, text: &str, _scope_hint: &str) -> Classification {
  let mut c = classify_inner(app, text).await.unwrap_or_default();
  normalize(&mut c, text);
  c
}

#[cfg(test)]
mod tests {
  use super::*;

  /// The brief's contract: a failed or unparsable classification must not lose the
  /// input. It degrades to a plain note that still carries the user's words.
  #[test]
  fn unparsable_output_degrades_to_a_note() {
    let mut c = parse("this is not json at all");
    normalize(&mut c, "buy milk tomorrow");
    assert_eq!(c.content_type, "note");
    assert_eq!(c.params.body, "buy milk tomorrow", "the raw text is preserved");
    assert!(!c.params.title.is_empty(), "a title is always produced");
    assert_eq!(c.salience, 0.0);
  }

  /// A half-filled reply is completed rather than rejected.
  #[test]
  fn partial_output_is_normalized() {
    let mut c = parse(r#"{"content_type":"bogus","params":{"body":"call the dentist"}}"#);
    normalize(&mut c, "call the dentist");
    assert_eq!(c.content_type, "note", "an unknown content type falls back");
    assert_eq!(c.intent, "create");
    assert_eq!(c.params.title, "call the dentist");
  }
}

/// The legacy intent adapter, so `capture::apply` (items) keeps working unchanged.
pub(crate) fn to_intent(c: &Classification) -> crate::capture::Intent {
  crate::capture::Intent {
    r#type: if c.content_type == "conversation" { "chat".into() } else { "feature".into() },
    feature: c.feature.clone(),
    action: c.action.clone(),
    params: crate::capture::IntentParams {
      title: c.params.title.clone(),
      body: c.params.body.clone(),
      url: c.params.url.clone(),
      tags: c.params.tags.clone(),
      due_at: c.params.due_at.clone(),
      cadence: c.params.cadence.clone(),
    },
    confidence: c.confidence,
  }
}
