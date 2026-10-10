# Terminal sessions

Run real CLI tools (e.g. `cmdc`) as long-lived **PTY** sessions you can drive from the panel
and from Telegram. Owned by `src-tauri/src/terminal.rs`; the panel UI is
`src/components/views/TerminalView.tsx`.

## Panel

**Terminal** (rail) → start a session (name, command line, optional working dir). The command
runs through the platform shell (`cmd /C` / `sh -c`) inside a real PTY, so interactive TUIs
work. The view shows an xterm.js terminal bound to the selected session: keystrokes →
`terminal_write`, output ← `terminal_attach`, resize → `terminal_resize`. Kill removes it.

Sessions live for the app's lifetime (a process-wide registry); output is kept in a 64 KB
rolling ring so reattaching replays the recent screen.

## Telegram

In **Telegram → bots**, set a bot's **target** to a terminal session (instead of an agent).
Then any message you send that bot is written to the CLI's stdin, and its output is relayed
back to that chat — debounced (~1.2 s of quiet) and ANSI-stripped, so a redrawing TUI doesn't
spam; the debounce skips a flush when the tail is unchanged. `/screen` dumps the current tail
on demand.

Commands: `terminal_start`, `terminal_write`, `terminal_read`, `terminal_attach`,
`terminal_resize`, `terminal_list`, `terminal_kill`.

## Ask the user (buttons + input)

The Manager has an `ask_user` tool: over Telegram it sends the question with **inline-keyboard
buttons** when `options` are given (or waits for the next message otherwise) and blocks until
the answer, then continues. Button taps arrive as `callback_query` updates (handled in both the
poll loop and the webhook); free-text answers are matched to a pending request for that chat and
consumed before normal routing.

`telegram_ask(botId, chatId, question, options, timeoutSecs?)` exposes the same flow to the
panel/future surfaces. Pending requests are keyed by a token embedded in `callback_data`
(`ask:<token>:<index>`) and time out (default 300 s).

## Limits

- Windows needs ConPTY (Win10 1809+); a failed spawn returns a clear error.
- The Telegram relay shows a **text tail**, not a live TUI; use `/screen` for the current view.
- Output is relayed only to the chat that last messaged the bot.
