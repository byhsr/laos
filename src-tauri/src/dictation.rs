// Local speech-to-text for the chat composers. Audio is captured straight from
// the OS with cpal and transcribed by whisper.cpp, so nothing leaves the machine
// and the webview never needs microphone permissions.
//
// Models are downloaded on demand into <app_data_dir>/models and kept loaded in
// memory, because re-reading a 142 MB file per utterance is a visible stall.

use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc, Mutex, OnceLock};
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager};

use crate::http;

const MODEL_DIR: &str = "models";
// Whisper only accepts 16 kHz mono f32 PCM; everything captured is resampled here.
const TARGET_RATE: u32 = 16_000;
// Bounds memory for a forgotten recording session. The buffer holds native-rate
// mono samples before resampling, so 5 minutes at 48 kHz is roughly 57 MB.
const MAX_RECORD_SECONDS: u32 = 300;

pub struct ModelSpec {
  pub id: &'static str,
  pub label: &'static str,
  pub file: &'static str,
  pub bytes: u64,
  pub english_only: bool,
}

// Sizes are display hints and a sanity floor only — never compared for equality,
// since Hugging Face is free to re-quantise a model.
pub const MODELS: &[ModelSpec] = &[
  ModelSpec { id: "tiny.en", label: "Tiny — fastest, least accurate", file: "ggml-tiny.en.bin", bytes: 77_704_715, english_only: true },
  ModelSpec { id: "base.en", label: "Base — balanced (recommended)", file: "ggml-base.en.bin", bytes: 147_951_465, english_only: true },
  ModelSpec { id: "small.en", label: "Small — accurate, slower", file: "ggml-small.en.bin", bytes: 487_601_967, english_only: true },
  ModelSpec { id: "small", label: "Small — accurate, multilingual", file: "ggml-small.bin", bytes: 487_601_967, english_only: false },
];

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DictationModelInfo {
  pub id: String,
  pub label: String,
  pub size_bytes: u64,
  pub english_only: bool,
  pub downloaded: bool,
}

fn spec(id: &str) -> Result<&'static ModelSpec, String> {
  MODELS.iter().find(|m| m.id == id).ok_or_else(|| format!("Unknown speech model \"{id}\"."))
}

fn model_dir(app: &AppHandle) -> Result<PathBuf, String> {
  let dir = app.path().app_data_dir().map_err(|e| e.to_string())?.join(MODEL_DIR);
  std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
  Ok(dir)
}

fn model_path(app: &AppHandle, spec: &ModelSpec) -> Result<PathBuf, String> {
  Ok(model_dir(app)?.join(spec.file))
}

fn model_url(spec: &ModelSpec) -> String {
  format!("https://huggingface.co/ggerganov/whisper.cpp/resolve/main/{}", spec.file)
}

/// The models offered in Settings, flagged with whether they are on disk yet.
#[tauri::command]
pub fn dictation_models(app: AppHandle) -> Result<Vec<DictationModelInfo>, String> {
  let dir = model_dir(&app)?;
  Ok(
    MODELS
      .iter()
      .map(|m| DictationModelInfo {
        id: m.id.to_string(),
        label: m.label.to_string(),
        size_bytes: m.bytes,
        english_only: m.english_only,
        downloaded: dir.join(m.file).is_file(),
      })
      .collect(),
  )
}

