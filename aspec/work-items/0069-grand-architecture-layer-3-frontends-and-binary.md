# Work Item: Task

Title: grand architecture refactor — part 4/5 — Layer 3 frontends (CLI, TUI, Headless) + Layer 4 binary; swap entrypoint
Issue: n/a — fourth of five work items implementing `aspec/architecture/2026-grand-architecture.md`

## Required reading before starting

This work item is the fourth of five executing the grand architecture refactor described in `aspec/architecture/2026-grand-architecture.md`. The implementing agent **MUST** read that document, the previous three work items (`0066-…`, `0067-…`, `0068-…`), and the current state of `src/data/`, `src/engine/`, and `src/command/` before writing any code.

The four tenets, again:

1. **Frontends contain NO business logic.** This is the most heavily enforced tenet of this work item. Any `if`, `match`, or computed-default behavior that depends on the *meaning* of a command, flag, or response is wrong and lives in Layer 2. Frontends parse keystrokes/HTTP/argv into `CommandFrontend` answers and render typed outcomes back. That is all.
2. Layer 3 (frontend) consumes Layer 0 (data), Layer 1 (engine), and Layer 2 (command) only — but in practice should consume *only* Layer 2 (`Dispatch`, `*CommandFrontend` traits, `*Outcome` types) and Layer 0 (`Session`, `SessionManager`). It should rarely need to touch Layer 1 directly. Anywhere it does, ASK THE DEVELOPER whether that touch is necessary or whether a missing Layer 2 surface should be added.
3. Layer 4 (binary) is minimal. `main.rs` builds clap from `CommandCatalogue`, parses argv, and dispatches to either the CLI frontend (when a subcommand is present) or the TUI frontend (bare invocation). That is the entire body of `main`.
4. When uncertain, ASK THE DEVELOPER.

The companion work items are:

- `0066-grand-architecture-foundation-and-layer-0-data.md` (must be merged)
- `0067-grand-architecture-layer-1-engines.md` (must be merged)
- `0068-grand-architecture-layer-2-command-and-dispatch.md` (must be merged)
- `0070-grand-architecture-finalize-and-remove-oldsrc.md`

## Summary:

- Build `src/frontend/cli/` — implements `CommandFrontend`, every `*CommandFrontend`, and the `ContainerFrontend` and `WorkflowFrontend` adapters needed for stdin/stdout/stderr binding. Builds clap arg matches and projects them through Dispatch. No business logic.
- Build `src/frontend/tui/` — fully reimplements the existing TUI on top of `SessionManager`, `Dispatch`, and the per-command frontend traits. Tabs become `Session` instances managed by `SessionManager`. Command-box input goes straight to `Dispatch`. Hints come from `CommandCatalogue::tui_hint_for`. Dialogs render data structures returned from per-command frontend trait calls; user choices are returned to lower layers as typed action enums. Every existing TUI behavior, keyboard shortcut, and visual element is preserved.
- Build `src/frontend/headless/` — fully reimplements the existing headless server on top of `SessionManager` and `Dispatch`. Routes come from `CommandCatalogue::rest_route_table`. Request validation is left to Dispatch. The handler body for each route is uniform: build a `HeadlessCommandFrontend`, hand it to `Dispatch::run_command`, serialize the `*Outcome` to JSON.
- Implement `src/main.rs` (Layer 4) as a tiny binary that builds clap from the catalogue, parses argv, constructs `SessionManager` + engines, and dispatches to either the CLI or the TUI frontend. The headless server is launched by the `headless start` *command* (Layer 2), not by `main.rs`.
- Swap the `Cargo.toml` so the user-facing `amux` binary is built from `src/main.rs`. Rename the previous `amux-next` target out of existence. The legacy `oldsrc/` tree remains in place as frozen reference material; it is no longer compiled.
- Comprehensive parity tests (existing user-visible behavior, no regressions). The next work item, 0070, deletes `oldsrc/` once parity is signed off.

## User Stories

### User Story 1:
As a: existing amux user

I want to:
upgrade to the new amux binary and have every CLI command, every TUI keystroke, every headless API endpoint behave identically to before

So I can:
benefit from the new architecture without learning anything new or losing any feature.

### User Story 2:
As a: future implementing agent adding a new frontend (desktop app, code editor extension, kubernetes operator)

