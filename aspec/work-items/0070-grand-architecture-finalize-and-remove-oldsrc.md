# Work Item: Task

Title: grand architecture refactor — part 5/5 — final parity validation, oldsrc removal, docs and aspec refresh
Issue: n/a — fifth and final work item implementing `aspec/architecture/2026-grand-architecture.md`

## Required reading before starting

This work item closes out the grand architecture refactor described in `aspec/architecture/2026-grand-architecture.md`. The implementing agent **MUST** read that document, the previous four work items (`0066-…` through `0069-…`), and the resulting `src/` tree before writing any code.

This work item has no architectural ambiguity — Layers 0 through 4 are in place and the user-facing binary already ships from `src/`. The remaining work is verification, deletion, and documentation. The implementing agent should still ASK THE DEVELOPER if any unexpected gap is discovered during validation rather than paper over it.

The companion work items are:

- `0066-grand-architecture-foundation-and-layer-0-data.md` (must be merged)
- `0067-grand-architecture-layer-1-engines.md` (must be merged)
- `0068-grand-architecture-layer-2-command-and-dispatch.md` (must be merged)
- `0069-grand-architecture-layer-3-frontends-and-binary.md` (must be merged)

## Summary:

- **Build a fresh integration and end-to-end test suite from scratch** under `tests/` (and `benches/` if relevant), designed against the new four-layer architecture. The legacy `tests/` directory is deleted along with `oldsrc/`; nothing is ported by default. This work item OWNS every cross-layer integration test, every real-Docker / real-git / real-network test, every parity test against pre-refactor user-visible behavior, and every binary-level smoke test.
- Run the resulting suite as a comprehensive parity validation pass: every CLI command, every TUI flow, every headless API endpoint must behave identically (or better) than the pre-refactor binary. Capture the results in a checked-in `aspec/review-notes/0070-parity-validation.md`.
- Audit the `src/` tree against every tenet of the grand architecture document and produce a checked-in report. Any tenet violation must be fixed in this work item.
- Delete `oldsrc/` in its entirety. Delete the legacy `tests/` and `benches/` trees in their entirety. Remove any stragglers in `Cargo.toml`, `Makefile`, `.gitignore`, `aspec/`, `docs/`, `scripts/`, and CI configuration that reference the legacy tree.
- Refresh `docs/` to reflect the new architecture (comprehensive docs, not per-work-item). Refresh affected `aspec/` files.
- Refresh `aspec/uxui/cli.md` to be the projection of `CommandCatalogue` (or to match it byte-for-byte if the projection is generated automatically).
- Add a `make architecture-lint` target (and a corresponding CI job) that mechanically enforces the layering tenet — Layer 0 imports nothing above; Layer 1 imports only Layer 0; Layer 2 imports only Layers 0/1; Layer 3 imports only Layers 0/1/2; Layer 4 imports any layer. Use a small Rust tool, a `cargo-deny` check, or a shell script over `grep` — ASK THE DEVELOPER which they prefer.

## User Stories

### User Story 1:
As a: maintainer

I want to:
have `oldsrc/` deleted and the new architecture be the only source of truth

So I can:
trust that no one accidentally edits or copies from legacy code, and CI no longer has to compile, lint, or carry around 50k+ lines of frozen reference code.

### User Story 2:
As a: future implementing agent or contributor

I want to:
read up-to-date `docs/` and `aspec/` that describe the four-layer architecture, with no lingering references to the pre-refactor structure

So I can:
ramp up on the codebase quickly and not be misled by stale instructions.

### User Story 3:
As a: maintainer adding a new feature six months from now

I want to:
have a `make architecture-lint` check that fails CI if a new edit accidentally introduces an upward import (e.g. Layer 1 reaching into Layer 3)

So I can:
catch tenet violations at PR time rather than during review.

## Implementation Details:

### 0. Required reading and ground rules

