# The Harness

The **harness** is the model-request layer: everything between "the app wants an LLM to do
something" and "a provider returned text (or tool calls)". It is shared by every surface —
agent chat, one-shot runs, workflows, tasks, the Manager, Telegram, and memory
summarization — but today it is **not implemented in one place**. This doc maps the paths,
the shared layer, and where they still diverge so they can be streamlined.

Related: [tool-calls.md](./tool-calls.md) (tool execution), [memory.md](./memory.md)
(context assembly), [data-model.md](./data-model.md) (`runs`).

## The call paths

There are five distinct places that build a request and call a provider:

| # | Path | Entry / caller | Transport | Tools | Provider branches |
| --- | --- | --- | --- | --- | --- |
| 1 | `agents::run_agent_once_structured` | `execute_agent` (command), `workflows.rs` (agent/subagent nodes), `tasks.rs` (delegation) | Non-streaming | Yes (`build_tools`) | 3 hand-written branches |
| 2 | `chat::stream_chat` | `stream_chat` (command), used by agent chat + Manager chat | **Streaming** (tool rounds are non-streaming) | Yes (agent tools, or `manager_tools`) | 1 unified branch |
| 3 | `manager::manager_turn` | `manager_message` (command), `telegram.rs` | Non-streaming | Yes (`manager_tools`) | 3 hand-written branches |
| 4 | `memory::one_shot_completion` | `summarize_old_turns`, `close_chat_session` | Non-streaming | No | 3 hand-written branches |
| 5 | `workflows::run_llm_judge` | checker nodes | Non-streaming | No | 3 hand-written branches |

Path **2** is the only one with a single unified provider abstraction. Paths **1, 3, 4, 5**
each re-implement the same three-way (`ollama:` / `groq:` / `openrouter:`) prefix dispatch.
This is the duplication the harness should collapse — see *Streamline targets* below.

### Path 1 — one-shot agent run (`agents.rs`)

`execute_agent` records a `runs` row (`status='running'`), then calls
`run_agent_once_structured(..., structured=true)`, which:

1. Ensures the agent home (`{app_data}/agents/{id}/{files,memory,runs,outputs}`) and writes `config.json`.
2. Renders attached skills via `skills_prompt`.
3. Builds the prompt (structured = JSON `{summary, result}` envelope, non-structured = plain answer).
4. Branches by model prefix:
   - `ollama:` → with tools `run_ollama_chat`, without tools `POST /api/generate`.
   - `openrouter:` / `groq:` → with tools `run_openai_tool_chat`, without tools a plain chat completion.
5. Writes `outputs/{run_id}.txt`, updates the `runs` row to `completed` with token counts (or `failed` on error).

`run_ollama_chat` and `run_openai_tool_chat` are a forked pair with the same shape: a
`MAX_TOOL_ROUNDS` loop, execute each call, append a tool message, return the first
non-tool content. They differ only in request/message encoding.

### Path 2 — streaming chat (`chat.rs`)

The reference implementation. It:
1. Loads conversation history, appends the user turn (rolling window; see [memory.md](./memory.md)).
2. Builds the system prompt (Manager prompt, or agent prompt + memory facts).
3. Runs the **tool-call loop** with `stream: false` rounds — this loop already handles both
   Ollama and OpenAI-compatible response shapes in one branch.
4. Streams the final answer (`stream: true`) and emits deltas over a Tauri `Channel`.
5. Persists the `runs` row and the conversation (+ chat session).

### Path 3 — Manager turn (`manager.rs`)

Near-duplicate of path 1's tool loop, specialized for `manager_tools()` and
`dispatch_manager_tool`. It supports both response shapes and both providers, but as its
own loop. `run_command` is explicitly refused here because this path has no confirmation
popup (the streaming path does).

## The shared layer: `http.rs`

Every network call goes through these helpers.

| Item | Value | Purpose |
| --- | --- | --- |
| `CONNECT_TIMEOUT` | 15s | Connect phase for every request. |
| `REQUEST_TIMEOUT` | 180s | Total timeout for non-streaming requests. |
| `STREAM_IDLE_TIMEOUT` | 90s | Per-chunk idle cap while streaming. |
| `CHAT_MAX_TOKENS` | 2048 | Default `max_tokens` for chat. |
| `SUMMARY_MAX_TOKENS` | 400 | Default for summarization / judge. |
| `MODEL_ATTEMPTS` | 4 | Attempts before giving up. |
| `client()` | `reqwest::Client` | Connect + total timeout — for awaited responses. |
| `stream_client()` | `reqwest::Client` | Connect timeout only — for streams (generation may legitimately run long). |
| `send_with_retry(f, attempts)` | — | Retries `429 / 502 / 503 / 504`; honours `Retry-After` (capped 30s), else `800ms · 2^attempt`; otherwise returns `Model provider returned {status}`. |
| `apply_openai_defaults(body, is_openrouter, max_tokens)` | — | Inserts `max_tokens` if absent; for OpenRouter adds `provider: {sort: "throughput"}` to avoid its slow default route. |

