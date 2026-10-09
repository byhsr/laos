// Anthropic Messages API adapter.
//
// The rest of the harness builds and reads **OpenAI-shaped** request bodies and
// responses. Anthropic is not OpenAI-compatible, so this module translates both
// directions and every call site only needs a guard when the provider is
// Anthropic:
//   * `to_wire`  — OpenAI request body -> Anthropic `/v1/messages` body
//   * `normalize_response` — Anthropic response -> OpenAI-shaped response
// The streaming deltas are mapped in `chat.rs` (Anthropic SSE uses named events).

use serde_json::{json, Value};

/// Converts an OpenAI-shaped body into a request for `POST /v1/messages`.
pub(crate) fn to_wire(body: &Value) -> Value {
  let model = body.get("model").cloned().unwrap_or(json!(""));
  let max_tokens = body.get("max_tokens").cloned().unwrap_or(json!(4096));
  let stream = body.get("stream").and_then(|s| s.as_bool()).unwrap_or(false);

  let mut system = String::new();
  let mut messages: Vec<Value> = Vec::new();
  if let Some(arr) = body.get("messages").and_then(|m| m.as_array()) {
    for m in arr {
      let role = m.get("role").and_then(|r| r.as_str()).unwrap_or("");
      match role {
        "system" => {
          let text = content_text(m.get("content").unwrap_or(&Value::Null));
          if !text.is_empty() {
            if !system.is_empty() { system.push('\n'); }
            system.push_str(&text);
          }
        }
        "tool" => {
          // A tool result is a user message carrying a tool_result block.
          let tool_use_id = m.get("tool_call_id").and_then(|c| c.as_str()).unwrap_or("");
          let result = content_text(m.get("content").unwrap_or(&Value::Null));
          messages.push(json!({ "role": "user", "content": [
            { "type": "tool_result", "tool_use_id": tool_use_id, "content": result }
          ] }));
        }
        "assistant" => {
          let mut blocks: Vec<Value> = Vec::new();
          append_text_blocks(&mut blocks, m.get("content").unwrap_or(&Value::Null));
          if let Some(calls) = m.get("tool_calls").and_then(|c| c.as_array()) {
            for call in calls {
              let id = call.get("id").and_then(|v| v.as_str()).unwrap_or("");
              let f = call.get("function").cloned().unwrap_or_else(|| json!({}));
              let name = f.get("name").and_then(|v| v.as_str()).unwrap_or("");
              let input = match f.get("arguments") {
                Some(Value::String(s)) => serde_json::from_str(s).unwrap_or_else(|_| json!({})),
                Some(v) => v.clone(),
                None => json!({}),
              };
              blocks.push(json!({ "type": "tool_use", "id": id, "name": name, "input": input }));
            }
          }
          // Anthropic rejects an empty content array.
          if blocks.is_empty() { blocks.push(json!({ "type": "text", "text": "" })); }
          messages.push(json!({ "role": "assistant", "content": blocks }));
        }
        _ => {
          messages.push(json!({ "role": "user", "content": to_user_content(m.get("content").unwrap_or(&Value::Null)) }));
        }
      }
    }
  }

  let mut out = json!({ "model": model, "max_tokens": max_tokens, "messages": messages });
  if !system.is_empty() { out["system"] = json!(system); }
  if stream { out["stream"] = json!(true); }

  if let Some(tools) = body.get("tools").and_then(|t| t.as_array()) {
    let converted: Vec<Value> = tools.iter().map(|t| {
      let f = t.get("function").cloned().unwrap_or_else(|| json!({}));
      json!({
        "name": f.get("name").cloned().unwrap_or(json!("")),
        "description": f.get("description").cloned().unwrap_or(json!("")),
        "input_schema": f.get("parameters").cloned().unwrap_or(json!({ "type": "object", "properties": {} })),
      })
    }).collect();
    if !converted.is_empty() { out["tools"] = json!(converted); }
  }

  // Extended thinking, if the reasoning layer set it.
  if let Some(thinking) = body.get("thinking") { out["thinking"] = thinking.clone(); }
  out
}