/// Downloads a model, streaming byte progress to the caller.
///
/// Uses the streaming client deliberately: http::client() carries a 180 s total
/// timeout that would abort a 142 MB download on a slow connection.
#[tauri::command]
pub async fn dictation_download_model(app: AppHandle, model: String, on_progress: Channel<String>) -> Result<String, String> {
  use futures_util::StreamExt;

  let spec = spec(&model)?;
  let target = model_path(&app, spec)?;
  if target.is_file() {
    let _ = on_progress.send(progress_event(spec.bytes, spec.bytes));
    return Ok(target.to_string_lossy().to_string());
  }
  let part = model_dir(&app)?.join(format!("{}.part", spec.file));

  let resp = http::stream_client().get(model_url(spec)).send().await.map_err(|e| format!("Could not reach the model host: {e}"))?;
  if !resp.status().is_success() {
    return Err(format!("The model host returned {} for {}.", resp.status(), spec.file));
  }
  let total = resp.content_length().unwrap_or(spec.bytes);

  // A failed attempt must not leave a half file behind for the next run to trust.
  let cleanup = |path: &Path| {
    let _ = std::fs::remove_file(path);
  };

  let mut file = std::fs::File::create(&part).map_err(|e| e.to_string())?;
  let mut stream = resp.bytes_stream();
  let mut received: u64 = 0;
  let mut last_emitted: u64 = 0;
  while let Some(chunk) = stream.next().await {
    let chunk = match chunk {
      Ok(c) => c,
      Err(e) => {
        cleanup(&part);
        return Err(format!("The model download was interrupted: {e}"));
      }
    };
    if let Err(e) = std::io::Write::write_all(&mut file, &chunk) {
      cleanup(&part);
      return Err(format!("Could not write the model file: {e}"));
    }
    received += chunk.len() as u64;
    // Coarse enough to keep the channel quiet, fine enough to look live.
    if received - last_emitted >= 1_048_576 {
      last_emitted = received;
      let _ = on_progress.send(progress_event(received, total));
    }
  }
  drop(file);

  if received < spec.bytes / 2 {
    cleanup(&part);
    return Err("The model download was incomplete. Check your connection and try again.".into());
  }
  std::fs::rename(&part, &target).map_err(|e| format!("Could not store the model: {e}"))?;
  let _ = on_progress.send(progress_event(received, total));
  Ok(target.to_string_lossy().to_string())
}

#[tauri::command]
pub fn dictation_delete_model(app: AppHandle, model: String) -> Result<(), String> {
  let spec = spec(&model)?;
  let path = model_path(&app, spec)?;
  if path.is_file() {
    std::fs::remove_file(&path).map_err(|e| e.to_string())?;
  }
  let mut cache = context_store().lock().map_err(|_| "The speech model is busy.".to_string())?;
  if cache.as_ref().is_some_and(|(id, _)| id == spec.id) {
    *cache = None;
  }
  Ok(())
}

/// Whether the machine has a usable microphone, so the UI can say so up front.
#[tauri::command]
pub fn dictation_available() -> bool {
  use cpal::traits::{DeviceTrait, HostTrait};
  cpal::default_host()
    .default_input_device()
    .and_then(|d| d.default_input_config().ok())
    .is_some()
}

fn progress_event(received: u64, total: u64) -> String {
  serde_json::json!({ "type": "progress", "received": received, "total": total }).to_string()
}

struct Session {
  stop_tx: mpsc::Sender<()>,
  samples: Arc<Mutex<Vec<f32>>>,
  sample_rate: u32,
  model_id: String,
  model_path: PathBuf,
}

// cpal::Stream is not Send on every backend, so the stream itself lives on a
// dedicated thread and only Send handles are kept here — the same shape as the
// other process-wide statics in this crate.
static SESSION: OnceLock<Mutex<Option<Session>>> = OnceLock::new();

fn session_store() -> &'static Mutex<Option<Session>> {
  SESSION.get_or_init(|| Mutex::new(None))
}

static CONTEXT: OnceLock<Mutex<Option<(String, whisper_rs::WhisperContext)>>> = OnceLock::new();

fn context_store() -> &'static Mutex<Option<(String, whisper_rs::WhisperContext)>> {
  CONTEXT.get_or_init(|| Mutex::new(None))
}

