# Local Agent OS — Docs

Reference docs for the execution **harness** and the systems around it. Each file is
scoped to one concern so a change lands in exactly one place.

## What this is

A local-first desktop app for configurable AI agents. A React/Vite frontend talks to a
Tauri v2 Rust backend over `invoke`; the backend owns SQLite, the model providers, the
tool registry, workflow execution, memory, and the Telegram adapter.

- Frontend: React 18 + TypeScript + Tailwind v4 + Zustand (`src/`)
- Backend: Rust / Tauri v2 (`src-tauri/src/`)
- Storage: SQLite via `rusqlite` (`local-agent-os.sqlite3` in the app data dir)

## Doc map

| Doc | Covers |
| --- | --- |
| [architecture.md](./architecture.md) | Layers, Rust/frontend module map, boot wiring, Tauri command surface, dependency graph |
| [harness.md](./harness.md) | **The model-request harness**: every provider call path, the shared HTTP client/retry/timeouts, streaming protocols, token accounting, and the duplication to streamline |
| [tool-calls.md](./tool-calls.md) | Tool registry + kinds, permission gating, the end-to-end tool-call lifecycle, dispatch, and the confirmation gate |
| [integrations.md](./integrations.md) | Integration catalog, how providers become tools, credential storage/masking, OAuth, testing |
| [mcp.md](./mcp.md) | The generic MCP connector: servers, tool discovery/import, how a call runs, limits |
| [memory.md](./memory.md) | Context assembly, the rolling window, summarization, long-term facts, chat sessions, day context, recall |
| [webhooks.md](./webhooks.md) | Telegram tunnel + webhook receiver + long-poll, the OAuth loopback, health checks and logs |
| [data-model.md](./data-model.md) | SQLite schema, migrations, record types, memory keys, ID conventions |

## Repo layout

```
src/                         Frontend (React)
  runtime.ts                 The only bridge to the backend — one wrapper per Tauri command
  App.tsx                    Thin composition root (views stay mounted, hidden when inactive)
  hooks/                     Zustand stores, one per domain (useAgents, useRuns, useManager, …)
  components/views/          One file per view (Home, Agents, Canvas, Manager, Tasks, Workshop, Settings…)
  components/ui/             Shared primitives (Drawer, Dropdown, ConfirmDialog, Toaster, AgentAvatar…)
  assets/agents/             Lottie persona JSON files (+ registry in components/ui/AgentAvatar.tsx)

src-tauri/                   Backend (Rust / Tauri v2)
  src/main.rs                Entry point: declares modules, spawns background tasks, registers commands
  src/db.rs                  SQLite connection + idempotent schema/migrations
  src/provider.rs            Provider dispatch: endpoint, credentials, reasoning + Ollama tuning
  src/storage.rs             CRUD commands + Manager bootstrap
  src/http.rs                Shared provider HTTP client, retry, timeouts, request defaults
  src/agents.rs              One-shot agent execution + toolset assembly
  src/chat.rs                Streaming chat + the shared tool-call round loop
  src/manager.rs             "Laos" system agent: prompt, turn loop, tool dispatch, approvals
  src/memory.rs              Conversations, rolling window, summaries/facts, chat sessions
  src/mcp.rs                 MCP connector (stdio JSON-RPC): servers, discovery, calls
  src/tools/                 AgentTool trait + implementations (api, filesystem, integration, mcp, web)
  src/workflows.rs           Rule engine (deterministic checks + LLM judge) and linear execution
  src/tasks.rs               Task lifecycle + runs feed
  src/integrations.rs        Integration catalog, credential masking/merge, OAuth
  src/telegram.rs            Telegram tunnel, webhook receiver, long-poll loop
  src/tg_markdown.rs         Markdown → Telegram HTML subset
  src/updater.rs             In-app updater: check manifest, install, restart

docs/                        This documentation
```

## Dev harness

