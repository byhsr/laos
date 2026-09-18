// Shared HTTP client for every provider/network call. Each request gets a connect
// timeout and, where the full response is awaited, a total timeout — so a slow or
// stalled upstream can't hang the UI indefinitely. OpenRouter in particular routes
// across many providers, and its default (price-driven) pick is often a slow one.
use std::time::Duration;

pub(crate) const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
pub(crate) const REQUEST_TIMEOUT: Duration = Duration::from_secs(180);
pub(crate) const STREAM_IDLE_TIMEOUT: Duration = Duration::from_secs(90);

pub(crate) const CHAT_MAX_TOKENS: u64 = 2048;
pub(crate) const SUMMARY_MAX_TOKENS: u64 = 400;

// How many times a model request is attempted before giving up.
pub(crate) const MODEL_ATTEMPTS: usize = 4;

// Sends a model request, retrying the statuses that mean "ask again shortly".
// Every provider call in the app goes through here, so a single 429 — which used
// to surface straight to the user as "Model provider returned 429" — is absorbed
// as long as the provider recovers within the retry budget.
pub(crate) async fn send_with_retry<F>(mut make: F, attempts: usize) -> Result<reqwest::Response, String>
where
  F: FnMut() -> reqwest::RequestBuilder,
{
  let mut last_status = String::new();
  for attempt in 0..attempts {
    let resp = make().send().await.map_err(|e| format!("Could not reach the model provider: {e}"))?;
    let status = resp.status();
    if status.is_success() { return Ok(resp); }
    last_status = status.to_string();
    let retryable = matches!(status.as_u16(), 429 | 502 | 503 | 504);
    if !retryable || attempt + 1 >= attempts { break; }
    let wait = retry_after(&resp).unwrap_or_else(|| backoff_delay(attempt));
    drop(resp);
    tokio::time::sleep(wait).await;
  }
  Err(format!("Model provider returned {last_status}"))
}

// Providers that throttle tell us exactly how long to wait; honour it.
fn retry_after(resp: &reqwest::Response) -> Option<Duration> {
  let raw = resp.headers().get(reqwest::header::RETRY_AFTER)?.to_str().ok()?;
  let secs = raw.trim().parse::<u64>().ok()?;
  Some(Duration::from_secs(secs.min(30)))
}

fn backoff_delay(attempt: usize) -> Duration {
  Duration::from_millis(800u64.saturating_mul(1 << attempt.min(4)))
}

// Client for requests whose full response is awaited.
pub(crate) fn client() -> reqwest::Client {
  reqwest::Client::builder()
    .connect_timeout(CONNECT_TIMEOUT)
    .timeout(REQUEST_TIMEOUT)
    .build()
    .unwrap_or_else(|_| reqwest::Client::new())
}

// Client for streaming requests. No total timeout (generations can legitimately
// run long); a stalled stream is caught per-chunk by the caller.
pub(crate) fn stream_client() -> reqwest::Client {
  reqwest::Client::builder()
    .connect_timeout(CONNECT_TIMEOUT)
    .build()
    .unwrap_or_else(|_| reqwest::Client::new())
}

// Adds the fields OpenAI-compatible providers need to behave: a bounded output
// length, and — for OpenRouter — a routing preference that avoids its default
// upstream selection in favour of the fastest responding provider.
pub(crate) fn apply_openai_defaults(body: &mut serde_json::Value, is_openrouter: bool, max_tokens: u64) {
  if let Some(obj) = body.as_object_mut() {
    obj.entry("max_tokens").or_insert(serde_json::json!(max_tokens));
    if is_openrouter {
      obj.entry("provider").or_insert(serde_json::json!({ "sort": "throughput" }));
    }
  }
}
