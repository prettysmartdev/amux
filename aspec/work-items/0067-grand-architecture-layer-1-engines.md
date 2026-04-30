# Work Item: Task

Title: grand architecture refactor — part 2/5 — Layer 1 engines (Container, Workflow, Git, Overlay, Auth)
Issue: n/a — second of five work items implementing `aspec/architecture/2026-grand-architecture.md`

## Required reading before starting

This work item is the second of five executing the grand architecture refactor described in `aspec/architecture/2026-grand-architecture.md`. The implementing agent **MUST** read that document, the previous work item `0066-grand-architecture-foundation-and-layer-0-data.md`, and the current state of `src/data/` before writing any code.

The four tenets that govern this work item:

1. Layer 1 (engine) consumes Layer 0 (data) only. It MUST NOT call into Layer 2 (command), Layer 3 (frontend), or Layer 4 (binary). When the engines need user input or output, they accept a frontend trait *defined by Layer 1* — higher layers implement it.
2. Frontends contain no business logic. This affects Layer 1 because every engine's API surface must be expressed in a way that a frontend can satisfy by implementing a small trait, never by routing back through engine code.
3. Typed objects over `pub fn`. Builder/factory patterns over `run_X_with_Y(...)` mega-functions. The grand architecture document gives the canonical worked example — `ContainerRuntime::new_with_options(vec<options>) -> ContainerInstance` then `ContainerInstance::run_with_frontend(some_frontend_trait)` — and explicitly forbids the legacy `run_container_with_*` style.
4. When uncertain, ASK THE DEVELOPER. Do not write ambiguous "you could try this or this" code.

The companion work items are:

- `0066-grand-architecture-foundation-and-layer-0-data.md` (must be merged before starting this)
- `0068-grand-architecture-layer-2-command-and-dispatch.md`
- `0069-grand-architecture-layer-3-frontends-and-binary.md`
- `0070-grand-architecture-finalize-and-remove-oldsrc.md`

## Summary:

- Build out `src/engine/` with five engine modules: `container/`, `workflow/`, `git/`, `overlay/`, `auth/`. Each is a typed object (or small set of typed objects) that owns its concern entirely.
- The `ContainerRuntime` is rewritten from scratch as a builder/factory: a small number of typed `ContainerOption` values feed `ContainerRuntime::build(...) -> ContainerInstance`, and `ContainerInstance::run_with_frontend(impl ContainerFrontend) -> ContainerExecution` is the only way to execute a container. The legacy `run_container_with_*` and `run_with_sink` style is forbidden.
- A new `ContainerExecution` type is introduced. It represents a "fully prepared, ready-to-run container handle" that Layer 2 can hand to `WorkflowEngine` without leaking the underlying frontend or runtime details.
- The `WorkflowEngine` is rewritten to hold all state, advancement logic, yolo/auto countdowns, agent/model resolution, exit-code handling, and step persistence. It accepts a frontend trait at construction (e.g. `WorkflowFrontend` exposing `user_choose_next_action`, `confirm_resume`, `report_step_status`, etc.) and is forbidden from rendering anything itself or making any direct user-input syscalls.
- The `GitEngine` consolidates every git operation amux performs (root resolution, dirty detection, worktree CRUD, merge, commit, future push/pull). The data layer's `GitRootResolver` trait is now satisfied by `GitEngine`.
- The `OverlayEngine` consolidates overlay construction and management — agent settings/config passthrough, user-defined directory overlays, env-var overlays, secret overlays, skill overlays. It consumes Layer 0's `OverlayPathResolver`.
- The `AuthEngine` consolidates host-side agent credential resolution and headless-server authentication. It consumes Layer 0's `AuthPathResolver` and `SqliteSessionStore`.
- All engines have unit tests. `ContainerRuntime` and `WorkflowEngine` have additional integration tests using lightweight fakes that satisfy their frontend traits.

## User Stories

### User Story 1:
As a: future implementing agent picking up Layer 2

I want to:
find Layer 1 engines that expose builder/factory APIs and accept frontend traits

So I can:
wire commands by composing typed engine objects without ever needing to touch container, git, or workflow internals.

### User Story 2:
As a: maintainer reading `src/engine/container/`

