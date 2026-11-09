# Dev log

One entry per working session — what changed, why, and what is still open. Newest first.

**Read this before starting a session.** The tree is deliberately not committed between
sessions, so "uncommitted" below means *carried forward*, not *lost*. For where to start
digging on a given concern, see the **Where to dive deep** table in
[ORCHESTRATION.md](./ORCHESTRATION.md).

---

## 2026-11-09 — release 0.2.7

Ships the working stop control for background tasks, the readable / full-window past-chat
history, and the diagnosable model-response errors (all above). Version `0.2.6` → `0.2.7`
across `package.json`, `src-tauri/Cargo.toml`, `src-tauri/Cargo.lock` and
`src-tauri/tauri.conf.json`; tag `v0.2.7` drives the release workflow.

---

## 2026-11-09 — stoppable jobs, readable chat history, clearer model errors

- **A stopped task actually stops.** `cancel_task` used to only flip the DB row — the
  background future kept running and `finish()` then overwrote `cancelled` back to
  `completed`, filed the result and fired the follow-up. Now a cancel is recorded in a
  registry that an agent task threads through `run_agent_once_structured` (checked between
  tool rounds) so it aborts, and `finish()` refuses to overwrite a cancelled row, file a
  result, notify or run a follow-up. The activity card now starts a drag only from its grip
  handle, so a click on the stop button is no longer swallowed by a drag gesture.
- **Past chats read as something other than "Chat".** A regular agent's session was created
  as `"Chat"` and never renamed (only the Manager renamed on the first message); the agent's
  `send` now names a fresh session from its first user message. And clicking a past chat
  opens it **in the main window** and makes it the active session, so it reads full-size and
  the next message continues it in place (`AgentWindow` and `ManagerView`) — replacing the
  cramped side-panel preview. The Manager list shows real titles instead of "current chat".
- **A non-JSON provider response is diagnosable.** New `http::json_body` turns a body that
  isn't JSON — an HTML block page, an empty 200, a truncated response — into
  `The response was not valid JSON (200): … Body: …` instead of reqwest's bare
  "error decoding response body". Every provider/model decode site now goes through it
  (`chat`, `agents`, `manager`, `memory`, `classify`, `insights`, `embed`, `workflows`,
  `media`, `transcribe`, `integrations`).

**Verified:** `npm run build` — clean; `cargo test` — **101 pass** (was 98; +1 task-cancel
registry, +2 `http::json_body`).

---

## 2026-11-09 — release 0.2.6

Ships the artifacts menu, the drag-to-split row and the Manager-as-pane, the
delegated-task conversation context, and the two notification / activity-panel
tweaks (all above). Version `0.2.5` → `0.2.6` across `package.json`,
`src-tauri/Cargo.toml`, `src-tauri/Cargo.lock` and `src-tauri/tauri.conf.json`;
tag `v0.2.6` drives the release workflow.

---

## 2026-11-09 — artifacts menu, drag-to-split, the Manager as a pane

The chat document surface was rebuilt and the split view learned to accept a drag.

- **Artifacts live behind one control, not inline in the conversation.** The inline
  `DocumentsSection` card list is gone (`src/components/chat/DocumentsSection.tsx` deleted).
  A new `ArtifactsMenu` sits in the chat header (both `AgentWindow` and `ManagerView`): a
  dropdown of the chat's documents, one opening in a `SidePanel` to read, copy or download.
  Same store and the same live `lumin-document-changed` refresh as before.
- **Keep a reply as an artifact.** A new `create_document` command (`documents.rs`,
  registered in `main.rs`) saves a reply's text into the same `documents` store the
  `document` tool writes to. Every assistant message's action row gained "keep as artifact"
  (`BookmarkPlus`); a copy button was added to every message's row as well.
- **Drag an agent into the split row.** New `useAgentDrag` store — a zustand store, not a
  `dataTransfer` payload, because WebView2's drag timing makes `dataTransfer` unreliable and
  both the drag sources and the drop targets need to read it. Sidebar rows and the agents
  grid cards are draggable; dropping on a `DropGap` between panes, on a pane itself, or on
  the full-view overlay (when a non-agent view is up) opens the agent at that slot.
  `dropAgentAt` inserts, moves or replaces while keeping the three-pane cap.
- **The Manager is a pane like any other.** `agentMenuItems` no longer excludes the lead
  agent from "open in split view", and `ManagerView` gained `agent` / `docked` props so it
  can render as a split pane while the root `manager` view keeps its own instance. Only the
  focused pane docks the (global) activity panel.
