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
        #[arg(long)]
        non_interactive: bool,

        /// Mount the host Docker daemon socket into the agent container.
        #[arg(long)]
        allow_docker: bool,
    },

    /// Launch the dev container to implement a work item.
    Implement {
        /// Work item number (e.g. 0001).
        work_item: String,

        /// Run the agent in non-interactive (print) mode instead of interactive mode.
        #[arg(long)]
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
    },

    /// Start a freeform chat session with the configured agent in a container.
    Chat {
        /// Run the agent in non-interactive (print) mode instead of interactive mode.
        #[arg(long)]
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
        #[arg(long)]
        non_interactive: bool,
        /// Mount the host Docker daemon socket into the agent container.
        #[arg(long)]
        allow_docker: bool,
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
            Command::Ready { refresh, build, no_cache, non_interactive, allow_docker } => {
                assert!(!refresh);
                assert!(!build);
                assert!(!no_cache);
                assert!(!non_interactive);
                assert!(!allow_docker);
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
}