I want to:
see a small number of `ContainerOption` variants and a single `ContainerRuntime::build` rather than a dozen `run_container_with_*` functions with overlapping parameter lists

So I can:
trust that adding a new container option is a small, local change rather than a sprawling refactor across every call site.

### User Story 3:
As a: maintainer reading `src/engine/workflow/`

I want to:
see all workflow execution logic, exit-code handling, yolo countdowns, and agent/model resolution in one place

So I can:
fix workflow bugs without sifting through TUI, CLI, and headless code paths that today re-implement parts of the same logic.

## Implementation Details:

### 0. Required reading and ground rules

- Read `aspec/architecture/2026-grand-architecture.md` end-to-end.
- Read `0066-grand-architecture-foundation-and-layer-0-data.md` and the resulting `src/data/` to understand the types Layer 1 consumes.
- For reference only (not to be edited or copied verbatim): `oldsrc/runtime/`, `oldsrc/workflow/`, `oldsrc/git.rs`, `oldsrc/overlays/`, `oldsrc/passthrough.rs`, and the auth bits in `oldsrc/commands/headless/auth.rs`. Use these to understand existing behavior; **do not** port the existing API surface verbatim, since the grand architecture explicitly mandates a redesign.
- When uncertain, ASK THE DEVELOPER.

### 1. `src/engine/container/` — `ContainerRuntime`, `ContainerInstance`, `ContainerExecution`

#### 1a. Types

```rust
// src/engine/container/options.rs
pub enum ContainerOption {
    Image(ImageRef),
    Entrypoint(Entrypoint),
    Overlay(OverlaySpec),
    EnvPassthrough(EnvVar),
    SeededPrompt(String),
    Interactive(bool),
    AllowDocker(bool),
    MountSsh(bool),
    Yolo(YoloMode),
    Auto(AutoMode),
    Plan(PlanMode),
    WorkingDir(PathBuf),
    Name(ContainerName),
    Cpu(CpuLimit),
    Memory(MemoryLimit),
    AgentSettingsPassthrough(AgentSettings),
    // ...exhaustive list — every flag the legacy code spreads across
    // run_container_with_* parameters becomes one variant here
}
```

The variant set MUST cover *every* knob the legacy `oldsrc/runtime/{docker,apple,mod}.rs` exposes, plus anything new the grand architecture calls out (e.g. `AgentSettingsPassthrough`).

```rust
// src/engine/container/runtime.rs
pub struct ContainerRuntime { /* dispatcher between Docker and Apple */ }

impl ContainerRuntime {
    pub fn detect(global_config: &GlobalConfig) -> Result<Self, EngineError>;
    pub fn build(&self, options: impl IntoIterator<Item = ContainerOption>)
        -> Result<ContainerInstance, EngineError>;
    pub fn list_running(&self, session: &Session) -> Result<Vec<ContainerHandle>, EngineError>;
    pub fn stats(&self, handle: &ContainerHandle) -> Result<ContainerStats, EngineError>;
    pub fn stop(&self, handle: &ContainerHandle) -> Result<(), EngineError>;
}
```

```rust
// src/engine/container/instance.rs
pub trait ContainerInstance: Send + Sync {
    fn id(&self) -> &ContainerId;
    fn name(&self) -> &ContainerName;
    fn image(&self) -> &ImageRef;
    fn run_with_frontend(self: Box<Self>, frontend: Box<dyn ContainerFrontend>)
        -> Result<ContainerExecution, EngineError>;
}
```

```rust
// src/engine/container/execution.rs
pub struct ContainerExecution {
    // Owns the running container handle, the wired-up frontend, and exit-code futures.
    // Cannot be cloned. Cannot be inspected for frontend details by Layer 2 callers.
}

impl ContainerExecution {
    pub async fn wait(self) -> Result<ContainerExitInfo, EngineError>;
    pub fn handle(&self) -> &ContainerHandle;
    pub fn cancel(&self) -> Result<(), EngineError>;
}
```

