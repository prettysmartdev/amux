# Work Item: Task

Title: grand architecture refactor — part 3/5 — Layer 2 (Command + Dispatch)
Issue: n/a — third of five work items implementing `aspec/architecture/2026-grand-architecture.md`

## Required reading before starting

This work item is the third of five executing the grand architecture refactor described in `aspec/architecture/2026-grand-architecture.md`. The implementing agent **MUST** read that document, the previous two work items (`0066-…` and `0067-…`), and the current state of `src/data/` and `src/engine/` before writing any code.

The four tenets are restated for emphasis:

1. Layer 2 (command) consumes Layer 0 (data) and Layer 1 (engine) only. It MUST NOT call into Layer 3 (frontend) or Layer 4 (binary). When commands need user input or output, they accept frontend traits *defined by Layer 2* — Layer 3 implements them.
2. Frontends contain no business logic. Every command knob — every flag, every prompt, every dialog selection — flows through Layer 2's `Dispatch` system or through a per-command frontend trait. Frontends do not parse, validate, or interpret command strings; they hand raw input to Dispatch and render whatever Dispatch hands back.
3. Typed objects over `pub fn`. Each amux command becomes a `*Command` struct that implements a `Command` trait and exposes `run_with_frontend(frontend) -> CommandOutcome`. No `pub async fn run(args)` style.
4. **The full list of available commands and flags lives ONLY in `Dispatch`. NEVER in any frontend.** Frontends ask Dispatch for projections (clap definitions, TUI hint strings, headless OpenAPI/JSON schemas). This is the single most important guarantee against mode-drift.

The companion work items are:

- `0066-grand-architecture-foundation-and-layer-0-data.md` (must be merged)
- `0067-grand-architecture-layer-1-engines.md` (must be merged)
- `0069-grand-architecture-layer-3-frontends-and-binary.md`
- `0070-grand-architecture-finalize-and-remove-oldsrc.md`

## Summary:

- Build `src/command/` with two halves: a `dispatch/` module that holds the canonical command catalogue and per-frontend projections, and a `commands/` module that holds one struct per amux command (`init`, `ready`, `implement`, `chat`, `exec prompt`, `exec workflow`, `claws`, `status`, `specs new`, `specs amend`, `config`, `headless`, `remote`, `new`, plus subcommands).
- Define a single `CommandCatalogue` data structure that enumerates every command, subcommand, flag, argument, and value type *exactly once*. Every projection (clap commands, TUI hints, headless schema) is generated from this catalogue. Adding a new flag is one edit in one file.
- Define a `Dispatch` type that frontends construct with a frontend-specific trait object (`CliCommandFrontend`, `TuiCommandFrontend`, `HeadlessCommandFrontend`). Dispatch uses the trait to pull flag values and then constructs and returns the appropriate `*Command` struct, instantiated with all engines, configs, and per-command frontend traits it needs.
- Define a `Command` trait: `async fn run_with_frontend(self, frontend: Self::Frontend) -> Result<CommandOutcome, CommandError>`. Each command has its own associated `Frontend` trait describing exactly the user-input methods that command requires.
- Move every command's business logic out of `oldsrc/commands/` (12k+ lines) into the appropriate `*Command::new` constructor + `run_with_frontend` body. No business logic remains anywhere else.
- Comprehensive unit tests for Dispatch projection consistency (clap ↔ TUI hints ↔ headless schema agree on every flag) plus per-command tests using fake engines and fake command frontends.

## User Stories

### User Story 1:
As a: future implementing agent picking up Layer 3

I want to:
construct a CLI, TUI, or headless frontend by handing Dispatch a frontend trait and rendering whatever it returns

So I can:
build a frontend in hundreds of lines instead of thousands, with zero risk of accidentally diverging from the canonical command list.

### User Story 2:
As a: maintainer adding a new flag to `amux implement`

I want to:
edit the command catalogue once and have the flag appear in CLI help, TUI hints, headless API schema, and the `*Command::new` signature simultaneously

So I can:
trust that mode parity is maintained by construction.

### User Story 3:
As a: maintainer reading `src/command/commands/implement.rs`

I want to:
see the entire `amux implement` business logic — flag interpretation, agent/model resolution, container option assembly, workflow construction, exit-code reporting — in one place, with all I/O routed through frontend traits

So I can:
fix bugs without sifting through CLI, TUI, and headless code paths.

## Implementation Details:

