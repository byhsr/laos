// In-app updater: checks the signed release manifest, installs an update, and
// restarts. Implemented Rust-side so the frontend needs no updater JS package —
// the UI just calls these three commands and renders the prompt itself.

use serde::Serialize;
use tauri::AppHandle;
use tauri_plugin_updater::UpdaterExt;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
  pub version: String,
  pub current_version: String,
  pub notes: Option<String>,
  pub date: Option<String>,
}

// Returns the available update, or None when the running version is current.
#[tauri::command]
pub async fn check_for_update(app: AppHandle) -> Result<Option<UpdateInfo>, String> {
  let updater = app.updater().map_err(|e| e.to_string())?;
  match updater.check().await {
    Ok(Some(update)) => Ok(Some(UpdateInfo {
      version: update.version.clone(),
      current_version: update.current_version.clone(),
      notes: update.body.clone(),
      date: update.date.map(|d| d.to_string()),
    })),
    Ok(None) => Ok(None),
    Err(e) => Err(e.to_string()),
  }
}

// Downloads and installs the pending update. The caller restarts afterwards so
// the new binary takes effect.
#[tauri::command]
pub async fn install_update(app: AppHandle) -> Result<(), String> {
  let updater = app.updater().map_err(|e| e.to_string())?;
  let Some(update) = updater.check().await.map_err(|e| e.to_string())? else {
    return Err("No update available.".into());
  };
  update
    .download_and_install(|_, _| {}, || {})
    .await
    .map_err(|e| e.to_string())
}

// Restarts the app so a freshly installed update takes effect. Never returns.
#[tauri::command]
pub fn restart_app(app: AppHandle) {
  app.restart();
}
