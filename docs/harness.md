# The Harness

The **harness** is the model-request layer: everything between "the app wants an LLM to do
something" and "a provider returned text (or tool calls)". It is shared by every surface —
agent chat, one-shot runs, workflows, tasks, the Manager, Telegram, and memory
summarization. Provider dispatch, credentials and request parameters live in **one place**
(`provider.rs`), as does the transport (`http.rs`). The five call paths below still differ in
streaming vs. non-streaming and in how they run tools; this doc maps them and where they
still diverge.

Related: [tool-calls.md](./tool-calls.md) (tool execution), [memory.md](./memory.md)
(context assembly), [data-model.md](./data-model.md) (`runs`).

## The call paths

There are five distinct places that build a request and call a provider:

| # | Path | Entry / caller | Transport | Tools | Provider dispatch |
| --- | --- | --- | --- | --- | --- |
| 1 | `agents::run_agent_once_structured` | `execute_agent` (command), `workflows.rs` (agent/subagent nodes), `tasks.rs` (delegation) | Non-streaming | Yes (`build_tools`) | `provider::resolve` |
| 2 | `chat::stream_chat` | `stream_chat` (command), used by agent chat + Manager chat | **Streaming** (tool rounds are non-streaming) | Yes (agent tools, or `manager_tools`) | `provider::resolve` |
| 3 | `manager::manager_turn` | `manager_message` (command), `telegram.rs` | Non-streaming | Yes (`manager_tools`) | `provider::resolve` |
| 4 | `memory::one_shot_completion` | `summarize_old_turns`, `close_chat_session` | Non-streaming | No | `provider::resolve` — no reasoning |
| 5 | `workflows::run_llm_judge` | checker nodes | Non-streaming | No | `provider::resolve` — no reasoning |

All five resolve their endpoint, credentials and request parameters through `provider.rs`, so
there is a single `ollama:` / `groq:` / `openrouter:` branch in the codebase. What remains
divergent is transport and tool handling. Path **2** is the streaming reference
implementation.

### Path 1 — one-shot agent run (`agents.rs`)

`execute_agent` records a `runs` row (`status='running'`), then calls
`run_agent_once_structured(..., structured=true)`, which:

1. Ensures the agent home (`{app_data}/agents/{id}/{files,memory,runs,outputs}`) and writes `config.json`.
2. Renders attached skills via `skills_prompt`.
3. Builds the prompt (structured = JSON `{summary, result}` envelope, non-structured = plain answer).
4. Resolves the provider via `provider::resolve`, then branches by transport:
   - Ollama → with tools `run_ollama_chat`, without tools `POST /api/generate`.
   - Groq / OpenRouter → with tools `run_openai_tool_chat`, without tools a plain chat completion.
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

## The shared layer: `provider.rs`

`provider::resolve(conn, "<provider>:<model>", override_key)` is the single entry point for
working out *where* a request goes and *how* it should be configured. It returns:

| Field | Meaning |
| --- | --- |
| `kind` | `Ollama` / `Groq` / `OpenRouter` — the transport family |
| `base` | For Ollama, the model's stored `host` (that UI field is honoured, not ignored); otherwise the provider's completions URL |
| `model` | The bare model name the provider expects — our prefix stripped |
| `key` | API key for cloud providers; `None` for Ollama |
| `reasoning` | The model's configured reasoning level (see *Reasoning control*) |

An unrecognised prefix is an error rather than a silent fall-through to OpenRouter.

`apply_reasoning` and `apply_ollama_options` mutate the request body before it is sent; see
*Reasoning control* and *Token/latency tuning*.

## The shared layer: `http.rs`

Every network call goes through these helpers.

| Item | Value | Purpose |
| --- | --- | --- |
| `CONNECT_TIMEOUT` | 15s | Connect phase for every request. |
| `REQUEST_TIMEOUT` | 180s | Total timeout for non-streaming requests. |
| `STREAM_IDLE_TIMEOUT` | 90s | Per-chunk idle cap while streaming. |
| `CHAT_MAX_TOKENS` | 2048 | Default `max_tokens` for chat. |
| `REASONING_MAX_TOKENS` | 8192 | `max_tokens` when reasoning is enabled — the trace bills against the same budget. |
| `SUMMARY_MAX_TOKENS` | 400 | Default for summarization / judge. |
| `MODEL_ATTEMPTS` | 4 | Attempts before giving up. |
| `client()` | `reqwest::Client` | Connect + total timeout — for awaited responses. |
| `stream_client()` | `reqwest::Client` | Connect timeout only — for streams (generation may legitimately run long). |
| `send_with_retry(f, attempts)` | — | Retries `429 / 502 / 503 / 504`; honours `Retry-After` (capped 30s), else `800ms · 2^attempt`; otherwise returns `Model provider returned {status}`. |
| `send_model_request(client, resolved, url, body, attempts)` | — | The one send path for model calls. Adds the `Authorization` header and OpenRouter's `HTTP-Referer` / `X-Title`, wraps `send_with_retry`, and retries once without reasoning parameters if the body is rejected with 400. |
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