### 0. Required reading and ground rules

- Read `aspec/architecture/2026-grand-architecture.md` end-to-end.
- Read `aspec/uxui/cli.md` to understand the canonical CLI command surface that must be preserved (no changes to user-visible CLI behavior in this work item).
- Read `0066-…` and `0067-…` and the current state of `src/data/` and `src/engine/`.
- For reference only: `oldsrc/cli.rs` (the legacy clap definitions, 2496 lines) and `oldsrc/commands/*.rs` (12k+ lines of business logic). Use these to understand existing behavior; **do not** port them verbatim — restructure into `*Command` types.
- When uncertain, ASK THE DEVELOPER.

### 1. `src/command/dispatch/` — the canonical catalogue and projections

#### 1a. `CommandCatalogue` (`src/command/dispatch/catalogue.rs`)

`CommandCatalogue` is a single static (or `OnceLock`-built) data structure listing every command. Each entry contains:

```rust
pub struct CommandSpec {
    pub name: &'static str,                     // "implement"
    pub aliases: &'static [&'static str],
    pub help: &'static str,                     // shown in clap, in TUI hint, in OpenAPI desc
    pub long_help: Option<&'static str>,
    pub arguments: &'static [ArgumentSpec],
    pub flags: &'static [FlagSpec],
    pub subcommands: &'static [&'static CommandSpec],
}

pub struct FlagSpec {
    pub long: &'static str,                     // "yolo"
    pub short: Option<char>,                    // None for --yolo
    pub help: &'static str,
    pub kind: FlagKind,                         // Bool, String, OptionalString, Enum(&'static [&'static str]), VecString, Path, etc.
    pub default: FlagDefault,
    pub frontends: FrontendVisibility,          // CLI-only? TUI-only? all three?
}

pub struct ArgumentSpec { /* analogous */ }
```

`CommandCatalogue` exposes:

```rust
impl CommandCatalogue {
    pub fn get() -> &'static CommandCatalogue;
    pub fn root() -> &'static CommandSpec;
    pub fn lookup(path: &[&str]) -> Option<&'static CommandSpec>;  // ["exec", "prompt"]
}
```

The catalogue MUST enumerate every command currently defined in `oldsrc/cli.rs`:

- `init`, `ready`, `implement`, `chat`, `exec prompt`, `exec workflow`, `claws *`, `status`, `specs new`, `specs amend`, `config *`, `headless *`, `remote *`, `new *`.

If the catalogue and `oldsrc/cli.rs` ever disagree on an existing command's name, alias, flag, or default, the catalogue is wrong and must be fixed in this work item — there is to be zero user-visible drift.

#### 1b. Projections (`src/command/dispatch/projections/`)

```rust
// src/command/dispatch/projections/clap.rs
impl CommandCatalogue {
    pub fn build_clap_command(&self) -> clap::Command;
}

// src/command/dispatch/projections/tui_hints.rs
impl CommandCatalogue {
    pub fn tui_hint_for(&self, path: &[&str]) -> Option<TuiHint>;     // hint shown above the TUI command box
    pub fn tui_completions(&self, partial: &str) -> Vec<TuiCompletion>;
}

// src/command/dispatch/projections/headless_schema.rs
impl CommandCatalogue {
    pub fn openapi_schema(&self) -> serde_json::Value;
    pub fn rest_route_table(&self) -> Vec<RestRoute>;
}
```

Frontends call only these projection methods; they MUST NEVER hard-code a command name, flag name, or default value. A unit test enforces this — see Test Considerations.

#### 1c. `Dispatch` (`src/command/dispatch/mod.rs`)

```rust
pub struct Dispatch<F: CommandFrontend> {
    catalogue: &'static CommandCatalogue,
    frontend: F,
    session: Arc<RwLock<Session>>,
    runtime: Arc<ContainerRuntime>,
    git_engine: Arc<GitEngine>,
    overlay_engine: Arc<OverlayEngine>,
    auth_engine: Arc<AuthEngine>,
    workflow_state_store: Arc<WorkflowStateStore>,
}

impl<F: CommandFrontend> Dispatch<F> {
    pub fn new(
        frontend: F,
        session: Arc<RwLock<Session>>,
        engines: Engines,
    ) -> Self;

    pub async fn run_command(self, path: &[&str]) -> Result<CommandOutcome, CommandError>;
    pub fn build_command(self, path: &[&str]) -> Result<BuiltCommand, CommandError>;
}
```