I want to:
read `src/frontend/cli/`, `src/frontend/tui/`, and `src/frontend/headless/` and see three small, self-similar implementations that all consume Dispatch the same way

So I can:
add a fourth frontend by following the same pattern, with no business-logic decisions to make.

### User Story 3:
As a: maintainer reading `src/main.rs`

I want to:
see fewer than 100 lines of code that build clap, dispatch, and return

So I can:
trust that the entrypoint is not hiding any business logic.

## Implementation Details:

### 0. Required reading and ground rules

- Read `aspec/architecture/2026-grand-architecture.md` end-to-end.
- Read `aspec/uxui/cli.md` for user-visible CLI behavior; nothing in this work item changes that surface.
- Read the current state of `src/data/`, `src/engine/`, and `src/command/`.
- For reference only (do not port verbatim): `oldsrc/main.rs`, `oldsrc/cli.rs`, `oldsrc/tui/*.rs` (~21k lines), `oldsrc/commands/headless/*.rs`. Use these to extract user-visible behavior; the implementation MUST be a fresh reimplementation on top of Dispatch.
- When uncertain, ASK THE DEVELOPER.

### 1. `src/frontend/cli/` — CLI frontend

Files:

- `mod.rs` — entry point; `pub async fn run(matches: clap::ArgMatches, runtime_ctx: RuntimeContext) -> ExitCode`.
- `command_frontend.rs` — `CliCommandFrontend` implementing `CommandFrontend` over `clap::ArgMatches`.
- `per_command/` — one file per command implementing the corresponding `*CommandFrontend` (e.g. `implement.rs` implements `ImplementCommandFrontend`).
- `container_frontend.rs` — `CliContainerFrontend` binding `ContainerFrontend` to stdin/stdout/stderr (with PTY allocation when stdin is a TTY).
- `workflow_frontend.rs` — `CliWorkflowFrontend` rendering workflow status to stderr, prompting on stdin for `user_choose_next_action`, etc.
- `output.rs` — small helpers for terminal styling (colors, hyperlinks). Pure presentation.

The CLI frontend's logic is small:

```rust
pub async fn run(matches: ArgMatches, ctx: RuntimeContext) -> ExitCode {
    let path = command_path_from_matches(&matches);
    let frontend = CliCommandFrontend::new(matches);
    let dispatch = Dispatch::new(frontend, ctx.session, ctx.engines);
    match dispatch.run_command(&path).await {
        Ok(outcome) => render_outcome_for_cli(outcome).await,
        Err(err) => render_error_for_cli(err).await,
    }
}
```

`render_outcome_for_cli` and `render_error_for_cli` are pure-presentation helpers that pattern-match on the typed outcome/error and write to stdout/stderr. Any decision that *changes behavior* belongs in Layer 2.

### 2. `src/frontend/tui/` — TUI frontend

This is the largest block of work in the refactor (legacy TUI is ~21k lines). The grand architecture document is explicit:

> User-perceptible functionality, UX, design, and keyboard operations should all remain identical to pre-refactor, but powered by the layered architecture instead of any TUI package business logic.

Files (proposed; ASK THE DEVELOPER if a different split fits better):