- **Delegated work carries its conversation.** `manager::dispatch_manager_tool`'s
  `delegate_task` / `create_task` now prepends the surrounding chat transcript (newest turns
  first, system turns skipped, trimmed to 4000 chars) to the task context, so the assigned
  agent understands the situation rather than the bare task. The transcript feeds only the
  task's prompt (`tasks::run_agent_task`) — it is never written to `chat_messages`. Two
  tests: `transcript_skips_system_turns_and_keeps_order`, `transcript_respects_the_char_budget`.
- **Two behavior tweaks.** A backgrounded **tool** task no longer raises an OS notification
  — only a delegated agent task does, since a tool call would fire one on nearly every turn;
  `tasks::finish` takes the task `kind` to decide. And the activity panel no longer
  auto-opens for a background job: it is off until the user toggles it (the `userClosed` /
  `openPanel` state is gone), so nothing re-opens a panel the user closed.

**Verified:** `npm run build` — `tsc -b` + `vite build` clean (only the pre-existing
lottie-eval and chunk-size warnings); `cargo test` — **98 pass** (was 96; +2 transcript).

**Open / to confirm at runtime:** the artifacts menu, "keep", drag-to-split, the
Manager-as-pane and the delegated transcript are code + build only — no GUI run this
session. `src-tauri/listenlabs_search.html` (a saved DuckDuckGo page) stays untracked and
was left alone, as does the `next-iteration.md` brief.

---

## 2026-11-05 — release 0.2.5

Ships the chat documents, the stoppable jobs, the live step text, the usage view and the
native-app polish (all below). Version `0.2.4` → `0.2.5` across `package.json`,
`src-tauri/Cargo.toml`, `src-tauri/Cargo.lock` and `src-tauri/tauri.conf.json`; tag `v0.2.5`
drives the release workflow.

---

## 2026-11-05 — live step text, downloadable documents, stoppable jobs

- **The chat bar leads with the abstracted step, not the placeholder.** `StreamIndicator` now
  headlines the latest concrete step (an action, the write phase, a wait) and falls back to a `think`
  line only when that is all there is — so it reports "Running a command" / "Searching files" as they
  happen instead of sitting on "Working out the next step". The model's raw reasoning stays in the
  trace (the chevron) and the activity panel; nothing extra is streamed. The activity card's running
  line uses the same preference so the two read alike.
- **Documents ("artifacts").** New `documents.rs` (store + `list_documents` / `get_document` /
  `delete_document`, `documents` table) and `tools/documents.rs` (the `document` tool:
  create/update/read/list/delete, `markdown` | `text` | `html`). Every agent and the Manager hold it.
  Documents appear inline in the chat (`DocumentsSection`, mounted in both `AgentWindow` and
  `ManagerView`) as a card the user can expand to read, copy, or download as `.md` / `.txt` / `.html`;
  the chat refreshes live on the new `lumin-document-changed` event.
- **Stop a running job.** A running card in the activity panel now carries a stop button — a
  background task through `cancel_task`, a live turn through the chat's own cancel.

**Verified:** `npm run build` — `tsc -b` + `vite build` clean; `cargo test` — **96 pass**.

---

## 2026-11-05 — usage analytics: million-scale tokens + a full usage view

- **Readouts step up past a thousand.** New `src/format.ts` (`fmtCompact` / `fmtFull`): 1 237 000 now
  reads as `1.24M` (then `B`) instead of the old `1237.0k`. The Home token cell and its in/out split
  use it.
- **A full usage view.** The Home token cell gained a `usage` button opening a new `UsageView`
  (`usage` view, so it lives in the nav history). One filter set — range (7d / 30d / 90d / all),
  metric (total / in / out), agent, model — drives three charts: a daily stacked in/out bar chart
  (auto-bucketed past 92 bars, with a hover readout), a summary strip (total · in · out · runs ·
  avg/run), and ranked by-agent / by-model breakdowns. Neutral palette only — bars are
  `foreground/30` + `foreground/70`, exact values behind Tooltips, no accent, no native `title`.

**Verified:** `npm run build` — `tsc -b` + `vite build` clean.

---

## 2026-11-05 — native-app polish: links, notes, split view, delegation

- **Links open in the system browser.** Clicking a link the app renders (a URL in a chat reply, an
  item's resource link) navigated the main webview, replacing the whole app with the page. `App.tsx`
  now intercepts `click`/`auxclick` on `http(s)`/`mailto` anchors and hands them to `openUrl`. In
  production the webview's native right-click menu is also suppressed (editable fields exempt, so
  paste still works) so the window reads as a native app, not a browser.
