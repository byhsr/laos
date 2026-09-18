// Generic integration tool: executes a provider REST action using the stored
// integration credentials. Data-driven - new actions are added to the match.

use async_trait::async_trait;

use super::{clip, AgentTool};
use crate::http;

// Generic integration tool: executes a provider REST action using the stored
// integration credentials. Data-driven â€” new actions are added to the provider match.
pub(crate) struct IntegrationTool {
  pub(crate) action: String,
  pub(crate) credentials: serde_json::Value,
}

#[async_trait]
impl AgentTool for IntegrationTool {
  fn name(&self) -> String { self.action.clone() }
  fn description(&self) -> String {
    match self.action.as_str() {
      "notion_search" => "Search Notion pages and databases. Params: query (string).".into(),
      "notion_create_page" => "Create a Notion page. Params: parentId (string, database or page id), title (string).".into(),
      "notion_get_page" => "Fetch a Notion page's content. Params: pageId (string).".into(),
      "airtable_list_records" => "List records from an Airtable table. Params: baseId (string), tableName (string), maxRecords (integer, optional).".into(),
      "airtable_create_record" => "Create a record in an Airtable table. Params: baseId (string), tableName (string), fields (object of field values).".into(),
      "airtable_update_record" => "Update a record in an Airtable table. Params: baseId (string), tableName (string), recordId (string), fields (object).".into(),
      "sheets_read" => "Read rows from a Google Sheet. Params: spreadsheetId (string), range (string, e.g. 'Sheet1!A1:C10').".into(),
      "sheets_append" => "Append rows to a Google Sheet. Params: spreadsheetId (string), range (string), values (array of arrays).".into(),
      "sheets_update" => "Update cells in a Google Sheet. Params: spreadsheetId (string), range (string), values (array of arrays).".into(),
      "docs_create" => "Create a Google Doc with content. Params: title (string), content (string).".into(),
      "docs_get" => "Read a Google Doc's content. Params: documentId (string).".into(),
      _ => "Integration action".into(),
    }
  }
  fn params_schema(&self) -> serde_json::Value {
    serde_json::json!({ "type": "object", "properties": { "query": { "type": "string" }, "parentId": { "type": "string" }, "title": { "type": "string" }, "pageId": { "type": "string" }, "baseId": { "type": "string" }, "tableName": { "type": "string" }, "maxRecords": { "type": "integer" }, "fields": { "type": "object" }, "recordId": { "type": "string" }, "spreadsheetId": { "type": "string" }, "range": { "type": "string" }, "values": { "type": "array" }, "documentId": { "type": "string" }, "content": { "type": "string" } }, "required": [] })
  }
  async fn run(&self, args: &serde_json::Value) -> Result<String, String> {
    let token = self.credentials.get("token").and_then(|t| t.as_str()).or_else(|| self.credentials.get("apiKey").and_then(|t| t.as_str())).or_else(|| self.credentials.get("accessToken").and_then(|t| t.as_str())).unwrap_or("");
    let client = http::client();
    let arg = |k: &str| args.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string();

    match self.action.as_str() {
      "notion_search" => {
        let body = serde_json::json!({ "query": arg("query") });
        let resp = client.post("https://api.notion.com/v1/search")
          .header("Authorization", format!("Bearer {token}"))
          .header("Notion-Version", "2022-06-28")
          .json(&body).send().await.map_err(|e| e.to_string())?;
        let text = resp.text().await.map_err(|e| e.to_string())?;
        Ok(clip(&text))
      }
      "notion_create_page" => {
        let body = serde_json::json!({
          "parent": { "type": "page_id", "page_id": arg("parentId") },
          "properties": { "title": { "title": [{ "text": { "content": arg("title") } }] } }
        });
        let resp = client.post("https://api.notion.com/v1/pages")
          .header("Authorization", format!("Bearer {token}"))
          .header("Notion-Version", "2022-06-28")
          .json(&body).send().await.map_err(|e| e.to_string())?;
        let text = resp.text().await.map_err(|e| e.to_string())?;
        Ok(clip(&text))
      }
      "notion_get_page" => {
        let resp = client.get(format!("https://api.notion.com/v1/pages/{}", arg("pageId")))
          .header("Authorization", format!("Bearer {token}"))
          .header("Notion-Version", "2022-06-28")
          .send().await.map_err(|e| e.to_string())?;
        let text = resp.text().await.map_err(|e| e.to_string())?;
        Ok(clip(&text))
      }
      "airtable_list_records" => {
        let resp = client.get(format!("https://api.airtable.com/v0/{}/{}/listRecords", arg("baseId"), arg("tableName")))
          .header("Authorization", format!("Bearer {token}"))
          .send().await.map_err(|e| e.to_string())?;
        let text = resp.text().await.map_err(|e| e.to_string())?;
        Ok(clip(&text))
      }
      "airtable_create_record" => {
        let body = serde_json::json!({ "fields": args.get("fields").cloned().unwrap_or(serde_json::json!({})) });
        let resp = client.post(format!("https://api.airtable.com/v0/{}/{}", arg("baseId"), arg("tableName")))
          .header("Authorization", format!("Bearer {token}"))
          .json(&body).send().await.map_err(|e| e.to_string())?;
        let text = resp.text().await.map_err(|e| e.to_string())?;
        Ok(clip(&text))
      }
      "airtable_update_record" => {
        let body = serde_json::json!({ "fields": args.get("fields").cloned().unwrap_or(serde_json::json!({})) });
        let resp = client.patch(format!("https://api.airtable.com/v0/{}/{}/{}", arg("baseId"), arg("tableName"), arg("recordId")))
          .header("Authorization", format!("Bearer {token}"))
          .json(&body).send().await.map_err(|e| e.to_string())?;
        let text = resp.text().await.map_err(|e| e.to_string())?;
        Ok(clip(&text))
      }
      "sheets_read" => {
        let resp = client.get(format!("https://sheets.googleapis.com/v4/spreadsheets/{}/values/{}", arg("spreadsheetId"), arg("range")))
          .header("Authorization", format!("Bearer {token}"))
          .send().await.map_err(|e| e.to_string())?;
        let text = resp.text().await.map_err(|e| e.to_string())?;
        Ok(clip(&text))
      }
      "sheets_append" => {
        let body = serde_json::json!({ "values": args.get("values").cloned().unwrap_or(serde_json::json!([])) });
        let resp = client.post(format!("https://sheets.googleapis.com/v4/spreadsheets/{}/values/{}:append?valueInputOption=RAW", arg("spreadsheetId"), arg("range")))
          .header("Authorization", format!("Bearer {token}"))
          .json(&body).send().await.map_err(|e| e.to_string())?;
        let text = resp.text().await.map_err(|e| e.to_string())?;
        Ok(clip(&text))
      }
      "sheets_update" => {
        let body = serde_json::json!({ "values": args.get("values").cloned().unwrap_or(serde_json::json!([])) });
        let resp = client.put(format!("https://sheets.googleapis.com/v4/spreadsheets/{}/values/{}?valueInputOption=RAW", arg("spreadsheetId"), arg("range")))
          .header("Authorization", format!("Bearer {token}"))
          .json(&body).send().await.map_err(|e| e.to_string())?;
        let text = resp.text().await.map_err(|e| e.to_string())?;
        Ok(clip(&text))
      }
      "docs_create" => {
        let body = serde_json::json!({ "title": arg("title") });
        let resp = client.post("https://docs.googleapis.com/v1/documents")
          .header("Authorization", format!("Bearer {token}"))
          .json(&body).send().await.map_err(|e| e.to_string())?;
        let text = resp.text().await.map_err(|e| e.to_string())?;
        Ok(clip(&text))
      }
      "docs_get" => {
        let resp = client.get(format!("https://docs.googleapis.com/v1/documents/{}", arg("documentId")))
          .header("Authorization", format!("Bearer {token}"))
          .send().await.map_err(|e| e.to_string())?;
        let text = resp.text().await.map_err(|e| e.to_string())?;
        Ok(clip(&text))
      }
      _ => Err(format!("Unknown integration action: {}", self.action)),
    }
  }
}