- `mod.rs` — entry point: builds `SessionManager` (in-memory), constructs the `App`, runs the event loop.
- `app.rs` — `App` owns the `Terminal`, the `SessionManager`, and the active dialog stack. No business logic.
- `tabs.rs` — tab management (one `Session` per tab) on top of `SessionManager`.
- `command_box.rs` — text input widget. Captures keystrokes; on submit, hands the raw string to Layer 2's `Dispatch::parse_command_box_input(...)` (added in 0068). Performs no parsing or interpretation itself.
- `command_frontend.rs` — `TuiCommandFrontend` implementing `CommandFrontend`. Pulls flag values from the parsed command-box input.
- `per_command/` — one file per command implementing the corresponding `*CommandFrontend`. Each is a thin wrapper that bridges command frontend trait calls into TUI dialog rendering and keyboard input.
- `container_view.rs` — `TuiContainerFrontend` implementing `ContainerFrontend`. Owns the PTY allocation, scrollback buffer, and rendering.
- `workflow_view.rs` — `TuiWorkflowFrontend` implementing `WorkflowFrontend`. Renders the workflow control dialog, yolo countdowns, etc.
- `dialogs/` — pure-presentation dialog widgets (selection lists, confirmations, text prompts). Each dialog has a typed input (the data Layer 2 wants the user to choose from) and a typed output (the user's choice). Dialogs do NOT decide what the next step is — they only render and collect.
- `keymap.rs` — keyboard shortcut definitions. Pure presentation.
- `render.rs` — pure rendering of UI chrome (tab bar, status bar, hints).
- `hints.rs` — pulls hint text via `CommandCatalogue::tui_hint_for`.

Critical constraints from the grand architecture document:

- All command-box input is routed directly to a method in the `Dispatch` package, no parsing or anything else done by the TUI itself.
- All hint text for commands, subcommands, and flags comes from methods in the `Dispatch` package.
- All data displayed in any dialog comes from per-command frontend trait calls. The dialog is a pure render; the data and the choice options flow up from Layer 2.
- Action objects (e.g. `NextAction::AdvanceWorkflow`, `NextAction::PauseWorkflow`) are typed enums returned by frontend trait methods. The TUI does not invent these; they are defined alongside `WorkflowFrontend` etc. in Layers 1/2.

#### Behavioral parity checklist

The TUI must preserve, with zero user-visible drift:

- Tab opening, closing, switching, and ordering (every existing keyboard shortcut).
- Per-tab session state (`Session` replaces `TabState`).
- Command box behavior, completion, hint display.
- Container window rendering (stdout/stderr, scrollback, dynamic tab widths from work item ~recent).
- Workflow control dialog (advance, pause, resume, abort) — content from `WorkflowFrontend`.
- Yolo-mode countdown rendering (timing from `WorkflowEngine`, rendering here).
- Stuck-agent detection display.
- All status-bar elements.
- All keyboard shortcuts documented today.
- All error rendering (translations of `CommandError`, `EngineError`, `DataError` into user-friendly strings).

A line-by-line port from `oldsrc/tui/` is *not* the goal. The goal is to reproduce user-perceptible behavior on top of the new layers. Where the legacy code embedded business logic in the TUI (workflow advance decisions, agent resolution, etc.), that logic lives in Layer 2 now and the TUI only renders the result.

### 3. `src/frontend/headless/` — Headless frontend

Files:

- `mod.rs` — entry point: `pub async fn serve(config: HeadlessServeConfig, engines: Engines, session_manager: Arc<RwLock<SessionManager>>) -> Result<(), HeadlessError>`. **Layer 2 cannot call `serve` directly — that would be an upward call.** Instead, `HeadlessStartCommand` (Layer 2) accepts a `HeadlessStartCommandFrontend` trait at instantiation. The trait exposes a method like `serve_until_shutdown(config: HeadlessServeConfig) -> Result<(), CommandError>`. The CLI frontend's `HeadlessStartCommandFrontend` impl calls `crate::frontend::headless::serve(...)` — that is a peer call within Layer 3 and is allowed. The headless frontend never starts itself; it is always launched by an impl living in some other Layer 3 frontend (today, only the CLI's impl exists).
- `routes.rs` — registers HTTP routes derived from `CommandCatalogue::rest_route_table`. Each route handler is uniform (see below).
- `command_frontend.rs` — `HeadlessCommandFrontend` implementing `CommandFrontend` over a deserialized request body + query parameters.
- `per_command/` — one file per command implementing the corresponding `*CommandFrontend`. Where a command needs interactive input, the headless frontend either (a) returns a structured "needs input" response and resumes via a follow-up request, or (b) defaults safely. ASK THE DEVELOPER which model to use for each interactive command.
- `container_stream.rs` — `HeadlessContainerFrontend` implementing `ContainerFrontend` over an SSE/WebSocket stream of stdin/stdout/stderr chunks.
- `workflow_stream.rs` — `HeadlessWorkflowFrontend` implementing `WorkflowFrontend` over the same streaming surface.
- `auth.rs` — TLS + API-key middleware. Pure plumbing; the cryptographic logic is in `AuthEngine` (Layer 1).
- `errors.rs` — translates `CommandError` etc. into HTTP status codes + JSON error bodies.

Each route handler is the same shape:

```rust
async fn handle(State(app): State<AppState>, req: Request) -> Result<Response, HeadlessError> {
    let frontend = HeadlessCommandFrontend::from_request(&req)?;
    let dispatch = Dispatch::new(frontend, app.session, app.engines);
    let outcome = dispatch.run_command(&req.command_path()).await?;
    Ok(serialize_outcome(outcome)?)
}
```

