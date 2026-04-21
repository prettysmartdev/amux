# amux Architecture

## High-level Overview

```
User
 │
 ▼
amux binary ──► command mode  ──► commands/{init,ready,implement,chat,new}
     │                                       │
     ├──────► interactive mode (TUI)         │
     │              │                        ▼
     │        tui/{mod,state,          runtime: AgentRuntime (Arc<dyn>)
     │         input,render,pty}             │
     │              │              ┌──────────┴──────────┐
     │              │         DockerRuntime       AppleContainersRuntime
     │              │              │                     │ (macOS 26+)
     │              ▼              ▼                     ▼
     │        Container Runtime ──────────────► Managed Container
     │          (Docker or                      (agent runs here)
     │       Apple Containers)
     │
     └──────► headless mode ──► commands/headless/{mod,server,db,process,logging}
                    │                        │
                    ▼                        ▼
             HTTP server (axum)      SQLite DB + log files
               localhost:<port>       ~/.amux/headless/
```

---

## Source Layout

```
src/
  main.rs                  Entry point: dispatch TUI or command mode
  lib.rs                   Re-exports public API for integration tests
  cli.rs                   clap CLI: Cli, Command, Agent enums
  config/
    mod.rs                 RepoConfig, GlobalConfig, load/save helpers,
                           DEFAULT_SCROLLBACK_LINES, effective_scrollback_lines()
  commands/
    mod.rs                 Public run() dispatcher
    spec.rs                CommandSpec + FlagSpec tables: canonical single source of truth
                           for all subcommand flags. Imported by cli.rs, tui/mod.rs, and
                           tui/input.rs. Never imports from those modules (leaf node).
    output.rs              OutputSink: routes output to stdout or TUI channel
    auth.rs                Agent credential path resolution, auth prompts
    agent.rs               Shared agent launching: run_agent_with_sink()
                           Used by both implement and chat
    download.rs            GitHub downloads: Dockerfile templates (raw files)
                           and aspec folder (tarball extraction)
    init_flow.rs           Canonical `init` engine (mode-agnostic). Owns all business logic:
                           InitFlow::execute(): sequential stage runner
                           InitQa trait: ask_replace_aspec(), ask_run_audit(), ask_work_items_setup()
                           InitContainerLauncher trait: build_image(), run_audit()
                           InitParams, InitSummary, and per-stage StepStatus
                           All helpers: write_project_dockerfile(), write_agent_dockerfile(),
                             download_or_fallback_dockerfile(), print_init_summary(), print_whats_next()
    init.rs                Thin CLI shim: constructs CliInitQa (stdin-backed) and
                           CliContainerLauncher (synchronous blocking), then delegates to
                           init_flow::execute(). Contains no business logic.
    new.rs                 `amux new` — run() + run_with_sink()
                           WorkItemKind, slugify, apply_template,
                           find_template, next_work_item_number
                           Auto-downloads aspec/ if template is missing
    ready.rs               `amux ready` — run() + run_with_sink()
                           ReadyOptions, ReadyContext, ReadySummary, AuditSetup
                           StepStatus, print_summary, print_interactive_notice,
                           audit_entrypoint, audit_entrypoint_non_interactive
                           Engine functions (called identically from CLI and TUI):
                             compute_ready_build_flag(refresh, build)
                             is_legacy_layout(git_root, agent_name)
                             perform_legacy_migration(git_root)
                             gather_ready_env_vars(git_root, agent_name)
                             create_ready_host_settings(agent_name)
                             apply_ready_user_directive(host_settings, ctx)
                             check_allow_docker(out, allow_docker, runtime)
                             build_audit_setup(ctx, non_interactive)
                           run_pre_audit(), run_post_audit()
    implement.rs           `amux implement` — run() + run_with_sink()
                           agent_entrypoint, agent_entrypoint_non_interactive
    chat.rs                `amux chat` — run() + run_with_sink()
                           chat_entrypoint, chat_entrypoint_non_interactive
    headless/
      mod.rs               Top-level dispatch: run_start, run_kill, run_logs, run_status
      server.rs            axum HTTP router + handlers; shared AppState (sessions, allowlist,
                           in-memory busy-session mutex); request/response types
      db.rs                SQLite schema setup (sessions + commands tables);
                           all data-access functions; session/command CRUD;
                           AMUX_HEADLESS_ROOT env override for test isolation
      process.rs           OS process manager integration: systemd-run (Linux),
                           launchd plist (macOS), double-fork fallback;
                           PID file write/read/delete; live-process detection
      logging.rs           tracing-subscriber setup: human-readable to stdout
                           (foreground) or JSON/appending to amux.log (background);
                           periodic heartbeat log every 60 s
  runtime/
    mod.rs                 AgentRuntime trait (all container operations);
                           resolve_runtime() factory (reads GlobalConfig);
                           HostSettings (sanitized config mount, shared by all runtimes);
                           ContainerStats; free utilities: generate_container_name,
                           project_image_tag, agent_image_tag, parse_cpu_percent,
                           parse_memory_mb, format_build_cmd, format_run_cmd
    docker.rs              DockerRuntime — implements AgentRuntime via the
                           `docker` CLI; replaces src/docker/mod.rs
    apple.rs               AppleContainersRuntime — implements AgentRuntime via
                           the `container` CLI; #[cfg(target_os = "macos")]
  tui/
    mod.rs                 run() entry point; event loop; action dispatcher;
                           ClipboardWriter trait; copy_selection_to_clipboard();
                           capture_vt100_snapshot(); extract_selection_text()
    state.rs               App struct; Focus/ExecutionPhase/Dialog enums;
                           PendingCommand (Ready/Implement/Chat with flags,
                             including agent: Option<String> on Chat and Implement);
                           TuiInitAnswers: pre-collected init Q&A answers for TuiInitQa;
                           ContainerWindowState, ContainerInfo,
                           LastContainerSummary; terminal selection state fields;
                           terminal_scrollback_lines; container_inner_area;
                           Tab.ready_summary: Option<ReadySummary> (stores
                           pre-audit summary for handoff to post-audit phase)
    input.rs               handle_key(); Action enum (incl. CopyToClipboard);
                           autocomplete (flag_suggestions_for() generated from
                             CommandSpec — no manual hint lists);
                           key→bytes; Ctrl+Y copy keybinding
    flag_parser.rs         parse_flags(): generic TUI flag parser driven by CommandSpec.
                           Handles both --flag value and --flag=value forms.
                           flag_bool() / flag_string() convenience helpers.
                           Replaces the deleted parse_chat_flags(),
                           parse_implement_flags(), and parse_agent_flag() functions.
    render.rs              draw(); draw_exec_window/command_box/dialog etc.;
                           render_vt100_screen/no_cursor (selection highlight);
                           cell_in_selection(); scrollback depth probe + indicator
    pty.rs                 PtySession; PtyEvent; spawn_text_command helper
templates/
  Dockerfile.project       Project base template: FROM debian:bookworm-slim;
                           installs git, curl, make, ca-certificates; no USER directive.
                           Written to GITROOT/Dockerfile.dev on init.
  Dockerfile.claude        Agent template: FROM {{AMUX_BASE_IMAGE}}; installs Claude Code;
                           creates non-root amux user. Written to .amux/Dockerfile.claude.
                           Bundled fallback via include_str!; primary source downloaded
                           from github.com/prettysmartdev/aspec-cli
  Dockerfile.codex         Agent template (same pattern as claude)
  Dockerfile.opencode      Agent template (same pattern as claude)
  Dockerfile.maki          Agent template (same pattern as claude)
  Dockerfile.gemini        Agent template (same pattern as claude)
  Dockerfile.nanoclaw      Nanoclaw persistent-agent template (see docs/06-nanoclaw.md)
tests/
  cli_integration.rs       Binary-level integration tests
  command_tui_parity.rs    Verifies command/TUI mode share the same logic
  dockerfile_build.rs      Builds each agent template Dockerfile to verify validity
  download_integration.rs  GitHub download tests: templates, aspec folder, fallback
  memory_bounds.rs         vt100 scrollback cap, tab cleanup, memory-per-tab bounds
  terminal_selection.rs    Text selection, clipboard (MockClipboard), scrollback depth,
                           coordinate mapping, resize-clears-selection integration tests
docs/
  usage.md                 End-user reference
  architecture.md          This file
```

---

## The `OutputSink` Abstraction

Every command function (`init::run_with_sink`, `ready::run_with_sink`, etc.) accepts
an `OutputSink` instead of calling `println!` directly:

```rust
pub enum OutputSink {
    Stdout,                               // command mode
    Channel(UnboundedSender<String>),     // TUI mode
}
```

