use clap::{Parser, Subcommand, ValueEnum};

/// A containerized code and claw agent manager.
#[derive(Parser)]
#[command(name = "amux", version)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,

    /// Force rebuild the dev container image from Dockerfile.dev (passed to ready on TUI startup).
    #[arg(long, global = true)]
    pub build: bool,

    /// Pass --no-cache to docker build (passed to ready on TUI startup).
    #[arg(long, global = true)]
    pub no_cache: bool,

    /// Run the Dockerfile agent audit (passed to ready on TUI startup).
    #[arg(long, global = true)]
    pub refresh: bool,
}

#[derive(Subcommand)]
pub enum Command {
    /// Initialize the current Git repo for use with amux.
    Init {
        /// Code agent to install in the Dockerfile.dev container.
        #[arg(long, value_enum, default_value = "claude")]
        agent: Agent,
        /// Download aspec templates to the current project.
        #[arg(long)]
        aspec: bool,
    },

    /// Check Docker daemon, verify Dockerfile.dev, build image, and report status.
    Ready {
        /// Run the Dockerfile agent audit (skipped by default).
        #[arg(long)]
        refresh: bool,

        /// Force rebuild the dev container image from Dockerfile.dev.
        #[arg(long)]
        build: bool,

        /// Pass --no-cache to docker build.
        #[arg(long)]
        no_cache: bool,

        /// Run the agent in non-interactive (print) mode instead of interactive mode.
        #[arg(short = 'n', long)]
        non_interactive: bool,

        /// Mount the host Docker daemon socket into the agent container.
        #[arg(long)]
        allow_docker: bool,

        /// Suppress human output and print structured JSON. Implies --non-interactive.
        #[arg(long)]
        json: bool,
    },

    /// Launch the dev container to implement a work item.
    Implement {
        /// Work item number (e.g. 0001).
        work_item: String,

        /// Run the agent in non-interactive (print) mode instead of interactive mode.
        #[arg(short = 'n', long)]
        non_interactive: bool,

        /// Run the agent in plan mode (read-only, no file modifications).
        #[arg(long)]
        plan: bool,

        /// Mount the host Docker daemon socket into the agent container.
        #[arg(long)]
        allow_docker: bool,

        /// Path to a workflow Markdown file. If omitted, the work item is implemented
        /// in a single agent run with the current prompt, unchanged.
        #[arg(long)]
        workflow: Option<std::path::PathBuf>,

        /// Run in an isolated Git worktree under ~/.amux/worktrees/.
        #[arg(long)]
        worktree: bool,

        /// Mount host ~/.ssh read-only into the agent container.
        #[arg(long)]
        mount_ssh: bool,

        /// Enable fully autonomous mode: skip all agent permission prompts, apply
        /// yoloDisallowedTools config, and (with --workflow) auto-advance stuck steps
        /// after countdown. Implies --worktree when combined with --workflow.
        #[arg(long)]
        yolo: bool,

        /// Enable auto permission mode: pass --permission-mode auto to the agent instead of
        /// --dangerously-skip-permissions. Applies yoloDisallowedTools config. With --workflow,
        /// implies --worktree but does NOT auto-advance stuck steps.
        #[arg(long)]
        auto: bool,

        /// Agent to use (overrides .amux/config.json). If the agent image does not exist,
        /// amux will offer to download and build it.
        /// Available agents: claude, codex, opencode, maki, gemini.
        #[arg(long, value_name = "NAME")]
        agent: Option<String>,

        /// Override the model used by the launched agent (e.g. claude-opus-4-6).
        #[arg(long, value_name = "NAME")]
        model: Option<String>,
    },

    /// Start a freeform chat session with the configured agent in a container.
    Chat {
        /// Run the agent in non-interactive (print) mode instead of interactive mode.
        #[arg(short = 'n', long)]
        non_interactive: bool,

        /// Run the agent in plan mode (read-only, no file modifications).
        #[arg(long)]
        plan: bool,

        /// Mount the host Docker daemon socket into the agent container.
        #[arg(long)]
        allow_docker: bool,

        /// Mount host ~/.ssh read-only into the agent container.
        #[arg(long)]
        mount_ssh: bool,

        /// Enable fully autonomous mode: skip all agent permission prompts and apply
        /// yoloDisallowedTools config.
        #[arg(long)]
        yolo: bool,

        /// Enable auto permission mode: pass --permission-mode auto to the agent instead of
        /// --dangerously-skip-permissions. Applies yoloDisallowedTools config.
        #[arg(long)]
        auto: bool,

        /// Agent to use (overrides .amux/config.json). If the agent image does not exist,
        /// amux will offer to download and build it.
        /// Available agents: claude, codex, opencode, maki, gemini.
        #[arg(long, value_name = "NAME")]
        agent: Option<String>,

        /// Override the model used by the launched agent (e.g. claude-opus-4-6).
        #[arg(long, value_name = "NAME")]
        model: Option<String>,
    },

    /// Manage work item specs (create, interview, amend).
    Specs {
        #[command(subcommand)]
        action: SpecsAction,
    },

    /// Manage persistent background agent containers (claws agents).
    Claws {
        #[command(subcommand)]
        action: ClawsAction,
    },

    /// Show the status of all running code-agent and nanoclaw containers.
    Status {
        /// Continuously refresh the output every 3 seconds.
        #[arg(long)]
        watch: bool,
    },

    /// View and edit global and repo configuration.
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },

    /// Run a one-shot command: inject a prompt or run a workflow without a work item.
    Exec {
        #[command(subcommand)]
        action: ExecAction,
    },

    /// Run amux as a headless HTTP server for remote/automated access.
    Headless {
        #[command(subcommand)]
        action: HeadlessAction,
    },

    /// Connect to a remote headless amux instance and execute commands.
    Remote {
        #[command(subcommand)]
        action: RemoteAction,
    },
}

/// Subcommands for `amux config`.
#[derive(Subcommand)]
pub enum ConfigAction {
    /// Display all config fields at both global and repo level.
    Show,
    /// Show a single field's global value, repo value, and effective value.
    Get {
        /// Config field name (e.g. terminal_scrollback_lines).
        field: String,
    },
    /// Set a config field value (repo scope by default).
    Set {
        /// Config field name (e.g. terminal_scrollback_lines).
        field: String,
        /// New value for the field.
        value: String,
        /// Write to global config instead of repo config.
        #[arg(long)]
        global: bool,
    },
}

/// Subcommands for `amux specs`.
#[derive(Subcommand)]
pub enum SpecsAction {
    /// Create a new work item from the template.
    New {
        /// Use interview mode: have the agent complete the work item based on a summary you provide.
        #[arg(long)]
        interview: bool,
    },
    /// Review and amend a completed work item to match the final implementation.
    Amend {
        /// Work item number (e.g. 0025).
        work_item: String,
        /// Run the agent in non-interactive (print) mode.
        #[arg(short = 'n', long)]
        non_interactive: bool,
        /// Mount the host Docker daemon socket into the agent container.
        #[arg(long)]
        allow_docker: bool,
    },
}

/// Clap value parser: reject empty or whitespace-only prompt strings.
fn parse_non_empty_prompt(s: &str) -> Result<String, String> {
    if s.trim().is_empty() {
        Err("prompt cannot be empty".to_string())
    } else {
        Ok(s.to_string())
    }
}

/// Subcommands for `amux exec`.
#[derive(Subcommand)]
pub enum ExecAction {
    /// Send a prompt to the agent in non-interactive mode (like chat -n with a prompt).
    Prompt {
        /// The prompt text to send to the agent.
        #[arg(value_parser = parse_non_empty_prompt)]
        prompt: String,

        /// Run the agent in non-interactive (print) mode instead of interactive mode.
        #[arg(short = 'n', long)]
        non_interactive: bool,

        /// Run the agent in plan mode (read-only, no file modifications).
        #[arg(long)]
        plan: bool,

        /// Mount the host Docker daemon socket into the agent container.
        #[arg(long)]
        allow_docker: bool,

        /// Mount host ~/.ssh read-only into the agent container.
        #[arg(long)]
        mount_ssh: bool,

        /// Enable fully autonomous mode: skip all agent permission prompts and apply
        /// yoloDisallowedTools config.
        #[arg(long)]
        yolo: bool,

        /// Enable auto permission mode: pass --permission-mode auto to the agent instead of
        /// --dangerously-skip-permissions. Applies yoloDisallowedTools config.
        #[arg(long)]
        auto: bool,

        /// Agent to use (overrides .amux/config.json).
        #[arg(long, value_name = "NAME")]
        agent: Option<String>,

        /// Override the model used by the launched agent (e.g. claude-opus-4-6).
        #[arg(long, value_name = "NAME")]
        model: Option<String>,
    },

    /// Run a workflow file without requiring a work item number.
    #[command(alias = "wf")]
    Workflow {
        /// Path to the workflow Markdown file.
        workflow: std::path::PathBuf,

        /// Optional work item number (e.g. 0001). When omitted, the workflow
        /// runs without a work item context.
        #[arg(long, value_name = "NUM")]
        work_item: Option<String>,

        /// Run the agent in non-interactive (print) mode instead of interactive mode.
        #[arg(short = 'n', long)]
        non_interactive: bool,

        /// Run the agent in plan mode (read-only, no file modifications).
        #[arg(long)]
        plan: bool,

        /// Mount the host Docker daemon socket into the agent container.
        #[arg(long)]
        allow_docker: bool,

        /// Run in an isolated Git worktree under ~/.amux/worktrees/.
        #[arg(long)]
        worktree: bool,

        /// Mount host ~/.ssh read-only into the agent container.
        #[arg(long)]
        mount_ssh: bool,

        /// Enable fully autonomous mode: skip all agent permission prompts, apply
        /// yoloDisallowedTools config, and auto-advance stuck steps after countdown.
        /// Implies --worktree.
        #[arg(long)]
        yolo: bool,

        /// Enable auto permission mode. With --workflow, implies --worktree but does NOT
        /// auto-advance stuck steps.
        #[arg(long)]
        auto: bool,

        /// Agent to use (overrides .amux/config.json).
        #[arg(long, value_name = "NAME")]
        agent: Option<String>,

        /// Override the model used by the launched agent (e.g. claude-opus-4-6).
        #[arg(long, value_name = "NAME")]
        model: Option<String>,
    },
}