- **Tasks no longer land in notes.** `routing::classifier_route` no longer maps `task` to Items, and
  the first-person `i need to / i have to / i must` cues are gone from `TODO_CUE` — a task or
  instruction is work to perform, not a captured note. Explicit "add a todo" / "remind me…" / notes /
  ideas / goals / habits still file. `ROUTING_RULES` says so in as many words, for the model path too.
- **"open in split view" in every agent's context menu.** A plain open now navigates within the
  current column (`showAgent`); the new menu item adds a window beside it (`splitAgent`). Shared by
  the sidebar and the agents browser through `agentMenuItems`.
- **Delegated work is visible where it runs.** A job the Manager hands to another agent now carries
  its `assignedAgent` on the activity item (so it shows in that agent's panel *and* the requester's),
  the assigned agent's chat gets the task and its result, and its window is brought forward instead of
  opening hidden behind the Manager view.

**Verified:** `npm run build` — `tsc -b` + `vite build` clean (only the pre-existing lottie-eval and
chunk-size warnings); `cargo test` — **96 pass** (was 95; `a_task_statement_is_not_routed` +
`an_explicit_todo_still_routes_to_items` replace the old `a_todo_routes_to_items`).

---

## 2026-11-04 — release 0.2.4

Ships the custom API tool suites, the agent scratchpad and the split view (all below). Version
`0.2.3` → `0.2.4` across `package.json`, `src-tauri/Cargo.toml`, `src-tauri/Cargo.lock` and
`src-tauri/tauri.conf.json`; tag `v0.2.4` drives the release workflow.

---

## 2026-11-04 — custom API tool suites, agent scratchpad, split view

Base: uncommitted on `8785eb9`. The custom-tool layer grew into a real "integration in a row",
the agent got its own private pad, and the agent windows learned to sit side by side.

### 1. Custom API tools become suites

- **An `api` row can be a suite.** A config with a `baseUrl` and an `actions[]` list (plus shared
  `headers`/`auth` and an `approval` setting) expands at build time into one `ApiTool` per action,
  named `<tool>_<action>` through `unique_name`, so two suites exposing the same action name don't
  collide. A row *without* `actions` is the original single-endpoint tool, unchanged. The expansion
  is `agents::api_tools_from_config`, extracted so the runtime, the Manager prompt and the test
  button all build from the same code rather than three lookalikes.
- **Params route themselves.** A parameter's `in` (`path | query | body | header`; absent = the
  legacy raw `{name}` substitution) decides where it lands — path values are percent-encoded into
  their segment so a `/` or space can't break out, query values go through `Url::query_pairs_mut`
  so they're encoded (and a value already written into the URL isn't appended twice), and body
  values keep their JSON type. `fill_template` now only treats *declared* params as placeholders,
  so a JSON body's structural braces survive (the old "any brace is a placeholder" check rejected
  every JSON body ever configured).
- **Auth + approval.** `auth` is `none | bearer | header | query`, its secret resolved from the
  vault under `tool/{id}/auth`; `save_tool` masks it like the header secrets and a deleted tool
  drops it. A configured credential that's no longer stored becomes `Missing`, so the call fails
  with something actionable instead of going out unauthenticated. `approval`
  (`never | writes | always`) decides when it asks first — `writes` gates
  `POST | PUT | PATCH | DELETE`, `never` gates nothing, `always` gates everything.
- **The Manager can author tools.** New `create_tool` Manager tool: `manager::model_tool_config`
  strips any credential the model tried to write (it keeps the auth *shape*, drops the value, and
  removes secret-looking static headers) and sanitises the name; the config is built through the
  real builder and rejected if it yields nothing callable; the tool is saved and optionally
  attached to an agent in the same call (nothing lets the model read an agent's current toolIds
  back to append to them). `storage::test_tool` + a ToolsView test button run one action straight
  from the form.
- **The Manager chat path can call custom tools.** `agents::custom_tools_for` is now extended into
  `chat::stream_chat_inner` and `manager_turn` (and listed in the Manager prompt), so an API tool
  attached to the lead agent is callable rather than a dead row.

### 2. Agent scratchpad

- **`scratchpad.rs`** *(new)* + **`tools/scratchpad.rs`** *(new)*: a per-chat private pad
  (`read` / `append` / `write` / `clear`), scoped `session:<id>` for a chat and `agent:<id>` for a
  session-less turn, trimmed from the front past 24k chars. It's the answer to "keep your working
  notes out of the user's notes" — `routing.rs` rules and the `items` tool description now say so
  in as many words.
