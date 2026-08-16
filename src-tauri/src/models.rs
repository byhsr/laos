// Shared data types for the Local Agent OS backend.
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentRequest {
  pub id: String, pub name: String, pub objective: String, pub model: String,
  pub tool_ids: Vec<String>, pub integrations: Vec<String>, pub home_path: String, pub permissions: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Execution { pub output: String, pub events: Vec<ExecutionEvent>, pub run_id: String, pub prompt_tokens: u64, pub completion_tokens: u64 }

#[derive(Serialize)]
pub struct ExecutionEvent { pub time: String, #[serde(rename = "type")] pub kind: String, pub title: String, pub detail: Option<String> }

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ModelConfigRecord { pub id: String, pub provider: String, pub label: String, pub model: String, pub host: Option<String>, pub api_key: Option<String>, pub enabled: bool }

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ToolRecord { pub id: String, pub name: String, pub kind: String, pub integration_id: String, pub description: Option<String>, pub enabled: bool, pub config: serde_json::Value }

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AgentRecord {
  pub id: String, pub name: String, pub objective: String, pub model: String,
  pub tool_ids: Vec<String>, pub integrations: Vec<String>, pub memory: bool,
  pub permissions: Vec<String>, pub home_path: String, pub color: String,
  #[serde(default)] pub x: f64, #[serde(default)] pub y: f64,
  #[serde(default)] pub is_manager: bool, #[serde(default)] pub description: String,
  #[serde(default)] pub persona: String,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowRecord { pub id: String, pub name: String, pub nodes: serde_json::Value, pub edges: serde_json::Value, pub updated_at: String }

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowStep { pub node_id: String, pub node_label: String, pub output: String, pub prompt_tokens: u64, pub completion_tokens: u64 }

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowExecution { pub steps: Vec<WorkflowStep>, pub final_output: String, pub total_prompt_tokens: u64, pub total_completion_tokens: u64 }

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct IntegrationRecord { pub id: String, pub name: String, pub provider: String, pub enabled: bool, pub connected: bool, pub config: serde_json::Value, pub actions: Vec<IntegrationAction> }

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct IntegrationAction { pub name: String, pub description: String }

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TaskRecord { pub id: String, pub requester: String, pub assigned_agent: String, pub status: String, pub input: String, pub context: String, pub result: Option<String>, pub created_at: String, pub completed_at: Option<String> }

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ConversationRecord { pub agent_id: String, pub messages: serde_json::Value }

// Ollama chat (tool-calling) request/response
#[derive(Serialize)]
pub struct ChatRequest { pub model: String, pub messages: Vec<ChatMessage>, pub tools: Option<Vec<serde_json::Value>>, pub stream: bool }
#[derive(Serialize, Deserialize, Clone)]
pub struct ChatMessage { pub role: String, pub content: Option<String>, #[serde(skip_serializing_if = "Option::is_none")] pub tool_calls: Option<Vec<ToolCall>>, #[serde(skip_serializing_if = "Option::is_none")] pub tool_call_id: Option<String> }
#[derive(Serialize, Deserialize, Clone)]
pub struct ToolCall { pub id: String, #[serde(rename = "type")] pub kind: String, pub function: ToolCallFunction }
#[derive(Serialize, Deserialize, Clone)]
pub struct ToolCallFunction { pub name: String, pub arguments: serde_json::Value }

#[derive(Deserialize)]
pub struct ChatResponse { pub message: ChatMessage, #[serde(default)] pub prompt_eval_count: u64, #[serde(default)] pub eval_count: u64 }

// Firecrawl/WebAPI response shapes
#[derive(Deserialize)]
pub struct FirecrawlSearchResponse { pub data: Option<FirecrawlSearchData> }
#[derive(Deserialize)]
pub struct FirecrawlSearchData { #[serde(default)] pub web: Vec<SearchResult> }
#[derive(Deserialize)]
pub struct SearchResult { pub title: Option<String>, pub description: Option<String>, pub url: Option<String> }
#[derive(Deserialize)]
pub struct FirecrawlScrapeResponse { pub data: Option<FirecrawlScrapeData> }
#[derive(Deserialize)]
pub struct FirecrawlScrapeData { pub markdown: Option<String> }
