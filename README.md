<p align="center">
  <strong>Secure, Predictable Code and Claw Agents</strong> <br>
  Run agents inside containers, not on your machine.<br>
  Use structured Markdown specs to guide agents for predictable results.
  <br>
  <br>
  <img src="./docs/amux_logo_v3.svg" width="420" alt="AMUX">
</p>


<p align="center">
  <img src="https://github.com/cohix/amux/actions/workflows/test.yml/badge.svg">

## What is `amux`?

Claw-like and code agents are powerful, but running them directly on your machine means giving them unrestricted access to your filesystem, environment, and credentials. One bad prompt or hallucination can be dangerous.

**amux** is a CLI that takes a different approach:

- **Containerized by design** — every agent action runs inside a Docker container. Your host machine is never exposed to agent-generated code execution.
- **Spec-driven development** — define your project's architecture, security constraints, and design decisions in structured spec files. Agents read these specs to produce code that fits your project, not just code that compiles.
- **Predictable workflows** — work items, environment setup, and agent sessions follow a repeatable process. No more ad-hoc prompting with inconsistent results.
- **Agent-agnostic** — supports Nanoclaw, Claude Code, Codex, and OpenCode out of the box. Swap agents without changing your workflow.

`amux` is built on the idea that agents are best used when they have clear context (specs) and hard boundaries (containers).

`amux` pairs with `aspec` for spec-driven-development templates. Learn more about the [aspec methodology](https://github.com/cohix/aspec).

---

## Quick Start

See the [Getting Started Guide](docs/getting-started.md) for a complete walkthrough.

All agent sessions run in Docker containers with your project directory mounted. The agent can read and write your code, but cannot execute anything on your host.

---
## Security first

amux enforces a hard security boundary: **agents never execute on the host machine**.

- All agent code runs inside Docker containers built from `Dockerfile.dev`
- Only the current Git repository is mounted — never parent directories
- amux itself is a statically compiled Rust binary - memory safe and immutable by agents
- Docker commands are displayed in full so you can see exactly what runs

See the [Usage Guide](docs/usage.md#agent-authentication) for details on credential handling.

---

## Spec-driven for predictability

`aspec` templates can be added to your project by `amux init`, allowing you to define the entire structure and best practices for your project:
- Architecture, design, components, and infrastruction
- Security, operations, CI/CD, and local development
- Work items that define specs for a new feature, bugfix, or enhancement

Your agent co-worker uses the project's `aspec` to remain grounded in your desired software development standards, and keeps everyting that was fed into the agent directly within the project iself. Agents gain sorely needed context, humans gain predictability by covering edge cases, testing, and more.

---

## Installation

### From releases

Download the latest binary for your platform from [GitHub Releases](https://github.com/cohix/amux/releases).

| Platform | Binary |
|----------|--------|
| Linux (x86_64) | `amux-linux-amd64` |
| Linux (ARM64) | `amux-linux-arm64` |
| macOS (Intel) | `amux-macos-amd64` |
| macOS (Apple Silicon) | `amux-macos-arm64` |
| Windows (x86_64) | `amux-windows-amd64.exe` |

### From source

Requires Rust 1.94+ and make:

```sh
git clone https://github.com/cohix/amux.git
cd amux
make install
```
---

## Development

```sh
make all      # build the amux binary
make install  # build + install to /usr/local/bin/
make test     # run all tests
make clean    # clean build artifacts
```
---

## Full Documentation

- [Getting Started](docs/getting-started.md) — installation and first workflow
- [Usage Guide](docs/usage.md) — all commands, flags, TUI reference, and configuration
- [Architecture](docs/architecture.md) — code structure, design patterns, and testing strategy

---

## License

See [LICENSE](LICENSE) for details.
