# MCP Connector

A generic [Model Context Protocol](https://modelcontextprotocol.io) connector: point the app
at a local MCP server, and its tools become agent tools. One implementation covers Notion,
GitHub, filesystem, Postgres, and anything else that speaks MCP.

Related: [tool-calls.md](./tool-calls.md) (the tool registry), [integrations.md](./integrations.md)
(the REST-based providers), [data-model.md](./data-model.md).

**Transport: stdio only.** A remote (streamable-HTTP) transport is not implemented — the
JSON-RPC layer in `mcp.rs` isolates the transport, so adding one means extending
`spawn()`/`request()` and nothing else.

## Data

`mcp_servers` — `id` (PK, `mcp-<ms>`), `name`, `command`, `args` (JSON array), `env` (JSON
object), `enabled`, `updated_at`.

`env` is where tokens live, so values whose **key** contains `token` / `key` / `secret` /
`password` are masked on read (`mask_env`) and preserved on save when the UI echoes the mask
back (`merge_env`) — the same contract as integrations.

Imported tools are ordinary rows in the existing `tools` table with `kind='mcp'`,
`integration_id` = the server id, and `config_json` = `{serverId, serverName, toolName,
description, schema}`. Because they're regular registry tools, agents attach them exactly like
any other tool and `build_tools` needs only the `mcp` branch.

## Commands

| Command | Does |
| --- | --- |
| `list_mcp_servers` | Servers with `env` masked. |
| `save_mcp_server(server)` | Upsert; returns the id (generated when empty). |
| `delete_mcp_server(id)` | Removes the server **and** its imported tools. |
| `test_mcp_server(id)` | Handshake + `tools/list`, returns the advertised tools. |
| `import_mcp_tools(id)` | Rebuilds the server's rows in `tools` (insert + prune stale); returns the count. |

## Flow

1. **Add** a server — command, args, env.
2. **Test** → `initialize` → `notifications/initialized` → `tools/list`.
3. **Save** imports automatically — each advertised tool becomes a `tools` row (`kind='mcp'`).
   The **Sync** button re-runs it on demand (after the server adds or changes tools).
4. **Attach** it to an agent in the agent's Tools list. MCP tools group under their server in
   the picker, and need no `network` permission — attaching one is the opt-in.

> The Manager (Laos) has no per-agent tool picker, so it holds **every** enabled MCP tool
> (`agents::all_mcp_tools`) alongside its own — any imported server is usable from it directly,
> and its system prompt lists them.

## How a call works

`tools/mcp.rs::McpTool` → `mcp::call_tool`, which runs against a **long-lived session** held for
that server:

1. **One process per server.** The first call spawns the command (stdin/stdout piped, stderr
   captured) and handshakes once: `initialize` (protocol `2024-11-05`, clientInfo
   `local-agent-os`) then `notifications/initialized`. The session lives in a process-wide
   registry keyed by server id and is reused by every later `tools/list` / `tools/call`, so
   **stateful servers keep their state** between calls (a browser session, a DB connection).
2. Calls to the same server are **serialized** through the session lock — stdio is a single
   request/response stream, so two calls never interleave.
3. Each request writes one JSON-RPC message and reads until the matching response id, skipping
   notifications and unrelated traffic; `result.content[]` text blocks are flattened and
   `isError: true` becomes an `Err`.
4. A **per-request 45s watchdog** kills a server that hangs; the session is then dropped and the
   next call respawns it. A healthy idle session is never killed.
5. A session whose process died (crash, kill) is detected and replaced on the next call. Editing
   a server's command/args/env is picked up automatically — the config **fingerprint** no longer
   matches, so the old process is stopped and a new one started.

The round-trip is blocking, so `McpTool::run` runs it on `tokio::task::spawn_blocking` to avoid
stalling the async runtime. Sessions are stopped when the server is deleted and when the app
exits (`RunEvent::Exit` → `mcp::shutdown_all`), so no child process is orphaned.

## Tool names

Providers only accept `[a-zA-Z0-9_-]` function names, but servers advertise names like
`API.post-search`. `McpTool::name()` sanitizes the name the model sees and prefixes it with the
server (`Notion_API-post-search`) so two servers can both expose e.g. `search` without
colliding. The real advertised name is what goes over `tools/call`.

## Notion (token, no OAuth)

Notion offers a hosted remote MCP server (`https://mcp.notion.com/mcp`) that is **OAuth**-only,
and an official local stdio server that takes an internal integration token:

| Field | Value |
| --- | --- |
| command | `npx` |
| args | `-y @notionhq/notion-mcp-server` |
| env | `NOTION_TOKEN=ntn_…` (or legacy `secret_…`) |

Then Save (tools import automatically). Requires Node/npx on the machine. Remember to **share the target Notion
pages/databases with the integration** in Notion, or the token will see nothing.

> `tools/list` succeeds even with an invalid token — Notion only validates it when a tool is
> *called*, so a green Test proves the wiring, not the PAT. Verified against
> `@notionhq/notion-mcp-server`, which advertises 24 tools (`API-post-search`,
> `API-retrieve-a-page`, …).

On Windows a command shim like `npx` is a `.cmd` file, which `Command::new` cannot launch
directly — `spawn_session()` wraps the command in `cmd /C` there (the same approach as
`run_command`).

## Browser control (Playwright)

Persistent sessions are what make a real browser usable: the server launches one Chrome/Edge and
every `navigate` → `click` → `type` → `extract` call drives the **same page** across the whole
conversation. Add it as an MCP server:

| Field | Value |
| --- | --- |
| command | `npx` |
| args | `-y @playwright/mcp@latest` |
| env | (none) |

Save → tools import → attach to an agent (or just ask Laos). The agent gets tools like
`browser_navigate`, `browser_click`, `browser_type`, `browser_snapshot`, and
`browser_take_screenshot`, driven over the Chrome DevTools Protocol against a real browser.
Requires Node/npx on the machine. `chrome-devtools-mcp`
(`-y chrome-devtools-mcp@latest`) is an alternative that speaks CDP directly.

> Before persistence this could not work — a fresh process per call launched a new browser every
> time and lost the page. Now the browser stays open for the session.

## Permission & trust

MCP tools are **not** gated behind a permission — attaching one to an agent is the opt-in.
(They used to require `network`, which silently dropped them, so a configured MCP tool looked
broken.) The agent controls only the tool *arguments*; it cannot change the command, which is
fixed at configuration time. Still, adding a server means running a local command, so treat
server config as trusted input.

## Limits

- stdio only; no remote/HTTP transport yet.
- Args are split on whitespace — quoted arguments aren't supported.
- Sessions are long-lived for the app's lifetime and stopped on delete / exit; there is no idle
  reaper yet, so many configured servers means many resident processes.
- No OAuth helper: servers that require OAuth (not just a token) must be run through a
  wrapper or served remotely.