/// Opens the default microphone and starts buffering audio until stopped.
#[tauri::command]
pub async fn dictation_start(app: AppHandle, model: String) -> Result<(), String> {
  use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

  let spec = spec(&model)?;
  let path = model_path(&app, spec)?;
  if !path.is_file() {
    return Err(format!("The \"{}\" speech model is not downloaded yet.", spec.label));
  }
  if session_store().lock().map_err(|_| "Dictation is busy.".to_string())?.is_some() {
    return Err("Already recording.".into());
  }

  let device = cpal::default_host().default_input_device().ok_or("No microphone was found on this machine.")?;
  let supported = device.default_input_config().map_err(|e| format!("No usable microphone input: {e}"))?;
  let sample_format = supported.sample_format();
  let stream_config: cpal::StreamConfig = supported.into();
  let sample_rate = stream_config.sample_rate;
  let channels = stream_config.channels;

  let samples = Arc::new(Mutex::new(Vec::<f32>::new()));
  let (stop_tx, stop_rx) = mpsc::channel::<()>();
  let (ready_tx, ready_rx) = mpsc::channel::<Result<(), String>>();

  let sink = samples.clone();
  std::thread::spawn(move || {
    let err_fn = |e| eprintln!("dictation: input stream error: {e}");
    let cap = sample_rate as usize * MAX_RECORD_SECONDS as usize;
    let built = match sample_format {
      cpal::SampleFormat::F32 => {
        let s = sink.clone();
        device.build_input_stream(stream_config, move |d: &[f32], _| push_mono(&s, d.iter().copied(), channels, cap), err_fn, None)
      }
      cpal::SampleFormat::I16 => {
        let s = sink.clone();
        device.build_input_stream(stream_config, move |d: &[i16], _| push_mono(&s, d.iter().map(|&v| v as f32 / 32768.0), channels, cap), err_fn, None)
      }
      cpal::SampleFormat::U16 => {
        let s = sink.clone();
        device.build_input_stream(stream_config, move |d: &[u16], _| push_mono(&s, d.iter().map(|&v| (v as f32 - 32768.0) / 32768.0), channels, cap), err_fn, None)
      }
      cpal::SampleFormat::I32 => {
        let s = sink.clone();
        device.build_input_stream(stream_config, move |d: &[i32], _| push_mono(&s, d.iter().map(|&v| v as f32 / 2_147_483_648.0), channels, cap), err_fn, None)
      }
      cpal::SampleFormat::F64 => {
        let s = sink.clone();
        device.build_input_stream(stream_config, move |d: &[f64], _| push_mono(&s, d.iter().map(|&v| v as f32), channels, cap), err_fn, None)
      }
      other => {
        let _ = ready_tx.send(Err(format!("Unsupported microphone sample format: {other:?}.")));
        return;
      }
    };

    match built {
      Ok(stream) => {
        if let Err(e) = stream.play() {
          let _ = ready_tx.send(Err(format!("Could not start the microphone: {e}")));
          return;
        }
        let _ = ready_tx.send(Ok(()));
        // Park until stop is requested, then drop the stream to close the device.
        let _ = stop_rx.recv();
        drop(stream);
      }
      Err(e) => {
        let _ = ready_tx.send(Err(format!("Could not open the microphone: {e}")));
      }
    }
  });

  match ready_rx.recv_timeout(std::time::Duration::from_secs(5)) {
    Ok(Ok(())) => {}
    Ok(Err(e)) => return Err(e),
    Err(_) => return Err("The microphone did not start in time.".into()),
  }

  let mut guard = session_store().lock().map_err(|_| "Dictation is busy.".to_string())?;
  *guard = Some(Session { stop_tx, samples, sample_rate, model_id: spec.id.to_string(), model_path: path });
  Ok(())
}

/// Downmixes to mono and appends to the buffer, stopping at the cap.
fn push_mono(sink: &Arc<Mutex<Vec<f32>>>, frames: impl Iterator<Item = f32>, channels: u16, cap: usize) {
  let ch = channels.max(1) as usize;
  let Ok(mut buf) = sink.lock() else { return };
  if buf.len() >= cap {
    return;
  }
  let mut sum = 0.0f32;
  let mut seen = 0usize;
  for value in frames {
    sum += value;
    seen += 1;
    if seen == ch {
      buf.push(sum / ch as f32);
      sum = 0.0;
      seen = 0;
    }
  }
}