- To make it per-session, `AgentTool` gained **`run_ctx`** with a `ToolCtx { agent_id, session_id }`
  (the default drops the ctx and defers to `run_with_images`, so every other tool is untouched).
  `chat::execute_tool` passes the ctx, and `tasks::run_tool_task` threads the originating session so
  a backgrounded call still has it. The turn's system prompt carries the pad's tail
  (`scratchpad::block`), and the UI got a read-only peek panel in both `AgentWindow` and
  `ManagerView` via a new `get_scratchpad` command.

### 3. Agent windows side by side

- `App.tsx` renders up to **three** agent panes at once; a fourth takes over the focused pane.
  Focus, the close button and the identity strip (`PaneBar`) only exist once the row is wider than
  one, so a single window looks exactly as it always has; only the focused pane docks the (global)
  activity panel. `AgentWindow` gained `docked` and `onNavigate` props.
- **The Manager creates and deletes agents from a chat turn**, but the roster only loaded at
  startup, so a new `lumin-agents-changed` event + `onAgentsChanged` reload it (and open the new
  agent's window). Work delegated to another agent (`assignedAgent` on the task start/finish
  events) also opens that agent's window.

### 4. Small fixes

- `runtime.ts` event listeners were registered with a race — unsubscribing before `listen()`
  resolved leaked the listener. All of them now carry a `disposed` flag.
- The tool picker shows `<name> (N actions)` for a suite; `AgentWindow` re-reads the roster and the
  tool list when the config panel opens (the Manager can rewrite either mid-turn), and the Workshop
  tools tab re-reads tools when opened.
- `App.tsx` handles each `lumin-task-finished` once (a `handledTasks` ref), so one finished job
  can't trigger two follow-up summaries.

**Verified:** `cargo test` — **95 pass** (was 78; +4 `agents::api_tool_tests`, +4
`manager::model_tool_tests`, +9 `tools::api`). `npm run build` — `tsc -b` + `vite build` clean
(only the two pre-existing warnings: the `lottie-web` eval notice and the chunk-size hint). One
pre-existing warning left alone: `unused import: clip` in `tools/monid.rs`.

**Open / to confirm at runtime:** the split view, the scratchpad panel, tool authoring from a chat
turn and the test button were code + tests only — no GUI run this session. **One behaviour change
to expect:** because an absent `approval` parses as `writes`, an *existing* custom API tool whose
method is `POST`/`PUT`/`PATCH`/`DELETE` now asks for confirmation, where before only the Manager's
own tools did. New tools default to the same policy (`emptyTool()` sets `approval: 'writes'`).
`next-iteration.md` (the build brief) stays untracked.

---

## 2026-11-03 — release 0.2.3

Ships the yt-dlp download tool and skill import / export (both below). Version `0.2.2` → `0.2.3`
across `package.json`, `src-tauri/Cargo.toml`, `src-tauri/Cargo.lock` and `src-tauri/tauri.conf.json`;
tag `v0.2.3` drives the release workflow.

---

## 2026-11-03 — skill import / export

- **Import + export agent skills** in the Agent Skills `SKILL.md` format. `storage.rs`:
  `import_skills` reads every `.md` under a folder (or a single file) — `---` frontmatter (`name`,
  `description`) plus a body of instructions, with reference material under a `## Reference` heading —
  and upserts by name, so re-importing a folder updates in place instead of duplicating.
  `export_skills` writes one `SKILL.md` per skill (default `~/Downloads/lup-skills`). `script_ids` are
  local ids and are not carried; a same-named skill keeps its own. Both take the filesystem path in the
  backend (the unit is a folder of files, not a single download — unlike the workflow JSON pattern) and
  run off the command thread via `spawn_blocking`.
- **UI:** `import` / `export` buttons on Workshop → skills open `SkillsTransferModal` (`SkillsView.tsx`).
  Import takes a folder from the new native picker or markdown dropped onto the panel; a drop is read in
  the webview and sent to `storage::import_skill_files` (the composer's drop pattern — drag-and-drop has
  no path). `runtime.ts` gained `importSkills` / `importSkillFiles` / `exportSkills`, and the modal reloads
  the skills store after an import. Passed through `WorkshopView` and mounted from `App.tsx`. The window
  keeps `dragDropEnabled: false`, which is what lets HTML5 drops work.
- **Native dialog plugin.** Added `tauri-plugin-dialog` (`Cargo.toml`, `main.rs`) + `@tauri-apps/plugin-dialog`
  + the `dialog:default` capability — the first (and only) place the app uses a native folder picker.
