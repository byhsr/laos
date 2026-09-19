# Local Agent OS

Local-first desktop MVP for configurable AI agents. It includes a React workspace and a Tauri v2 backend that creates a local SQLite store and can run an Ollama-backed agent.

## Docs

Design and internals live in [`docs/`](./docs/README.md): architecture, the model-request
[harness](./docs/harness.md), [tool calls](./docs/tool-calls.md),
[integrations](./docs/integrations.md), [memory](./docs/memory.md),
[webhooks/tunnels](./docs/webhooks.md), and the [data model](./docs/data-model.md). Start
with the [index](./docs/README.md) for the dev harness, ports, and a maintenance map.

## Run it

1. Install [Ollama](https://ollama.com), then run `ollama pull qwen3:8b`.
2. Run `npm.cmd install`.
3. Start the desktop application with `npm.cmd run tauri dev`.

The browser command (`npm.cmd run dev`) is for UI iteration only. It cannot call Tauri commands, so it deliberately cannot execute agents.

## Current executable MVP boundary

- `ollama:<model>` references execute against a local Ollama service.
- Tauri creates a SQLite database in the app data directory and records runs.
- Each run sends the agent objective and task through its configured local model and returns an observable result.
- The canvas represents explicit sequential workflow dependencies.

Provider calls (`ollama:`, `groq:`, `openrouter:`) share one tool-calling and streaming path, and API keys are stored in the local database.