`OutputSink` implements `Clone`, allowing it to be passed to streaming callbacks
like `runtime.build_image_streaming()`.

This is the core mechanism that allows zero code duplication between the two
execution modes. The command logic is identical — only the destination of output differs.

In command mode, `run()` wraps `run_with_sink(…, &OutputSink::Stdout)`.
In TUI mode, `execute_command()` passes `OutputSink::Channel(app.output_tx.clone())`.

---

## The `AgentRuntime` Abstraction

All container operations go through a single `AgentRuntime` trait defined in
`src/runtime/mod.rs`. This decouples the agent-launching logic from any
specific container technology.

```rust
pub trait AgentRuntime: Send + Sync {
    fn is_available(&self) -> bool;
    fn name(&self) -> &'static str;
    fn cli_binary(&self) -> &'static str;

    // Image lifecycle
    fn build_image(&self, tag: &str, dockerfile: &Path, context: &Path, no_cache: bool) -> Result<String>;
    fn build_image_streaming<F>(&self, ...) -> Result<String>;
    fn image_exists(&self, tag: &str) -> bool;

    // Container run variants
    fn run_container(&self, ...) -> Result<()>;
    fn run_container_captured(&self, ...) -> Result<(String, String)>;
    fn run_container_detached(&self, ...) -> Result<String>;
    // … additional run_container_at_path variants …

    // Container lifecycle
    fn start_container(&self, id: &str) -> Result<()>;
    fn stop_container(&self, id: &str) -> Result<()>;
    fn remove_container(&self, id: &str) -> Result<()>;
    fn is_container_running(&self, id: &str) -> bool;

    // Discovery & stats
    fn list_running_containers_by_prefix(&self, prefix: &str) -> Vec<String>;
    fn query_container_stats(&self, name: &str) -> Option<ContainerStats>;

    // PTY argument builders (for TUI interactive sessions)
    fn build_run_args_pty(&self, ...) -> Vec<String>;
    fn build_exec_args_pty(&self, ...) -> Vec<String>;
}
```

The runtime is resolved once at startup via `resolve_runtime(&GlobalConfig)`,
which reads the `runtime` config field and returns an `Arc<dyn AgentRuntime>`.
This `Arc` is threaded from `main.rs` through the command dispatcher into every
command handler and the TUI event loop.

### Runtime implementations

| Struct | File | Notes |
|--------|------|-------|
| `DockerRuntime` | `src/runtime/docker.rs` | Wraps the `docker` CLI; identical behavior to the old `src/docker/mod.rs` |
| `AppleContainersRuntime` | `src/runtime/apple.rs` | Wraps the `container` CLI; `#[cfg(target_os = "macos")]` |

### Shared utilities

The following free functions in `src/runtime/mod.rs` are not runtime-specific
and are used by all implementations:

- `generate_container_name()` — produces `amux-{hash}` names
- `project_image_tag()` — produces `amux-{project}:latest` (the project base image)
- `agent_image_tag()` — produces `amux-{project}-{agent}:latest` (the agent-specific image used for `chat` and `implement`)
- `parse_cpu_percent()` / `parse_memory_mb()` — stat output parsers (each
  runtime may use its own format variant)
- `format_build_cmd()` / `format_run_cmd()` — display-only command string builders

### `HostSettings`

`HostSettings` (the sanitized Claude config mount — `.claude.json` and
`settings.json`) lives in `src/runtime/mod.rs`. It is not Docker-specific; all
runtime implementations that support bind mounts use it for credential
injection.

---

## Working Directory Contract

All `run_with_sink` functions accept an explicit `cwd: &Path` parameter that
determines where the Git root is searched from. This ensures correctness for
both execution modes:

| Mode | `cwd` value | Behaviour |
|------|-------------|-----------|
| CLI (command mode) | `std::env::current_dir()` | Uses the directory where `amux` was launched |
| TUI (interactive mode) | `app.active_tab().cwd` | Uses the tab's working directory |

**Rule:** No command implementation may call `find_git_root()` (which reads the
process CWD). All callers must use `find_git_root_from(cwd)` with an explicitly
provided `cwd`. This prevents TUI tabs from accidentally operating on the wrong
repository when a tab's working directory differs from the process's launch
directory.

The `find_git_root()` helper (which reads `std::env::current_dir()`) exists only
for the CLI `run()` entry points, which call it once to determine the `cwd` to
pass down.

---

## TUI State Machine

The TUI state is split across three orthogonal enums plus the `App` struct:

### `Focus`

```
CommandBox  ←──── Esc ────── ExecutionWindow
    │                                ▲
    └─────── ↑ arrow / running ──────┘
```

### `ExecutionPhase`

```
Idle ──[Submit]──► Running ──[exit 0]──► Done
                      │
                      └──[exit ≠ 0]──► Error
```

`Done` and `Error` are both read-only scroll states. Any non-scroll key press
in the window, or any new Submit, transitions back through `Idle → Running`.

Mouse scrolling is enabled via `crossterm::EnableMouseCapture` and works in all
phases and focus states. Scroll events adjust `App::scroll_offset` by 3 lines
per tick, allowing the user to navigate output even while a process is running
and capturing keyboard input.

### `Dialog`

```
None ──[q / Ctrl+C]──────────────────────────► QuitConfirm      ──[y]──► quit
     ──[ready|implement|chat, cwd ≠ root]──► MountScope        ──[r/c]──► resume
     ──[new]───────────────────────────────► NewKindSelect      ──[1/2/3]──► NewTitleInput ──[Enter]──► create
     ──[init, --aspec + aspec/ exists]─────► InitReplaceAspec   ──[y/n]─┐
     ──[init, all other cases]────────────────────────────────────────►  InitAuditConfirm ──[y/n]──► InitWorkItemsSetup ──[y/n]──► launch_init()
```

Dialogs intercept all key events until dismissed. For the `init` flow, dialogs
collect answers into a `TuiInitAnswers` struct; `launch_init()` reads those answers
via `TuiInitQa` and delegates to `init_flow::execute()`. A `PendingCommand` enum
(`Ready { refresh, non_interactive }`,
`Implement { agent, work_item, non_interactive, plan, allow_docker, workflow, worktree, mount_ssh, yolo, auto }`,
or `Chat { agent, non_interactive, plan, allow_docker, mount_ssh, yolo, auto }`)
and the mount path are preserved in `App` fields while a dialog is active, so
the correct command resumes after the dialog is dismissed. All flag fields —
including `agent: Option<String>` — are populated from the parsed TUI command
line before any dialog is shown, so the flag values survive through the dialog
flow and are applied when the command launches.

---

## CLI/TUI Flag Unification

Before work item 0053, every command's flags were defined in three separate
places with no structural guarantee they stayed in sync:

| Location | Role |
|---|---|
| `src/cli.rs` — clap struct | CLI argument parsing |
| `src/tui/mod.rs` — `parse_chat_flags()` / `parse_implement_flags()` | TUI command-line parsing |
| `src/tui/input.rs` — hint lists in `flag_suggestions_for()` | TUI autocomplete |

This meant a flag added to `cli.rs` could be silently ignored in the TUI: the
parser would not extract it, the `PendingCommand` had no field for it, and
autocomplete would not hint it.

### `CommandSpec` — single source of truth (`src/commands/spec.rs`)

`spec.rs` is the leaf module that all three sites import from. It defines every
flag for every subcommand as static data:

```rust
pub struct FlagSpec {
    pub name: &'static str,       // long flag name without "--"
    pub takes_value: bool,        // true for --agent NAME, false for --plan
    pub value_name: &'static str, // metavar shown in hints ("NAME", "FILE", "")
    pub hint: &'static str,       // short description for autocomplete display
}

pub struct CommandSpec {
    pub name: &'static str,
    pub flags: &'static [FlagSpec],
}

pub static ALL_COMMANDS: &[CommandSpec] = &[
    CommandSpec { name: "chat",      flags: CHAT_FLAGS      },
    CommandSpec { name: "implement", flags: IMPLEMENT_FLAGS },
    // … all subcommands
];
```

`spec.rs` has zero imports from `cli.rs`, `tui/mod.rs`, or `tui/input.rs`. It is
re-exported from `src/commands/mod.rs`.

### Generic TUI flag parser (`src/tui/flag_parser.rs`)

`parse_flags(parts, spec)` replaces all ad-hoc `parse_*_flags()` functions. It
accepts a tokenized command line and a `&CommandSpec`, and returns a
`HashMap<&'static str, String>` of flag name → value (empty string for boolean
flags):

