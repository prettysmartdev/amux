# Headless Mode

Headless mode exposes amux's session and subcommand execution over HTTP. Start a persistent server with `amux headless start`, then drive sessions and subcommands from scripts, CI pipelines, or any HTTP client — no interactive terminal or TUI required.

A **session** in headless mode is conceptually identical to a TUI tab: a named, isolated workspace bound to a working directory. Subcommands dispatched to a session (`implement`, `chat`, etc.) execute exactly as they would in a TUI tab — inside a Docker container, with all the same security and isolation guarantees.

All operations, inputs, and outputs are recorded durably in `~/.amux/headless/` for auditability.

---

## When to use headless mode

Headless mode is useful for:

- CI pipelines that trigger `implement` or `exec prompt` runs and poll for results
- Scripts or tooling that submit work items and retrieve output programmatically
- Remote integrations where the amux server runs on one machine and clients run elsewhere
- Audit-heavy environments where a complete durable record of every agent action is required
- One-shot agent invocations from scripts using `amux exec prompt` or `amux exec workflow`

For single interactive sessions, use `amux chat` or `amux implement` instead.

---

## One-shot scripted execution (`exec`)

The `exec` subcommand group provides two commands for running agent tasks non-interactively from scripts, CI pipelines, or the headless HTTP server — without a persistent session or TUI.

### `amux exec prompt <prompt>`

Launches an agent container with a pre-supplied prompt. Behaves identically to `amux chat`, except the initial prompt is baked into the launch arguments rather than requiring a live terminal session.

```sh
# Run a single task and exit
amux exec prompt "Fix the failing tests in src/api"

# Non-interactive: agent executes and exits; output goes to stdout
amux exec prompt "Summarise recent changes" --non-interactive

# Use a specific agent and model
amux exec prompt "Refactor the auth module" --agent codex --model gpt-4o

# Full autonomous run
amux exec prompt "Implement caching for the API layer" --yolo --non-interactive
```

The prompt must be non-empty. Passing an empty string exits immediately with:

```
error: prompt cannot be empty
```

**Flags accepted by `exec prompt`:**

| Flag | Description |
|------|-------------|
| `--non-interactive` / `-n` | Run in print/batch mode — agent executes and exits |
| `--plan` | Read-only analysis mode — agent cannot modify files |
| `--allow-docker` | Mount host Docker socket into the container |
| `--mount-ssh` | Mount `~/.ssh` read-only into the container |
| `--auto` | Auto-approve file edits; prompt before shell commands |
| `--yolo` | Fully autonomous mode — skip all permission prompts |
| `--agent=<name>` | Override the agent for this run |
| `--model=<NAME>` | Override the model for this run |