```rust
// src/engine/container/frontend.rs — defined by Layer 1, implemented by Layer 3
pub trait ContainerFrontend: Send + Sync {
    fn write_stdout(&mut self, bytes: &[u8]) -> Result<(), EngineError>;
    fn write_stderr(&mut self, bytes: &[u8]) -> Result<(), EngineError>;
    fn read_stdin(&mut self, buf: &mut [u8]) -> Result<usize, EngineError>;   // 0 = EOF
    fn report_status(&mut self, status: ContainerStatus);
    fn report_progress(&mut self, progress: ContainerProgress);  // image pulls, build steps
    fn resize_pty(&mut self, cols: u16, rows: u16);
    // etc — must cover everything a TUI pty, CLI stdin/stdout binding, and a headless
    // SSE/WebSocket binding need. Define this trait once; implementations live in 0069.
}
```

The `Docker` and `Apple` variants of the runtime live in `src/engine/container/docker.rs` and `src/engine/container/apple.rs`. They share the `ContainerInstance` trait. They MUST NOT be referenced by name from outside `src/engine/container/`; consumers always go through `ContainerRuntime::build`.

#### 1b. What is forbidden in this module

- No `pub fn run_container_with_*`. Every previous "run with X" use case becomes one or more `ContainerOption` variants plus a frontend trait method.
- No direct PTY allocation. PTYs are a Layer 3 (frontend) concern; Layer 1 hands raw stdin/stdout bytes to the frontend trait and lets the frontend decide whether they go through a PTY (TUI), straight to fds (CLI), or over a socket (headless).
- No printing to stdout/stderr. All output goes through `ContainerFrontend::write_stdout`/`write_stderr`.
- No `tracing::info!` or similar to the user-facing console. Engine logs go to a `tracing` subscriber that the binary configures; they do not bypass the frontend.

### 2. `src/engine/workflow/` — `WorkflowEngine`

The legacy `oldsrc/workflow/mod.rs` (944 lines) and `oldsrc/workflow/parser.rs` (841 lines) and `oldsrc/workflow/dag.rs` (231 lines) collectively own workflow execution today, but workflow logic also leaks into `oldsrc/commands/implement.rs` (2087 lines), `oldsrc/commands/exec.rs`, and `oldsrc/tui/state.rs`. All of that logic consolidates here.

```rust
// src/engine/workflow/mod.rs
pub struct WorkflowEngine {
    workflow: Workflow,                // parsed workflow definition (Layer 0 data type)
    state: WorkflowState,              // persistable state (Layer 0 data type)
    state_store: WorkflowStateStore,   // Layer 0 — persists state on each step
    frontend: Box<dyn WorkflowFrontend>,
    container_factory: Box<dyn ContainerExecutionFactory>,  // see below
    git_engine: Arc<GitEngine>,
    overlay_engine: Arc<OverlayEngine>,
}

impl WorkflowEngine {
    pub fn new(
        session: &Session,
        workflow: Workflow,
        frontend: Box<dyn WorkflowFrontend>,
        container_factory: Box<dyn ContainerExecutionFactory>,
        git_engine: Arc<GitEngine>,
        overlay_engine: Arc<OverlayEngine>,
    ) -> Result<Self, EngineError>;

    pub async fn run_to_completion(&mut self) -> Result<WorkflowOutcome, EngineError>;
    pub async fn step_once(&mut self) -> Result<StepOutcome, EngineError>;
    pub async fn pause(&mut self) -> Result<(), EngineError>;
    pub async fn resume(&mut self) -> Result<(), EngineError>;
    pub fn state(&self) -> &WorkflowState;
}
```

The `ContainerExecutionFactory` trait is the mechanism the grand architecture document calls out: Layer 2 builds a factory that, when invoked by the engine, returns a `ContainerExecution` for a given step. The engine never sees raw `ContainerOption` lists or frontend implementations; it only consumes already-prepared executions.

```rust
pub trait ContainerExecutionFactory: Send + Sync {
    fn execution_for_step(
        &self,
        step: &WorkflowStep,
        session: &Session,
        runtime: &WorkflowRuntimeContext,
    ) -> Result<ContainerExecution, EngineError>;
}
```

The `WorkflowFrontend` trait covers every user-input concern the engine needs:

```rust
pub trait WorkflowFrontend: Send + Sync {
    fn user_choose_next_action(
        &mut self,
        state: &WorkflowState,
    ) -> Result<NextAction, EngineError>;     // workflow control dialog

    fn confirm_resume(&mut self, mismatch: &ResumeMismatch) -> Result<bool, EngineError>;
    fn report_step_status(&mut self, status: StepStatus);
    fn report_step_output(&mut self, step: &WorkflowStep, output: StepOutput);
    fn yolo_countdown_tick(&mut self, remaining: Duration) -> Result<(), EngineError>;
    fn report_workflow_completed(&mut self, outcome: &WorkflowOutcome);
    // ...exhaustively cover every prompt or report the legacy code performs
}
```

Workflow parsing (markdown, YAML, TOML — already supported per work item 0056) belongs to Layer 0 (`src/data/workflow_definition.rs` — created here if not already in 0066; ASK THE DEVELOPER if uncertain whether parsing belongs at Layer 0 or in `src/engine/workflow/parser.rs`. The grand architecture document is silent on this exact split; the strongest argument for Layer 0 is that parsed `Workflow` is a serializable data type, and parsers are filesystem concerns. The strongest argument for Layer 1 is that DAG validation is engine logic. **Decide with the developer.**)

#### What moves into `WorkflowEngine`

- Yolo-mode auto-advance (countdown timing + advance-on-stuck logic) — currently in `oldsrc/tui/state.rs` and `oldsrc/commands/implement.rs`.
- Agent and model resolution per step — currently scattered across `oldsrc/commands/implement.rs` and `oldsrc/commands/exec.rs`.
- Exit-code interpretation — currently in `oldsrc/commands/implement.rs` and `oldsrc/commands/exec.rs`.
- Resume compatibility validation — currently `oldsrc/workflow/mod.rs::validate_resume_compatibility`.
- Step persistence — currently `oldsrc/workflow/mod.rs::save_workflow_state`.

#### What is forbidden in `WorkflowEngine`

- No direct container construction. Engines never call `ContainerRuntime::build`; they receive prepared `ContainerExecution` from a factory.
- No rendering, no `eprintln!`, no `tracing` to the user console. Status flows through `WorkflowFrontend::report_*`.
- No `clap` or `crossterm` use. Those are Layer 3 concerns.
- No knowledge of which frontend (CLI vs TUI vs headless) is on the other side of the trait. The engine treats all three identically.

### 3. `src/engine/git/` — `GitEngine`

Consolidates every git operation amux performs. Replaces the free `pub fn`s in `oldsrc/git.rs`.

```rust
pub struct GitEngine { /* probably stateless, but a struct enforces typed access */ }

impl GitEngine {
    pub fn new() -> Self;
    pub fn version_check(&self) -> Result<GitVersion, EngineError>;
    pub fn resolve_root(&self, working_dir: &Path) -> Result<PathBuf, EngineError>;
    pub fn is_clean(&self, path: &Path) -> Result<bool, EngineError>;
    pub fn uncommitted_files(&self, path: &Path) -> Result<Vec<PathBuf>, EngineError>;
    pub fn worktree_path(&self, git_root: &Path, work_item: u32) -> Result<PathBuf, EngineError>;
    pub fn worktree_path_named(&self, git_root: &Path, name: &str) -> Result<PathBuf, EngineError>;
    pub fn create_worktree(&self, git_root: &Path, worktree: &Path, branch: &str) -> Result<(), EngineError>;
    pub fn remove_worktree(&self, git_root: &Path, worktree: &Path) -> Result<(), EngineError>;
    pub fn merge_branch(&self, git_root: &Path, branch: &str) -> Result<(), EngineError>;
    pub fn commit_all(&self, path: &Path, message: &str) -> Result<(), EngineError>;
    pub fn delete_branch(&self, git_root: &Path, branch: &str) -> Result<(), EngineError>;
    pub fn branch_exists(&self, git_root: &Path, branch: &str) -> bool;
    pub fn is_detached_head(&self, git_root: &Path) -> bool;
}
```

`GitEngine` implements Layer 0's `GitRootResolver` trait (introduced in 0066) so `Session::open` can use it. Provide an explicit `impl GitRootResolver for GitEngine` in `src/engine/git/`.

### 4. `src/engine/overlay/` — `OverlayEngine`

Consolidates overlay construction and management. Replaces `oldsrc/overlays/` and the agent-settings-passthrough bits of `oldsrc/passthrough.rs`.