- `--flag value` and `--flag=value` forms are both handled
- Unknown flags are silently ignored (the user may be mid-typing)
- A value token that starts with `--` is never consumed as the value of a
  preceding flag (e.g. `--workflow --non-interactive` does not treat
  `--non-interactive` as the workflow path)
- `--flag=` (empty value) is parsed as `Some("")`

The deleted functions `parse_chat_flags()`, `parse_implement_flags()`, and
`parse_agent_flag()` are all replaced by calls to `parse_flags()` with the
corresponding `CommandSpec`.

### Autocomplete driven by `CommandSpec` (`src/tui/input.rs`)

`flag_suggestions_for(cmd)` is now a thin wrapper over `ALL_COMMANDS`:

```rust
pub fn flag_suggestions_for(cmd: &str) -> Vec<String> {
    let Some(spec) = ALL_COMMANDS.iter().find(|c| c.name == cmd) else {
        return vec![];
    };
    spec.flags.iter().map(|f| {
        if f.takes_value {
            format!("--{} <{}>  — {}", f.name, f.value_name, f.hint)
        } else {
            format!("--{}  — {}", f.name, f.hint)
        }
    }).collect()
}
```

The handwritten hint strings for positional argument examples (e.g.
`"implement <NNNN>  e.g. implement 0001"`) are prepended separately.
Because the flag hints are now derived, there is no separate hint list to drift.

### Enforcement tests

#### CLI/spec parity (compile-time guarantee)

A `#[test]` in `src/cli.rs` enumerates all clap `Arg` long names for each
subcommand and asserts they match the corresponding `spec::*_FLAGS` table
bidirectionally. The test fails immediately when a flag is added to `cli.rs`
but not `spec.rs`, or vice versa. A single test failure surfaces both problems:
the missing spec entry and the missing CLI arg.

#### TUI parser coverage

A unit test for each `CommandSpec` in `ALL_COMMANDS` calls `parse_flags()` with
every flag in both `--flag value` and `--flag=value` forms and asserts the
correct value is extracted. A separate test verifies that a value-taking flag
followed by another `--flag` does not consume the second flag as the value.

#### Autocomplete structural guarantee

Because `flag_suggestions_for()` reads directly from `ALL_COMMANDS`, there is no
separate hint list that could drift out of sync. The derivation itself is the
guarantee — no additional test is needed for autocomplete completeness.

### Agent override resolution order

The agent used for a session is resolved in this order, matching both CLI and TUI:

1. **Flag** — `--agent <name>` passed on the command line (CLI or TUI)
2. **Repo config** — `agent` field in `aspec/.amux.json`
3. **Global config** — `default_agent` field in `~/.amux/config.json`
4. **Built-in default** — `claude`

---

## Ready Command

The `ready` command has two modes based on the `--refresh` flag:

### Without `--refresh` (default)

1. Check configured runtime is available (`runtime.is_available()`) — report name and status
2. Check `Dockerfile.dev` exists (init from template if missing)
3. Check project image exists (build if missing, with streaming output)
4. Print skip message and tip about `--refresh`
5. Display summary table

### With `--refresh`

1–3: Same as above
4. Launch agent to audit `Dockerfile.dev` (interactive or non-interactive)
5. Rebuild image with updated `Dockerfile.dev` (streaming output)
6. Display summary table

### `ReadyOptions`

```rust
pub struct ReadyOptions {
    pub refresh: bool,          // run the Dockerfile audit
    pub build: bool,            // force rebuild the dev image
    pub no_cache: bool,         // pass --no-cache to docker build
    pub non_interactive: bool,  // launch agent in print mode
    pub allow_docker: bool,     // mount Docker socket into audit container
    pub auto_create_dockerfile: bool, // create Dockerfile.dev if missing (TUI: skip prompting)
    pub legacy_mode: bool,      // use project image only; skip agent image steps
}
```

Shared between command mode and TUI mode. All fields default to `false`.

The `build` flag is set to `true` programmatically after a successful legacy
layout migration, overriding the value computed by `compute_ready_build_flag()`.
This ensures the project image is rebuilt from the new minimal `Dockerfile.dev`
before the audit runs.

### `ReadySummary`

```rust
pub struct ReadySummary {
    pub docker_daemon: StepStatus,
    pub dockerfile: StepStatus,
    pub aspec_folder: StepStatus,
    pub work_items_config: StepStatus,
    pub local_agent: StepStatus,
    pub dev_image: StepStatus,
    pub refresh: StepStatus,
    pub image_rebuild: StepStatus,
}
```

Each step status is one of `Pending`, `Ok(msg)`, `Skipped(msg)`, `Failed(msg)`,
or `Warn(msg)`. The summary table is rendered via `print_summary()` at the end
of every ready run.

The `ReadySummary` produced by `run_pre_audit()` is passed to `run_post_audit()`
so that post-audit can include the pre-audit results (docker_daemon, dockerfile,
dev_image) in the final printed table. In TUI mode, the summary is stored in
`Tab.ready_summary` between phases rather than being reconstructed from defaults.

### Interactive Notice

Before launching any interactive agent (in `ready --refresh` or `implement`),
`print_interactive_notice()` displays a large ASCII-art banner alerting the user
that:
- The agent is in interactive mode
- They need to quit the agent when done

This notice is suppressed when `--non-interactive` is used.

### Ready Engine Functions

All business logic for the `ready` command lives in `src/commands/ready.rs` (the
engine). `src/tui/mod.rs` is the orchestrator: it sequences phases, manages I/O
routing, and holds state — but contains no inline Docker or filesystem operations
related to `ready`. Every such operation goes through a function in `ready.rs`.

Both CLI (`run()` in `ready.rs`) and TUI (`execute_command`, `launch_ready*` in
`mod.rs`) call the same engine functions. The only differences between CLI and TUI
are:

- **User Q&A mechanism**: stdin prompts (CLI) vs. dialogs/actions (TUI)
- **Audit container execution**: inherited stdio (CLI) vs. PTY session (TUI)

All other logic — detection, migration, flag computation, build sequencing,
socket checks, entrypoint selection, image selection, host-settings application,
summary accumulation — uses the shared engine functions.

| Engine function | Description |
|---|---|
| `compute_ready_build_flag(refresh, build)` | Returns `build` unless `refresh` is set (refresh always rebuilds post-audit, so forcing a pre-audit build is redundant). Migration overrides this value afterward. |
| `is_legacy_layout(git_root, agent_name)` | Returns `true` when `Dockerfile.dev` exists, the agent is a known amux agent, and `.amux/Dockerfile.{agent}` does not yet exist. |
| `perform_legacy_migration(git_root)` | Backs up `Dockerfile.dev` to `Dockerfile.dev.bak` and overwrites it with the minimal project base template. Returns display messages. |
| `gather_ready_env_vars(git_root, agent_name)` | Calls `resolve_auth()` (handles keychain, env-var, and file-based auth) then appends `effective_env_passthrough` vars not already present. |
| `create_ready_host_settings(agent_name)` | Thin wrapper: calls `passthrough_for_agent(agent_name).prepare_host_settings()`. |
| `apply_ready_user_directive(host_settings, ctx)` | Applies the USER directive from the agent dockerfile to host settings so files are mounted at the correct home directory inside the container. Called after `run_pre_audit()` returns, before the audit container launches. |
| `check_allow_docker(out, allow_docker, runtime)` | Verifies the host Docker socket is accessible when `--allow-docker` is set. Returns `Ok(())` when not needed or when socket is found (with a warning); returns `Err` when socket is missing. |
| `build_audit_setup(ctx, non_interactive)` | Returns an `AuditSetup` with the image tag (agent image when available, project image in legacy mode) and the correct entrypoint. |
| `run_pre_audit(…)` | Phase 1: daemon check, Dockerfile init, aspec check, local agent check, image build. Returns `ReadyContext`. |
| `run_post_audit(…)` | Phase 3: rebuilds both images after the audit agent updates `Dockerfile.dev`. |

### `AuditSetup`

```rust
pub struct AuditSetup {
    pub image_tag: String,
    pub entrypoint: Vec<String>,
}
```

Produced by `build_audit_setup()`. Carries the image and entrypoint for the audit
container: uses the agent image (`amux-{project}-{agent}:latest`) when available,
or the project base image in legacy mode. The entrypoint uses the interactive form
unless `non_interactive` is `true`.

---

## Implement Command

The `implement` command accepts a 4-digit work item number (e.g. `0001`) and
launches the configured agent to implement it. The agent receives a structured
prompt that instructs it to implement the work item, iterate on builds and tests,
write documentation, and ensure final success.

### Interactive Mode (default)

