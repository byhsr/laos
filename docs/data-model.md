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

## Vault

**No secret is stored in SQLite.** API keys, tool headers, MCP env, integration credentials and
Telegram tokens live in the OS credential store — Windows Credential Manager, macOS Keychain, or
Secret Service on Linux — behind `vault.rs` (service `local-agent-os`, account `model/<id>`,
`tool/<toolId>/<header>`, `mcp/<serverId>/<KEY>`, `integration/<id>/<field>`,
`telegram/<botId>/{token,webhook}`).

The column or JSON field that used to hold the secret now holds the sentinel `__vault__`, and:

- **Reads** resolve it. `integration_secret`, `provider::model_settings`, `mcp::env_pairs`,
  `telegram::load_bot` and the `api` tool's header builder are the only places that see a real value.
- **Writes** move a newly submitted value straight into the store and keep the sentinel. A value
  the UI echoes back as the mask (`••••••••`) is left untouched, so a form round-trip can't clobber
  a key.
- **The renderer never receives a secret.** Model configs return `hasKey` instead of `apiKey`;
  masked fields are write-only.
- **Migration** runs once at startup (`initialize_storage` → `vault::migrate`): any plaintext left
  by an older build moves into the store, and a row whose secret can't be written is left exactly
  as it was.

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
| `pinned` | INTEGER | Added by migration; pinned agents sort above the rest in the sidebar. |
| `avatar` | TEXT | Added by migration; optional custom avatar (data URL). Empty falls back to the persona animation. |

### `runs`

`id` (PK), `agent_id`, `started_at`, `status`, `model`, `input`, `output`,
`prompt_tokens`, `completion_tokens`.
Written by `execute_agent` (agents.rs) and `stream_chat` (chat.rs); read by `list_runs`.

### `memory`

`agent_id`, `key`, `value`, `updated_at`, PK `(agent_id, key)`. See [memory.md](./memory.md).

### `am_events` / `am_memories` / `am_self_model` / `am_namespaces`

Native agent memory (`src-tauri/src/agent_memory.rs`) — the store that used to be the fox
`agent-memory` MCP server. `am_events`(id, agent_id, scope, scope_type, type, role, content, data,
importance, occurred_at); `am_memories`(id, agent_id, scope, scope_type, kind, status, title,
content, tags, confidence, importance, source, expires_at, created_at, updated_at);
`am_self_model`(id, agent_id, scope, scope_type, key, value, confidence, importance, source,
created_at, updated_at, UNIQUE(agent_id, scope, key)); `am_namespaces`(key PK, type, id, name,
description, created_at, updated_at, metadata). Scopes are keys (`agent:<id>` / `project:<id>` /
`global`). Distilled chat memory is written under the session's scope — `project:<id>` when the
session belongs to a project, else `agent:<id>` — but always with the agent's `agent_id`. The
Memory view reads agent-centric (`am_list_memories` / `am_search`): its own scope, anything it
authored (project scopes included), and `global`.

### `brain_nodes` / `brain_edges` / `brain_events` / `brain_observations` / `attention_state` / `embeddings`

The personal context engine (`src-tauri/src/brain.rs`, `classify.rs`, `retrieve.rs`, `embed.rs`).
Additive: nothing existing is migrated.

- `brain_nodes`(id PK, kind, scope, title, body, ref_kind, ref_id, source, provenance, status,
  tags, importance, salience, confidence, meta, created_at, updated_at) — the unified node
  registry. When `ref_id != ''` the authoritative row lives in `items` / `am_memories` /
  `chat_messages` (`ref_kind` names it) and the node is an index + edge anchor; otherwise the node
  owns its content (people, decisions, ideas, goals…). A partial unique index
  `(ref_kind, ref_id)` keeps re-filing an item idempotent. `provenance` is `stated` or `inferred`.
- `brain_edges`(id PK, src_id, dst_id, relation, provenance, confidence, evidence, created_at,
  updated_at, UNIQUE(src_id, dst_id, relation)) — typed relationships; a re-observed edge raises
  its confidence. Relations include `associated with`, `belongs to project`, `related to`, and the
  inferred `supersedes` (a new goal/decision replacing an earlier one).
- `brain_events`(id PK, agent_id, scope, kind, content, data, node_id, occurred_at) — append-only
  log of interactions and state changes (distinct from `am_events`, the raw episode log).
