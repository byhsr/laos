# Architecture

## Layers

```
┌──────────────────────────────────────────────────────────────────────┐
│ Frontend  (React 18 + TS + Tailwind v4 + Zustand)          src/      │
│   App.tsx  ─ thin composition root; all views stay mounted, hidden   │
│   hooks/   ─ one Zustand store per domain                            │
│   runtime.ts ─ the ONLY bridge: one wrapper per Tauri command        │
└───────────────────────────────┬──────────────────────────────────────┘
                                │  invoke() / Channel
┌───────────────────────────────▼──────────────────────────────────────┐
│ Backend  (Rust / Tauri v2)                        src-tauri/src/      │
│   commands ─── SQLite (rusqlite)                                     │
│               model providers (Ollama / Groq / OpenRouter)           │
│               tools, workflows, memory, integrations, Telegram       │
└──────────────────────────────────────────────────────────────────────┘
```

The frontend never touches SQLite, the network providers, or files directly — everything
crosses `src/runtime.ts`. The backend has no knowledge of React; it returns plain JSON.

## Backend module map

| File | Responsibility |
| --- | --- |
| `main.rs` | Declares modules, spawns the 3 background tasks, registers every `#[tauri::command]`. |
| `db.rs` | Opens `local-agent-os.sqlite3`, runs idempotent `CREATE TABLE IF NOT EXISTS` + column migrations. Exposes `db(app)` and `now()`. |
| `models.rs` | Shared `serde` record/DTO types only (no logic). |
| `storage.rs` | CRUD commands for knowledge docs, models, tools, skills, agents, workflows; Manager bootstrap; `stored_api_key`, `manager_default_model`. |
| `http.rs` | The shared provider HTTP layer: clients, timeouts, retry/backoff, request defaults. |
| `agents.rs` | One-shot agent execution (`execute_agent`), toolset assembly (`build_tools`), tool schemas, skills prompt, workspace context. |
| `chat.rs` | Streaming chat (`stream_chat`) + the shared tool-call round loop and the confirmation gate. |
| `manager.rs` | The "Laos" system agent: system prompt builder, non-streaming turn loop, `dispatch_manager_tool`, approvals. |
| `memory.rs` | Conversations, rolling window, summarization, chat sessions, day context, `build_context_bundle`. |
| `mcp.rs` | MCP connector: JSON-RPC over stdio to a local MCP server; tool discovery, import, and calls. |
| `tools/` | `AgentTool` trait + implementations (`api.rs`, `filesystem.rs`, `integration.rs`, `web.rs`). |
| `updater.rs` | In-app updater: checks the signed release manifest, installs an update, restarts. |
| `workflows.rs` | Deterministic rule engine + LLM judge; linear workflow execution. |
| `tasks.rs` | Task lifecycle (`pending → running → completed/failed/cancelled`) and the runs feed. |
| `integrations.rs` | Integration catalog, credential masking/merge, connection tests, OAuth. |
| `telegram.rs` | Telegram tunnel, local webhook receiver, long-poll loop, health, activity log. |
| `tg_markdown.rs` | Markdown → Telegram HTML subset. |

## Boot wiring

`main.rs::setup()` spawns three long-lived tasks, then runs the app:

1. `telegram::telegram_reconcile_on_boot` — clears a dead ephemeral webhook left by a previous session so long-polling resumes instead of stalling.
2. `telegram::telegram_loop` — the long-poll adapter (no-op until a bot token is configured).
3. `telegram::telegram_webhook_server(handle, 14789)` — the local webhook receiver.

Every command builds its own SQLite connection via `db(&app)` (no shared pool). In-memory
state is deliberately tiny: `manager::PENDING_APPROVALS`, and `telegram`'s
`TELEGRAM_TUNNEL_URL` / `TELEGRAM_TURN_LOCK` / `SEEN_UPDATES`.

## Tauri command surface

Registered in `main.rs::invoke_handler`. The frontend wrapper for each lives in `src/runtime.ts`.

