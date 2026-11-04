# Routing — where a captured thing belongs

Three stores can hold something a user says, and they are not interchangeable. Before this
existed, agents held only `knowledge_base` and `remember`, so every preference, todo and reminder
was filed as a reference doc — the knowledge base is pull-only, so a rule like "never use emoji"
only applied if the model happened to search for it.

Owned by `src-tauri/src/routing.rs` (the contract + the deterministic reading) and
`src-tauri/src/tools/items.rs` (the destination that was missing).

Related: [memory.md](./memory.md) (the memory tiers), [lumin.md](./lumin.md) (items + scheduler),
[data-model.md](./data-model.md) (tables).

## The contract

`routing::ROUTING_RULES` is injected into every agent prompt (`chat.rs`) and the Manager prompt
(`manager.rs`), and mirrored into each tool's `description()` so tool choice and prose agree.

| Store | Holds | Scope | In the prompt? |
| --- | --- | --- | --- |
| `items` | anything with a **when or a do** — notes, todos, reminders, goals, resources, habits | global + `project_id` | no (the scheduler reads it) |
| `remember` | durable **facts and standing rules about the user** ("never use emoji", "prefers IST", "my name is Ranas") | rules: `global`; facts: `agent:<id>` | **yes** — injected every turn |
| `knowledge_base` | shared **reference material** any agent may need later — wiki, ICP, specs, policies, runbooks, product facts | global, shared | no — pull-only |

A reminder in `items` is scheduled and pushed to Telegram; a rule in `remember` is injected into
every future prompt and is exempt from retrieval decay; a knowledge-base doc is only read when the
model searches for it. That difference is why the destination matters.

## The reading

`routing::triage` runs **before the model is offered its tools** — in `chat.rs::stream_chat_inner`
and `manager.rs::manager_turn` — and files the turn when the reading is clear:

1. **Cues first** (`route()`): a never/always rule → memory as `policy`/`preference`; a reminder
   cue *with a time* → items as `reminder`; a todo cue → items as `todo`. A timed reminder needs
   its time — "remind me what we decided" is a message, not a schedule.
2. **Classifier second**: a turn with no cue is only sent to the `classifier` role when it looks
   like content wanting to be filed (`FILING_HINTS`), so an ordinary chat turn costs no extra
   model call. Every item kind is filed — `task`, `reminder`, `habit`, `goal`, `note`, `idea`
   and `event` land in items, `preference` in memory; `project`, `decision` and `person` are
   left to the model.
3. **Otherwise nothing is filed** and the model's own tools decide.

When the triage does file, the prompt says so ("This message was already filed as a reminder for
Nov 01 09:00 — do not create it again"), and the post-turn `is_explicit_remember` fast path is
skipped for that turn, so a rule is never written twice. Idempotence is the backstop either way:
items de-dupe on an exact active title, memory de-dupes on content within a scope.

## Rules are global

A standing rule is written by `memory::store_rule` at **`global` scope** with kind `policy`
(never/always) or `preference` (a taste). `agent_memory::scopes_for` inherits only
`<scope> + global`, so an agent-scoped rule would be invisible inside a project chat — the
injection path is `compile_context(scope = "project:<pid>")`. `remember` writes a rule globally
and a plain fact to the agent's own scope.

## Timezone

Every due date, reminder and relative time resolves in the user's configured zone
(`Settings → you & your assistant → timezone`), an IANA name defaulting to **`Asia/Kolkata`**
(`tz::DEFAULT_TZ`). `tz.rs` owns `zone()`, `resolve_clock()`, `resolve_date_time()` and
`format()`. Before this, `Identity.timezone` was an unused string and everything used
`chrono::Local` — the machine's zone, not the user's.

## The re-file pass

`refile.rs` migrates a knowledge base that already accumulated memories and todos. `preview`
reads every doc, routes it (`routing::classify_text` — cues, then the classifier), and returns a
proposal; nothing moves until the user approves. `apply` writes the memory or item and deletes the
doc. Surfaced as the **tidy** button in the gallery tab, and as the Manager's `tidy_knowledge`
tool (read-only — it returns the proposals).

## If you change…

| If you change… | Code | Doc |
| --- | --- | --- |
| What belongs where (a store, a kind) | `src-tauri/src/routing.rs` (`ROUTING_RULES`, `route`), the tool descriptions in `tools/items.rs` / `tools/knowledge.rs` | this file |
| The item kinds or the due parsing | `src-tauri/src/capture.rs`, `src-tauri/src/tools/items.rs` | [lumin.md](./lumin.md) |
| Rule scope or kinds | `src-tauri/src/memory.rs` (`store_rule`), `src-tauri/src/agent_memory.rs` (`MEMORY_KINDS`, `scopes_for`) | [memory.md](./memory.md) |
| The default zone or time resolution | `src-tauri/src/tz.rs`, `src/components/views/SettingsView.tsx` | this file |
