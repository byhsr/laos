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

Set a bot's **target** to a terminal session (in **Telegram → bots**), or start one from the chat
with **`/cli`**. Messages are written to the CLI's stdin; its **screen** is relayed back.

**Rendering.** Output is fed through a VT parser (`vt100`) and the relay reads the *rendered
screen*, not the raw byte stream — so cursor moves and erases are applied and there are no partial
frames or escape-code fragments. Repeated chrome (the `❯` prompt, `· Working… esc to interrupt`,
`? for shortcuts`, alt+v hints, the banner, separator rules) is filtered out; what remains is the
real content — replies, file bodies, TODOs, diffs.

**Timing.** The relay flushes only after the session goes quiet (~2.2 s), so you get the answer
once a turn finishes instead of the processing frames.

**`/cli` — start a CLI from the chat.** `/cli` (or `/cli <command>`) opens a PTY bound to that
chat: `/cli` is the platform shell, `/cli cmdc` starts the CLI. Every following message is typed
into its stdin character-by-character, with Enter sent as its own keystroke, so a TUI treats it as
a keypress and submits rather than a paste. `/exit` (or `/stop` / `/cli stop`) ends it. A chat-bound
CLI takes precedence over the bot's target and short-circuits before the agent path, so **CLI
sessions never write to memory** — they don't teach the agent. The session also appears in the
Terminal view while it's running.

**Output control.**

- `/live on` (default) / `/live off` — auto-relay the screen after each turn. Off means nothing is
  pushed automatically; the screen is still tracked, so re-enabling doesn't dump a backlog.
- `/screen` — the current screen, on demand (works whether live is on or off).

Driving a **TUI** (option lists, prompts) needs raw keys, not typed lines:

- `/key <name> [count]` — send a key: `up`, `down`, `left`, `right`, `enter`, `esc`, `tab`,
  `space`, `backspace`, `delete`, `home`, `end`, `pageup`, `pagedown`, `ctrl-c` … A count repeats
  it (`/key down 3`); several keys chain (`/key down down enter`).
- `/pick <n>` — select the nth row of a menu: `down` × (n−1), then `enter`.
- `/key ctrl-c` — interrupt the running program without closing the session.

**Access.** A bot is private: list **allowed users** (Telegram ids) in the editor, or leave it empty
and the **first** sender is bound as its owner (trust-on-first-use), after which it's locked to
them. `/id` (or `/whoami`) always replies with your id so you can add it.

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
- The relay shows the **rendered screen** (the visible rows), not scrollback; for a long run use
  `/screen` near the part you care about, or `/live off` and pull when you want.
- Output is relayed only to the chat that last messaged the bot.
- Each message is handled in its own task, so a slow turn or a CLI session being typed into never
  blocks receiving the next message.
