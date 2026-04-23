<p align="center">
  <strong>Run and coordinate AI code agents from your terminal.</strong> <br>
  Parallel sessions, multi-step workflows, full container isolation.<br>
  <br>
  <img src="./docs/amux_logo_v3.svg" width="320" alt="AMUX">
</p>

<p align="center">
  <img src="https://github.com/prettysmartdev/amux/actions/workflows/test.yml/badge.svg">
</p>

---

`amux` is a terminal multiplexer for AI code agents. Open multiple agent sessions in parallel, run them through structured multi-step workflows, run agents on a fleet of machines, and keep everything safe — every agent runs inside a container, never on the host.

![amux TUI](./docs/blog/images/tui-screenshot.png)

---

## Installation

```sh
curl -s https://prettysmart.dev/install/amux.sh | sh
```

The installer detects your platform and puts `amux` on your `PATH`.

<details>
<summary>Other installation options</summary>

**From GitHub Releases** — download the binary for your platform from [GitHub Releases](https://github.com/prettysmartdev/amux/releases):

| Platform | Binary |
|----------|--------|
| Linux (x86_64) | `amux-linux-amd64` |
| Linux (ARM64) | `amux-linux-arm64` |
| macOS (Intel) | `amux-macos-amd64` |
| macOS (Apple Silicon) | `amux-macos-arm64` |
| Windows (x86_64) | `amux-windows-amd64.exe` |

**From source** — requires Rust 1.94+ and make:

```sh
git clone https://github.com/prettysmartdev/amux.git
cd amux
sudo make install
```

</details>

---

## Quick Start

```sh
# 1. Initialize your repo (once per project)
amux init

# 2. Open the TUI
amux

# 3. Start an agent session
chat

# 4. Run the Dockerfile.dev refresh agent to
#    ensure all your project's tools get installed 
ready --refresh
```

See the [Getting Started Guide](docs/00-getting-started.md) for a full walkthrough.

---

## What you can do

### Run multiple agents at once

Open a new tab with **Ctrl+T**. Each tab is independent — its own working directory, its own container, running in the background while you work in another tab. Switch between tabs with **Ctrl+A** / **Ctrl+D**.

If a running agent gets stuck or completes its task, its tab turns yellow so you know to check in. 

### Run structured workflows

![amux workflows](./docs/blog/images/tui-workflow.png)

A workflow breaks complex work into phases — for example, plan → implement → review → docs. Each phase is a separate agent session. You review the output between phases and decide whether to continue, retry, or redirect.

Workflows are plain Markdown (or TOML, or YAML) files in your repo:

```markdown
## Step: plan
Prompt: Read work item {{work_item_content}} and produce an implementation plan.

## Step: implement
Depends-on: plan
Prompt: Implement work item {{work_item_number}} according to the plan.

## Step: review
Depends-on: implement
Prompt: Review the implementation for correctness and style.
```

```sh
amux exec workflow ./aspec/workflows/implement-feature.md --work-item 0027
```
Workflows can optionally be passed a specific work item - a spec you've written - to work on new features, fix bugs, etc.


### Use different agents per step

Each workflow step can specify which agent runs it:

```markdown
## Step: implement
Depends-on: plan
Agent: codex
Prompt: Implement the plan.

## Step: review
Depends-on: implement
Agent: claude
Prompt: Review for correctness and style.
```

Supported agents: `claude`, `codex`, `opencode`, `maki`, `gemini`. Steps without an `Agent:` field use your configured default.


### Hand off to the agent completely (yolo mode)

![amux yolo mode](./docs/blog/images/tui-yolo-mode.png)

`--yolo` disables your agent's permission prompts and auto-advances completed workflow steps. Use it when you have a well-specified task and want to return to a finished result.

```sh
# Implement fully autonomously, changes isolated to a git worktree
amux exec workflow ./aspec/workflows/implement-feature.md --yolo --work-item 0042
```

When a workflow step completes, a 60-second yolo countdown starts. If the agent doesn't resume, the workflow advances automatically. The countdown is visible in the tab bar across all tabs — you can monitor multiple autonomous sessions without switching to each one.

`--yolo --workflow` automatically runs in an isolated Git worktree, so you can review and discard the result if it isn't right.

For lighter autonomy, `--auto` approves file edits automatically but still requires permission for other commands.

### Manage agents across remote machines 

`amux headless start` runs an HTTP server that allows remote control of amux. This is useful when you want to run heavy agent workflows on a remote machine or manage a fleet of agent-runner boxes.

```sh
# On the remote machine, start headless server (prints an API key on first run)
amux headless start --port 9090
```

From your local machine, use `amux remote` or cURL:

```sh
amux config set remote.defaultAPIKey <key>
amux config set remote.defaultAddr <host>
amux remote session start /workspace/myproject
amux remote run implement 0027 --session <id> --follow
```

```sh
# Create a session bound to a directory
curl -s -X POST http://localhost:9090/v1/sessions \
  -H "Authorization: Bearer <key>" \
  -H "Content-Type: application/json" \
  -d '{"workdir": "/workspace/myproject"}'

# Submit a command to that session
curl -s -X POST http://localhost:9090/v1/commands \
  -H "Authorization: Bearer <key>" \
  -H "x-amux-session: <session-id>" \
  -H "Content-Type: application/json" \
  -d '{"subcommand": "implement", "args": ["0027"]}'

# Poll for completion, then fetch the log
curl -s http://localhost:9090/v1/commands/<command-id>
curl -s http://localhost:9090/v1/commands/<command-id>/logs
```
Headless commands run inside containers with the same isolation as running amux locally. All inputs and outputs and logs are stored in `~/.amux/headless/` on the server for later review or auditing. The Headless server is authenticated using an API key generated the first time it is run, and can be refreshed (invalidating the old key) using `amux headless start --refresh-key`.

See [Headless Mode](docs/08-headless-mode.md) and [Remote Mode](docs/09-remote-mode.md) for details.

### Keep a persistent background agent

`amux claws` manages a long-lived [nanoclaw](https://github.com/qwibitai/nanoclaw) container — a machine-global background agent accessible via your messaging app (Slack, Discord, WhatsApp). Unlike per-project sessions, it survives reboots and accumulates context across sessions.

```sh
amux claws init    # guided first-time setup
amux claws ready   # check status / restart after reboot
amux claws chat    # attach for an interactive session (Ctrl+C to detach)
```

---

## Security

Every agent runs inside a Docker container built from `Dockerfile.dev` — agent-generated code never executes on your host machine.

- Only the current Git repository is mounted into the container
- Credentials are passed as environment variables and masked in all displayed commands — never written to files inside containers
- `amux ready --refresh` scans your project and updates `Dockerfile.dev` with exactly the tools your workflow needs
- amux itself is a statically compiled Rust binary — it cannot be modified by anything running inside a container

Apple Containers (macOS 26+) is also supported as an alternative to Docker Desktop.

![amux TUI status](./docs/blog/images/tui-status.png)

---

## Commands

```sh
amux                                  # open the TUI
amux init [--agent <name>]            # set up a project
amux ready [--refresh]                # verify environment; rebuild Dockerfile.dev
amux chat [--agent <name>] [--plan] [--auto] [--yolo]
amux implement <nnnn> [--agent <name>] [--plan] [--auto] [--yolo] [--workflow <path>] [--worktree]
amux specs new [--interview]          # create a work item
amux specs amend <nnnn>               # update a spec to match what was built
amux status [--watch]                 # dashboard of all running agent containers
amux config show                      # view all config values
amux claws init                       # set up the persistent nanoclaw container
amux claws ready                      # check / restart nanoclaw
amux headless start [--port <n>]      # start the HTTP server (generates API key on first run)
amux headless status                  # check if the server is running
amux headless kill                    # stop the server
amux remote run <cmd> [--follow]      # run a command on a remote headless server
amux remote session start <dir>       # create a session on a remote server
amux remote session kill <id>         # close a session on a remote server
```

All commands work in both TUI mode (without the `amux` prefix) and CLI mode.

---

## Documentation

- [Getting Started](docs/00-getting-started.md)
- [Using the TUI](docs/01-using-the-tui.md)
- [Agent Sessions](docs/02-agent-sessions.md)
- [Security & Isolation](docs/03-security-and-isolation.md)
- [Workflows](docs/04-workflows.md)
- [Yolo Mode](docs/05-yolo-mode.md)
- [Nanoclaw](docs/06-nanoclaw.md)
- [Configuration](docs/07-configuration.md)
- [Headless Mode](docs/08-headless-mode.md)
- [Remote Mode](docs/09-remote-mode.md)
- [Architecture](docs/architecture.md)

---

## License

See [LICENSE](LICENSE) for details.