`CommandFrontend` is the catch-all trait that frontends implement to *supply* flag values to Dispatch:

```rust
pub trait CommandFrontend: Send + Sync {
    fn flag_bool(&self, command_path: &[&str], flag: &str) -> Result<Option<bool>, CommandError>;
    fn flag_string(&self, command_path: &[&str], flag: &str) -> Result<Option<String>, CommandError>;
    fn flag_strings(&self, command_path: &[&str], flag: &str) -> Result<Vec<String>, CommandError>;
    fn flag_path(&self, command_path: &[&str], flag: &str) -> Result<Option<PathBuf>, CommandError>;
    fn flag_enum(&self, command_path: &[&str], flag: &str) -> Result<Option<String>, CommandError>;
    fn argument(&self, command_path: &[&str], name: &str) -> Result<Option<String>, CommandError>;
    // …complete the surface so every FlagKind has a corresponding method
}
```

Three concrete `CommandFrontend` implementations live in Layer 3 (built in 0069):

- `CliCommandFrontend` — wraps `clap::ArgMatches`.
- `TuiCommandFrontend` — wraps the parsed TUI command-box input.
- `HeadlessCommandFrontend` — wraps an HTTP request body + query parameters.

Dispatch validates flag types and required-vs-optional based on the catalogue and surfaces structured errors back to the frontend (`CommandError::MissingRequiredFlag`, etc.). **Validation lives only here**; Layer 3 never validates user input.

Dispatch also exposes a `parse_command_box_input(raw: &str) -> Result<ParsedCommandBoxInput, CommandError>` helper used by the TUI's command-box widget. The TUI submits the raw user string; Dispatch tokenizes it against the catalogue, returns a typed `ParsedCommandBoxInput { path, flags, arguments }`, and the TUI feeds that back through a `TuiCommandFrontend` to invoke `Dispatch::run_command`. **All command-string interpretation lives here**, never in the TUI.

`Dispatch::run_command(["implement"])` looks up the spec, asks the frontend for every flag, instantiates `ImplementCommand::new(...)`, and calls its `run_with_frontend`. The per-command frontend trait (e.g. `ImplementCommandFrontend`) is *requested from* the outer `CommandFrontend` via a method like:

```rust
pub trait CommandFrontend: Send + Sync {
    // ...flag methods...
    fn implement_frontend(&self) -> Box<dyn ImplementCommandFrontend>;
    fn ready_frontend(&self) -> Box<dyn ReadyCommandFrontend>;
    fn chat_frontend(&self) -> Box<dyn ChatCommandFrontend>;
    // …one per command that needs a per-command frontend
}
```

ASK THE DEVELOPER if you find a cleaner pattern (e.g. associated types, trait objects keyed by `TypeId`); the grand architecture document calls out the trait-per-command pattern explicitly so default to that.

### 2. `src/command/commands/` — one struct per command

For each command in the catalogue, create a module under `src/command/commands/` containing:

- The `*Command` struct, owning every flag value, every engine reference, and every Layer 0 type it needs.
- The `*CommandFrontend` trait, listing exactly the user-input methods that command needs.
- The `impl Command for *Command` block with `run_with_frontend(frontend) -> CommandOutcome`.
- Unit tests against fake engines and a fake frontend.

#### Example skeletons

`src/command/commands/implement.rs`:

```rust
pub struct ImplementCommand {
    work_item: WorkItemId,
    flags: ImplementFlags,
    session: Arc<RwLock<Session>>,
    runtime: Arc<ContainerRuntime>,
    git: Arc<GitEngine>,
    overlay: Arc<OverlayEngine>,
    workflow_store: Arc<WorkflowStateStore>,
    workflow: Option<Workflow>,    // resolved from --workflow flag
}

pub trait ImplementCommandFrontend: ContainerFrontend + WorkflowFrontend + Send {
    fn report_work_item_summary(&mut self, summary: &WorkItemSummary);
    fn confirm_destructive_worktree_remove(&mut self, branch: &str) -> Result<bool, CommandError>;
    // ...everything currently prompted in oldsrc/commands/implement.rs that is not
    //    already covered by ContainerFrontend / WorkflowFrontend
}

impl Command for ImplementCommand {
    type Frontend = Box<dyn ImplementCommandFrontend>;
    type Outcome = ImplementOutcome;

    async fn run_with_frontend(self, frontend: Self::Frontend) -> Result<Self::Outcome, CommandError> {
        // 1. Resolve agent + model (via Layer 0 EffectiveConfig + Layer 1 OverlayEngine).
        // 2. Build the OverlayRequest, call OverlayEngine::build_overlays.
        // 3. Build the ContainerOption list.
        // 4a. If self.workflow.is_some(): construct a WorkflowEngine, run it.
        // 4b. Else: ContainerRuntime::build → ContainerInstance → ContainerExecution → wait.
        // 5. Wrap the exit info in ImplementOutcome and return.
    }
}
```

