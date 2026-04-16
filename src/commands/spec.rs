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
    FlagSpec { name: "agent", takes_value: true,  value_name: "NAME", hint: "agent to install (claude, codex, opencode, maki, gemini)" },
    FlagSpec { name: "aspec", takes_value: false, value_name: "",     hint: "download aspec templates to the current project" },
];

pub static READY_FLAGS: &[FlagSpec] = &[
    FlagSpec { name: "refresh",         takes_value: false, value_name: "", hint: "run the Dockerfile agent audit" },
    FlagSpec { name: "build",           takes_value: false, value_name: "", hint: "force rebuild the dev container image" },
    FlagSpec { name: "no-cache",        takes_value: false, value_name: "", hint: "pass --no-cache to docker build" },
    FlagSpec { name: "non-interactive", takes_value: false, value_name: "", hint: "run without interactive prompt" },
    FlagSpec { name: "allow-docker",    takes_value: false, value_name: "", hint: "allow Docker access" },
];

pub static IMPLEMENT_FLAGS: &[FlagSpec] = &[
    FlagSpec { name: "agent",           takes_value: true,  value_name: "NAME", hint: "override configured agent" },
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

pub static CONFIG_SET_FLAGS: &[FlagSpec] = &[
    FlagSpec { name: "global", takes_value: false, value_name: "", hint: "write to global config instead of repo config" },
];

/// All top-level amux subcommands, each with their full flag set.
/// This is the single source of truth consumed by TUI parsing and autocomplete.
pub static ALL_COMMANDS: &[CommandSpec] = &[
    CommandSpec { name: "init",       flags: INIT_FLAGS        },
    CommandSpec { name: "ready",      flags: READY_FLAGS       },
    CommandSpec { name: "implement",  flags: IMPLEMENT_FLAGS   },
    CommandSpec { name: "chat",       flags: CHAT_FLAGS        },
    CommandSpec { name: "status",     flags: STATUS_FLAGS      },
    CommandSpec { name: "specs new",  flags: SPECS_NEW_FLAGS   },
    CommandSpec { name: "specs amend",flags: SPECS_AMEND_FLAGS },
];