- `brain_observations`(id PK, scope, dimension, kind, title, body, confidence, evidence, status,
  first_seen_at, last_seen_at, created_at, updated_at) — inferred patterns with evidence,
  de-duplicated by title. `status` is `new` / `accepted` / `dismissed` / `corrected`. `dimension`
  names the signal (`displacement`, `drift`, …) or `entity_resolution` for a name that matched two
  nodes equally and needs the user to choose; `evidence` may carry `{"phrased":true}` once the
  wording has been through the model.
- `attention_state`(id PK, scope, node_id, dimension, value, evidence, confidence, observed_at,
  updated_at, UNIQUE(node_id, dimension)) — per-node attention signals, computed deterministically
  from observable evidence: `allocation`, `dormancy`, `intention`, `persistence`, `momentum`,
  `displacement`, `drift`, `reentry`, `salience`. `brain_graph` returns them per node (keyed by
  dimension) so the Graph view's attention lens can size/colour nodes.
- `embeddings`(id PK, source_kind, source_id, model, dim, vector BLOB, content_hash, created_at,
  UNIQUE(source_kind, source_id)) — vectors stored as little-endian f32 and ranked in Rust
  (cosine); no native SQLite extension.
- `brain_fts` — an FTS5 index over `brain_nodes` (`node_id UNINDEXED, title, body, tags`). When
  SQLite is built without FTS5 the keyword path falls back to a token-containment scan.

### `model_roles`

`role` (PK — `classifier` / `embedding` / `reranker` / `vision` / `transcriber` / `distillation`),
`model_id` (the pinned `"<provider>:<model>"` id, or `''` for auto), `updated_at`. `classifier` and
`embedding` always resolve to a default (`provider::resolve_role` — embedding picks the first enabled
provider that supports embeddings, preferring local; classifier prefers a cloud model). `reranker` /
`vision` / `transcriber` / `distillation` are **opt-in**: they take effect only when pinned
(`provider::explicit_role`), so an unconfigured app behaves exactly as it did before the role
existed. Where each is used: `classifier` reads captures and phrases observations, `embedding`
vectorises for recall, `reranker` reorders retrieved hits, `vision` describes media dropped into a
chat, `transcriber` turns voice notes and audio into text (`POST /audio/transcriptions` — OpenAI,
Groq, or a local whisper server; see [harness.md](./harness.md)), `distillation` writes durable
memory from a finished chat.
A model record also reports two computed flags so the role picker can mark suitable models:
`vision` (`provider::looks_like_vision`, a name heuristic) and `transcribes`
(`provider::supports_transcription`, a provider-level fact). Neither is stored state.

### `model_configs`

`id` (PK, e.g. `groq:llama-3.3-70b-versatile`), `provider`, `label`, `model`, `host`,
`api_key`, `enabled`, `reasoning`.