- Docs: `docs/architecture.md` (command table + a *Skill import / export* section).

**Verified:** `cargo test` — **78 pass** (was 73; +4 parse/render/slug, +1 upsert-by-name).
`npm run build` — `tsc -b` + `vite build` clean.

**Note / open:** the shell had `NODE_ENV=production` with npm `omit=dev`, so `npm install` pruned the
devDependencies — restored with `npm install --include=dev`. The native picker and drag-and-drop were not
exercised in this session (no GUI run). Only `.md` files with a frontmatter `name` import; other markdown
is counted as skipped.

---

## 2026-11-03 — yt-dlp download tool

- **`yt_dlp` tool.** `tools/ytdlp.rs` (new): wraps the yt-dlp CLI so an agent (and the Manager) can save a
  video (merged to mp4) or extract its audio (mp3) from a URL into a folder the user can find. Params:
  `url` (required), `mode` (`video` | `audio`, default `video`), `outputDir` (defaults to the user's
  Downloads), `filename` (base name; default the site's own title), `playlist` (default false →
  `--no-playlist`, so a video that sits inside a playlist doesn't pull the whole list). Gated on `host_fs` —
  it shells out and writes to disk, the `monid` precedent. Wired in `agents::build_tools` (the host_fs
  block), the Manager tool list in `chat.rs`, and `manager_turn`, and listed in the Manager prompt
  (`build_manager_system_prompt`). A pure `shell_line` builds the flags, so the command is covered by tests
  without yt-dlp installed. Not on the confirmation list — like the other `host_fs` tools, the grant is the
  opt-in. Docs updated: the router-tools rows in `ORCHESTRATION.md` and `docs/tool-calls.md`.

**Verified:** `cargo test` — **73 pass** (was 69; +4 ytdlp). `npm run build` — `tsc -b` + `vite build` clean
(frontend untouched, so no UI change was needed). One pre-existing warning is left alone: `unused import:
clip` in `tools/monid.rs`, unrelated to this change.

**Open / to confirm at runtime:** yt-dlp must be on PATH, and ffmpeg for the audio-extract and mp4-merge
steps; when either is missing the tool returns the CLI's own error. Downloads land in the user's Downloads
by default, which is why the tool needs the `host_fs` grant.

---

## 2026-11-03 — agent-owned items, notification queue, connections layer, new tools

The "next iteration" batch: agent work separated from the user's own, reminders made durable, a
resource/connection/adapter layer, new tools and providers, Google one-tap OAuth, and an agent clock.

- **Phase 1 — `current_datetime` tool.** `tools/datetime.rs` (new). Every agent + the Manager now
  hold a clock that reports now/tomorrow/yesterday in the configured zone (`tz::zone`), so a turn
  that must schedule or reason about "now" no longer guesses. Wired in `agents::build_tools` (the
  unconditional block), `manager_turn`, the `chat.rs` Manager branch, and the Manager prompt.
- **Phase 2 — agent-owned items.** `items.owner` added (migration; `''` = the user, else the agent
  id). `items_tool(app, agent_id)` now tags what it files (the `remember_tool` precedent), threaded
  at `agents.rs` / `chat.rs` / `manager.rs`. `capture::list_filtered` + `item_list(app, kind, owner)`
  read by owner. `ItemsView` gained a **filed by** filter (mine / each agent) and an owner badge.
- **Phase 3 — notification queue.** New `notifications` table (pruned to 500) + `notifications.rs`
  with `push`/`push_event` and the `list / unread_count / mark_read / dismiss / mark_all_read /
  clear` commands. `notify::notify_and_push` is the one entry point; `reminder_loop`, insights,
  tasks and workflows all route through it, so the in-app toast, the OS toast and the durable inbox
  can't drift. New `lumin-notification` event (replaces `lumin-reminder`); the Rail has a bell with
  an unread badge; `NotificationsView` is the inbox.
- **Phase 4 — resource / connection / adapter layer.** `adapters` (catalog, seeded from
  `integration_definitions()` + `provider::builtins()`) and `resources` tables; `connections.rs`
  surfaces integrations (`integration_configs`) and LLM endpoints (`providers`) as one set. New
  `connections::*` commands + a `useConnections` store, and a **Connections** tab in Workshop
  (adapter catalog, connection list, resource add/list/delete) wrapping the existing integration
  config/OAuth flow.
- **Phase 5 — new integrations & Google one-tap OAuth.** Tavily (`tavily_search`) and TinyFish
  (`tinyfish_search`, `GET api.search.tinyfish.ai`, `X-API-Key`) as API-key connectors; Google
  **Calendar** (`calendar_list_events`, `calendar_create_event`) and **Gmail** (`gmail_send`,
  `gmail_list`). Google integrations share a bundled desktop OAuth client (`GOOGLE_CLIENT_ID/_SECRET`
  in `integrations.rs`), so connecting is one tap; scopes are per-id; token exchange now stores
  `expiresAt`, and `integrations::google_token` refreshes lazily from the stored refresh token,
  which every Google action calls before it runs. New `refresh_google_token` command.
- **Phase 6 — Monid router.** `tools/monid.rs` (new): a `monid` tool wrapping the documented Monid
  CLI (`discover` → `inspect` → `run`) so an agent reaches its 2,000+ endpoint catalog dynamically.
  Gated on `host_fs` (it shells out), also held by the Manager. A `monid` router adapter is seeded
  into the catalog.

**Verified:** `cargo test` — **69 pass** (was 62; +1 datetime, +1 owner, +2 notification, +1
adapter-seed). `npm run build` — `tsc -b` + `vite build` clean.

**Open / to confirm at runtime:** the bundled Google client is a placeholder (`GOOGLE_CLIENT_ID`
empty) — fill it with a Google Cloud **Desktop** client for the one-tap flow to work end-to-end
(redirect `http://127.0.0.1:14852/callback`). Monid needs the CLI installed and keyed
(`npm i -g @monid-ai/cli`, `monid keys add`). TinyFish/Exa/Parallel keys are per-integration. The
Connections layer keeps `integration_configs`/`providers` as the storage of record (a genuine
`connections` table would need a migration of every reader — deferred).

---

## 2026-11-01 — release 0.2.2

Ships the items CRUD + `promise` → `goal` + classify-every-kind work, the blank-screen fix, and the
workflow token-accounting fixes below. Version bumped `0.2.1` → `0.2.2` across `package.json`,
`src-tauri/Cargo.toml`, `src-tauri/Cargo.lock` and `src-tauri/tauri.conf.json`; tag `v0.2.2` drives
the release workflow.

- **Token accounting — audit + two fixes.** The main paths are correct: agent runs and chat
  (streaming and non-streaming) sum usage across tool rounds and the final stream, for every
  provider including Anthropic (`usage.input_tokens`/`output_tokens`), and streaming requests ask
  for usage (`stream_options.include_usage`, OpenRouter `usage.include`). Two workflow gaps fixed:
  each node's real tokens now land in its `WorkflowStep` (they were hardcoded to `0`), and the
  checker's LLM-judge call now contributes to the run totals (`run_checker`/`run_llm_judge` return
  `(value, pt, ct)`).
  Still uncounted (no `runs` row is written at all): delegated-task/sub-agent runs (`tasks.rs`),
  Telegram agent turns, and the non-streaming Manager turn (`manager_turn`) — plus internal utility
  calls (classifier, memory distillation, insights, media). Left out of scope; there is no run row
  to hang a count on.

