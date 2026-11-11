// Shared data types for the Local Agent OS backend.
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentRequest {
  pub id: String, pub name: String, pub objective: String, pub model: String,
  pub tool_ids: Vec<String>, pub integrations: Vec<String>, pub home_path: String, pub permissions: Vec<String>,
  #[serde(default)] pub skill_ids: Vec<String>,
  // The agent's own reasoning control; empty falls back to the model's setting.
  #[serde(default)] pub reasoning: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Execution { pub output: String, pub events: Vec<ExecutionEvent>, pub run_id: String, pub prompt_tokens: u64, pub completion_tokens: u64 }

#[derive(Serialize)]
pub struct ExecutionEvent { pub time: String, #[serde(rename = "type")] pub kind: String, pub title: String, pub detail: Option<String> }

// What the UI sees: never the key itself, only whether one is set.
#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ModelConfigRecord { pub id: String, pub provider: String, pub label: String, pub model: String, #[serde(default = "default_model_kind")] pub kind: String, #[serde(default)] pub voice: String, pub host: Option<String>, pub has_key: bool, pub enabled: bool, pub reasoning: String, pub vision: bool, pub transcribes: bool }

// What the UI sends back. `api_key` is write-only: empty leaves the stored key
// alone, `clear_key` removes it.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelConfigInput {
  pub id: String, pub provider: String, pub label: String, pub model: String,
  // What the model is for (llm, vision, image, video, media, multimedia, stt, tts).
  #[serde(default = "default_model_kind")] pub kind: String,
  // Voice for a tts model (provider voices are model-specific, e.g. OpenRouter).
  #[serde(default)] pub voice: String,
  #[serde(default)] pub host: Option<String>,
  #[serde(default)] pub api_key: Option<String>,
  #[serde(default)] pub clear_key: bool,
  pub enabled: bool, pub reasoning: String,
}

// Reasoning control for a model. 'auto' sends nothing at all to the provider, so
// models with no reasoning support are never sent a parameter they would reject.
pub fn default_reasoning() -> String { "auto".to_string() }

// The model kind a row falls back to when none was ever chosen (every existing
// row, and any older client that doesn't send one).
pub fn default_model_kind() -> String { "llm".to_string() }

// A provider adapter as the UI sees it. Built-ins are read-only; custom ones are
// editable. `api_key` is write-only, exactly like a model's key.
#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ProviderRecord {
  pub id: String, pub label: String, pub base_url: String,
  pub auth: String, pub auth_header: String,
  pub extra_headers: serde_json::Value,
  pub requires_key: bool, pub reasoning: String, pub provider_routing: bool,
  pub caps: serde_json::Value,
  pub has_key: bool, pub builtin: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderInput {
  pub id: String, pub label: String,
  #[serde(default)] pub base_url: String,
  #[serde(default = "default_auth")] pub auth: String,
  #[serde(default)] pub auth_header: String,
  #[serde(default)] pub extra_headers: serde_json::Value,
  #[serde(default = "default_true")] pub requires_key: bool,
  #[serde(default = "default_reasoning_style")] pub reasoning: String,
  #[serde(default)] pub provider_routing: bool,
  #[serde(default)] pub caps: serde_json::Value,
  #[serde(default)] pub api_key: Option<String>,
  #[serde(default)] pub clear_key: bool,
}

fn default_auth() -> String { "bearer".to_string() }
fn default_reasoning_style() -> String { "openai".to_string() }
fn default_true() -> bool { true }

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ToolRecord { pub id: String, pub name: String, pub kind: String, pub integration_id: String, pub description: Option<String>, pub enabled: bool, pub config: serde_json::Value }

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SkillRecord {
  pub id: String, pub name: String, pub description: String, pub content: String,
  #[serde(default)] pub docs: String,
  #[serde(default)] pub script_ids: Vec<String>,
}

// A project groups conversations and scopes their memory, so work on one thing
// can be tracked across many chats.
#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ProjectRecord { pub id: String, pub name: String, pub description: String, pub created_at: String, pub updated_at: String }

// A reusable runnable app: a shell command (`kind = "command"`) or an internal
// model prompt (`kind = "prompt"`). Run from the UI, by an agent (command apps,
// host_fs), or from a workflow node.
#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ScriptRecord {
  pub id: String, pub name: String, pub description: String,
  #[serde(default = "default_script_kind")] pub kind: String,
  #[serde(default)] pub command: String,
  #[serde(default)] pub cwd: String,
  #[serde(default)] pub prompt: String,
  #[serde(default)] pub model: String,
  pub updated_at: String,
}

