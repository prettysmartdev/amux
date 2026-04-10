# Configuration

amux uses two JSON config files: a per-repository config and a global config. Most settings can live in either; the per-repo config takes precedence.

---

## Per-repository config

**Path:** `GITROOT/aspec/.amux.json`

This file is created by `amux init` and should be committed to source control. It configures amux for a specific project.

```json
{
  "agent": "claude",
  "terminal_scrollback_lines": 10000,
  "yoloDisallowedTools": ["Bash", "computer"]
}
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `agent` | string | `"claude"` | Agent to use for this repository: `claude`, `codex`, or `opencode` |
| `terminal_scrollback_lines` | integer | `10000` | Number of scrollback lines in the container terminal emulator. Overrides the global value |
| `yoloDisallowedTools` | string array | `[]` | Tools the agent cannot use when `--yolo` is active. Overrides the global list entirely |

---

## Global config

**Path:** `$HOME/.amux/config.json`

Applies to all projects on the machine unless overridden by a per-repo config.

```json
{
  "default_agent": "claude",
  "terminal_scrollback_lines": 10000,
  "runtime": "docker",
  "yoloDisallowedTools": ["Bash"]
}
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `default_agent` | string | `"claude"` | Default agent when no per-repo agent is configured |
| `terminal_scrollback_lines` | integer | `10000` | Default scrollback lines for all repos unless overridden |
| `runtime` | string | `"docker"` | Container runtime: `"docker"` or `"apple-containers"` (macOS 26+ only) |
| `yoloDisallowedTools` | string array | `[]` | Global fallback list of tools forbidden when `--yolo` is active |

**Note:** `runtime` is a global (machine-level) setting only. It is not available in the per-repo config — container runtime is a property of the machine, not the project.

---

## Config precedence

| Field | Precedence |
|-------|-----------|
| `agent` / `default_agent` | Per-repo → Global → Built-in default (`claude`) |
| `terminal_scrollback_lines` | Per-repo → Global → Built-in default (10,000) |
| `yoloDisallowedTools` | Per-repo → Global → Empty list (no restriction) |
| `runtime` | Global only |

For `yoloDisallowedTools`, if a per-repo list is set it **replaces** the global list entirely — lists are not merged. To inherit the global list for a repo, omit the field from the repo config.

A 10,000-line scrollback buffer at 80 columns uses approximately 3 MB per tab. Increase for long-running build or test sessions; decrease when running many simultaneous tabs.

---

## Runtime selection

amux supports two container runtimes. Switching runtimes requires no changes to your `Dockerfile.dev`, workflow files, or any other project config.

| Runtime | Value | Platform | Requirement |
|---------|-------|----------|-------------|
| Docker | `"docker"` | macOS, Linux, Windows | Docker daemon running |
| Apple Containers | `"apple-containers"` | macOS 26+ only | `container` CLI in PATH |

Set the runtime in your global config:

```json
{ "runtime": "apple-containers" }
```

An unrecognised value (e.g. a typo) falls back to `"docker"` with a warning — your workflow is not broken, but you should fix the value.

### Verifying runtime

`amux ready` validates the configured runtime before any other checks and prints which is active:

```
Runtime: docker (daemon running)
```

If the runtime is unavailable, `ready` exits immediately with a clear message:

```
error: runtime 'apple-containers' is not available: 'container' not found in PATH.
Install Apple Containers (macOS 26+) or set "runtime": "docker" in your config.
```

### Apple Containers runtime

Apple Containers (`container` CLI, macOS 26+) is an OCI-compatible container runtime. It supports Dockerfiles natively and amux maps every operation to the equivalent `container` CLI invocation. The user experience is identical to the Docker runtime.

**Limitations:**

- **`--allow-docker`**: Docker socket passthrough is not meaningful under Apple Containers. Passing `--allow-docker` produces a warning and the socket is not mounted. If your task needs Docker-in-container, switch to the Docker runtime.
- **Nanoclaw (`amux claws`)**: Nanoclaw requires detached container mode and Docker socket access. `claws init`, `claws ready`, and `claws chat` are not supported with `"apple-containers"`. Use the Docker runtime for nanoclaw.
- **macOS only**: If `"apple-containers"` is configured on Linux or Windows, amux exits with an error at startup rather than silently falling back to Docker.

---

## Build & development

```sh
make all                      # cargo build --release
make install                  # build + install to /usr/local/bin/ (may need sudo)
make test                     # cargo test
make clean                    # cargo clean
make release VERSION=v1.0.0   # create and publish a release
```

### Releasing

`make release VERSION=vx.y.z` automates the full release process:

1. Switches to `main`, pulls latest, and verifies a clean working tree
2. Creates `docs/releases/vx.y.z.md` with a release notes template
3. Launches `amux chat` to prompt an agent to write release notes
4. Runs all tests locally
5. Commits the release notes and tags the commit with the version
6. Pushes the commit and tag to `main`
7. Creates a GitHub Release with the release notes via `gh`

The tag push triggers the release CI pipeline, which builds binaries for all platforms and uploads them to the GitHub Release.

---

[← Nanoclaw](06-nanoclaw.md) · [Architecture →](architecture.md)
