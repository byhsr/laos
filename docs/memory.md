# Memory & Context

How conversation history, long-term memory, and time-based context are assembled and fed to
the model. Owned by `src-tauri/src/memory.rs`; the chat path that consumes it is in
`src-tauri/src/chat.rs`.

Related: [harness.md](./harness.md) (the summarization call), [data-model.md](./data-model.md)
(tables and keys).

## The three tiers

| Tier | Stored in | Injected per turn? | Purpose |
| --- | --- | --- | --- |
| **1. Rolling window** | `agent_conversations.messages` (JSON) | Yes — last `ROLLING_WINDOW` messages | Coherent recent conversation. |
| **2. Long-term memory** | `memory` rows: `__summary__` + `fact:*` | Yes (summary + facts) | The agent's evolving "brain". |
| **3. Time-based context** | `chat_sessions.summary`, `day_contexts` | Yes — `build_day_context` | Older chat/day summaries. |

All three tiers are injected on every turn. `recall_memory` (Manager only) still returns the
full bundle on demand.

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

1. `load_conversation(agent_id)` → append the new user turn.
2. If history exceeds `ROLLING_WINDOW + 1`, spawn a **background** summarization (never in
   front of the reply the user is waiting for).
3. `load_memory` → render `[Memory summary: …]` plus `- fact: value` lines into the system prompt.
4. `build_day_context` → append `## Last chat summary`, `## Today's context`, and
   `## Yesterday's context` (whichever exist).
5. Take the last `ROLLING_WINDOW` messages as the window.

So the system prompt contains: agent objective + **memory summary + facts** + **day context** +
tool note.

> The Manager path (`build_manager_system_prompt`, used by both `stream_chat` and
> `manager_turn`) injects the same summary + facts + day context. `recall_memory` still returns
> the full bundle on demand.

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

All three tiers now run for every agent, and all three are injected on every turn. The Manager
additionally has `recall_memory` for pulling the full bundle on demand.

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
| `stream_chat` | ✔ (last 4) | ✔ injected | ✔ injected |
| `manager_turn` (command + Telegram) | ✖ (single message) | ✔ injected | ✔ injected |
| `run_agent_once` (one-shot / workflow / task) | ✖ | skills only | ✖ |

## Wiring notes

- Sessions are keyed **per agent** (`useManagerStore.sessionIds`), so switching agents never
  attaches to another agent's session.
- Only the Manager has `recall_memory`; regular agents rely on the automatically injected day
  context instead.
- `manager_turn` (Telegram / `manager_message`) injects tier-2 facts + summary and the day
  context like the streaming path; it still has no rolling window (it receives a single message).
