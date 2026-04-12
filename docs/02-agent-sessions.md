# Agent Sessions

An agent session is a Docker container running your configured AI agent (Claude Code, Codex, OpenCode, Maki, or Gemini) against your project. amux handles starting the container, injecting your credentials, and connecting your terminal to the agent's input/output.

There are two session types: **freeform chat** and **work item implementation**.

---

## Freeform chat

```sh
amux chat
# or, in the TUI command box:
chat
```

`chat` launches an agent with no pre-configured prompt — a clean, blank slate. Use it for exploring the codebase, asking questions, prototyping ideas, or any task where you want to drive the conversation yourself.

In the TUI, the container window opens immediately and all keyboard input is forwarded to the agent. In command mode, the container's stdin/stdout/stderr are directly connected to your terminal.

Press **Ctrl+C** to exit the agent session when you're done.

---

## Implementing a work item

```sh
amux implement 0001
# or, in the TUI:
implement 0001
```

`implement` finds `aspec/work-items/0001-*.md`, builds a structured prompt from its contents, and launches the agent in a container. The prompt instructs the agent to implement the work item, iterate on builds and tests, write documentation, and report when complete.

The work item number can be written with or without leading zeros: `1` and `0001` are equivalent.

After the agent launches, you can interact with it directly — add follow-up instructions, review output, or let it run autonomously. Press **Ctrl+C** or type `exit` in the agent to end the session.

---

## Flags common to `chat` and `implement`

### `--non-interactive`

Run the agent in print/batch mode — no interactivity required. The agent executes, produces output, and exits.

| Agent | Flag used |
|-------|-----------|
| Claude | `-p` (print mode) |
| Codex | `--quiet` |
| OpenCode | `run` subcommand |
| Maki | `--print` |
| Gemini | `-p` (`--prompt`) |

Useful for CI pipelines, scripting, or when you want the output captured rather than live.

### `--plan`

Run the agent in read-only mode — it can analyse the codebase and suggest changes, but cannot modify files. Useful for getting a second opinion on an approach before committing to implementation.

| Agent | Plan mode |
|-------|-----------|
| Claude | `--plan` |
| Codex | `--approval-mode plan` |
| OpenCode | Not supported (flag is silently ignored) |
| Maki | Not supported (flag is silently ignored) |
| Gemini | `--approval-mode=plan` |

`--plan` can be combined with `--non-interactive`.

### `--allow-docker`

