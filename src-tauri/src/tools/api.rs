// Configurable REST API tool: method + URL template with {param} placeholders,
// headers, optional JSON body, and a manual parameter list.
//
// A tool configured as a *suite* (a base URL plus a list of actions) becomes one
// of these per action, sharing the suite's base URL, headers and auth — which is
// how a user-defined tool stands in for what used to need a native integration.

use async_trait::async_trait;

use super::{clip, AgentTool};
use crate::http;

// Where a parameter goes in the request. `Template` is the legacy behaviour: a
// raw `{name}` substitution wherever the placeholder appears (URL, header values,
// body) — kept as the default so tools written before this existed are untouched.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ParamIn { Template, Path, Query, Body, Header }

impl ParamIn {
  pub(crate) fn parse(s: &str) -> Self {
    match s {
      "path" => Self::Path,
      "query" => Self::Query,
      "body" => Self::Body,
      "header" => Self::Header,
      _ => Self::Template,
    }
  }
}

pub(crate) struct ApiParam {
  pub(crate) name: String,
  pub(crate) param_type: String,
  pub(crate) description: String,
  pub(crate) required: bool,
  pub(crate) in_: ParamIn,
}

// How the tool authenticates. The secret itself is resolved from the credential
// store before the tool is built; nothing here is ever plaintext on disk.
#[derive(Clone)]
pub(crate) enum ApiAuth {
  None,
  /// `Authorization: Bearer <secret>`.
  Bearer(String),
  /// A named header carrying the secret, e.g. `x-api-key`.
  Header(String, String),
  /// The secret appended as a query pair, e.g. `?api_key=<secret>`.
  Query(String, String),
  /// Configured, but the credential store no longer holds it. Fails the call
  /// loudly rather than quietly sending an unauthenticated request.
  Missing(String),
}

// When the tool asks the user first. Only consulted while autonomy is off, so
// the global switch still overrides everything.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Approval { Never, Writes, Always }

impl Approval {
  pub(crate) fn parse(s: &str) -> Self {
    match s {
      "never" => Self::Never,
      "always" => Self::Always,
      // Reads are safe to run freely; the default only gates the methods that change things.
      _ => Self::Writes,
    }
  }
}

pub(crate) struct ApiTool {
  pub(crate) name: String,
  pub(crate) description: String,
  pub(crate) method: String,
  pub(crate) url_template: String,
  pub(crate) headers: Vec<(String, String)>,
  pub(crate) body_template: Option<String>,
  pub(crate) params: Vec<ApiParam>,
  pub(crate) auth: ApiAuth,
  pub(crate) approval: Approval,
}

// Percent-encodes a value for use inside a URL path segment: everything outside
// the unreserved set is escaped, so a value holding a space, `/`, `&` or `#`
// cannot break out of its segment or re-route the request.
fn encode_segment(s: &str) -> String {
  let mut out = String::with_capacity(s.len());
  for b in s.bytes() {
    match b {
      b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => out.push(b as char),
      _ => out.push_str(&format!("%{b:02X}")),
    }
  }
  out
}

// Replaces {name} in `template` with the arg's value. `Body` params keep their
// JSON type — either the author writes `"count": {n}` (unquoted) or `"name": "{name}"`
// (quoted) and both land as valid JSON, so nobody has to hand-assemble quotes.
fn fill_template(template: &str, args: &serde_json::Value, params: &[ApiParam]) -> Result<String, String> {
  let mut out = template.to_string();
  let Some(obj) = args.as_object() else { return Err("args must be an object".into()) };
  for (key, value) in obj {
    let placeholder = format!("{{{key}}}");
    let is_body = params.iter().any(|p| p.name == *key && p.in_ == ParamIn::Body);
    if is_body {
      // Quoted form: the JSON encoding brings its own quotes, so the template's
      // pair is consumed with the placeholder.
      let quoted = format!("\"{placeholder}\"");
      let json = serde_json::to_string(value).unwrap_or_else(|_| value.to_string());
      if out.contains(&quoted) {
        out = out.replace(&quoted, &json);
      } else if out.contains(&placeholder) {
        out = out.replace(&placeholder, &json);
      }
      continue;
    }
    if !out.contains(&placeholder) { continue; }
    let text = match value {
      serde_json::Value::String(s) => s.clone(),
      other => other.to_string(),
    };
    out = out.replace(&placeholder, &text);
  }
  // A leftover placeholder means a declared argument never arrived. Checking the
  // parameter names rather than "any brace at all" leaves a JSON body's structural
  // braces alone — the old check rejected every JSON body ever configured.
  for p in params {
    if out.contains(&format!("{{{}}}", p.name)) {
      return Err(format!("Missing value for the '{}' parameter.", p.name));
    }
  }
  Ok(out)
}