Uses `agent_entrypoint()` which launches the agent in interactive mode. An
ASCII-art interactive notice is shown before launch.

### Non-Interactive Mode (`--non-interactive`)

Uses `agent_entrypoint_non_interactive()` which adds print-mode flags:
- Claude: `-p` flag
- Codex: `--quiet` flag
- Opencode: same `run` subcommand

Output is captured via `docker::run_container_captured()` and displayed.
A tip suggests removing `--non-interactive` for direct interaction.

### Plan Mode (`--plan`)

When `--plan` is passed, the agent is initialized in read-only plan mode.
Plan flags are appended after the regular entrypoint arguments via
`append_plan_flags()`:
- Claude: `--plan`
- Codex: `--approval-mode plan`
- Opencode: no plan mode (flag is silently ignored)

`--plan` can be combined with `--non-interactive`.

Host agent settings are mounted read-only into the container via
`docker::HostSettings::prepare()`, which copies sanitized versions of
`~/.claude.json` (with `oauthAccount` stripped) and `~/.claude/settings.json`
into a temporary directory. These are mounted at `/root/.claude.json:ro` and
`/root/.claude:ro`. The temp directory is cleaned up automatically when the
`HostSettings` struct is dropped (after the container exits).

When the host has no `~/.claude.json` (first-time users, CI machines),
`HostSettings::prepare()` returns `None`. In this case, callers fall back to
`HostSettings::prepare_minimal()`, which creates a settings-only mount with
LSP suppression but no auth forwarding. This guarantees that LSP recommendation
dialogs are always suppressed regardless of whether the host has a Claude config.

Authentication is handled entirely via the `CLAUDE_CODE_OAUTH_TOKEN` environment
variable — the host settings mount provides agent configuration (onboarding
state, model preferences, plugins) without interfering with auth.

---

## Chat Command

The `chat` command starts a freeform agent session with no pre-configured prompt.
It shares the same underlying container-launching logic as `implement` via the
`commands/agent.rs` module.

### Shared Agent Launching (`commands/agent.rs`)

The `run_agent_with_sink()` function is the shared code path for both `implement`
and `chat`. It handles:

- Git root detection and config loading
- Mount path resolution
- Docker image tag derivation
- Docker command display (with masked secrets)
- Interactive notice display
- Container launching (interactive or captured)

The only differences between `chat` and `implement` are:
- **Entrypoint**: `chat` passes just the agent command (e.g. `["claude"]`);
  `implement` passes the agent command + a structured prompt
- **Status message**: `chat` shows "Starting chat session"; `implement` shows
  the work item being implemented

### Chat Entrypoints

| Agent | Interactive | Non-Interactive | Plan (appended) |
|-------|-----------|-----------------|-----------------|
| `claude` | `["claude"]` | `["claude", "-p"]` | `["--plan"]` |
| `codex` | `["codex"]` | `["codex", "--quiet"]` | `["--approval-mode", "plan"]` |
| `opencode` | `["opencode"]` | `["opencode"]` | (none) |

---

## New Command

The `new` command creates a new work item from the `0000-template.md` template.

1. Locates the template at `GITROOT/aspec/work-items/0000-template.md`
2. Scans existing work item files to determine the next sequential number
3. Collects the work item kind (Feature/Bug/Task) and title
4. Generates a slug from the title (lowercase, spaces→hyphens, strip non-alphanumeric)
5. Writes the new file with template substitutions applied
6. Opens the file in VS Code if running in the VS Code terminal

In **command mode**, kind and title are collected via stdin prompts.
In **TUI mode**, two dialog overlays (`NewKindSelect` → `NewTitleInput`) collect
the information, then `run_with_sink` is called with the pre-supplied values.

---

## Init Command

The `init` command sets up a new project for use with amux. All business logic
lives in `src/commands/init_flow.rs`, which is called identically from both the
CLI and TUI adapters. The two surfaces differ only in how they collect user input
(`InitQa` trait) and how they launch containers (`InitContainerLauncher` trait).
It is structurally impossible for the two surfaces to diverge in stage coverage
or file output.

### Unified Engine (`src/commands/init_flow.rs`)

`InitFlow::execute()` is the single entry point for everything `init` does:

```rust
pub async fn execute<Q: InitQa, L: InitContainerLauncher>(
    params: InitParams,
    qa: &mut Q,
    launcher: &L,
    sink: &OutputSink,
    runtime: &dyn AgentRuntime,
) -> anyhow::Result<InitSummary>
```

Stages run in order, each updating `InitSummary`. If an early stage fails, later
stages set their status to `Skipped` rather than running against broken
preconditions.

| # | Stage | Description |
|---|-------|-------------|
| 1 | Collect Q&A | Calls `qa.ask_replace_aspec()` and `qa.ask_run_audit()` |
| 2 | Repo config | Reads or creates `aspec/.amux.json` with the chosen agent |
| 3 | aspec folder | Downloads or skips `aspec/` based on `params.aspec` flag |
| 4 | Dockerfile.dev | Writes project base template if absent |
| 5 | Agent dockerfile | Writes `.amux/Dockerfile.{agent}` template if absent |
| 6 | Runtime check | Verifies container runtime is available; exits early on error |
| 7a | With audit | Build project image → build agent image → run audit → rebuild both |
| 7b | Without audit (new files only) | Build project image → build agent image |
| 8 | Work items setup | Calls `qa.ask_work_items_setup()`; writes result to repo config |
| 9 | Summary | Prints `InitSummary` table and "What's Next?" guide |

Stage 7a rebuilds both images after the audit because the audit agent may rewrite
`Dockerfile.dev`. This rebuild is non-optional and is always performed by the
launcher, not gated by a flag.

### `InitQa` Trait

Handles all user question-and-answer interactions during the flow:

```rust
pub trait InitQa {
    fn ask_replace_aspec(&mut self) -> anyhow::Result<bool>;
    fn ask_run_audit(&mut self) -> anyhow::Result<bool>;
    fn ask_work_items_setup(&mut self) -> anyhow::Result<Option<WorkItemsConfig>>;
}
```

| Implementation | Backing mechanism |
|---|---|
| `CliInitQa` | `ask_yes_no_stdin()` and `read_line()` — blocks on stdin |
| `TuiInitQa` | Holds a pre-collected `TuiInitAnswers` struct; returns answers immediately without blocking |

`TuiInitQa` can accurately represent "the user was never asked this question"
(e.g. `ask_replace_aspec` is skipped when `--aspec` was not passed or `aspec/`
does not exist) — this is encoded as `replace_aspec = false`, never as an error.

### `InitContainerLauncher` Trait

Decouples the flow from any specific blocking vs. async container strategy:

```rust
pub trait InitContainerLauncher {
    fn build_image(&self, tag: &str, dockerfile: &Path, context: &Path, sink: &OutputSink) -> anyhow::Result<()>;
    fn run_audit(&self, agent: Agent, cwd: &Path, sink: &OutputSink) -> anyhow::Result<()>;
}
```

| Implementation | Behavior |
|---|---|
| `CliContainerLauncher` | Delegates to `AgentRuntime`; blocks synchronously (inherited stdio) |
| `TuiContainerLauncher` | Runs inside the background task spawned by `launch_init()`; blocking there is safe since the task has its own thread |

Both implementations delegate to `AgentRuntime` rather than calling Docker
directly — `InitContainerLauncher` is an orchestration boundary, not a
runtime abstraction.

### CLI Adapter (`src/commands/init.rs`)

A thin shim with no business logic:

```rust
pub async fn run(agent: Agent, aspec: bool, cwd: PathBuf, runtime: &dyn AgentRuntime) -> anyhow::Result<()> {
    let git_root = find_git_root_from(&cwd)?;
    let mut qa = CliInitQa::new(&git_root);
    let launcher = CliContainerLauncher::new(runtime);
    let sink = OutputSink::Stdout;
    let params = InitParams { agent, aspec, git_root };
    init_flow::execute(params, &mut qa, &launcher, &sink, runtime).await?;
    Ok(())
}
```

All Q&A (including `ask_replace_aspec` and `ask_run_audit`) happens inside
`execute()` at the correct stage — there is no upfront pre-flight Q&A outside the
flow.

### TUI Adapter (`src/tui/mod.rs`)

The TUI collects answers through three dialog states before calling `launch_init()`:

| Dialog | Purpose | Condition |
|--------|---------|-----------|
| `InitReplaceAspec` | Ask whether to overwrite existing `aspec/` | Only when `--aspec` was passed and `aspec/` already exists |
| `InitAuditConfirm` | Ask whether to run the Dockerfile audit | Always |
| `InitWorkItemsSetup` | Ask for work items directory / template paths | When `aspec/` will not be downloaded and no work items dir is configured |