`src/command/commands/ready.rs`, `chat.rs`, `init.rs`, `init_flow.rs`-equivalent, `exec_prompt.rs`, `exec_workflow.rs`, `claws.rs`, `status.rs`, `specs_new.rs`, `specs_amend.rs`, `config.rs`, `headless_*.rs`, `remote.rs`, `new_workflow.rs`, `new_skill.rs`, `parity.rs`, `download.rs`, `output.rs`, `agent.rs`, `auth.rs` — every command currently in `oldsrc/commands/` becomes one of these structs.

#### What moves into `*Command::run_with_frontend`

- All flag interpretation, all option construction, all engine invocation, all output assembly.
- Any prompts to the user — moved to per-command frontend trait methods.
- Any reporting of progress — moved to frontend trait methods like `report_*`.
- Any exit-code interpretation — turned into typed `*Outcome` values.

#### What is forbidden

- No `eprintln!`, no `println!`, no direct user-facing I/O. Output goes through the frontend trait.
- No `clap::ArgMatches` references inside `*Command` bodies. Flag values arrive as typed fields populated by Dispatch.
- No `crossterm`, no `ratatui`, no `axum`. Those are Layer 3.
- No "if this is the CLI vs TUI vs headless" checks. The command never knows which frontend is on the other side.

### 3. Errors

`src/command/error.rs` defines `CommandError` covering every failure mode in Layer 2. It wraps `EngineError` and `DataError` from below. Layer 3 wraps `CommandError` in its own user-facing presentation; Layer 2 does not depend on Layer 3 errors.

### 4. Migration of legacy command modules

Every file under `oldsrc/commands/` has a Layer 2 destination:

| oldsrc                           | Layer 2 destination                              |
|----------------------------------|--------------------------------------------------|
| `commands/agent.rs`              | `command/commands/agent.rs` (subcommands of `amux agent` if user-facing; otherwise an engine helper — ASK THE DEVELOPER) |
| `commands/auth.rs`               | `command/commands/auth.rs` if it is a user command, else absorbed into `engine/auth/` |
| `commands/chat.rs`               | `command/commands/chat.rs`                       |
| `commands/claws.rs`              | `command/commands/claws.rs`                      |
| `commands/config.rs`             | `command/commands/config.rs`                     |
| `commands/download.rs`           | `command/commands/download.rs`                   |
| `commands/exec.rs`               | `command/commands/exec_prompt.rs` + `exec_workflow.rs` |
| `commands/headless/*`            | `command/commands/headless/*` (start/stop/status/etc) |
| `commands/implement.rs`          | `command/commands/implement.rs`                  |
| `commands/init.rs` + `init_flow.rs` | `command/commands/init.rs`                    |
| `commands/new.rs` + `new_cmd.rs` + `new_workflow.rs` + `new_skill.rs` | `command/commands/new/*` |
| `commands/output.rs`             | `command/commands/output.rs` *or* a frontend helper — ASK THE DEVELOPER |
| `commands/parity.rs`             | `command/commands/parity.rs` (used by tests; keep as a command) |
| `commands/ready.rs` + `ready_flow.rs` | `command/commands/ready.rs`                 |
| `commands/remote.rs`             | `command/commands/remote.rs`                     |
| `commands/spec.rs` + `specs.rs`  | `command/commands/specs/*`                       |
| `commands/status.rs`             | `command/commands/status.rs`                     |

Anything in this table that is "actually a helper, not a command" should be flagged with the developer and moved into Layer 1 instead.

### 5. What must NOT happen in this work item

- No changes to `oldsrc/`. The user-visible binary still ships from `oldsrc/`.
- No work in `src/frontend/` beyond ensuring it compiles. The CLI/TUI/headless rebuild is 0069.
- No `pub fn run(args)` style command entry points. Every command is a struct + trait impl.
- No frontend-specific code in `src/command/`. Dispatch projects to clap/TUI/headless via methods on `CommandCatalogue`; it does not host frontend logic.
- No swap of the binary entrypoint. `amux` still runs from `oldsrc/`.