## 2026-11-01 — items CRUD, `promise` → `goal`, classify all kinds

- **Renamed the `promise` item kind to `goal`** everywhere: `capture::KINDS`, the `/p` command
  (`/promise` kept as an alias), `capture::content_type_for`, the classifier's `feature` enum +
  rules (`classify.rs`), `brain::item_kind_to_node`, the `items` tool description, the Manager
  prompt, `ITEM_KINDS` in `runtime.ts`, and the docs (`lumin.md`, `data-model.md`, `routing.md`,
  `ORCHESTRATION.md`).
- **Classification now files every item kind, not just a task.** `routing::classifier_route`
  maps `note` / `idea` / `goal` / `event` to items (alongside `task` / `reminder` / `habit`), and
  `FILING_HINTS` gained goal / link / habit / note phrasing so those turns reach the classifier.
- **The `items` agent tool is full CRUD.** Added an `update` action (title / body / kind / due /
  cadence / status), kept `add` / `list` / `complete` / `delete`, and rewrote the tool description
  + `ROUTING_RULES` + Manager prose to advertise all of them (they previously named only
  add / list / complete, which is why the model looked write-only). Delete now goes through a new
  `capture::forget`, which removes the brain index node with the item — `/forget`, the `delete`
  intent and `item_delete` all share it, so no orphaned `ref_kind='item'` node is left behind.