- Read `aspec/architecture/2026-grand-architecture.md` end-to-end.
- Read all four prior work items.
- Read the entire `src/` tree.
- For reference only (and only briefly, since it is about to be deleted): `oldsrc/` exists for one last comparison pass. Do not edit it. Do not extend its lifetime.
- When uncertain, ASK THE DEVELOPER.

### 1. Build the new `tests/` tree from scratch

Work items 0066–0069 deliberately produced **only colocated unit tests**. This work item is where every cross-layer integration test, every real-Docker / real-git / real-network end-to-end test, every binary-level smoke test, and every parity test against the pre-refactor binary is written. Build the new `tests/` directory from scratch.

**Do not port files from the pre-refactor `tests/` directory.** Those tests target the legacy command entry points, untyped flags, and frontend-conflated business logic. Carrying them forward defeats the refactor's purpose. The narrow exception is a single test file or fixture that satisfies all three of:

1. Asserts a precise wire-format or on-disk invariant the new architecture must preserve (e.g. headless API SSE chunk format, persisted workflow-state JSON shape, `.amux.json` schema).
2. Compiles unchanged or with mechanical edits against the new types.
3. Adds coverage no new test in this work item already provides.

If any old test is brought forward, the PR description MUST list it with a one-sentence justification.

#### 1a. Proposed `tests/` layout

```
tests/
  data_layer/                      # Layer 0 cross-module integration
    config_session_roundtrip.rs
    sqlite_upgrade_compat.rs       # opens a fixture DB written by the prior amux release
  engine/                          # Layer 1 — real-system tests
    container_docker.rs            # real Docker daemon required
    container_apple.rs             # real Apple containers required (cfg(target_os = "macos"))
    workflow_end_to_end.rs         # real Docker, three-step workflow
    git_engine.rs                  # real `git init` worktree create/merge/remove cycle
    overlay_engine.rs              # real filesystem with canonicalization edge cases
    auth_engine_tls.rs             # real rustls cert generation, fingerprint stability
  command/                         # Layer 2 against real Layers 0+1
    dispatch_real_engines.rs       # Dispatch::run_command end-to-end for init/ready/status/single-step implement
  cli_parity/                      # Layer 3 CLI parity vs. pre-refactor (or vs. documented behavior)
    help_text.rs                   # golden-file: amux help, amux <sub> --help for every level
    init.rs
    ready.rs
    implement.rs
    chat.rs
    exec_prompt.rs
    exec_workflow.rs
    claws.rs
    status.rs
    specs.rs
    config.rs
    headless.rs
    remote.rs
    new.rs
    json_outputs.rs                # every --json command's JSON shape against checked-in fixtures
  tui_parity/                      # Layer 3 TUI parity (vt100/expect-style harness)
    startup_and_tabs.rs
    command_box.rs
    workflow_dialog.rs
    yolo_countdown.rs
    keyboard_shortcuts.rs          # every documented shortcut
    rendering_snapshots.rs
  headless_parity/                 # Layer 3 headless API
    routes.rs                      # one test per route × method
    auth_modes.rs
    tls.rs
    sse_wire_format.rs
    websocket_wire_format.rs
  binary_smoke/                    # Layer 4 — invokes the real `amux` binary
    cli_subprocess.rs              # std::process::Command against the built binary
    tui_subprocess.rs              # spawn under a pty, drive a small recorded session
    headless_subprocess.rs         # spawn the server, curl every endpoint, kill cleanly
  fixtures/
    sqlite_upgrade/<version>.db    # captured from prior releases
    cli_help/<command>.txt         # golden help text
    headless_openapi.json          # frozen schema for compatibility checks
    workflow_state/v1.json         # persisted-state shape
  helpers/
    docker_skip.rs                 # gate tests with a real-Docker check; skip on CI without it
    test_repo.rs                   # build a synthetic git repo for engine + command tests
    test_session.rs                # build a Session backed by a tempdir + temp HOME
    recording_frontend.rs          # the same fakes used in colocated unit tests, available to integration tests
```

