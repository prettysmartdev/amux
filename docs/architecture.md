# amux Architecture

## Overview

amux has two coexisting source trees:

- **`src/`** — the new five-layer architecture (in progress; Layer 0 complete). The `amux-next` binary is built from here.
- **`oldsrc/`** — the frozen pre-refactor source. The user-facing `amux` binary continues to build from here until the refactor is complete in work item 0070.

The `oldsrc/` tree is frozen: no edits are allowed. It will be deleted when `amux-next` reaches full parity in work item 0070. The rest of this document covers both trees: the new layered architecture first, then the legacy architecture that currently ships to users.

---

## Grand Architecture Refactor

### Purpose

amux grew into three execution modes (CLI, TUI, headless) that share the same core functionality but implement it separately, producing subtle behavioural drift and making parity across modes hard to guarantee. The grand architecture refactor replaces this with a strict five-layer system where every frontend is a thin presentation shell over a shared, tested core.

### Tenets

1. **No upward calls.** Lower layers never call functions or use types from higher layers. If a lower layer needs to delegate upward, it defines a trait that a higher layer implements.
2. **Frontends are dumb.** No frontend (CLI, TUI, headless) may implement business logic. All logic lives in Layer 2 (`command`) or below.
3. **Typed objects over free functions.** Every significant abstraction is a struct with methods. Free `pub fn` is acceptable only for stateless helpers, constructors, and one-off utilities.

### Layers

```
Layer 4: binary    main.rs — sets up frontends, delegates everything
Layer 3: frontend  CLI, TUI, Headless — input/output only
Layer 2: command   Dispatch, per-command business logic
Layer 1: engine    ContainerRuntime, WorkflowEngine, GitEngine, OverlayEngine, AuthEngine
Layer 0: data      Session, config, filesystem, database, typed data
```

