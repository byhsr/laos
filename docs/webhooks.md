# Webhooks & Tunnels

The app runs two inbound listeners plus a Telegram long-poll fallback. Owned by
`src-tauri/src/telegram.rs` (Telegram) and `src-tauri/src/integrations.rs` (OAuth loopback).

Related: [integrations.md](./integrations.md), [architecture.md](./architecture.md) (boot wiring).

## Surfaces at a glance

| Listener | Port | Purpose |
| --- | --- | --- |
| Local Telegram webhook receiver | 14789 | `POST /webhook/telegram` from Telegram (when a tunnel is up). |
| OAuth loopback | 14852 | `GET /callback?code=…` capture during Connect. |
| Telegram long-poll | outbound | `getUpdates` — active when no tunnel/webhook is set. |
| Cloudflare quick tunnel | outbound | Exposes 14789 at a public `trycloudflare.com` URL. |

## Telegram: config

Telegram stores everything in the `integration_configs` row `id='telegram'` (`config_json`):

| Key | Meaning |
| --- | --- |
| `token` | Bot token (from BotFather). |
| `tunnelUrl` | Current public tunnel URL. |
| `tunnelPort` | Local port the tunnel forwards to (default 14789). |
| `webhookRegistered` | Whether `setWebhook` succeeded. |
| `webhookSecret` | `secret_token` sent to Telegram and checked on inbound. |

Helpers: `telegram_config`, `cfg_str`, `write_telegram_config`, `telegram_token`,
`webhook_registered`, `webhook_secret`, `ensure_webhook_secret`.

## Tunnel (`telegram_start_tunnel`)

1. Locate `cloudflared` — `which_cloudflared` (next to the binary / on PATH), else
   `which_cloudflared_download` fetches the official release into the app data dir (cached,
   `chmod +x` on Unix).
2. Spawn `cloudflared tunnel --url http://127.0.0.1:<port>` (default 14789).
3. Scrape the `trycloudflare.com` URL from stdout/stderr, emit progress over a `Channel`,
   persist it, and `std::mem::forget` the child so it outlives the command.
4. A tunnel from a previous session never survives a restart — `telegram_reconcile_on_boot`
   clears the dead webhook on startup so long-polling takes over instead of silently stalling.

## Webhook registration

| Command | Behavior |
| --- | --- |
| `telegram_register_webhook` | Registers the live tunnel URL with Telegram via `setWebhook`, including `secret_token`. |
| `telegram_register_custom_url` | Same, but for a user-supplied public URL. |
| `telegram_stop_tunnel` | Calls `deleteWebhook` and clears the tunnel flags. |

## Webhook receiver (`telegram_webhook_server`, port 14789)

A hand-rolled `tokio` TCP server handling only `POST /webhook/telegram`:

1. Validate the `X-Telegram-Bot-Api-Secret-Token` header against the stored secret.
2. Ignore the `__health_probe__` self-test message (used by the health check).
3. Log the inbound message to `telegram_logs`, ACK `200` immediately.
4. Spawn the Manager turn and send the reply back via `sendMessage`.

## Long-poll loop (`telegram_loop`)

Started at boot; a no-op until a token exists.

- Polls `getUpdates?timeout=30&offset=<next>`.
- **Pauses while a tunnel/webhook is active** so the two paths never double-process.
- Dedupes against `SEEN_UPDATES` (a 256-entry ring of `update_id`s).
- Handles `/agents` and `/tasks` locally (deterministic, no LLM) via
  `agents::build_workspace_context` and `tasks::list_tasks`.
- Otherwise routes to `telegram_manager_turn` → `manager::manager_turn`.

Global in-memory state (`OnceLock`): `TELEGRAM_TUNNEL_URL`, `TELEGRAM_TURN_LOCK`
(serializes Manager turns), `SEEN_UPDATES`.

## Health

| Command | Returns |
| --- | --- |
| `telegram_tunnel_status` | `{ tunnelUrl, webhookRegistered }`. |
| `telegram_webhook_health` | `tunnelUrl`, `webhookRegistered`, `receiverListening` (port 14789), `liveTunnel`, `urlMismatch`, and Telegram's `getWebhookInfo` (`url`, `pending_update_count`, `last_error_message`, `last_error_date`). |

The receiver also performs an end-to-end probe POST through the tunnel. A receiver
bind-failure is recorded as an error row so a broken link in the chain is never silent.

## Activity log

`telegram_logs` records every inbound/outbound message (direction, chat id, text, reply,
status `ok`/`error`, detail, timestamp) and is pruned to the newest **500** rows.
Outbound messages use `split_for_telegram` (4000-char limit) and try HTML via
`tg_markdown::to_telegram_html`, falling back to raw text on failure.

## OAuth loopback (port 14852)

`connect_oauth` opens the provider's authorize URL in the system browser and spawns a
background `TcpListener` on **14852** to receive `http://127.0.0.1:14852/callback?code=…`,
then exchanges the code for tokens. The command returns immediately (the wait happens in the
background task), so the UI is never blocked on the redirect. `complete_oauth` accepts a
manually pasted code as a fallback. See [integrations.md](./integrations.md) for exchanges.