## Edge Case Considerations:

- **Subcommand nesting (`exec prompt`, `headless start`)**: the catalogue must support arbitrary nesting. Test depth-2 lookups (`["exec", "prompt"]`, `["headless", "start"]`) explicitly.
- **Catalogue-clap drift**: if any flag exists in `clap` but not in the catalogue (or vice versa), the unit test `catalogue_clap_consistency` fails. Same for `catalogue_tui_consistency` and `catalogue_headless_consistency`.
- **Mutually exclusive flags**: today's clap uses `conflicts_with` and `requires`. The catalogue MUST encode these constraints in `FlagSpec` so projections honor them. ASK THE DEVELOPER if a richer constraint language is needed (e.g. "exactly one of {plan, yolo, auto}").
- **Per-command frontend trait composition**: some commands need both a `ContainerFrontend` and a `WorkflowFrontend` (e.g. `implement` with `--workflow`). Per-command frontend traits MUST be expressed as supertrait bounds (`trait ImplementCommandFrontend: ContainerFrontend + WorkflowFrontend`) so a single Layer 3 type satisfies them all.
- **Default value drift**: `aspec/uxui/cli.md` documents some defaults; the catalogue is the source of truth post-refactor. ASK THE DEVELOPER whether to regenerate `aspec/uxui/cli.md` from the catalogue (work item 0070's responsibility) or by hand.
- **`--json` output mode**: today some commands accept `--json` to produce structured output. In the new architecture, the command's `*Outcome` is a typed value; JSON serialization is a frontend concern, not a command concern. Ensure every `*Outcome` derives `Serialize`.
- **`always_non_interactive` global config**: today's `commands/mod.rs::run` mutates flags before dispatch. In the new architecture, this mutation belongs in `Dispatch::build_command` after pulling the flag value but before constructing the `*Command`. Cover with unit tests.
- **`AMUX_OVERLAYS` env validation**: today's `commands/mod.rs::run` validates this env up front for every command. In the new architecture, this validation belongs to `OverlayEngine::new` (Layer 1) or `EffectiveConfig::overlays` (Layer 0) — ASK THE DEVELOPER. Whichever layer owns it, every command path MUST trigger the validation early.

## Test Considerations:

### Test philosophy (read first)

Tests for Layer 2 are **designed and written from scratch** alongside the new dispatch and command structs. **Do not port tests from `oldsrc/commands/**/#[cfg(test)] mod tests` or from `oldsrc/cli.rs` test blocks.** Those tests assume the legacy parameter-style command entry points (`pub async fn run(args)`) and frontend-conflated business logic. Reusing them carries forward the very design we are replacing.

The narrow exception is a test that satisfies **all** of the following:

1. Asserts a precise behavioral invariant the new command MUST preserve (e.g. flag precedence ordering, `AMUX_OVERLAYS` env validation timing, `always_non_interactive` global config behavior, exit-code mapping).
2. Compiles unchanged or with mechanical edits against the new `*Command` types.
3. Exercises only Layer 0 + Layer 1 + Layer 2 — no Layer 3, no legacy types.

If any old test is brought forward under this exception, the PR description MUST list it with a one-sentence justification. The default answer is "rewrite from scratch."

This work item produces **only Layer 2 unit tests** using fake engines and fake `CommandFrontend` / per-command frontends. **No real Docker, no real git beyond hermetic `git init` against `tempfile`, no real HTTP server, and no real CLI/TUI binary.** All cross-layer integration, end-to-end, parity, and binary-level smoke tests are 0070's responsibility against a freshly rebuilt `tests/` directory.

### Unit tests (colocated `#[cfg(test)] mod tests`)

- **`CommandCatalogue`**:
  - Every command and flag listed in `aspec/uxui/cli.md` is present in the catalogue with the documented name, kind, default, and `FrontendVisibility`. (Drive via a data-table test, not per-flag duplicated assertions.)
  - `lookup(["exec", "prompt"])` returns the expected spec; `lookup(["bogus"])` returns `None`; `lookup(["init", "bogus"])` returns `None`.
  - Mutually exclusive constraints in `FlagSpec` are honored by a `FlagSpec::conflicts_with` accessor.
- **Projections (consistency — these are Layer 2 unit tests, not integration tests)**:
  - `catalogue_clap_consistency`: build the clap command from the catalogue, walk every `Arg`, assert each is present in the catalogue with matching kind/default/help.
  - `catalogue_tui_consistency`: every catalogue command has a `TuiHint`; every documented flag appears in `tui_completions` for an appropriate prefix.
  - `catalogue_headless_consistency`: every catalogue command appears in `rest_route_table` and `openapi_schema`; method + path stable against a checked-in fixture.
  - **No drift test against `oldsrc`** — the catalogue is the new source of truth. Compare against `aspec/uxui/cli.md` and the checked-in projection fixtures, not against legacy clap definitions.
- **`Dispatch`** (with a recording `FakeCommandFrontend`):
  - For each catalogue entry, `Dispatch::run_command` builds the expected `*Command` struct with the expected field values (mock the constructor to record arguments).
  - Missing required flag → `CommandError::MissingRequiredFlag { command, flag }`.
  - Unknown flag (frontend supplies a value for a flag not in the catalogue) → `CommandError::UnknownFlag`.
  - Mutually exclusive flags both supplied → `CommandError::MutuallyExclusive`.
  - `parse_command_box_input("implement 0042 --yolo")` returns the expected `ParsedCommandBoxInput { path: ["implement"], arguments: {"work_item": "0042"}, flags: {"yolo": true} }`.
  - `parse_command_box_input` rejects unknown commands and unknown flags with structured errors that the TUI can render.
  - `always_non_interactive` global-config override is applied before `*Command` construction (verify by inspecting the recorded constructor argument, not by behavior).
  - `AMUX_OVERLAYS` env validation runs before any per-command construction (verify ordering by failing the env validator first and asserting no command was built).
- **Per-command unit tests** (`src/command/commands/<name>.rs`):
  - Each `*Command` has a focused test suite using a `FakeEngines` (mock `ContainerRuntime`, `GitEngine`, `OverlayEngine`, `AuthEngine`, `WorkflowStateStore`) and a recording per-command frontend.
  - Happy path: command resolves flags, calls the expected engine methods with expected arguments, produces the expected `*Outcome`.
  - Frontend interactions: every per-command frontend method is exercised at least once (e.g. `confirm_destructive_worktree_remove` invoked with the expected branch when the relevant scenario is set up).
  - Error mapping: each upstream `EngineError` / `DataError` variant maps to a defined `CommandError` variant.
  - `*Outcome` `Serialize` round-trip is byte-stable for `--json` callers (the outcome itself is JSON-stable; how a frontend renders it is Layer 3).

### What does NOT belong in this work item

- Tests using real Docker, real container runtimes, real network, or real HTTP servers.
- Tests that drive a real Layer 1 engine end-to-end (e.g. real `ContainerRuntime::build`). Use the fake/mock at the trait surface defined in 0067.
- Tests in the top-level `tests/` directory. Leave it untouched; 0070 rebuilds it.
- Tests of any Layer 3 surface (CLI, TUI, headless) — those layers do not exist yet.
- Parity tests of any kind.

### Build & CI

- `cargo build --bin amux` (still from `oldsrc/`) succeeds.
- `cargo build --bin amux-next` succeeds — Layers 0+1+2 compile cleanly.
- `cargo test` passes including the new dispatch + per-command unit tests.

### Manual smoke test

- Run `amux` (still legacy code). Behavior must be identical to pre-refactor.

## Codebase Integration:

- Follow `aspec/architecture/2026-grand-architecture.md` as the source of truth.
- Follow `aspec/uxui/cli.md` for the user-facing command surface; do not change user-visible CLI behavior in this work item.
- Follow established conventions, best practices, testing, and architecture patterns from the project's `aspec/`.
- Do not edit `oldsrc/`. Do not delete `oldsrc/`. Both are in 0070's scope.
- Do not introduce upward calls from Layer 2 to Layer 3/4. Use traits owned by Layer 2.
- Do not introduce free `pub fn` for stateful command concerns. Prefer struct + methods.
- The PR description MUST link to `aspec/architecture/2026-grand-architecture.md` and to this work item, MUST list any developer-clarification questions raised, and MUST include a checklist confirming that every entry in `oldsrc/commands/` has a destination in `src/command/commands/` (and call out any items that turned out to be Layer 1 helpers instead).
- After this work item lands, the next agent picks up `0069-grand-architecture-layer-3-frontends-and-binary.md`.
