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
  "yoloDisallowedTools": ["Bash", "computer"],
  "envPassthrough": ["ANTHROPIC_API_KEY", "OPENAI_API_KEY"]
}
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `agent` | string | `"claude"` | Agent to use for this repository: `claude`, `codex`, `opencode`, `maki`, or `gemini` |
| `terminal_scrollback_lines` | integer | `10000` | Number of scrollback lines in the container terminal emulator. Overrides the global value |
| `yoloDisallowedTools` | string array | `[]` | Tools the agent cannot use when `--yolo` is active. Overrides the global list entirely |
| `envPassthrough` | string array | `[]` | Host environment variable names to inject into agent containers at launch. Overrides the global list entirely. See [`envPassthrough`](#envpassthrough) |

---

## Global config

**Path:** `$HOME/.amux/config.json`

Applies to all projects on the machine unless overridden by a per-repo config.

```json
{
  "default_agent": "claude",
  "terminal_scrollback_lines": 10000,
  "runtime": "docker",
  "yoloDisallowedTools": ["Bash"],
  "envPassthrough": ["ANTHROPIC_API_KEY"]
}
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `default_agent` | string | `"claude"` | Default agent when no per-repo agent is configured: `claude`, `codex`, `opencode`, `maki`, or `gemini` |
| `terminal_scrollback_lines` | integer | `10000` | Default scrollback lines for all repos unless overridden |
| `runtime` | string | `"docker"` | Container runtime: `"docker"` or `"apple-containers"` (macOS 26+ only) |
| `yoloDisallowedTools` | string array | `[]` | Global fallback list of tools forbidden when `--yolo` is active |
| `envPassthrough` | string array | `[]` | Host environment variable names to inject into agent containers at launch. See [`envPassthrough`](#envpassthrough) |

**Note:** `runtime` is a global (machine-level) setting only. It is not available in the per-repo config — container runtime is a property of the machine, not the project.

---

## Config precedence

| Field | Precedence |
|-------|-----------|
| `agent` / `default_agent` | Per-repo → Global → Built-in default (`claude`) |
| `terminal_scrollback_lines` | Per-repo → Global → Built-in default (10,000) |
| `yoloDisallowedTools` | Per-repo → Global → Empty list (no restriction) |
| `envPassthrough` | Per-repo → Global → Empty list (no passthrough) |
| `runtime` | Global only |

For `yoloDisallowedTools` and `envPassthrough`, if a per-repo list is set it **replaces** the global list entirely — lists are not merged. To inherit the global list for a repo, omit the field from the repo config.

A 10,000-line scrollback buffer at 80 columns uses approximately 3 MB per tab. Increase for long-running build or test sessions; decrease when running many simultaneous tabs.

---

## `envPassthrough`

`envPassthrough` is an allowlist of host environment variable names that amux reads from your current shell and injects into agent containers at launch time. It applies to all agents — not just maki — but is the primary way to authenticate agents that use API keys rather than a system keychain.

### Why an allowlist?

amux deliberately cannot forward your entire host environment into a container. You must name each variable explicitly. This preserves the security principle that containers receive only the minimum secrets they need.

### Configuration

Add the field to your global config to apply it to all projects:

```json
{
  "envPassthrough": ["ANTHROPIC_API_KEY", "OPENAI_API_KEY"]
}
```

Or add it to a per-repo config to apply it to one project only:

```json
{
  "agent": "maki",
  "envPassthrough": ["ANTHROPIC_API_KEY", "ZHIPU_API_KEY"]
}
```

When a variable is listed but not present in your shell environment, it is silently skipped — no error or warning is produced. This is intentional: you may list variables that are only set in some contexts (e.g. CI vs. local).

### Using maki with `envPassthrough`

Maki authenticates exclusively via API keys. There is no system keychain integration. A typical maki setup looks like:

**Global config** (`~/.amux/config.json`):
```json
{
  "envPassthrough": ["ANTHROPIC_API_KEY", "OPENAI_API_KEY"]
}
```

**Per-repo config** (`aspec/.amux.json`):
```json
{
  "agent": "maki"
}
```

With this setup, `amux chat` reads `ANTHROPIC_API_KEY` and `OPENAI_API_KEY` from your shell and passes them into the maki container as `-e` flags on the `docker run` invocation. The values are masked (`***`) in all displayed Docker commands.

### Using gemini with `envPassthrough`

Gemini supports API-key-based authentication via `envPassthrough`. A typical setup for users with a Google AI Studio key:

**Global config** (`~/.amux/config.json`):
```json
{
  "envPassthrough": ["GEMINI_API_KEY"]
}
```

**Per-repo config** (`aspec/.amux.json`):
```json
{
  "agent": "gemini"
}
```

For Vertex AI, include the relevant variables:

```json
{
  "envPassthrough": ["GOOGLE_API_KEY", "GOOGLE_CLOUD_PROJECT", "GOOGLE_CLOUD_LOCATION", "GOOGLE_GENAI_USE_VERTEXAI"]
}
```

In addition to `envPassthrough`, amux automatically copies `~/.gemini/` (your OAuth token directory) into a temporary directory and mounts it at `/root/.gemini` inside the container. This means that if you've already authenticated gemini on the host (`gemini auth login`), the container picks up your session automatically with no extra config. See [Gemini authentication](02-agent-sessions.md#gemini-authentication) for the full auth details.

### Precedence and deduplication

Per-repo config wins entirely over global config — lists are not merged. To use the global list for a specific repo, omit `envPassthrough` from the repo config.

If a variable name appears in both `envPassthrough` and the agent's keychain credentials (e.g. a user who configured `CLAUDE_CODE_OAUTH_TOKEN` in both places), the keychain value takes precedence and the passthrough entry for that name is skipped.

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
