// Microphone permission.
//
// The desktop webview remembers a media permission per origin: once the mic has
// been denied (the user cancels the WebView2 prompt), the prompt never comes back
// and `getUserMedia` fails immediately. Clearing the stored state back to "ask"
// restores the prompt — the composer calls this when recording is refused.

/// Clears a remembered microphone block for the current origin so the webview
/// prompts again. Best-effort: a no-op off Windows or when the API is absent.
#[tauri::command]
pub async fn reset_microphone_permission(window: tauri::WebviewWindow) -> Result<(), String> {
  // The origin as `scheme://host[:port]`, the form the permission API expects.
  // Built from parts rather than `Url::origin()` — a `tauri://` origin is opaque
  // and would serialize as "null".
  #[cfg(windows)]
  let origin = {
    let url = window.url().map_err(|e| e.to_string())?;
    match (url.scheme(), url.host_str()) {
      (scheme, Some(host)) => match url.port() {
        Some(port) => format!("{scheme}://{host}:{port}"),
        None => format!("{scheme}://{host}"),
      },
      (scheme, None) => format!("{scheme}://"),
    }
  };

  #[cfg(windows)]
  window
    .with_webview(move |webview| unsafe {
      use webview2_com::Microsoft::Web::WebView2::Win32::*;
      use windows::core::{Interface, PCWSTR};

      let Ok(core) = webview.controller().CoreWebView2() else { return };
      let Ok(core) = core.cast::<ICoreWebView2_13>() else { return };
      let Ok(profile) = core.Profile() else { return };
      let Ok(profile) = profile.cast::<ICoreWebView2Profile4>() else { return };

      let wide: Vec<u16> = origin.encode_utf16().chain(std::iter::once(0)).collect();
      let _ = profile.SetPermissionState(
        COREWEBVIEW2_PERMISSION_KIND_MICROPHONE,
        PCWSTR(wide.as_ptr()),
        COREWEBVIEW2_PERMISSION_STATE_DEFAULT,
        None::<&ICoreWebView2SetPermissionStateCompletedHandler>,
      );
    })
    .map_err(|e| e.to_string())?;

  #[cfg(not(windows))]
  let _ = &window;

  Ok(())
}
