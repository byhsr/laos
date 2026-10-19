// Embedded mini-browser: one child webview parented to the main window and
// painted over the left slide-out panel. It is a REAL browser (WebView2 /
// WebKit) sharing a persistent profile, so logins survive restarts and the user
// can click, type and sign in by hand while an agent drives it through the
// `browser_*` tools. Browser control replaced the old headless Playwright MCP
// server, which needed Node/npx and could not show the page.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use tauri::{
  AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, Webview, WebviewBuilder, WebviewUrl,
};
use tauri::webview::PageLoadEvent;

// How long a single eval() round-trip may take before the tool gives up.
const EVAL_TIMEOUT: Duration = Duration::from_secs(15);
// A short pause so a page that just navigated has painted before we read/click.
const SETTLE: Duration = Duration::from_millis(400);

struct BrowserState {
  app: AppHandle,
  webview: Option<Webview>,
  visible: bool,
  url: String,
  // Last panel rect in physical pixels, re-applied whenever the webview appears.
  bounds: Option<(i32, i32, u32, u32)>,
}

static STATE: OnceLock<Mutex<BrowserState>> = OnceLock::new();
static EVAL_WAITERS: OnceLock<Mutex<HashMap<String, tokio::sync::oneshot::Sender<serde_json::Value>>>> = OnceLock::new();
static EVAL_SEQ: AtomicU64 = AtomicU64::new(1);

fn state() -> &'static Mutex<BrowserState> {
  STATE.get().expect("browser::init must run in setup before any browser command")
}

fn with_state<T>(f: impl FnOnce(&mut BrowserState) -> T) -> Result<T, String> {
  let mut guard = state().lock().map_err(|_| "browser state is poisoned".to_string())?;
  Ok(f(&mut guard))
}

fn waiters() -> &'static Mutex<HashMap<String, tokio::sync::oneshot::Sender<serde_json::Value>>> {
  EVAL_WAITERS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn app_handle() -> Result<AppHandle, String> {
  with_state(|s| s.app.clone())
}

// Called once from `.setup` (main thread).
pub fn init(app: AppHandle) {
  let _ = STATE.set(Mutex::new(BrowserState { app, webview: None, visible: false, url: String::new(), bounds: None }));
}

// Tells the frontend whether the drawer should be open and what to show in the
// address bar. The UI listens on `lup-browser`.
fn emit_state(app: &AppHandle, visible: bool, url: &str, title: &str, loading: bool) {
  let _ = app.emit("lup-browser", serde_json::json!({
    "visible": visible, "url": url, "title": title, "loading": loading,
  }));
}

// Builds the child webview. Must run on the main thread (Tauri requirement for
// webview creation), so callers go through `ensure_webview`.
fn create_webview(app: &AppHandle, initial: Option<String>) -> Result<Webview, String> {
  let window = app.get_window("main").ok_or("The main window is not available.")?;
  let url = match initial.as_deref() {
    Some(u) => WebviewUrl::External(normalize(u)?),
    None => WebviewUrl::External("about:blank".parse().map_err(|_| "about:blank is not a URL".to_string())?),
  };
  // A persistent profile is what keeps logins/cookies across restarts.
  let profile = app.path().app_data_dir().map_err(|e| e.to_string())?.join("browser-profile");
  let _ = std::fs::create_dir_all(&profile);

  let nav_app = app.clone();
  let load_app = app.clone();
  let builder = WebviewBuilder::new("browser", url)
    .data_directory(profile)
    .on_navigation(move |u| {
      emit_state(&nav_app, true, u.as_str(), "", false);
      true
    })
    .on_page_load(move |_webview, payload| {
      emit_state(&load_app, true, payload.url().as_str(), "", matches!(payload.event(), PageLoadEvent::Started));
    });

  // Created off-screen and hidden; the frontend positions it once the drawer
  // measures its viewport.
  let webview = window
    .add_child(builder, tauri::LogicalPosition::new(-20000.0, -20000.0), tauri::LogicalSize::new(1.0, 1.0))
    .map_err(|e| format!("Could not open the mini browser: {e}"))?;
  let _ = webview.hide();
  Ok(webview)
}

// Returns the live child webview, creating it (on the main thread) on first use.
fn ensure_webview(app: &AppHandle, initial: Option<String>) -> Result<Webview, String> {
  if let Some(w) = with_state(|s| s.webview.clone())? { return Ok(w); }
  let (tx, rx) = std::sync::mpsc::channel();
  let handle = app.clone();
  app
    .run_on_main_thread(move || { let _ = tx.send(create_webview(&handle, initial)); })
    .map_err(|e| e.to_string())?;
  let webview = rx.recv().map_err(|e| e.to_string())??;
  with_state(|s| s.webview = Some(webview.clone()))?;
  Ok(webview)
}