Endpoints in use:

| Provider | Endpoint |
| --- | --- |
| Ollama chat | `http://127.0.0.1:11434/api/chat` |
| Ollama generate | `http://127.0.0.1:11434/api/generate` |
| Groq | `https://api.groq.com/openai/v1/chat/completions` |
| OpenRouter | `https://openrouter.ai/api/v1/chat/completions` |

OpenRouter requests also send `HTTP-Referer: https://local-agent-os.app` and
`X-Title: Local Agent OS`.

## Provider message shapes

The two families differ in three places; every path must normalize all three:

| Concern | Ollama | OpenAI-compatible (Groq / OpenRouter) |
| --- | --- | --- |
| Tool calls | `message.tool_calls[]`, `function.arguments` is an **object** | `choices[0].message.tool_calls[]`, `function.arguments` is a **JSON string** |
| Final text | `message.content` | `choices[0].message.content` |
| Token counts | `prompt_eval_count` / `eval_count` | `usage.prompt_tokens` / `usage.completion_tokens` |

API keys are resolved by `storage::stored_api_key(conn, "<provider>:<model>")` — never sent
from the UI for stored models.

## Streaming protocol

Handled in `chat::stream_chat`:

- **Ollama** emits NDJSON — one JSON object per line; read `message.content` per line.
- **Groq / OpenRouter** emit SSE `data:` lines; ignore `[DONE]` and OpenRouter's
  `: OPENROUTER PROCESSING` keep-alives.
- Buffering: chunks are appended to a `String` and drained on `\n` so partial lines never
  break parsing.
- Usage: the stream requests usage on the final chunk (`stream_options.include_usage` for
  Groq, `usage.include` for OpenRouter) so the recorded run has real token counts.
- Delta delivery: plain-text deltas go over the channel; structured events are JSON strings
  (see the confirmation gate in [tool-calls.md](./tool-calls.md)).

## Token accounting & run recording

| Path | Where counted | Persisted |
| --- | --- | --- |
| `execute_agent` | summed across tool rounds in `run_*_chat` | `runs.prompt_tokens` / `completion_tokens` on completion |
| `stream_chat` | accumulated from stream usage | inserted into `runs` at the end |
| `execute_workflow` | sums only agent/subagent node tokens | **per-step token fields are always 0** — only the workflow totals are populated |
| Manager turn / summarization / judge | not persisted as runs | — |

## Streamline targets

These are the concrete divergences to collapse when unifying the harness (no behavior change
intended — only removing the second, third, and fourth copies):

1. **Provider prefix dispatch is duplicated 4×.** `run_agent_once_structured`,
   `manager_turn`, `one_shot_completion`, and `run_llm_judge` each hand-roll the
   `ollama:` / `groq:` / `openrouter:` branch. A single `ProviderRequest` (endpoint, auth,
   body, response-shape adapter) would leave one branch.
2. **The tool loop is duplicated.** `run_ollama_chat` + `run_openai_tool_chat` (path 1) and
   the loops in `manager_turn` (path 3) re-implement what `stream_chat` already does for
   both shapes. One loop, parameterized by a stream/non-stream sink, would cover all of them.
3. **Response-shape parsing is re-derives per call site.** The "Ollama first, then
   `choices[0]`" fallback appears in at least four places; it belongs in one adapter.
4. **Token extraction differs per provider and per path.** Centralize in the adapter so
   every path records counts the same way.
5. **`apply_openai_defaults` callers pass `is_openrouter` inconsistently** (e.g. the Manager
   path uses `!is_groq`). Deriving it from the resolved provider removes the guesswork.

## Adding a provider

If the provider is OpenAI-compatible, the minimal change is: add a prefix branch where
`stored_api_key` is resolved and a `(url, model, key, is_openrouter)` tuple is produced
(today: `chat.rs`, `agents.rs`, `manager.rs`, `memory.rs`). Non-compatible providers need a
response adapter as well. Always update the endpoint table above.

## Quick reference

- Tool-round cap: `MAX_TOOL_ROUNDS = 5` (`tools/mod.rs`).
- Tool result cap: `clip()` truncates at 8000 chars (`tools/mod.rs`).
- Idle stream timeout message: "The model provider stopped responding (no data for 90s)."
- Retryable statuses: `429, 502, 503, 504`.
