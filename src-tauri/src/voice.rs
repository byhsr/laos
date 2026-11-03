// Live voice mode: speech-to-text for one captured utterance, and text-to-speech
// for the reply to it.
//
// Both are optional and fail loudly-but-gently: with no suitable model the
// command says so rather than dropping audio silently, and the caller can fall
// back (system voices for speech, a typed message for words).

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;
use rusqlite::Connection;
use tauri::AppHandle;
use tauri_plugin_global_shortcut::GlobalShortcutExt;

use crate::db::db;
use crate::provider;

pub(crate) const DEFAULT_HOTKEY: &str = "CommandOrControl+Shift+Space";

/// The persisted global hotkey; an empty string means "disabled".
pub(crate) fn hotkey(app: &AppHandle) -> String {
  db(app)
    .ok()
    .and_then(|c| crate::app_settings::get(&c, "voice_hotkey"))
    .map(|s| s.trim().to_string())
    .unwrap_or_else(|| DEFAULT_HOTKEY.to_string())
}

/// Registers the global hotkey, replacing whatever was there. An empty
/// accelerator disables it. On failure the previous shortcut is restored, so a
/// bad value can never leave voice mode unreachable.
#[tauri::command]
pub fn set_voice_hotkey(app: AppHandle, accelerator: String) -> Result<(), String> {
  let accel = accelerator.trim().to_string();
  let previous = hotkey(&app);
  let gs = app.global_shortcut();
  let _ = gs.unregister_all();
  if !accel.is_empty() {
    if let Err(e) = gs.register(accel.as_str()) {
      if !previous.is_empty() {
        let _ = gs.register(previous.as_str());
      }
      return Err(format!("could not register “{accel}”: {e}"));
    }
  }
  let conn = db(&app)?;
  crate::app_settings::set(&conn, "voice_hotkey", &accel)
}

/// The model to speak with: a pinned `tts` role, else a model typed `tts`.
fn resolve_tts(conn: &Connection) -> Option<String> {
  provider::explicit_role(conn, "tts")
    .or_else(|| {
      conn.query_row(
        "SELECT id FROM model_configs WHERE enabled=1 AND kind='tts' ORDER BY id LIMIT 1",
        [], |r| r.get::<_, String>(0),
      ).ok()
    })
    .map(|s| s.trim().to_string())
    .filter(|s| !s.is_empty())
}

/// Transcribes one captured utterance. Prefers a pinned `transcriber`, then a
/// model typed `stt`, then a whisper-like id.
#[tauri::command]
pub async fn voice_transcribe(app: AppHandle, name: Option<String>, mime: Option<String>, data: String) -> Result<String, String> {
  let bytes = crate::media::decode_audio(&data)?;
  let model = {
    let conn = db(&app)?;
    provider::resolve_transcriber(&conn).ok_or_else(|| {
      "No speech-to-text model. Add an OpenAI/Groq whisper model, or pin a transcriber in Settings → Models.".to_string()
    })?
  };
  let name = name.unwrap_or_else(|| "voice.webm".into());
  let mime = mime.unwrap_or_else(|| "audio/webm".into());
  crate::transcribe::transcribe(&app, &model, &name, &mime, &bytes).await
}

/// Synthesises speech for the reply. Returns base64 mp3, or `None` when no TTS
/// model is configured (the frontend then uses the system voices).
#[tauri::command]
pub async fn voice_speak(app: AppHandle, text: String) -> Result<Option<String>, String> {
  let text = text.trim().to_string();
  if text.is_empty() {
    return Ok(None);
  }
  let (model_id, fallback_voice) = {
    let conn = db(&app)?;
    let voice = crate::app_settings::get(&conn, "tts_voice").unwrap_or_else(|| "alloy".to_string());
    (resolve_tts(&conn), voice)
  };
  let Some(model_id) = model_id else { return Ok(None) };
  let (resolved, model_voice) = {
    let conn = db(&app)?;
    let resolved = provider::resolve(&conn, &model_id, None)?;
    // A voice set on the model wins — provider voices are model-specific.
    let voice = conn.query_row("SELECT voice FROM model_configs WHERE id=?1", rusqlite::params![model_id], |r| r.get::<_, String>(0)).ok().unwrap_or_default();
    (resolved, voice)
  };
  if !resolved.caps().speech {
    return Ok(None);
  }
  // A voice is normally required; fall back to the global setting. Empty is only
  // valid for models that document a provider-side default.
  let voice = if model_voice.trim().is_empty() { fallback_voice } else { model_voice };
  let body = serde_json::json!({
    "model": resolved.model,
    "input": text,
    "voice": voice.trim(),
    "response_format": "mp3",
  });
  let url = provider::speech_url(&resolved);
  let client = crate::http::client();
  let resp = crate::http::send_model_request(&client, &resolved, &url, &body, crate::http::MODEL_ATTEMPTS).await?;
  let bytes = resp.bytes().await.map_err(|e| e.to_string())?;
  if bytes.is_empty() {
    return Err("the speech provider returned no audio".into());
  }
  Ok(Some(BASE64.encode(&bytes)))
}