All three dialogs populate a `TuiInitAnswers` struct. When the final dialog is
dismissed, `launch_init()` constructs `TuiInitQa { answers }` and
`TuiContainerLauncher` and calls `init_flow::execute()` inside a background task
— identical in shape to how `launch_ready()` drives `ready.rs`.

The `pending_init_run_audit` flag and `check_init_continuation()` that
previously deferred the audit to a separate `ready --refresh` invocation no longer
exist. The audit is now run inline inside `execute()` via `TuiContainerLauncher`.

### `InitSummary`

```rust
pub struct InitSummary {
    pub config:           StepStatus,
    pub aspec_folder:     StepStatus,
    pub dockerfile_dev:   StepStatus,
    pub agent_dockerfile: StepStatus,
    pub agent_audit:      StepStatus,
    pub base_image:       StepStatus,
    pub agent_image:      StepStatus,
    pub work_items:       StepStatus,
}
```

Each `StepStatus` is one of `Pending`, `Ok(msg)`, `Skipped(msg)`, `Failed(msg)`,
or `Warn(msg)`. `print_init_summary()` renders the table shown at the end of
every `init` run. `InitSummary` lives in `init_flow.rs` — it is part of the
shared flow, not the CLI or TUI presentation layer.

---

## Docker Build Streaming

`docker::build_image_streaming()` spawns `docker build` and reads stdout and
stderr concurrently in separate background threads. Both threads send lines
through a shared `std::sync::mpsc` channel, and the calling thread receives
lines from the channel and forwards them to the `on_line` callback as they
arrive. This ensures real-time streaming of Docker build output — including
stderr, where Docker emits most of its build progress — rather than buffering
stderr until after stdout finishes.

The `OutputSink`'s `Clone` implementation enables passing it into the streaming
callback closure.

---

## PTY Architecture

For `implement`, the container process must have a real terminal (PTY) so that
interactive agent CLIs (Claude, Codex, etc.) work correctly.

```
App::pty (PtySession)
    │
    ├── master (Box<dyn MasterPty>)       ← held for resize()
    └── input_tx (SyncSender<Vec<u8>>)    ← TUI keypresses → writer thread
                                                           → PTY master
                                                           → container stdin

PtyEvent channel (std::sync::mpsc)
    ├── reader thread → Data(Vec<u8>)     ← PTY master → strip ANSI → output_lines
    └── wait thread   → Exit(i32)         ← child.wait() → finish_command()
```

Key design decisions:
- `master` stays on the main thread (no `Send` required); only `resize()` is called on it
- The writer (`Box<dyn Write + Send>`) is moved to a dedicated `std::thread` and communicated
  with via a bounded `std::sync::mpsc::sync_channel`
- The child (`Box<dyn Child + Send>`) is moved to a wait thread; its exit code is sent
  back via `std::sync::mpsc`
- PTY output bytes are processed for `\r` (carriage return) and `\n` (newline) from
  the raw byte stream *before* ANSI stripping, because `strip_ansi_escapes::strip`
  removes `\r` characters. A bare `\r` clears the line buffer (overwrite from start),
  `\r\n` is treated as a newline, and content segments between control characters are
  ANSI-stripped before appending. A "live line" at the end of `output_lines` is updated
  in-place until finalized by `\n`, enabling correct display of terminal spinners and
  progress indicators. Full terminal emulation (cursor tracking, screen clearing) is
  a future enhancement.

For `init` and `ready` (no PTY needed), `spawn_text_command` runs a tokio task that
passes an `OutputSink::Channel` to `run_with_sink` and sends the exit code through
a `tokio::sync::oneshot` channel.

### Dockerfile Audit (ready --refresh)

The `ready --refresh` command runs a three-phase workflow:

1. **Pre-audit** (text command via `OutputSink`): checks Docker daemon, ensures
   `Dockerfile.dev` exists, checks aspec folder, checks local agent, builds the
   image (streaming). Returns a `ReadyContext` with the image tag, mount path,
   agent name, env vars, and agent image tag. Also returns a `ReadySummary` with
   the status of each pre-audit step.
2. **Audit** (interactive PTY or captured): launches the agent to scan the project
   and update `Dockerfile.dev`. In command mode with interactive: uses
   `runtime.run_container()` with inherited stdio. In command mode with
   `--non-interactive`: uses `runtime.run_container_captured()`. In TUI mode:
   uses a PTY session (interactive) or captured command (non-interactive).
3. **Post-audit** (text command): rebuilds both the project base image and the
   agent image with streaming output, then prints the final summary table.

Without `--refresh`, only phase 1 runs, followed by the summary table.

In TUI mode, `ReadyPhase` tracks which phase is active. When a phase completes,
`check_ready_continuation()` automatically launches the next phase.

**Summary continuity in TUI mode**: after phase 1 completes, `check_ready_continuation()`
stores both the `ReadyContext` and the `ReadySummary` in `Tab.ready_ctx` and
`Tab.ready_summary`. Phase 3 (`launch_ready_post_audit()`) retrieves this stored
summary and passes it directly to `run_post_audit()`, so the final table includes
the docker_daemon, dockerfile, and dev_image statuses from phase 1 — not
reconstructed defaults.

Image tags are project-specific (`amux-{projectname}:latest`) derived from the
Git root folder name via `runtime::project_image_tag()`.

**Migration and image rebuild**: when a legacy layout migration runs, `build`
is set to `true` after `perform_legacy_migration()` succeeds. `run_pre_audit()`
checks `opts.build` as part of its `needs_build` condition, so the project base
image is rebuilt from the new minimal `Dockerfile.dev` before the agent image is
built on top of it. Without this flag, the cached legacy image would be used and
the audit would run inside the old environment.

### Host Settings Injection

`docker::HostSettings` encapsulates the preparation and lifetime of the
sanitized Claude configuration that is bind-mounted into every agent container.

```
~/.claude.json          ──sanitize──► temp/claude.json       (oauthAccount removed,
~/.claude/              ──filter──►  temp/dot-claude/         /workspace trust added,
                                         settings.json        LSP suppression applied)
                                         (denylist applied)
```

**Sanitization steps performed by `HostSettings::prepare()`:**

1. Read `~/.claude.json`; strip `oauthAccount` (OAuth tokens live in the
   macOS keychain, not in this file, but the field references the account and
   can produce broken state inside the container).
2. Inject `/workspace` project trust so Claude Code does not show the
   "do you trust this project?" dialog inside the container.
3. Copy `~/.claude/` into a temp directory with a denylist filter that excludes
   large, host-specific, or irrelevant entries (`projects/`, `sessions/`,
   `history.jsonl`, `telemetry/`, etc.).
4. Call `disable_lsp_recommendations()` to write the correct suppression key
   into `settings.json`, preventing LSP installation dialogs inside the container
   (containers have no IDE and no pre-installed language servers).

**LSP recommendation suppression (`disable_lsp_recommendations`):**

Reads the existing `settings.json` (or starts from `{}`), merges the LSP
suppression key, and writes the result back. Existing settings keys are
preserved. If `settings.json` contains invalid JSON, the function falls back to
`{}` so that the container launch is never blocked.

**Fallback when host has no `~/.claude.json` (`HostSettings::prepare_minimal`):**

`prepare()` returns `None` when the host has no `~/.claude.json` (first-time
users, CI machines). Callers use `or_else(|| HostSettings::prepare_minimal())`
to ensure a minimal settings mount is always created. `prepare_minimal()` skips
auth and config forwarding but still applies LSP suppression, guaranteeing that
LSP dialogs are suppressed even on machines where Claude has never been used.

**Lifetime management:**

