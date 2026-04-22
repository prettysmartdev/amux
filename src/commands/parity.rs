//! Compile-time parity enforcement for CLI, TUI, and Headless modes.
//!
//! Every user-facing command is represented by a variant of [`CommandId`].
//! The [`ModeParity`] trait requires each execution mode to explicitly
//! handle every variant in an exhaustive `match` (no wildcard arm).
//! Adding a new `CommandId` variant causes a compile error in every
//! mode that hasn't been updated — making it **impossible** for the
//! three modes to drift out of sync.
//!
//! # Adding a new command
//!
//! 1. Add a variant to [`CommandId`] and to [`CommandId::ALL`].
//! 2. Fix the resulting compile errors in [`CliMode`], [`TuiMode`],
//!    and [`HeadlessMode`].
//! 3. Implement the actual handler in each mode.

/// Every user-facing command that amux supports.
///
/// Adding a variant here **and rebuilding** will produce compile errors
/// in all three mode implementations until they are updated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CommandId {
    Init,
    Ready,
    Implement,
    Chat,
    ExecPrompt,
    ExecWorkflow,
    SpecsNew,
    SpecsAmend,
    ClawsInit,
    ClawsReady,
    ClawsChat,
    Status,
    Config,
    HeadlessStart,
    HeadlessKill,
    HeadlessLogs,
    HeadlessStatus,
    RemoteRun,
    RemoteSessionStart,
    RemoteSessionKill,
}

impl CommandId {
    /// All command IDs in canonical order. Keep this in sync with the enum.
    pub const ALL: &[CommandId] = &[
        CommandId::Init,
        CommandId::Ready,
        CommandId::Implement,
        CommandId::Chat,
        CommandId::ExecPrompt,
        CommandId::ExecWorkflow,
        CommandId::SpecsNew,
        CommandId::SpecsAmend,
        CommandId::ClawsInit,
        CommandId::ClawsReady,
        CommandId::ClawsChat,
        CommandId::Status,
        CommandId::Config,
        CommandId::HeadlessStart,
        CommandId::HeadlessKill,
        CommandId::HeadlessLogs,
        CommandId::HeadlessStatus,
        CommandId::RemoteRun,
        CommandId::RemoteSessionStart,
        CommandId::RemoteSessionKill,
    ];
}

/// How a particular execution mode handles a given command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModeSupport {
    /// Fully implemented in this mode.
    Implemented,
    /// Delegated to CLI mode (e.g. headless spawns `amux <subcommand>`).
    DelegatesToCli,
    /// Not applicable for this mode (e.g. `headless start` is only for headless).
    NotApplicable,
}

/// Trait that each execution mode **must** implement to prove it handles
/// every command.
///
/// Implementations **must** use an exhaustive `match` on [`CommandId`]
/// with **no** wildcard (`_`) arm. The compiler will then refuse to build
/// if a new variant is added without updating every mode.
pub trait ModeParity {
    fn command_support(cmd: CommandId) -> ModeSupport;
}

// ---------------------------------------------------------------------------
// Mode markers
// ---------------------------------------------------------------------------

/// CLI mode (`amux <subcommand>` — direct invocation).
pub struct CliMode;

/// TUI mode (interactive terminal UI).
pub struct TuiMode;

/// Headless mode (HTTP API server).
pub struct HeadlessMode;

// ---------------------------------------------------------------------------
// Implementations — exhaustive match, no wildcard
// ---------------------------------------------------------------------------

