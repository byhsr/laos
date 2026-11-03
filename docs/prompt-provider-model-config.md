# Agent prompt — implement provider + model configuration

_Paste this whole file as the task prompt for the coding agent that will build the same system in another app. It describes the architecture, the data model, the backend surface and the UI/UX flow. Adapt names/paths to the target stack, but keep the design decisions._

---

You are implementing a **provider and model configuration system** for this app. The app talks to LLM APIs (OpenAI, Anthropic, Google, Groq, OpenRouter, xAI, local Ollama, and any OpenAI-compatible server). You must make this configurable by the user without hard-coding each vendor into every call site.

## The one rule that governs everything

**A provider is data, not a branch.** Do not write `if provider == "openai"` in request code. Instead:

- A **provider** is an *adapter* describing: an endpoint root, a wire dialect (how requests/responses are shaped), an auth scheme, extra headers, and a capability list. Built-ins are defined in code and are read-only. Custom providers are user-created rows in a table and are always **OpenAI-compatible**.
- A **model** is a config row whose id is `"<providerId>:<modelName>"`. It carries the user-chosen label, type, its own (optional) key, a host override, and a per-model reasoning setting.

Every request path (chat, agents, embeddings, transcription, speech, classification, etc.) resolves a model id through **one function** — a `resolve(modelId, overrideKey?)` that returns everything needed to make the request: base URL, bare model name, resolved key, reasoning setting, and URL builders. Adding a new provider must never require touching a call site.

Adopt this split. It is the load-bearing decision of the whole feature.

## Data model

Create these tables (names illustrative):

```
model_configs(
  id TEXT PK,              -- "<providerId>:<modelName>"
  provider TEXT,           -- the provider id (prefix of `id`)
  label TEXT,              -- user-facing name
  model TEXT,              -- the bare model name sent to the provider
  host TEXT,               -- endpoint override (used for Ollama / self-hosted)
  api_key TEXT,            -- NEVER the key itself; stores a vault sentinel or NULL
  enabled INTEGER,
  reasoning TEXT,          -- 'auto' | 'off' | 'low' | 'medium' | 'high'
  kind TEXT,               -- llm | vision | image | video | media | multimedia | stt | tts
  voice TEXT               -- for tts models
)

providers(
  id TEXT PK, label TEXT, base_url TEXT, auth TEXT, auth_header TEXT,
  extra_headers TEXT,      -- JSON object
  requires_key INTEGER, reasoning TEXT, provider_routing INTEGER,
  caps TEXT,               -- JSON object { chat, vision, embeddings, transcription, speech }
  api_key TEXT, created_at TEXT
)

model_roles(role TEXT PK, model_id TEXT, updated_at TEXT)
```

Key rule: the `api_key` column stores only a sentinel (`__vault__`) or NULL — the real secret lives in the OS credential store under `provider/<id>` or `model/<id>`. If the credential store is unavailable the write must fail loudly; never fall back to plaintext.

## Backend surface

**Provider registry**
- Define built-ins in code as an ordered list of records (order = UI list order). Seven to start: Ollama, OpenAI, Anthropic, Google (Gemini), Groq, OpenRouter, xAI.
- Four enums carry all behavior:
  - `Dialect`: `Ollama | OpenAiCompat | Anthropic` — decides request/response shape.
  - `Auth`: `Bearer | XApiKey | None` — decides how the key travels.
  - `ReasoningStyle`: `None | OpenAiEffort | OpenRouterEffort | OllamaThink | AnthropicThinking` — decides which request parameter carries the reasoning setting.
  - `Caps`: `{ chat, vision, embeddings, transcription, speech }` — explicit, not inferred, so a role can be gated before the request is made.
- `findProvider(id)` returns a built-in if it exists, otherwise loads the custom row. Custom providers force `Dialect::OpenAiCompat`.
- URL composition: store the **API root** (the part before `/chat/completions`) and build endpoints from it — never strip a full URL back to a base. Provide builders: `chatUrl`, `embeddingsUrl`, `transcriptionUrl`, `speechUrl`.

**Resolution**
- `resolve(modelId, overrideKey?)`:
  1. Take the provider prefix of the id.
  2. Find the provider def (error clearly if unknown → "Add it in Settings → Providers").
  3. Load the model's own row (host, key, reasoning).
  4. Key precedence: caller override → model's own key → provider's shared key.
  5. If the provider `requires_key` and none resolved, error clearly.
  6. For Ollama, honor the editable host; otherwise use the provider's base.
