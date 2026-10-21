# Orchestration pipeline

How a request becomes work: the layers, the chat turn, tools, memory, workflows.

## Layers

| Layer | Where | Job |
| --- | --- | --- |
| Frontend | `src/` | React UI. Talks to the backend only through `src/runtime.ts`. |
| Backend | `src-tauri/src/` | Tauri commands. Owns SQLite, providers, tools, memory. |
| Store | `local-agent-os.sqlite3` | Everything persists here. |

One rule: the frontend never touches the DB, the network, or files — every call crosses `runtime.ts` → `invoke(...)`.

## One chat turn

`AgentWindow.send` → `runtime.streamChat` → `invoke('stream_chat')` → `chat::stream_chat_inner`:

1. Load history (the current session's messages, or the agent conversation).
2. Assemble tools — the agent's tools, or `manager_tools()` for the Manager.
3. Build the system prompt — objective, identity, memory block.
4. Take the last `ROLLING_WINDOW` (4) messages as context.
5. Resolve the provider (`provider::resolve`; falls back to local Ollama if it fails).
6. **Tool rounds** (up to `MAX_TOOL_ROUNDS = 5`): send `stream:false`, read tool calls, run them, append results.
7. **Final stream**: send `stream:true`, emit text deltas.
8. Persist the run + conversation; record the turn into memory (in the background).

Everything streams over one Tauri Channel: plain text is a token delta, JSON is a control event (`status`, `reasoning`, `confirm`).

## The harness (model calls)

Five places call a model. All share `provider.rs` (endpoint, key, reasoning) and `http.rs` (timeouts, retry).

| Path | Used by | Streams | Tools |
| --- | --- | --- | --- |
| `agents::run_agent_once_structured` | one-shot runs, workflows, tasks | no | yes |
| `chat::stream_chat` | agent chat, Manager chat | yes | yes |
| `manager::manager_turn` | `manager_message`, Telegram | no | yes |
| `memory::one_shot_completion` | summarization | no | no |
| `workflows::run_llm_judge` | checker nodes | no | no |

`chat::stream_chat` is the reference path; the rest are its non-streaming cousins.

## Tools

Every capability implements one trait (`tools/mod.rs`):

```rust
trait AgentTool {
  fn name(&self) -> String;
  fn description(&self) -> String;
  fn params_schema(&self) -> serde_json::Value;   // JSON schema for the LLM
  async fn run(&self, args: &serde_json::Value) -> Result<String, String>;
}
```

`agents::build_tools` turns the schemas into the request's `tools` field.

### Where tools come from

| Source | Built by | Gate |
| --- | --- | --- |
| DB tools (`tools` table) | `agents::build_tools` | permission (below) |
| Integration actions | `agents::build_tools` | `network` + integration enabled/connected |
| Host-filesystem tools | `agents::build_tools` | `host_fs` |
| PC-control tools | `tools::desktop_tools` | `pc_control` (Windows) |
| MCP tools | `agents::mcp_tools_for` | attaching the tool is the opt-in |
| Scripts | `tools::script_tools` | `host_fs` |
| Manager tools | `manager::manager_tools()` | Manager only |

### DB tool kinds

| `kind` | Implementation | Gate |
| --- | --- | --- |
| `http_get` | `tools/web.rs` | `network` |
| `api` | `tools/api.rs` | `network` |
| `read_file` / `write_file` | `tools/filesystem.rs` | `files` (sandboxed to `agents/<id>/files/`) |
| `mcp` | `tools/mcp.rs` | attach = opt-in |
| `browser` | `tools/browser.rs` | attach = opt-in |

### Permission gate

`agent.permissions` is `network`, `files`, `host_fs`, `pc_control`. A tool is only offered to the model if its gate is satisfied — checked when the toolset is built, not at call time. `host_fs` tools deliberately escape the agent sandbox.

### Dispatch

`chat::execute_tool` finds the tool by name and calls `run`: agent tools by name, Manager tools through the single `manager::dispatch_manager_tool` match.

### Confirmation gate

Mutating tools pause for approval; read-only ones run immediately.

1. `run_tool` sees a mutating tool → sends `{type:"confirm", requestId, tool, args}` over the channel.
2. `runtime.streamChat` → `useConfirm` → `ConfirmDialog` (inline above the chat input).
3. The user approves/edits → `confirm_manager_tool(requestId, approved, args)` records the decision.
4. `run_tool` polls that record (≤120s), then runs once with the edited args, or returns "declined".

`manager::requires_confirmation` is the list (create/update/delete agent & workflow, `run_workflow`, `delegate_task`, `run_command`, and the coarse PC actions). Low-level input tools (mouse/type/scroll) and all reads run without asking. Telegram can't ask, so `manager_turn` refuses `run_command` and runs the rest with no popup.

## Tool sources

- **MCP** (`mcp.rs`): spawns the server over stdio JSON-RPC, one long-lived session per server, 45s per-request timeout. `import_mcp_tools` turns `tools/list` into `kind='mcp'` rows; `call_tool` runs one.
- **Integrations** (`integrations.rs` + `tools/integration.rs`): a catalog (Notion, Airtable, Sheets, Docs, Telegram, Exa, Parallel); each enabled action becomes a tool when the agent has `network`. Secrets live in the OS credential store.
- **Browser** (`browser.rs` + `BrowserPanel.tsx`): a real embedded webview. Agents drive it through JS eval (`browser_open/navigate/read/click/type/…`); state flows to the UI over `lup-browser` events.
- **PC control** (`tools/desktop.rs`, Windows): `screen_capture` returns an image; the model acts on it with `mouse_*`, `type_text`, `press_keys`, window/clipboard/app tools.
- **Terminal** (`terminal.rs`): a real PTY per session, bindable to a Telegram bot (`telegram_bots.terminal_id`) that relays commands and screen output.

## Memory

- **Conversation context** (`memory.rs`): rolling window (4 messages), background summarization into `__summary__` + `fact:*`, chat sessions, day contexts.
- **Agent memory** (`agent_memory.rs`, native — was the fox `agent-memory` MCP server): `am_events` (raw turns), `am_memories` (typed), `am_self_model` (keyed), `am_namespaces` (scopes). Captured every turn, distilled in the background, injected into the prompt via `memory_block`.

## Workflows

`execute_workflow` (`workflows.rs`) runs nodes in **BFS (topological) order**, threading each node's output into the next; the last output is the result.

| Node | Does |
| --- | --- |
| `agent` / `subagent` | `run_agent_once` (identical) |
| `llm` | one model call, `{input}` templated |
| `script` | a saved app — a prompt call or a shell command |
| `integration` | one integration action |
| `checker` | deterministic rules, then an LLM judge (fails closed) |
| `gate` | evaluates a condition into `{decision}` |
| `trigger` / `loop` | pass-through (phase 1) |

Branch outcomes travel as **data envelopes**, not routing — `fromPort`/`toPort` are UI-only. Each run is saved to `workflow_runs`.

## Tasks

`run_task` creates a `tasks` row (`pending`), `delegate_task` sets it `running` and calls an agent one-shot (`structured=false`), then marks it `completed`/`failed`. Agent runs are recorded separately in `runs` (the Runs console).

## File map

| Concern | Files |
| --- | --- |
| Turn loop | `chat.rs`, `manager.rs`, `agents.rs` |
| Harness | `provider.rs`, `http.rs`, `anthropic.rs`, `fallback.rs` |
| Tools | `tools/*.rs`, `mcp.rs`, `integrations.rs`, `browser.rs` |
| Memory | `memory.rs`, `agent_memory.rs`, `fox.rs` |
| Workflows / tasks | `workflows.rs`, `tasks.rs` |
| Frontend bridge / UI | `runtime.ts`, `hooks/*`, `components/views/*` |

Deeper detail: `docs/harness.md`, `docs/tool-calls.md`, `docs/memory.md`, `docs/data-model.md`.