impl ApiTool {
  // Assembles the request URL: path values escaped into their segments, query
  // values appended so they are encoded, query-borne auth added. Split out from
  // `run` because this is the part that used to paste a value straight into the
  // URL — a space, `&` or `#` in an argument silently misrouted the request.
  fn resolve_url(&self, args: &serde_json::Value) -> Result<reqwest::Url, String> {
    let arg = |name: &str| args.get(name).and_then(|v| match v {
      serde_json::Value::Null => None,
      serde_json::Value::String(s) => Some(s.clone()),
      other => Some(other.to_string()),
    });
    let mut url = self.url_template.clone();
    for p in &self.params {
      if p.in_ != ParamIn::Path { continue; }
      let Some(text) = arg(&p.name) else { continue };
      url = url.replace(&format!("{{{}}}", p.name), &encode_segment(&text));
    }
    // A query value the author wrote straight into the URL is substituted raw, the
    // original behaviour; otherwise it is appended below, properly encoded.
    for p in &self.params {
      if p.in_ != ParamIn::Query { continue; }
      let Some(text) = arg(&p.name) else { continue };
      url = url.replace(&format!("{{{}}}", p.name), &text);
    }
    let url = fill_template(&url, args, &self.params)?;

    let mut parsed = reqwest::Url::parse(&url).map_err(|e| format!("Invalid URL {url}: {e}"))?;
    for p in &self.params {
      if p.in_ != ParamIn::Query { continue; }
      let Some(text) = arg(&p.name) else { continue };
      if self.url_template.contains(&format!("{{{}}}", p.name)) { continue; }
      parsed.query_pairs_mut().append_pair(&p.name, &text);
    }
    // Auth that rides in the query string is added here, before the request is
    // built from this URL, or it would never reach the wire.
    if let ApiAuth::Query(name, secret) = &self.auth {
      parsed.query_pairs_mut().append_pair(name, secret);
    }
    Ok(parsed)
  }
}

#[async_trait]
impl AgentTool for ApiTool {
  fn name(&self) -> String { self.name.clone() }
  fn description(&self) -> String { self.description.clone() }
  fn params_schema(&self) -> serde_json::Value {
    let mut properties = serde_json::Map::new();
    let mut required = Vec::new();
    for p in &self.params {
      properties.insert(p.name.clone(), serde_json::json!({
        "type": p.param_type,
        "description": p.description,
      }));
      if p.required { required.push(p.name.clone()); }
    }
    serde_json::json!({ "type": "object", "properties": properties, "required": required })
  }
  // A custom tool is network-bound, so it may be backgrounded — but only when it
  // wouldn't need to ask first (see `should_background`).
  fn is_long_running(&self) -> bool { true }
  fn requires_approval(&self) -> bool {
    match self.approval {
      Approval::Never => false,
      Approval::Always => true,
      Approval::Writes => matches!(self.method.as_str(), "POST" | "PUT" | "PATCH" | "DELETE"),
    }
  }
  async fn run(&self, args: &serde_json::Value) -> Result<String, String> {
    if let ApiAuth::Missing(what) = &self.auth {
      return Err(format!("No credential is stored for {what} — open the tool in the workshop and re-enter it."));
    }
    let parsed = self.resolve_url(args)?;
    let mut req = http::client().request(
      reqwest::Method::from_bytes(self.method.as_bytes()).map_err(|e| format!("Invalid HTTP method {}: {e}", self.method))?,
      parsed.clone(),
    );
    match &self.auth {
      ApiAuth::Bearer(secret) => { req = req.header("Authorization", format!("Bearer {secret}")); }
      ApiAuth::Header(name, secret) => { req = req.header(name, secret); }
      _ => {}
    }
    for (k, v) in &self.headers {
      let value = fill_template(v, args, &self.params)?;
      req = req.header(k, value);
    }
    if let Some(body) = &self.body_template {
      let filled = fill_template(body, args, &self.params)?;
      req = req.header("Content-Type", "application/json").body(filled);
    }
    let response = req.send().await.map_err(|e| format!("Request to {parsed} failed: {e}"))?;
    let status = response.status();
    let text = response.text().await.map_err(|e| e.to_string())?;
    // Prefer pretty-printed JSON so the model can read the structure.
    let body = if let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) {
      serde_json::to_string_pretty(&v).unwrap_or(text)
    } else { text };
    if !status.is_success() {
      return Err(format!("API returned {status}: {body}"));
    }
    Ok(clip(&body))
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn param(name: &str, in_: ParamIn) -> ApiParam {
    ApiParam { name: name.into(), param_type: "string".into(), description: String::new(), required: false, in_ }
  }

