# Lumin — the second brain

**"Text your life. Lumin figures out the rest."**

Most productivity apps assume you're clear, focused and organised — so they add decision
overhead instead of removing it. Lumin assumes the opposite: you're tired, vague, acting on
impulse. You don't manage anything; you **text**, and the system builds the structure around
you. That is a port of **fren** (`A:\code\Projects\fren`).

This doc covers the parts that make that work: the extractor (the anchor every message passes
through), the items store, habits, reminders + the scheduler, and the learning layer.

Code: `src-tauri/src/capture.rs` (extractor + store), `reminders.rs` (scheduler),
`insights.rs` (learning), `telegram.rs` (commands + delivery). UI: `ItemsView.tsx`.
Data: `items` / `insights` (see [data-model.md](./data-model.md)).

## The anchor: the extractor

Every message goes through one interpreter before anything else. Short commands are exact and
never call a model; free text is extracted.

```
"remind me to call mom tonight"  →  { feature: "reminder", action: "create",
                                      params: { due_at: "…T21:00:00+05:00" } }
"i'll start gym tomorrow"        →  { feature: "promise",  action: "create" }
"what are my todos?"             →  { feature: "todo",     action: "read" }
"slept through it again"         →  { feature: null,       type: "chat" }
```

The schema (`EXTRACTOR` in `capture.rs`) fills: `type`, `feature`, `action`, `params`
(`title`, `body`, `url`, `tags`, `due_at`, `cadence`), `mood`, `intent_shape`,
`needs_guardrail`, `confidence`.

- **Relative times are resolved against "now"** — the prompt is given the current local
  timestamp, so "tonight" becomes an absolute instant. Nothing is stored as "tonight".
- **The parse is lenient and never loses a message**: a bad or failed reply degrades to a
  `note`, and `type: chat` means "nothing to file" (it says so rather than swallowing it).
- The request goes through the shared harness (`provider::resolve` → `http::send_model_request`),
  so every provider and Anthropic's wire format work as in chat.

## The items store

One `items` table for all six kinds:

| Kind | Meaning |
| --- | --- |
| `note` | An idea, thought, fact or snippet worth keeping. |
| `todo` | Something to do (no time). |
| `reminder` | A task **with** a time — the scheduler fires it. |
| `promise` | An explicit commitment to future behaviour. |
| `link` | A URL / resource. |
| `habit` | A recurring behaviour with a `cadence` and a `streak`. |

`#tags` become `tags` + the `collection`; a URL becomes `url`; `project_id` scopes an item to
a project (see [architecture.md](./architecture.md) for projects).

## Telegram

Commands work on **any** bot. A bot ticked **second brain** also files free text.

| Command | Effect |
| --- | --- |
| `/n <text>` | note · `/tdl` todo · `/p` promise · `/lib` link |
| `/r <text>` | reminder · `/r 18:30 call mom` sets an explicit time |
| `/habit <text>` | habit (cadence from "daily"/"weekly"/"monthly", default daily) |
| `/items [kind]` | newest 20, numbered · `/due` what's due now |
| `/done <n>` | toggle done — a habit bumps its streak and schedules the next occurrence |
| `/forget <n>` | delete · `/capture` the command list |
| `/insights` | new insights · `/accept <n>` accept one |

Filing runs *before* the agent and terminal paths, so capture never costs a model turn and —
like terminal sessions — never writes agent memory.

## Reminders + the scheduler

`reminders.rs` is the app's only time-based machinery: a **30 s tick** that

1. finds `items` where `kind IN ('reminder','habit')`, `status='active'`, `due_at <= now`,
   `fired_at IS NULL`;
2. messages the bot that captured it (its `owner_chat_id`, filled from the first authorized
   message) via `telegram::notify`;
3. emits a `lumin-reminder` Tauri event, which `App.tsx` toasts;
4. marks it fired, or for a `cadence` rolls `due_at` forward to the next occurrence.

**It is in-app**: it runs while the app is open (the tray keeps it alive), and a closed app
fires overdue items on the next launch. There is no OS-level service.

## The learning layer (bounded)

`insights.rs` aggregates what you actually file (counts by kind/status/tag, repeated titles,
recent items), asks a model for durable observations, and stores them in `insights` as
**proposals**:

- `goal` / `preference` / `pattern` — mirrored into native memory (`am_memories`,
  `am_self_model`) so agents can recall them.
- `habit_suggestion` — a title that keeps repeating; accepting it creates the habit.

It **never** rewrites habits or reschedules reminders on its own — `accept`/`dismiss` is
yours. It runs on a 30-minute loop, on demand (`reflect_now` / **reflect now** in the Items
view), and nothing is proposed when the data doesn't support it.

## Identity

Settings → general → **you & your assistant** (`Identity` in `app_settings.rs`): your name,
the bot's name, a persona and tone. These are appended to every system prompt
(`identity_prompt`), so the assistant sounds like yours. `timezone` is informational — times
use the system zone.

## Not ported

Billing / Clerk / star transactions, the Cloudflare D1+KV deployment, fren's Next.js settings
app, and the `lab`/`brainstorm` session type (lup has chat sessions and projects instead).