The exact layout MAY differ — ASK THE DEVELOPER before the file plan ossifies — but the *coverage* must include every category above.

#### 1b. What each tier covers

- **`tests/data_layer/`** — Layer 0 multi-module exercises that don't fit as colocated unit tests. Always hermetic (`tempfile`, no network). Includes the sqlite-upgrade compatibility fixture so users upgrading across the refactor do not lose data.
- **`tests/engine/`** — Layer 1 against real systems. Real Docker, real `git`, real filesystem canonicalization, real rustls. Gated behind feature flags / `helpers::docker_skip` so the suite runs cleanly on minimal CI.
- **`tests/command/`** — Layer 2 wired into real Layers 0 + 1 (no fakes). Asserts that the typed-object refactor of dispatch + commands continues to produce correct end-to-end behavior when the engines are real.
- **`tests/cli_parity/`** — for every command and subcommand in `aspec/uxui/cli.md`, exercise the new binary as a subprocess and assert stdout/stderr/exit-code match a checked-in golden fixture. Each fixture is captured from the pre-refactor binary on a known-clean repo state, then frozen. Help text fixtures cover `amux <command> --help` at every depth.
- **`tests/tui_parity/`** — drive the new TUI under a `vt100`-style terminal harness (e.g. the `vt100` crate, or `expectrl`). For every documented keyboard shortcut, every dialog, every yolo countdown behavior, capture a rendered-screen snapshot and assert against a checked-in fixture. (Snapshot tests must be deterministic — no wall-clock leakage. Drive time with `tokio::time::pause` where the TUI uses tokio timers, or stub the clock at the engine level.)
- **`tests/headless_parity/`** — start the new headless server bound to an ephemeral loopback port; issue real `reqwest` calls; assert wire compatibility with checked-in fixtures (frozen OpenAPI, frozen SSE chunk shapes). Cover every auth mode and every TLS configuration.
- **`tests/binary_smoke/`** — exercise the real `amux` binary as a subprocess. Confirms `cargo build --release` produces a binary that links and runs end-to-end. Catches anything missed by integration tests that link against the library.

#### 1c. Real-system gating

Every test that needs Docker, Apple containers, a working `git`, or network access MUST be gated by a `helpers::docker_skip!` (or analogous) macro that skips with a clear message on environments lacking the dependency. CI runs the full suite on Linux + macOS runners that have Docker; minimal local environments (`make test-fast`) skip the real-system tests by default.

Add `make test-full` (runs everything) and `make test-fast` (skips real-system tests). Update CI to run `make test-full` on at least one runner per supported OS.

### 2. Comprehensive parity validation

With the new test suite in place, produce `aspec/review-notes/0070-parity-validation.md` capturing the results.

#### 2a. CLI parity

- Run `tests/cli_parity/` against the new binary; capture pass/fail per command.
- For any drift, classify as MINOR-DRIFT (justify, freeze new fixture, get developer sign-off) or REGRESSION (block).
- Manually run `amux help`, `amux <subcommand> --help`, `amux <subcommand> <subsubcommand> --help` for every level and spot-check the rendered output.

#### 2b. TUI parity

- Run `tests/tui_parity/` and capture pass/fail per scenario.
- Additionally, the implementing agent MUST launch the new TUI on a real terminal and walk through the documented user flows:
  - Launch → tab list visible → status bar correct.
  - Open multiple tabs (every tab-open shortcut). Switch between them. Close them.
  - Run `implement` from the command box; complete a single-step workflow; observe the workflow control dialog; choose advance, pause, abort.
  - Run a multi-step workflow with `--yolo` and observe the auto-advance countdown.
  - Trigger an error path (e.g. a missing work item) and confirm the error rendering is identical or improved.
  - Resize the terminal during execution; confirm dynamic tab widths and PTY resize work.
  - Exercise every documented keyboard shortcut at least once.
- Capture screenshots or terminal recordings for the report.

#### 2c. Headless parity

