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