  fn tool(method: &str, approval: Approval) -> ApiTool {
    ApiTool {
      name: "t".into(), description: String::new(), method: method.into(),
      url_template: String::new(), headers: Vec::new(), body_template: None,
      params: Vec::new(), auth: ApiAuth::None, approval,
    }
  }

  #[test]
  fn path_values_are_escaped() {
    assert_eq!(encode_segment("a b/c?d&e#f"), "a%20b%2Fc%3Fd%26e%23f");
    assert_eq!(encode_segment("safe-_.~ok"), "safe-_.~ok");
  }

  #[test]
  fn body_params_keep_their_json_type() {
    let params = vec![param("q", ParamIn::Body), param("n", ParamIn::Body)];
    let args = serde_json::json!({ "q": "hello \"world\"", "n": 3 });
    // Unquoted in the template: the JSON value is inserted verbatim.
    let out = fill_template(r#"{"count": {n}}"#, &args, &params).unwrap();
    assert_eq!(out, r#"{"count": 3}"#);
    // Quoted by the author: the quotes belong to the value, and it is escaped.
    let out = fill_template(r#"{"query": "{q}"}"#, &args, &params).unwrap();
    assert_eq!(out, r#"{"query": "hello \"world\""}"#);
    assert!(serde_json::from_str::<serde_json::Value>(&out).is_ok());
  }

  #[test]
  fn legacy_template_params_still_substitute_raw() {
    let params = vec![param("id", ParamIn::Template)];
    let args = serde_json::json!({ "id": "42" });
    assert_eq!(fill_template("https://x.dev/{id}", &args, &params).unwrap(), "https://x.dev/42");
    // A declared parameter that was never supplied is still an error.
    assert!(fill_template("https://x.dev/{id}", &serde_json::json!({}), &params).is_err());
  }

  #[test]
  fn a_json_body_is_not_mistaken_for_placeholders() {
    // Structural braces must survive: only declared params are placeholders.
    let params = vec![param("id", ParamIn::Template)];
    let args = serde_json::json!({ "id": "42" });
    let out = fill_template(r#"{"a": {"b": "{id}"}}"#, &args, &params).unwrap();
    assert_eq!(out, r#"{"a": {"b": "42"}}"#);
  }

  fn with(method: &str, url: &str, params: Vec<ApiParam>, auth: ApiAuth) -> ApiTool {
    ApiTool {
      name: "t".into(), description: String::new(), method: method.into(), url_template: url.into(),
      headers: Vec::new(), body_template: None, params, auth, approval: Approval::Writes,
    }
  }

  #[test]
  fn query_values_are_encoded_not_pasted_into_the_url() {
    let t = with("GET", "https://x.dev/search", vec![param("q", ParamIn::Query)], ApiAuth::None);
    let url = t.resolve_url(&serde_json::json!({ "q": "a b&c=d#e" })).unwrap();
    assert_eq!(url.query(), Some("q=a+b%26c%3Dd%23e"));
  }

  #[test]
  fn path_values_stay_inside_their_segment() {
    let t = with("GET", "https://x.dev/items/{id}", vec![param("id", ParamIn::Path)], ApiAuth::None);
    let url = t.resolve_url(&serde_json::json!({ "id": "a/b c" })).unwrap();
    assert_eq!(url.path(), "/items/a%2Fb%20c");
  }

  #[test]
  fn query_auth_is_appended_alongside_the_existing_query() {
    let t = with("GET", "https://x.dev/p?keep=1", Vec::new(), ApiAuth::Query("api_key".into(), "s3cret".into()));
    let url = t.resolve_url(&serde_json::json!({})).unwrap();
    assert_eq!(url.query(), Some("keep=1&api_key=s3cret"));
  }

  #[test]
  fn a_query_value_written_into_the_url_is_not_appended_twice() {
    let t = with("GET", "https://x.dev/s?q={q}", vec![param("q", ParamIn::Query)], ApiAuth::None);
    let url = t.resolve_url(&serde_json::json!({ "q": "x" })).unwrap();
    assert_eq!(url.as_str(), "https://x.dev/s?q=x");
  }

  #[test]
  fn approval_follows_the_method() {
    assert!(!tool("GET", Approval::Writes).requires_approval());
    assert!(tool("POST", Approval::Writes).requires_approval());
    assert!(tool("DELETE", Approval::Writes).requires_approval());
    assert!(tool("GET", Approval::Always).requires_approval());
    assert!(!tool("POST", Approval::Never).requires_approval());
  }
}