The grand architecture document explicitly forbids the server from "just calling the CLI": the headless frontend talks to `Dispatch` directly, never spawns a child `amux` process.

#### Headless behavioral parity checklist

- Every route documented in the existing OpenAPI/handler set continues to exist with the same path, method, body schema, and response schema. Use `CommandCatalogue::rest_route_table` to enforce this; the catalogue MUST already match the existing surface as of 0068.
- TLS, bind-address, and auth-disabled behavior from work item 0065 is preserved. The `AuthEngine` (Layer 1) holds the logic; this frontend is plumbing.
- SSE/WebSocket streaming endpoints (chat, exec, implement output) preserve their wire format byte-for-byte.

### 4. `src/main.rs` — Layer 4

`main.rs` after this work item:

```rust
#![forbid(unsafe_code)]

use anyhow::Result;
use amux::command::dispatch::CommandCatalogue;
use amux::data::{Session, SessionManager, GlobalConfig};
use amux::engine::{ContainerRuntime, GitEngine, OverlayEngine, AuthEngine, WorkflowStateStore};
use amux::frontend::{cli, tui};

#[tokio::main]
async fn main() -> Result<std::process::ExitCode> {
    let clap_cmd = CommandCatalogue::get().build_clap_command();
    let matches = clap_cmd.get_matches();

    let global_config = GlobalConfig::load().unwrap_or_default();
    let git = std::sync::Arc::new(GitEngine::new());
    let runtime = std::sync::Arc::new(ContainerRuntime::detect(&global_config)?);
    // ...other engines...

    let session_manager = std::sync::Arc::new(parking_lot::RwLock::new(SessionManager::in_memory()));
    let session = Session::open(std::env::current_dir()?, &*git)?;
    session_manager.write().insert(session.clone())?;

    let ctx = RuntimeContext { session_manager, session: std::sync::Arc::new(parking_lot::RwLock::new(session)), engines: Engines { runtime, git, /* ... */ } };

    if matches.subcommand().is_some() {
        Ok(cli::run(matches, ctx).await)
    } else {
        Ok(tui::run(matches, ctx).await)
    }
}
```

That is the entire `main.rs` body. The `headless start` command launches the headless server through Layer 2 → Layer 1 → Layer 3 (`frontend::headless::serve`); `main.rs` does not branch on `headless`.

### 5. `Cargo.toml` swap

After this work item:

```toml
[[bin]]
name = "amux"
path = "src/main.rs"

[lib]
name = "amux"
path = "src/lib.rs"
```

Remove the `amux-next` target. Remove the `[[bin]]` and `[lib]` blocks pointing at `oldsrc/`. Leave the `oldsrc/` directory and its files in place — they are no longer compiled by Cargo, but they are not deleted yet. Update `Makefile` so `make all`, `make install`, `make test` continue to work; remove any `make test-next` shim added in 0066.

The `oldsrc/README.md` from 0066 stays. Add a note: "no longer compiled — see work item 0070 for removal."

### 6. What must NOT happen in this work item

- No business logic in `src/frontend/`. If a frontend needs to make a decision that affects behavior, the missing surface is in Layer 2; add it there.
- No deletion of `oldsrc/`. That is 0070.
- No edits inside `oldsrc/` other than possibly the `oldsrc/README.md` note.
- No new commands, new flags, or new user-visible behavior. This work item is *parity only*.
- No regressions in the `aspec/uxui/cli.md` documented surface.

## Edge Case Considerations:

