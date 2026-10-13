# Memory & Context

How conversation history, long-term memory, and time-based context are assembled and fed to
the model. Owned by `src-tauri/src/memory.rs`; the chat path that consumes it is in
`src-tauri/src/chat.rs`.

Related: [harness.md](./harness.md) (the summarization call), [data-model.md](./data-model.md)
(tables and keys).

## The three tiers

| Tier | Stored in | In the prompt? | Purpose |
| --- | --- | --- | --- |
| **1. Rolling window** | `agent_conversations.messages` (JSON) / a session's `chat_messages` | Yes — last `ROLLING_WINDOW` messages of the **current session** | Coherent recent conversation. |
| **2. Long-term memory** | `am_*` typed memories + self-model; flat `memory` rows (`__summary__`, `fact:*`) | `am_*` **Yes — injected**; `memory` rows **pull only** | The agent's evolving "brain". |
| **3. Time-based context** | `chat_sessions.summary`, `day_contexts` | **Yes — injected** | Older chat/day summaries. |

**Memory is learned continuously and injected.** `memory_block` (used by `stream_chat` and
`manager_turn`) appends what the agent has learned — `agent_memory::compile_context` (typed
memories + self-model for the turn's scope) plus the day context — to every turn's system
prompt. It is empty when there is nothing to recall, so a fresh agent's prompt is unchanged. The
flat `memory` rows (`__summary__`, `fact:*`) remain pull-only, reachable via the Manager's
`recall_memory`, an attached memory tool, or the Memory view.

Learning happens *during* a session, not only at its close: `distill_if_due` re-distills a
growing session every `DISTILL_EVERY` (8) new messages, and `close_session` distills again on
close (and `delete_chat_session` distills before deleting). Re-distillation is safe because
`fox::write_distilled` skips a memory title the scope already holds.

## Constants & keys

| Symbol | Value | Meaning |
| --- | --- | --- |
| `ROLLING_WINDOW` | 4 | Max messages (user + assistant) sent as context. |
| `MEMORY_SUMMARY_KEY` | `__summary__` | The rolling conversation summary row. |
| `MEMORY_WATERMARK_KEY` | `__summary_upto__` | Index of the last message already folded into the summary. |
| `MAX_SUMMARY_INPUT_CHARS` | 6000 | Cap on the transcript handed to the summarizer. |
| `fact:*` keys | `fact:<up to 40 chars>` | Extracted durable facts. |

`load_memory` returns the **last 20** `memory` rows by `updated_at DESC`. Keys prefixed with
`__` are internal and excluded from the facts list (`load_memory_facts`).

## Context assembly in `stream_chat`

Per streaming turn (`chat.rs`):

1. Load the **current session's** messages (`load_session_messages`), or the agent-level
   conversation when a call carries no session; append the new user turn.
2. If history exceeds `ROLLING_WINDOW + 1`, spawn a **background** summarization (never in
   front of the reply the user is waiting for).
3. Take the last `ROLLING_WINDOW` messages as the window.

So the system prompt contains: the agent's objective (+ a tool note, and a memory-usage note
when a memory tool is attached), the identity lines, and **`memory_block`** — the learned typed
memories + self-model for the turn's scope, plus the day context (last chat summary, today,
yesterday). The block is empty until there is something to recall.

> The Manager path (`build_manager_system_prompt` callers and `manager_turn`) gets the same
> block. `recall_memory` still returns the full flat bundle on demand.

## Summarization (`summarize_old_turns`)

Runs in a spawned task after a chat turn. Bounded work per pass via the watermark:

1. `old_end = history.len() - ROLLING_WINDOW`; `upto = min(watermark, old_end)`.
2. Return early unless `old_end > upto + 1` (not worth a model call).
3. Build the transcript from `history[upto..old_end]`, truncate to `MAX_SUMMARY_INPUT_CHARS`.
4. Load the previous `__summary__`, and ask the model (via
   `memory::one_shot_completion`) to return:

   ```
   SUMMARY: <summary (max 150 words)>
   FACTS:
   - fact
   - fact
   ```

5. Upsert `__summary__` and one `fact:*` row per fact; save the watermark.

The summarizer is one-shot, non-streaming, and tool-free (`SUMMARY_MAX_TOKENS = 400`). With
`ROLLING_WINDOW = 4`, turns leave the window quickly, so once a conversation passes ~3
exchanges this fires on most turns.

## Chat sessions (bifurcated history)

Sessions exist so older chats are browsable separately from the live thread.

| Table | Holds |
| --- | --- |
| `chat_sessions` | `id, agent_id, title, created_at, updated_at, summary` |
| `chat_messages` | `id (autoincrement), session_id, role, content, time` |

- `append_session_message` lazily creates the session (`Chat` default title) on first write,
  appends the message, and bumps `updated_at`. Called from `stream_chat` for both the user
  and assistant turns.
- Sessions are auto-titled from the first user message by the frontend, then listed newest
  first (`list_chat_sessions`, limit 100).
- `close_session` → `close_chat_session`: summarizes the transcript, stores it on
  `chat_sessions.summary`, and **folds** it into today's `day_contexts` row (appended,
  capped at 3000 chars).

## Who invokes what (frontend wiring)

Where each step is triggered:

