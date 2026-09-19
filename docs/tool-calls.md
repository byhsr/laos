# Tool Calls

How a model's tool call becomes a real action: the tool registry, permission gating, the
round loop, dispatch, and the confirmation popup.

Related: [harness.md](./harness.md) (provider shapes), [integrations.md](./integrations.md)
(integration actions as tools), [architecture.md](./architecture.md).

## The trait

Every agent-callable capability implements `AgentTool` (`src-tauri/src/tools/mod.rs`):

```rust
#[async_trait]
pub(crate) trait AgentTool: Send + Sync {
  fn name(&self) -> String;
  fn description(&self) -> String;
  fn params_schema(&self) -> serde_json::Value;   // JSON Schema for the LLM
  async fn run(&self, args: &serde_json::Value) -> Result<String, String>;
}
```

`tool_schemas(&tools)` (`agents.rs`) renders these into the OpenAI `{"type":"function", ...}`
array sent as the request `tools` field.

Caps and helpers:

| Symbol | Value | Meaning |
| --- | --- | --- |
| `MAX_TOOL_ROUNDS` | 5 | Max tool-call rounds per model turn. |
| `MAX_TOOL_CHARS` / `clip()` | 8000 | Tool results are truncated to this length. |
| `str_arg(args, key)` | — | Safe string extraction from call args. |

## Where tools come from

| Source | Built by | Examples |
| --- | --- | --- |
| DB-configured tools (`tools` table) | `agents::build_tools` | `http_get`, `api`, `read_file`, `write_file` |
| Integration actions | `agents::build_tools` (from `integration_definitions()`) | `notion_search`, `sheets_append`, `airtable_create_record` |
| Host-filesystem tools | `agents::build_tools` when `host_fs` is granted | `search_files`, `read_file_any`, `run_command` |
| Manager tools | `manager::manager_tools()` | `create_agent`, `run_workflow`, `delegate_task`, `recall_memory`, `knowledge_base` |
| Inline (tool-call loops) | `chat.rs`, `agents.rs`, `manager.rs` | the loop itself |

### DB tool kinds

`tools.kind` selects the implementation in `build_tools`:

| `kind` | Implementation | Requires permission |
| --- | --- | --- |
| `http_get` | `tools/web.rs::HttpTool` | `network` |
| `api` | `tools/api.rs::ApiTool` | `network` |
| `read_file` | `tools/filesystem.rs::ReadFileTool` | `files` |
| `write_file` | `tools/filesystem.rs::WriteFileTool` | `files` |

Unknown or disabled tools are skipped. `ApiTool` supports a URL template with `{param}`
placeholders, headers, an optional JSON body, and a manual parameter list that becomes the
LLM's argument schema; responses are pretty-printed JSON and clipped.

## Permission gating (`build_tools`)

`agent.permissions` is a list; the three recognized values are `network`, `files`, and
`host_fs`. A tool is only offered to the model if its gate is satisfied:

```
network  → http_get, api, AND every integration action
files    → read_file, write_file        (sandboxed to agents/<id>/files/)
host_fs  → search_files, read_file_any, run_command   (escapes the sandbox)
```

Integration actions additionally require the integration row to be `enabled` **and**
`connected`. Grants are explicit per agent — an integration the agent does not list is never
offered.

The agent sandbox is `{app_data}/agents/{id}/` with subfolders `files/ memory/ runs/
outputs/`; `ReadFileTool` / `WriteFileTool` resolve `home/files/<name>` and reject paths
that don't stay under `home`. `host_fs` tools deliberately escape this — `read_file_any`
takes an absolute path, `run_command` runs through the system shell (`cmd /C` on Windows).

## The tool-call lifecycle (streaming chat)

`chat::stream_chat` is the path with confirmation support:

1. **Advertise.** Tools = `manager_tools()` when `is_manager`, else `build_tools(...)`.
   They are sent as `tools` on every round.