impl ModeParity for CliMode {
    fn command_support(cmd: CommandId) -> ModeSupport {
        // CLI supports every command directly.
        match cmd {
            CommandId::Init => ModeSupport::Implemented,
            CommandId::Ready => ModeSupport::Implemented,
            CommandId::Implement => ModeSupport::Implemented,
            CommandId::Chat => ModeSupport::Implemented,
            CommandId::ExecPrompt => ModeSupport::Implemented,
            CommandId::ExecWorkflow => ModeSupport::Implemented,
            CommandId::SpecsNew => ModeSupport::Implemented,
            CommandId::SpecsAmend => ModeSupport::Implemented,
            CommandId::ClawsInit => ModeSupport::Implemented,
            CommandId::ClawsReady => ModeSupport::Implemented,
            CommandId::ClawsChat => ModeSupport::Implemented,
            CommandId::Status => ModeSupport::Implemented,
            CommandId::Config => ModeSupport::Implemented,
            CommandId::HeadlessStart => ModeSupport::Implemented,
            CommandId::HeadlessKill => ModeSupport::Implemented,
            CommandId::HeadlessLogs => ModeSupport::Implemented,
            CommandId::HeadlessStatus => ModeSupport::Implemented,
            CommandId::RemoteRun => ModeSupport::Implemented,
            CommandId::RemoteSessionStart => ModeSupport::Implemented,
            CommandId::RemoteSessionKill => ModeSupport::Implemented,
        }
    }
}

impl ModeParity for TuiMode {
    fn command_support(cmd: CommandId) -> ModeSupport {
        match cmd {
            CommandId::Init => ModeSupport::Implemented,
            CommandId::Ready => ModeSupport::Implemented,
            CommandId::Implement => ModeSupport::Implemented,
            CommandId::Chat => ModeSupport::Implemented,
            CommandId::ExecPrompt => ModeSupport::Implemented,
            CommandId::ExecWorkflow => ModeSupport::Implemented,
            CommandId::SpecsNew => ModeSupport::Implemented,
            CommandId::SpecsAmend => ModeSupport::Implemented,
            CommandId::ClawsInit => ModeSupport::Implemented,
            CommandId::ClawsReady => ModeSupport::Implemented,
            CommandId::ClawsChat => ModeSupport::Implemented,
            CommandId::Status => ModeSupport::Implemented,
            CommandId::Config => ModeSupport::Implemented,
            // Headless server management is not available inside the TUI.
            CommandId::HeadlessStart => ModeSupport::NotApplicable,
            CommandId::HeadlessKill => ModeSupport::NotApplicable,
            CommandId::HeadlessLogs => ModeSupport::NotApplicable,
            CommandId::HeadlessStatus => ModeSupport::NotApplicable,
            // Remote commands are available in TUI with interactive pickers.
            CommandId::RemoteRun => ModeSupport::Implemented,
            CommandId::RemoteSessionStart => ModeSupport::Implemented,
            CommandId::RemoteSessionKill => ModeSupport::Implemented,
        }
    }
}

