# Data Model

SQLite is the single store for everything. Opened per command by `db::db(app)`; there is no
shared pool.

- **File:** `local-agent-os.sqlite3` in the Tauri app data dir — on Windows
  `%APPDATA%\com.localagentos.app\` (`…\Roaming\com.localagentos.app`). Next to it sits the
  per-agent workspace: `agents/<id>/{files,memory,runs,outputs}` plus a generated `config.json`.
- **Connection:** a fresh `rusqlite::Connection` per call, `busy_timeout` 5s so background
  summarization can overlap a chat turn.
- **Schema:** one idempotent `CREATE TABLE IF NOT EXISTS` batch in `db.rs`, plus targeted
  `ALTER TABLE` migrations for older DBs.
- **Serialization:** Rust records use `serde(rename_all = "camelCase")`; the frontend types
  in `src/types.ts` match.

## Tables

All `CREATE TABLE` statements live in `db.rs`. Column lists below are exact.

### `agents`

| Column | Type | Notes |
| --- | --- | --- |
| `id` | TEXT PK | `manager` for the system agent; otherwise `agent-<ms>`. |
| `name`, `objective`, `model` | TEXT | |
| `tool_ids` | TEXT | JSON array of tool ids. |
| `integrations` | TEXT | JSON array of integration ids. |
| `memory` | INTEGER | Bool. |
| `permissions` | TEXT | JSON array: `network` / `files` / `host_fs`. |
| `home_path` | TEXT | |
| `color` | TEXT | Default `#8b5cf6`. |
| `x`, `y` | REAL | Canvas position. |
| `is_manager` | INTEGER | Added by migration; `1` = the Laos system agent. |
| `description` | TEXT | Added by migration. |
| `skill_ids` | TEXT | Added by migration; JSON array. |
| `persona` | TEXT | **Added by migration only** (not in the initial `CREATE`). |

### `runs`

`id` (PK), `agent_id`, `started_at`, `status`, `model`, `input`, `output`,
`prompt_tokens`, `completion_tokens`.
Written by `execute_agent` (agents.rs) and `stream_chat` (chat.rs); read by `list_runs`.

### `memory`

`agent_id`, `key`, `value`, `updated_at`, PK `(agent_id, key)`. See [memory.md](./memory.md).

### `model_configs`

`id` (PK, e.g. `groq:llama-3.3-70b-versatile`), `provider`, `label`, `model`, `host`,
`api_key`, `enabled`.

### `tools`

`id` (PK), `name`, `kind`, `integration_id`, `description`, `enabled`, `config_json`.
`kind` is one of `http_get` / `api` / `read_file` / `write_file`.

### `workflows`

`id` (PK), `name`, `nodes` (JSON), `edges` (JSON), `updated_at`.

### `workflow_runs`

`id` (PK, `wfr-<ms>`), `workflow_id`, `workflow_name`, `started_at`, `ended_at`, `status`
(`running` / `completed` / `failed`), `input`, `final_output`, `steps` (JSON array of
`{nodeId, nodeLabel, output, promptTokens, completionTokens}`), `prompt_tokens`,
`completion_tokens`. Written by `workflows::execute_workflow`, read by `list_workflow_runs`.

### `integration_configs`

`id` (PK), `name`, `provider`, `config_json`, `enabled`, `connected`, `updated_at`.

### `tasks`

`id` (PK), `requester`, `assigned_agent`, `status`, `input`, `context`, `result`,
`created_at`, `completed_at`.

### `skills`

`id` (PK), `name`, `description`, `content`, `updated_at`. Rendered into prompts by
`agents::skills_prompt`.

### `agent_conversations`

`agent_id` (PK), `messages` (JSON array). The rolling-window history.

### `chat_sessions`

`id` (PK), `agent_id`, `title`, `created_at`, `updated_at`, `summary` (added by migration).
`title` defaults to `Chat`.

### `chat_messages`

