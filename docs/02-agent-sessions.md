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

`implement` finds the work item file matching `0001-*.md` in the configured work items directory, builds a structured prompt from its contents, and launches the agent in a container. The prompt instructs the agent to implement the work item, iterate on builds and tests, write documentation, and report when complete.

By default, amux looks in `aspec/work-items/`. If your repo uses a different layout, configure the path with `amux config set work_items.dir <path>`. See [Work item paths](07-configuration.md#work-item-paths) for the full resolution order.

The work item number can be written with or without leading zeros: `1` and `0001` are equivalent.

After the agent launches, you can interact with it directly — add follow-up instructions, review output, or let it run autonomously. Press **Ctrl+C** or type `exit` in the agent to end the session.

---

## Flags common to `chat` and `implement`

### `--agent <name>`

Override the configured agent for this session. Available agents: `claude`, `codex`, `opencode`, `maki`, `gemini`.

```sh
# CLI
amux chat --agent codex               # launch a Codex session for this project
amux implement 0050 --agent gemini    # implement with Gemini instead of the configured agent
amux chat --agent=codex               # --flag=value form is also accepted

# TUI command box
chat --agent codex
implement 0042 --agent=opencode
```

Both `--agent NAME` and `--agent=NAME` forms are accepted in both the CLI and the TUI command box. The TUI command box honours the flag and passes the correct agent to the container — it is not silently ignored.

This overrides the `agent` field in your repo config for this run only — no config file is modified. amux uses the agent-specific image (`amux-{project}-{agent}:latest`) for the session.

If the agent image does not yet exist, amux offers to download the template and build both the project base image (if needed) and the agent image before launching.

Passing an unknown agent name exits immediately with a list of valid options:

```
error: unknown agent "foo"; available agents: claude, codex, opencode, maki, gemini
```

### `--model <NAME>`

Override the model used by the launched agent for this session.

```sh
# CLI
amux chat --model claude-opus-4-6
amux implement 0050 --model claude-haiku-4-5
amux chat --model=gpt-4o               # --flag=value form is also accepted

# TUI command box
chat --model claude-opus-4-6
implement 0042 --model=claude-haiku-4-5
```

Both `--model NAME` and `--model=NAME` forms are accepted in both the CLI and the TUI command box.

The model name is passed verbatim to the agent's own model flag — amux does not validate the value. If the name is not recognised by the agent, the agent surfaces its own error. This means any model the agent supports can be used without amux needing updates when providers release new models.

Per-agent translation:

| Agent | Flag appended |
|-------|--------------|
| `claude` | `--model <NAME>` |
| `codex` | `--model <NAME>` |
| `gemini` | `--model <NAME>` |
| `opencode` | `--model <NAME>` |
| `maki` | `--model <NAME>` |

If an agent does not support `--model`, a `WARNING:` is printed to stderr and the session launches without the flag — it is not aborted.

`--model` can be combined freely with `--agent`, `--yolo`, `--auto`, and all other flags. When used with `--workflow` on `implement`, the flag value acts as the default model for every workflow step that does not define its own `Model:` field. See [Per-step model overrides](04-workflows.md#per-step-model-overrides).

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

Prompts for a type (Feature, Bug, Task, or Enhancement) and a title, then creates a numbered work item file in the configured work items directory using the project's template.

By default, amux writes to `aspec/work-items/` and uses `aspec/work-items/0000-template.md`. If neither exists, amux auto-discovers any `*template.md` file in the work items directory and prompts you to confirm it. You can also configure the paths explicitly:

```sh
amux config set work_items.dir docs/work-items
amux config set work_items.template docs/work-items/my-template.md
```

If no template is found or confirmed, the new file is created with a minimal stub (`# Kind: Title`). See [Work item paths](07-configuration.md#work-item-paths) for full details on path resolution and auto-discovery.

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

When `--aspec` is not passed and no `aspec/` folder exists, `init` offers to configure a custom work items directory and template path interactively. This sets `work_items.dir` (and optionally `work_items.template`) in the repo config so `specs new` and `implement` work without requiring the `aspec/` folder layout. See [Work item paths](07-configuration.md#work-item-paths).

---

## Reference: `amux ready`

```sh
amux ready [--refresh] [--build] [--no-cache] [--non-interactive] [--allow-docker]
```

Verifies your environment is ready for agent sessions.

| Flag | Description |
|------|-------------|
| `--refresh` | Run the Dockerfile agent audit, update `Dockerfile.dev`, and rebuild both images |
| `--build` | Rebuild the project base image and agent images in `.amux/`. When multiple agent Dockerfiles exist, amux asks which to build |
| `--no-cache` | Pass `--no-cache` to every `docker build` invocation, including the project base image and all agent images |
| `--non-interactive` | Run the audit agent in print mode |
| `--allow-docker` | Give the audit container access to the host Docker socket |

Use `--refresh` after your project's toolchain changes to update `Dockerfile.dev` (the project base) and rebuild both images. The agent dockerfile is not touched by the audit.

### Rebuilding multiple agent images

If your `.amux/` directory contains Dockerfiles for more than one agent (for example, `.amux/Dockerfile.claude` and `.amux/Dockerfile.codex`), running `amux ready --build` prompts before starting any builds:

```
Found 2 agent Dockerfiles:
  claude  (default)
  codex   (extra)

Build all agent images, or only the default (claude)? [all/default]:
```

- **all** — builds the project base image, then all agent images in `.amux/`, in sequence.
- **default** — builds the project base image and only the default agent image from config.

The `--no-cache` flag applies to every image built in this sequence.

### Build output

Each image build — project base or agent — is framed with prominent start and end markers so you can track progress across a multi-image sequence:

```
══════════════════════════════════════════════════
  Building project base image: amux-myproject:latest
══════════════════════════════════════════════════
[build output...]

══════════════════════════════════════════════════
  ✓ Built amux-myproject:latest
══════════════════════════════════════════════════


══════════════════════════════════════════════════
  Building agent image: amux-myproject-codex:latest
══════════════════════════════════════════════════
[build output...]
```

This applies whenever `ready` starts a build — `--build`, `--refresh`, or the initial `amux init` sequence.

### Migration from single-file layout

If you have an existing `Dockerfile.dev` that bundles agent tooling (the layout used before amux 0.6), `amux ready` detects this and offers a guided migration:

```
Detected legacy single-file Dockerfile.dev layout.
Would you like to migrate to the modular layout? (agent tools move to .amux/Dockerfile.{agent})

Migrating will:
  1. Recreate Dockerfile.dev with a minimal debian:bookworm-slim base
  2. Write .amux/Dockerfile.{agent} using the agent template
  3. Build both images
  4. Run the audit agent to restore project dependencies in Dockerfile.dev

[y/N]:
```

If you accept, amux handles the entire migration automatically. Commit the resulting `Dockerfile.dev` and `.amux/Dockerfile.{agent}` to source control.

If you decline, your existing image continues to work for the current session with a deprecation warning printed each time.

When `amux chat` or `amux implement` encounters the legacy layout (before you run `amux ready` to migrate), it exits with a short message:

```
Run `amux ready` to migrate to the modular Dockerfile layout, or pass `--no-migrate` to use the existing image.
```

`amux ready` also checks whether work item paths are configured. If neither `aspec/work-items/` exists nor `work_items.dir` is set, the summary shows a `⚠ not configured` warning (not a failure) for the `work items config` row, and prints a tip to run `amux config set work_items.dir <path>`.

---

## Reference: all `implement` and `chat` flags

| Flag | `chat` | `implement` | Description |
|------|--------|-------------|-------------|
| `--agent=<name>` | ✓ | ✓ | Override the agent for this session |
| `--model=<NAME>` | ✓ | ✓ | Override the model used by the agent |
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
