# Orchestration pipeline

How a request becomes work: the layers, the chat turn, tools, memory, capture routing,
workflows.

> Starting a session? Read [DEVLOG.md](./DEVLOG.md) first — it records what changed last
> time and what is still open. This file is the map; the dev log is the state.

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
| Router tools (`monid`, `yt_dlp`) | `agents::build_tools` | `host_fs` (drives the Monid and yt-dlp CLIs) |
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

## Capture routing — where a thing goes

Three stores, not interchangeable. `routing.rs` owns the contract *and* the reading.

| Store | Holds | Scope | In the prompt? |
| --- | --- | --- | --- |
| `items` | anything with a when or a do — notes, todos, reminders, goals, links, habits | global + `project_id` | no (the scheduler reads it) |
| `remember` | facts and **standing rules about the user** | rules: `global`; facts: `agent:<id>` | **yes**, every turn |
| `knowledge_base` | shared *reference* — wiki, ICP, specs, runbooks | global, shared | no, pull-only |

`routing::triage` runs **before the model is offered its tools** (`chat::stream_chat_inner`,
`manager::manager_turn`): deterministic cues first, the `classifier` role only when the message
looks like content worth filing. Agents hold three corresponding tools — `items`, `remember`,
`knowledge_base` — whose descriptions restate the contract.

Rules are stored at `global` scope because `agent_memory::scopes_for` inherits only
`<scope> + global`, so an agent-scoped rule is invisible inside a project chat. Due dates and
relative times resolve in the user's configured zone (`tz.rs`, default `Asia/Kolkata`).

`refile.rs` migrates a knowledge base that already accumulated memories/todos (the **tidy**
button in the Knowledge tab). Full detail: `docs/routing.md`.

Items carry an `owner` (`''` = the user, else the filing agent's id). The `items` tool is built with
the calling agent (`items_tool(app, agent_id)`, like `remember_tool`), so an agent's todos are kept
apart from the user's own — the Items view has a **filed by** filter that reads it.

## Notifications

`notifications.rs` + the `notifications` table are the durable inbox behind every alert. A fired
reminder, a new insight, a finished task and a workflow result all go through
`notify::notify_and_push`, which enqueues a row (pruned to 500), emits `lumin-notification`, and
raises the OS toast (suppressed while the window is focused). The Notifications view is the inbox;
the Rail's bell shows the unread count. Everything that used to be a 5-second toast is now
recoverable after a reload — including while the window sat on the tray.

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
| Tools | `tools/*.rs` (`items`, `knowledge`, `browser`, `desktop`, `filesystem`, `mcp`, `script`, `web`), `mcp.rs`, `integrations.rs` |
| Memory | `memory.rs`, `agent_memory.rs`, `fox.rs`, `brain.rs`, `retrieve.rs` |
| Notifications | `notifications.rs`, `notify.rs` |
| Connections / adapters / resources | `connections.rs`, `integrations.rs`, `provider.rs` |
| Capture routing | `routing.rs`, `tools/items.rs`, `tz.rs`, `refile.rs`, `capture.rs`, `reminders.rs` |
| Media | `media.rs`, `transcribe.rs` |
| Workflows / tasks | `workflows.rs`, `tasks.rs` |
| Frontend bridge / UI | `runtime.ts`, `hooks/*`, `components/views/*` |

## Where to dive deep

Find your concern, then start at the file that decides, and follow to the rest.

| If you need to… | Start here | Then | Doc |
| --- | --- | --- | --- |
| Change what a model call sends or how it streams | `chat::stream_chat_inner` (the reference path) | `provider.rs`, `http.rs`, `fallback.rs` | [harness.md](./docs/harness.md) |
| Add or change a tool | `tools/mod.rs` (the `AgentTool` trait) + the impl | `agents::build_tools`, `manager::manager_tools` | [tool-calls.md](./docs/tool-calls.md) |
| Change a Manager tool or what needs approval | `manager.rs` (`manager_tools`, `dispatch_manager_tool`, `requires_confirmation`) | `useConfirm`, `ConfirmDialog.tsx` | [tool-calls.md](./docs/tool-calls.md) |
| Change **where something the user says gets stored** | `routing.rs` (`ROUTING_RULES`, `route`, `triage`) | `tools/items.rs`, `tools/knowledge.rs`, `memory::store_rule` | [routing.md](./docs/routing.md) |
| Fix a reminder not firing, or the wrong time | `reminders::reminder_loop` | `tz.rs`, `telegram::owner_target` / `default_owner` | [routing.md](./docs/routing.md), [lumin.md](./docs/lumin.md) |
| Change what a turn is told about the user | `memory::memory_block` | `agent_memory::compile_context`, `retrieve::context_block` | [memory.md](./docs/memory.md) |
| Change retrieval ranking or decay | `retrieve.rs` (`rank`, `decayable`, the query profiles) | `brain.rs` | [memory.md](./docs/memory.md) |
| Add a media kind (audio, image, …) | `media.rs` (`prepare`) | `transcribe.rs`, `provider.rs` (the role), `ChatComposer.tsx` | [harness.md](./docs/harness.md) |
| Add a table, column, or migration | `db.rs` (migrations) | the owning module's SQL | [data-model.md](./docs/data-model.md) |
| Add a view, tab, or nav entry | `types.ts` (`View`), `useNavLabels.ts`, `Rail.tsx` | `App.tsx` (the pane) | [architecture.md](./docs/architecture.md) |
| Add a frontend → backend call | `runtime.ts` (one wrapper per command) | `main.rs` (`invoke_handler`) | [architecture.md](./docs/architecture.md) |

Deeper detail: [harness.md](./docs/harness.md), [tool-calls.md](./docs/tool-calls.md),
[memory.md](./docs/memory.md), [routing.md](./docs/routing.md), [lumin.md](./docs/lumin.md),
[data-model.md](./docs/data-model.md), [architecture.md](./docs/architecture.md). The doc map
in [docs/README.md](./docs/README.md) covers the rest.
