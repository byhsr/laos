// Media dropped into a chat turn: screenshots, photos, pasted images, plus voice
// notes and audio files.
//
// Which route an image takes is the user's choice in Settings → Models:
//   * a `vision` model is pinned — it looks at the images and the turn carries
//     its written description, so any chat model can act on what is in them;
//   * no vision model — the images ride along in the request itself, so a
//     vision-capable chat model sees them directly.
// Audio takes a single route: a pinned `transcriber` model turns it into text,
// which joins the turn like anything the user typed. Nothing is ever silently
// dropped: a file the model cannot read is named for it.

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;
use serde::Deserialize;
use serde_json::json;
use tauri::AppHandle;

use crate::provider;

/// Up to this many files per turn, and this much per file. The cap exists so a
/// mis-drop cannot stall the IPC bridge or blow up a provider request.
const MAX_ATTACHMENTS: usize = 4;
const MAX_BYTES: usize = 6 * 1024 * 1024;
const VISION_MAX_TOKENS: u64 = 900;

/// A file the composer handed over. `data` is base64 without the `data:` prefix,
/// matching what the vision tools already produce.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Attachment {
  #[serde(default)] pub name: String,
  #[serde(default)] pub mime: String,
  #[serde(default)] pub data: String,
}

impl Attachment {
  fn is_image(&self) -> bool { self.mime.starts_with("image/") && !self.data.is_empty() }
  fn is_audio(&self) -> bool { self.mime.starts_with("audio/") && !self.data.is_empty() }
  /// Decoded size, estimated from the base64 length so nothing has to be decoded.
  fn bytes(&self) -> usize { self.data.len() / 4 * 3 }
  fn label(&self) -> String {
    let name = if self.name.trim().is_empty() { "attachment" } else { self.name.trim() };
    let mime = if self.mime.trim().is_empty() { "unknown type" } else { self.mime.trim() };
    format!("{name} ({mime})")
  }
  fn data_url(&self) -> String { format!("data:{};base64,{}", self.mime, self.data) }
}

/// Decodes an attachment's base64 payload to bytes (audio has to be uploaded as
/// a file, unlike images which ride along as data URLs).
pub(crate) fn decode_audio(data: &str) -> Result<Vec<u8>, String> {
  BASE64.decode(data.trim()).map_err(|e| e.to_string())
}

/// What a turn should do with the media it was handed.
pub(crate) struct Prepared {
  /// Text appended to the user's turn, saying what came with it. Part of the
  /// turn so a follow-up question still knows what was on screen.
  pub(crate) note: String,
  /// Image data URLs for the request itself, when no vision model is pinned to
  /// describe them instead.
  pub(crate) images: Vec<String>,
}

/// Decides how the dropped files reach the model. Never fails: an unusable
/// attachment is described in the note rather than dropped.
pub(crate) async fn prepare(app: &AppHandle, chat_model: &str, attachments: &[Attachment]) -> Prepared {
  let files: Vec<&Attachment> = attachments.iter().take(MAX_ATTACHMENTS).collect();
  if files.is_empty() { return Prepared { note: String::new(), images: Vec::new() }; }

  let images: Vec<&Attachment> = files.iter().copied().filter(|a| a.is_image() && a.bytes() <= MAX_BYTES).collect();
  let mut lines: Vec<String> = Vec::new();
  let mut native: Vec<String> = Vec::new();

  if !images.is_empty() {
    let names = images.iter().map(|a| a.label()).collect::<Vec<_>>().join(", ");
    let vision = crate::db::db(app).ok().and_then(|c| provider::explicit_role(&c, "vision"));
    match vision {
      // The pinned vision model does the looking. Its words are what the chat
      // model gets, so the picture only ever reaches one provider.
      Some(vid) => match describe(app, &vid, &images).await {
        Ok(summary) => lines.push(format!("- {names}: {summary}")),
        Err(e) => {
          native.extend(images.iter().map(|a| a.data_url()));
          lines.push(format!("- {names}: the vision model could not describe these ({e}); attached instead."));
        }
      },
      // No vision model chosen: hand the pictures over, but only when the chat
      // model looks like it can see — otherwise say plainly that it cannot.
      None if provider::looks_like_vision(chat_model) => {
        native.extend(images.iter().map(|a| a.data_url()));
        lines.push(format!("- {names}: attached as image(s) — look at them directly."));
      }
      None => lines.push(format!(
        "- {names}: attached, but no vision model is set and {chat_model} is not a model that can be shown images. Pick one under vision in Settings → Models."
      )),
    }
  }

  // Audio: transcribed into the turn text through the `transcriber` role. With no
  // role pinned, or a failure, the file is named so the user knows why it is mute.
  let transcriber = crate::db::db(app).ok().and_then(|c| provider::explicit_role(&c, "transcriber"));
  for a in files.iter().copied().filter(|a| a.is_audio()) {
    if a.bytes() > crate::transcribe::MAX_AUDIO_BYTES {
      lines.push(format!("- {}: too large to transcribe (limit {} MB).", a.label(), crate::transcribe::MAX_AUDIO_BYTES / (1024 * 1024)));
      continue;
    }
    let Some(model) = transcriber.clone() else {
      lines.push(format!("- {}: audio attached, but no transcriber model is set — pick one under transcriber in Settings → Models.", a.label()));
      continue;
    };
    match decode_audio(&a.data) {
      Ok(bytes) => match crate::transcribe::transcribe(app, &model, &a.name, &a.mime, &bytes).await {
        Ok(text) => lines.push(format!("- {}: “{text}”", a.label())),
        Err(e) => lines.push(format!("- {}: could not be transcribed ({e}).", a.label())),
      },
      Err(e) => lines.push(format!("- {}: could not be read ({e}).", a.label())),
    }
  }

  for a in files.iter().copied().filter(|a| a.is_image() && a.bytes() > MAX_BYTES) {
    lines.push(format!("- {}: too large to send (limit {} MB).", a.label(), MAX_BYTES / (1024 * 1024)));
  }
  for a in files.iter().copied().filter(|a| !a.is_image() && !a.is_audio()) {
    lines.push(format!("- {}: attached, but it is not an image, so you cannot see its contents.", a.label()));
  }

  let note = if lines.is_empty() { String::new() } else { format!("[attached files]\n{}", lines.join("\n")) };
  Prepared { note, images: native }
}