**Layer 0 (data)** owns every data definition, config concern, filesystem access, and database interaction. No business logic, no container calls, no git operations, no workflow execution. See [Layer 0 reference](#layer-0-data-srcdata) below.

**Layer 1 (engine)** owns core runtime primitives: container lifecycle, workflow execution, git operations, overlay construction, and authentication logic. Implemented in work item 0067.

**Layer 2 (command)** owns higher-level business logic: the `Dispatch` type that routes input to typed command objects, and command-specific types (`ChatCommand`, `InitCommand`, etc.). Implemented in work item 0068.

**Layer 3 (frontend)** contains the CLI, TUI, and headless server. Each is a presentation layer only: it translates user input into `Dispatch` calls and renders command output. Implemented in work item 0069.

**Layer 4 (binary)** is `src/main.rs` — currently a stub. It becomes the real entrypoint in work item 0069.

### Current Status

| Layer | Location | Status |
|-------|----------|--------|
| 0 — data | `src/data/` | Complete (work item 0066) |
| 1 — engine | `src/engine/` | Stub — populated in 0067 |
| 2 — command | `src/command/` | Stub — populated in 0068 |
| 3 — frontend | `src/frontend/` | Stub — populated in 0069 |
| 4 — binary | `src/main.rs` | Stub — wired in 0069 |
| Legacy binary | `oldsrc/` | Frozen, ships to users |

---

## New Source Tree (`src/`)

```
src/
  main.rs                 Layer 4 stub (amux-next binary)
  lib.rs                  Re-exports the four layers
  data/                   Layer 0 — fully implemented
    mod.rs
    session.rs            Session, SessionState, SessionId, AgentName, …
    session_manager.rs    SessionManager, SessionStore, InMemorySessionStore
    error.rs              DataError
    config/
      mod.rs
      repo.rs             RepoConfig and related types
      global.rs           GlobalConfig
      env.rs              EnvSnapshot, Env, env var constants
      flags.rs            FlagConfig
      effective.rs        EffectiveConfig (merged view)
    fs/
      mod.rs
      headless_db.rs      SqliteSessionStore, SessionRecord, CommandRecord
      headless_paths.rs   HeadlessPaths
      workflow_state.rs   WorkflowStateStore
      skill_dirs.rs       SkillDirs
      workflow_dirs.rs    WorkflowDirs
      overlay_paths.rs    OverlayPathResolver
      auth_paths.rs       AuthPathResolver, AgentAuthPaths
  engine/
    mod.rs                (stub — populated in 0067)
  command/
    mod.rs                (stub — populated in 0068)
  frontend/
    mod.rs                (stub — populated in 0069)
```

---

## Layer 0: Data (`src/data/`)

Layer 0 is the foundation every other layer builds on. It owns:

- The `Session` ruling type and its runtime state
- The `SessionManager` collection and persistence interface
- All configuration loading, saving, and merging
- All filesystem and database interactions
- The typed `DataError` error enum

Nothing in `src/data/` ever spawns a process, opens a network socket, calls `git`, or manages a container. Those are Layer 1 concerns.

---

### Session (`src/data/session.rs`)

`Session` is the ruling type for every amux operation. It ties together a working directory, a resolved git root, loaded configurations, and the in-flight runtime state. Every command and workflow invocation starts with a `Session`.

- The **CLI** creates one `Session` per invocation.
- The **TUI** creates one `Session` per tab.
- The **headless server** creates one `Session` per API session.

#### `SessionId`

```rust
pub struct SessionId(Uuid);
```

Newtype over `uuid::Uuid`. Implements `Display` (UUID string format), `Hash`, and `Eq`. `SessionId::new()` generates a random v4 UUID; `SessionId::from_uuid(uuid)` wraps an existing one for persistence round-trips.

#### `AgentName`

```rust
pub struct AgentName(String);
```

Newtype over `String` with validation: ASCII alphanumerics, hyphens, and underscores; 1–64 characters. `AgentName::new("claude")` returns `Result<AgentName, DataError>`. `as_str()` and `Display` give the inner string.

#### `ContainerHandle`

```rust
pub struct ContainerHandle {
    pub id: String,
    pub image_tag: String,
    pub name: String,
    pub started_at: chrono::DateTime<chrono::Utc>,
}
```

The persistable identity of a running container. Layer 0 holds only the identity; the runtime object that controls a container (start/stop/wait) is Layer 1.

#### `SessionState`

```rust
pub struct SessionState {
    pub current_command: Option<CommandInvocation>,
    pub current_workflow: Option<WorkflowInvocation>,
    pub current_container: Option<ContainerHandle>,
    pub errors: Vec<SessionLogEntry>,
    pub notes: Vec<SessionLogEntry>,
}
```

Mutable runtime state owned by a `Session`. `record_error(msg)` and `record_note(kind, msg)` append to the respective logs. `SessionLogEntry` carries a UTC timestamp, a `SessionLogKind` (Info / Warning / Error / Diagnostic), and a message string.

#### `CommandInvocation` and `WorkflowInvocation`

`CommandInvocation` is the persistable record of a single in-flight command (subcommand name, args, status, exit code, timestamps). `WorkflowInvocation` is the persistable record of a running workflow (workflow name and hash, work item, step records, paused/yolo/auto flags, current step index).

Both are serializable via serde and stored in `SessionState` for persistence by the headless server.

#### `GitRootResolver` trait

```rust
pub trait GitRootResolver: Send + Sync {
    fn resolve(&self, working_dir: &Path) -> Result<PathBuf, DataError>;
}
```

Layer 0 never calls `git rev-parse` directly. Instead, `Session::open` accepts a `&dyn GitRootResolver` and delegates resolution to Layer 1's `GitEngine` (wired in work item 0067). `StaticGitRootResolver` is the test-only implementation that returns a fixed path.

#### `Session` constructors and accessors

```rust
impl Session {
    pub fn open(
        working_dir: PathBuf,
        resolver: &dyn GitRootResolver,
        opts: SessionOpenOptions,
    ) -> Result<Self, DataError>;

    pub fn open_at_git_root(
        working_dir: PathBuf,
        git_root: PathBuf,
        opts: SessionOpenOptions,
    ) -> Result<Self, DataError>;

    // Read accessors
    pub fn id(&self) -> SessionId;
    pub fn working_dir(&self) -> &Path;
    pub fn git_root(&self) -> &Path;
    pub fn repo_config(&self) -> &RepoConfig;
    pub fn global_config(&self) -> &GlobalConfig;
    pub fn env(&self) -> &EnvSnapshot;
    pub fn flags(&self) -> &FlagConfig;
    pub fn default_agent(&self) -> Option<&AgentName>;
    pub fn available_agents(&self) -> &[AgentName];
    pub fn state(&self) -> &SessionState;
    pub fn created_at(&self) -> SystemTime;
    pub fn last_active_at(&self) -> SystemTime;
    pub fn uptime(&self) -> Duration;

    // Mutation
    pub fn state_mut(&mut self) -> &mut SessionState;
    pub fn touch(&mut self);
    pub fn set_flags(&mut self, flags: FlagConfig);
    pub fn set_env(&mut self, env: EnvSnapshot);
    pub fn set_available_agents(&mut self, agents: Vec<AgentName>);

    // Merged config view
    pub fn effective_config(&self) -> EffectiveConfig;
}
```

`Session::open` resolves the git root via the resolver, loads `RepoConfig` and `GlobalConfig` from disk, resolves the default agent using the precedence order (flags > repo config > global config), and records creation timestamps. It returns `DataError::GitRootNotFound` if the resolver fails.

`SessionOpenOptions` carries optional `FlagConfig`, an optional `EnvSnapshot`, and an optional `Vec<AgentName>` for available agents.

---

### SessionManager (`src/data/session_manager.rs`)

```rust
pub struct SessionManager { … }
```

A concurrency-safe collection of `Session` values backed by a `tokio::sync::RwLock`. All methods are `async`.

```rust
impl SessionManager {
    pub fn in_memory() -> Self;
    pub fn with_persistence(store: Arc<dyn SessionStore>) -> Self;

    pub async fn create(&self, session: Session) -> Result<SessionId, DataError>;
    pub async fn get(&self, id: SessionId) -> Result<Session, DataError>;
    pub async fn update<F, T>(&self, id: SessionId, f: F) -> Result<T, DataError>
    where F: FnOnce(&mut Session) -> T;
    pub async fn list(&self) -> Vec<Session>;
    pub async fn len(&self) -> usize;
    pub async fn is_empty(&self) -> bool;
    pub async fn remove(&self, id: SessionId) -> Result<(), DataError>;
    pub fn has_persistence(&self) -> bool;
}
```

`SessionManager::in_memory()` creates a manager with no persistence backend (used by the CLI for its single session and by the TUI for per-tab sessions). `SessionManager::with_persistence(store)` attaches a `SessionStore` backend that receives an `upsert` call on every `create` or `update` and a `remove` call on every `remove`. The headless server uses this variant with `SqliteSessionStore`.

`update` takes a closure instead of returning `&mut Session` to avoid exposing an unguarded mutable reference across an `await` point.

`create` returns `DataError::SessionIdCollision` (instead of panicking) in the astronomically unlikely event of a UUID v4 collision.

#### `SessionStore` trait

```rust
pub trait SessionStore: Send + Sync {
    fn upsert(&self, session: &Session) -> Result<(), DataError>;
    fn remove(&self, id: SessionId) -> Result<(), DataError>;
}
```

The persistence interface implemented by Layer 0's `SqliteSessionStore` (and by `InMemorySessionStore` for tests).

---

### Configuration (`src/data/config/`)

All configuration concerns live in `src/data/config/`. Four source layers are merged in a fixed priority order:

```
flags  >  env  >  repo config  >  global config  >  built-in default
```

The merge is enforced by `EffectiveConfig` and is never duplicated elsewhere.

#### `RepoConfig` (`config/repo.rs`)

Per-repository configuration stored at `<git_root>/.amux/config.json`.

```rust
pub struct RepoConfig {
    pub agent: Option<String>,
    pub auto_agent_auth_accepted: Option<bool>,
    pub terminal_scrollback_lines: Option<usize>,
    pub yolo_disallowed_tools: Option<Vec<String>>,  // "yoloDisallowedTools" in JSON
    pub env_passthrough: Option<Vec<String>>,          // "envPassthrough" in JSON
    pub work_items: Option<WorkItemsConfig>,           // "workItems" in JSON
    pub overlays: Option<OverlaysConfig>,
    pub agent_stuck_timeout_secs: Option<u64>,        // "agentStuckTimeout" in JSON
}
```

Key methods:

| Method | Description |
|--------|-------------|
| `RepoConfig::path(git_root)` | Returns `<git_root>/.amux/config.json` |
| `RepoConfig::legacy_path(git_root)` | Returns `<git_root>/aspec/.amux.json` (pre-migration path) |
| `RepoConfig::load(git_root)` | Loads from disk; returns `default()` when absent, `DataError::ConfigParse` on malformed JSON |
| `RepoConfig::save(&self, git_root)` | Persists to disk, creating parent dirs as needed |
| `RepoConfig::migrate_legacy(git_root)` | Moves `aspec/.amux.json` → `.amux/config.json` if and only if legacy exists and new path does not; returns `true` when migration occurred |
| `RepoConfig::work_items_dir(git_root)` | Resolves configured work items directory |
| `RepoConfig::work_items_template(git_root)` | Resolves configured work item template path |

Nested types: `WorkItemsConfig` (dir, template), `OverlaysConfig` (directories list), `DirectoryOverlayConfig` (host, container, permission), `HeadlessConfig` (workDirs, alwaysNonInteractive), `RemoteConfig` (defaultAddr, savedDirs, defaultAPIKey).

#### `GlobalConfig` (`config/global.rs`)

Global configuration stored at `$HOME/.amux/config.json`. The path is overridden by the `AMUX_CONFIG_HOME` environment variable (used by tests to isolate state).

```rust
pub struct GlobalConfig {
    pub default_agent: Option<String>,
    pub terminal_scrollback_lines: Option<usize>,
    pub runtime: Option<String>,
    pub yolo_disallowed_tools: Option<Vec<String>>,
    pub env_passthrough: Option<Vec<String>>,
    pub headless: Option<HeadlessConfig>,
    pub remote: Option<RemoteConfig>,
    pub overlays: Option<OverlaysConfig>,
    pub agent_stuck_timeout_secs: Option<u64>,
}
```

Key methods:

| Method | Description |
|--------|-------------|
| `GlobalConfig::home_dir()` | Resolves `$AMUX_CONFIG_HOME` or `$HOME/.amux` |
| `GlobalConfig::home_dir_with(env)` | Same, reading from an `EnvSnapshot` |
| `GlobalConfig::path()` / `path_with(env)` | Resolves the full config file path |
| `GlobalConfig::load()` / `load_with(env)` | Loads from disk; returns `default()` when absent |
| `GlobalConfig::save()` / `save_with(env)` | Persists to disk |

#### `EnvSnapshot` and `Env` (`config/env.rs`)

`EnvSnapshot` is a frozen snapshot of every environment variable amux reads. No scattered `std::env::var()` calls appear elsewhere in Layer 0.

```rust
pub struct EnvSnapshot { … }

impl EnvSnapshot {
    pub fn empty() -> Self;
    pub fn with_overrides<I, K, V>(entries: I) -> Self;
    pub fn get(&self, key: &str) -> Option<&str>;

    // Typed accessors for known vars
    pub fn config_home(&self) -> Option<PathBuf>;    // AMUX_CONFIG_HOME
    pub fn headless_root(&self) -> Option<PathBuf>;  // AMUX_HEADLESS_ROOT
    pub fn overlays(&self) -> Option<&str>;          // AMUX_OVERLAYS
    pub fn remote_addr(&self) -> Option<&str>;       // AMUX_REMOTE_ADDR
    pub fn remote_session(&self) -> Option<&str>;    // AMUX_REMOTE_SESSION
    pub fn api_key(&self) -> Option<&str>;           // AMUX_API_KEY
}
```

`Env` is a stateless namespace used to read from the real process environment at startup. Tests use `EnvSnapshot::with_overrides([("AMUX_CONFIG_HOME", tmp_path)])` to avoid touching the filesystem.

Defined constants for every env var amux reads:

| Constant | Variable | Purpose |
|----------|----------|---------|
| `AMUX_CONFIG_HOME` | `AMUX_CONFIG_HOME` | Override global config home dir |
| `AMUX_HEADLESS_ROOT` | `AMUX_HEADLESS_ROOT` | Override headless storage root |
| `AMUX_OVERLAYS` | `AMUX_OVERLAYS` | Comma-separated overlay specs |
| `AMUX_REMOTE_ADDR` | `AMUX_REMOTE_ADDR` | Override remote server address |
| `AMUX_REMOTE_SESSION` | `AMUX_REMOTE_SESSION` | Sticky session id for remote ops |
| `AMUX_API_KEY` | `AMUX_API_KEY` | API key for headless server |

#### `FlagConfig` (`config/flags.rs`)

Typed struct carrying the flag values parsed by a frontend. Frontends (CLI via clap, TUI via the flag parser) populate a `FlagConfig` and pass it into `SessionOpenOptions`. The config layer itself never parses command-line strings.

Key fields: `agent`, `terminal_scrollback_lines`, `agent_stuck_timeout`, `non_interactive`, `env_passthrough`, `yolo_disallowed_tools`, `remote_addr`, `remote_session`, `api_key`.

#### `EffectiveConfig` (`config/effective.rs`)

The merged view of all four config sources. `Session::effective_config()` returns a fresh `EffectiveConfig` on demand; it is not cached on the session because flags can be updated via `Session::set_flags`.

```rust
pub struct EffectiveConfig {
    flags: FlagConfig,
    env: EnvSnapshot,
    repo: RepoConfig,
    global: GlobalConfig,
}

impl EffectiveConfig {
    pub fn new(flags, env, repo, global) -> Self;

    // Raw source access
    pub fn flags(&self) -> &FlagConfig;
    pub fn env(&self) -> &EnvSnapshot;
    pub fn repo(&self) -> &RepoConfig;
    pub fn global(&self) -> &GlobalConfig;

    // Merged accessors (precedence enforced internally)
    pub fn agent(&self) -> Option<String>;           // flag > repo > global
    pub fn env_passthrough(&self) -> Vec<String>;    // flag > repo > global > []
    pub fn yolo_disallowed_tools(&self) -> Vec<String>; // flag > repo > global > []
    pub fn scrollback_lines(&self) -> usize;         // flag > repo > global > 10_000
    pub fn agent_stuck_timeout(&self) -> Duration;   // flag > repo > global > 30s
    pub fn headless_work_dirs(&self) -> Vec<String>; // global only
    pub fn always_non_interactive(&self) -> bool;    // flag > global > false
    pub fn remote_default_addr(&self) -> Option<String>;  // flag > env > global
    pub fn remote_default_api_key(&self) -> Option<String>; // flag > env > global
    pub fn remote_saved_dirs(&self) -> Vec<String>;  // global only
    pub fn remote_session(&self) -> Option<String>;  // flag > env
    pub fn runtime(&self) -> Option<String>;         // global only
}
```

Built-in defaults: `scrollback_lines` = 10,000 lines; `agent_stuck_timeout` = 30 seconds.

---

### Filesystem Stores (`src/data/fs/`)

Every direct filesystem or database interaction in Layer 0 is encapsulated in a typed object in this module. Higher layers consume these objects; they never call `std::fs::*` or `rusqlite::*` directly.

#### `SqliteSessionStore` (`fs/headless_db.rs`)

Sqlite-backed persistence for headless-mode session and command metadata. Schema is compatible with `oldsrc/commands/headless/db.rs` so that existing on-disk databases written by earlier amux releases remain readable.

```rust
pub struct SqliteSessionStore { conn: Mutex<Connection> }

impl SqliteSessionStore {
    pub fn open(root: &Path) -> Result<Self, DataError>;
    pub fn open_from_paths(paths: &HeadlessPaths) -> Result<Self, DataError>;

    pub fn insert_session(&self, id, workdir, created_at) -> Result<(), DataError>;
    pub fn close_session(&self, id, closed_at) -> Result<(), DataError>;
    pub fn list_sessions(&self) -> Result<Vec<SessionRecord>, DataError>;
    pub fn get_session(&self, id) -> Result<Option<SessionRecord>, DataError>;

    pub fn insert_command(&self, id, session_id, subcommand, args, log_path) -> Result<(), DataError>;
    pub fn update_command_status(&self, id, status, exit_code, finished_at) -> Result<(), DataError>;
    pub fn list_commands(&self, session_id) -> Result<Vec<CommandRecord>, DataError>;
    pub fn get_command(&self, id) -> Result<Option<CommandRecord>, DataError>;
}
```

`SqliteSessionStore::open(root)` creates the database at `<root>/amux.db`, enables WAL mode, and runs schema migrations idempotently. The schema has two tables: `sessions` and `commands`.

`SessionRecord` and `CommandRecord` are plain structs (no Arc, no async) that carry the persisted metadata fields.

#### `HeadlessPaths` (`fs/headless_paths.rs`)

Typed accessors for every path used by the headless server. Replaces ad-hoc `dirs::data_dir().join("amux/headless/…")` calls scattered through the legacy code.

```rust
pub struct HeadlessPaths { root: PathBuf }

impl HeadlessPaths {
    pub fn from_env(env: &EnvSnapshot) -> Result<Self, DataError>;
    pub fn root(&self) -> &Path;
    pub fn db_path(&self) -> PathBuf;          // <root>/amux.db
    pub fn log_path(&self) -> PathBuf;         // <root>/amux.log
    pub fn pid_path(&self) -> PathBuf;         // <root>/amux.pid
    pub fn tls_dir(&self) -> PathBuf;          // <root>/tls/
    pub fn sessions_dir(&self) -> PathBuf;     // <root>/sessions/
    pub fn session_dir(&self, id) -> PathBuf;  // <root>/sessions/<id>/
    pub fn command_dir(&self, session_id, command_id) -> PathBuf;
    pub fn stdout_log(&self, session_id, command_id) -> PathBuf;
    pub fn stderr_log(&self, session_id, command_id) -> PathBuf;
}
```

`HeadlessPaths::from_env` reads `AMUX_HEADLESS_ROOT` from the snapshot; if unset, uses `$HOME/.amux/headless`.

#### `WorkflowStateStore` (`fs/workflow_state.rs`)

Persists and retrieves `WorkflowInvocation` to/from disk. Replaces the free `pub fn` helpers `workflow_state_path`, `save_workflow_state`, `load_workflow_state`, and `validate_resume_compatibility` in the legacy code.

```rust
pub struct WorkflowStateStore { base_dir: PathBuf }

impl WorkflowStateStore {
    pub fn new(base_dir: PathBuf) -> Self;
    pub fn for_session(session: &Session) -> Self;

    pub fn state_path(&self, workflow_name: &str) -> PathBuf;
    pub fn save(&self, invocation: &WorkflowInvocation) -> Result<(), DataError>;
    pub fn load(&self, workflow_name: &str) -> Result<Option<WorkflowInvocation>, DataError>;
    pub fn validate_resume(&self, invocation: &WorkflowInvocation) -> Result<(), DataError>;
    pub fn remove(&self, workflow_name: &str) -> Result<(), DataError>;
}
```

Workflow state is stored as JSON at `<base_dir>/workflow-state/<workflow_name>.json`. `validate_resume` checks that the workflow hash in the stored invocation matches the hash of the workflow file on disk, returning `DataError::WorkflowResumeIncompatible` if they differ.

#### `SkillDirs` (`fs/skill_dirs.rs`)

Typed access to global and per-repo skill directories.

```rust
pub struct SkillDirs {
    global_dir: Option<PathBuf>,
    repo_dir: Option<PathBuf>,
}

impl SkillDirs {
    pub fn resolve(session: &Session) -> Self;
    pub fn global_dir(&self) -> Option<&Path>;
    pub fn repo_dir(&self) -> Option<&Path>;
    pub fn all_dirs(&self) -> Vec<&Path>;
}
```

Global skills live at `$HOME/.amux/skills/` (or `$AMUX_CONFIG_HOME/skills/`). Per-repo skills live at `<git_root>/.amux/skills/`.

#### `WorkflowDirs` (`fs/workflow_dirs.rs`)

Typed access to global and per-repo workflow directories. Same structure as `SkillDirs`: global at `$HOME/.amux/workflows/`, per-repo at `<git_root>/.amux/workflows/`.

#### `OverlayPathResolver` (`fs/overlay_paths.rs`)

Resolves overlay host paths from raw user input. Path *mounting* into containers is Layer 1; path *resolution* is Layer 0.

```rust
pub struct OverlayPathResolver;

impl OverlayPathResolver {
    pub fn new() -> Self;
    pub fn expand_tilde(path: &str) -> PathBuf;
    pub fn make_absolute_with_cwd(path: &str, cwd: &Path) -> PathBuf;
    pub fn make_absolute(path: &str) -> PathBuf;
    pub fn canonicalize_lossy(path: &Path) -> PathBuf;
}
```

`canonicalize_lossy` handles the common case of overlay paths that don't exist yet: it walks up to the nearest existing ancestor, canonicalises that, and re-appends the missing trailing components. This mirrors the behaviour of `oldsrc/overlays/make_host_path_canonical` from work item 0065.

#### `AuthPathResolver` (`fs/auth_paths.rs`)

Resolves host-side credential and settings paths for each supported agent. The *passthrough* of those paths into containers (file copying, scrubbing, bind-mount construction) is a Layer 1 concern.

```rust
pub struct AuthPathResolver { home: PathBuf }

impl AuthPathResolver {
    pub fn at_home(home: impl Into<PathBuf>) -> Self;
    pub fn from_process_env() -> Result<Self, DataError>;
    pub fn home(&self) -> &Path;
    pub fn resolve(&self, agent: &str) -> AgentAuthPaths;
}

pub struct AgentAuthPaths {
    pub agent: String,
    pub config_file: Option<PathBuf>,
    pub settings_dir: Option<PathBuf>,
}
```

`resolve("claude")` returns `config_file = Some(~/.claude.json)`, `settings_dir = Some(~/.claude)`. Each supported agent maps to its own file locations.

---

### Error Types (`src/data/error.rs`)

All Layer 0 errors are variants of `DataError`. Higher layers wrap `DataError` in their own error enums.

```rust
#[derive(Debug, Error)]
pub enum DataError {
    GitRootNotFound { working_dir: PathBuf },
    GitRootResolution { working_dir: PathBuf, message: String },
    SessionNotFound { id: Uuid },
    SessionIdCollision { id: Uuid },
    InvalidAgentName { name: String, reason: String },
    ConfigParse { path: PathBuf, source: serde_json::Error },
    ConfigSerialize { source: serde_json::Error },
    Io { path: PathBuf, source: std::io::Error },
    HomeNotFound,
    Sqlite(rusqlite::Error),
    WorkflowState(String),
    WorkflowResumeIncompatible(String),
    InvalidPath { path: PathBuf, reason: String },
}
```

`DataError::io(path, err)` and `DataError::config_parse(path, err)` are convenience constructors. `DataError` uses `thiserror` for `Display` and `Error::source` implementations.

---

## Legacy Architecture (`oldsrc/`)

The following describes the user-facing `amux` binary, which continues to build from `oldsrc/` until work item 0070. The `oldsrc/` tree is frozen — no edits are allowed.

### High-level Overview

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

### Source Layout

```
oldsrc/
  main.rs                  Entry point: dispatch TUI or command mode
  lib.rs                   Re-exports public API for integration tests
  cli.rs                   clap CLI: Cli, Command, Agent enums
  config/
    mod.rs                 RepoConfig, GlobalConfig, HeadlessConfig, load/save helpers,
                           DEFAULT_SCROLLBACK_LINES, effective_scrollback_lines(),
                           effective_headless_work_dirs(), effective_always_non_interactive()
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
    new.rs                 `amux new` (work item creation) — run() + run_with_sink()
                           WorkItemKind, slugify, apply_template,
                           find_template, next_work_item_number
                           Auto-downloads aspec/ if template is missing
    new_cmd.rs             `amux new` top-level dispatcher (spec/workflow/skill)
                           Routes NewAction variants to the appropriate module;
                           new spec delegates to specs::run_new()
    new_workflow.rs        `amux new workflow` — run_new_workflow() + run_new_workflow_with_sink()
                           WorkflowInput, WorkflowStepInput, WorkflowFormat
                           validate_artefact_name(), resolve_workflow_dest()
                           write_workflow_file(), serialize_workflow() (TOML / YAML / Markdown)
                           skeleton_workflow() for --interview mode
                           workflow_interview_agent_entrypoint() + non-interactive variant
    new_skill.rs           `amux new skill` — run_new_skill() + run_new_skill_with_sink()
                           SkillInput, resolve_skill_dest()
                           render_skill_file(), render_skill_skeleton(), write_skill_file()
                           skill_interview_agent_entrypoint() + non-interactive variant
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
    exec.rs                `amux exec` — run_prompt(), run_workflow()
                           Thin dispatch layer: delegates to agent::run_agent_with_sink
                           (for prompt) and implement::run_workflow (for workflow);
                           agent_entrypoint_with_prompt helper
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
```

---

### The `OutputSink` Abstraction

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

### The `AgentRuntime` Abstraction

All container operations go through a single `AgentRuntime` trait defined in
`oldsrc/runtime/mod.rs`. This decouples the agent-launching logic from any
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

### Runtime implementations

| Struct | File | Notes |
|--------|------|-------|
| `DockerRuntime` | `oldsrc/runtime/docker.rs` | Wraps the `docker` CLI |
| `AppleContainersRuntime` | `oldsrc/runtime/apple.rs` | Wraps the `container` CLI; `#[cfg(target_os = "macos")]` |

---

### Working Directory Contract

All `run_with_sink` functions accept an explicit `cwd: &Path` parameter that
determines where the Git root is searched from. This ensures correctness for
both execution modes:

| Mode | `cwd` value | Behaviour |
|------|-------------|-----------|
| CLI (command mode) | `std::env::current_dir()` | Uses the directory where `amux` was launched |
| TUI (interactive mode) | `app.active_tab().cwd` | Uses the tab's working directory |

**Rule:** No command implementation may call `find_git_root()` (which reads the
process CWD). All callers must use `find_git_root_from(cwd)` with an explicitly
provided `cwd`.

---

### TUI State Machine

The TUI state is split across three orthogonal enums plus the `App` struct:

#### `Focus`

```
CommandBox  ←──── Esc ────── ExecutionWindow
    │                                ▲
    └─────── ↑ arrow / running ──────┘
```

#### `ExecutionPhase`

```
Idle ──[Submit]──► Running ──[exit 0]──► Done
                      │
                      └──[exit ≠ 0]──► Error
```

#### `Dialog`

```
None ──[q / Ctrl+C]──────────────────────────► QuitConfirm      ──[y]──► quit
     ──[ready|implement|chat, cwd ≠ root]──► MountScope        ──[r/c]──► resume
     ──[new]───────────────────────────────► NewKindSelect      ──[1/2/3]──► NewTitleInput ──[Enter]──► create
     ──[init, --aspec + aspec/ exists]─────► InitReplaceAspec   ──[y/n]─┐
     ──[init, all other cases]────────────────────────────────────────►  InitAuditConfirm ──[y/n]──► InitWorkItemsSetup ──[y/n]──► launch_init()
```

---

### CLI/TUI Flag Unification

`spec.rs` is the leaf module that all three sites import from. It defines every
flag for every subcommand as static data:

```rust
pub struct FlagSpec {
    pub name: &'static str,
    pub takes_value: bool,
    pub value_name: &'static str,
    pub hint: &'static str,
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

`parse_flags(parts, spec)` in `tui/flag_parser.rs` replaces all ad-hoc `parse_*_flags()` functions and drives both TUI parsing and autocomplete from the same `CommandSpec`.

### Agent override resolution order

1. **Flag** — `--agent <name>` passed on the command line (CLI or TUI)
2. **Repo config** — `agent` field in `.amux/config.json`
3. **Global config** — `default_agent` field in `~/.amux/config.json`
4. **Built-in default** — `claude`

---

### Ready Command

The `ready` command has two modes based on the `--refresh` flag:

**Without `--refresh`** (default): check runtime, Dockerfile.dev, and images; print summary.

**With `--refresh`**: check runtime → launch agent to audit Dockerfile.dev → rebuild images → print summary.

All business logic for `ready` lives in `oldsrc/commands/ready.rs`. The TUI and CLI call the same engine functions; the only difference is how user input is collected and how the audit container is executed.

---

### Init Command

All business logic lives in `oldsrc/commands/init_flow.rs`, called identically from the CLI (`init.rs`) and TUI adapters. The two differ only in `InitQa` (stdin vs. pre-collected TUI answers) and `InitContainerLauncher` (synchronous vs. background task).

---

### Docker Build Streaming

`docker::build_image_streaming()` spawns `docker build` and reads stdout and stderr concurrently in separate background threads, forwarding lines through a shared `mpsc` channel to the `on_line` callback as they arrive.

---

### PTY Architecture

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

---

### Container Window

```
Hidden ──[start_container()]──► Maximized ──[Esc]──► Minimized ──['c']──► Maximized
                                     │                    │
                                     └────[finish]────────┘──► Hidden + Summary bar
```

When maximized, the container window covers 95% of the outer execution window. When minimized, a 1-line green-bordered bar shows the agent name and live stats.

Container stats are polled every 5 seconds via a tokio task that calls `docker stats --no-stream`.

---

### Host Settings Injection

`HostSettings` encapsulates the preparation and lifetime of the sanitized agent configuration mounted into every container:

```
~/.claude.json   ──sanitize──► temp/claude.json      (oauthAccount removed,
~/.claude/       ──filter──►   temp/dot-claude/        /workspace trust added,
                                   settings.json        LSP suppression applied)
```

The denylist excludes `projects/`, `sessions/`, `history.jsonl`, `telemetry/`, and similar host-only artefacts.

---

### Agent Auth Flow

```
ready/implement/chat submitted
        │
        ▼
   read_keychain_raw() → extract OAuth JSON → CLAUDE_CODE_OAUTH_TOKEN env var
```

Credentials are sourced from the macOS system keychain and passed as an environment variable — never mounted as files.

---

### Performance Characteristics

**Render loop:** `terminal.draw()` is called unconditionally on every loop iteration (~60 Hz). Ratatui double-buffering means terminal I/O is proportional to changed cells, not screen size.

**Output buffer:** `TabState` holds an `output_lines: Vec<String>`. A 10,000-line cap (configurable) applies to the vt100 container parser. The outer text buffer is bounded by a VecDeque cap (see work item 0035).

**Docker interaction:** all Docker operations spawn a new `std::process::Command` child. Stats are polled every 5 seconds per active container.

**Scalability target:** 20 concurrent tabs.

---

### Headless Mode

The headless server runs as a third execution mode alongside command mode and the TUI.

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
                                     └── tokio::spawn ──► commands::run() dispatch
                                                               │
                                                               ▼
                                                         Docker container
                                                         stdout/stderr → log files
                                                         status → db::update_command()
```

`AppState` holds the allowlist, a `Mutex<Connection>`, and a per-session mutex map. The `AMUX_HEADLESS_ROOT` env var overrides the storage root for test isolation.

Background daemonization: systemd-run on Linux, launchd plist on macOS, double-fork fallback elsewhere.

---

### Testing Strategy

| Layer | Location | What is tested |
|-------|----------|----------------|
| Layer 0 unit | `src/data/**/#[cfg(test)]` | Session, SessionManager, all config types, all fs stores |
| Unit — per module | `oldsrc/**/#[cfg(test)]` | Individual functions, data structures |
| Unit — border colors | `oldsrc/tui::state::tests` | All 6 combinations of phase × focus |
| Unit — PTY data | `oldsrc/tui::state::tests` | `\r`/`\n`/`\r\n` processing, live-line updates |
| Unit — container window | `oldsrc/tui::state::tests` | Container state transitions, PTY routing, summary generation |
| Unit — CLI/spec parity | `oldsrc/cli::tests` | Every clap flag for each subcommand is present in `spec::*_FLAGS` and vice versa |
| Unit — flag parser | `oldsrc/tui::flag_parser::tests` | `parse_flags()` with every flag in both forms |
| Unit — init flow | `oldsrc/commands::init_flow::tests` | Each stage via mock InitQa + InitContainerLauncher |
| Unit — headless db | `oldsrc/commands::headless::db::tests` | Schema creation, session/command CRUD |
| Integration — CLI | `tests/cli_integration.rs` | Binary-level: help, version, flags, work items |
| Integration — parity | `tests/command_tui_parity.rs` | Shared logic between command/TUI modes |
| Integration — headless HTTP | `oldsrc/commands::headless::server::tests` | Full session + command lifecycle |
| End-to-end — headless | `tests/headless_integration.rs` | `amux headless start` subprocess; HTTP requests via reqwest |

---

[← Headless Mode](08-headless-mode.md) · [Contents](contents.md)
