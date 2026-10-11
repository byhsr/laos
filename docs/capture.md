# Capture — the second brain

File notes, todos, reminders, promises and links from Telegram (or the panel) into one
`items` store. Ported from **fren** (`A:\code\Projects\fren`), where the same flow drove a
Telegram bot. Code: `src-tauri/src/capture.rs`; data: the `items` table (see
[data-model.md](./data-model.md)); UI: `src/components/views/ItemsView.tsx`.

## Kinds

| Kind | Meaning |
| --- | --- |
| `note` | An idea, thought, fact or snippet worth keeping. |
| `todo` | Something the user intends to do (no time). |
| `reminder` | A task with a time. |
| `promise` | An explicit commitment to future behaviour. |
| `link` | A URL / resource to save. |

## Telegram

Every bot understands the capture commands; a bot marked **second brain** also files free
text.

| Command | Effect |
| --- | --- |
| `/n <text>` | Save a note. |
| `/tdl <text>` | Save a todo. |
| `/r <text>` | Save a reminder. |
| `/p <text>` | Save a promise. |
| `/lib <url> <text>` | Save a link. |
| `/items [kind]` | List the newest 20 (optionally one kind), numbered. |
| `/done <n>` | Toggle done on item *n* from `/items`. |
| `/forget <n>` | Delete item *n*. |
| `/capture` | Print the command list. |

Free text sent to a **second brain** bot is classified into one of the five kinds and filed;
the bot replies `Saved <kind> — <title>`.

**Enabling it:** Telegram → bots → edit a bot → tick **second brain — file free text as
items**. The bot row then shows a `· capture` marker. A bot keeps its agent/terminal target
for everything that isn't capture.

Capture runs *before* the agent and terminal paths in `route_bot_message`, so filing a note
never costs a model turn for the request itself, and — like terminal sessions — it never
writes agent memory.

## Classification

`capture::remember` asks the first enabled model (`model_configs.enabled=1`, ordered by id)
to pick a kind, using a trimmed version of fren's **LuminCore** extractor prompt: a strict
JSON reply `{"feature":"note|todo|reminder|promise|collection|null"}`. The reply is parsed
leniently (code fences stripped, unknown → `note`); no model or a failed call also falls
back to `note`, so capture never loses a message.

The request goes through the shared harness (`provider::resolve` → `http::send_model_request`),
so every provider and the Anthropic wire translation work the same as chat.

## Panel

**Items** (rail) lists what was captured: filter by kind, file an item by hand, tick it done,
or delete it. Items saved from the panel are tagged `source: "app"`; from Telegram, the bot
name.

## Limits / not ported

fren has more than capture — deliberately left out for now:

- **Billing / Clerk auth** (`billing`, `star_transactions`, `clerkAuth`) — lup is local-first
  and single-user, so accounts and paywalls have no equivalent.
- **Habits, lab/brainstorm sessions, collections** — the fren schema has these (`habits`,
  `sessions`, `session_messages`); only the five capture kinds are wired here.
- **Reminder scheduling** — a reminder is stored as an item but nothing fires it yet;
  there is no scheduler in lup.
- **Per-user persona/tone config** (`userConfig.ts`) — lup has per-agent personas instead.