`id` (INTEGER PK AUTOINCREMENT), `session_id`, `role`, `content`, `time`. Ordered by `id`.

### `day_contexts`

`agent_id`, `day` (`YYYY-MM-DD`, local), `summary`, `updated_at`, PK `(agent_id, day)`.

### `knowledge_docs`

`id` (PK), `title`, `content`, `tags` (JSON), `updated_at`.

### `telegram_logs`

`id` (INTEGER PK AUTOINCREMENT), `direction`, `chat_id`, `text`, `reply`, `status`, `detail`,
`created_at`. Pruned to the newest 500 rows.

## Migrations

`db.rs` checks `PRAGMA table_info(<table>)` and adds any missing column:

| Table | Added columns |
| --- | --- |
| `agents` | `is_manager`, `description`, `persona`, `skill_ids` |
| `runs` | `prompt_tokens`, `completion_tokens` |
| `chat_sessions` | `summary` |

There is no version table — migrations are single-column `ALTER`s that are safe to re-run.

## ID conventions

| Entity | Pattern |
| --- | --- |
| Manager agent | `manager` (fixed) |
| Agent | `agent-<timestamp_ms>` |
| Run | `<agent_id>-<timestamp_ms>` |
| Chat session | `sess-<timestamp_ms>` |
| Workflow | `wf-<timestamp_ms>` |
| Knowledge doc (auto) | `kb-<timestamp_ms>` |
| Task | `task-<timestamp_ms>` |

## Record types

| Rust (`models.rs`) | Frontend (`types.ts`) | Table |
| --- | --- | --- |
| `AgentRecord` / `AgentRequest` | `Agent` | `agents` |
| `ModelConfigRecord` | `ModelConfig` | `model_configs` |
| `ToolRecord` | `Tool` | `tools` |
| `SkillRecord` | `Skill` | `skills` |
| `WorkflowRecord` | `Workflow` | `workflows` |
| `TaskRecord` | `Task` | `tasks` |
| `ExecutionEvent` | `RunEvent` | (inside `runs.events` payloads) |
| `WorkflowExecution` / `WorkflowStep` | `WorkflowRunResult` / `WorkflowRunStep` | (returned) |
| `IntegrationRecord` | `Integration` | `integration_configs` |

`ExecutionEvent.kind` is serialized as `type` (`#[serde(rename = "type")]`).

## Write/read map

| Producer | Writes |
| --- | --- |
| `storage.rs` | agents, models, tools, skills, workflows, knowledge docs |
| `agents.rs` | `runs`; agent home files (outside the DB) |
| `chat.rs` | `runs`, `agent_conversations`, `chat_sessions` + `chat_messages` |
| `memory.rs` | `memory`, `agent_conversations`, `chat_sessions`, `day_contexts` |
| `tasks.rs` | `tasks` |
| `workflows.rs` | `workflow_runs` (each execution + its per-node steps) |
| `integrations.rs` | `integration_configs` |
| `telegram.rs` | `integration_configs` (telegram row), `telegram_logs` |

`workflows.nodes` / `workflows.edges` are stored as JSON and executed by
`workflows::execute_workflow` (linear BFS order; `loop`/`trigger` pass through, `checker` and
`gate` emit envelopes; per-step token fields are currently always `0`).

## What is *not* persisted

- **Workflow agent-node runs are not rows in `runs`.** Each execution is stored once in
  `workflow_runs` with its per-node steps; the individual nodes call `run_agent_once`, which
  creates no `runs` row.
- **Failed or cancelled chat turns.** `stream_chat` inserts the `runs` row only after the stream
  completes; an error mid-stream records nothing. A workflow that errors is still recorded, as
  `status='failed'`.
- Per-agent `files/`, `memory/` and `runs/` folders are created as scaffolding but unused — only
  `outputs/<runId>.txt` (one-shot `execute_agent` runs) and `config.json` are actually written.
