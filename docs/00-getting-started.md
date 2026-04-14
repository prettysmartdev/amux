# Getting Started with amux

`amux` is a terminal multiplexer for AI code agents. Every agent action runs inside a Docker container — never directly on your machine. This makes agent sessions reproducible, isolated, and safe to run autonomously.

This guide walks you through the core concepts and gets you to your first working agent session.

---

## Core concepts

Before running a single command, it helps to understand what amux is actually doing.

### Why containers?

When a code agent runs on your machine directly, it has access to your home directory, SSH keys, credentials, environment variables, and everything else your user account can touch. A bug in the agent — or a poorly-scoped task — can cause unintended side effects.

amux solves this by running agents inside containers. The container only sees your project directory (mounted read-only by default). Your credentials are injected as environment variables, not mounted as files. Your SSH keys are never exposed unless you explicitly opt in. The container is thrown away when the session ends.

### What is `Dockerfile.dev`?

`Dockerfile.dev` is the blueprint for the container your agents run inside. It defines:

- The base OS and language runtimes (Node, Python, Rust, Go, etc.)
- Build tools, test runners, and linters specific to your project
- Any services the agent needs to run tests (e.g. databases)

amux ships a generic template, but the template won't know your project's specific toolchain. The **agent audit** (run via `amux ready --refresh` or during `amux init`) launches an agent to inspect your codebase and update `Dockerfile.dev` with the exact tools your project needs. You should re-run the audit any time your toolchain changes significantly.

You own `Dockerfile.dev` — check it into source control, review it, and edit it like any other project file.

### What is `aspec`?

`aspec` is an optional folder of Markdown specification files that describe your project to agents. Before writing any code, agents read these specs to understand the architecture, security constraints, coding conventions, and what "done" looks like for a given task.

The spec folder typically contains:

| File | Purpose |
|------|---------|
| `foundation.md` | Project purpose, language choices, personas |
| `architecture/design.md` | Patterns, module layout, design decisions |
| `architecture/security.md` | Security constraints (non-negotiable) |
| `uxui/cli.md` | CLI commands, flags, and config |
| `devops/localdev.md` | How to build, test, and run locally |
| `work-items/` | Individual feature, bug, task, and enhancement specs |

`aspec` is optional but strongly recommended. Without it, agents have to guess at the intent and context behind a task. With it, agents make decisions consistent with the rest of the codebase.

### Work items

A work item is a Markdown file that describes a specific piece of work: a feature, bug fix, enhancement, or task. Work items follow a numbered naming convention (`0001-add-auth.md`, `0002-fix-crash.md`) and contain everything the agent needs to implement, test, and document the change.

By default, amux looks for work items in `aspec/work-items/`. If your repo uses a different folder structure, you can configure the path:

```sh
amux config set work_items.dir docs/work-items
```

When you run `amux implement 0001`, amux finds the matching file in the configured directory, constructs a prompt from its contents, and launches the agent inside a container to do the work.

---

## Prerequisites