```rust
pub struct OverlayEngine {
    path_resolver: OverlayPathResolver,   // Layer 0
    auth_resolver: AuthPathResolver,      // Layer 0
}

impl OverlayEngine {
    pub fn new(session: &Session) -> Result<Self, EngineError>;
    pub fn build_overlays(
        &self,
        session: &Session,
        request: &OverlayRequest,
    ) -> Result<Vec<OverlaySpec>, EngineError>;
    pub fn resolve_user_overlay(&self, spec: &str) -> Result<DirectoryOverlay, EngineError>;
    pub fn agent_settings_overlays(&self, agent: &AgentName) -> Result<Vec<OverlaySpec>, EngineError>;
}
```

`OverlayRequest` describes "I want overlays for command X with these flags"; `build_overlays` returns the resolved set, deduplicated and canonicalized. Layer 2 hands the result into `ContainerOption::Overlay` variants.

Auth-credential overlays for agents (Claude config, Codex config, OpenCode config, Crush config, etc. — currently sprinkled through `oldsrc/passthrough.rs`) move here. They are constructed via `OverlayEngine::agent_settings_overlays(agent)`.

### 5. `src/engine/auth/` — `AuthEngine`

Consolidates two distinct concerns the legacy code conflates:

- Resolving host-side agent credentials (read host paths to mount-as-overlays). This delegates to `OverlayEngine` for the overlay construction; `AuthEngine` only enumerates which credentials exist and are available.
- Headless server authentication (API key generation, hashing, comparison, persistence, refresh). This replaces `oldsrc/commands/headless/auth.rs`.

```rust
pub struct AuthEngine {
    auth_paths: AuthPathResolver,     // Layer 0
    headless_paths: HeadlessPaths,    // Layer 0
}

impl AuthEngine {
    pub fn new(session: &Session) -> Self;

    // Agent credential discovery
    pub fn list_agent_credentials(&self, agent: &AgentName) -> Result<AgentCredentialStatus, EngineError>;

    // Headless API-key lifecycle
    pub fn generate_api_key(&self) -> Result<ApiKey, EngineError>;
    pub fn write_api_key_hash(&self, hash: &ApiKeyHash) -> Result<(), EngineError>;
    pub fn read_api_key_hash(&self) -> Result<Option<ApiKeyHash>, EngineError>;
    pub fn verify_api_key(&self, presented: &ApiKey) -> Result<AuthOutcome, EngineError>;
    pub fn refresh_api_key(&self) -> Result<ApiKey, EngineError>;

    // TLS material (post-0065 feature)
    pub fn ensure_self_signed_tls(&self, bind_ip: IpAddr) -> Result<TlsMaterial, EngineError>;
    pub fn load_tls_from_paths(&self, cert: &Path, key: &Path) -> Result<TlsMaterial, EngineError>;
}
```

All cryptographic comparisons MUST use `subtle::ConstantTimeEq` exactly as `aspec/architecture/security.md` requires.

### 6. Errors

`src/engine/error.rs` defines `EngineError` covering every failure mode in Layer 1. It wraps `DataError` for failures bubbling up from Layer 0. Higher layers wrap `EngineError` in their own error types; Layer 1 does not depend on higher-layer errors.

### 7. What must NOT happen in this work item

- No changes to `oldsrc/`. The user-visible `amux` binary continues to ship from `oldsrc/`.
- No work in `src/command/` or `src/frontend/` beyond ensuring they compile as empty modules.
- No `pub fn run_container_with_*` style APIs. Hard-fail any review that introduces them.
- No PTY/crossterm code in `src/engine/`. PTYs are Layer 3.
- No `clap` references in `src/engine/`. Clap is Layer 4 / Layer 3 (CLI).
- No "just do it like the legacy code did" decisions. If the grand architecture's tenets disagree with the legacy approach, follow the tenets and ASK THE DEVELOPER if the cost looks high.

## Edge Case Considerations:

- **Apple containers vs Docker dispatching**: `ContainerRuntime::detect` must return the same runtime backend for the lifetime of a `Session`. If the user runs Docker in one tab and Apple in another, the global config field that selects the backend is per-process; ASK THE DEVELOPER whether the backend is selectable per-session (suggests `ContainerRuntime` belongs to `Session`) or process-wide (suggests it lives in a process-global). The grand architecture document is silent.
- **Container lifetime exceeding `ContainerExecution`**: today some commands intentionally leave a container running (e.g. headless background mode). The `ContainerExecution::wait` API forces a join; provide an alternative `ContainerExecution::detach() -> ContainerHandle` that hands ownership of the running container back to the caller without joining.
- **Workflow resume across amux versions**: `WorkflowState` is persisted by Layer 0, but the *interpretation* of state lives in `WorkflowEngine`. The engine must reject (with a structured error, not a panic) any workflow state whose `schema_version` is newer than the engine understands.
- **Yolo countdown precision**: today the countdown uses wallclock; prefer `tokio::time::Instant` (monotonic) so suspending the process or system clock skew does not accelerate or skip the countdown. ASK THE DEVELOPER if they prefer wallclock for any user-facing reason.
- **`OverlayEngine` deduplication keys**: today's dedup uses canonicalized paths. Re-use `OverlayPathResolver::canonicalize` (Layer 0) — do not re-implement.
- **`AuthEngine::verify_api_key` timing**: every comparison MUST be constant-time even when no hash exists on disk (compare against a fixed-length sentinel). This avoids leaking "is the server running with auth disabled" via timing.
- **`GitEngine::resolve_root` failure on a directory that *is* a git root**: `git rev-parse --show-toplevel` already returns the input dir if it is itself a git root; cover this in a unit test.
- **`ContainerFrontend::read_stdin` blocking semantics**: define explicitly whether `read_stdin` may block, and how cancellation works. The frontend trait MUST be usable from both async (TUI, headless) and sync (CLI) contexts. ASK THE DEVELOPER whether to make the trait `async_trait` or to keep it sync with `tokio::task::spawn_blocking` adapters at frontend implementation sites.
- **PTY size changes mid-execution**: `ContainerFrontend::resize_pty` is called by Layer 3; the engine forwards to the underlying Docker/Apple resize syscall. Cover with an integration test that resizes mid-stream and confirms the container sees the new size.

## Test Considerations:

### Test philosophy (read first)

Tests for Layer 1 are **designed and written from scratch** alongside the new engines. **Do not port tests from `oldsrc/tests/*` or from `oldsrc/runtime/**/#[cfg(test)] mod tests`, `oldsrc/workflow/**`, `oldsrc/git.rs`, `oldsrc/overlays/**`, or `oldsrc/passthrough.rs` test blocks.** Those tests assume the legacy `run_container_with_*` API surface, the legacy workflow/CLI flow that conflated business logic with frontend output, and the legacy free-function helpers. Carrying them forward defeats the refactor's purpose.

The narrow exception is a test that satisfies **all** of the following:

1. Asserts a precise behavioral invariant the new engine MUST preserve (e.g. exit-code semantics, container name format, branch-naming convention, overlay dedup rules, constant-time auth verification).
2. Compiles unchanged or with mechanical edits against the new engine surfaces.
3. Exercises only Layer 0 + Layer 1 — no upward calls, no legacy-runtime types.

If any old test is brought forward under this exception, the PR description MUST list it with a one-sentence justification. The default answer is "rewrite from scratch."

This work item produces **only Layer 1 unit tests** using fakes that satisfy the engine-defined frontend traits. **No real Docker, no real network, no real PTY, no real HTTP, and no end-to-end multi-engine scenarios** in this work item. Those are 0070's responsibility, against a freshly rebuilt `tests/` directory.

### Unit tests (colocated `#[cfg(test)] mod tests`)

All tests use either fully synthetic inputs or hermetic temp-directories. Container tests use a `FakeContainerInstance` that the test module owns, satisfying `ContainerInstance` by recording calls without invoking Docker.

- **`ContainerRuntime`**:
  - For each `ContainerOption` variant, a focused test asserts the option lands in the resulting `ContainerInstance`'s recorded config.
  - Conflicting options (e.g. `Yolo(true)` + `Auto(true)` if mutually exclusive) produce a structured `EngineError::ConflictingOptions` rather than a panic.
  - `ContainerRuntime::detect` chooses the right backend based on `GlobalConfig`.
- **`ContainerInstance` (via `FakeContainerInstance`)**:
  - `run_with_frontend` drives the recording frontend through the expected lifecycle (open → write_stdout chunks → status updates → exit).
  - PTY resize calls forwarded through `ContainerFrontend::resize_pty`.