| Group | Commands | Module |
| --- | --- | --- |
| Storage bootstrap + CRUD | `initialize_storage`, `list_model_configs`, `save_model_config`, `delete_model_config`, `list_tools`, `save_tool`, `delete_tool`, `list_agents`, `save_agent`, `delete_agent`, `list_workflows`, `save_workflow`, `delete_workflow`, `list_knowledge_docs`, `get_knowledge_doc`, `save_knowledge_doc`, `delete_knowledge_doc`, `list_skills`, `save_skill`, `delete_skill` | `storage.rs` |
| Integrations | `list_integrations`, `save_integration_config`, `test_integration`, `start_oauth`, `connect_oauth`, `complete_oauth` | `integrations.rs` |
| MCP | `list_mcp_servers`, `save_mcp_server`, `delete_mcp_server`, `test_mcp_server`, `import_mcp_tools` | `mcp.rs` |
| Execution | `execute_agent`, `execute_workflow` | `agents.rs`, `workflows.rs` |
| Tasks + runs | `list_all_tasks`, `get_task`, `run_task`, `cancel_task`, `list_runs` | `tasks.rs` |
| Manager + chat | `manager_message`, `confirm_manager_tool`, `stream_chat` | `manager.rs`, `chat.rs` |
| Memory + sessions | `get_conversation`, `clear_agent_memory`, `list_chat_sessions`, `get_chat_session`, `create_chat_session`, `delete_chat_session`, `rename_chat_session`, `close_session` | `memory.rs` |
| Updater | `check_for_update`, `install_update`, `restart_app` | `updater.rs` |
| Telegram | `list_telegram_logs`, `telegram_start_tunnel`, `telegram_register_webhook`, `telegram_register_custom_url`, `telegram_stop_tunnel`, `telegram_tunnel_status`, `telegram_webhook_health` | `telegram.rs` |

Note: `execute_agent` (non-streaming) and `stream_chat` (streaming) are two different entry
points into the same underlying model layer — see [harness.md](./harness.md).

## Frontend map

| Area | Files | Notes |
| --- | --- | --- |
| Composition root | `App.tsx` | Wires stores + views. Views are kept mounted and CSS-hidden when inactive so in-flight streaming and per-agent state survive navigation. Drawer forms for tool/model/skill are hosted here. |
| Backend bridge | `runtime.ts` | One typed wrapper per command. `streamChat` wraps a Tauri `Channel`; structured events arrive as JSON strings, token deltas as plain text. |
| Stores | `hooks/useAgents.ts`, `useRuns.ts`, `useModels.ts`, `useTools.ts`, `useSkills.ts`, `useWorkflows.ts`, `useWorkspace.ts`, `useIntegrations.ts`, `useManager.ts`, `useTasks.ts`, `useConfirm.ts`, `useToast.ts` | One Zustand store per domain; loaded once on mount in `App.tsx`. |
| Views | `components/views/*` | Home, Agents (browse), Canvas (workflows), Manager (Laos), Tasks, Runs, Telegram, Workshop (Skills/Tools/Integrations/Knowledge), Settings (General/Models), the agent window, and first-run `Onboarding`. |
| Shell | `components/Topbar.tsx`, `components/Sidebar.tsx` | The topbar carries the workspace sections + Graph + New agent + window controls; the sidebar is the **agent chat list**, where the **lead agent always holds the first slot** (never deletable — `delete_agent` refuses `is_manager=1`). Section names are user-editable via `hooks/useNavLabels.ts`; the lead's *own* name comes from its agent record. |
| Primitives | `components/ui/*` | Drawer (resizable right panel), Dropdown/MultiDropdown (portaled), ConfirmDialog, Toaster, Tooltip (shell icon controls), AgentAvatar/PersonaPicker, ContextMenu. |
| Personas | `assets/agents/*.json` + `components/ui/AgentAvatar.tsx` (`PERSONAS`) | Lottie files; add a file and register it in `PERSONAS`. |

## Onboarding

First run shows `components/Onboarding.tsx` (Welcome → Name + persona → Purpose → Model). It writes
the answers onto the lead agent's record and drops you into its chat, then sets the
`laos.onboarded` localStorage flag so it doesn't return. Settings → General → **Re-run onboarding**
clears it again. It renders at `z-40` so the topbar (and its window controls) stay reachable.

## Dependency graph

```
main.rs ──registers──▶ storage, integrations, agents, workflows, tasks, manager, chat, memory, telegram

db.rs       ◀── everyone
http.rs     ◀── agents, chat, memory, manager, workflows, integrations, telegram
storage.rs  ◀── agents, chat, memory, manager, tasks, workflows
models.rs   ◀── all modules needing DTOs
integrations◀── agents, manager, workflows
tools/*     ◀── agents, manager, chat, workflows
memory.rs   ◀── chat, manager
agents.rs   ◀── workflows, tasks, chat, manager, telegram
tasks.rs    ◀── manager, telegram, agents
workflows.rs◀── manager
manager.rs  ◀── chat (approvals, dispatch), telegram
chat.rs     ── top-level command
```

## Data-flow rules to preserve

- **One harness, many entry points.** Chat, one-shot runs, workflows, Telegram, and
  summarization all reach the providers; they should share `http.rs` and ideally one tool
  loop. See the duplication table in [harness.md](./harness.md).
- **Deterministic work stays out of the LLM.** Slash commands, checker rules, and gates are
  resolved in code.
- **Secrets never leave the backend.** `stored_api_key` / `integration_secret` read them;
  the UI only ever receives masked values. See [integrations.md](./integrations.md).
- **Writes are recorded.** Agent runs and streaming chats insert into `runs`; tools and
  workflows read/write through the owning module's SQL. See [data-model.md](./data-model.md).