- Run `tests/headless_parity/` and capture pass/fail per endpoint.
- Manually spot-check: start the headless server with default flags; confirm bind, TLS, auth banner are identical to pre-refactor.
- Manually issue a representative request to every documented endpoint with a real `curl` invocation; record any drift.

#### 2d. Sign-off rule

The work item cannot proceed to step 4 (deletion) until every parity entry is PASS or has an explicit, developer-approved MINOR-DRIFT justification. REGRESSIONs block the PR.

### 3. Architectural tenet audit

Produce `aspec/review-notes/0070-architecture-audit.md` covering:

#### 3a. Layering — no upward calls

- For each Rust file in `src/`, confirm the file's imports respect the layering rule:
  - `src/data/**`: imports from `std`, third-party crates, and `crate::data::*` only.
  - `src/engine/**`: imports from above plus `crate::data::*`.
  - `src/command/**`: imports from above plus `crate::engine::*`.
  - `src/frontend/**`: imports from above plus `crate::command::*`.
  - `src/main.rs`: any.
- Implement this as a `make architecture-lint` rule — see step 5.
- Any violation found must be fixed in this work item.

#### 3b. No business logic in frontends

- Walk every file in `src/frontend/`. Flag any `if`, `match`, or computed default whose decision affects *behavior* rather than *presentation*. Move flagged logic into Layer 2.
- Common false positives (acceptable): branching on `OutcomeKind` to choose how to *render* the outcome, branching on terminal capabilities (TTY vs not), branching on rendering width.
- Common true positives (must move): default-value computation for a flag that wasn't supplied; choosing an agent if the user didn't specify one; computing a workflow step's container options.

#### 3c. Typed objects over `pub fn`

- Walk every `pub fn` in `src/`. Flag any that is stateful, takes more than one or two simple inputs, or could be expressed as a method on an existing struct. Convert flagged ones to methods. Document any exception in the audit report.

#### 3d. Catalogue completeness

- Confirm `CommandCatalogue::root()` covers every documented command. Confirm `CommandCatalogue::flag_iter()` covers every documented flag. Re-run the consistency tests from work item 0068.

### 4. Delete `oldsrc/` and the legacy `tests/` + `benches/`

Once §2 (parity) and §3 (audit) are PASS, perform the deletions in a single atomic commit:

- `git rm -r oldsrc/`
- `git rm -r` any pre-refactor test files in `tests/` that have been superseded by §1's freshly built tree (the directory itself stays — it now contains only the new tree from §1).
- `git rm -r` any pre-refactor `benches/` files; if `benches/` is no longer needed, delete the directory entirely.

Sweep for any remaining references:

- `Cargo.toml` — confirm no `path = "oldsrc/…"` remains; remove the `amux-next` `[[bin]]` entry; confirm `[[bin]] name = "amux"` points at `src/main.rs`.
- `Makefile` — confirm no `oldsrc` reference remains; `make all`, `make install`, `make test`, `make test-fast`, `make test-full` all work.
- `.gitignore`, `.github/workflows/*.yml`, `scripts/*.sh`, `Dockerfile.dev` — search for `oldsrc` and `amux-next` and remove any straggler.
- `aspec/`, `docs/`, `README.md`, `CLAUDE.md` — same search.
- `tests/` — confirm every file in the directory compiles against `src/` only; no `oldsrc` imports anywhere.

Confirm:

```
$ rg -i 'oldsrc|amux-next' -l --hidden -g '!target' -g '!.git'
```

returns only documentation files in `aspec/architecture/2026-grand-architecture.md`, `aspec/work-items/006[6-9]-*.md`, `aspec/work-items/0070-*.md`, and `aspec/review-notes/0070-*.md`.

### 5. `make architecture-lint`

Add a Make target that mechanically enforces layering. Two acceptable implementations:

1. A small Rust binary in `tools/architecture-lint/` that uses `cargo metadata` + `syn` to walk every module and confirm import direction. Preferred; survives renames.
2. A shell script using `rg` patterns. Acceptable for v1.

