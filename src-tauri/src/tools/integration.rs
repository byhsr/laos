// Generic integration tool: executes a provider REST action using the stored
// integration credentials. Data-driven - new actions are added to the match.

use async_trait::async_trait;
use base64::Engine as _;

use super::{clip, AgentTool};
use crate::http;

// Reads an action's response, turning a non-2xx into an error instead of handing
// the model an error body it would read as success. A 401/404 from Notion used to
// come back looking like a normal result.
async fn read(resp: reqwest::Response) -> Result<String, String> {
  let status = resp.status();
  let text = resp.text().await.map_err(|e| e.to_string())?;
  if !status.is_success() {
    let body: String = text.trim().chars().take(500).collect();
    let body = if body.is_empty() { "(empty body)".to_string() } else { body };
    return Err(format!("The integration request failed ({status}): {body}"));
  }
  Ok(clip(&text))
}

// Generic integration tool: executes a provider REST action using the stored
// integration credentials. Data-driven — new actions are added to the provider match.
pub(crate) struct IntegrationTool {
  pub(crate) action: String,
  pub(crate) credentials: serde_json::Value,
  /// The integration id and app handle, so a Google action can refresh an expired
  /// token before it runs. `None` in contexts with no app handle.
  pub(crate) id: String,
  pub(crate) app: Option<tauri::AppHandle>,
}