/// Rewrites an Anthropic response into the OpenAI shape the call sites read:
/// `choices[0].message.{content, tool_calls}` plus `usage.{prompt,completion}_tokens`.
pub(crate) fn normalize_response(json: &Value) -> Value {
  let mut text = String::new();
  let mut tool_calls: Vec<Value> = Vec::new();
  if let Some(blocks) = json.get("content").and_then(|c| c.as_array()) {
    for b in blocks {
      match b.get("type").and_then(|t| t.as_str()) {
        Some("text") => {
          if let Some(t) = b.get("text").and_then(|t| t.as_str()) { text.push_str(t); }
        }
        Some("tool_use") => {
          let id = b.get("id").and_then(|v| v.as_str()).unwrap_or("");
          let name = b.get("name").and_then(|v| v.as_str()).unwrap_or("");
          let input = b.get("input").cloned().unwrap_or_else(|| json!({}));
          tool_calls.push(json!({
            "id": id,
            "type": "function",
            "function": { "name": name, "arguments": serde_json::to_string(&input).unwrap_or_else(|_| "{}".into()) },
          }));
        }
        _ => {} // thinking / other blocks are ignored
      }
    }
  }

  let mut message = json!({ "role": "assistant", "content": text });
  if !tool_calls.is_empty() { message["tool_calls"] = json!(tool_calls); }

  let prompt = json.get("usage").and_then(|u| u.get("input_tokens")).and_then(|v| v.as_u64()).unwrap_or(0);
  let completion = json.get("usage").and_then(|u| u.get("output_tokens")).and_then(|v| v.as_u64()).unwrap_or(0);

  json!({
    "choices": [ { "message": message } ],
    "usage": { "prompt_tokens": prompt, "completion_tokens": completion },
  })
}

/// Flattens an OpenAI content value (string or text parts) into plain text.
fn content_text(v: &Value) -> String {
  match v {
    Value::String(s) => s.clone(),
    Value::Array(parts) => parts.iter().filter_map(|p| {
      p.as_str().map(|s| s.to_string()).or_else(|| p.get("text").and_then(|t| t.as_str()).map(|s| s.to_string()))
    }).collect::<Vec<_>>().join("\n"),
    _ => String::new(),
  }
}

fn append_text_blocks(blocks: &mut Vec<Value>, content: &Value) {
  match content {
    Value::String(s) => { if !s.is_empty() { blocks.push(json!({ "type": "text", "text": s })); } }
    Value::Array(parts) => {
      for p in parts {
        if let Some(t) = p.get("text").and_then(|t| t.as_str()) {
          if !t.is_empty() { blocks.push(json!({ "type": "text", "text": t })); }
        }
      }
    }
    _ => {}
  }
}

/// Converts a user content value into Anthropic content (text + image blocks).
/// OpenAI image parts are `data:` URLs; Anthropic wants base64 source blocks.
fn to_user_content(v: &Value) -> Value {
  match v {
    Value::String(s) => json!(s),
    Value::Array(parts) => {
      let blocks: Vec<Value> = parts.iter().filter_map(|p| {
        match p.get("type").and_then(|t| t.as_str()) {
          Some("text") => Some(json!({ "type": "text", "text": p.get("text").cloned().unwrap_or_else(|| json!("")) })),
          Some("image_url") => {
            let url = p.get("image_url").and_then(|i| i.get("url")).and_then(|u| u.as_str()).unwrap_or("");
            parse_data_url(url).map(|(media_type, data)| json!({
              "type": "image",
              "source": { "type": "base64", "media_type": media_type, "data": data },
            }))
          }
          _ => None,
        }
      }).collect();
      json!(blocks)
    }
    _ => json!(""),
  }
}

/// `data:image/png;base64,AAAA` -> ("image/png", "AAAA").
fn parse_data_url(url: &str) -> Option<(String, String)> {
  let rest = url.strip_prefix("data:")?;
  let (meta, data) = rest.split_once(',')?;
  let media_type = meta.split(';').next().filter(|s| !s.is_empty()).unwrap_or("image/png").to_string();
  Some((media_type, data.to_string()))
}
