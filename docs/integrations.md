# Integrations

Integrations are the configurable external providers (Notion, Airtable, Google Sheets/Docs,
Telegram). They store credentials in the DB, connect via a token or OAuth, and expose
**actions** that become agent tools.

Related: [tool-calls.md](./tool-calls.md) (how actions are offered), [webhooks.md](./webhooks.md)
(Telegram runtime), [data-model.md](./data-model.md) (`integration_configs`).

## Catalog

Defined in `integrations::integration_definitions()`. Each entry is
`{ id, name, provider, actions[] }`; `enabled` / `connected` live in the DB, not the code.

| id | name | provider | actions | auth |
| --- | --- | --- | --- | --- |
| `notion` | Notion | `notion` | `notion_search`, `notion_create_page`, `notion_get_page` | token **or** OAuth |
| `airtable` | Airtable | `airtable` | `airtable_list_records`, `airtable_create_record`, `airtable_update_record` | token |
| `sheets` | Google Sheets | `google` | `sheets_read`, `sheets_append`, `sheets_update` | OAuth |
| `docs` | Google Docs | `google` | `docs_create`, `docs_get` | OAuth |
| `telegram` | Telegram | `telegram` | `telegram_send` | bot token |

Action implementations live in `src-tauri/src/tools/integration.rs` — a single `match` on the
action name. The tool's `description()` and `params_schema()` are also switched there.

## How an action becomes a tool

`agents::build_tools` grants actions to an agent only when **all** hold:

1. The integration id is in `agent.integrations` (explicit per-agent grant).
2. Its `integration_configs` row is `enabled = 1` **and** `connected = 1`.
3. The agent has the `network` permission.

Then each action in the definition is pushed as an `IntegrationTool { action, credentials }`.
See [tool-calls.md](./tool-calls.md) for the permission matrix.

## Credential storage

Credentials live in `integration_configs.config_json` (a JSON blob). The stored key the
tools read is picked from, in order: `token`, `apiKey`, `accessToken`
(`tools/integration.rs`).

| Function (`integrations.rs`) | Role |
| --- | --- |
| `save_integration_config` | Merge incoming config over the stored one, then persist. |
| `list_integrations` | Read rows for the UI, applying `mask_config` to each stored config. |
| `mask_config(cfg)` | Replaces any key containing `token` / `key` / `secret` with `••••••••`. |
| `merge_config(existing, incoming)` | Preserves stored secrets when the UI echoes the mask or sends `null`; rejects numeric values for string fields. |
| `integration_secret(conn, id)` | Full (unmasked) config for backend use; nulls non-string secret fields. |
| `stored_api_key(conn, model_id)` | Separate concern: model provider API keys (`model_configs.api_key`), not integrations. |

Secrets never round-trip to the UI: the frontend only ever sees masked values, and saving a
masked value keeps the real secret unchanged.

## OAuth

Supported by `notion`, `sheets`, and `docs`. Redirect URI is fixed:
`http://127.0.0.1:14852/callback` (`OAUTH_REDIRECT_PORT = 14852`).

| Command | Behavior |
| --- | --- |
| `start_oauth(id)` | Builds the provider authorize URL (requires a saved `clientId`) and returns it. |
| `connect_oauth(id)` | Opens the URL in the system browser via `tauri-plugin-opener`, then spawns a background task listening on the loopback port to capture `?code=` and exchange it. Returns immediately — the UI is never blocked on the redirect. |
| `complete_oauth(id, code)` | Manual fallback for a pasted code. |

Token exchange targets:

| Provider | Endpoint |
| --- | --- |
| Google | `https://oauth2.googleapis.com/token` |
| Notion | `https://api.notion.com/v1/oauth/token` |

Exchanged tokens are stored as `accessToken` / `refreshToken` in the integration config.

## Testing

`test_integration(id)` probes the provider and returns a boolean:

| Provider | Probe |
| --- | --- |
| Notion | `GET /v1/users/me` |
| Airtable | `GET /v0/meta/whoami` |
| Telegram | `getMe` |
| Google | assumed OK (no probe) |

## Manager control

The Manager can drive integrations through tools: `configure_integration`, `list_integrations`,
and `test_integration` (`manager.rs`). `configure_integration` **is** behind the confirmation
popup because it stores secrets. Note `test_integration` for the Manager only returns an
instructional message; the real probe is the frontend's `testIntegration`.

## Adding an integration

1. Add a definition to `integration_definitions()` (`integrations.rs`).
2. Add the action(s) to the `match` in `tools/integration.rs`: name, `description`,
   `params_schema` (shared property bag), and the HTTP call.
3. If it needs OAuth, extend `start_oauth` (authorize URL + scopes) and
   `complete_oauth_inner` (token exchange) and register the provider in the OAuth list.
4. Add a `test_integration` probe.
5. Frontend: map a brand logo and any config fields in `components/views/IntegrationsView.tsx`.
6. Update the catalog table above.