/// Subcommands for `amux headless`.
#[derive(Subcommand)]
pub enum HeadlessAction {
    /// Start the headless HTTP server.
    Start {
        /// Port to listen on.
        #[arg(long, default_value = "9876")]
        port: u16,

        /// Allowlisted working directories (repeatable). Only sessions with a workdir
        /// in this list will be accepted.
        #[arg(long = "workdirs")]
        workdirs: Vec<String>,

        /// Daemonize via the OS process manager (systemd on Linux, launchd on macOS).
        #[arg(long)]
        background: bool,

        /// Regenerate the API key: creates a new key, stores the new hash,
        /// prints the new key to stdout, and discards the old one.
        #[arg(long)]
        refresh_key: bool,

        /// Disable authentication for this execution even if a key hash exists on disk.
        /// WARNING: any client can reach the server without credentials.
        #[arg(long)]
        dangerously_skip_auth: bool,
    },

    /// Stop the background headless server.
    Kill,

    /// Stream the background server log file to stdout.
    Logs,

    /// Show headless server status (PID, port, sessions, uptime).
    Status,
}

/// Subcommands for `amux remote`.
#[derive(Subcommand)]
pub enum RemoteAction {
    /// Execute a command on the remote headless amux host.
    Run {
        /// The amux subcommand and arguments to execute on the remote host
        /// (e.g. "execute prompt hello --yolo").
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        command: Vec<String>,

        /// Address of the remote headless amux host (e.g. http://1.2.3.4:9876).
        /// Overrides AMUX_REMOTE_ADDR env var and remote.defaultAddr config.
        #[arg(long)]
        remote_addr: Option<String>,

        /// Session ID to run the command in. Required in CLI/headless modes.
        /// In TUI mode, if omitted, shows an interactive session picker.
        /// Overrides AMUX_REMOTE_SESSION env var.
        #[arg(long)]
        session: Option<String>,

        /// Stream logs from the remote host until the command completes,
        /// then print a summary table.
        #[arg(long, short = 'f')]
        follow: bool,

        /// API key for the remote headless amux host.
        /// Overrides AMUX_API_KEY env var and remote.defaultAPIKey config.
        #[arg(long)]
        api_key: Option<String>,
    },

    /// Manage sessions on the remote headless amux host.
    Session {
        #[command(subcommand)]
        action: RemoteSessionAction,
    },
}

/// Subcommands for `amux remote session`.
#[derive(Subcommand)]
pub enum RemoteSessionAction {
    /// Start a new session on the remote host for the given directory.
    Start {
        /// Working directory to use for the new session (absolute path on remote host).
        /// Required in CLI/headless modes.
        /// In TUI mode, if omitted, shows an interactive selection from remote.savedDirs.
        dir: Option<String>,

        /// Address of the remote headless amux host.
        /// Overrides AMUX_REMOTE_ADDR env var and remote.defaultAddr config.
        #[arg(long)]
        remote_addr: Option<String>,

        /// API key for the remote headless amux host.
        /// Overrides AMUX_API_KEY env var and remote.defaultAPIKey config.
        #[arg(long)]
        api_key: Option<String>,
    },

    /// Kill a session on the remote host.
    Kill {
        /// Session ID to kill. Required in CLI/headless modes.
        /// In TUI mode, if omitted, shows an interactive session picker.
        session_id: Option<String>,

        /// Address of the remote headless amux host.
        /// Overrides AMUX_REMOTE_ADDR env var and remote.defaultAddr config.
        #[arg(long)]
        remote_addr: Option<String>,

        /// API key for the remote headless amux host.
        /// Overrides AMUX_API_KEY env var and remote.defaultAPIKey config.
        #[arg(long)]
        api_key: Option<String>,
    },
}

/// Subcommands for `amux claws`.
#[derive(Subcommand)]
pub enum ClawsAction {
    /// First-time setup: fork/clone nanoclaw, build the image, and launch the container.
    Init,
    /// Check whether the nanoclaw container is running and show status.
    Ready,
    /// Attach to the running nanoclaw container for a freeform chat session.
    Chat,
}

#[derive(Clone, Debug, PartialEq, ValueEnum)]
pub enum Agent {
    Claude,
    Codex,
    Opencode,
    Maki,
    Gemini,
}

impl Agent {
    pub fn as_str(&self) -> &'static str {
        match self {
            Agent::Claude => "claude",
            Agent::Codex => "codex",
            Agent::Opencode => "opencode",
            Agent::Maki => "maki",
            Agent::Gemini => "gemini",
        }
    }

    /// All supported agents, in the canonical order used by CLI and TUI alike.
    /// This is the single source of truth — add new agents here only.
    pub fn all() -> &'static [Agent] {
        &[Agent::Claude, Agent::Codex, Agent::Opencode, Agent::Maki, Agent::Gemini]
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Agent::Claude => "Claude Code",
            Agent::Codex => "Codex",
            Agent::Opencode => "Opencode",
            Agent::Maki => "Maki",
            Agent::Gemini => "Gemini",
        }
    }
}

/// The canonical list of agent names accepted by `--agent`.
pub const KNOWN_AGENT_NAMES: &[&str] = &["claude", "codex", "opencode", "maki", "gemini"];