All flags behave identically to their `chat` counterparts. See [Agent Sessions](02-agent-sessions.md#flags-common-to-chat-and-implement).

---

### `amux exec workflow <path>` / `amux exec wf <path>`

Runs a workflow file without requiring a paired work item. Behaves identically to `amux implement --workflow`, except the work item is optional.

```sh
# Run a workflow without a work item
amux exec workflow ./aspec/workflows/implement-feature.md

# Alias: exec wf
amux exec wf ./aspec/workflows/implement-feature.md

# Optionally associate a work item for template variable substitution
amux exec workflow ./aspec/workflows/implement-feature.md --work-item 0053

# Non-interactive workflow run
amux exec workflow ./aspec/workflows/review.md --non-interactive
```

`exec workflow` and `exec wf` are identical — `wf` is a short alias.

**Work item template variables:** When no `--work-item` is given, prompt templates that use `{{work_item_number}}`, `{{work_item_content}}`, or `{{work_item_section:[Name]}}` are left unexpanded with a warning:

```
warning: workflow uses {{work_item_content}} but no --work-item was provided; placeholder left unexpanded
```

When `--work-item <N>` is provided, amux resolves the work item file exactly as `implement` does, and substitutes all template variables.

**Workflow state files:** When no work item is given, the state file is keyed by the workflow file's name and content hash:

```
~/.amux/headless/<workflow-name>-<content-hash8>.state.json
```

When a work item is given, the state file follows the same path as `implement`:

```
$GITROOT/.amux/workflows/<repo-hash8>-<work-item>-<workflow-name>.json
```

**Flags accepted by `exec workflow`:**

| Flag | Description |
|------|-------------|
| `--work-item <N>` / `-w <N>` | Work item number; enables template variable substitution |
| `--non-interactive` / `-n` | Run each step's agent in print/batch mode |
| `--plan` | Read-only mode for all steps |
| `--allow-docker` | Mount host Docker socket into each step's container |
| `--worktree` | Run all steps in an isolated Git worktree |
| `--mount-ssh` | Mount `~/.ssh` read-only into each step's container |
| `--auto` | Auto-approve file edits; prompt before shell commands |
| `--yolo` | Fully autonomous mode; implies `--worktree`; auto-advances stuck steps |
| `--agent=<name>` | Default agent for steps that do not specify an `Agent:` field |
| `--model=<NAME>` | Default model for steps that do not specify a `Model:` field |

All flags behave identically to their `implement --workflow` counterparts. See [Workflows](04-workflows.md#flags).

---

## Starting the server

### Foreground

```sh
amux headless start --port 9876 --workdirs /path/to/repo
```

The server starts on the specified port (default `9876`) and accepts HTTP requests for the life of the process. Logs are emitted to stdout in human-readable format. Press `Ctrl+C` to stop.

```sh
# Multiple working directories
amux headless start --workdirs /repo-a --workdirs /repo-b

# Custom port
amux headless start --port 8080 --workdirs /repo
```

`--workdirs` accepts one or more absolute paths (repeat the flag for multiple values). Only working directories on the allowlist can be used to create sessions — requests with any other path are rejected with HTTP 403. See [Working directory allowlist](#working-directory-allowlist).

### Background

```sh
amux headless start --background --port 9876 --workdirs /path/to/repo
```

`--background` daemonizes the server using the OS process manager:

| Platform | Mechanism |
|----------|-----------|
| Linux (systemd available) | `systemd-run --user` writes a transient unit |
| macOS (launchd) | Writes `~/Library/LaunchAgents/io.amux.headless.plist` and calls `launchctl load` |
| Fallback (no systemd/launchd) | Double-fork; PID written directly |

The PID is stored at `~/.amux/headless/amux.pid`. Logs go to `~/.amux/headless/amux.log`.

If a server is already running (detected via `amux.pid` and a live process check), `start` prints an error and exits with a non-zero code rather than silently competing for the port.

If `bind()` fails because the port is already in use, the error message includes the port number and the PID holding it (when discoverable):

```
error: port 9876 is already in use (PID 41290)
```

---

## Server lifecycle commands

### Status

```sh
amux headless status
```

Prints whether the server is running, its PID, port, active session count, and uptime:

```
Status:          running
PID:             81234
Port:            9876
Active sessions: 2
Uptime:          3h 14m
```

If the server is not running:

```
Status:  not running
```

### Logs

```sh
amux headless logs
```

Streams `~/.amux/headless/amux.log` to stdout in real time (equivalent to `tail -f`). Only available when the server was started with `--background`. Press `Ctrl+C` to stop streaming.

If no log file exists:

```
error: no log file found at ~/.amux/headless/amux.log
       start the server with --background to enable file logging
```

### Kill

```sh
amux headless kill
```

Sends `SIGTERM` to the background server process, allows in-flight requests to drain (up to the graceful shutdown period), and removes the PID file. On macOS, also unloads the launchd plist.

If the server is not running:

```
info: server is not running (no PID file found)
```

---

## Working directory allowlist

The server maintains a strict allowlist of working directories. Any session creation request that specifies a path not on the allowlist is rejected with HTTP 403.

**At startup**, the allowlist is populated from two sources:

1. `--workdirs` flags passed to `amux headless start`
2. `headless.workDirs` in the global config (`~/.amux/config.json`)

Both sources are merged. Every path is resolved to its canonical form (symlinks resolved, trailing slashes stripped) via `std::fs::canonicalize`. If a listed path does not exist at startup, a warning is logged but the server still starts — the path stays on the allowlist in case the directory is created later.

To see the current allowlist over HTTP:

```sh
curl http://localhost:9876/v1/workdirs
```

```json
{
  "workdirs": [
    "/home/user/my-project",
    "/home/user/other-project"
  ]
}
```

---

## HTTP API

All endpoints speak JSON. All requests and responses are logged at `INFO` level or above.

### Base URL

```
http://localhost:<port>/v1
```

### Endpoint reference

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/v1/workdirs` | List the server's allowlisted working directories |
| `POST` | `/v1/sessions` | Create a new session |
| `GET` | `/v1/sessions` | List all sessions (active and closed) |
| `GET` | `/v1/sessions/:id` | Get session detail |
| `DELETE` | `/v1/sessions/:id` | Close a session |
| `POST` | `/v1/commands` | Submit a subcommand to a session |
| `GET` | `/v1/commands/:id` | Get command status and metadata |
| `GET` | `/v1/commands/:id/logs` | Get captured command output (snapshot) |
| `GET` | `/v1/commands/:id/logs/stream` | Stream live command output via Server-Sent Events |
| `GET` | `/v1/status` | Server health (uptime, active sessions, running commands) |

---

### Sessions

#### Create a session

```sh
curl -s -X POST http://localhost:9876/v1/sessions \
  -H 'Content-Type: application/json' \
  -d '{"workdir":"/home/user/my-project"}'
```

Creates a new session bound to the given working directory. The directory must be in the allowlist. Returns immediately with a session UUID:

```json
{ "session_id": "a1b2c3d4-e5f6-7890-abcd-ef1234567890" }
```

Error responses:

| Situation | HTTP status |
|-----------|-------------|
| `workdir` not in allowlist | 403 — includes `allowed_workdirs` list |
| `workdir` field missing | 400 |

#### List sessions

```sh
curl -s http://localhost:9876/v1/sessions
```

```json
{
  "sessions": [
    {
      "id": "a1b2c3d4-...",
      "workdir": "/home/user/my-project",
      "status": "active",
      "created_at": "2026-04-20T12:00:00Z",
      "closed_at": null
    },
    {
      "id": "b2c3d4e5-...",
      "workdir": "/home/user/my-project",
      "status": "closed",
      "created_at": "2026-04-19T09:00:00Z",
      "closed_at": "2026-04-19T09:47:13Z"
    }
  ]
}
```

#### Get session detail

```sh
curl -s http://localhost:9876/v1/sessions/<session-id>
```

Returns the session record. Returns HTTP 404 if the ID does not exist.

#### Close a session

```sh
curl -s -X DELETE http://localhost:9876/v1/sessions/<session-id>
```

Marks the session `closed`. Closed sessions cannot receive new commands. All existing command records and output files are preserved — no data is deleted. Returns HTTP 204 on success, HTTP 404 if the session does not exist.

---

### Commands

Commands are submitted to a session and execute asynchronously. Submit a command and receive a `command_id` immediately; poll the command endpoint to track progress and retrieve output.

#### Submit a command

```sh
curl -s -X POST http://localhost:9876/v1/commands \
  -H 'x-amux-session: <session-id>' \
  -H 'Content-Type: application/json' \
  -d '{"subcommand":"implement","args":["0057"]}'
```

Dispatches a subcommand to the session identified by the `x-amux-session` header. Valid values for `subcommand`: `implement`, `chat`, `ready`, `exec`, `remote`.

For `exec`, the `args` array starts with the exec action (`prompt` or `workflow`/`wf`), followed by any further arguments:

```sh
# exec prompt via headless API
curl -s -X POST http://localhost:9876/v1/commands \
  -H 'x-amux-session: <session-id>' \
  -H 'Content-Type: application/json' \
  -d '{"subcommand":"exec","args":["prompt","Fix the failing tests","--non-interactive"]}'

# exec workflow via headless API
curl -s -X POST http://localhost:9876/v1/commands \
  -H 'x-amux-session: <session-id>' \
  -H 'Content-Type: application/json' \
  -d '{"subcommand":"exec","args":["workflow","./aspec/workflows/implement-feature.md","--work-item","0053"]}'
```

Returns immediately with a command UUID — execution is asynchronous:

```json
{ "command_id": "e5f6a7b8-..." }
```

**One command at a time per session.** If a command is already running in the session, the request returns HTTP 403:

```json
{
  "error": "session busy",
  "running_command_id": "e5f6a7b8-..."
}
```

Once the running command completes, the session accepts new commands.

Error responses:

| Situation | HTTP status | `error` field |
|-----------|-------------|---------------|
| Session not found or closed | 404 | `"session not found"` (includes session UUID) |
| Another command is running | 403 | `"session busy"` |
| Unknown subcommand | 400 | `"unknown subcommand"` (includes list of valid subcommands) |
| `x-amux-session` header missing | 400 | `"missing x-amux-session header"` |

#### Get command status

```sh
curl -s http://localhost:9876/v1/commands/<command-id>
```

Returns the current status and metadata for a command:

```json
{
  "id": "e5f6a7b8-...",
  "session_id": "a1b2c3d4-...",
  "subcommand": "implement",
  "args": ["0057"],
  "status": "running",
  "exit_code": null,
  "started_at": "2026-04-20T12:01:00Z",
  "finished_at": null,
  "log_path": "~/.amux/headless/sessions/a1b2c3d4-.../commands/e5f6a7b8-.../output.log"
}
```

| `status` value | Meaning |
|----------------|---------|
| `pending` | Accepted; not yet started |
| `running` | Container is executing |
| `done` | Completed with exit code 0 |
| `error` | Completed with a non-zero exit code |

#### Get command logs

```sh
curl -s http://localhost:9876/v1/commands/<command-id>/logs
```

Returns the captured output for a command. Stdout and stderr are combined into a single stream in the order they were written. For a running command, returns whatever has been written so far. For a completed command, returns the full output.

```json
{
  "output": "Implementing work item 0057...\n✓ Tests pass\n"
}
```

Output is written incrementally as the subprocess produces it — not buffered in memory.

#### Stream command logs (live)

```sh
curl -s http://localhost:9876/v1/commands/<command-id>/logs/stream
```

Opens a persistent HTTP response using [Server-Sent Events (SSE)](https://developer.mozilla.org/en-US/docs/Web/API/Server-sent_events). The server replays any output already written, then tails the log file and sends new lines as they arrive. When the command completes, the server sends a `[amux:done]` sentinel event and closes the response.

**SSE event format:**

```
data: <line of log output>

data: <another line>

data: [amux:done]

```

Each event is terminated by a blank line (standard SSE format). The sentinel `[amux:done]` signals that the command has finished — no more output will follow.

**Shell example — stream and print until done:**

```sh
curl -s http://localhost:9876/v1/commands/<command-id>/logs/stream \
| while IFS= read -r line; do
    case "$line" in
      "data: [amux:done]") echo "--- done ---"; break ;;
      data:\ *)             echo "${line#data: }" ;;
    esac
  done
```

**Behaviour notes:**

- If the command has already completed when you connect, the server replays the full historical log and sends `[amux:done]` immediately — no output is missed.
- If the client disconnects mid-stream, the command continues executing unaffected.
- If the log file does not yet exist (the command is `pending`), the server waits up to 10 s for it to appear before returning HTTP 404.
- The `Content-Type` response header is `text/event-stream`.

`amux remote run --follow` uses this endpoint internally. The cURL form above is equivalent and is useful in scripts where the amux binary is unavailable on the client.

---

### Server health

```sh
curl -s http://localhost:9876/v1/status
```

```json
{
  "uptime_seconds": 11640,
  "active_sessions": 2,
  "running_commands": 1
}
```

---

## Full example: session lifecycle

```sh
SERVER=http://localhost:9876

# 1. Create a session
SESSION=$(curl -s -X POST "$SERVER/v1/sessions" \
  -H 'Content-Type: application/json' \
  -d '{"workdir":"/home/user/my-project"}' | jq -r .session_id)
echo "Session: $SESSION"

# 2. Submit a command
CMD=$(curl -s -X POST "$SERVER/v1/commands" \
  -H "x-amux-session: $SESSION" \
  -H 'Content-Type: application/json' \
  -d '{"subcommand":"implement","args":["0057"]}' | jq -r .command_id)
echo "Command: $CMD"

# 3. Poll until done
while true; do
  STATUS=$(curl -s "$SERVER/v1/commands/$CMD" | jq -r .status)
  echo "Status: $STATUS"
  [ "$STATUS" = "done" ] || [ "$STATUS" = "error" ] && break
  sleep 10
done

# 4. Retrieve output
curl -s "$SERVER/v1/commands/$CMD/logs" | jq -r .output

# 5. Close the session
curl -s -X DELETE "$SERVER/v1/sessions/$SESSION"
```

### Example: one-shot exec prompt

For tasks that don't need a persistent session, `exec prompt` can be run directly from the CLI without starting the HTTP server:

```sh
# Run a single one-shot task; output goes to stdout; exit code reflects agent result
amux exec prompt "Fix the failing tests in src/api" --non-interactive

# Combine with shell tools
amux exec prompt "List all TODO comments in the codebase" --non-interactive | tee todos.txt
```

To drive the same task via the headless HTTP server (so the result is logged and auditable):

```sh
SERVER=http://localhost:9876
SESSION=$(curl -s -X POST "$SERVER/v1/sessions" \
  -H 'Content-Type: application/json' \
  -d '{"workdir":"/home/user/my-project"}' | jq -r .session_id)

CMD=$(curl -s -X POST "$SERVER/v1/commands" \
  -H "x-amux-session: $SESSION" \
  -H 'Content-Type: application/json' \
  -d '{"subcommand":"exec","args":["prompt","Fix the failing tests","--non-interactive"]}' | jq -r .command_id)

# Poll as usual...
```

---

## Storage layout

Everything headless mode writes lives under `~/.amux/headless/`:

```
~/.amux/headless/
  amux.log                         # server log (background mode only)
  amux.pid                         # PID file for the background process
  amux.db                          # SQLite database: sessions + commands
  sessions/
    <session-uuid>/
      commands/
        <command-uuid>/
          output.log               # combined stdout+stderr (written incrementally)
          metadata.json            # request payload, timestamps, exit code
```

`amux.db` contains two tables:

**`sessions`** — one row per session: `id` (UUID), `workdir`, `status` (`active`/`closed`), `created_at`, `closed_at`.

**`commands`** — one row per command: `id` (UUID), `session_id`, `subcommand`, `args` (JSON array), `status` (`pending`/`running`/`done`/`error`), `exit_code`, `started_at`, `finished_at`, `log_path`.

The database is the authoritative record of all activity. The per-command log files hold raw output. Neither is deleted when a session is closed.

`metadata.json` for each command contains the original request payload and precise timestamps:

```json
{
  "id": "e5f6a7b8-...",
  "session_id": "a1b2c3d4-...",
  "subcommand": "implement",
  "args": ["0057"],
  "started_at": "2026-04-20T12:01:00Z",
  "finished_at": "2026-04-20T12:43:17Z",
  "exit_code": 0
}
```

---

## Configuration

Headless mode settings live under a `headless` key in the global config (`~/.amux/config.json`). All fields are optional.

```json
{
  "headless": {
    "workDirs": [
      "/home/user/my-project",
      "/home/user/other-project"
    ],
    "alwaysNonInteractive": true
  }
}
```

### `headless.workDirs`

Pre-configure working directories so you don't have to repeat `--workdirs` every time you start the server:

```sh
amux config set --global headless.workDirs "/home/user/my-project,/home/user/other-project"
```

Paths from `headless.workDirs` and paths from `--workdirs` flags are merged at startup — both sources can be used together. See [Configuration](07-configuration.md#global-config) for the full global config reference.

### `headless.alwaysNonInteractive`

When set to `true`, amux automatically injects `--non-interactive` into every dispatched command that supports it — including `implement`, `chat`, `exec prompt`, `exec workflow`, `ready`, and `specs amend`.

```sh
amux config set --global headless.alwaysNonInteractive true
```

This is the recommended setting for headless server deployments where no TTY is available. It guarantees that no command blocks waiting for interactive input.

When `alwaysNonInteractive` is `true` and a command is dispatched via the HTTP API, the flag is automatically injected into the args vector — you do not need to include `--non-interactive` in your API requests explicitly.

The setting defaults to `false` so that amux's interactive defaults remain unchanged for users who have not configured a headless server.

---

## Security

Headless mode preserves all of amux's container isolation guarantees: every subcommand runs inside a Docker container, never directly on the host. The HTTP server has no built-in authentication layer — bind it to `localhost` (the default) and control access via firewall rules or SSH tunnels when running on a shared or remote machine.

The working directory allowlist is the primary access control on the server: even a client that can reach the HTTP server can only create sessions in pre-approved directories.

---

## Graceful shutdown

On `SIGTERM` or `SIGINT`, the server finishes all in-flight HTTP responses and allows running commands up to 30 seconds to complete before force-terminating them. Both shutdown start and completion are logged. The 30-second grace period applies whether the server was stopped by `amux headless kill` or by sending the signal directly.

---

## Edge cases

| Situation | Behaviour |
|-----------|-----------|
| `workdir` not in allowlist on `POST /v1/sessions` | HTTP 403; response includes the list of allowed directories |
| Session not found or already closed on `POST /v1/commands` | HTTP 404; response includes the session UUID |
| Second `POST /v1/commands` while one is running | HTTP 403 `"session busy"`; includes running command ID |
| Server already running when `headless start` is invoked | Error printed; exits non-zero |
| Port already bound (EADDRINUSE) | Error includes the port number and PID holding it |
| `--workdirs` path doesn't exist at startup | Warning logged; path remains on allowlist |
| `amux headless kill` when server is not running | Informational message; exits 0 |
| `amux headless logs` with no log file | Clear error suggesting `--background` |
| Unknown `subcommand` in `POST /v1/commands` | HTTP 400; response lists valid subcommands |
| `x-amux-session` header missing | HTTP 400 |
| `exec prompt` with empty string | CLI validation error: `"prompt cannot be empty"` before any container launches |
| `exec workflow` with `{{work_item_content}}` and no `--work-item` | Warning printed; placeholder left unexpanded; workflow continues |
| `exec workflow --work-item <N>` where file not found | Error pointing to the expected path pattern; same message as `implement` |
| `headless.alwaysNonInteractive` true + duplicate `--non-interactive` flag in args | Flag is deduplicated; no error |
| `exec` dispatched via HTTP API with unknown action (not `prompt`/`workflow`/`wf`) | HTTP 400; response lists valid exec actions |
| `remote` subcommand dispatched via HTTP API without required args (e.g. no `--session`) | Subprocess exits with a clear error; output appears in the command log |

---

[← Configuration](07-configuration.md) · [Remote Mode →](09-remote-mode.md)
