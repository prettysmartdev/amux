# Remote Mode

Remote mode lets you connect to a headless amux server running on another machine and run commands there — from your terminal, from a CI pipeline, or from inside the TUI. Live log streaming lets you watch agent output in real time, exactly as if the session were local.

---

## Overview

A typical setup has one machine running `amux headless start` (the _remote host_), and one or more developers or pipelines using `amux remote` (the _client_) to dispatch work to it. The remote host manages all sessions and containers. The client only needs the server's address.

```
Local machine                          Remote host
──────────────                         ──────────────────────────
amux remote run implement 0059 -f ───► POST /v1/commands
                                       ◄─── SSE stream: log output
                                       ◄─── [amux:done] sentinel
```

Three subcommands cover the full lifecycle:

| Command | What it does |
|---------|-------------|
| `amux remote run <command>` | Dispatch a command to a session on the remote host |
| `amux remote session start [dir]` | Create a new session on the remote host |
| `amux remote session kill [session-id]` | Close a session on the remote host |

All three subcommands work from the terminal (CLI mode) and from inside the TUI (where interactive pickers are also available). A headless server can also delegate `remote` subcommands to itself as subprocesses when triggered via the HTTP API.

---

## Connecting to a remote host

Every `remote` subcommand needs to know the address of the remote headless server. The address is resolved in this order:

| Priority | Source |
|----------|--------|
| 1 | `--remote-addr <URL>` flag on the command |
| 2 | `AMUX_REMOTE_ADDR` environment variable |
| 3 | `remote.defaultAddr` in `~/.amux/config.json` |

If none of the three sources provides an address, the command fails immediately with:

```
error: No remote address configured. Pass --remote-addr, set AMUX_REMOTE_ADDR,
       or set remote.defaultAddr in ~/.amux/config.json.
```

The most convenient setup for day-to-day use is to set a default address once:

```sh
amux config set --global remote.defaultAddr http://build-server.example.com:9876
```

After that, every `amux remote` command targets that host by default, with no flag required.

---

## `amux remote run`

Dispatches an amux subcommand to a session on the remote host.

```sh
amux remote run <command> [--session <ID>] [--follow] [--remote-addr <URL>]
```

`<command>` is any amux subcommand that the remote host can execute — for example `implement 0059`, `exec prompt "Fix the tests" --yolo`, or `chat`. Everything after `remote run` (except the `--session`, `--follow`, and `--remote-addr` flags) is forwarded to the remote host verbatim.

### Basic usage

```sh
# Dispatch implement 0059 to a session; return a command ID immediately
amux remote run implement 0059 --session abc123

# Wait for the command to complete and stream its output to your terminal
amux remote run implement 0059 --session abc123 --follow

# Short form for --follow
amux remote run implement 0059 --session abc123 -f

# Pass inner-command flags through unchanged; amux does not consume them
amux remote run exec prompt "Fix the tests" --yolo --non-interactive --session abc123 -f
```

### Flags

| Flag | Short | Description |
|------|-------|-------------|
| `--session <ID>` | | Session ID on the remote host. Required in CLI mode; interactive in TUI mode. Overrides `AMUX_REMOTE_SESSION` |
| `--follow` | `-f` | Stream log output until the command completes, then print a summary table |
| `--remote-addr <URL>` | | Remote host address. Overrides `AMUX_REMOTE_ADDR` and `remote.defaultAddr` |

### Session resolution

For `remote run`, the session is resolved in this order:

| Priority | Source |
|----------|--------|
| 1 | `--session <ID>` flag |
| 2 | `AMUX_REMOTE_SESSION` environment variable |
| 3 | TUI only — last session used in this tab (not available in CLI/headless) |
| 4 | TUI only — interactive session picker |

In CLI mode, if neither `--session` nor `AMUX_REMOTE_SESSION` is set, the command fails with:

```
error: No session specified. Pass --session <ID> or set AMUX_REMOTE_SESSION.
       Use `amux remote session start` to create a session, or list sessions
       with `curl <remote-addr>/v1/sessions`.
```

### Live log streaming (`--follow`)

Without `--follow`, `remote run` submits the command and returns immediately with the command ID:

```
Command dispatched: e5f6a7b8-...
```

With `--follow`, amux connects to the SSE log-streaming endpoint and relays the command's output to your terminal in real time, as if the session were local:

```sh
amux remote run implement 0059 --session abc123 --follow
```

```
Connecting to log stream for e5f6a7b8-... on http://build-server.example.com:9876...
Implementing work item 0059...
✓ Tests pass
✓ Cargo build successful
...
```

Once the command completes, amux prints a summary table and exits:

```
┌──────────────┬────────────────────────────────────────┐
│ Field        │ Value                                  │
├──────────────┼────────────────────────────────────────┤
│ Command ID   │ e5f6a7b8-…                             │
│ Session ID   │ abc123                                 │
│ Subcommand   │ implement 0059                         │
│ Status       │ done                                   │
│ Exit Code    │ 0                                      │
│ Started      │ 2026-04-22T10:00:00Z                   │
│ Finished     │ 2026-04-22T10:02:31Z                   │
└──────────────┴────────────────────────────────────────┘
```

If the command had already completed before you connected, `--follow` replays the full historical log and then immediately prints the summary — there is no gap or missed output.

When output is piped rather than printed to a terminal, log lines are written without ANSI decoration — output is script-friendly by default.

---

## `amux remote session start`

Creates a new session on the remote host.

```sh
amux remote session start [dir] [--remote-addr <URL>]
```

`dir` is the absolute path (on the remote host) of the working directory for the session. It must be in the remote host's `--workdirs` allowlist.

```sh
# Start a session bound to /home/user/my-project
amux remote session start /home/user/my-project

# Specify a non-default remote host
amux remote session start /home/user/my-project --remote-addr http://alt-host:9876
```

On success, amux prints the new session ID:

```
Session started: a1b2c3d4-e5f6-7890-abcd-ef1234567890
Workdir: /home/user/my-project
```