Mount the host Docker socket into the container, giving the agent the ability to build and run Docker containers. See [Security & Isolation](03-security-and-isolation.md#docker-socket-access) for details on when to use this.

### `--mount-ssh`

Mount your host `~/.ssh` directory read-only into the container, allowing the agent to clone private repos or push branches over SSH. See [Security & Isolation](03-security-and-isolation.md#ssh-key-mounting).

### `--worktree`

Run the agent in an isolated Git worktree instead of your main working tree. After the agent finishes you choose to merge, discard, or keep the branch. See [Security & Isolation](03-security-and-isolation.md#worktree-isolation).

### `--auto`

Enable intermediate autonomous operation — the agent auto-approves file edits and writes, but still prompts before shell commands and other high-risk operations. Less permissive than `--yolo`.

| Agent | Flag used |
|-------|-----------|
| `claude` | `--permission-mode auto` |
| `codex` | `--full-auto` |
| `opencode` | *(no equivalent — a warning is printed, flag omitted)* |
| `maki` | `--yolo` (maki's own flag) |
| `gemini` | `--approval-mode=auto_edit` |

`--auto` applies `yoloDisallowedTools` config the same way `--yolo` does. Combined with `--workflow`, it implies `--worktree` but does **not** auto-advance stuck workflow steps.

When both `--yolo` and `--auto` are passed, `--yolo` wins.

### `--yolo`

Enable fully autonomous operation — the agent skips all permission prompts. See [Yolo Mode](05-yolo-mode.md).

### `--workflow=<path>`

(`implement` only) Run a multi-step workflow instead of a single agent session. See [Workflows](04-workflows.md).

---

## Work item management

### Creating a work item

```sh
amux specs new
# or in TUI:
specs new
```

Prompts for a type (Feature, Bug, Task, or Enhancement) and a title, then creates the file at `aspec/work-items/NNNN-title.md` using the project's template.

```sh
amux specs new --interview
```

After creating the file, prompts for a brief summary of the work, then launches an agent session to complete the spec — filling in user stories, implementation plan, edge cases, and test plan based on your summary. More thorough specs lead to better implementations.

In the TUI, a freeform text box dialog opens for the summary input. Use **Ctrl+Enter** to submit or **Esc** to cancel.

### Updating a spec after implementation

```sh
amux specs amend 0001
```

After implementing a work item, the actual implementation sometimes differs from the original spec. `specs amend` launches the agent to review the code that was written and update the spec to match — adding an "Agent implementation notes" section describing what changed and why. Useful for keeping specs accurate as a long-term reference.

---

## Monitoring running agents

```sh
amux status          # one-shot snapshot
amux status --watch  # auto-refreshing dashboard (every 3 seconds)
```

`status` works outside the TUI. It shows every active code agent container and the nanoclaw container (if running), with CPU usage, memory, project path, and runtime.

```
CODE AGENTS
┌────────────────────────────┬────────┬───────┬─────────┐
│ Project                    │ Agent  │ CPU   │ Memory  │
├────────────────────────────┼────────┼───────┼─────────┤
│ /home/user/myproject       │ claude │ 5.23% │ 210MiB  │
└────────────────────────────┴────────┴───────┴─────────┘
```

If amux is launched outside of any Git repository, `status --watch` runs automatically instead of the normal startup.

---

## Agent authentication

amux automatically passes your agent's credentials into the container — you never have to log in manually inside a container session.

For Claude Code, amux reads the OAuth token from the macOS Keychain (service: `Claude Code-credentials`) and passes it into the container as the `CLAUDE_CODE_OAUTH_TOKEN` environment variable. Credentials are never mounted as files, and the token value is masked (`***`) in all displayed Docker commands.

| Agent | Env var | Keychain service (macOS) |
|-------|---------|--------------------------|
| `claude` | `CLAUDE_CODE_OAUTH_TOKEN` | `Claude Code-credentials` |
| `codex` | — | — |
| `opencode` | — | — |
| `maki` | via `envPassthrough` | — |
| `gemini` | via `envPassthrough` or `~/.gemini/` mount | — |

Maki and Gemini authenticate via API keys or OAuth tokens passed from your host environment. See [Configuration](07-configuration.md#envpassthrough) for details and [Gemini authentication](#gemini-authentication) below for the full Gemini auth options.

### Host settings injection

For Claude sessions, amux also mounts sanitized copies of your Claude Code settings so the agent starts pre-configured with your model preferences, plugins, and onboarding state:

| Host file | Container path | Notes |
|-----------|----------------|-------|
| `~/.claude.json` | `/root/.claude.json:ro` | `oauthAccount` field stripped to prevent broken auth state |
| `~/.claude/settings.json` | `/root/.claude/settings.json:ro` | Model preferences, plugins — copied as-is |

Your original files are never modified. The copies are created in a temporary directory before each launch and cleaned up when the container exits.

---

## Gemini authentication

Gemini supports two authentication paths. You can use either or both — amux sets up both automatically.

### API key (`envPassthrough`)

Add `GEMINI_API_KEY` (or one of the Vertex AI variables) to your `envPassthrough` config:

```json
{ "envPassthrough": ["GEMINI_API_KEY"] }
```

Get a free API key from [Google AI Studio](https://aistudio.google.com/apikey) (1,000 requests/day on the free tier). amux reads the value from your host shell and injects it into the container as a `-e` flag on the `docker run` invocation. The value is masked (`***`) in all displayed Docker commands.

Supported Gemini auth environment variables:

| Variable | Description |
|----------|-------------|
| `GEMINI_API_KEY` | API key from Google AI Studio |
| `GOOGLE_API_KEY` | Vertex AI API key (takes precedence over `GEMINI_API_KEY`) |
| `GOOGLE_CLOUD_PROJECT` | Vertex AI project ID |
| `GOOGLE_CLOUD_LOCATION` | Vertex AI region |
| `GOOGLE_GENAI_USE_VERTEXAI` | Set to `true` to enable the Vertex AI auth path |

> **Note on `GOOGLE_APPLICATION_CREDENTIALS`:** This variable points to a file path on the host. Passing it via `envPassthrough` injects the path string but not the file itself, so the container cannot read it. Service account JSON authentication requires either embedding the key in your `Dockerfile.dev` or mounting it manually. For most users, `GEMINI_API_KEY` is simpler.

### OAuth token (`~/.gemini/` mount)

Gemini's default interactive auth stores OAuth tokens in `~/.gemini/settings.json` on your host after you run `gemini` for the first time and complete the browser login flow. amux automatically copies `~/.gemini/` into a temporary directory and mounts it into the container at `/root/.gemini`, so the agent picks up your existing OAuth session without a manual login step.

If `~/.gemini/` does not exist on the host (you've never run `gemini` locally), amux creates an empty directory and mounts that instead. Gemini will prompt for authentication inside the container on first use.

The mount is a copy, not a bind mount — changes the agent makes to its auth state inside the container are isolated and do not affect the live `~/.gemini/` on your host.

### Auth precedence

When both an API key env var and OAuth tokens are present, Gemini uses the API key. This is Gemini's own resolution logic — amux does not arbitrate. If you want to use OAuth auth exclusively, omit the key variables from `envPassthrough`.

---

## Reference: `amux init`

```sh
amux init [--agent=<name>] [--aspec]
```

Initialises the current Git repository for use with amux. See [Getting Started](00-getting-started.md) for a full walkthrough.

| Flag | Values | Default |
|------|--------|---------|
| `--agent` | `claude`, `codex`, `opencode`, `maki`, `gemini` | `claude` |
| `--aspec` | (flag) | off |

`--aspec` downloads the `aspec/` folder from `github.com/prettysmartdev/aspec`, providing spec templates and work item scaffolding. Skipped without the flag.

---

## Reference: `amux ready`

```sh
amux ready [--refresh] [--build] [--no-cache] [--non-interactive] [--allow-docker]
```

Verifies your environment is ready for agent sessions.

| Flag | Description |
|------|-------------|
| `--refresh` | Run the Dockerfile agent audit and rebuild the image |
| `--build` | Force-rebuild the dev image from the current `Dockerfile.dev` |
| `--no-cache` | Pass `--no-cache` to all `docker build` invocations |
| `--non-interactive` | Run the audit agent in print mode |
| `--allow-docker` | Give the audit container access to the host Docker socket |

Use `--refresh` after your project's toolchain changes to update `Dockerfile.dev` and rebuild the image.

---

## Reference: all `implement` and `chat` flags

| Flag | `chat` | `implement` | Description |
|------|--------|-------------|-------------|
| `--non-interactive` | ✓ | ✓ | Print/batch mode |
| `--plan` | ✓ | ✓ | Read-only analysis mode |
| `--allow-docker` | ✓ | ✓ | Mount host Docker socket |
| `--mount-ssh` | ✓ | ✓ | Mount `~/.ssh` read-only |
| `--worktree` | — | ✓ | Run in isolated Git worktree |
| `--auto` | ✓ | ✓ | Auto-approve file edits, prompt for shell commands |
| `--yolo` | ✓ | ✓ | Fully autonomous mode |
| `--workflow=<path>` | — | ✓ | Multi-step workflow file |

---

[← Using the TUI](01-using-the-tui.md) · [Next: Security & Isolation →](03-security-and-isolation.md)
