# Local Agent OS

Local-first desktop MVP for configurable AI agents. It includes a React workspace and a Tauri v2 backend that creates a local SQLite store and can run an Ollama-backed agent.

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

## Running Firecrawl locally

Firecrawl is open source and self-hosted. Agents with the `firecrawl` integration call a **local** instance at `http://localhost:3002` — no API key, no cloud dependency.

1. Install [Docker Desktop](https://www.docker.com/products/docker-desktop/) (or Docker Engine + Compose v2).
2. Clone the pinned release and copy the baseline env (included in this repo):
   ```bash
   git clone https://github.com/firecrawl/firecrawl.git
   cd firecrawl
   git checkout v2.11.162
   cp /path/to/this/repo/firecrawl.env .env
   # edit .env: replace POSTGRES_PASSWORD with a long random value
   docker compose up --build -d
   ```
3. Verify it is reachable:
   ```bash
   curl http://localhost:3002/v0/health/readiness
   # {"status":"ok"}
   ```
4. Launch Local Agent OS (`npm.cmd run tauri dev`). The app auto-detects the local instance and marks the Firecrawl integration as configured. Agents with the `firecrawl` integration can then call `web_search` and `web_crawl` as real tools.

Notes:
- The vendored `docker-compose.firecrawl.yml` in this repo is the official compose file at `v2.11.162` for reference; the canonical run path is a checkout of the firecrawl repo at that tag (the compose file builds from source).
- The default stack is unauthenticated — keep it on a trusted network. This is a local-first baseline, not a production deployment.
- Stop it with `docker compose down`.

## Current executable MVP boundary

- `ollama:<model>` references execute against a local Ollama service.
- Tauri creates a SQLite database in the app data directory and records runs.
- Each run sends the agent objective and task through its configured local model and returns an observable result.
- The canvas represents explicit sequential workflow dependencies.
- Self-hosted Firecrawl (Docker) powers a real `web_search` / `web_crawl` tool-calling loop for agents that have the firecrawl integration and `network` permission.

OpenRouter configuration screens are present, but credential storage and an OpenRouter tool-calling loop are future extensions; OpenRouter runs are single-shot today.