// A URL missing a scheme defaults to https, so "example.com" works.
fn normalize(url: &str) -> Result<tauri::Url, String> {
  let trimmed = url.trim();
  if trimmed.is_empty() { return Err("A URL is required.".into()); }
  let full = if trimmed.contains("://") || trimmed.starts_with("about:") { trimmed.to_string() } else { format!("https://{trimmed}") };
  full.parse::<tauri::Url>().map_err(|_| format!("'{url}' is not a valid URL."))
}

fn apply_bounds(webview: &Webview, bounds: Option<(i32, i32, u32, u32)>) {
  if let Some((x, y, w, h)) = bounds {
    let _ = webview.set_position(PhysicalPosition::new(x, y));
    let _ = webview.set_size(PhysicalSize::new(w.max(1), h.max(1)));
  }
}

// Makes the webview visible at the last known panel rect and tells the UI to
// slide the drawer open. Every browser tool calls this first.
pub(crate) fn ensure_visible(app: &AppHandle) -> Result<(), String> {
  let webview = ensure_webview(app, None)?;
  apply_bounds(&webview, with_state(|s| s.bounds)?);
  let _ = webview.show();
  let url = with_state(|s| { s.visible = true; s.url.clone() })?;
  emit_state(app, true, &url, "", false);
  Ok(())
}

// Navigates and shows the panel. Used by tools and the address bar.
pub(crate) fn navigate_to(app: &AppHandle, url: &str) -> Result<String, String> {
  let target = normalize(url)?;
  let webview = ensure_webview(app, Some(url.to_string()))?;
  ensure_visible(app)?;
  webview.navigate(target.clone()).map_err(|e| e.to_string())?;
  let shown = target.to_string();
  with_state(|s| s.url = shown.clone())?;
  emit_state(app, true, &shown, "", true);
  Ok(shown)
}

// ---------------------------------------------------------------------------
// JS evaluation (result round-trip)
// ---------------------------------------------------------------------------

// The page runs BODY inside an IIFE and hands its JSON-encoded result back over
// the IPC to `browser_eval_result`, which resolves the awaiting oneshot.
const EVAL_TEMPLATE: &str = "(function(){try{var v=(function(){ __BODY__ })();var r=JSON.stringify({ok:true,value:v===undefined?null:v});}catch(e){var r=JSON.stringify({ok:false,error:String((e&&e.message)||e)});}window.__TAURI_INTERNALS__.invoke('browser_eval_result',{reqId:'__ID__',result:r});})();";

// Runs BODY (JS statements ending in `return`) in the page and awaits its value.
async fn eval_body(body: &str) -> Result<serde_json::Value, String> {
  let webview = with_state(|s| s.webview.clone())?.ok_or("The mini browser is not open.")?;
  let id = format!("ev-{}", EVAL_SEQ.fetch_add(1, Ordering::SeqCst));
  let (tx, rx) = tokio::sync::oneshot::channel();
  waiters().lock().map_err(|_| "browser eval registry is poisoned".to_string())?.insert(id.clone(), tx);
  let script = EVAL_TEMPLATE.replace("__ID__", &id).replace("__BODY__", body);
  let outcome = match webview.eval(&script).map_err(|e| e.to_string()) {
    Err(e) => Err(e),
    Ok(()) => match tokio::time::timeout(EVAL_TIMEOUT, rx).await {
      Ok(Ok(v)) => Ok(v),
      Ok(Err(_)) => Err("The page closed while the browser was working.".to_string()),
      Err(_) => Err("The page did not respond in time.".to_string()),
    },
  };
  // Whoever won, drop the waiter so the map can't grow unbounded.
  let _ = waiters().lock().map(|mut m| m.remove(&id));
  let value = outcome?;
  if value.get("ok").and_then(|v| v.as_bool()).unwrap_or(false) {
    Ok(value.get("value").cloned().unwrap_or(serde_json::Value::Null))
  } else {
    Err(value.get("error").and_then(|e| e.as_str()).unwrap_or("The page script failed.").to_string())
  }
}

// A JS string literal from arbitrary text (JSON strings are valid JS strings).
fn js_str(s: &str) -> String { serde_json::to_string(s).unwrap_or_else(|_| "\"\"".to_string()) }

async fn settle() { tokio::time::sleep(SETTLE).await; }