- **Fixed the blank screen ("renders for a second, then goes blank").** Root cause is a class, not one
  line: `App.tsx` mounts **every** view at once (hidden with CSS) and there was **no error boundary
  anywhere**, so one render-time throw in any view unmounted the tree and blanked the window — and
  the views dereferenced backend fields with no guard, so it fired on the second render once data
  arrived. Added `components/ui/ErrorBoundary.tsx` and a per-pane `Pane` wrapper in `App.tsx` (a
  broken view now shows its error in place; the shell and other views keep working), and guarded the
  unguarded accesses: `a.toolIds`, `w.nodes`/`w.edges`, `i.actions`, `d.tags`, `t.config`,
  `draft.integrations`/`permissions`, `wfNode.nodes`/`edges`, `stats.*`/`n.counts`, and the
  unvalidated `navLabels` localStorage shape.

Not run this session (code + docs only, per the standing "I'll run the app myself"): no `cargo
test` / `npm run build`. New-behaviour tests for the broadened `classifier_route` and the tool's
`update` / `delete` are not added yet.

## 2026-10-31 — release 0.2.1

The capture-routing session's work (below) landed as six feature commits, `83b8626` (voice)
through `ba44c0f` (routing). This session committed the docs that describe it and cut **0.2.1**.

- Docs committed: `ORCHESTRATION.md` (routing section + *Where to dive deep* table),
  `docs/README.md`, `docs/harness.md`, `docs/memory.md`, `docs/data-model.md`,
  `docs/tool-calls.md`, new `docs/routing.md`, and this log.
- Version `0.2.0` → `0.2.1` across `package.json`, `src-tauri/Cargo.toml`, `src-tauri/Cargo.lock`
  and `src-tauri/tauri.conf.json`; tag `v0.2.1` drives the release workflow.
- Verified: `cargo test` — **62 pass**; `npm run build` — `tsc -b` + `vite build` clean.
- `next-iteration.md` (the build brief) stays untracked — the user's working file.

---

## 2026-10-31 — capture routing, audio in, a UI pass

Base commit: `5d4785e chore: release 0.2.0`. Now committed as `83b8626`…`ba44c0f` —
39 files changed (~1200 insertions), 10 new files.

### 1. Audio input (composer + Telegram) → transcription

Audio is transcribed by a **model role**, the same way images go to a `vision` role — not a
bundled local model.

- `provider.rs` — `transcriber` added to `ROLES` (now 6); `supports_transcription` (OpenAI,
  Groq; a local whisper server speaking the same API covers the rest via an `openai` provider
  pointed at its host); `transcription_url` = `/audio/transcriptions` beside chat.
- `transcribe.rs` *(new)* — multipart upload through the role, retries, graceful errors.
- `http.rs` — shared `auth_headers` so the multipart path reuses provider auth.
- `media.rs` — audio detection, base64→bytes, the transcribe branch; audio no longer trips the
  "not an image" complaint.
- `telegram.rs` — voice-note helpers; both receive paths download + transcribe before routing.
- `ChatComposer.tsx` — mic via `MediaRecorder`, audio attachment icon, 25 MB audio cap.
- `Cargo.toml` — reqwest `multipart`; `base64` promoted from Windows-only to a shared dep.
- Docs: `harness.md`, `data-model.md`.

*Why a role:* the chat model shouldn't have to be a transcription model, and one-shot
whole-clip transcription is all the whisper endpoints offer — no streaming.

### 2. Permission popup, the 2D graph, and the empty Memory tab

- `ConfirmDialog.tsx` rebuilt on the shared `Modal` (dim backdrop, mono `approve tool` title,
  `✕`/Escape = reject, warning row, editable args) and mounted once at App level so it works
  from any view; friendly labels for every tool in `requires_confirmation`.
- `GraphView.tsx` rewritten: **the 2D graph is gone for good** (mode toggle, zoom/pan/fit,
  drag handlers, SVG renderer all deleted); 3D is unconditional and the right column is one
  persistent panel with `filters | inspect` tabs.
- Zoom ceiling fixed — `minDistance` 4 → 0.5, and camera/controls now target the graph
  **centroid** instead of a never-changed origin (that was the whole bug).
- `graph/layout.ts` — memory nodes carry `scope` (the scope filter used to no-op on them), and
  positions are computed from the *full* graph so toggling a kind no longer re-runs the force
  layout and jumps the view.
- Memory tab was empty because `am_list_memories` read only `agent:<id>` + `global`, while
  distilled chat memory is filed under `project:<pid>`. Now `*_for_agent` queries cover the
  agent's own scope, anything it authored, and `global`; two tests added.