`api_key` holds the vault sentinel, never the key — the value lives under `model/<id>` (see
[Vault](#vault)). `list_model_configs` returns `hasKey` rather than the key; `save_model_config`
takes a write-only `apiKey` plus `clearKey`.

`host` overrides the endpoint root for `ollama:` models (default `http://127.0.0.1:11434`).
`reasoning` is one of `auto` / `off` / `low` / `medium` / `high` and controls the provider's
reasoning parameter — see [harness.md](./harness.md#reasoning-control).

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

`id` (PK), `name`, `description`, `content` (instructions), `docs` (reference material),
`script_ids` (JSON array of saved-script ids it can run), `updated_at`. Rendered into prompts by
`agents::skills_prompt` (instructions + reference + the names of its scripts, which the agent
runs with the `run_script` tool).

### `scripts`

`id` (PK), `name`, `description`, `kind` (`command` | `prompt`), `command`, `cwd`, `prompt`,
`model`, `updated_at`. Reusable runnable **apps** (Workshop → apps): a `command` app runs a
shell command, a `prompt` app runs a single internal model call. Run from the UI
(`run_script_now`), by an agent with `host_fs` (`run_script`, command apps), or from a workflow
`script` node.

### `projects`

`id` (PK), `name`, `description`, `created_at`, `updated_at`. Group conversations and scope
their memory; the active project is chosen in Workshop → projects and tags new chat sessions.

### `agent_conversations`

`agent_id` (PK), `messages` (JSON array). The rolling-window history.

### `chat_sessions`

`id` (PK), `agent_id`, `title`, `created_at`, `updated_at`, `summary` (added by migration),
`project_id` (added by migration; '' = none), `distilled_count` (added by migration; the message
count at the last distillation, so a growing chat is re-learned every 8 messages). `title`
defaults to `Chat`. A session's project
scopes its turn memory (`chat.rs` passes it to `fox::record_turn`).

### `chat_messages`

`id` (INTEGER PK AUTOINCREMENT), `session_id`, `role`, `content`, `time`. Ordered by `id`.

### `day_contexts`

`agent_id`, `day` (`YYYY-MM-DD`, local), `summary`, `updated_at`, PK `(agent_id, day)`.

### `knowledge_docs`

`id` (PK), `title`, `content`, `tags` (JSON), `updated_at`.

### `telegram_logs`

`id` (INTEGER PK AUTOINCREMENT), `direction`, `chat_id`, `text`, `reply`, `status`, `detail`,
`created_at`. Pruned to the newest 500 rows. `detail` carries the bot's name (or an error).

### `items`

Lumin's store ([lumin.md](./lumin.md)): `id` (PK), `kind` (`note` | `todo` | `reminder` |
`goal` | `link` | `habit`), `title`, `body`, `url`, `tags` (JSON), `collection`,
`status` (`active` | `done`), `due_at` (RFC3339; the scheduler fires on it), `cadence`
(`''` | `daily` | `weekly` | `monthly`; recurring items roll forward), `streak` (habits),
`last_done_at`, `project_id` (scopes an item to a project), `fired_at` (`NULL` = not yet
fired), `source` (bot name, `app` or `insight`), `created_at`, `updated_at`.

### `insights`

The learning layer's proposals: `id` (PK), `kind` (`goal` | `preference` | `pattern` |
`habit_suggestion`), `title`, `body`, `confidence`, `status` (`new` | `accepted` |
`dismissed`), `created_at`, `updated_at`.

### `destinations`

Where captured items are pushed ([mcp.md](./mcp.md)): `id` (PK, `dst-<ms>`), `name`,
`server_id` (an `mcp_servers` id), `tool`, `arg_template` (JSON with
`{{title}}`/`{{body}}`/`{{url}}`/`{{tags}}`/`{{kind}}`), `enabled`, `created_at`, `updated_at`.

### `telegram_bots`

`id` (PK), `name`, `token`, `agent_id` (`''`/`manager` = the Manager), `terminal_id` (bound
PTY session, or `''`), `allowed_users` (JSON array of Telegram user ids; empty = open),
`capture` (added by migration; ticked = free text is filed as an item, see
[lumin.md](./lumin.md)), `owner_chat_id` (added by migration; the chat the scheduler messages
unprompted, filled from the first authorized sender), `destination_id` (added by migration; the
destination captured items are pushed to, or `''`), `poll_offset` (added by migration; the
persisted long-poll offset, so a restart can't replay old updates), `enabled`, `webhook_secret`,
`webhook_registered`, `updated_at`. One
long-poll loop runs per enabled bot (`telegram.rs`); incoming messages route to the bot's agent
**or** into its bound terminal session. A bot can instead register a
webhook at `{base}/webhook/telegram/{botId}` with its own secret, so **one tunnel serves many
bots** (routed by path) and only that bot's polling pauses. The legacy single
`integration_configs` token is migrated into a `bot-default` row on boot.

## Migrations

`db.rs` checks `PRAGMA table_info(<table>)` and adds any missing column:

| Table | Added columns |
| --- | --- |
| `agents` | `is_manager`, `description`, `persona`, `skill_ids`, `pinned`, `avatar` |
| `runs` | `prompt_tokens`, `completion_tokens` |
| `chat_sessions` | `summary`, `project_id`, `distilled_count` |

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
| `brain.rs` (+ `classify.rs`, `retrieve.rs`, `insights.rs`, `eval.rs`) | `brain_nodes`, `brain_edges`, `brain_events`, `brain_observations`, `attention_state`, `embeddings` |

`workflows.nodes` / `workflows.edges` are stored as JSON and executed by
`workflows::execute_workflow` (linear BFS order; `loop`/`trigger` pass through, `checker` and
`gate` emit envelopes; `llm` makes a single model call and `script` runs a shell command;
`retrieve_context` runs the hybrid retrieval service and emits `{ text, items }`;
per-step token fields are currently always `0`).

## What is *not* persisted

- **Workflow agent-node runs are not rows in `runs`.** Each execution is stored once in
  `workflow_runs` with its per-node steps; the individual nodes call `run_agent_once`, which
  creates no `runs` row.
- **Failed or cancelled chat turns.** `stream_chat` inserts the `runs` row only after the stream
  completes; an error mid-stream records nothing. A workflow that errors is still recorded, as
  `status='failed'`.
- Per-agent `files/`, `memory/` and `runs/` folders are created as scaffolding but unused — only
  `outputs/<runId>.txt` (one-shot `execute_agent` runs) and `config.json` are actually written.
