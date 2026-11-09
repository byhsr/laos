// Audio transcription through the `transcriber` model role.
//
// A voice note — recorded in the composer, dropped in as a file, or sent to a
// Telegram bot — is turned into text by an OpenAI-compatible
// `/audio/transcriptions` endpoint (OpenAI, Groq, or a local whisper server
// speaking the same API). The text then joins the turn like any other words.
//
// Optional at every call site: with no `transcriber` role pinned, or a provider
// that cannot transcribe, the caller says so plainly rather than dropping audio.

use tauri::AppHandle;

use crate::provider;

/// Hard cap on one audio file. Telegram's own download limit is 20 MB, and a
/// few minutes of compressed voice is far below this, so it only guards against a
/// mis-picked music file.
pub(crate) const MAX_AUDIO_BYTES: usize = 25 * 1024 * 1024;

const DEFAULT_MIME: &str = "audio/ogg";

/// The filename to hand the provider, with an extension the endpoint recognises.
/// A missing name or extension is filled from the mime type.
pub(crate) fn audio_filename(name: &str, mime: &str) -> String {
  let name = name.trim();
  if !name.is_empty() && name.contains('.') {
    return name.to_string();
  }
  let ext = match mime.trim() {
    m if m.starts_with("audio/webm") => "webm",
    m if m.starts_with("audio/mpeg") || m.starts_with("audio/mp3") => "mp3",
    m if m.starts_with("audio/mp4") || m.starts_with("audio/m4a") => "m4a",
    m if m.starts_with("audio/wav") || m.starts_with("audio/x-wav") => "wav",
    m if m.starts_with("audio/flac") => "flac",
    _ => "ogg",
  };
  if name.is_empty() { format!("audio.{ext}") } else { format!("{name}.{ext}") }
}

/// A multipart part carrying the audio bytes. The mime is set when the provider
/// can parse it; otherwise the filename extension still tells the endpoint the
/// format, so a malformed mime never drops the file.
fn audio_part(filename: &str, mime: &str, bytes: &[u8]) -> reqwest::multipart::Part {
  let part = reqwest::multipart::Part::bytes(bytes.to_vec()).file_name(filename.to_string());
  if mime.trim().is_empty() {
    part
  } else {
    part.mime_str(mime).unwrap_or_else(|_| reqwest::multipart::Part::bytes(bytes.to_vec()).file_name(filename.to_string()))
  }
}

/// Transcribes one audio clip through the given model. Returns Err (never
/// panics) when the provider cannot transcribe, the file is unusable, or the
/// call fails — callers degrade to a note rather than losing the attachment.
pub(crate) async fn transcribe(app: &AppHandle, model_id: &str, name: &str, mime: &str, bytes: &[u8]) -> Result<String, String> {
  if bytes.is_empty() { return Err("the audio was empty".into()); }
  if bytes.len() > MAX_AUDIO_BYTES {
    return Err(format!("the audio is too large (limit {} MB)", MAX_AUDIO_BYTES / (1024 * 1024)));
  }
  let resolved = {
    let conn = crate::db::db(app)?;
    provider::resolve(&conn, model_id, None)?
  };
  if !resolved.caps().transcription {
    return Err("that provider cannot transcribe audio".into());
  }
  let url = provider::transcription_url(&resolved);
  let model = resolved.model.clone();
  let filename = audio_filename(name, mime);
  let mime = if mime.trim().is_empty() { DEFAULT_MIME.to_string() } else { mime.trim().to_string() };
  let bytes = bytes.to_vec();
  let client = crate::http::client();

  // Rebuilt per attempt: a multipart body is consumed by `send`, so the retry
  // loop needs a fresh form each time.
  let make = || {
    let form = reqwest::multipart::Form::new()
      .part("file", audio_part(&filename, &mime, &bytes))
      .text("model", model.clone());
    crate::http::auth_headers(client.post(&url).multipart(form), &resolved)
  };
  let response = crate::http::send_with_retry(make, crate::http::MODEL_ATTEMPTS).await?;
  let json: serde_json::Value = crate::http::json_body(response).await?;
  let text = json["text"].as_str().unwrap_or("").trim().to_string();
  if text.is_empty() { return Err("the transcriber returned nothing".into()); }
  Ok(text)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn a_filename_keeps_its_own_extension() {
    assert_eq!(audio_filename("note.m4a", "audio/mp4"), "note.m4a");
    assert_eq!(audio_filename("voice.ogg", ""), "voice.ogg");
  }

  #[test]
  fn a_missing_extension_is_filled_from_the_mime() {
    assert_eq!(audio_filename("note", "audio/webm;codecs=opus"), "note.webm");
    assert_eq!(audio_filename("", "audio/ogg"), "audio.ogg");
    assert_eq!(audio_filename("", "audio/mpeg"), "audio.mp3");
    assert_eq!(audio_filename("", ""), "audio.ogg", "an unknown type falls back to ogg");
  }
}