### 3. Mic permission, graph filters, knowledge tab

- **Mic popup gone forever after one cancel.** WebView2 remembers a denied media permission
  per origin, so `getUserMedia` fails immediately and never prompts again. `mic.rs` *(new)* +
  the `webview2-com` dep add `reset_microphone_permission`, which reaches
  `ICoreWebView2Profile4::SetPermissionState` and clears the stored decision; the composer
  calls it on a `NotAllowedError` and retries once (a second refusal names the OS setting).
- Graph filters + the Memory scope filter now use the app's custom `Select` (the codebase rule
  is that `Select` replaces every native `<select>`); the right panel is **closed by default**
  with a mini toggle button.
- The knowledge base became a first-class nav tab (`types.ts`, `useNavLabels.ts`, `Rail.tsx`,
  `App.tsx`) reusing `KnowledgeBaseView`; the Workshop tab still exists.

### 4. Routing — memory vs knowledge base vs items

The bug being fixed: *everything* the user said was written to the knowledge base. Root cause
was a **missing destination** — agents held only `knowledge_base` and `remember`, and the KB
description literally advertised "notes, decisions, preferences". The KB is also pull-only, so
a rule like "never use emoji" only applied if the model happened to search for it.

- `routing.rs` *(new)* — the contract (`ROUTING_RULES`, injected into every agent **and**
  Manager prompt and mirrored into each tool description) plus the deterministic reading.
  `triage` runs **before the model is offered its tools** in `chat.rs::stream_chat_inner` and
  `manager.rs::manager_turn`: cues first ("never use…" → memory as `policy`; "remind me… at
  5pm" → items as `reminder`; "i need to…" → `todo`), then the **existing classifier role**
  only when the message looks like content worth filing — so an ordinary chat turn costs no
  extra model call. When it files, the prompt says so and the post-turn fast path is skipped.
- `tools/items.rs` *(new)* — the destination that was missing: `add | list | complete | delete`
  over `capture::add/list/complete`, brain-indexed like the capture paths. Given to every agent
  and the Manager.
- **Rules are global.** `memory::store_rule` writes `policy`/`preference` at `global` scope
  because `agent_memory::scopes_for` inherits only `<scope> + global` — an agent-scoped rule is
  invisible inside a project chat, which is exactly the injection path.
- Two latent bugs found on the way: `remember_explicit` hardcoded `kind: "semantic"` (so rules
  decayed — the opposite of the `durable_preferences_do_not_decay` guarantee), and
  `store_remembered` hardcoded agent scope.
- `tz.rs` *(new)* — `chrono-tz`, default **`Asia/Kolkata`**, wired through capture's time
  parsing, the classifier prompt, reminder display and snooze. Settings has a timezone `Select`
  (IANA list, IST first) replacing a text field whose placeholder admitted "times use the
  system zone" — `Identity.timezone` was previously a dead string.
- `telegram::default_owner` — a fallback owner bot, so a reminder created by an agent or in the
  app reaches Telegram instead of only toasting in-app.
- `refile.rs` *(new)* — the one-time pass for a knowledge base that already accumulated
  memories and todos: `refile_knowledge_preview` / `_apply`, the **tidy** button in the
  Knowledge tab (preview → approve → move + prune), and a read-only `tidy_knowledge` Manager
  tool.
- Docs: new `docs/routing.md`; updates to `docs/memory.md` and `docs/README.md`.

### Verification

`cargo test` — **62 pass** (51 before; +5 routing, +3 tz, +1 global-rule, +1 item round-trip,
+1 first-person guard). `npm run build` — `tsc -b` + `vite build` clean.

Two things caught while testing: my first tz test asserted a fixed 5h30m delta and failed — it
was DST-naive, the code was right (it now round-trips), and "I never use Spotify" would have
matched the "never use" cue and become a bogus policy, so first-person habits are excluded.

### Open / next session

- **Committed and released** as `83b8626`…`ba44c0f`, shipped in `v0.2.1`. `next-iteration.md`
  (the build brief) remains untracked and is the user's working file.
- **Not exercised at runtime** in this session — verified by tests and types only: Telegram
  reminder delivery, the tidy button, the mic prompt re-arm, and whether real model output
  routes as expected (the triage cues are only covered by unit tests).
- `src-tauri/src/transcribe.rs`, `knowledge.rs`, `tools/knowledge.rs`, `mic.rs`, `routing.rs`,
  `tz.rs`, `refile.rs`, `tools/items.rs` and `docs/routing.md` were all **new** files in this
  session — now tracked.