/// The one vision call: images in, a description out. Deliberately not a chat
/// turn — it only has to produce something the chat model can act on.
async fn describe(app: &AppHandle, model_id: &str, images: &[&Attachment]) -> Result<String, String> {
  let resolved = {
    let conn = crate::db::db(app)?;
    provider::resolve(&conn, model_id, None)?
  };
  if !resolved.caps().vision {
    return Err("that provider cannot take images".into());
  }
  let prompt = "Look at the attached image(s) and describe them for another model that cannot see them. \
    Give any text, code, error message, label or UI state verbatim where it matters, then the salient details, \
    then a one-line summary of what the image is. Be concrete and do not speculate about anything not visible.";

  let mut body = if resolved.is_ollama() {
    json!({
      "model": resolved.model,
      "messages": [{ "role": "user", "content": prompt, "images": images.iter().map(|a| a.data.clone()).collect::<Vec<_>>() }],
      "stream": false,
    })
  } else {
    let mut content = vec![json!({ "type": "text", "text": prompt })];
    for a in images { content.push(json!({ "type": "image_url", "image_url": { "url": a.data_url() } })); }
    json!({ "model": resolved.model, "messages": [{ "role": "user", "content": content }], "stream": false })
  };

  let budget = provider::apply_reasoning(&mut body, &resolved, VISION_MAX_TOKENS);
  if resolved.is_ollama() {
    provider::apply_ollama_options(&mut body, budget);
  } else {
    crate::http::apply_openai_defaults(&mut body, resolved.provider_routing(), budget);
  }
  if resolved.is_anthropic() { body = crate::anthropic::to_wire(&body); }

  let client = crate::http::client();
  let response = crate::http::send_model_request(&client, &resolved, &resolved.chat_url(), &body, crate::http::MODEL_ATTEMPTS).await?;
  let mut parsed: serde_json::Value = crate::http::json_body(response).await?;
  if resolved.is_anthropic() { parsed = crate::anthropic::normalize_response(&parsed); }
  // OpenAI-compatible nest the reply under choices[0]; Ollama puts it at the top.
  let text = parsed["choices"][0]["message"]["content"].as_str()
    .or_else(|| parsed["message"]["content"].as_str())
    .unwrap_or("")
    .trim()
    .to_string();
  if text.is_empty() { return Err("the vision model returned nothing".into()); }
  Ok(text)
}

#[cfg(test)]
mod tests {
  use super::*;

  fn att(name: &str, mime: &str, data: &str) -> Attachment {
    Attachment { name: name.into(), mime: mime.into(), data: data.into() }
  }

  #[test]
  fn only_images_with_payloads_count_as_images() {
    assert!(att("a.png", "image/png", "AAAA").is_image());
    assert!(!att("a.png", "image/png", "").is_image());
    assert!(!att("doc.pdf", "application/pdf", "AAAA").is_image());
  }

  #[test]
  fn only_audio_with_payloads_counts_as_audio() {
    assert!(att("a.ogg", "audio/ogg", "AAAA").is_audio());
    assert!(!att("a.ogg", "audio/ogg", "").is_audio());
    assert!(!att("a.png", "image/png", "AAAA").is_audio());
    assert!(!att("doc.pdf", "application/pdf", "AAAA").is_audio());
  }

  #[test]
  fn audio_decodes_from_base64_for_upload() {
    // "AAAA" is three zero bytes.
    assert_eq!(decode_audio("AAAA").unwrap(), vec![0u8, 0, 0]);
    assert!(decode_audio("not base64 £").is_err());
  }

  #[test]
  fn the_size_estimate_matches_base64_length() {
    // 4 base64 chars carry 3 bytes.
    assert_eq!(att("a.png", "image/png", "AAAAAAAA").bytes(), 6);
  }

  #[test]
  fn a_data_url_carries_the_mime_the_provider_needs() {
    assert_eq!(att("a.png", "image/png", "AAAA").data_url(), "data:image/png;base64,AAAA");
  }

  #[test]
  fn a_model_that_looks_like_it_can_see_is_recognised() {
    assert!(provider::looks_like_vision("openai:gpt-4o-mini"));
    assert!(provider::looks_like_vision("ollama:llava:13b"));
    assert!(provider::looks_like_vision("anthropic:claude-sonnet-4-5"));
    assert!(provider::looks_like_vision("google:gemini-2.0-flash"));
    assert!(!provider::looks_like_vision("ollama:qwen3:8b"));
    assert!(!provider::looks_like_vision("groq:llama-3.3-70b-versatile"));
  }
}