In CLI mode, `dir` is required. In TUI mode, `dir` is optional — if omitted and saved directories are configured, amux shows an interactive picker (see [TUI interactive flows](#tui-interactive-flows)).

---

## `amux remote session kill`

Closes a session on the remote host.

```sh
amux remote session kill [session-id] [--remote-addr <URL>]
```

```sh
# Kill a specific session
amux remote session kill a1b2c3d4-e5f6-7890-abcd-ef1234567890

# Specify a non-default remote host
amux remote session kill abc123 --remote-addr http://alt-host:9876
```

On success:

```
Session closed: a1b2c3d4-e5f6-7890-abcd-ef1234567890
```

In CLI mode, `session-id` is required. In TUI mode, if omitted, amux fetches the current session list from the remote host and shows an interactive picker.

---

## TUI interactive flows

When used inside the amux TUI, `remote` subcommands gain interactive capabilities that are not available in CLI mode.

### Session picker (`remote run` without `--session`)

If you run `remote run` in the TUI without specifying a session, and no session is stored from previous activity in the current tab, amux fetches the active session list from the remote host and displays an interactive picker:

```
╭─── Select Session ─────────────────────────────────────────────────────────╮
│                                                                              │
│   ▶  a1b2c3d4  /home/user/my-project                                        │
│      b2c3d4e5  /home/user/other-project                                      │
│                                                                              │
│  ↑↓ navigate  Enter confirm  Esc cancel                                      │
╰──────────────────────────────────────────────────────────────────────────────╯
```

Use `↑` / `↓` to highlight a session and `Enter` to confirm. The TUI remembers your choice for the rest of the tab's lifetime — subsequent `remote run` commands in the same tab skip the picker and use the remembered session automatically.

If the remote host has no active sessions, the picker displays:

```
No active sessions on http://build-server.example.com:9876. Run `remote session start` first.
```

### Saved-dir picker (`remote session start` without a directory)

If you run `remote session start` in the TUI without a directory argument, and `remote.savedDirs` is configured, amux shows a directory picker:

```
╭─── Select Directory ───────────────────────────────────────────────────────╮
│                                                                              │
│   ▶  /home/user/my-project                                                   │
│      /home/user/other-project                                                │
│      /opt/builds/service-a                                                   │
│                                                                              │
│  ↑↓ navigate  Enter confirm  Esc cancel                                      │
╰──────────────────────────────────────────────────────────────────────────────╯
```

If no saved directories are configured and no directory argument is given:

```
error: No directory specified and no savedDirs configured.
       Pass a directory argument or add paths via: config set remote.savedDirs --global
```

### Save-dir prompt (`remote session start` with a new directory)

When you start a session with a directory that is not in `remote.savedDirs`, the TUI offers to save it:

```
╭────────────────────────────────────────────────────────────────╮
│  Save '/home/user/new-project' to remote.savedDirs? (y/n)      │
╰────────────────────────────────────────────────────────────────╯
```

- `y` — saves the path to `remote.savedDirs` in your global config, then starts the session
- `n` or `Enter` — starts the session without saving
- `Esc` — cancels the session start entirely

### Session kill picker (`remote session kill` without a session ID)

If you run `remote session kill` in the TUI without a session ID, amux fetches the active session list and shows a picker titled "Kill Session". Navigation is identical to the session picker above.

---

## Configuration

Remote mode settings live under a `remote` key in the global config (`~/.amux/config.json`). All fields are optional.

```json
{
  "remote": {
    "defaultAddr": "http://build-server.example.com:9876",
    "savedDirs": [
      "/home/user/my-project",
      "/home/user/other-project"
    ]
  }
}
```

### `remote.defaultAddr`

The default address of the remote headless amux server. When set, you don't need to pass `--remote-addr` on every command.

```sh
amux config set --global remote.defaultAddr http://build-server.example.com:9876
```

Overridden per-invocation by `--remote-addr` or `AMUX_REMOTE_ADDR`.

### `remote.savedDirs`

A list of working directory paths (absolute paths on the remote host) for use by the TUI's saved-dir picker when running `remote session start` without a directory argument.

```sh
# Set a single directory
amux config set --global remote.savedDirs /home/user/my-project

# Set multiple directories (comma-separated)
amux config set --global remote.savedDirs "/home/user/my-project,/home/user/other-project"

# Clear all saved directories
amux config set --global remote.savedDirs ""
```

Directories can also be added interactively from the TUI: when you start a session with a directory not already in the list, the TUI offers to save it for you.

---

## Full example: end-to-end CLI workflow

```sh
# Configure the remote host once
amux config set --global remote.defaultAddr http://build-server.example.com:9876

# Create a session on the remote host
SESSION=$(amux remote session start /home/user/my-project | grep 'Session started:' | awk '{print $NF}')
echo "Session: $SESSION"

# Dispatch a command and stream its output
amux remote run implement 0059 --session "$SESSION" --follow

# Or pipe into a log file (no ANSI decoration)
amux remote run implement 0059 --session "$SESSION" --follow > implement-0059.log

# Kill the session when you are done
amux remote session kill "$SESSION"
```

---

## Full example: CI pipeline

```sh
export AMUX_REMOTE_ADDR=http://build-server.internal:9876
export AMUX_REMOTE_SESSION=<pre-provisioned-session-id>

# Dispatch the work item; exit code reflects the command's exit code
amux remote run implement 0059 --follow
```

For CI contexts where a session is long-lived and pre-provisioned, setting `AMUX_REMOTE_SESSION` in the pipeline environment avoids session management boilerplate.

---

## Using cURL directly

Because `remote run` is built on the headless HTTP API, you can use cURL (or any HTTP client) wherever `amux remote` is inconvenient — for example, in scripts with no amux binary available:

```sh
SERVER=http://build-server.example.com:9876
SESSION=a1b2c3d4-...

# Submit a command
CMD=$(curl -s -X POST "$SERVER/v1/commands" \
  -H "x-amux-session: $SESSION" \
  -H 'Content-Type: application/json' \
  -d '{"subcommand":"implement","args":["0059"]}' | jq -r .command_id)

# Stream live output via SSE (prints each log line as it arrives)
curl -s "$SERVER/v1/commands/$CMD/logs/stream" | while IFS= read -r line; do
  case "$line" in
    "data: [amux:done]") echo "--- done ---"; break ;;
    data:\ *)             echo "${line#data: }" ;;
  esac
done

# Or poll until done, then fetch the full log
while true; do
  STATUS=$(curl -s "$SERVER/v1/commands/$CMD" | jq -r .status)
  [ "$STATUS" = "done" ] || [ "$STATUS" = "error" ] && break
  sleep 5
done
curl -s "$SERVER/v1/commands/$CMD/logs" | jq -r .output
```

See [Headless Mode](08-headless-mode.md) for the full HTTP API reference, including session management endpoints.

---

## Edge cases

| Situation | Behaviour |
|-----------|-----------|
| No remote address configured | Error with instructions to pass `--remote-addr`, set `AMUX_REMOTE_ADDR`, or configure `remote.defaultAddr` |
| `remote run` without `--session` in CLI/headless mode | Error with instructions to pass `--session` or set `AMUX_REMOTE_SESSION` |
| `remote session start` without a directory in CLI mode | Error with instructions to pass a directory argument |
| `remote session kill` without a session ID in CLI mode | Error with instructions to pass a session ID |
| Session not found on remote | HTTP 404 from the server; error message includes the session ID and suggests `remote session start` |
| Remote host unreachable | Connection timeout after 10 s; error message includes the target address |
| Session is busy (command already running) | HTTP 403 from the server; error message includes the running command ID |
| `--follow` on a command that already completed | Full historical log is replayed, then summary is printed — no output is missed |
| `remote session start` with a new dir (TUI) | Offers to save the path to `remote.savedDirs`; session starts regardless of whether you save |
| TUI session picker: remote host has no active sessions | Picker modal shows "No active sessions" message; `Enter` and `Esc` both cancel |
| TUI session picker: fetch fails | Error shown in command input bar; no modal opens |
| TUI `remote session start` with no saved dirs and no dir argument | Error in command input bar: "No directory specified and no savedDirs configured" |
| `remote session start` with a dir already in `savedDirs` | Dir is not duplicated in the list when the save-dir prompt is accepted |
| Inner command flags (e.g. `--yolo`) | Forwarded to the remote host verbatim; not consumed by the `remote run` parser |

---

[← Headless Mode](08-headless-mode.md) · [Architecture →](architecture.md)
