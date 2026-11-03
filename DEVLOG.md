# Dev log

One entry per working session — what changed, why, and what is still open. Newest first.

**Read this before starting a session.** The tree is deliberately not committed between
sessions, so "uncommitted" below means *carried forward*, not *lost*. For where to start
digging on a given concern, see the **Where to dive deep** table in
[ORCHESTRATION.md](./ORCHESTRATION.md).

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
