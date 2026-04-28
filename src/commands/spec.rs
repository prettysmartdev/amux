/// A single flag accepted by an amux subcommand.
pub struct FlagSpec {
    /// Long flag name without leading `--` (e.g. `"agent"`).
    pub name: &'static str,
    /// Whether the flag takes a value argument (e.g. `--agent NAME` vs `--non-interactive`).
    pub takes_value: bool,
    /// Metavar shown in autocomplete hints (e.g. `"NAME"`, `"FILE"`). Empty for boolean flags.
    pub value_name: &'static str,
    /// Short description for autocomplete display.
    pub hint: &'static str,
}

/// The full flag set for a single amux subcommand.
pub struct CommandSpec {
    pub name: &'static str,
    pub flags: &'static [FlagSpec],
}

pub static INIT_FLAGS: &[FlagSpec] = &[
    FlagSpec { name: "agent", takes_value: true,  value_name: "NAME", hint: "agent to install (claude, codex, opencode, maki, gemini, copilot, crush, cline)" },
    FlagSpec { name: "aspec", takes_value: false, value_name: "",     hint: "download aspec templates to the current project" },
];

pub static READY_FLAGS: &[FlagSpec] = &[
    FlagSpec { name: "refresh",         takes_value: false, value_name: "", hint: "run the Dockerfile agent audit" },
    FlagSpec { name: "build",           takes_value: false, value_name: "", hint: "force rebuild the dev container image" },
    FlagSpec { name: "no-cache",        takes_value: false, value_name: "", hint: "pass --no-cache to docker build" },
    FlagSpec { name: "non-interactive", takes_value: false, value_name: "", hint: "run without interactive prompt" },
    FlagSpec { name: "allow-docker",    takes_value: false, value_name: "", hint: "allow Docker access" },
    FlagSpec { name: "json",            takes_value: false, value_name: "", hint: "output structured JSON (implies --non-interactive)" },
];

pub static IMPLEMENT_FLAGS: &[FlagSpec] = &[
    FlagSpec { name: "agent",           takes_value: true,  value_name: "NAME", hint: "override configured agent" },
    FlagSpec { name: "model",           takes_value: true,  value_name: "NAME", hint: "override agent model (e.g. claude-opus-4-6)" },
    FlagSpec { name: "non-interactive", takes_value: false, value_name: "",     hint: "run without interactive prompt" },
    FlagSpec { name: "plan",            takes_value: false, value_name: "",     hint: "plan mode" },
    FlagSpec { name: "allow-docker",    takes_value: false, value_name: "",     hint: "allow Docker access" },
    FlagSpec { name: "workflow",        takes_value: true,  value_name: "FILE", hint: "workflow file path" },
    FlagSpec { name: "worktree",        takes_value: false, value_name: "",     hint: "use git worktree" },
    FlagSpec { name: "mount-ssh",       takes_value: false, value_name: "",     hint: "mount SSH agent" },
    FlagSpec { name: "yolo",            takes_value: false, value_name: "",     hint: "skip confirmation prompts" },
    FlagSpec { name: "auto",            takes_value: false, value_name: "",     hint: "auto mode" },
];

pub static CHAT_FLAGS: &[FlagSpec] = &[
    FlagSpec { name: "agent",           takes_value: true,  value_name: "NAME", hint: "override configured agent" },
    FlagSpec { name: "model",           takes_value: true,  value_name: "NAME", hint: "override agent model (e.g. claude-opus-4-6)" },
    FlagSpec { name: "non-interactive", takes_value: false, value_name: "",     hint: "run without interactive prompt" },
    FlagSpec { name: "plan",            takes_value: false, value_name: "",     hint: "plan mode" },
    FlagSpec { name: "allow-docker",    takes_value: false, value_name: "",     hint: "allow Docker access" },
    FlagSpec { name: "mount-ssh",       takes_value: false, value_name: "",     hint: "mount SSH agent" },
    FlagSpec { name: "yolo",            takes_value: false, value_name: "",     hint: "skip confirmation prompts" },
    FlagSpec { name: "auto",            takes_value: false, value_name: "",     hint: "auto mode" },
];

pub static STATUS_FLAGS: &[FlagSpec] = &[
    FlagSpec { name: "watch", takes_value: false, value_name: "", hint: "continuously refresh every 3 seconds" },
];

pub static SPECS_NEW_FLAGS: &[FlagSpec] = &[
    FlagSpec { name: "interview", takes_value: false, value_name: "", hint: "use interview mode" },
];

pub static SPECS_AMEND_FLAGS: &[FlagSpec] = &[
    FlagSpec { name: "non-interactive", takes_value: false, value_name: "", hint: "run without interactive prompt" },
    FlagSpec { name: "allow-docker",    takes_value: false, value_name: "", hint: "allow Docker access" },
];