`HostSettings` holds a `tempfile::TempDir` (RAII). The temp directory — and all
bind-mounted files — is automatically deleted when `HostSettings` is dropped,
which occurs after the container exits. `prepare_to_dir` writes into a
caller-supplied stable directory instead so that bind-mount sources survive
process restarts (used by the TUI's persistent session path).

**Denylist (`CLAUDE_DIR_DENYLIST`):**

Top-level `~/.claude/` entries skipped during copy:
`projects`, `sessions`, `session-env`, `debug`, `file-history`,
`history.jsonl`, `telemetry`, `downloads`, `ide`, `shell-snapshots`,
`paste-cache`.

### Agent Credential Passing

Agent credentials are extracted from the macOS system keychain and passed
into the container via a single environment variable:

- **`CLAUDE_CODE_OAUTH_TOKEN`**: The OAuth credential JSON (containing
  `accessToken`, `refreshToken`, `expiresAt`), passed via `-e`. Claude Code
  reads this env var on startup for authentication.

No credential files are mounted. The environment variable is the only
credential passed to the container. Host agent settings (model preferences,
onboarding state) are mounted separately via `HostSettings` — see the
Implement Command section above.

The credential extraction flow:

1. `auth::read_keychain_raw()` calls macOS `security find-generic-password`
   to read the full JSON blob from the keychain (service: `Claude Code-credentials`)
2. `auth::extract_token_from_keychain_json()` parses the JSON and extracts
   the `claudeAiOauth` inner object as a JSON string
3. The JSON is returned and passed as the `CLAUDE_CODE_OAUTH_TOKEN` env var

`auth::resolve_auth()` always returns keychain credentials (auto-passthrough)
without prompting. No opt-in dialog is needed.

`docker::append_env_args()` translates `(key, value)` pairs into
`-e KEY=VALUE` Docker flags.

For display purposes (CLI output, TUI window), `build_run_args_display()`
masks env var values as `KEY=***` to prevent accidental secret exposure.

### Docker Command Visibility

Every `docker build` and `docker run` invocation is formatted as a CLI string
via `docker::format_build_cmd()` / `docker::format_run_cmd()` and printed
through the `OutputSink` before execution. In command mode this appears on
stdout; in TUI mode it appears in the execution window output.

---

## Container Window

When `implement`, `chat`, or `ready --refresh` launches an interactive agent, the TUI
displays a dedicated **container window** overlaying the outer execution window.

### State Machine

```
Hidden ──[start_container()]──► Maximized ──[Esc]──► Minimized ──['c']──► Maximized
                                     │                    │
                                     └────[finish]────────┘──► Hidden + Summary bar
```

`ContainerWindowState` is an enum with three variants: `Hidden`, `Maximized`,
and `Minimized`. The state transitions are:

- **Hidden → Maximized**: `start_container()` is called when an agent launches.
  It sets the container name, agent display name, start time, and initializes
  the stats channel receiver.
- **Maximized → Minimized**: User presses `Esc`. The outer window becomes
  visible and scrollable while the container continues running in the background.
  A 1-line green-bordered bar shows the agent name and live stats.
- **Minimized → Maximized**: User presses `c`. The container window re-overlays
  the outer window and keyboard input is forwarded to the container again.
- **Maximized/Minimized → Hidden**: `finish_command()` transitions the container
  window to `Hidden` and generates a `LastContainerSummary` with average CPU,
  peak memory, and total runtime.

### Layout

When **maximized**, the container window covers 95% of the outer execution
window's width and height, centered. It has a green border with:
- Left title: `🔒 {agent} (containerized)` (e.g. `🔒 Claude Code (containerized)`)
- Right title: `{container_name} | CPU {cpu}% | Mem {mem}MB | {runtime}`

When **minimized**, a 1-line bar with green border appears between the outer
execution window and the command box, showing agent name and live stats.

After the container **exits**, a summary bar with dashed border shows:
`{agent} exited | avg CPU {cpu}% | peak mem {mem}MB | runtime {duration}`

### Container Scrollback

When the container window is maximized, the mouse scroll wheel scrolls through
the vt100 terminal's scrollback buffer at 5 lines per tick. The view is
controlled via the vt100 crate's `set_scrollback()` API:

- **Scroll up**: increases `container_scroll_offset` (capped at the actual
  scrollback depth). `parser.set_scrollback(offset)` shifts the rendered view
  into the buffer; `render_vt100_screen_no_cursor()` displays that slice.
- **Scroll down**: decreases the offset; at 0 the live screen is shown and
  `render_vt100_screen()` (with cursor) is used instead.
- **Indicator**: a centered yellow title (`↑ scrollback (N / M lines)`) appears
  in the container border when scrolled — `N` is the current offset and `M` is
  the total scrollback depth available.

**Scrollback depth probe:**

The `vt100::Screen` does not expose a direct `scrollback_len()` accessor. The
actual depth is probed by calling `parser.set_scrollback(usize::MAX)` (which
internally clamps to the real length) and then reading `screen.scrollback()`:

```rust
parser.set_scrollback(usize::MAX);
let max = parser.screen().scrollback();
parser.set_scrollback(0);
```

This probe is performed in both the scroll handler (to cap the offset) and the
renderer (to compute the `M` value for the scrollback indicator). The parser is
reset to `0` (live view) before any rendering begins.

**Configurable scrollback capacity:**

The parser is created with `vt100::Parser::new(rows, cols, scrollback_lines)`,
where `scrollback_lines` comes from `tab.terminal_scrollback_lines`. This field
defaults to `DEFAULT_SCROLLBACK_LINES` (10,000) and is loaded from config before
each `start_container()` call via `config::effective_scrollback_lines()`.

Config precedence: per-repo (`GITROOT/.amux/config.json`) → global
(`$HOME/.amux/config.json`) → built-in default (10,000). A 10,000-line buffer at
80 columns uses approximately 3 MB per tab.

Scrollback state (`container_scroll_offset`) resets to 0 when a new container starts.

### Terminal Text Selection

When the container window is maximized, users can select terminal output with
the mouse and copy it to the clipboard with **Ctrl+Y**.

**Selection state (`TabState`):**

| Field | Type | Description |
|-------|------|-------------|
| `terminal_selection_start` | `Option<(u16, u16)>` | Anchor cell in vt100 (row, col) space; set on `MouseDown` |
| `terminal_selection_end` | `Option<(u16, u16)>` | End cell; extended on `MouseDrag`, finalized on `MouseUp` |
| `terminal_selection_snapshot` | `Option<Vec<Vec<String>>>` | Grid of cell strings captured at `MouseDown`; isolated from live output |
| `container_inner_area` | `Option<Rect>` | Inner content area recorded each render frame; used for mouse→vt100 coordinate conversion |

**Coordinate conversion:**

Mouse terminal coordinates are converted to vt100 cell positions using the
stored `container_inner_area`:

```
vt100_col = mouse.column - inner.x
vt100_row = mouse.row   - inner.y
```

Drag events clamp to `inner.width - 1` / `inner.height - 1` to stay within
bounds. Any click outside the `container_inner_area` rectangle is ignored.

**Output snapshot isolation:**

When `MouseDown` fires, `capture_vt100_snapshot()` captures the current
`vt100::Screen` cell contents into `terminal_selection_snapshot`. Subsequent
drag and copy operations read from this snapshot instead of the live parser,
preventing live output from shifting cell coordinates under the selection.

**Text extraction:**

`extract_selection_text()` normalises the selection so start ≤ end in row-major
order, iterates the snapshot rows and columns within the range, strips trailing
spaces from each row, and joins rows with `\n`. ANSI attributes are not present
in the snapshot — cell contents are already plain text.

**Clipboard abstraction:**

Clipboard writes go through the `ClipboardWriter` trait (defined in
`tui/mod.rs`), which has a single method `set_text(&str) -> Result<(), String>`.
The production implementation wraps `arboard::Clipboard`. A `MockClipboard` is
provided in tests. The public `copy_selection_to_clipboard(tab, clipboard)`
function drives extraction and write; it returns `true` if non-empty text was
written successfully.

In headless environments (no X11/Wayland display server), `arboard::Clipboard::new()`
returns an error; `amux` logs a warning and degrades gracefully — the copy
keybinding does nothing rather than panicking.

**Selection lifecycle:**

| Event | Effect |
|-------|--------|
| `MouseDown` inside inner area | Sets `terminal_selection_start`, `terminal_selection_end`, captures snapshot |
| `MouseDrag` (left button) | Updates `terminal_selection_end` (clamped) |
| `MouseUp` | No-op; selection already set |
| Ctrl+Y (selection active) | Calls `copy_selection_to_clipboard`; clears selection |
| Ctrl+Y (no selection) | Forwarded to PTY (byte 0x19) |
| Esc | Minimizes window; clears selection via `clear_terminal_selection()` |
| Terminal resize | `clear_terminal_selection()` on all tabs (vt100 re-wraps on resize) |
| `start_container()` | Clears selection |

**Rendering:**

`render_vt100_screen()` and `render_vt100_screen_no_cursor()` accept a
`selection: Option<((u16, u16), (u16, u16))>`. Selected cells have
`Modifier::REVERSED` applied on top of their normal style, matching standard
terminal selection appearance. The selection is normalised inside each render
function before the `cell_in_selection()` helper is called per cell.

### PTY Output Routing

PTY output bytes are routed to different line buffers depending on the container
window state:

- **Container window active** (`Maximized` or `Minimized`): PTY data goes to
  `container_output_lines`, displayed inside the container window.
- **Container window hidden**: PTY data goes to `output_lines`, displayed in
  the outer execution window (original behavior).

The routing decision is made in `process_pty_data()` using `pty_uses_container()`,
which returns `true` when `container_window` is not `Hidden`. This avoids a
mutable borrow conflict by returning a boolean flag instead of a mutable
reference to the target buffer.

### Docker Stats Polling

When a container starts, `spawn_stats_poller()` creates a tokio task that polls
Docker stats every 5 seconds:

```
tokio::spawn ──► loop {
    interval.tick().await           (5s)
    spawn_blocking(query_container_stats)
    tx.send(stats)
}
```

`query_container_stats()` runs `docker stats --no-stream --format` and parses
the JSON output into a `ContainerStats` struct (name, cpu_percent, memory).
The stats are sent via `tokio::sync::mpsc::unbounded_channel` and drained in
`App::tick()` each render cycle.

Each polled stats snapshot is appended to `ContainerInfo::stats_history` for
computing averages and peaks when the container exits.

### Container Naming

`generate_container_name()` produces a deterministic name (`amux-{pid}-{nanos}`)
passed to `docker run --name`. This allows `query_container_stats()` to query
stats for the specific container by name.

---

## Agent Auth Flow

```
ready/implement/chat submitted
        │
        ▼
   read_keychain_raw() → extract OAuth JSON → CLAUDE_CODE_OAUTH_TOKEN env var
```

If the host agent is installed and authenticated, credentials are sourced from
the macOS system keychain and passed automatically into the container — no
prompting required. If credentials are unavailable, the container launches
without them.

---

## Performance Characteristics

This section documents the performance design of amux, based on the audit conducted in work item 0033. It covers the render loop, memory model, async task architecture, and Docker interaction overhead.

---

### Render Loop

The TUI event loop runs in `src/tui/mod.rs` and drives all rendering:

```
loop {
    terminal.draw(|f| render::draw(f, &mut app))?;  // redraws every iteration
    if event::poll(Duration::from_millis(16))? {    // ≤16ms wait
        // handle key/mouse event
    }
    tick_all(&mut app);   // drains channels, updates state
}
```

**Always-redraw (current behaviour):** `terminal.draw()` is called unconditionally on every loop iteration (~60 Hz), regardless of whether any state changed. When the user is idle and no container is running, the full widget tree is rebuilt and diffed every ~16 ms. A dirty-flag optimisation is planned (work item 0034) that will skip `terminal.draw()` when no state has changed.

**Ratatui double-buffering:** `Terminal::draw()` compares the new widget cell buffer against the previous frame and emits only changed cells as terminal escape codes. This means terminal I/O is proportional to changed cells, not screen size, so the idle-CPU cost is widget construction rather than terminal output.

**Tick rate:** the `event::poll(16ms)` call caps the maximum frame rate at ~60 Hz.

---

### Output Buffer

Each `TabState` holds an `output_lines: Vec<String>` for non-container (text command) output. This buffer is currently **unbounded** — lines accumulate for the lifetime of the tab. A bounded ring-buffer replacement using `VecDeque<String>` with a configurable cap (default 10,000 lines) is planned in work item 0035.

**Memory estimates at current behaviour:**
- Typical terminal line: ~80 bytes average after ANSI stripping
- After 1 hour of moderate output: ~4–8 MB per tab
- After 3+ hours of high-throughput output: can grow to tens of MB per tab

**Cleanup on tab close:** `TabState` is dropped when a tab is closed, freeing `output_lines` immediately via Rust's ownership model. There is no cross-tab leak — the risk is growth during the tab's own lifetime.

The `vt100::Parser` used for container window rendering is initialised with a **1,000-line scrollback cap** (matching common terminal emulators), which is a hard memory bound on the full-terminal emulation path.

The scroll computation in `draw_exec_window` iterates all retained lines each frame to compute the total visual row count for scroll offset rendering (O(n) where n = lines in buffer). With a bounded buffer this becomes O(max_lines); until work item 0035 lands, n is unbounded.

---

### Async Task Architecture

amux uses a mixed async/thread model:

| Task/Thread | Spawn mechanism | Exit condition |
|---|---|---|
| Stats poller | `tokio::spawn` + `spawn_blocking` for Docker call | `stats_rx` receiver dropped on `finish_command` |
| Text command (init, ready, non-interactive implement) | `tokio::spawn` via `spawn_text_command` | Function returns |
| PTY reader | `std::thread::spawn` | EOF on PTY master (process exit or master close) |
| PTY wait | `std::thread::spawn` | Child process exits |
| PTY writer | `std::thread::spawn` | `input_rx` channel closed when `PtySession` is dropped |
| Docker build stdout/stderr | `std::thread::spawn` | EOF on subprocess stdout/stderr |
| Status watch | `tokio::spawn` via `spawn_text_command` | `status_watch_cancel_tx` fires cancel |

**Tab close cleanup:** dropping a `TabState` closes the PTY master (`Box<dyn MasterPty>`), which sends SIGHUP to the foreground process group of the PTY on Linux and macOS. This causes the `docker run` child process to exit, which in turn causes the PTY reader thread and wait thread to exit. Dropping `PtySession` closes the writer channel, causing the writer thread to exit. Cleanup is RAII-driven; no explicit join or cancel call is needed for PTY sessions.

**Blocking calls and Tokio:** `run_container_captured` and `run_container` are synchronous functions that block until the Docker subprocess exits. They are called inside `tokio::spawn` tasks via `spawn_text_command` without `spawn_blocking`, which occupies a Tokio worker thread for the container's full runtime. During a long agent run (minutes), this can starve other tasks scheduled on that worker thread. The stats poller correctly uses `spawn_blocking` for its `docker stats` call and serves as the model for the fix planned in work item 0036.

**Channel sizing:**

| Channel | Type | Capacity |
|---|---|---|
| PTY event (`PtyEvent`) | `std::sync::mpsc::sync_channel` | 256 |
| PTY input | `std::sync::mpsc::sync_channel` | 64 |
| Text output (`output_tx`/`output_rx`) | `tokio::sync::mpsc::unbounded_channel` | Unbounded (bounded+lossy replacement planned in work item 0038) |
| Stats | `tokio::sync::mpsc::unbounded_channel` | Unbounded (≤1 message queued at 5s poll rate; effectively bounded) |

---

### Docker Interaction Overhead

All Docker operations spawn a new `std::process::Command` child process. There is no persistent Docker HTTP client. Typical per-operation costs:

| Operation | Approximate latency |
|---|---|
| `docker info` (daemon check) | 50–200 ms |
| `docker stats --no-stream` (stats poll) | 200–500 ms |
| `docker build` | seconds–minutes (cache-dependent) |
| `docker run` startup | dominated by container init, not subprocess spawn (~5 ms) |

Stats are polled every **5 seconds** per active container, amortising the ~300 ms Docker call cost adequately. Each container session has its own stats poller task; in normal usage containers have unique generated names so there is no deduplication overhead.

Container cleanup uses `--rm` on all `docker run` invocations, causing Docker to remove the container immediately on exit. No manual cleanup is required.

---

### Scalability Target

**20 concurrent tabs** (containers) is the validated scalability target. Key O(n) paths and their cost at 20 tabs:

| Path | Complexity | Cost at 20 tabs |
|---|---|---|
| `tick_all()` | O(tabs) | ~20 µs (negligible) |
| `draw_tab_bar()` | O(tabs) | Negligible |
| `draw_exec_window()` | O(output_lines of active tab only) | Unaffected by tab count |
| `tui_tabs_shared` lock | O(tabs) | Brief write lock per tick; no contention |

Inactive tabs are rendered only as a tab bar entry — the full render path runs only for the active tab.

---

## Headless Mode

The headless server is a third execution mode alongside command mode and the TUI. It is implemented entirely within `src/commands/headless/` and shares no state with the TUI event loop.

### Request flow

```
HTTP client
     │
     ▼
axum router (server.rs)
     │
     ├── POST /v1/sessions ──► db::create_session() ──► SQLite
     │
     └── POST /v1/commands ──► validate session (DB)
                                     │
                                     ├── acquire per-session mutex (in-memory)
                                     │
                                     └── tokio::spawn ──► commands::run() dispatch
                                                               │
                                                               ▼
                                                         Docker container
                                                         stdout/stderr → log files
                                                         status → db::update_command()
```

### `AppState`

The axum router is backed by a shared `AppState` (wrapped in `Arc`) that holds:

- The resolved working-directory allowlist (`Vec<PathBuf>`)
- A `tokio::sync::Mutex<rusqlite::Connection>` — single writer, avoids `SQLITE_BUSY` under concurrent requests
- A `HashMap<Uuid, Arc<Mutex<()>>>` — one mutex per active session, enforces the one-command-at-a-time invariant without blocking unrelated sessions

### Database access

`db.rs` is a leaf module with no knowledge of axum or the HTTP layer. All data-access functions accept a `MutexGuard<Connection>`. The schema is created on first run inside `db::init_schema()` and never migrated — future schema changes require a new version.

The `AMUX_HEADLESS_ROOT` environment variable overrides the storage root (`~/.amux/headless/`). Set it in tests to redirect all DB and log-file writes to a `tempfile::TempDir`, keeping tests hermetic.

### Subcommand execution

When `POST /v1/commands` is accepted:

1. A new `commands` row is written with `status = pending`.
2. A Tokio task is spawned. The task:
   a. Updates the row to `status = running` and records `started_at`.
   b. Creates `~/.amux/headless/sessions/<session-id>/commands/<command-id>/` and opens `stdout.log` and `stderr.log` for incremental async writes (`tokio::fs`).
   c. Dispatches to the existing `commands::run()` path with an `OutputSink` wired to the log files, using the session's `workdir` as the execution CWD.
   d. On completion, updates the row with `status`, `exit_code`, and `finished_at`.
3. The HTTP response (`{ "command_id": "..." }`) is returned before step 2 completes — execution is fire-and-forget from the client's perspective.

All file I/O within the server uses `tokio::fs` (async) to avoid blocking the Tokio executor. Synchronous `std::fs` is restricted to startup and shutdown paths.

### Background mode

`process.rs` encapsulates all OS-specific daemonization:

| Platform | Strategy |
|----------|----------|
| Linux (systemd) | `systemd-run --user --unit=amux-headless.service -- amux headless start …` (without `--background`) |
| macOS (launchd) | Writes `~/Library/LaunchAgents/io.amux.headless.plist`; calls `launchctl load` |
| Fallback | Double-fork via `nix::unistd::fork()`; setsid; redirect stdio to `/dev/null` |

In all cases the child PID is written to `~/.amux/headless/amux.pid`. `run_kill()` reads the PID, sends `SIGTERM`, and waits up to 5 seconds before `SIGKILL`; then removes the PID file and (on macOS) unloads the plist.

### Logging

`logging.rs` configures `tracing-subscriber` differently by mode:

- **Foreground**: `fmt::Subscriber` with `ANSI` colours, directed to stdout.
- **Background**: `fmt::Subscriber` with JSON format, appending to `~/.amux/headless/amux.log`. A size guard truncates the log when it exceeds 100 MB (keeping the last 10 MB) to prevent unbounded growth.

A heartbeat task logs an `INFO`-level summary every 60 seconds: active session count, running command count.

---

## Testing Strategy

| Layer | Location | What is tested |
|-------|----------|----------------|
| Unit — per module | inline `#[cfg(test)]` | Individual functions, data structures |
| Unit — border colors | `tui::state::tests` | All 6 combinations of phase × focus |
| Unit — PTY data | `tui::state::tests` | `\r`/`\n`/`\r\n` processing, live-line updates |
| Unit — container window | `tui::state::tests` | Container state transitions, PTY routing, summary generation |
| Unit — container render | `tui::render::tests` | Container window overlay, minimized bar, summary bar |
| Unit — container input | `tui::input::tests` | Key handling in maximized/minimized/hidden states |
| Unit — CLI/spec parity | `cli::tests` | Every clap flag for each subcommand is present in `spec::*_FLAGS` and vice versa — fails immediately when the two diverge |
| Unit — flag parser | `tui::flag_parser::tests` | `parse_flags()` with every flag in `--flag value` and `--flag=value` forms; unknown flags ignored; value-taking flag not consuming a following `--flag` token; empty-value form |
| Unit — autocomplete completeness | `tui::input::tests` | `flag_suggestions_for("chat")` contains `--agent`; `flag_suggestions_for("implement")` contains `--agent` and `--workflow` |
| Integration — TUI chat with `--agent` | `tui::mod::tests` | Submitting `"chat --agent codex"` produces `PendingCommand::Chat { agent: Some("codex"), .. }` |
| Integration — TUI implement with `--agent=` | `tui::mod::tests` | Submitting `"implement 0042 --agent=opencode"` produces `PendingCommand::Implement { agent: Some("opencode"), .. }` |
| Unit — docker build streaming | `docker::tests` | Incremental line delivery, stderr capture, failure handling |
| Unit — docker stats | `docker::tests` | Stats parsing, container name generation |
| Unit — host settings / LSP suppression | `docker::tests` | `disable_lsp_recommendations` file creation, key merging, invalid-JSON fallback; `prepare_minimal` returns valid settings with LSP key |
| Unit — PTY | `tui::pty::tests` | Real `echo` and `sh -c 'exit 42'` processes |
| Unit — ready | `commands::ready::tests` | Summary table, interactive notice, options, entrypoints |
| Unit — implement | `commands::implement::tests` | Entrypoints (interactive + non-interactive) |
| Unit — chat | `commands::chat::tests` | Entrypoints, no-prompt verification |
| Unit — agent | `commands::agent::tests` | Shared agent launching |
| Unit — new | `commands::new::tests` | Slugify, numbering, template, find_template, kind parsing, run_with_sink |
| Unit — init flow | `commands::init_flow::tests` | Each stage independently via mock `InitQa` + `InitContainerLauncher`; `InitSummary` correctness; no filesystem or Docker access |
| Unit — CliInitQa | `commands::init_flow::tests` | Parses stdin responses (yes/no/empty/EOF) via byte cursor; edge cases for `ask_work_items_setup` |
| Unit — TuiInitQa | `commands::init_flow::tests` | Pre-collected answers returned without blocking; "never asked" represented as `false` not error |
| Integration — init CLI | `commands::init_flow::tests` | Temp git repo + mock launchers; asserts expected files written and `InitSummary` reports Ok per stage |
| Integration — init TUI parity | `commands::init_flow::tests` | Same scenario with `TuiInitQa`/`TuiContainerLauncher`; asserts identical file outcomes to CLI — structural guarantee surfaces cannot diverge |
| Integration — CLI | `tests/cli_integration.rs` | Binary-level: help, version, flags, work items |
| Integration — parity | `tests/command_tui_parity.rs` | Shared logic between command/TUI modes, container lifecycle, tab-cwd correctness |
| Unit — download | `commands::download::tests` | Tarball extraction, file counting, empty tarball error |
| Integration — download | `tests/download_integration.rs` | GitHub template downloads, aspec folder download, init integration, fallback |
| Integration — Docker | `tests/dockerfile_build.rs` | Builds each agent template Dockerfile to verify validity |
| Unit — headless db | `commands::headless::db::tests` | Schema creation, session CRUD, command CRUD, UUID uniqueness, field round-trips through serde |
| Unit — headless process | `commands::headless::process::tests` | PID file write/read/delete; live vs. stopped process detection; missing PID file handling |
| Unit — headless config | `config::tests` | `headlessWorkDirs` deserializes from `GlobalConfig` JSON; round-trips through save/load; missing field produces empty Vec |
| Unit — headless CLI parsing | `cli::tests` | Each `HeadlessAction` variant parses correctly: `--port`, `--workdirs` (single and multiple), `--background`; default values |
| Integration — headless HTTP | `commands::headless::server::tests` | Full session + command lifecycle against a server on a random port: create session → submit command → poll status → retrieve stdout; DB state matches HTTP responses |
| Integration — headless allowlist | `commands::headless::server::tests` | Server started with one allowlisted dir; session creation with non-allowlisted dir returns HTTP 403 |
| End-to-end — headless | `tests/headless_integration.rs` | `amux headless start` in a subprocess; HTTP requests via `reqwest`; assert response shape, log files created, DB entries exist |

### Window Border Color Matrix

| Phase | Focus | Color |
|-------|-------|-------|
| Running | ExecutionWindow (selected) | Blue |
| Running | CommandBox (unselected) | Grey |
| Done | ExecutionWindow (selected) | Green |
| Done | CommandBox (unselected) | Grey |
| Error | ExecutionWindow (selected) | Red |
| Error | CommandBox (unselected) | Red |
| Idle | any | DarkGray |

The parity tests are the most important: they verify that `run_with_sink`,
`find_work_item`, autocomplete, auth functions, summary table, interactive notice,
and non-interactive entrypoints produce the same results regardless of which
caller invokes them.

---

[← Headless Mode](08-headless-mode.md) · [Contents](contents.md)