- **Existing TUI tests**: `oldsrc/tui/state.rs` has substantial tests. They cannot run against the new TUI; reproduce the equivalent assertions against `Session` + `SessionManager` + the TUI's view code. ASK THE DEVELOPER if a particular test reveals a behavior that is not preserved.
- **`StartupReadyFlags`**: the legacy `main.rs` passes `--build`, `--no-cache`, `--refresh` into the TUI to be applied to a startup `ready` invocation. The new architecture handles this via `Dispatch` calling `ReadyCommand` at TUI startup; the TUI startup path constructs a `Dispatch` for `["ready"]` with the global flags pre-populated. Confirm with developer whether this is the right model.
- **Session lifetime in the TUI**: each tab owns one `Session`. Closing a tab removes the session from `SessionManager`. If a session has an in-flight container, `SessionManager::remove` must orchestrate cancellation through `ContainerExecution::cancel`. ASK THE DEVELOPER whether closing a tab forcibly kills running containers (legacy behavior) or prompts the user.
- **CLI vs TUI Session count**: `SessionManager::in_memory()` works for both single-session (CLI) and multi-session (TUI). Cover this with a unit test asserting both modes.
- **Headless multi-session concurrency**: each API session is a `Session`; `Dispatch::run_command` borrows the `Session` via the `Arc<RwLock<Session>>` provided to `Dispatch::new`. Long-running commands (chat, implement, exec workflow) hold the read lock across the lifetime of the command. Verify this does not deadlock with concurrent inspection requests.
- **Error rendering parity**: every error message a user might see today must be reproducible by the new error rendering. Capture the existing user-visible strings (or close paraphrases) in `tests/cli_error_parity.rs` and assert.
- **Color and TTY detection**: `oldsrc/commands/output.rs` handles color/no-color logic. Move this to `src/frontend/cli/output.rs` (pure presentation).
- **Help text**: `clap` builds help from the catalogue. Compare `amux help` and `amux <subcommand> --help` output before and after; differences must be limited to noise (whitespace, version string, help-ordering).
- **TUI keyboard shortcut conflicts**: the new TUI adds no shortcuts; preserve every existing one. ASK THE DEVELOPER if any new shortcut is requested as part of this work item (default: no).

## Test Considerations:

### Test philosophy (read first)

Tests for Layer 3 + Layer 4 are **designed and written from scratch** alongside the new frontends. **Do not port tests from `oldsrc/tui/**/#[cfg(test)] mod tests`, `oldsrc/commands/headless/**/#[cfg(test)]`, or `oldsrc/cli.rs` test blocks.** The old TUI tests assume `TabState` plus business-logic-in-the-frontend; the old headless tests assume the legacy ad-hoc routing; the old CLI tests assume a parameter-style command surface. All of these are explicitly designed away.

The narrow exception is a test that satisfies **all** of the following:

1. Asserts a user-visible behavior the new frontend MUST preserve (e.g. exact help-text format, exact SSE wire format, exact keyboard-shortcut set, exact prompt text in a confirmation dialog).
2. Compiles unchanged or with mechanical edits against the new frontend types.
3. Exercises only Layer 0 + 1 + 2 + 3 (and Layer 4 for binary-level tests). No legacy types.

If any old test is brought forward under this exception, the PR description MUST list it with a one-sentence justification. The default answer is "rewrite from scratch."

This work item produces **only Layer 3 unit tests and pure-presentation snapshot tests** plus a **manual sign-off checklist** that gates 0070. The full parity test suite, the real-Docker / real-network end-to-end tests, and the freshly rebuilt top-level `tests/` directory are 0070's responsibility. **Do not create any file under `tests/` in this work item.**

### Unit tests (colocated `#[cfg(test)] mod tests`)

- **CLI** (`src/frontend/cli/`):
  - `CliCommandFrontend::flag_bool / flag_string / flag_strings / flag_path / flag_enum / argument` correctly extract values from a synthesized `clap::ArgMatches` for every `FlagKind` in the catalogue (data-table test).
  - `render_outcome_for_cli` snapshot per `*Outcome` variant — uses `insta` or equivalent to lock the rendered stdout.
  - `render_error_for_cli` snapshot per `CommandError` variant — locks the rendered stderr including exit code mapping.
  - TTY-vs-pipe rendering decisions (color on, hyperlinks on/off, etc.) are unit-tested with a `Termios`-style abstraction.
- **TUI** (`src/frontend/tui/`):
  - `App` event loop processes a synthetic key event sequence and updates `SessionManager` as expected (open tab, close tab, switch tab — one test per shortcut, driven by a data table of `(key, expected_state_delta)`).
  - Command-box submit forwards the raw string to a mocked `Dispatch::parse_command_box_input` and routes the parsed result back through `Dispatch::run_command` with the expected path + flags.
  - `TuiWorkflowFrontend::user_choose_next_action` renders the dialog with the data passed in, simulates a user keypress, and returns the typed `NextAction`. (Pure unit test — no real terminal.)
  - Dialog widgets (selection list, confirmation, text input) snapshot-tested with `insta` against synthetic inputs and key sequences.
  - Hint rendering pulls from `CommandCatalogue::tui_hint_for` — assert the hint text comes from the catalogue, not a hard-coded string in the TUI.
  - Tab close with an in-flight container calls `ContainerExecution::cancel` on the right execution (mock the engine).