The target must:

- Run in CI (`.github/workflows/test.yml`).
- Print every violation with file path + line + offending import.
- Exit non-zero on any violation.
- Take well under 10 seconds on a clean tree (so it can be run on every commit pre-push).

Add a corresponding `make pre-push` umbrella that runs `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, and `make architecture-lint`. Update the contributor docs.

### 6. Refresh `docs/`

The grand architecture document is the source of truth, but `docs/` is the user-facing site. Update:

- `docs/` overview pages to describe the four-layer architecture in user-friendly terms.
- Any "how amux works internally" page to point at `src/data/`, `src/engine/`, `src/command/`, `src/frontend/`.
- Removal of any references to `src/runtime/`, `src/tui/`, `src/commands/`, etc. that pointed at the pre-refactor layout.
- `docs/releases/<next-version>.md`: a changelog entry summarizing the refactor and any migration notes (there should be no user-visible migration; if there is, ASK THE DEVELOPER why).
- `docs/blog/` if a maintainer wants a write-up of the refactor (optional, ASK THE DEVELOPER).

### 7. Refresh `aspec/`

- `aspec/foundation.md`: keep the project mission unchanged; add a single sentence noting the four-layer architecture if it isn't already implied.
- `aspec/architecture/design.md`: replace any pre-refactor architecture description with a pointer to `aspec/architecture/2026-grand-architecture.md` and a one-paragraph summary. The grand architecture document is the canonical reference going forward.
- `aspec/architecture/security.md`: confirm every constraint still holds; nothing in this refactor was supposed to weaken security.
- `aspec/uxui/cli.md`: regenerate from `CommandCatalogue` (preferred) or audit by hand. The aim is byte-for-byte agreement between `aspec/uxui/cli.md` and the catalogue going forward.
- `aspec/devops/localdev.md`, `aspec/devops/cicd.md`, `aspec/devops/operations.md`, `aspec/devops/subagents.md`: update any path or module reference that no longer matches the new tree.
- `aspec/work-items/0000-template.md`: leave unchanged unless the developer requests an update.

### 8. Final sanity pass

- `cargo build --release` produces a single statically-linked `amux`.
- `cargo test` passes (entire new suite, including all `tests/*` from §1).
- `make test-full` passes on a runner with Docker available.
- `make test-fast` passes on a runner without Docker (skips real-system tests with clear messaging).
- `cargo clippy --all-targets -- -D warnings` passes.
- `make architecture-lint` passes.
- `make all`, `make install`, `make test` work.
- `git status` is clean. The repository is ready to release.

### 9. What must NOT happen in this work item

- No new features.
- No new flags.
- No new commands.
- No user-visible behavior change. If a parity check turns up something that "feels worse" but is technically equivalent, leave it alone unless the developer says otherwise.
- No leaving any `oldsrc` reference behind.

## Edge Case Considerations:

- **Architecture-lint on third-party crate paths**: the lint should ignore imports from `std::*` and external crates; only inspect intra-crate paths under `crate::*`.
- **`#[cfg(test)]` test modules**: tests under `src/data/` may reasonably want to use a tiny test helper from another layer. Allow `#[cfg(test)]`-gated upward imports only if the developer explicitly approves the carve-out; default is to forbid them and add the helper to the same layer.
- **Workspace splits**: if the Cargo layout in 0066 chose a workspace, deleting `oldsrc/` may also mean deleting an entire workspace member. Confirm `Cargo.toml` reflects the final shape.
- **Existing user data**: users who upgrade across the refactor must not lose any data. The `SqliteSessionStore` schema must remain readable; any persisted workflow state must continue to load. This was supposed to be guaranteed in 0066 — confirm it once more here, with a real database from a prior install if the developer can supply one.
- **Release notes**: the next release after this lands should call out the architecture refactor at a high level for users (the CLI behavior is unchanged but the internal structure has changed dramatically). ASK THE DEVELOPER for the desired tone.
- **CI flake risk**: deleting 50k+ lines and adding a new lint at the same time can mask flakes. Run the full CI suite at least twice on this PR before merging.
- **Coverage drop**: if any line of `oldsrc` had a test that produced unique coverage, the deletion of `oldsrc` will reduce overall coverage. The new tree's tests should already cover the equivalent behavior; confirm by running coverage before and after on the parity test suite.

## Test Considerations:

### Test philosophy (read first)

This work item is the **only** point in the refactor that adds tests to the top-level `tests/` directory (and, if needed, `benches/`). 0066–0069 produced colocated unit tests only. Here, the entire integration / end-to-end / parity / binary-smoke / wire-format suite is built from scratch — see step 1 above for the proposed layout.

**Do not port tests from the pre-refactor `tests/` or `benches/`.** Those tests assume legacy command surfaces, untyped flags, frontend-conflated business logic, and ad-hoc filesystem helpers. They are deleted in step 4 along with `oldsrc/`. The narrow exception is a single fixture or test that satisfies all three of:

1. Asserts a precise wire-format or on-disk invariant (SSE chunk shape, persisted state JSON, `.amux.json` schema, sqlite migration compatibility) the new architecture must preserve byte-for-byte.
2. Compiles unchanged or with mechanical edits against the new types.
3. Adds coverage that no freshly written test in this work item already provides.

If any old test or fixture is brought forward, the PR description MUST list it with a one-sentence justification.

### Tests added in this work item

- The complete `tests/` tree as detailed in step 1 — `tests/data_layer/`, `tests/engine/`, `tests/command/`, `tests/cli_parity/`, `tests/tui_parity/`, `tests/headless_parity/`, `tests/binary_smoke/`, plus `tests/fixtures/` and `tests/helpers/`.
- `tools/architecture-lint/` unit tests (against synthetic source trees verifying upward imports are rejected and same-or-lower imports are accepted), if the tool is implemented as a Rust binary.
- A repo-level guard (test or shell check) that fails if any file outside the documented allowlist mentions `oldsrc` or `amux-next`.

### Tests preserved from 0066–0069

All colocated `#[cfg(test)] mod tests` blocks added in 0066–0069 remain in place and continue to pass. This work item adds the cross-layer / real-system tests; it does not touch the unit tests that already exist alongside the source.

### Build & CI

- `make test-fast` (skips real-system tests) runs in under a minute on a warm cache.
- `make test-full` runs the full suite on at least one CI runner per supported OS that has Docker.
- `make architecture-lint` runs in CI on every PR.
- `make pre-push` (`fmt --check` + `clippy -D warnings` + `cargo test` + `architecture-lint`) is documented and runs locally in under 2 minutes on a warm cache.
- Release build still produces a single static binary for macOS, Linux, and Windows.

### Manual smoke test

- The implementing agent MUST install the new binary on a real machine and run a representative session: `amux init`, `amux ready`, open the TUI, run an `implement` workflow, exit.
- The implementing agent MUST start `amux headless start`, issue real `curl` calls to a representative endpoint set, and stop the server cleanly.

## Codebase Integration:

- Follow `aspec/architecture/2026-grand-architecture.md` as the source of truth.
- Follow `aspec/uxui/cli.md` after it is regenerated from the catalogue.
- Follow established conventions, best practices, testing, and architecture patterns from the project's `aspec/`.
- Do not edit anything inside `oldsrc/` before deleting it; do not partially delete it.
- Do not introduce upward calls or new free `pub fn` for stateful concerns. Fix any leftover violations from prior work items as part of the audit.
- The PR description MUST link to `aspec/architecture/2026-grand-architecture.md` and to this work item, MUST include the parity report, the architecture audit report, and a confirmation that `oldsrc/` is gone, and MUST list any developer-clarification questions raised.
- After this work item lands, the grand architecture refactor described in `aspec/architecture/2026-grand-architecture.md` is complete. amux is ready for the next decade.