// ---------------------------------------------------------------------------
// Actions shared by the tools
// ---------------------------------------------------------------------------

pub(crate) async fn tool_open(url: &str) -> Result<String, String> {
  let app = app_handle()?;
  let shown = navigate_to(&app, url)?;
  Ok(format!("Opened {shown} in the mini browser."))
}

pub(crate) async fn tool_read() -> Result<String, String> {
  let app = app_handle()?;
  ensure_visible(&app)?;
  settle().await;
  let value = eval_body("return { url: location.href, title: document.title, text: (document.body ? document.body.innerText : '') };").await?;
  let url = value.get("url").and_then(|v| v.as_str()).unwrap_or("");
  let title = value.get("title").and_then(|v| v.as_str()).unwrap_or("");
  let text = value.get("text").and_then(|v| v.as_str()).unwrap_or("");
  let body = format!("URL: {url}\nTitle: {title}\n\n{}", text.trim());
  Ok(crate::tools::clip(&body))
}

pub(crate) async fn tool_back() -> Result<String, String> {
  let app = app_handle()?;
  ensure_visible(&app)?;
  eval_body("history.back(); return true;").await?;
  settle().await;
  Ok("Went back.".into())
}

pub(crate) async fn tool_forward() -> Result<String, String> {
  let app = app_handle()?;
  ensure_visible(&app)?;
  eval_body("history.forward(); return true;").await?;
  settle().await;
  Ok("Went forward.".into())
}

pub(crate) async fn tool_reload() -> Result<String, String> {
  let app = app_handle()?;
  ensure_visible(&app)?;
  eval_body("location.reload(); return true;").await?;
  settle().await;
  Ok("Reloaded the page.".into())
}

pub(crate) async fn tool_click(selector: Option<&str>, text: Option<&str>) -> Result<String, String> {
  let app = app_handle()?;
  ensure_visible(&app)?;
  settle().await;
  let sel = selector.map(|s| format!("document.querySelector({})", js_str(s))).unwrap_or_else(|| "null".into());
  let wanted = text.map(js_str).unwrap_or_else(|| "''".into());
  let body = format!(
    "var sel = {sel}; var wanted = {wanted}; var el = sel; \
     if (!el && wanted) {{ var cands = document.querySelectorAll('a,button,[role=button],input[type=submit],input[type=button],summary,label'); \
     for (var i=0;i<cands.length;i++) {{ var e=cands[i]; var t=(e.innerText||e.value||e.getAttribute('aria-label')||'').trim().toLowerCase(); \
     if (t && t.indexOf(wanted.trim().toLowerCase())!==-1) {{ el=e; break; }} }} }} \
     if (!el) return {{ clicked:false }}; \
     el.scrollIntoView({{block:'center'}}); el.click(); \
     return {{ clicked:true, tag: el.tagName, text: (el.innerText||el.value||'').slice(0,80) }};"
  );
  let value = eval_body(&body).await?;
  if value.get("clicked").and_then(|v| v.as_bool()).unwrap_or(false) {
    let tag = value.get("tag").and_then(|v| v.as_str()).unwrap_or("element");
    let label = value.get("text").and_then(|v| v.as_str()).unwrap_or("");
    Ok(if label.is_empty() { format!("Clicked <{tag}>.") } else { format!("Clicked <{tag}> \"{label}\".") })
  } else {
    Err("No element matched that selector/text.".into())
  }
}