API keys are resolved inside `provider::resolve`, from the model's `api_key` column — never
sent from the UI for stored models. The browser preview may pass one explicitly to
`execute_agent`; it wins over the stored key.

## Reasoning control

Each model carries a `reasoning` setting (`model_configs.reasoning`): `auto` | `off` | `low` |
`medium` | `high`. `provider::apply_reasoning` maps it onto the provider's own field and
returns the output budget to use.

| Setting | Ollama `think` | OpenRouter `reasoning` | Groq `reasoning_effort` |
| --- | --- | --- | --- |
| `auto` (default) | omitted | omitted | omitted |
| `off` | `false` | `{ "enabled": false }` | `"none"` |
| `low` / `medium` / `high` | the level string | `{ "effort": <level>, "exclude": true }` | the level string |

- **`auto` writes nothing.** Thinking is *on by default* in both Ollama and Groq for models
  that support it, so a model with no reasoning support is never sent a parameter it would
  reject.
- **Reasoning tokens bill against `max_tokens`.** At `CHAT_MAX_TOKENS = 2048` a model can
  spend the whole budget thinking and return empty content, so an enabled level raises the
  budget to `REASONING_MAX_TOKENS` for that request.
- **The trace is discarded.** Ollama returns it as `message.thinking`, separate from
  `message.content`, and only `content` is read — reasoning never reaches a chat bubble.
- **Utility calls opt out.** `memory::one_shot_completion` and `workflows::run_llm_judge`
  deliberately skip reasoning: they are cheap, frequent calls that gain nothing from it.
- **Unsupported flags degrade instead of failing.** If a provider rejects the body with 400
  and reasoning was set, `send_model_request` retries once without it — which is what makes
  `off` safe to try on a model whose reasoning is mandatory.

## Token/latency tuning

Ollama accepts no tuning fields by default, so every local turn ran with an unbounded output
budget and a model that unloaded itself after a few idle minutes. `provider::apply_ollama_options`
now sets:

| Field | Value | Why |
| --- | --- | --- |
| `keep_alive` | `30m` | Without it Ollama unloads the model after its idle window, so the next turn pays a multi-second reload. |
| `options.num_predict` | the request's token budget | Ollama's default is unlimited, while every cloud path caps output at `max_tokens`; this makes the local path bounded too. |

`num_ctx` is deliberately **not** set: too small a value would truncate the Manager's large
tool-schema prompt, and the app has no reliable way to know the model's window.

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

1. ~~**Provider prefix dispatch is duplicated 4×.**~~ **Done** — `provider::resolve` is the
   single dispatch point for all five paths, and it also owns request parameters (reasoning,
   Ollama tuning) and the provider headers via `http::send_model_request`.
2. **The tool loop is duplicated.** `run_ollama_chat` + `run_openai_tool_chat` (path 1) and
   the loops in `manager_turn` (path 3) re-implement what `stream_chat` already does for
   both shapes. One loop, parameterized by a stream/non-stream sink, would cover all of them.
3. **Response-shape parsing is re-derives per call site.** The "Ollama first, then
   `choices[0]`" fallback appears in at least four places; it belongs in one adapter.
4. **Token extraction differs per provider and per path.** Centralize in the adapter so
   every path records counts the same way.
5. ~~**`apply_openai_defaults` callers pass `is_openrouter` inconsistently**~~ **Done** —
   every caller now derives it from `resolved.kind`, so the Manager path no longer guesses
   with `!is_groq`.

## Adding a provider

If the provider is OpenAI-compatible: add one branch to `provider::resolve` in `provider.rs`
returning a `Resolved { kind, base, model, key, reasoning }`, add the variant to `Kind`, and
handle it in `apply_reasoning`. Nothing else in the backend changes — all five paths pick it
up. Non-compatible providers also need a response-shape adapter where replies are parsed.
Always update the endpoint table above.

## Quick reference

- Tool-round cap: `MAX_TOOL_ROUNDS = 5` (`tools/mod.rs`).
- Tool result cap: `clip()` truncates at 8000 chars (`tools/mod.rs`).
- Idle stream timeout message: "The model provider stopped responding (no data for 90s)."
- Retryable statuses: `429, 502, 503, 504`.
- Reasoning output budget: `REASONING_MAX_TOKENS = 8192` (`http.rs`).
- Ollama `keep_alive`: 30m; `num_predict` is set to the request's token budget.
- Reasoning setting values: `auto` | `off` | `low` | `medium` | `high` (`model_configs.reasoning`).