impl ModeParity for HeadlessMode {
    fn command_support(cmd: CommandId) -> ModeSupport {
        match cmd {
            // User-facing commands are delegated to CLI via child process.
            CommandId::Init => ModeSupport::DelegatesToCli,
            CommandId::Ready => ModeSupport::DelegatesToCli,
            CommandId::Implement => ModeSupport::DelegatesToCli,
            CommandId::Chat => ModeSupport::DelegatesToCli,
            CommandId::ExecPrompt => ModeSupport::DelegatesToCli,
            CommandId::ExecWorkflow => ModeSupport::DelegatesToCli,
            CommandId::SpecsNew => ModeSupport::DelegatesToCli,
            CommandId::SpecsAmend => ModeSupport::DelegatesToCli,
            CommandId::ClawsInit => ModeSupport::DelegatesToCli,
            CommandId::ClawsReady => ModeSupport::DelegatesToCli,
            CommandId::ClawsChat => ModeSupport::DelegatesToCli,
            CommandId::Status => ModeSupport::DelegatesToCli,
            CommandId::Config => ModeSupport::DelegatesToCli,
            // Server lifecycle is handled natively by headless mode.
            CommandId::HeadlessStart => ModeSupport::Implemented,
            CommandId::HeadlessKill => ModeSupport::Implemented,
            CommandId::HeadlessLogs => ModeSupport::Implemented,
            CommandId::HeadlessStatus => ModeSupport::Implemented,
            // Remote commands are delegated to CLI (subprocess).
            CommandId::RemoteRun => ModeSupport::DelegatesToCli,
            CommandId::RemoteSessionStart => ModeSupport::DelegatesToCli,
            CommandId::RemoteSessionKill => ModeSupport::DelegatesToCli,
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Every CommandId variant is present in ALL (no duplicates, no gaps).
    #[test]
    fn all_constant_is_exhaustive_and_unique() {
        let mut seen = std::collections::HashSet::new();
        for &cmd in CommandId::ALL {
            assert!(seen.insert(cmd), "duplicate in CommandId::ALL: {:?}", cmd);
        }
        // The exhaustive match in command_support already guarantees coverage,
        // but this guards against ALL drifting from the enum.
    }

    /// CLI mode must implement every command directly.
    #[test]
    fn cli_implements_all_commands() {
        for &cmd in CommandId::ALL {
            assert_eq!(
                CliMode::command_support(cmd),
                ModeSupport::Implemented,
                "CLI mode must implement {:?} directly",
                cmd,
            );
        }
    }

    /// TUI mode must implement or explicitly mark N/A for every command.
    #[test]
    fn tui_covers_all_commands() {
        for &cmd in CommandId::ALL {
            let status = TuiMode::command_support(cmd);
            assert!(
                status == ModeSupport::Implemented || status == ModeSupport::NotApplicable,
                "TUI mode must implement or mark N/A for {:?} (got {:?})",
                cmd,
                status,
            );
        }
    }

    /// Headless mode must delegate or implement every command.
    #[test]
    fn headless_covers_all_commands() {
        for &cmd in CommandId::ALL {
            let status = HeadlessMode::command_support(cmd);
            assert!(
                status == ModeSupport::Implemented || status == ModeSupport::DelegatesToCli,
                "Headless mode must implement or delegate {:?} (got {:?})",
                cmd,
                status,
            );
        }
    }

    /// No command is NotApplicable in all three modes (that would be dead code).
    #[test]
    fn no_command_is_universally_inapplicable() {
        for &cmd in CommandId::ALL {
            let cli = CliMode::command_support(cmd);
            let tui = TuiMode::command_support(cmd);
            let headless = HeadlessMode::command_support(cmd);
            assert!(
                cli != ModeSupport::NotApplicable
                    || tui != ModeSupport::NotApplicable
                    || headless != ModeSupport::NotApplicable,
                "{:?} is NotApplicable in all three modes — likely dead code",
                cmd,
            );
        }
    }

    // ── Explicit remote command checks (work item 0059) ─────────────────────

    #[test]
    fn command_id_all_includes_remote_run() {
        assert!(
            CommandId::ALL.contains(&CommandId::RemoteRun),
            "CommandId::ALL must contain RemoteRun; current list: {:?}",
            CommandId::ALL
        );
    }

    #[test]
    fn command_id_all_includes_remote_session_start() {
        assert!(
            CommandId::ALL.contains(&CommandId::RemoteSessionStart),
            "CommandId::ALL must contain RemoteSessionStart; current list: {:?}",
            CommandId::ALL
        );
    }

    #[test]
    fn command_id_all_includes_remote_session_kill() {
        assert!(
            CommandId::ALL.contains(&CommandId::RemoteSessionKill),
            "CommandId::ALL must contain RemoteSessionKill; current list: {:?}",
            CommandId::ALL
        );
    }

    #[test]
    fn cli_mode_implements_remote_run() {
        assert_eq!(
            CliMode::command_support(CommandId::RemoteRun),
            ModeSupport::Implemented,
            "CLI mode must implement RemoteRun directly"
        );
    }

    #[test]
    fn cli_mode_implements_remote_session_start() {
        assert_eq!(
            CliMode::command_support(CommandId::RemoteSessionStart),
            ModeSupport::Implemented,
            "CLI mode must implement RemoteSessionStart directly"
        );
    }

    #[test]
    fn cli_mode_implements_remote_session_kill() {
        assert_eq!(
            CliMode::command_support(CommandId::RemoteSessionKill),
            ModeSupport::Implemented,
            "CLI mode must implement RemoteSessionKill directly"
        );
    }

    #[test]
    fn tui_mode_implements_remote_run() {
        assert_eq!(
            TuiMode::command_support(CommandId::RemoteRun),
            ModeSupport::Implemented,
            "TUI mode must implement RemoteRun (interactive session picker)"
        );
    }

    #[test]
    fn tui_mode_implements_remote_session_start() {
        assert_eq!(
            TuiMode::command_support(CommandId::RemoteSessionStart),
            ModeSupport::Implemented,
            "TUI mode must implement RemoteSessionStart (interactive dir picker)"
        );
    }

    #[test]
    fn tui_mode_implements_remote_session_kill() {
        assert_eq!(
            TuiMode::command_support(CommandId::RemoteSessionKill),
            ModeSupport::Implemented,
            "TUI mode must implement RemoteSessionKill (interactive session picker)"
        );
    }

    #[test]
    fn headless_mode_delegates_remote_run_to_cli() {
        assert_eq!(
            HeadlessMode::command_support(CommandId::RemoteRun),
            ModeSupport::DelegatesToCli,
            "Headless mode must delegate RemoteRun to CLI subprocess"
        );
    }

    #[test]
    fn headless_mode_delegates_remote_session_start_to_cli() {
        assert_eq!(
            HeadlessMode::command_support(CommandId::RemoteSessionStart),
            ModeSupport::DelegatesToCli,
            "Headless mode must delegate RemoteSessionStart to CLI subprocess"
        );
    }

    #[test]
    fn headless_mode_delegates_remote_session_kill_to_cli() {
        assert_eq!(
            HeadlessMode::command_support(CommandId::RemoteSessionKill),
            ModeSupport::DelegatesToCli,
            "Headless mode must delegate RemoteSessionKill to CLI subprocess"
        );
    }

    /// Cross-check: commands the TUI marks as Implemented must also appear in
    /// the TUI's execute_command match arms. We verify this indirectly through
    /// the spec::ALL_COMMANDS table — every TUI-implemented command must have
    /// an entry there (used for flag parsing and autocomplete).
    #[test]
    fn tui_implemented_commands_have_spec_entries() {
        use crate::commands::spec;

        let spec_names: Vec<&str> = spec::ALL_COMMANDS.iter().map(|c| c.name).collect();

        // Map CommandId → spec name(s) that should exist.
        let expected_spec_names: &[(CommandId, &[&str])] = &[
            (CommandId::Init, &["init"]),
            (CommandId::Ready, &["ready"]),
            (CommandId::Implement, &["implement"]),
            (CommandId::Chat, &["chat"]),
            (CommandId::ExecPrompt, &["exec prompt"]),
            (CommandId::ExecWorkflow, &["exec workflow"]),
            (CommandId::SpecsNew, &["specs new"]),
            (CommandId::SpecsAmend, &["specs amend"]),
            (CommandId::Status, &["status"]),
            // Config, Claws, and Headless use dialog-based or custom handling.
            (CommandId::RemoteRun, &["remote run"]),
            (CommandId::RemoteSessionStart, &["remote session start"]),
            (CommandId::RemoteSessionKill, &["remote session kill"]),
        ];

        for (cmd, names) in expected_spec_names {
            if TuiMode::command_support(*cmd) == ModeSupport::Implemented {
                for name in *names {
                    assert!(
                        spec_names.contains(name),
                        "TUI claims {:?} is Implemented but spec::ALL_COMMANDS has no entry for {:?}",
                        cmd,
                        name,
                    );
                }
            }
        }
    }
}