2. **Call (non-streaming round).** `POST` with `stream: false`.
3. **Parse.** Read tool calls from `message.tool_calls` (Ollama) **or**
   `choices[0].message.tool_calls` (OpenAI-compatible); args are an object for Ollama, a
   JSON string for the others.
4. **Append + execute.** Push the assistant tool-call message, then for each call run it and
   push a `tool` message carrying `tool_call_id`.
5. **Loop.** Repeat up to `MAX_TOOL_ROUNDS` while calls come back.
6. **Finish.** When a round returns text (no calls), fall through to the streaming pass that
   emits the final answer token-by-token.

The non-streaming paths (`agents.rs::run_ollama_chat` / `run_openai_tool_chat`,
`manager.rs::manager_turn`) run the same 1–5 loop but return text instead of streaming.

## Dispatch

Two dispatchers, one per tool population:

| Dispatcher | Used by | Behavior |
| --- | --- | --- |
| `chat::execute_tool` → `tool.run(args)` | agent tools | Finds the tool by name; unknown name → `Unknown tool '<name>'.` |
| `manager::dispatch_manager_tool` | Manager tools | One `match` over every Manager tool name; also backs the confirmation path and Telegram. |

Both are reached through `chat::run_tool`, which is where confirmation is enforced. The
single shared `dispatch_manager_tool` exists specifically so a tool added to
`manager_tools()` cannot come back "unknown" from a second, drifted implementation.

## The confirmation gate

Only the **streaming** path (`chat::stream_chat`) confirms — it backs both the Manager chat
and agent chats. Mutating tools are gated; read-only ones run immediately.

**Which tools confirm** — `manager::requires_confirmation`:

```
create_agent  update_agent  delete_agent
create_workflow  update_workflow  delete_workflow  run_workflow
configure_integration  create_task  cancel_task  delegate_task
run_command
```

Everything else (listers, `get_workspace_status`, `recall_memory`, `knowledge_base`
list/get/search, `search_files`, `read_file_any`, `get_task_status`) executes without asking.

**The flow:**

1. `run_tool` sees a mutating tool and emits a structured event over the channel:
   `{"type":"confirm","requestId":"req-<ms>","tool":name,"args":{...}}`.
2. `runtime.ts::streamChat` parses it and calls the `onConfirm` callback.
3. `useConfirm` + `ConfirmDialog` render an inline popup just above the chat input (both
   `ManagerView` and `AgentWindow` mount it) with editable fields auto-built from the args
   (arrays comma-separated, secrets as password fields).
4. `confirm_manager_tool(requestId, approved, tool, args)` records the decision in the
   in-memory `PENDING_APPROVALS` map (`manager.rs`).
5. `run_tool` polls that map every 200ms for up to **120s**; on timeout it is treated as
   declined. Approved → execute **once** with `edited_args`; declined → the model receives
   `"The user declined this action."`. `run_tool` is the single executor —
   `confirm_manager_tool` only records the decision.

**Not confirmed:** `manager_turn` (used by `manager_message` and Telegram) calls
`dispatch_manager_tool` directly. It refuses `run_command` (`"run_command needs your
approval, which this channel can't ask for."`) and executes every other Manager tool without
a popup. This is the one intentional divergence — keep it explicit.

### Known quirks

- `knowledge_base` with `action: "save"` mutates the knowledge base but is **not** in
  `requires_confirmation`.

## Adding a tool

1. Implement `AgentTool` under `tools/` (or add a `manager_tool!` macro entry in `manager.rs`
   for a Manager-only tool) and export it from `tools/mod.rs`.
2. For a DB-configured tool: handle its `kind` in `agents::build_tools` and gate it behind a
   permission.
3. For a Manager tool: add it to `manager_tools()` **and** a `match` arm in
   `dispatch_manager_tool`; add it to `requires_confirmation` if it changes state.
4. Frontend: nothing to do for agent tools; Manager confirmations are data-driven by the
   confirm event.
5. Update this doc and the table in [harness.md](./harness.md) if the tool loop changes.