fn default_script_kind() -> String { "command".to_string() }

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AgentRecord {
  pub id: String, pub name: String, pub objective: String, pub model: String,
  pub tool_ids: Vec<String>, pub integrations: Vec<String>, pub memory: bool,
  pub permissions: Vec<String>, pub home_path: String, pub color: String,
  #[serde(default)] pub x: f64, #[serde(default)] pub y: f64,
  #[serde(default)] pub is_manager: bool, #[serde(default)] pub description: String,
  #[serde(default)] pub persona: String,
  #[serde(default)] pub skill_ids: Vec<String>,
  // Pinned agents sort above the rest in the sidebar.
  #[serde(default)] pub pinned: bool,
  // Optional custom avatar (data URL); empty falls back to the persona animation.
  #[serde(default)] pub avatar: String,
  // The agent's own reasoning control (overrides the model's default).
  #[serde(default)] pub reasoning: String,
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

// A persisted workflow execution: the run row plus its per-node steps (as JSON,
// matching how workflows.nodes/edges are stored).
#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowRunRecord {
  pub id: String, pub workflow_id: String, pub workflow_name: String,
  pub started_at: String, pub ended_at: Option<String>, pub status: String,
  pub input: String, pub final_output: Option<String>, pub steps: serde_json::Value,
  pub prompt_tokens: u64, pub completion_tokens: u64,
}

// A configured MCP server. Two transports: a local command over stdio (`command`
// + `args` + `env`, the original) or a hosted server over Streamable HTTP (`url` +
// OAuth). `env` values and the OAuth tokens are masked on read since that's where
// secrets live; `connected` reports whether a remote server holds a token.
#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct McpServerRecord {
  pub id: String, pub name: String, pub command: String,
  pub args: serde_json::Value, pub env: serde_json::Value, pub enabled: bool,
  #[serde(default = "default_transport")] pub transport: String,
  #[serde(default)] pub url: String,
  #[serde(default)] pub connected: bool,
}

pub fn default_transport() -> String { "stdio".to_string() }

// A tool advertised by an MCP server (tools/list).
#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct McpToolInfo {
  pub name: String, pub description: String, pub schema: serde_json::Value,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct IntegrationRecord { pub id: String, pub name: String, pub provider: String, pub enabled: bool, pub connected: bool, pub config: serde_json::Value, pub actions: Vec<IntegrationAction> }

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct IntegrationAction { pub name: String, pub description: String }

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TaskRecord { pub id: String, pub requester: String, pub assigned_agent: String, pub status: String, pub input: String, pub context: String, pub result: Option<String>, pub created_at: String, pub completed_at: Option<String> }

// Ollama chat response shape. Requests are built as JSON in the provider layer so
// reasoning/options can be attached uniformly.
#[derive(Serialize, Deserialize, Clone)]
pub struct ChatMessage { pub role: String, pub content: Option<String>, #[serde(skip_serializing_if = "Option::is_none")] pub tool_calls: Option<Vec<ToolCall>>, #[serde(skip_serializing_if = "Option::is_none")] pub tool_call_id: Option<String> }
#[derive(Serialize, Deserialize, Clone)]
pub struct ToolCall { pub id: String, #[serde(rename = "type")] pub kind: String, pub function: ToolCallFunction }
#[derive(Serialize, Deserialize, Clone)]
pub struct ToolCallFunction { pub name: String, pub arguments: serde_json::Value }

#[derive(Deserialize)]
pub struct ChatResponse { pub message: ChatMessage, #[serde(default)] pub prompt_eval_count: u64, #[serde(default)] pub eval_count: u64 }