- Reasoning: `'auto'` must write **nothing** to the request, so models without reasoning support never receive a parameter they would reject. Map the style to the right field (`reasoning_effort`, `reasoning.effort`, `think`, `thinking.budget_tokens`). Note that reasoning tokens bill against `max_tokens`, so an enabled level must raise the output budget.

**Model roles** — optional pins (e.g. `classifier`, `embedding`, `reranker`, `vision`, `transcriber`, `distillation`). An unset role means "use a deterministic default", so a fresh install works with zero configuration. Roles with a capability requirement (vision, transcriber) take effect only when explicitly pinned.

**Commands / API** — expose CRUD for both entities: `list_providers`, `save_provider`, `delete_provider`, `list_model_configs`, `save_model_config`, `delete_model_config`, `list_model_roles`, `set_model_role`. Reject saving or deleting a built-in provider id. Save semantics for the write-only key:
- a submitted key → write to credential store, store the sentinel;
- `clear_key` → delete from the store, clear the column;
- neither → leave the stored value untouched.

The record returned to the UI must **never include the raw key** — only a `hasKey` boolean.

## Frontend surface

**Provider store (single source of truth)**
- Keep all configured providers in one client-side store loaded once at bootstrap. Every provider picker reads from it, so a newly added provider appears everywhere without a reload.
- On save, strip the key from local state and recompute `hasKey`.

**Provider branch (Settings → Providers)**
- A button to add a provider opens a modal form.
- Form fields: `id` (slug: `^[a-z0-9._-]+$`, no `:`, immutable after create), `label`, `base url`, `auth` select (`bearer` | `api-key header` | `none`), a header-name field shown only for the api-key scheme, `api key` (write-only; show "saved — type to replace" and a "remove key" affordance when one exists), `capabilities` checkboxes (`chat` always on; vision/embeddings/transcription/speech), `reasoning style` select, and toggles for `requires an API key` and `provider routing`.
- Helper copy should explain that custom providers speak the OpenAI API and the base URL is the API root.
- List custom providers as cards (logo, `label · id · custom`, base URL, key-set marker, auth, capability summary). Built-ins show no edit/delete buttons. Deleting shows a confirm modal that names the `"<id>:…"` models that will stop resolving.

**Add-model flow (Settings → Models)**
- A button opens a model drawer/modal. Fields: `provider` (select fed from the provider store, with a hard-coded fallback before load), `type/kind` (the model-type list), `label`, `model id` (placeholder per provider so the user sees the expected shape), `voice` (only for `tts`), `reasoning` slider, `host` (only for Ollama), `api key` (hidden for Ollama; write-only with remove), and an `enabled` checkbox. Show a live logo preview.
- The model `id` defaults to `${provider}:${model}`.
- The same view renders a **model-roles card**: one select per role, models that satisfy the role's capability sorting first and marked, with `auto` / `off` / `using <resolved>` feedback.

**Composer / picker UX**
- The in-chat model picker groups enabled models by kind, and offers a reasoning slider on a second tab.
- Resolve a logo per model: OpenRouter ids dispatch by the author prefix before `/`, known brands map to a brand glyph, everything else falls back to a neutral glyph or the endpoint's favicon.

## Conventions to follow

- One client store per domain; UI reads live from it.
- Tiny mono labels; one modal primitive with a header action for save; consistent button/select/checkbox/slider/card primitives.
- Toast on every write.
- Small, dense, scrollable lists; side-by-side tab panels that toggle visibility rather than unmount.
- Anything the user can mis-configure must explain its consequence inline (e.g. "a model without its own key inherits this provider's key").

## Acceptance checklist

- [ ] Adding a custom provider makes its models selectable everywhere with no reload.
- [ ] A model is addressed only as `"<provider>:<model>"`; no request path switches on provider id.
- [ ] API keys are write-only to the UI, stored in the OS credential store, never in the DB or the frontend.
- [ ] Built-in providers cannot be edited or deleted.
- [ ] `reasoning = auto` sends no reasoning parameter at all.
- [ ] A fresh install with only a local Ollama works with zero configuration.
- [ ] Deleting a provider or model leaves no orphaned credentials and clearly reports which models stop resolving.