- **`ContainerExecution`**:
  - `wait` returns a structured `ContainerExitInfo` that includes exit code, signal (if applicable), and start/end timestamps.
  - `cancel` on an already-finished execution is a no-op (does not panic).
  - `detach` transfers ownership of the handle without joining.
- **`WorkflowEngine`** (against a `FakeContainerExecutionFactory` and `FakeWorkflowFrontend`):
  - `step_once` advances exactly one step and persists state via the injected `WorkflowStateStore` (Layer 0).
  - `run_to_completion` runs every step when the frontend returns `NextAction::Advance`.
  - `pause` then `resume` (with no schema drift) returns to the same step.
  - Resume against a workflow whose persisted hash differs invokes `confirm_resume`; engine respects the return value.
  - Yolo mode invokes `WorkflowFrontend::yolo_countdown_tick` at the configured cadence under a `tokio::time::pause()` clock.
  - Exit-code interpretation: non-zero → `StepStatus::Failed`; zero → `Succeeded`; cancelled → `Cancelled`.
- **`GitEngine`**:
  - Each method runs against a per-test `tempfile::TempDir` with `git init`. These are *unit tests in form* (one method, one assertion) but use real `git` because git is the system under test.
  - `resolve_root` returns the input dir when the input *is* the root.
  - `create_worktree` then `remove_worktree` is idempotent against the same name.
  - `branch_exists` / `is_detached_head` against synthetic states.
- **`OverlayEngine::build_overlays`**:
  - Dedupes overlapping host paths after canonicalization.
  - `agent_settings_overlays` returns empty when no credentials exist on disk; emits the right overlay set when they do.
  - User-supplied overlay specs are validated and rejected with structured errors when malformed.
- **`AuthEngine`**:
  - `generate_api_key` → `write_api_key_hash` → `read_api_key_hash` → `verify_api_key` round-trip.
  - `verify_api_key` on a missing hash file is constant-time vs. `verify_api_key` on a present hash with a wrong key (use `criterion`'s `black_box` + a relaxed timing assertion, or simply assert that the code path performs a sentinel comparison rather than short-circuits).
  - `ensure_self_signed_tls` writes cert + key with `0o600` on Unix and produces a stable fingerprint on idempotent reruns within the validity window.

### What does NOT belong in this work item

- Real-Docker container startup, image pulls, network calls, or PTY interactions. These are 0070.
- Multi-engine scenarios that combine `WorkflowEngine` + real `ContainerRuntime` + real `GitEngine`. These are 0070.
- Any test in the top-level `tests/` directory. Leave `tests/` alone in this work item; 0070 rebuilds it.
- Parity tests against pre-refactor behavior of any kind.
- TUI, CLI, or headless surface tests — those layers don't exist yet.

### Build & CI

- `cargo build --bin amux` (still from `oldsrc/`) succeeds — the user-facing CLI is unchanged.
- `cargo build --bin amux-next` succeeds — Layer 0 + Layer 1 compile cleanly together.
- `cargo test` passes including the new engine unit tests.

### Manual smoke test

- Run the existing `amux` binary against a real repo. Confirm `amux ready`, `amux init`, `amux status`, `amux chat`, `amux implement`, etc. behave exactly as before. (Still legacy code; this work item does not change user-visible behavior.)

## Codebase Integration:

- Follow `aspec/architecture/2026-grand-architecture.md` as the source of truth.
- Follow established conventions, best practices, testing, and architecture patterns from the project's `aspec/`.
- Do not edit `oldsrc/`. Do not delete `oldsrc/`. Both are in 0070's scope.
- Do not introduce upward calls from Layer 1 to Layer 2/3/4. Use traits owned by Layer 1.
- Do not introduce free `pub fn` for stateful engine concerns. Prefer struct + methods.
- The PR description MUST link to `aspec/architecture/2026-grand-architecture.md` and to this work item, MUST list any developer-clarification questions raised and how they were resolved, and MUST explicitly call out any place a legacy `oldsrc` API was *not* preserved verbatim (with rationale).
- After this work item lands, the next agent picks up `0068-grand-architecture-layer-2-command-and-dispatch.md`.