pub(crate) async fn tool_type(selector: Option<&str>, label: Option<&str>, text: &str, submit: bool) -> Result<String, String> {
  let app = app_handle()?;
  ensure_visible(&app)?;
  settle().await;
  let sel = selector.map(|s| format!("document.querySelector({})", js_str(s))).unwrap_or_else(|| "null".into());
  let wanted = label.map(js_str).unwrap_or_else(|| "''".into());
  let body = format!(
    "var sel = {sel}; var wanted = {wanted}; var el = sel; \
     if (!el && wanted) {{ var f = document.querySelectorAll('input,textarea'); \
     for (var i=0;i<f.length;i++) {{ var e=f[i]; var t=((e.placeholder||'')+' '+(e.getAttribute('aria-label')||'')+' '+(e.name||'')+' '+(e.id||'')).toLowerCase(); \
     if (t.indexOf(wanted.trim().toLowerCase())!==-1) {{ el=e; break; }} }} }} \
     if (!el) return {{ typed:false }}; \
     el.focus(); \
     var proto = (el.tagName==='TEXTAREA') ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype; \
     var setter = Object.getOwnPropertyDescriptor(proto,'value').set; \
     if (setter) {{ setter.call(el, {text}); }} else {{ el.value = {text}; }} \
     el.dispatchEvent(new Event('input',{{bubbles:true}})); el.dispatchEvent(new Event('change',{{bubbles:true}})); \
     if ({submit}) {{ if (el.form && el.form.requestSubmit) {{ el.form.requestSubmit(); }} else {{ el.dispatchEvent(new KeyboardEvent('keydown',{{key:'Enter',keyCode:13,which:13,bubbles:true}})); }} }} \
     return {{ typed:true, tag: el.tagName }};",
    text = js_str(text),
    submit = submit,
  );
  let value = eval_body(&body).await?;
  if value.get("typed").and_then(|v| v.as_bool()).unwrap_or(false) {
    Ok(format!("Typed into the field{}.", if submit { " and submitted" } else { "" }))
  } else {
    Err("No input matched that selector/label.".into())
  }
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

// Opens (creating if needed) and shows the mini browser, optionally navigating.
#[tauri::command]
pub async fn browser_open(app: AppHandle, url: Option<String>) -> Result<(), String> {
  match url.as_deref().map(str::trim).filter(|u| !u.is_empty()) {
    Some(u) => { navigate_to(&app, u)?; }
    None => ensure_visible(&app)?,
  }
  Ok(())
}

#[tauri::command]
pub async fn browser_navigate(app: AppHandle, url: String) -> Result<(), String> {
  navigate_to(&app, &url)?;
  Ok(())
}

#[tauri::command]
pub async fn browser_back(_app: AppHandle) -> Result<(), String> {
  tool_back().await.map(|_| ())
}

#[tauri::command]
pub async fn browser_forward(_app: AppHandle) -> Result<(), String> {
  tool_forward().await.map(|_| ())
}

#[tauri::command]
pub async fn browser_reload(_app: AppHandle) -> Result<(), String> {
  tool_reload().await.map(|_| ())
}

// Hides the panel but keeps the page (and its session) alive.
#[tauri::command]
pub fn browser_close(app: AppHandle) -> Result<(), String> {
  let webview = with_state(|s| { s.visible = false; s.webview.clone() })?;
  if let Some(w) = webview { let _ = w.hide(); }
  emit_state(&app, false, "", "", false);
  Ok(())
}

// Fully tears the webview down — closes the persistent session's page.
#[tauri::command]
pub fn browser_destroy(app: AppHandle) -> Result<(), String> {
  let webview = with_state(|s| { s.visible = false; s.webview.take() })?;
  if let Some(w) = webview { let _ = w.close(); }
  emit_state(&app, false, "", "", false);
  Ok(())
}

// Positions the webview over the drawer's viewport hole. Coordinates are
// physical pixels (the frontend multiplies CSS px by devicePixelRatio).
#[tauri::command]
pub fn browser_set_bounds(x: i32, y: i32, width: u32, height: u32) -> Result<(), String> {
  let webview = with_state(|s| { s.bounds = Some((x, y, width, height)); s.webview.clone() })?;
  if let Some(w) = webview { apply_bounds(&w, Some((x, y, width, height))); }
  Ok(())
}

#[tauri::command]
pub fn browser_state() -> Result<serde_json::Value, String> {
  with_state(|s| serde_json::json!({ "open": s.webview.is_some(), "visible": s.visible, "url": s.url }))
}

// Keeps the in-app browser's pages in step with the app theme: the window's
// colour scheme is what WebView2 maps to `prefers-color-scheme`, so a light app
// renders light pages.
#[tauri::command]
pub fn browser_set_theme(app: AppHandle, theme: String) -> Result<(), String> {
  let t = if theme == "light" { tauri::Theme::Light } else { tauri::Theme::Dark };
  if let Some(w) = app.get_window("main") { let _ = w.set_theme(Some(t)); }
  Ok(())
}

// Resolves the oneshot a pending `eval_body` is awaiting. Called from the page.
#[tauri::command]
pub fn browser_eval_result(req_id: String, result: String) -> Result<(), String> {
  let value: serde_json::Value = serde_json::from_str(&result).unwrap_or_else(|_| serde_json::json!({ "ok": false, "error": "invalid eval result" }));
  if let Some(tx) = waiters().lock().map_err(|_| "browser eval registry is poisoned".to_string())?.remove(&req_id) {
    let _ = tx.send(value);
  }
  Ok(())
}

// Closes the webview on app exit so no page is left running.
pub fn shutdown() {
  let webview = with_state(|s| { s.visible = false; s.webview.take() }).unwrap_or(None);
  if let Some(w) = webview { let _ = w.close(); }
}