| Step | Invoked from | Scope |
| --- | --- | --- |
| Rolling window + `summarize_old_turns` | `chat::stream_chat` | **every** agent + Manager |
| Session write (`append_session_message`) | `chat::stream_chat` (needs `sessionId`) | AgentWindow and Manager both pass one |
| History load | `loadHistory` — AgentWindow on mount, ManagerView on mount | per agent |
| `close_session` (tier-3 summary + day-context fold) | `useManagerStore.newSession` (Manager) and `AgentWindow.newChat` (agent) | every agent + Manager |
| `recall_memory` (full context bundle) | `manager_tools()` | **Manager only** |
| Reset | `useManagerStore.reset` → `clear_agent_memory` | per agent |

The rolling window is sent every turn, and the learned `memory_block` is injected with it; the
flat `memory` rows stay pull-only. The Manager additionally has `recall_memory` for pulling the
full bundle on demand.

## Day context

`day_contexts` is keyed `(agent_id, day)` where `day` is a local `YYYY-MM-DD` string
(`today_key()` / `yesterday_key()`). A closed chat appends into today's row; days accumulate
over time. `build_day_context` reads today's/yesterday's rows back into every turn's system
prompt; it is written whenever `close_session` runs (see the invocation map above).

## Retrieval: `build_day_context` / `build_context_bundle`

Two builders, both in `memory.rs`:

`build_day_context(conn, agent_id)` — the time-based half, injected into every turn:

1. `## Last chat summary` — the newest non-empty `chat_sessions.summary`.
2. `## Today's context` — today's `day_contexts` row.
3. `## Yesterday's context` — yesterday's `day_contexts` row.

`build_context_bundle(conn, agent_id)` — the full bundle, for on-demand recall:

1. `## Long-term memory (facts learned about you)` — the `fact:*` rows.
2. everything `build_day_context` returns.

The Manager's `recall_memory` tool returns this full bundle as tool output, for when the user
asks "what have we done/talked about" or wants to continue prior work. `recall_memory` lives in
`manager_tools()` only.

## Clearing

`clear_agent_memory(agent_id)` deletes the agent's `memory` rows **and** its
`agent_conversations` row. It does **not** delete `chat_sessions` / `chat_messages` /
`day_contexts` — those persist as browsable history.

## Memory matrix

| Path | Window | Summary + facts | Day / last-chat context |
| --- | --- | --- | --- |
| `stream_chat` | ✔ (last 4, current session) | ✖ pull only | ✖ pull only |
| `manager_turn` (command + Telegram) | ✖ (single message) | ✖ pull only | ✖ pull only |
| `run_agent_once` (one-shot / workflow / task) | ✖ | skills only | ✖ |

## Wiring notes

- Sessions are keyed **per agent** (`useManagerStore.sessionIds`), so switching agents never
  attaches to another agent's session.
- Only the Manager has `recall_memory`; regular agents pull memory through an attached memory
  tool (or the Memory view), since nothing is injected automatically.
- `manager_turn` (Telegram / `manager_message`) injects no memory either (matching the
  streaming path); it still has no rolling window (it receives a single message).

## The agent's own memory (native — was fox/`agent-memory`)

Beyond the three tiers above (which are about *the user's* context), lup ships a
**built-in memory store**, implemented natively in `src-tauri/src/agent_memory.rs`
(it used to be fox's `agent-memory` package run as an MCP server; that connector has
been retired — memory is in-process now, no child process). It is the agent's own durable
memory — a keyed self-model plus a searchable store of typed memories
(episodic/semantic/procedural/policy/preference/reflection), raw events, and a namespace
registry. It is **on by default** with no setup, and `memory.rs` still owns conversation
context.

It is driven by the host (`src-tauri/src/fox.rs` is the host policy on top of the store): memory
is **learned** in the background, **injected** into the prompt, and **recalled** on demand:

| Phase | Where | What happens |
| --- | --- | --- |
| **Capture** | `chat.rs` (`stream_chat_inner`) | After every turn, the user turn + reply are written as `am_events` (`user_message`/`assistant_message`). No tool call, no instruction. |
| **Inject** | `memory.rs::memory_block` → `chat.rs`, `manager.rs` | Each turn's system prompt carries `compile_context` (typed memories + self-model for the scope) and the day context. Empty until there is something to recall. |
| **Distill** | `memory.rs` (`distill_if_due` while the chat runs; `close_chat_session` on close) | One model call extracts the agent's own **self-model entries + typed memories** from the transcript and writes them into the store. Re-runs are de-duplicated by title. |
| **Recall (on demand)** | `am_search` / `am_compile_context` / Memory view | A deeper pull than the injected slice — search, or the full compiled context. |
| **Feedback loop** | `fox::rate_turn` + `MessageBubble` | A good/bad signal on a reply is recorded as an `outcome` event; a bad one triggers `reflect` (recurring failures → a reflection memory). |

Data (all in the app SQLite DB; see [data-model.md](./data-model.md)):

- `am_events` — raw episodes. `am_memories` — durable typed memories.
- `am_self_model` — a keyed self-model per `(agent, scope)`. `am_namespaces` — tracked scopes.
- Scopes are compact keys (`agent:<id>`, `project:<id>`, `global`); recall inherits `global`.

Wiring:

- The **Memory** view reads the store through the `am_*` Tauri commands
  (`am_list_memories` / `am_search` / `am_get_self_model` / `am_stats` / `am_list_namespaces`).
- A lup **project** registers a `project:<id>` namespace on save and removes it on delete, so
  project memory is tracked together; turns in that project file under the project's scope.