Prerequisites: Node 22+ (see `.nvmrc`), [Ollama](https://ollama.com) with `ollama pull qwen3:8b`
for the default local model. Cloud providers (Groq / OpenRouter) need an API key saved in
**Settings → Models**.

| Command | What it does |
| --- | --- |
| `npm run tauri dev` | Full desktop app (frontend + Rust backend). This is the real app. |
| `npm run dev` | Vite only, port 1420. UI iteration — cannot call Tauri commands, so it deliberately cannot execute agents. `executeAgent` has a browser-only Ollama fallback for preview. |
| `npm run build` | `tsc -b && vite build` — typecheck + production bundle. |
| `npm run preview` | Serve the production build (browser, no backend). |
| `cargo check` (in `src-tauri/`) | Typecheck the Rust backend. |

Ports and paths:

| Port | Used by |
| --- | --- |
| 1420 | Vite dev server (`strictPort`, `host: true`) |
| 11434 | Ollama (`http://127.0.0.1:11434`) |
| 14789 | Local Telegram webhook receiver |
| 14852 | OAuth loopback redirect (`http://127.0.0.1:14852/callback`) |

Data lives under the Tauri app data dir: `local-agent-os.sqlite3`, plus
`agents/<agentId>/{files,memory,runs,outputs}` and a generated `config.json` per agent.

### Live development vs. releases

`npm run tauri dev` is the real-time loop — frontend edits hot-reload; Rust edits rebuild and
restart the window on save. `tauri.conf.json` deliberately uses `npm run` (not `npm.cmd`) so
the same config works on Windows and on the CI runners.

`.github/workflows/release.yml` builds installers for Windows, macOS (Intel + Apple Silicon)
and Linux on any version tag (`v0.1.0` or `0.1.0`) and attaches them to a GitHub Release:

```
git tag v0.1.0
git push origin v0.1.0
```

macOS builds are unsigned, so the first launch needs right-click → Open.

### Auto-update

The app checks for a new release ~4s after launch and shows an in-app prompt
(`components/ui/UpdatePrompt.tsx`) offering **Install & restart**. Settings → General also has a
manual **Check for updates** (showing the installed version, from `updater::app_version`).

- Rust side: `src-tauri/src/updater.rs` (`check_for_update`, `install_update`, `restart_app`)
  via `tauri-plugin-updater`. No JS updater package is used.
- Manifest: `https://github.com/byhsr/laos/releases/latest/download/latest.json`. The repo must
  stay **public** — the app fetches this anonymously, so a private repo returns 404.
- **Required repo secrets:** `TAURI_SIGNING_PRIVATE_KEY` (the private key file's contents) and
  `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`. `createUpdaterArtifacts` makes both mandatory — without
  them the bundle step fails, so add them before tagging.
- The public key lives in `tauri.conf.json` (`plugins.updater.pubkey`). The matching private
  key (`~/.tauri/laos.key`) is gitignored — never commit it.
- macOS updates install only for a signed + notarized build, so Mac stays manual for now.

To ship an update: bump `version` in `tauri.conf.json` (and `package.json`), then push a tag
matching it. The updater compares the version in `tauri.conf.json`, **not** the tag name — a
tag without a version bump produces a release no client will offer to install.

## Maintenance map

Find the row for what you're changing, update the listed code and doc together.

| If you change… | Code | Doc |
| --- | --- | --- |
| A model provider (URL, auth, request/response shape) | `src-tauri/src/http.rs`, the provider branches in `agents.rs` / `chat.rs` / `manager.rs` / `memory.rs` / `workflows.rs` | [harness.md](./harness.md) |
| Timeouts, retries, backoff, max tokens | `src-tauri/src/http.rs` | [harness.md](./harness.md) |
| Streaming or a Tauri event/channel payload | `src-tauri/src/chat.rs`, `src/runtime.ts` | [harness.md](./harness.md), [tool-calls.md](./tool-calls.md) |
| A tool (`AgentTool` impl), its kind, or its permission gate | `src-tauri/src/tools/*`, `src-tauri/src/agents.rs` (`build_tools`) | [tool-calls.md](./tool-calls.md) |
| The tool-call loop or the round cap | `src-tauri/src/chat.rs`, `src-tauri/src/agents.rs`, `src-tauri/src/tools/mod.rs` | [tool-calls.md](./tool-calls.md) |
| A Manager tool or the confirmation list | `src-tauri/src/manager.rs` (`manager_tools`, `dispatch_manager_tool`, `requires_confirmation`) | [tool-calls.md](./tool-calls.md) |
| The confirmation popup UX | `src/components/ui/ConfirmDialog.tsx`, `src/hooks/useConfirm.ts`, `src/runtime.ts` | [tool-calls.md](./tool-calls.md) |
| An integration provider, action, or credential field | `src-tauri/src/integrations.rs`, `src-tauri/src/tools/integration.rs` | [integrations.md](./integrations.md) |
| MCP servers, discovery, or the connector protocol | `src-tauri/src/mcp.rs`, `src-tauri/src/tools/mcp.rs`, `src/components/views/McpServers.tsx` | [mcp.md](./mcp.md) |
| OAuth scopes, redirect port, or token exchange | `src-tauri/src/integrations.rs` | [integrations.md](./integrations.md), [webhooks.md](./webhooks.md) |
| Telegram tunnel/webhook/polling/receiver | `src-tauri/src/telegram.rs`, `src-tauri/src/tg_markdown.rs` | [webhooks.md](./webhooks.md) |
| Memory, context assembly, or summarization | `src-tauri/src/memory.rs`, `src-tauri/src/chat.rs` | [memory.md](./memory.md) |
| A table, column, or migration | `src-tauri/src/db.rs` (+ the owning module's SQL) | [data-model.md](./data-model.md) |
| A Tauri command (add/rename/remove) | `src-tauri/src/main.rs` + the module, `src/runtime.ts` | [architecture.md](./architecture.md) |
| A view, store, or nav entry | `src/App.tsx`, `src/components/views/*`, `src/hooks/*` | [architecture.md](./architecture.md) |
| Personas | `src/assets/agents/*.json`, `src/components/ui/AgentAvatar.tsx` (`PERSONAS`) | [architecture.md](./architecture.md) |
| Update check/install or the release manifest | `src-tauri/src/updater.rs`, `tauri.conf.json` (`plugins.updater`), `.github/workflows/release.yml` | [README.md](./README.md) |