- **Headless** (`src/frontend/headless/`):
  - For each route in `CommandCatalogue::rest_route_table`, a focused test sends a representative `axum::http::Request` to the handler with a mocked `Dispatch::run_command` and asserts the handler called dispatch with the right command path and a `HeadlessCommandFrontend` populated from the request.
  - Auth middleware: token mode rejects bad tokens with 401, accepts good tokens with the expected response; disabled mode emits `X-Amux-Auth: disabled`; TLS-required mode rejects non-loopback bind without TLS.
  - SSE/WebSocket adapter (`HeadlessContainerFrontend`) writes stdout chunks in the expected wire format against a mocked stream sink — pure unit test, no real container.
  - Error translation: each `CommandError` variant maps to the documented HTTP status code and JSON error body.
- **Layer 4** (`src/main.rs`):
  - The body of `main` is small enough to test indirectly. Add a single integration-style unit test (still colocated, still no real binary) that runs the same logic with a synthetic argv and asserts the right frontend (cli vs tui) is selected.
  - Cargo bin compiles without warnings (CI guard).

### What does NOT belong in this work item

- Tests in the top-level `tests/` directory. Leave it untouched; 0070 rebuilds it from scratch.
- Tests that exercise the real `amux` binary as a subprocess.
- Tests that start a real headless HTTP server bound to a real port.
- Tests that launch a real TUI in a real terminal (or a `vt100`/`expect`-style terminal harness).
- Tests that hit a real Docker daemon, real git remote, or real network.
- Parity tests against the pre-refactor binary's output. Those are 0070.

### Build & CI

- `cargo build --release` produces a single statically-linked `amux` binary from `src/main.rs` (after the `Cargo.toml` swap).
- `cargo test` passes including the new Layer 3 unit tests.
- `cargo clippy --all-targets -- -D warnings` passes.
- `make all`, `make install`, `make test` work.

### Manual sign-off checklist (gating 0070)

This work item is the last point at which the legacy `oldsrc/` is still in the repo. Before merging, the implementing agent MUST manually exercise the new binary against a real environment and post a sign-off checklist in the PR description. **Automated parity tests are not yet written** — they are 0070's deliverable — so this manual pass is what catches regressions before 0070 deletes the legacy code.

The PR description MUST include:

- A table listing every command and subcommand documented in `aspec/uxui/cli.md`, each marked PASS / MINOR-DRIFT (with one-sentence justification) / REGRESSION (block).
- A confirmation that the TUI was launched on a real terminal, every documented keyboard shortcut was exercised, at least 3 tabs were opened, an `implement` workflow was run end-to-end (with at least one user dialog), and rendering was visually identical (or improved with documented justification) to pre-refactor.
- A confirmation that the headless server was started, every documented endpoint received a real `curl` invocation, and responses were wire-compatible with pre-refactor.

Any item that is REGRESSION blocks the PR. The implementing agent MUST fix or escalate to the developer. Do not merge with open regressions.

The corresponding **automated** tests for all of the above are written in 0070, against the freshly rebuilt `tests/` directory.

## Codebase Integration:

- Follow `aspec/architecture/2026-grand-architecture.md` as the source of truth.
- Follow `aspec/uxui/cli.md` for user-facing behavior; nothing in this work item changes that surface.
- Follow established conventions, best practices, testing, and architecture patterns from the project's `aspec/`.
- Do not edit `oldsrc/` (other than the README note).
- Do not delete `oldsrc/` — that is 0070.
- Do not introduce business logic in `src/frontend/`. If you find yourself wanting to, the missing surface is in Layer 2.
- Do not introduce upward calls. Use traits.
- The PR description MUST link to `aspec/architecture/2026-grand-architecture.md` and to this work item, MUST include the parity smoke-test checklist, and MUST list every developer-clarification question raised.
- After this work item lands, the next agent picks up `0070-grand-architecture-finalize-and-remove-oldsrc.md`.