- **Git** — your project must be in a Git repository
- **A container runtime** — one of:
  - **Docker** (default, all platforms) — a running Docker daemon ([install Docker](https://docs.docker.com/get-docker/))
  - **Apple Containers** (macOS 26+ only) — Apple's native `container` CLI; no Docker Desktop required
- **A supported agent** — one of: Claude Code, OpenAI Codex, OpenCode, Maki, or Google Gemini CLI installed and authenticated on your machine

---

## Installation

```sh
curl -s https://prettysmart.dev/install/amux.sh | sh
```

The installer detects your platform and installs `amux` to `/usr/local/bin`.

<details>
<summary>Other installation options</summary>

**From GitHub Releases** — download the binary for your platform from the [Releases page](https://github.com/prettysmartdev/amux/releases), make it executable, and move it onto your `PATH`:

```sh
chmod +x amux-*
mv amux-* /usr/local/bin/amux
```

**From source** — requires Rust 1.94+ and `make`:

```sh
git clone https://github.com/prettysmartdev/amux.git
cd amux
make install    # builds and installs to /usr/local/bin/amux
```

</details>

---

## First-time project setup

Navigate to your project's Git root and run:

```sh
amux init
```

This does several things:

1. Writes `.amux/config.json` (per-repo config) with the chosen agent
2. Downloads `Dockerfile.dev` from the agent template
3. Offers to run the **agent audit** — launches a container that inspects your project and updates `Dockerfile.dev` with the tools your codebase actually needs. It's strongly advised that you accept; it's the main reason `Dockerfile.dev` exists.
4. Builds the dev container image from the (now-customised) `Dockerfile.dev`
5. Prints a summary table showing the result of each step

The init summary looks like this:

```
┌──────────────────────────────────────────────────┐
│              Init Summary (claude)                │
├───────────────────┬──────────────────────────────┤
│            Config │ ✓ saved                       │
│      aspec folder │ – use --aspec to download     │
│    Dockerfile.dev │ ✓ created                     │
│       Agent audit │ ✓ completed                   │
│      Docker image │ ✓ built                       │
│       Work items  │ ✓ configured                  │
└───────────────────┴──────────────────────────────┘
```

The **Work items** row appears when `--aspec` is not passed and no `aspec/` folder exists. `init` offers to set a custom work items directory interactively during setup. If you decline or already have `aspec/`, the row shows `– not needed`.

To also download the `aspec/` folder with spec templates and work item scaffolding:

```sh
amux init --aspec
```

---

## Verifying your environment

After init, run:

```sh
amux ready
```

This checks:

1. That your container runtime (Docker or Apple Containers) is available and running
2. That `Dockerfile.dev` exists and is configured
3. That your agent (e.g. Claude Code) is installed and authenticated — it sends a test greeting and shows the response
4. That the dev container image has been built

If everything is green, you're ready to run agents.

### Re-running the Dockerfile audit

If your project's toolchain has changed (you added a new language, test framework, or dependency), update your `Dockerfile.dev` by re-running the audit:

```sh
amux ready --refresh
```

This launches the audit agent, updates `Dockerfile.dev`, and rebuilds the image. You should commit the updated `Dockerfile.dev` to source control.

---

## Opening the TUI

```sh
amux
```

This opens the interactive TUI. You'll see:

- A **tab bar** at the top (one tab per project session)
- An **execution window** in the middle (shows command output)
- A **command input box** at the bottom

Type any amux subcommand (like `chat`) and press **Enter** to run it. The TUI supports autocomplete — start typing and suggestions appear below the input.

---

## Your first agent session

### Freeform chat

```sh
chat
```

(Type this in the TUI command box and press Enter, or run `amux chat` from your terminal.)

This launches an agent session in a container against your project. A **container window** opens over the execution window — this is a full terminal emulator connected to the agent. You can type directly to the agent, ask questions, request changes, and see output in real time.

Press **Esc** to minimize the container window (the agent keeps running in the background). Press **c** to bring it back.

### Implementing a work item

If you have a work item at `aspec/work-items/0001-add-auth.md`:

```sh
implement 0001
```

amux finds the file, builds a structured prompt from its contents, and launches the agent in a container. The agent reads the spec, writes code, runs tests, and reports back — all inside the container.

---

## Creating work items

```sh
specs new               # prompts for type and title, creates the file
specs new --interview   # creates the skeleton, then opens an agent to help fill it out
```

Four work item types are available: Feature, Bug, Task, and Enhancement.

Work items are created in the configured work items directory (defaulting to `aspec/work-items/`). If you haven't run `amux init --aspec` and haven't configured `work_items.dir`, amux will prompt you to auto-discover a template or create the file with a minimal stub. You can configure a custom directory at any time:

```sh
amux config set work_items.dir docs/work-items
```

With `--interview`, after you provide a brief summary, the agent asks clarifying questions and writes out the full spec (user stories, implementation plan, edge cases, test plan) before any implementation starts.

After implementing a work item, you can have the agent update the spec to match what was actually built:

```sh
specs amend 0001
```

---

## What's next

- **[Using the TUI](01-using-the-tui.md)** — tabs, keyboard shortcuts, container window controls, scrollback
- **[Agent Sessions](02-agent-sessions.md)** — all `chat` and `implement` flags, authentication, work item management
- **[Security & Isolation](03-security-and-isolation.md)** — worktrees, SSH keys, Docker socket access
- **[Workflows](04-workflows.md)** — multi-step agent runs with plan → implement → review phases
- **[Yolo Mode](05-yolo-mode.md)** — fully autonomous operation for long-running tasks
- **[Nanoclaw](06-nanoclaw.md)** — persistent 24/7 background agents
- **[Configuration](07-configuration.md)** — all config file options

---

[Next: Using the TUI →](01-using-the-tui.md)