/// Validate an agent name from `--agent`. Returns `Ok(name)` for known names,
/// or an error with the list of available agents for unknown names.
pub fn validate_agent_name(name: &str) -> anyhow::Result<String> {
    if KNOWN_AGENT_NAMES.contains(&name) {
        Ok(name.to_string())
    } else {
        anyhow::bail!(
            "unknown agent \"{}\"; available agents: {}",
            name,
            KNOWN_AGENT_NAMES.join(", ")
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    fn parse(args: &[&str]) -> Cli {
        Cli::parse_from(args)
    }

    #[test]
    fn no_args_gives_no_subcommand() {
        let cli = parse(&["amux"]);
        assert!(cli.command.is_none());
    }

    #[test]
    fn init_default_agent_is_claude() {
        let cli = parse(&["amux", "init"]);
        match cli.command.unwrap() {
            Command::Init { agent, .. } => assert_eq!(agent.as_str(), "claude"),
            _ => panic!("expected init"),
        }
    }

    #[test]
    fn init_explicit_agent() {
        let cli = parse(&["amux", "init", "--agent", "codex"]);
        match cli.command.unwrap() {
            Command::Init { agent, .. } => assert_eq!(agent.as_str(), "codex"),
            _ => panic!("expected init"),
        }
    }

    #[test]
    fn init_aspec_flag_false_by_default() {
        let cli = parse(&["amux", "init"]);
        match cli.command.unwrap() {
            Command::Init { aspec, .. } => assert!(!aspec),
            _ => panic!("expected init"),
        }
    }

    #[test]
    fn init_aspec_flag_set() {
        let cli = parse(&["amux", "init", "--aspec"]);
        match cli.command.unwrap() {
            Command::Init { aspec, .. } => assert!(aspec),
            _ => panic!("expected init"),
        }
    }

    #[test]
    fn init_aspec_with_agent() {
        let cli = parse(&["amux", "init", "--aspec", "--agent", "codex"]);
        match cli.command.unwrap() {
            Command::Init { agent, aspec } => {
                assert_eq!(agent.as_str(), "codex");
                assert!(aspec);
            }
            _ => panic!("expected init"),
        }
    }

    #[test]
    fn implement_parses_work_item_number() {
        let cli = parse(&["amux", "implement", "42"]);
        match cli.command.unwrap() {
            Command::Implement { work_item, .. } => assert_eq!(work_item, "42"),
            _ => panic!("expected implement"),
        }
    }

    #[test]
    fn implement_workflow_flag_some() {
        let cli = parse(&["amux", "implement", "0001", "--workflow", "wf.md"]);
        match cli.command.unwrap() {
            Command::Implement { workflow, .. } => {
                assert_eq!(workflow, Some(std::path::PathBuf::from("wf.md")));
            }
            _ => panic!("expected implement"),
        }
    }

    #[test]
    fn implement_workflow_flag_none_by_default() {
        let cli = parse(&["amux", "implement", "0001"]);
        match cli.command.unwrap() {
            Command::Implement { workflow, .. } => assert!(workflow.is_none()),
            _ => panic!("expected implement"),
        }
    }

    #[test]
    fn implement_workflow_with_other_flags() {
        let cli = parse(&["amux", "implement", "0001", "--workflow", "my-wf.md", "--non-interactive"]);
        match cli.command.unwrap() {
            Command::Implement { workflow, non_interactive, .. } => {
                assert!(workflow.is_some());
                assert!(non_interactive);
            }
            _ => panic!("expected implement"),
        }
    }

    #[test]
    fn implement_parses_four_digit_work_item() {
        let cli = parse(&["amux", "implement", "0001"]);
        match cli.command.unwrap() {
            Command::Implement { work_item, .. } => assert_eq!(work_item, "0001"),
            _ => panic!("expected implement"),
        }
    }

    #[test]
    fn ready_subcommand_parsed() {
        let cli = parse(&["amux", "ready"]);
        assert!(matches!(cli.command.unwrap(), Command::Ready { .. }));
    }

    #[test]
    fn ready_refresh_flag() {
        let cli = parse(&["amux", "ready", "--refresh"]);
        match cli.command.unwrap() {
            Command::Ready { refresh, .. } => assert!(refresh),
            _ => panic!("expected ready"),
        }
    }

    #[test]
    fn ready_non_interactive_flag() {
        let cli = parse(&["amux", "ready", "--non-interactive"]);
        match cli.command.unwrap() {
            Command::Ready { non_interactive, .. } => assert!(non_interactive),
            _ => panic!("expected ready"),
        }
    }

    #[test]
    fn ready_all_flags() {
        let cli = parse(&["amux", "ready", "--refresh", "--build", "--no-cache", "--non-interactive"]);
        match cli.command.unwrap() {
            Command::Ready { refresh, build, no_cache, non_interactive, .. } => {
                assert!(refresh);
                assert!(build);
                assert!(no_cache);
                assert!(non_interactive);
            }
            _ => panic!("expected ready"),
        }
    }

    #[test]
    fn ready_defaults_no_refresh_no_non_interactive() {
        let cli = parse(&["amux", "ready"]);
        match cli.command.unwrap() {
            Command::Ready { refresh, build, no_cache, non_interactive, allow_docker, json } => {
                assert!(!refresh);
                assert!(!build);
                assert!(!no_cache);
                assert!(!non_interactive);
                assert!(!allow_docker);
                assert!(!json);
            }
            _ => panic!("expected ready"),
        }
    }

    #[test]
    fn ready_build_flag() {
        let cli = parse(&["amux", "ready", "--build"]);
        match cli.command.unwrap() {
            Command::Ready { build, .. } => assert!(build),
            _ => panic!("expected ready"),
        }
    }

    #[test]
    fn ready_no_cache_flag() {
        let cli = parse(&["amux", "ready", "--no-cache"]);
        match cli.command.unwrap() {
            Command::Ready { no_cache, .. } => assert!(no_cache),
            _ => panic!("expected ready"),
        }
    }

    #[test]
    fn ready_build_and_no_cache_flags() {
        let cli = parse(&["amux", "ready", "--build", "--no-cache"]);
        match cli.command.unwrap() {
            Command::Ready { build, no_cache, .. } => {
                assert!(build);
                assert!(no_cache);
            }
            _ => panic!("expected ready"),
        }
    }

    #[test]
    fn implement_non_interactive_flag() {
        let cli = parse(&["amux", "implement", "0001", "--non-interactive"]);
        match cli.command.unwrap() {
            Command::Implement { non_interactive, .. } => assert!(non_interactive),
            _ => panic!("expected implement"),
        }
    }

    #[test]
    fn implement_defaults_interactive() {
        let cli = parse(&["amux", "implement", "0001"]);
        match cli.command.unwrap() {
            Command::Implement { non_interactive, .. } => assert!(!non_interactive),
            _ => panic!("expected implement"),
        }
    }

    #[test]
    fn chat_subcommand_parsed() {
        let cli = parse(&["amux", "chat"]);
        assert!(matches!(cli.command.unwrap(), Command::Chat { .. }));
    }

    #[test]
    fn chat_defaults_interactive() {
        let cli = parse(&["amux", "chat"]);
        match cli.command.unwrap() {
            Command::Chat { non_interactive, .. } => assert!(!non_interactive),
            _ => panic!("expected chat"),
        }
    }

    #[test]
    fn chat_non_interactive_flag() {
        let cli = parse(&["amux", "chat", "--non-interactive"]);
        match cli.command.unwrap() {
            Command::Chat { non_interactive, .. } => assert!(non_interactive),
            _ => panic!("expected chat"),
        }
    }

    #[test]
    fn chat_plan_flag() {
        let cli = parse(&["amux", "chat", "--plan"]);
        match cli.command.unwrap() {
            Command::Chat { plan, non_interactive, .. } => {
                assert!(plan);
                assert!(!non_interactive);
            }
            _ => panic!("expected chat"),
        }
    }

    #[test]
    fn chat_defaults_no_plan() {
        let cli = parse(&["amux", "chat"]);
        match cli.command.unwrap() {
            Command::Chat { plan, .. } => assert!(!plan),
            _ => panic!("expected chat"),
        }
    }

    #[test]
    fn chat_plan_and_non_interactive() {
        let cli = parse(&["amux", "chat", "--plan", "--non-interactive"]);
        match cli.command.unwrap() {
            Command::Chat { plan, non_interactive, .. } => {
                assert!(plan);
                assert!(non_interactive);
            }
            _ => panic!("expected chat"),
        }
    }

    #[test]
    fn implement_plan_flag() {
        let cli = parse(&["amux", "implement", "0001", "--plan"]);
        match cli.command.unwrap() {
            Command::Implement { plan, work_item, non_interactive, .. } => {
                assert!(plan);
                assert_eq!(work_item, "0001");
                assert!(!non_interactive);
            }
            _ => panic!("expected implement"),
        }
    }

    #[test]
    fn implement_defaults_no_plan() {
        let cli = parse(&["amux", "implement", "0001"]);
        match cli.command.unwrap() {
            Command::Implement { plan, .. } => assert!(!plan),
            _ => panic!("expected implement"),
        }
    }

    #[test]
    fn implement_plan_and_non_interactive() {
        let cli = parse(&["amux", "implement", "0001", "--plan", "--non-interactive"]);
        match cli.command.unwrap() {
            Command::Implement { plan, non_interactive, .. } => {
                assert!(plan);
                assert!(non_interactive);
            }
            _ => panic!("expected implement"),
        }
    }

    #[test]
    fn root_build_flag() {
        let cli = parse(&["amux", "--build"]);
        assert!(cli.build);
        assert!(!cli.no_cache);
        assert!(!cli.refresh);
        assert!(cli.command.is_none());
    }

    #[test]
    fn root_no_cache_flag() {
        let cli = parse(&["amux", "--no-cache"]);
        assert!(cli.no_cache);
        assert!(!cli.build);
        assert!(!cli.refresh);
    }

    #[test]
    fn root_refresh_flag() {
        let cli = parse(&["amux", "--refresh"]);
        assert!(cli.refresh);
        assert!(!cli.build);
        assert!(!cli.no_cache);
    }

    #[test]
    fn root_all_flags() {
        let cli = parse(&["amux", "--build", "--no-cache", "--refresh"]);
        assert!(cli.build);
        assert!(cli.no_cache);
        assert!(cli.refresh);
        assert!(cli.command.is_none());
    }

    #[test]
    fn root_flags_default_false() {
        let cli = parse(&["amux"]);
        assert!(!cli.build);
        assert!(!cli.no_cache);
        assert!(!cli.refresh);
    }

    #[test]
    fn status_subcommand_parsed() {
        let cli = parse(&["amux", "status"]);
        assert!(matches!(cli.command.unwrap(), Command::Status { .. }));
    }

    #[test]
    fn status_defaults_no_watch() {
        let cli = parse(&["amux", "status"]);
        match cli.command.unwrap() {
            Command::Status { watch } => assert!(!watch),
            _ => panic!("expected status"),
        }
    }

    #[test]
    fn status_watch_flag() {
        let cli = parse(&["amux", "status", "--watch"]);
        match cli.command.unwrap() {
            Command::Status { watch } => assert!(watch),
            _ => panic!("expected status"),
        }
    }

    // --- --allow-docker flag tests ---

    #[test]
    fn implement_allow_docker_flag() {
        let cli = parse(&["amux", "implement", "0001", "--allow-docker"]);
        match cli.command.unwrap() {
            Command::Implement { allow_docker, .. } => assert!(allow_docker),
            _ => panic!("expected implement"),
        }
    }

    #[test]
    fn implement_defaults_no_allow_docker() {
        let cli = parse(&["amux", "implement", "0001"]);
        match cli.command.unwrap() {
            Command::Implement { allow_docker, .. } => assert!(!allow_docker),
            _ => panic!("expected implement"),
        }
    }

    #[test]
    fn implement_allow_docker_with_plan() {
        let cli = parse(&["amux", "implement", "0001", "--allow-docker", "--plan"]);
        match cli.command.unwrap() {
            Command::Implement { allow_docker, plan, .. } => {
                assert!(allow_docker);
                assert!(plan);
            }
            _ => panic!("expected implement"),
        }
    }

    #[test]
    fn chat_allow_docker_flag() {
        let cli = parse(&["amux", "chat", "--allow-docker"]);
        match cli.command.unwrap() {
            Command::Chat { allow_docker, .. } => assert!(allow_docker),
            _ => panic!("expected chat"),
        }
    }

    #[test]
    fn chat_defaults_no_allow_docker() {
        let cli = parse(&["amux", "chat"]);
        match cli.command.unwrap() {
            Command::Chat { allow_docker, .. } => assert!(!allow_docker),
            _ => panic!("expected chat"),
        }
    }

    #[test]
    fn chat_allow_docker_with_plan() {
        let cli = parse(&["amux", "chat", "--allow-docker", "--plan"]);
        match cli.command.unwrap() {
            Command::Chat { allow_docker, plan, .. } => {
                assert!(allow_docker);
                assert!(plan);
            }
            _ => panic!("expected chat"),
        }
    }

    #[test]
    fn ready_allow_docker_flag() {
        let cli = parse(&["amux", "ready", "--allow-docker"]);
        match cli.command.unwrap() {
            Command::Ready { allow_docker, .. } => assert!(allow_docker),
            _ => panic!("expected ready"),
        }
    }

    #[test]
    fn ready_defaults_no_allow_docker() {
        let cli = parse(&["amux", "ready"]);
        match cli.command.unwrap() {
            Command::Ready { allow_docker, .. } => assert!(!allow_docker),
            _ => panic!("expected ready"),
        }
    }

    #[test]
    fn ready_allow_docker_with_refresh() {
        let cli = parse(&["amux", "ready", "--allow-docker", "--refresh"]);
        match cli.command.unwrap() {
            Command::Ready { allow_docker, refresh, .. } => {
                assert!(allow_docker);
                assert!(refresh);
            }
            _ => panic!("expected ready"),
        }
    }

    #[test]
    fn claws_ready_parsed() {
        let cli = parse(&["amux", "claws", "ready"]);
        assert!(matches!(
            cli.command.unwrap(),
            Command::Claws { action: ClawsAction::Ready }
        ));
    }

    #[test]
    fn claws_ready_is_ready_action() {
        let cli = parse(&["amux", "claws", "ready"]);
        match cli.command.unwrap() {
            Command::Claws { action } => assert!(matches!(action, ClawsAction::Ready)),
            _ => panic!("expected claws"),
        }
    }

    #[test]
    fn claws_init_parsed() {
        let cli = parse(&["amux", "claws", "init"]);
        assert!(matches!(
            cli.command.unwrap(),
            Command::Claws { action: ClawsAction::Init }
        ));
    }

    #[test]
    fn claws_init_is_init_action() {
        let cli = parse(&["amux", "claws", "init"]);
        match cli.command.unwrap() {
            Command::Claws { action } => assert!(matches!(action, ClawsAction::Init)),
            _ => panic!("expected claws"),
        }
    }

    #[test]
    fn claws_chat_parsed() {
        let cli = parse(&["amux", "claws", "chat"]);
        assert!(matches!(
            cli.command.unwrap(),
            Command::Claws { action: ClawsAction::Chat }
        ));
    }

    #[test]
    fn claws_chat_is_chat_action() {
        let cli = parse(&["amux", "claws", "chat"]);
        match cli.command.unwrap() {
            Command::Claws { action } => assert!(matches!(action, ClawsAction::Chat)),
            _ => panic!("expected claws"),
        }
    }

    #[test]
    fn specs_new_parsed() {
        let cli = parse(&["amux", "specs", "new"]);
        match cli.command.unwrap() {
            Command::Specs { action: SpecsAction::New { interview } } => assert!(!interview),
            _ => panic!("expected specs new"),
        }
    }

    #[test]
    fn specs_new_interview_flag() {
        let cli = parse(&["amux", "specs", "new", "--interview"]);
        match cli.command.unwrap() {
            Command::Specs { action: SpecsAction::New { interview } } => assert!(interview),
            _ => panic!("expected specs new --interview"),
        }
    }

    #[test]
    fn specs_amend_parsed() {
        let cli = parse(&["amux", "specs", "amend", "0025"]);
        match cli.command.unwrap() {
            Command::Specs { action: SpecsAction::Amend { work_item, non_interactive, allow_docker } } => {
                assert_eq!(work_item, "0025");
                assert!(!non_interactive);
                assert!(!allow_docker);
            }
            _ => panic!("expected specs amend"),
        }
    }

    #[test]
    fn specs_amend_non_interactive_flag() {
        let cli = parse(&["amux", "specs", "amend", "0025", "--non-interactive"]);
        match cli.command.unwrap() {
            Command::Specs { action: SpecsAction::Amend { non_interactive, .. } } => {
                assert!(non_interactive);
            }
            _ => panic!("expected specs amend --non-interactive"),
        }
    }

    #[test]
    fn specs_amend_allow_docker_flag() {
        let cli = parse(&["amux", "specs", "amend", "0025", "--allow-docker"]);
        match cli.command.unwrap() {
            Command::Specs { action: SpecsAction::Amend { allow_docker, .. } } => {
                assert!(allow_docker);
            }
            _ => panic!("expected specs amend --allow-docker"),
        }
    }

    #[test]
    fn claws_actions_are_distinct() {
        let init = parse(&["amux", "claws", "init"]);
        let ready = parse(&["amux", "claws", "ready"]);
        let chat = parse(&["amux", "claws", "chat"]);
        assert!(matches!(
            init.command.unwrap(),
            Command::Claws { action: ClawsAction::Init }
        ));
        assert!(matches!(
            ready.command.unwrap(),
            Command::Claws { action: ClawsAction::Ready }
        ));
        assert!(matches!(
            chat.command.unwrap(),
            Command::Claws { action: ClawsAction::Chat }
        ));
    }

    // -----------------------------------------------------------------------
    // --worktree flag (work item 0030)
    // -----------------------------------------------------------------------

    #[test]
    fn implement_worktree_flag_true() {
        let cli = parse(&["amux", "implement", "0001", "--worktree"]);
        match cli.command.unwrap() {
            Command::Implement { worktree, .. } => assert!(worktree),
            _ => panic!("expected implement"),
        }
    }

    #[test]
    fn implement_worktree_flag_false_by_default() {
        let cli = parse(&["amux", "implement", "0001"]);
        match cli.command.unwrap() {
            Command::Implement { worktree, .. } => assert!(!worktree),
            _ => panic!("expected implement"),
        }
    }

    #[test]
    fn implement_worktree_and_workflow_flags_together() {
        let cli = parse(&["amux", "implement", "0001", "--worktree", "--workflow", "wf.md"]);
        match cli.command.unwrap() {
            Command::Implement { worktree, workflow, .. } => {
                assert!(worktree);
                assert_eq!(workflow, Some(std::path::PathBuf::from("wf.md")));
            }
            _ => panic!("expected implement"),
        }
    }

    // -----------------------------------------------------------------------
    // --mount-ssh flag (work item 0030)
    // -----------------------------------------------------------------------

    #[test]
    fn chat_mount_ssh_flag_true() {
        let cli = parse(&["amux", "chat", "--mount-ssh"]);
        match cli.command.unwrap() {
            Command::Chat { mount_ssh, .. } => assert!(mount_ssh),
            _ => panic!("expected chat"),
        }
    }

    #[test]
    fn chat_mount_ssh_default_false() {
        let cli = parse(&["amux", "chat"]);
        match cli.command.unwrap() {
            Command::Chat { mount_ssh, .. } => assert!(!mount_ssh),
            _ => panic!("expected chat"),
        }
    }

    #[test]
    fn implement_mount_ssh_flag_true() {
        let cli = parse(&["amux", "implement", "0001", "--mount-ssh"]);
        match cli.command.unwrap() {
            Command::Implement { mount_ssh, .. } => assert!(mount_ssh),
            _ => panic!("expected implement"),
        }
    }

    #[test]
    fn implement_mount_ssh_default_false() {
        let cli = parse(&["amux", "implement", "0001"]);
        match cli.command.unwrap() {
            Command::Implement { mount_ssh, .. } => assert!(!mount_ssh),
            _ => panic!("expected implement"),
        }
    }

    #[test]
    fn implement_worktree_and_mount_ssh_flags_together() {
        let cli = parse(&["amux", "implement", "0001", "--worktree", "--mount-ssh"]);
        match cli.command.unwrap() {
            Command::Implement { worktree, mount_ssh, .. } => {
                assert!(worktree);
                assert!(mount_ssh);
            }
            _ => panic!("expected implement"),
        }
    }

    #[test]
    fn implement_worktree_mount_ssh_and_workflow_together() {
        let cli = parse(&["amux", "implement", "0001", "--worktree", "--mount-ssh", "--workflow", "wf.md"]);
        match cli.command.unwrap() {
            Command::Implement { worktree, mount_ssh, workflow, .. } => {
                assert!(worktree);
                assert!(mount_ssh);
                assert_eq!(workflow, Some(std::path::PathBuf::from("wf.md")));
            }
            _ => panic!("expected implement"),
        }
    }

    // -----------------------------------------------------------------------
    // --auto flag
    // -----------------------------------------------------------------------

    #[test]
    fn implement_auto_flag_true() {
        let cli = parse(&["amux", "implement", "0001", "--auto"]);
        match cli.command.unwrap() {
            Command::Implement { auto, .. } => assert!(auto),
            _ => panic!("expected implement"),
        }
    }

    #[test]
    fn implement_auto_flag_false_by_default() {
        let cli = parse(&["amux", "implement", "0001"]);
        match cli.command.unwrap() {
            Command::Implement { auto, .. } => assert!(!auto),
            _ => panic!("expected implement"),
        }
    }

    #[test]
    fn chat_auto_flag_true() {
        let cli = parse(&["amux", "chat", "--auto"]);
        match cli.command.unwrap() {
            Command::Chat { auto, .. } => assert!(auto),
            _ => panic!("expected chat"),
        }
    }

    #[test]
    fn chat_auto_flag_false_by_default() {
        let cli = parse(&["amux", "chat"]);
        match cli.command.unwrap() {
            Command::Chat { auto, .. } => assert!(!auto),
            _ => panic!("expected chat"),
        }
    }

    #[test]
    fn implement_auto_and_yolo_can_coexist() {
        let cli = parse(&["amux", "implement", "0001", "--auto", "--yolo"]);
        match cli.command.unwrap() {
            Command::Implement { auto, yolo, .. } => {
                assert!(auto);
                assert!(yolo);
            }
            _ => panic!("expected implement"),
        }
    }

    // ── config subcommand parsing ─────────────────────────────────────────────

    #[test]
    fn config_show_parsed() {
        let cli = parse(&["amux", "config", "show"]);
        assert!(matches!(
            cli.command.unwrap(),
            Command::Config { action: ConfigAction::Show }
        ));
    }

    #[test]
    fn config_get_parsed() {
        let cli = parse(&["amux", "config", "get", "terminal_scrollback_lines"]);
        match cli.command.unwrap() {
            Command::Config { action: ConfigAction::Get { field } } => {
                assert_eq!(field, "terminal_scrollback_lines");
            }
            _ => panic!("expected config get"),
        }
    }

    #[test]
    fn config_set_parsed_without_global() {
        let cli = parse(&["amux", "config", "set", "agent", "codex"]);
        match cli.command.unwrap() {
            Command::Config { action: ConfigAction::Set { field, value, global } } => {
                assert_eq!(field, "agent");
                assert_eq!(value, "codex");
                assert!(!global);
            }
            _ => panic!("expected config set"),
        }
    }

    #[test]
    fn config_set_parsed_with_global_flag() {
        let cli = parse(&["amux", "config", "set", "--global", "default_agent", "gemini"]);
        match cli.command.unwrap() {
            Command::Config { action: ConfigAction::Set { field, value, global } } => {
                assert_eq!(field, "default_agent");
                assert_eq!(value, "gemini");
                assert!(global);
            }
            _ => panic!("expected config set --global"),
        }
    }

    #[test]
    fn config_set_global_flag_default_false() {
        let cli = parse(&["amux", "config", "set", "agent", "claude"]);
        match cli.command.unwrap() {
            Command::Config { action: ConfigAction::Set { global, .. } } => {
                assert!(!global);
            }
            _ => panic!("expected config set"),
        }
    }

    #[test]
    fn config_show_listed_in_help() {
        // Smoke-test that the Config variant is wired into the top-level help.
        let cli = parse(&["amux"]);
        assert!(cli.command.is_none()); // no subcommand given
    }

    // ─── CLI/spec parity (work item 0053 Test A) ─────────────────────────────
    //
    // Each test enumerates the long-flag names that clap exposes for a
    // subcommand and compares them against the corresponding `*_FLAGS`
    // constant in `spec.rs`.  A failure means someone added a flag to one
    // place but not the other.

    fn cli_long_flags_for(subcommand: &str) -> Vec<String> {
        use clap::CommandFactory;
        Cli::command()
            .find_subcommand(subcommand)
            .unwrap_or_else(|| panic!("subcommand '{}' not found in CLI", subcommand))
            .get_arguments()
            .filter_map(|a| a.get_long())
            .filter(|&name| name != "help")
            .map(str::to_string)
            .collect()
    }

    #[test]
    fn cli_spec_parity_chat() {
        use crate::commands::spec;
        let cli_flags = cli_long_flags_for("chat");
        let spec_flags: Vec<&str> = spec::CHAT_FLAGS.iter().map(|f| f.name).collect();
        for flag in &cli_flags {
            assert!(
                spec_flags.contains(&flag.as_str()),
                "CLI flag --{flag} missing from CHAT_FLAGS in spec.rs",
            );
        }
        for flag in &spec_flags {
            assert!(
                cli_flags.contains(&flag.to_string()),
                "Spec flag --{flag} missing from CLI `chat` subcommand in cli.rs",
            );
        }
    }

    #[test]
    fn cli_spec_parity_implement() {
        use crate::commands::spec;
        let cli_flags = cli_long_flags_for("implement");
        let spec_flags: Vec<&str> = spec::IMPLEMENT_FLAGS.iter().map(|f| f.name).collect();
        for flag in &cli_flags {
            assert!(
                spec_flags.contains(&flag.as_str()),
                "CLI flag --{flag} missing from IMPLEMENT_FLAGS in spec.rs",
            );
        }
        for flag in &spec_flags {
            assert!(
                cli_flags.contains(&flag.to_string()),
                "Spec flag --{flag} missing from CLI `implement` subcommand in cli.rs",
            );
        }
    }

    #[test]
    fn cli_spec_parity_init() {
        use crate::commands::spec;
        let cli_flags = cli_long_flags_for("init");
        let spec_flags: Vec<&str> = spec::INIT_FLAGS.iter().map(|f| f.name).collect();
        for flag in &cli_flags {
            assert!(
                spec_flags.contains(&flag.as_str()),
                "CLI flag --{flag} missing from INIT_FLAGS in spec.rs",
            );
        }
        for flag in &spec_flags {
            assert!(
                cli_flags.contains(&flag.to_string()),
                "Spec flag --{flag} missing from CLI `init` subcommand in cli.rs",
            );
        }
    }

    #[test]
    fn cli_spec_parity_ready() {
        use crate::commands::spec;
        let cli_flags = cli_long_flags_for("ready");
        let spec_flags: Vec<&str> = spec::READY_FLAGS.iter().map(|f| f.name).collect();
        for flag in &cli_flags {
            assert!(
                spec_flags.contains(&flag.as_str()),
                "CLI flag --{flag} missing from READY_FLAGS in spec.rs",
            );
        }
        for flag in &spec_flags {
            assert!(
                cli_flags.contains(&flag.to_string()),
                "Spec flag --{flag} missing from CLI `ready` subcommand in cli.rs",
            );
        }
    }

    #[test]
    fn cli_spec_parity_status() {
        use crate::commands::spec;
        let cli_flags = cli_long_flags_for("status");
        let spec_flags: Vec<&str> = spec::STATUS_FLAGS.iter().map(|f| f.name).collect();
        for flag in &cli_flags {
            assert!(
                spec_flags.contains(&flag.as_str()),
                "CLI flag --{flag} missing from STATUS_FLAGS in spec.rs",
            );
        }
        for flag in &spec_flags {
            assert!(
                cli_flags.contains(&flag.to_string()),
                "Spec flag --{flag} missing from CLI `status` subcommand in cli.rs",
            );
        }
    }

    // ─── CLI --flag=value regression (work item 0053 step 6) ─────────────────
    //
    // Clap handles the `=`-separated form natively.  These tests act as a
    // regression guard to ensure both forms always produce identical results.

    #[test]
    fn chat_agent_both_forms_produce_identical_result() {
        let space_form = parse(&["amux", "chat", "--agent", "codex"]);
        let eq_form    = parse(&["amux", "chat", "--agent=codex"]);
        let agent_space = match space_form.command.unwrap() { Command::Chat { agent, .. } => agent, _ => panic!() };
        let agent_eq    = match eq_form.command.unwrap()    { Command::Chat { agent, .. } => agent, _ => panic!() };
        assert_eq!(agent_space, agent_eq, "--agent codex and --agent=codex must parse identically");
    }

    #[test]
    fn implement_agent_both_forms_produce_identical_result() {
        let space_form = parse(&["amux", "implement", "0042", "--agent", "opencode"]);
        let eq_form    = parse(&["amux", "implement", "0042", "--agent=opencode"]);
        let agent_space = match space_form.command.unwrap() { Command::Implement { agent, .. } => agent, _ => panic!() };
        let agent_eq    = match eq_form.command.unwrap()    { Command::Implement { agent, .. } => agent, _ => panic!() };
        assert_eq!(agent_space, agent_eq, "--agent opencode and --agent=opencode must parse identically");
    }

    // ─── --model flag on chat / implement (work item 0055) ────────────────────

    #[test]
    fn chat_model_both_forms_produce_identical_result() {
        let space_form = parse(&["amux", "chat", "--model", "claude-opus-4-6"]);
        let eq_form    = parse(&["amux", "chat", "--model=claude-opus-4-6"]);
        let model_space = match space_form.command.unwrap() { Command::Chat { model, .. } => model, _ => panic!() };
        let model_eq    = match eq_form.command.unwrap()    { Command::Chat { model, .. } => model, _ => panic!() };
        assert_eq!(model_space, model_eq, "--model claude-opus-4-6 and --model=claude-opus-4-6 must parse identically");
    }

    #[test]
    fn implement_model_both_forms_produce_identical_result() {
        let space_form = parse(&["amux", "implement", "0042", "--model", "claude-haiku-4-5"]);
        let eq_form    = parse(&["amux", "implement", "0042", "--model=claude-haiku-4-5"]);
        let model_space = match space_form.command.unwrap() { Command::Implement { model, .. } => model, _ => panic!() };
        let model_eq    = match eq_form.command.unwrap()    { Command::Implement { model, .. } => model, _ => panic!() };
        assert_eq!(model_space, model_eq, "--model claude-haiku-4-5 and --model=claude-haiku-4-5 must parse identically");
    }

    #[test]
    fn chat_without_model_is_none() {
        let cli = parse(&["amux", "chat"]);
        match cli.command.unwrap() {
            Command::Chat { model, .. } => {
                assert!(model.is_none(), "chat without --model should produce None");
            }
            _ => panic!("expected chat"),
        }
    }

    #[test]
    fn implement_without_model_is_none() {
        let cli = parse(&["amux", "implement", "0001"]);
        match cli.command.unwrap() {
            Command::Implement { model, .. } => {
                assert!(model.is_none(), "implement without --model should produce None");
            }
            _ => panic!("expected implement"),
        }
    }

    #[test]
    fn model_appears_in_chat_flags_spec() {
        use crate::commands::spec;
        let spec_flags: Vec<&str> = spec::CHAT_FLAGS.iter().map(|f| f.name).collect();
        assert!(
            spec_flags.contains(&"model"),
            "`model` must be present in CHAT_FLAGS; got: {:?}",
            spec_flags
        );
    }

    #[test]
    fn model_appears_in_implement_flags_spec() {
        use crate::commands::spec;
        let spec_flags: Vec<&str> = spec::IMPLEMENT_FLAGS.iter().map(|f| f.name).collect();
        assert!(
            spec_flags.contains(&"model"),
            "`model` must be present in IMPLEMENT_FLAGS; got: {:?}",
            spec_flags
        );
    }

    // ─── --agent flag on chat / validate_agent_name (work item 0049) ─────────

    #[test]
    fn chat_agent_claude_is_some() {
        let cli = parse(&["amux", "chat", "--agent", "claude"]);
        match cli.command.unwrap() {
            Command::Chat { agent, .. } => {
                assert_eq!(agent, Some("claude".to_string()));
            }
            _ => panic!("expected chat"),
        }
    }

    #[test]
    fn chat_without_agent_is_none() {
        let cli = parse(&["amux", "chat"]);
        match cli.command.unwrap() {
            Command::Chat { agent, .. } => {
                assert!(agent.is_none(), "chat without --agent should produce None");
            }
            _ => panic!("expected chat"),
        }
    }

    #[test]
    fn validate_agent_name_unknown_returns_error() {
        let result = validate_agent_name("unknown");
        assert!(result.is_err(), "unknown agent name should return Err");
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("unknown"),
            "error should mention the unknown agent name; got: {}",
            msg
        );
        assert!(
            msg.contains("available agents:"),
            "error should list available agents; got: {}",
            msg
        );
    }

    #[test]
    fn validate_agent_name_known_agents_are_accepted() {
        for &name in KNOWN_AGENT_NAMES {
            let result = validate_agent_name(name);
            assert!(result.is_ok(), "{} should be accepted by validate_agent_name", name);
        }
    }

    // ─── HeadlessAction parsing (work item 0057) ─────────────────────────────

    // ── headless start defaults ──────────────────────────────────────────────

    #[test]
    fn headless_start_parses_with_all_defaults() {
        let cli = parse(&["amux", "headless", "start"]);
        match cli.command.unwrap() {
            Command::Headless {
                action: HeadlessAction::Start { port, workdirs, background, refresh_key, dangerously_skip_auth },
            } => {
                assert_eq!(port, 9876, "default port must be 9876");
                assert!(workdirs.is_empty(), "default workdirs must be empty");
                assert!(!background, "background must default to false");
                assert!(!refresh_key, "refresh_key must default to false");
                assert!(!dangerously_skip_auth, "dangerously_skip_auth must default to false");
            }
            _ => panic!("expected headless start"),
        }
    }

    // ── --port ───────────────────────────────────────────────────────────────

    #[test]
    fn headless_start_port_flag_space_form() {
        let cli = parse(&["amux", "headless", "start", "--port", "8080"]);
        match cli.command.unwrap() {
            Command::Headless { action: HeadlessAction::Start { port, .. } } => {
                assert_eq!(port, 8080);
            }
            _ => panic!("expected headless start"),
        }
    }

    #[test]
    fn headless_start_port_flag_eq_form() {
        let cli = parse(&["amux", "headless", "start", "--port=12345"]);
        match cli.command.unwrap() {
            Command::Headless { action: HeadlessAction::Start { port, .. } } => {
                assert_eq!(port, 12345);
            }
            _ => panic!("expected headless start"),
        }
    }

    // ── --workdirs ───────────────────────────────────────────────────────────

    #[test]
    fn headless_start_single_workdir() {
        let cli = parse(&["amux", "headless", "start", "--workdirs", "/workspace/repo"]);
        match cli.command.unwrap() {
            Command::Headless { action: HeadlessAction::Start { workdirs, .. } } => {
                assert_eq!(workdirs, vec!["/workspace/repo".to_string()]);
            }
            _ => panic!("expected headless start"),
        }
    }

    #[test]
    fn headless_start_multiple_workdirs_via_repeated_flag() {
        let cli = parse(&[
            "amux", "headless", "start",
            "--workdirs", "/workspace/a",
            "--workdirs", "/workspace/b",
            "--workdirs", "/workspace/c",
        ]);
        match cli.command.unwrap() {
            Command::Headless { action: HeadlessAction::Start { workdirs, .. } } => {
                assert_eq!(
                    workdirs,
                    vec![
                        "/workspace/a".to_string(),
                        "/workspace/b".to_string(),
                        "/workspace/c".to_string(),
                    ]
                );
            }
            _ => panic!("expected headless start"),
        }
    }

    // ── --background ─────────────────────────────────────────────────────────

    #[test]
    fn headless_start_background_flag_sets_true() {
        let cli = parse(&["amux", "headless", "start", "--background"]);
        match cli.command.unwrap() {
            Command::Headless { action: HeadlessAction::Start { background, .. } } => {
                assert!(background);
            }
            _ => panic!("expected headless start"),
        }
    }

    #[test]
    fn headless_start_all_flags_together() {
        let cli = parse(&[
            "amux", "headless", "start",
            "--port", "9999",
            "--workdirs", "/tmp/work",
            "--background",
            "--refresh-key",
            "--dangerously-skip-auth",
        ]);
        match cli.command.unwrap() {
            Command::Headless {
                action: HeadlessAction::Start { port, workdirs, background, refresh_key, dangerously_skip_auth },
            } => {
                assert_eq!(port, 9999);
                assert_eq!(workdirs, vec!["/tmp/work".to_string()]);
                assert!(background);
                assert!(refresh_key);
                assert!(dangerously_skip_auth);
            }
            _ => panic!("expected headless start"),
        }
    }

    #[test]
    fn headless_start_refresh_key_flag_alone() {
        let cli = parse(&["amux", "headless", "start", "--refresh-key"]);
        match cli.command.unwrap() {
            Command::Headless {
                action: HeadlessAction::Start { refresh_key, dangerously_skip_auth, .. },
            } => {
                assert!(refresh_key, "--refresh-key must set refresh_key=true");
                assert!(!dangerously_skip_auth, "dangerously_skip_auth must default to false");
            }
            _ => panic!("expected headless start"),
        }
    }

    #[test]
    fn headless_start_dangerously_skip_auth_flag_alone() {
        let cli = parse(&["amux", "headless", "start", "--dangerously-skip-auth"]);
        match cli.command.unwrap() {
            Command::Headless {
                action: HeadlessAction::Start { refresh_key, dangerously_skip_auth, .. },
            } => {
                assert!(!refresh_key, "refresh_key must default to false");
                assert!(dangerously_skip_auth, "--dangerously-skip-auth must set flag=true");
            }
            _ => panic!("expected headless start"),
        }
    }

    // ── headless kill / logs / status ─────────────────────────────────────────

    #[test]
    fn headless_kill_parses() {
        let cli = parse(&["amux", "headless", "kill"]);
        assert!(matches!(
            cli.command.unwrap(),
            Command::Headless { action: HeadlessAction::Kill }
        ));
    }

    #[test]
    fn headless_logs_parses() {
        let cli = parse(&["amux", "headless", "logs"]);
        assert!(matches!(
            cli.command.unwrap(),
            Command::Headless { action: HeadlessAction::Logs }
        ));
    }

    #[test]
    fn headless_status_parses() {
        let cli = parse(&["amux", "headless", "status"]);
        assert!(matches!(
            cli.command.unwrap(),
            Command::Headless { action: HeadlessAction::Status }
        ));
    }

    // ── CLI/spec parity for headless start ───────────────────────────────────

    #[test]
    fn cli_spec_parity_headless_start() {
        use crate::commands::spec;
        use clap::CommandFactory;

        // Navigate to the `headless start` subcommand.
        let mut cmd = Cli::command();
        let headless = cmd
            .find_subcommand_mut("headless")
            .expect("headless subcommand must exist");
        let start = headless
            .find_subcommand("start")
            .expect("headless start subcommand must exist");

        let cli_flags: Vec<String> = start
            .get_arguments()
            .filter_map(|a| a.get_long())
            .filter(|&name| name != "help")
            .map(str::to_string)
            .collect();

        let spec_flags: Vec<&str> = spec::HEADLESS_START_FLAGS.iter().map(|f| f.name).collect();

        for flag in &cli_flags {
            assert!(
                spec_flags.contains(&flag.as_str()),
                "CLI flag --{flag} is missing from HEADLESS_START_FLAGS in spec.rs"
            );
        }
        for flag in &spec_flags {
            assert!(
                cli_flags.contains(&flag.to_string()),
                "Spec flag --{flag} is missing from the CLI `headless start` subcommand"
            );
        }
    }

    // ── ExecAction parsing (work item 0058) ──────────────────────────────────

    // ── exec prompt ──────────────────────────────────────────────────────────

    #[test]
    fn exec_prompt_empty_string_is_rejected_at_cli_level() {
        // The value_parser on the `prompt` field must reject empty strings before
        // any command handler is invoked.
        // Use pattern matching rather than unwrap_err() to avoid needing Cli: Debug.
        match Cli::try_parse_from(["amux", "exec", "prompt", ""]) {
            Ok(_) => panic!("empty prompt must be rejected by CLI validation"),
            Err(e) => {
                let err_msg = e.to_string();
                assert!(
                    err_msg.contains("prompt cannot be empty"),
                    "error message must mention 'prompt cannot be empty'; got: {err_msg}"
                );
            }
        }
    }

    #[test]
    fn exec_prompt_whitespace_only_string_is_rejected_at_cli_level() {
        match Cli::try_parse_from(["amux", "exec", "prompt", "   "]) {
            Ok(_) => panic!("whitespace-only prompt must be rejected by CLI validation"),
            Err(_) => {} // expected
        }
    }

    #[test]
    fn exec_prompt_parses_with_prompt_only() {
        let cli = parse(&["amux", "exec", "prompt", "hello world"]);
        match cli.command.unwrap() {
            Command::Exec { action: ExecAction::Prompt { prompt, .. } } => {
                assert_eq!(prompt, "hello world");
            }
            _ => panic!("expected exec prompt"),
        }
    }

    #[test]
    fn exec_prompt_defaults() {
        let cli = parse(&["amux", "exec", "prompt", "hi"]);
        match cli.command.unwrap() {
            Command::Exec { action: ExecAction::Prompt {
                non_interactive, plan, allow_docker, mount_ssh, yolo, auto, agent, model, ..
            } } => {
                assert!(!non_interactive, "non_interactive must default to false");
                assert!(!plan, "plan must default to false");
                assert!(!allow_docker, "allow_docker must default to false");
                assert!(!mount_ssh, "mount_ssh must default to false");
                assert!(!yolo, "yolo must default to false");
                assert!(!auto, "auto must default to false");
                assert!(agent.is_none(), "agent must default to None");
                assert!(model.is_none(), "model must default to None");
            }
            _ => panic!("expected exec prompt"),
        }
    }

    #[test]
    fn exec_prompt_non_interactive_long_form() {
        let cli = parse(&["amux", "exec", "prompt", "hi", "--non-interactive"]);
        match cli.command.unwrap() {
            Command::Exec { action: ExecAction::Prompt { non_interactive, .. } } => {
                assert!(non_interactive);
            }
            _ => panic!("expected exec prompt"),
        }
    }

    #[test]
    fn exec_prompt_non_interactive_short_alias() {
        // -n is the short alias for --non-interactive on exec prompt.
        let cli = parse(&["amux", "exec", "prompt", "hi", "-n"]);
        match cli.command.unwrap() {
            Command::Exec { action: ExecAction::Prompt { non_interactive, .. } } => {
                assert!(non_interactive, "-n must set non_interactive = true");
            }
            _ => panic!("expected exec prompt"),
        }
    }

    #[test]
    fn exec_prompt_all_flags() {
        let cli = parse(&[
            "amux", "exec", "prompt", "do stuff",
            "--plan", "--allow-docker", "--mount-ssh", "--yolo", "--auto",
            "--agent", "codex", "--model", "claude-opus-4-6",
        ]);
        match cli.command.unwrap() {
            Command::Exec { action: ExecAction::Prompt {
                prompt, plan, allow_docker, mount_ssh, yolo, auto, agent, model, ..
            } } => {
                assert_eq!(prompt, "do stuff");
                assert!(plan);
                assert!(allow_docker);
                assert!(mount_ssh);
                assert!(yolo);
                assert!(auto);
                assert_eq!(agent, Some("codex".to_string()));
                assert_eq!(model, Some("claude-opus-4-6".to_string()));
            }
            _ => panic!("expected exec prompt"),
        }
    }

    #[test]
    fn exec_prompt_agent_eq_form() {
        let cli = parse(&["amux", "exec", "prompt", "hi", "--agent=opencode"]);
        match cli.command.unwrap() {
            Command::Exec { action: ExecAction::Prompt { agent, .. } } => {
                assert_eq!(agent, Some("opencode".to_string()));
            }
            _ => panic!("expected exec prompt"),
        }
    }

    #[test]
    fn exec_prompt_model_eq_form() {
        let cli = parse(&["amux", "exec", "prompt", "hi", "--model=claude-haiku-4-5"]);
        match cli.command.unwrap() {
            Command::Exec { action: ExecAction::Prompt { model, .. } } => {
                assert_eq!(model, Some("claude-haiku-4-5".to_string()));
            }
            _ => panic!("expected exec prompt"),
        }
    }

    // ── exec workflow ─────────────────────────────────────────────────────────

    #[test]
    fn exec_workflow_parses_with_path_only() {
        let cli = parse(&["amux", "exec", "workflow", "./wf.md"]);
        match cli.command.unwrap() {
            Command::Exec { action: ExecAction::Workflow { workflow, .. } } => {
                assert_eq!(workflow, std::path::PathBuf::from("./wf.md"));
            }
            _ => panic!("expected exec workflow"),
        }
    }

    #[test]
    fn exec_workflow_defaults() {
        let cli = parse(&["amux", "exec", "workflow", "./wf.md"]);
        match cli.command.unwrap() {
            Command::Exec { action: ExecAction::Workflow {
                work_item, non_interactive, plan, allow_docker, worktree,
                mount_ssh, yolo, auto, agent, model, ..
            } } => {
                assert!(work_item.is_none(), "work_item must default to None");
                assert!(!non_interactive, "non_interactive must default to false");
                assert!(!plan, "plan must default to false");
                assert!(!allow_docker, "allow_docker must default to false");
                assert!(!worktree, "worktree must default to false");
                assert!(!mount_ssh, "mount_ssh must default to false");
                assert!(!yolo, "yolo must default to false");
                assert!(!auto, "auto must default to false");
                assert!(agent.is_none(), "agent must default to None");
                assert!(model.is_none(), "model must default to None");
            }
            _ => panic!("expected exec workflow"),
        }
    }

    #[test]
    fn exec_workflow_parses_work_item_flag() {
        let cli = parse(&["amux", "exec", "workflow", "./wf.md", "--work-item", "0053"]);
        match cli.command.unwrap() {
            Command::Exec { action: ExecAction::Workflow { work_item, .. } } => {
                assert_eq!(work_item, Some("0053".to_string()));
            }
            _ => panic!("expected exec workflow"),
        }
    }

    #[test]
    fn exec_workflow_non_interactive_long_form() {
        let cli = parse(&["amux", "exec", "workflow", "./wf.md", "--non-interactive"]);
        match cli.command.unwrap() {
            Command::Exec { action: ExecAction::Workflow { non_interactive, .. } } => {
                assert!(non_interactive);
            }
            _ => panic!("expected exec workflow"),
        }
    }

    #[test]
    fn exec_workflow_non_interactive_short_alias() {
        // -n is the short alias for --non-interactive on exec workflow.
        let cli = parse(&["amux", "exec", "workflow", "./wf.md", "-n"]);
        match cli.command.unwrap() {
            Command::Exec { action: ExecAction::Workflow { non_interactive, .. } } => {
                assert!(non_interactive, "-n must set non_interactive = true");
            }
            _ => panic!("expected exec workflow"),
        }
    }

    #[test]
    fn exec_workflow_all_flags() {
        let cli = parse(&[
            "amux", "exec", "workflow", "my-workflow.md",
            "--work-item", "0001",
            "--plan", "--allow-docker", "--worktree", "--mount-ssh",
            "--yolo", "--auto",
            "--agent", "maki", "--model", "claude-sonnet-4-6",
        ]);
        match cli.command.unwrap() {
            Command::Exec { action: ExecAction::Workflow {
                workflow, work_item, plan, allow_docker, worktree,
                mount_ssh, yolo, auto, agent, model, ..
            } } => {
                assert_eq!(workflow, std::path::PathBuf::from("my-workflow.md"));
                assert_eq!(work_item, Some("0001".to_string()));
                assert!(plan);
                assert!(allow_docker);
                assert!(worktree);
                assert!(mount_ssh);
                assert!(yolo);
                assert!(auto);
                assert_eq!(agent, Some("maki".to_string()));
                assert_eq!(model, Some("claude-sonnet-4-6".to_string()));
            }
            _ => panic!("expected exec workflow"),
        }
    }

    // ── exec wf alias ─────────────────────────────────────────────────────────

    #[test]
    fn exec_wf_alias_parses_same_as_exec_workflow() {
        // `exec wf` is an alias for `exec workflow`.
        let via_alias = parse(&["amux", "exec", "wf", "my-workflow.md"]);
        let via_full = parse(&["amux", "exec", "workflow", "my-workflow.md"]);

        let path_alias = match via_alias.command.unwrap() {
            Command::Exec { action: ExecAction::Workflow { workflow, .. } } => workflow,
            _ => panic!("exec wf must parse as exec workflow"),
        };
        let path_full = match via_full.command.unwrap() {
            Command::Exec { action: ExecAction::Workflow { workflow, .. } } => workflow,
            _ => panic!("exec workflow must parse as exec workflow"),
        };
        assert_eq!(path_alias, path_full, "exec wf and exec workflow must produce the same result");
    }

    #[test]
    fn exec_wf_alias_with_work_item_flag() {
        let cli = parse(&["amux", "exec", "wf", "./wf.md", "--work-item", "0053"]);
        match cli.command.unwrap() {
            Command::Exec { action: ExecAction::Workflow { work_item, .. } } => {
                assert_eq!(work_item, Some("0053".to_string()),
                    "exec wf alias must accept --work-item flag");
            }
            _ => panic!("expected exec workflow via wf alias"),
        }
    }

    // ─── RemoteAction parsing (work item 0059) ──────────────────────────────

    #[test]
    fn remote_run_parses_command_and_follow_flag() {
        // --follow must come before the first positional arg because trailing_var_arg = true
        // causes clap to capture everything after the first positional into `command`.
        let cli = parse(&["amux", "remote", "run", "--follow", "execute", "prompt", "hello"]);
        match cli.command.unwrap() {
            Command::Remote {
                action: RemoteAction::Run { command, follow, session, remote_addr, .. },
            } => {
                assert_eq!(command, vec!["execute", "prompt", "hello"]);
                assert!(follow, "--follow must be true");
                assert!(session.is_none());
                assert!(remote_addr.is_none());
            }
            _ => panic!("expected remote run"),
        }
    }

    #[test]
    fn remote_run_short_follow_flag_f_is_accepted() {
        // -f must come before the first positional arg (trailing_var_arg = true).
        let cli = parse(&["amux", "remote", "run", "-f", "implement", "0042"]);
        match cli.command.unwrap() {
            Command::Remote { action: RemoteAction::Run { follow, command, .. } } => {
                assert!(follow, "-f must be accepted as short form of --follow");
                assert_eq!(command, vec!["implement", "0042"]);
            }
            _ => panic!("expected remote run"),
        }
    }

    #[test]
    fn remote_run_parses_remote_addr_and_session_flags() {
        let cli = parse(&[
            "amux", "remote", "run",
            "--remote-addr", "http://1.2.3.4:9876",
            "--session", "abc123",
            "implement", "0042",
        ]);
        match cli.command.unwrap() {
            Command::Remote {
                action: RemoteAction::Run { command, remote_addr, session, follow, .. },
            } => {
                assert_eq!(command, vec!["implement", "0042"]);
                assert_eq!(remote_addr.as_deref(), Some("http://1.2.3.4:9876"));
                assert_eq!(session.as_deref(), Some("abc123"));
                assert!(!follow);
            }
            _ => panic!("expected remote run"),
        }
    }

    #[test]
    fn remote_session_start_parses_with_dir() {
        let cli = parse(&["amux", "remote", "session", "start", "/workspace/proj"]);
        match cli.command.unwrap() {
            Command::Remote {
                action: RemoteAction::Session {
                    action: RemoteSessionAction::Start { dir, remote_addr, .. },
                },
            } => {
                assert_eq!(dir.as_deref(), Some("/workspace/proj"));
                assert!(remote_addr.is_none());
            }
            _ => panic!("expected remote session start"),
        }
    }

    #[test]
    fn remote_session_start_parses_with_no_args() {
        let cli = parse(&["amux", "remote", "session", "start"]);
        match cli.command.unwrap() {
            Command::Remote {
                action: RemoteAction::Session {
                    action: RemoteSessionAction::Start { dir, .. },
                },
            } => {
                assert!(dir.is_none(), "dir must be None when no arg given; got: {dir:?}");
            }
            _ => panic!("expected remote session start"),
        }
    }

    #[test]
    fn remote_session_kill_parses_with_no_session_id() {
        let cli = parse(&["amux", "remote", "session", "kill"]);
        match cli.command.unwrap() {
            Command::Remote {
                action: RemoteAction::Session {
                    action: RemoteSessionAction::Kill { session_id, .. },
                },
            } => {
                assert!(
                    session_id.is_none(),
                    "session_id must be None; got: {session_id:?}"
                );
            }
            _ => panic!("expected remote session kill"),
        }
    }

    #[test]
    fn remote_run_api_key_flag() {
        let cli = parse(&["amux", "remote", "run", "--api-key", "abc123", "echo", "hi"]);
        match cli.command.unwrap() {
            Command::Remote {
                action: RemoteAction::Run { api_key, .. },
            } => {
                assert_eq!(api_key.as_deref(), Some("abc123"), "api_key must be Some(\"abc123\")");
            }
            _ => panic!("expected remote run"),
        }
    }

    #[test]
    fn remote_session_start_api_key_flag() {
        let cli = parse(&["amux", "remote", "session", "start", "--api-key", "mykey"]);
        match cli.command.unwrap() {
            Command::Remote {
                action: RemoteAction::Session {
                    action: RemoteSessionAction::Start { api_key, .. },
                },
            } => {
                assert_eq!(api_key.as_deref(), Some("mykey"), "api_key must be Some(\"mykey\")");
            }
            _ => panic!("expected remote session start"),
        }
    }

    #[test]
    fn remote_session_kill_api_key_flag() {
        let cli = parse(&["amux", "remote", "session", "kill", "--api-key", "killkey"]);
        match cli.command.unwrap() {
            Command::Remote {
                action: RemoteAction::Session {
                    action: RemoteSessionAction::Kill { api_key, .. },
                },
            } => {
                assert_eq!(api_key.as_deref(), Some("killkey"), "api_key must be Some(\"killkey\")");
            }
            _ => panic!("expected remote session kill"),
        }
    }

    // ─── CLI/spec parity: remote flags ───────────────────────────────────────

    #[test]
    fn cli_spec_parity_remote_run() {
        use crate::commands::spec;
        use clap::CommandFactory;

        let mut cmd = Cli::command();
        let remote = cmd
            .find_subcommand_mut("remote")
            .expect("remote subcommand must exist in CLI");
        let run = remote
            .find_subcommand("run")
            .expect("remote run subcommand must exist in CLI");

        let cli_flags: Vec<String> = run
            .get_arguments()
            .filter_map(|a| a.get_long())
            .filter(|&name| name != "help")
            .map(str::to_string)
            .collect();

        let spec_flags: Vec<&str> = spec::REMOTE_RUN_FLAGS.iter().map(|f| f.name).collect();

        for flag in &cli_flags {
            assert!(
                spec_flags.contains(&flag.as_str()),
                "CLI flag --{flag} is missing from REMOTE_RUN_FLAGS in spec.rs; spec has: {spec_flags:?}"
            );
        }
        for flag in &spec_flags {
            assert!(
                cli_flags.contains(&flag.to_string()),
                "Spec flag --{flag} is missing from CLI `remote run`; CLI has: {cli_flags:?}"
            );
        }
    }

    #[test]
    fn cli_spec_parity_remote_session_start() {
        use crate::commands::spec;
        use clap::CommandFactory;

        let mut cmd = Cli::command();
        let remote = cmd
            .find_subcommand_mut("remote")
            .expect("remote subcommand must exist in CLI");
        let session = remote
            .find_subcommand_mut("session")
            .expect("remote session subcommand must exist in CLI");
        let start = session
            .find_subcommand("start")
            .expect("remote session start must exist in CLI");

        let cli_flags: Vec<String> = start
            .get_arguments()
            .filter_map(|a| a.get_long())
            .filter(|&name| name != "help")
            .map(str::to_string)
            .collect();

        let spec_flags: Vec<&str> =
            spec::REMOTE_SESSION_START_FLAGS.iter().map(|f| f.name).collect();

        for flag in &cli_flags {
            assert!(
                spec_flags.contains(&flag.as_str()),
                "CLI flag --{flag} is missing from REMOTE_SESSION_START_FLAGS; spec has: {spec_flags:?}"
            );
        }
        for flag in &spec_flags {
            assert!(
                cli_flags.contains(&flag.to_string()),
                "Spec flag --{flag} is missing from CLI `remote session start`; CLI has: {cli_flags:?}"
            );
        }
    }

    #[test]
    fn cli_spec_parity_remote_session_kill() {
        use crate::commands::spec;
        use clap::CommandFactory;

        let mut cmd = Cli::command();
        let remote = cmd
            .find_subcommand_mut("remote")
            .expect("remote subcommand must exist in CLI");
        let session = remote
            .find_subcommand_mut("session")
            .expect("remote session subcommand must exist in CLI");
        let kill = session
            .find_subcommand("kill")
            .expect("remote session kill must exist in CLI");

        let cli_flags: Vec<String> = kill
            .get_arguments()
            .filter_map(|a| a.get_long())
            .filter(|&name| name != "help")
            .map(str::to_string)
            .collect();

        let spec_flags: Vec<&str> =
            spec::REMOTE_SESSION_KILL_FLAGS.iter().map(|f| f.name).collect();

        for flag in &cli_flags {
            assert!(
                spec_flags.contains(&flag.as_str()),
                "CLI flag --{flag} is missing from REMOTE_SESSION_KILL_FLAGS; spec has: {spec_flags:?}"
            );
        }
        for flag in &spec_flags {
            assert!(
                cli_flags.contains(&flag.to_string()),
                "Spec flag --{flag} is missing from CLI `remote session kill`; CLI has: {cli_flags:?}"
            );
        }
    }

    // ── CLI/spec parity for exec prompt and exec workflow ────────────────────
    //
    // Verifies bidirectional coverage between the clap Arg definitions and the
    // FlagSpec constants in spec.rs so that autocomplete and CLI stay in sync.

    #[test]
    fn cli_spec_parity_exec_prompt() {
        use crate::commands::spec;
        use clap::CommandFactory;

        let mut cmd = Cli::command();
        let exec = cmd
            .find_subcommand_mut("exec")
            .expect("exec subcommand must exist in CLI");
        let prompt = exec
            .find_subcommand("prompt")
            .expect("exec prompt subcommand must exist in CLI");

        let cli_flags: Vec<String> = prompt
            .get_arguments()
            .filter_map(|a| a.get_long())
            .filter(|&name| name != "help")
            .map(str::to_string)
            .collect();

        let spec_flags: Vec<&str> = spec::EXEC_PROMPT_FLAGS.iter().map(|f| f.name).collect();

        for flag in &cli_flags {
            assert!(
                spec_flags.contains(&flag.as_str()),
                "CLI flag --{flag} is missing from EXEC_PROMPT_FLAGS in spec.rs; \
                 spec has: {spec_flags:?}"
            );
        }
        for flag in &spec_flags {
            assert!(
                cli_flags.contains(&flag.to_string()),
                "Spec flag --{flag} is missing from CLI `exec prompt` subcommand; \
                 CLI has: {cli_flags:?}"
            );
        }
    }

    #[test]
    fn cli_spec_parity_exec_workflow() {
        use crate::commands::spec;
        use clap::CommandFactory;

        let mut cmd = Cli::command();
        let exec = cmd
            .find_subcommand_mut("exec")
            .expect("exec subcommand must exist in CLI");
        let workflow = exec
            .find_subcommand("workflow")
            .expect("exec workflow subcommand must exist in CLI");

        let cli_flags: Vec<String> = workflow
            .get_arguments()
            .filter_map(|a| a.get_long())
            .filter(|&name| name != "help")
            .map(str::to_string)
            .collect();

        let spec_flags: Vec<&str> = spec::EXEC_WORKFLOW_FLAGS.iter().map(|f| f.name).collect();

        for flag in &cli_flags {
            assert!(
                spec_flags.contains(&flag.as_str()),
                "CLI flag --{flag} is missing from EXEC_WORKFLOW_FLAGS in spec.rs; \
                 spec has: {spec_flags:?}"
            );
        }
        for flag in &spec_flags {
            assert!(
                cli_flags.contains(&flag.to_string()),
                "Spec flag --{flag} is missing from CLI `exec workflow` subcommand; \
                 CLI has: {cli_flags:?}"
            );
        }
    }
}
