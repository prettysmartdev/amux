# Getting Started with aspec

This guide walks you through installing `aspec` and running your first agentic coding session.

---

## Prerequisites

- **Git** — your project must be a Git repository
- **Docker** — a running Docker daemon ([Install Docker](https://docs.docker.com/get-docker/))
- **make** — standard on macOS and Linux; on Windows use WSL or `cargo` directly

---

## Installation

### From source

Clone the repository and build:

```sh
git clone https://github.com/cohix/aspec-cli.git
cd aspec-cli
make install    # builds and installs to /usr/local/bin/aspec
```

To install to a different location:

```sh
INSTALL_PATH=~/bin make install
```

### From GitHub Releases

Download the latest binary for your platform from the
[Releases page](https://github.com/cohix/aspec-cli/releases), make it
executable, and move it to a directory on your `PATH`:

```sh
chmod +x aspec-*
mv aspec-* /usr/local/bin/aspec
```

---

## Quick Start Workflow

### 1. Initialise your repository

Navigate to your project's Git root and run:

```sh
aspec init
```

This creates the `aspec/` directory with project specification templates and a
`Dockerfile.dev` for building your development container. By default, the
Claude agent is used. To choose a different agent:

```sh
aspec init --agent=codex      # or opencode
```

See [init reference](usage.md#aspec-init---agentname) for details.

### 2. Verify your environment

```sh
aspec ready
```

This checks that Docker is running, `Dockerfile.dev` exists, and the dev
container image is built. If the image does not exist yet, `aspec ready`
builds it automatically.

On your first run (or after changing project dependencies), use `--refresh`
to have the agent audit your Dockerfile and add any missing tools:

```sh
aspec ready --refresh
```

See [ready reference](usage.md#aspec-ready---refresh---build---no-cache---non-interactive) for all flags and options.

### 3. Start a chat session

To explore the codebase or ask questions before implementing anything:

```sh
aspec chat
```

This launches an interactive session with your agent inside a Docker container.
The agent has full access to your project files (mounted read-write) but runs
entirely inside the container — nothing executes on your host machine.

See [chat reference](usage.md#aspec-chat---non-interactive---plan) for details.

### 4. Implement a work item

Create a work item describing what you want to build:

```sh
aspec new
```

Follow the prompts to choose a type (Feature, Bug, or Task) and enter a title.
This creates a new file in `aspec/work-items/` from the template. Edit the file
to describe the implementation details, edge cases, and test considerations.

Then implement it:

```sh
aspec implement 0001
```

The agent launches in interactive mode inside a container. It reads the work
item, implements the changes, runs tests, and writes documentation — all while
you can observe and interact with it in real time.

See [implement reference](usage.md#aspec-implement-nnnn---non-interactive---plan) for details.

---

## Interactive TUI Mode

Running `aspec` with no arguments opens the interactive TUI:

```sh
aspec
```

The TUI provides a persistent command box where you can type any subcommand.
Autocomplete suggestions appear as you type. When an agent runs, a dedicated
container window shows the live session with Docker stats.

See the [TUI reference](usage.md#interactive-tui-reference) for keyboard
shortcuts and layout details.

---

## Plan Mode

To have the agent analyse code and suggest changes without modifying any files,
use plan mode:

```sh
aspec implement 0001 --plan
aspec chat --plan
```

This is useful for code review, architecture exploration, or understanding
unfamiliar code before making changes.

---

## What's Next

- Edit the spec files in `aspec/` to describe your project's architecture,
  security constraints, and design decisions — this gives the agent the context
  it needs to write better code
- Read the full [Usage Guide](usage.md) for all commands, flags, and
  configuration options
- Read the [Architecture Guide](architecture.md) to understand how `aspec`
  works internally
