# Dev log

One entry per working session — what changed, why, and what is still open. Newest first.

**Read this before starting a session.** The tree is deliberately not committed between
sessions, so "uncommitted" below means *carried forward*, not *lost*. For where to start
digging on a given concern, see the **Where to dive deep** table in
[ORCHESTRATION.md](./ORCHESTRATION.md).

---

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