/// Stops capture, transcribes what was recorded, and returns the text.
#[tauri::command]
pub async fn dictation_stop() -> Result<String, String> {
  let session = session_store().lock().map_err(|_| "Dictation is busy.".to_string())?.take();
  let Some(session) = session else {
    return Err("Not recording.".into());
  };
  let _ = session.stop_tx.send(());
  // Let the capture thread drop the stream so the final frames land in the buffer.
  tokio::time::sleep(std::time::Duration::from_millis(120)).await;

  let captured = {
    let mut buf = session.samples.lock().map_err(|_| "Dictation is busy.".to_string())?;
    std::mem::take(&mut *buf)
  };
  let pcm = resample_mono(&captured, session.sample_rate, TARGET_RATE);
  // Under a quarter second there is nothing to decode, and whisper hallucinates
  // on near-silence.
  if pcm.len() < TARGET_RATE as usize / 4 {
    return Ok(String::new());
  }

  let model_id = session.model_id.clone();
  let model_path = session.model_path.clone();
  tokio::task::spawn_blocking(move || transcribe(&model_id, &model_path, &pcm)).await.map_err(|e| e.to_string())?
}

/// Stops capture and throws the audio away.
#[tauri::command]
pub fn dictation_cancel() -> Result<(), String> {
  let session = session_store().lock().map_err(|_| "Dictation is busy.".to_string())?.take();
  if let Some(session) = session {
    let _ = session.stop_tx.send(());
  }
  Ok(())
}

/// Resamples mono audio to `to` Hz with a box pre-filter, which keeps the
/// decimation from folding high frequencies back into the speech band.
fn resample_mono(input: &[f32], from: u32, to: u32) -> Vec<f32> {
  if input.is_empty() || from == 0 {
    return Vec::new();
  }
  if from == to {
    return input.to_vec();
  }
  let step = from as f64 / to as f64;
  let window = step.round().max(1.0) as usize;
  let smoothed: Vec<f32> = if window > 1 {
    let mut out = Vec::with_capacity(input.len());
    let mut acc = 0.0f32;
    for (i, &sample) in input.iter().enumerate() {
      acc += sample;
      if i >= window {
        acc -= input[i - window];
      }
      out.push(acc / window.min(i + 1) as f32);
    }
    out
  } else {
    input.to_vec()
  };

  let ratio = to as f64 / from as f64;
  let out_len = ((smoothed.len() as f64) * ratio).round() as usize;
  let mut out = Vec::with_capacity(out_len);
  for i in 0..out_len {
    let src = i as f64 / ratio;
    let idx = src.floor() as usize;
    let frac = (src - idx as f64) as f32;
    let a = smoothed[idx.min(smoothed.len() - 1)];
    let b = smoothed[(idx + 1).min(smoothed.len() - 1)];
    out.push(a + (b - a) * frac);
  }
  out
}

fn transcribe(model_id: &str, model_path: &Path, pcm: &[f32]) -> Result<String, String> {
  use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

  let mut cache = context_store().lock().map_err(|_| "The speech model is busy.".to_string())?;
  let stale = cache.as_ref().map_or(true, |(id, _)| id != model_id);
  if stale {
    let path = model_path.to_str().ok_or("The speech model path is not valid UTF-8.")?;
    let ctx = WhisperContext::new_with_params(path, WhisperContextParameters::default())
      .map_err(|e| format!("Could not load the speech model: {e}"))?;
    *cache = Some((model_id.to_string(), ctx));
  }
  let ctx = &cache.as_ref().expect("context was just loaded").1;

  let mut state = ctx.create_state().map_err(|e| format!("Could not prepare the speech model: {e}"))?;
  let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
  params.set_translate(false);
  params.set_print_special(false);
  params.set_print_progress(false);
  params.set_print_realtime(false);
  params.set_print_timestamps(false);
  params.set_no_context(true);
  params.set_single_segment(false);
  if !MODELS.iter().any(|m| m.id == model_id && m.english_only) {
    // Auto-detect for the multilingual model; the .en models are English anyway.
    params.set_language(None);
  }
  let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
  params.set_n_threads(threads.min(8) as i32);

  state.full(params, pcm).map_err(|e| format!("Transcription failed: {e}"))?;

  let mut text = String::new();
  for segment in state.as_iter() {
    if let Ok(part) = segment.to_str_lossy() {
      text.push_str(&part);
    }
  }
  Ok(text.trim().to_string())
}