#[async_trait]
impl AgentTool for IntegrationTool {
  fn name(&self) -> String { self.action.clone() }
  fn description(&self) -> String {
    match self.action.as_str() {
      "notion_search" => "Search Notion pages and databases. Params: query (string).".into(),
      "notion_create_page" => "Create a Notion page. Params: parentId (string — a page id or a database id), parentType (string — 'page_id' or 'database_id', default 'page_id'), title (string).".into(),
      "notion_get_page" => "Fetch a Notion page's content. Params: pageId (string).".into(),
      "airtable_list_records" => "List records from an Airtable table. Params: baseId (string), tableName (string), maxRecords (integer, optional).".into(),
      "airtable_create_record" => "Create a record in an Airtable table. Params: baseId (string), tableName (string), fields (object of field values).".into(),
      "airtable_update_record" => "Update a record in an Airtable table. Params: baseId (string), tableName (string), recordId (string), fields (object).".into(),
      "sheets_read" => "Read rows from a Google Sheet. Params: spreadsheetId (string), range (string, e.g. 'Sheet1!A1:C10').".into(),
      "sheets_append" => "Append rows to a Google Sheet. Params: spreadsheetId (string), range (string), values (array of arrays).".into(),
      "sheets_update" => "Update cells in a Google Sheet. Params: spreadsheetId (string), range (string), values (array of arrays).".into(),
      "docs_create" => "Create a Google Doc with content. Params: title (string), content (string).".into(),
      "docs_get" => "Read a Google Doc's content. Params: documentId (string).".into(),
      "exa_search" => "Search the web with Exa and get ranked results. Params: query (string), numResults (integer, optional).".into(),
      "parallel_search" => "Search the web with Parallel Web Systems and get ranked excerpts. Params: query (string).".into(),
      "tavily_search" => "Search the web with Tavily and get LLM-ready results. Params: query (string), maxResults (integer, optional).".into(),
      "tinyfish_search" => "Search the live web with TinyFish — ranked results with titles, snippets and URLs. Params: query (string).".into(),
      "calendar_list_events" => "List upcoming events on a Google Calendar. Params: calendarId (string, optional — default 'primary'), q (string, optional search), maxResults (integer, optional).".into(),
      "calendar_create_event" => "Create a Google Calendar event. Params: calendarId (optional), title (string), start (RFC3339 string), end (RFC3339 string), content (string, optional description).".into(),
      "gmail_send" => "Send an email through Gmail. Params: to (string), title (subject), content (body).".into(),
      "gmail_list" => "List Gmail messages. Params: q (string, Gmail search query), maxResults (integer, optional).".into(),
      _ => "Integration action".into(),
    }
  }
  fn params_schema(&self) -> serde_json::Value {
    serde_json::json!({ "type": "object", "properties": { "query": { "type": "string" }, "numResults": { "type": "integer" }, "maxResults": { "type": "integer" }, "parentId": { "type": "string" }, "parentType": { "type": "string" }, "title": { "type": "string" }, "pageId": { "type": "string" }, "baseId": { "type": "string" }, "tableName": { "type": "string" }, "maxRecords": { "type": "integer" }, "fields": { "type": "object" }, "recordId": { "type": "string" }, "spreadsheetId": { "type": "string" }, "range": { "type": "string" }, "values": { "type": "array" }, "documentId": { "type": "string" }, "content": { "type": "string" }, "to": { "type": "string" }, "start": { "type": "string" }, "end": { "type": "string" }, "q": { "type": "string" }, "calendarId": { "type": "string" } }, "required": [] })
  }
  async fn run(&self, args: &serde_json::Value) -> Result<String, String> {
    let snapshot = self.credentials.get("token").and_then(|t| t.as_str())
      .or_else(|| self.credentials.get("apiKey").and_then(|t| t.as_str()))
      .or_else(|| self.credentials.get("accessToken").and_then(|t| t.as_str()))
      .unwrap_or("")
      .to_string();
    let client = http::client();
    let arg = |k: &str| args.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string();
    // A Google access token dies after ~1h. Refresh from the stored refresh token
    // before a Google call, so a long-lived connection never fails on expiry.
    let token = if self.action.starts_with("sheets_") || self.action.starts_with("docs_")
      || self.action.starts_with("calendar_") || self.action.starts_with("gmail_") {
      match &self.app {
        Some(app) => crate::integrations::google_token(app, &self.id).await.unwrap_or_else(|_| snapshot.clone()),
        None => snapshot.clone(),
      }
    } else { snapshot.clone() };

    match self.action.as_str() {
      "notion_search" => {
        let body = serde_json::json!({ "query": arg("query") });
        let resp = client.post("https://api.notion.com/v1/search")
          .header("Authorization", format!("Bearer {token}"))
          .header("Notion-Version", "2022-06-28")
          .json(&body).send().await.map_err(|e| e.to_string())?;
        read(resp).await
      }
      "notion_create_page" => {
        // A page can sit under another page or under a database, and Notion needs
        // the parent typed correctly — a database id sent as page_id is rejected.
        let pt = { let t = arg("parentType"); if t.is_empty() { "page_id".to_string() } else { t } };
        let mut parent = serde_json::Map::new();
        parent.insert("type".into(), serde_json::Value::String(pt.clone()));
        parent.insert(pt, serde_json::Value::String(arg("parentId")));
        let body = serde_json::json!({
          "parent": serde_json::Value::Object(parent),
          "properties": { "title": { "title": [{ "text": { "content": arg("title") } }] } }
        });
        let resp = client.post("https://api.notion.com/v1/pages")
          .header("Authorization", format!("Bearer {token}"))
          .header("Notion-Version", "2022-06-28")
          .json(&body).send().await.map_err(|e| e.to_string())?;
        read(resp).await
      }
      "notion_get_page" => {
        // A page's *content* is its block children, not the page object.
        let resp = client.get(format!("https://api.notion.com/v1/blocks/{}/children", arg("pageId")))
          .header("Authorization", format!("Bearer {token}"))
          .header("Notion-Version", "2022-06-28")
          .send().await.map_err(|e| e.to_string())?;
        read(resp).await
      }
      "airtable_list_records" => {
        let resp = client.get(format!("https://api.airtable.com/v0/{}/{}/listRecords", arg("baseId"), arg("tableName")))
          .header("Authorization", format!("Bearer {token}"))
          .send().await.map_err(|e| e.to_string())?;
        read(resp).await
      }
      "airtable_create_record" => {
        let body = serde_json::json!({ "fields": args.get("fields").cloned().unwrap_or(serde_json::json!({})) });
        let resp = client.post(format!("https://api.airtable.com/v0/{}/{}", arg("baseId"), arg("tableName")))
          .header("Authorization", format!("Bearer {token}"))
          .json(&body).send().await.map_err(|e| e.to_string())?;
        read(resp).await
      }
      "airtable_update_record" => {
        let body = serde_json::json!({ "fields": args.get("fields").cloned().unwrap_or(serde_json::json!({})) });
        let resp = client.patch(format!("https://api.airtable.com/v0/{}/{}/{}", arg("baseId"), arg("tableName"), arg("recordId")))
          .header("Authorization", format!("Bearer {token}"))
          .json(&body).send().await.map_err(|e| e.to_string())?;
        read(resp).await
      }
      "sheets_read" => {
        let resp = client.get(format!("https://sheets.googleapis.com/v4/spreadsheets/{}/values/{}", arg("spreadsheetId"), arg("range")))
          .header("Authorization", format!("Bearer {token}"))
          .send().await.map_err(|e| e.to_string())?;
        read(resp).await
      }
      "sheets_append" => {
        let body = serde_json::json!({ "values": args.get("values").cloned().unwrap_or(serde_json::json!([])) });
        let resp = client.post(format!("https://sheets.googleapis.com/v4/spreadsheets/{}/values/{}:append?valueInputOption=RAW", arg("spreadsheetId"), arg("range")))
          .header("Authorization", format!("Bearer {token}"))
          .json(&body).send().await.map_err(|e| e.to_string())?;
        read(resp).await
      }
      "sheets_update" => {
        let body = serde_json::json!({ "values": args.get("values").cloned().unwrap_or(serde_json::json!([])) });
        let resp = client.put(format!("https://sheets.googleapis.com/v4/spreadsheets/{}/values/{}?valueInputOption=RAW", arg("spreadsheetId"), arg("range")))
          .header("Authorization", format!("Bearer {token}"))
          .json(&body).send().await.map_err(|e| e.to_string())?;
        read(resp).await
      }
      "docs_create" => {
        let body = serde_json::json!({ "title": arg("title") });
        let resp = client.post("https://docs.googleapis.com/v1/documents")
          .header("Authorization", format!("Bearer {token}"))
          .json(&body).send().await.map_err(|e| e.to_string())?;
        read(resp).await
      }
      "docs_get" => {
        let resp = client.get(format!("https://docs.googleapis.com/v1/documents/{}", arg("documentId")))
          .header("Authorization", format!("Bearer {token}"))
          .send().await.map_err(|e| e.to_string())?;
        read(resp).await
      }
      "exa_search" => {
        let num = args.get("numResults").and_then(|v| v.as_i64()).unwrap_or(8);
        let body = serde_json::json!({ "query": arg("query"), "type": "auto", "numResults": num, "contents": { "highlights": true } });
        let resp = client.post("https://api.exa.ai/search")
          .header("x-api-key", token)
          .json(&body).send().await.map_err(|e| e.to_string())?;
        read(resp).await
      }
      "parallel_search" => {
        let query = arg("query");
        let body = serde_json::json!({ "objective": query.clone(), "search_queries": [query], "mode": "basic" });
        let resp = client.post("https://api.parallel.ai/v1/search")
          .header("x-api-key", token)
          .json(&body).send().await.map_err(|e| e.to_string())?;
        read(resp).await
      }
      "tavily_search" => {
        let num = args.get("maxResults").and_then(|v| v.as_i64()).unwrap_or(5);
        let body = serde_json::json!({ "api_key": token, "query": arg("query"), "max_results": num, "search_depth": "basic" });
        let resp = client.post("https://api.tavily.com/search").json(&body).send().await.map_err(|e| e.to_string())?;
        read(resp).await
      }
      "tinyfish_search" => {
        // TinyFish Search: GET https://api.search.tinyfish.ai?query=… with X-API-Key.
        let resp = client.get("https://api.search.tinyfish.ai")
          .query(&[("query", arg("query"))])
          .header("X-API-Key", token)
          .send().await.map_err(|e| e.to_string())?;
        read(resp).await
      }
      "calendar_list_events" => {
        let cal = if arg("calendarId").is_empty() { "primary".to_string() } else { arg("calendarId") };
        let max = args.get("maxResults").and_then(|v| v.as_i64()).unwrap_or(10);
        let mut req = client.get(format!("https://www.googleapis.com/calendar/v3/calendars/{cal}/events"))
          .header("Authorization", format!("Bearer {token}"))
          .query(&[("maxResults", max.to_string()), ("singleEvents", "true".to_string()), ("orderBy", "startTime".to_string())]);
        let q = arg("q");
        if !q.is_empty() { req = req.query(&[("q", q)]); }
        let resp = req.send().await.map_err(|e| e.to_string())?;
        read(resp).await
      }
      "calendar_create_event" => {
        let cal = if arg("calendarId").is_empty() { "primary".to_string() } else { arg("calendarId") };
        let body = serde_json::json!({
          "summary": arg("title"),
          "description": arg("content"),
          "start": { "dateTime": arg("start") },
          "end": { "dateTime": arg("end") },
        });
        let resp = client.post(format!("https://www.googleapis.com/calendar/v3/calendars/{cal}/events"))
          .header("Authorization", format!("Bearer {token}"))
          .json(&body).send().await.map_err(|e| e.to_string())?;
        read(resp).await
      }
      "gmail_send" => {
        // Gmail wants the RFC822 message base64url-encoded in `raw`.
        let raw = format!(
          "To: {}\r\nSubject: {}\r\nContent-Type: text/plain; charset=\"UTF-8\"\r\n\r\n{}",
          arg("to"), arg("title"), arg("content"),
        );
        let encoded = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(raw.as_bytes());
        let body = serde_json::json!({ "raw": encoded });
        let resp = client.post("https://gmail.googleapis.com/gmail/v1/users/me/messages/send")
          .header("Authorization", format!("Bearer {token}"))
          .json(&body).send().await.map_err(|e| e.to_string())?;
        read(resp).await
      }
      "gmail_list" => {
        let max = args.get("maxResults").and_then(|v| v.as_i64()).unwrap_or(10);
        let mut req = client.get("https://gmail.googleapis.com/gmail/v1/users/me/messages")
          .header("Authorization", format!("Bearer {token}"))
          .query(&[("maxResults", max.to_string())]);
        let q = arg("q");
        if !q.is_empty() { req = req.query(&[("q", q)]); }
        let resp = req.send().await.map_err(|e| e.to_string())?;
        read(resp).await
      }
      _ => Err(format!("Unknown integration action: {}", self.action)),
    }
  }
}