pub static EXEC_PROMPT_FLAGS: &[FlagSpec] = &[
    FlagSpec { name: "non-interactive", takes_value: false, value_name: "",     hint: "run without interactive prompt" },
    FlagSpec { name: "plan",            takes_value: false, value_name: "",     hint: "plan mode" },
    FlagSpec { name: "allow-docker",    takes_value: false, value_name: "",     hint: "allow Docker access" },
    FlagSpec { name: "mount-ssh",       takes_value: false, value_name: "",     hint: "mount SSH agent" },
    FlagSpec { name: "yolo",            takes_value: false, value_name: "",     hint: "skip confirmation prompts" },
    FlagSpec { name: "auto",            takes_value: false, value_name: "",     hint: "auto mode" },
    FlagSpec { name: "agent",           takes_value: true,  value_name: "NAME", hint: "override configured agent" },
    FlagSpec { name: "model",           takes_value: true,  value_name: "NAME", hint: "override agent model (e.g. claude-opus-4-6)" },
];

pub static EXEC_WORKFLOW_FLAGS: &[FlagSpec] = &[
    FlagSpec { name: "work-item",       takes_value: true,  value_name: "NUM",  hint: "optional work item number" },
    FlagSpec { name: "non-interactive", takes_value: false, value_name: "",     hint: "run without interactive prompt" },
    FlagSpec { name: "plan",            takes_value: false, value_name: "",     hint: "plan mode" },
    FlagSpec { name: "allow-docker",    takes_value: false, value_name: "",     hint: "allow Docker access" },
    FlagSpec { name: "worktree",        takes_value: false, value_name: "",     hint: "use git worktree" },
    FlagSpec { name: "mount-ssh",       takes_value: false, value_name: "",     hint: "mount SSH agent" },
    FlagSpec { name: "yolo",            takes_value: false, value_name: "",     hint: "skip confirmation prompts" },
    FlagSpec { name: "auto",            takes_value: false, value_name: "",     hint: "auto mode" },
    FlagSpec { name: "agent",           takes_value: true,  value_name: "NAME", hint: "override configured agent" },
    FlagSpec { name: "model",           takes_value: true,  value_name: "NAME", hint: "override agent model (e.g. claude-opus-4-6)" },
];

pub static CONFIG_SET_FLAGS: &[FlagSpec] = &[
    FlagSpec { name: "global", takes_value: false, value_name: "", hint: "write to global config instead of repo config" },
];

pub static HEADLESS_START_FLAGS: &[FlagSpec] = &[
    FlagSpec { name: "port",                  takes_value: true,  value_name: "PORT", hint: "port to listen on (default 9876)" },
    FlagSpec { name: "workdirs",              takes_value: true,  value_name: "DIR",  hint: "allowlisted working directory (repeatable)" },
    FlagSpec { name: "background",            takes_value: false, value_name: "",     hint: "daemonize via OS process manager" },
    FlagSpec { name: "refresh-key",           takes_value: false, value_name: "",     hint: "regenerate the API key" },
    FlagSpec { name: "dangerously-skip-auth", takes_value: false, value_name: "",     hint: "disable authentication for this execution" },
];

pub static REMOTE_RUN_FLAGS: &[FlagSpec] = &[
    FlagSpec { name: "remote-addr", takes_value: true,  value_name: "URL", hint: "remote headless amux host address" },
    FlagSpec { name: "session",     takes_value: true,  value_name: "ID",  hint: "session ID on the remote host" },
    FlagSpec { name: "follow",      takes_value: false, value_name: "",    hint: "stream logs until command completes" },
    FlagSpec { name: "api-key",     takes_value: true,  value_name: "KEY", hint: "API key for the remote host" },
];

pub static REMOTE_SESSION_START_FLAGS: &[FlagSpec] = &[
    FlagSpec { name: "remote-addr", takes_value: true,  value_name: "URL", hint: "remote headless amux host address" },
    FlagSpec { name: "api-key",     takes_value: true,  value_name: "KEY", hint: "API key for the remote host" },
];

pub static REMOTE_SESSION_KILL_FLAGS: &[FlagSpec] = &[
    FlagSpec { name: "remote-addr", takes_value: true,  value_name: "URL", hint: "remote headless amux host address" },
    FlagSpec { name: "api-key",     takes_value: true,  value_name: "KEY", hint: "API key for the remote host" },
];

/// All top-level amux subcommands, each with their full flag set.
/// This is the single source of truth consumed by TUI parsing and autocomplete.
pub static ALL_COMMANDS: &[CommandSpec] = &[
    CommandSpec { name: "init",       flags: INIT_FLAGS        },
    CommandSpec { name: "ready",      flags: READY_FLAGS       },
    CommandSpec { name: "implement",  flags: IMPLEMENT_FLAGS   },
    CommandSpec { name: "chat",       flags: CHAT_FLAGS        },
    CommandSpec { name: "status",     flags: STATUS_FLAGS      },
    CommandSpec { name: "exec prompt",  flags: EXEC_PROMPT_FLAGS   },
    CommandSpec { name: "exec workflow",flags: EXEC_WORKFLOW_FLAGS },
    CommandSpec { name: "specs new",  flags: SPECS_NEW_FLAGS   },
    CommandSpec { name: "specs amend",flags: SPECS_AMEND_FLAGS },
    CommandSpec { name: "headless start", flags: HEADLESS_START_FLAGS },
    CommandSpec { name: "remote run",           flags: REMOTE_RUN_FLAGS           },
    CommandSpec { name: "remote session start", flags: REMOTE_SESSION_START_FLAGS },
    CommandSpec { name: "remote session kill",  flags: REMOTE_SESSION_KILL_FLAGS  },
];
