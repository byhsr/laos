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
`integration_id` = the server id, and `config_json` = `{serverId, toolName, description,
schema}`. Because they're regular registry tools, agents attach them exactly like any other
tool and `build_tools` needs only the `mcp` branch.

## Commands

| Command | Does |
| --- | --- |
| `list_mcp_servers` | Servers with `env` masked. |
| `save_mcp_server(server)` | Upsert; returns the id (generated when empty). |
| `delete_mcp_server(id)` | Removes the server **and** its imported tools. |
| `test_mcp_server(id)` | Handshake + `tools/list`, returns the advertised tools. |
| `import_mcp_tools(id)` | Upserts every advertised tool into `tools`; returns the count. |

## Flow

1. **Add** a server — command, args, env.
2. **Test** → `initialize` → `notifications/initialized` → `tools/list`.
3. **Import** → each advertised tool becomes a `tools` row (`kind='mcp'`).
4. **Attach** it to an agent in the agent's TOOLS list (needs the `network` permission).

## How a call works

`tools/mcp.rs::McpTool` → `mcp::call_tool`, which:

1. Spawns the server's command (stdin/stdout piped, stderr discarded) with a **45s watchdog**
   thread that kills a hung process.
2. Handshakes: `initialize` (protocol `2024-11-05`, clientInfo `local-agent-os`) then
   `notifications/initialized`.
3. Sends `tools/call` with `{name, arguments}` and reads until the matching response id,
   skipping notifications and unrelated traffic.
4. Flattens `result.content[]` text blocks; `isError: true` becomes an `Err`.
5. Kills the process (a fresh process per call — nothing leaks between calls).

The round-trip is blocking, so `McpTool::run` runs it on `tokio::task::spawn_blocking` to
avoid stalling the async runtime.

## Notion (token, no OAuth)

Notion offers a hosted remote MCP server (`https://mcp.notion.com/mcp`) that is **OAuth**-only,
and an official local stdio server that takes an internal integration token:

| Field | Value |
| --- | --- |
| command | `npx` |
| args | `-y @notionhq/notion-mcp-server` |
| env | `NOTION_TOKEN=ntn_…` (or legacy `secret_…`) |

Then Test → Import. Requires Node/npx on the machine. Remember to **share the target Notion
pages/databases with the integration** in Notion, or the token will see nothing.

## Permission & trust

MCP tools are gated behind the agent's **`network`** permission — the same gate as `http_get`
and `api`. The agent controls only the tool *arguments*; it cannot change the command, which
is fixed at configuration time. Still, adding a server means running a local command, so treat
server config as trusted input.

## Limits

- stdio only; no remote/HTTP transport yet.
- Args are split on whitespace — quoted arguments aren't supported.
- A fresh process per call: correct and leak-free, but `npx` cold starts cost seconds. A
  long-lived child per server would be the optimization.
- No OAuth helper: servers that require OAuth (not just a token) must be run through a
  wrapper or served remotely.
