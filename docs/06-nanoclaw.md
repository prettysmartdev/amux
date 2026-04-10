# Nanoclaw

The `claws` commands manage a **persistent nanoclaw agent** — a machine-global, always-on background agent that runs in Docker and survives across amux sessions and reboots.

Unlike `chat` and `implement` sessions (ephemeral, per-project, discarded when the session ends), the nanoclaw container is long-lived and machine-global. It's designed for ongoing, cross-project work where you want a continuously available agent that accumulates context over time.

---

## When to use nanoclaw

Nanoclaw is useful for:

- Long-running tasks that span multiple days or sessions
- Cross-project work where you don't want to set up a per-project container each time
- Monitoring, scheduled, or reactive tasks that need to run in the background
- Keeping an always-available agent accessible via messaging apps (Slack, Discord, WhatsApp)

For per-project, task-specific work, use `amux chat` or `amux implement` instead.

---

## First-time setup

```sh
amux claws init
```

The setup wizard walks through:

1. **Fork check** — asks whether you've already forked nanoclaw on GitHub
   - **Yes** — prompts for your GitHub username and clones `github.com/<username>/nanoclaw` to `$HOME/.nanoclaw`
   - **No** — offers to fork and clone using the GitHub CLI (`gh repo fork`); provides manual instructions if you decline
2. **Docker daemon check** — verifies Docker is running
3. **Dockerfile setup** — writes `Dockerfile.dev` inside the nanoclaw repo and builds the `amux-nanoclaw:latest` image
4. **Agent audit** — runs a nanoclaw-specific audit to update `Dockerfile.dev` and configure container-to-container networking
5. **Docker socket warning** — explains that the nanoclaw container requires host Docker socket access (elevated, like `--allow-docker`) and requires explicit acceptance
6. **`/setup` reminder** — reminds you to run `/setup` inside the agent after launching (CLI only; TUI shows this as a dialog)
7. **Container launch** — starts the nanoclaw container in the background, waits for it to reach running state, saves the container ID to `$HOME/.nanoclaw/.amux.json`
8. **Attach** — attaches to the running container and launches the configured agent interactively

In the TUI, the wizard is presented via modal dialogs and the audit agent runs in the tab's container window. In command mode, the wizard runs interactively on stdin.

---

## Checking nanoclaw status

```sh
amux claws ready
```

Checks whether the nanoclaw container is running.

- **Not installed** (`$HOME/.nanoclaw` missing) — prints a message suggesting `amux claws init`; exits without error
- **Container running** — shows a status summary table and exits
- **Container stopped** — interactively offers to start the container in the background; saves the new container ID if accepted

Run `amux claws ready` after a reboot or if you're not sure whether the container is up.

---

## Attaching to a running nanoclaw session

```sh
amux claws chat
```

Attaches to the running nanoclaw container for an interactive agent session. Identical to `amux chat`, but connected to the persistent container.

| Situation | Behaviour |
|-----------|-----------|
| Not installed | Error; suggests `claws init` |
| Container not running | Error; suggests `claws ready` |
| Container running | Attaches interactively |

**Press Ctrl+C to detach** — the container continues running in the background. Run `claws chat` again to re-attach at any time.

In the TUI, the nanoclaw tab is shown in **purple**.

---

## Authentication

The nanoclaw container is authenticated using the same keychain passthrough as `chat` and `implement`. No manual login is required.

---

## Docker socket access

The nanoclaw container always mounts the host Docker socket. This is required for nanoclaw to manage containers on your behalf. A warning is shown and explicit acceptance is required during `claws init`.

This grants the nanoclaw agent root-equivalent access to your host Docker daemon — the same as passing `--allow-docker` on a regular session. This is intentional; nanoclaw is designed to operate as a persistent, elevated agent.

**Nanoclaw is not supported with the Apple Containers runtime.** The nanoclaw container requires detached container mode and Docker socket access, both of which depend on the Docker runtime. Use the Docker runtime for nanoclaw sessions. See [Configuration](07-configuration.md#runtime-selection).

---

## Configuration

The container ID is stored at `$HOME/.nanoclaw/.amux.json`:

```json
{
  "nanoclawContainerID": "abc123..."
}
```

The nanoclaw repo itself lives at `$HOME/.nanoclaw/`.

---

## Difference from ephemeral sessions

| | `chat` / `implement` | Nanoclaw |
|---|---|---|
| Lifetime | Single session; removed on exit | Persistent; survives reboots |
| Scope | Per-project | Machine-global |
| Docker socket | Opt-in (`--allow-docker`) | Always mounted |
| TUI tab colour | Green | Purple |
| Start command | `amux chat` | `amux claws chat` |
| Stop | Agent exits or Ctrl+C | Container continues; Ctrl+C detaches |

---

[← Yolo Mode](05-yolo-mode.md) · [Next: Configuration →](07-configuration.md)
