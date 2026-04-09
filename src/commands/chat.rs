use crate::commands::agent::run_agent_with_sink;
use crate::commands::auth::resolve_auth;
use crate::commands::implement::confirm_mount_scope_stdin;
use crate::commands::init::find_git_root;
use crate::commands::output::OutputSink;
use crate::config::{effective_yolo_disallowed_tools, load_repo_config};
use crate::docker;
use anyhow::{Context, Result};
use std::path::PathBuf;

/// Command-mode entry point for `amux chat`.
pub async fn run(non_interactive: bool, plan: bool, allow_docker: bool, mount_ssh: bool, yolo: bool, runtime: std::sync::Arc<dyn crate::runtime::AgentRuntime>) -> Result<()> {
    let git_root = find_git_root().context("Not inside a Git repository")?;
    let mount_path = confirm_mount_scope_stdin(&git_root)?;
    let credentials = resolve_auth(&git_root, agent_name(&git_root)?)?;
    let config = load_repo_config(&git_root)?;
    let agent = config.agent.as_deref().unwrap_or("claude");
    let host_settings = docker::HostSettings::prepare(agent)
        .or_else(|| docker::HostSettings::prepare_minimal(agent));

    run_with_sink(
        &OutputSink::Stdout,
        Some(mount_path),
        credentials.env_vars.clone(),
        non_interactive,
        plan,
        host_settings.as_ref(),
        allow_docker,
        mount_ssh,
        yolo,
        &*runtime,
    )
    .await
}

/// Core logic shared between command mode and TUI mode.
///
/// `mount_override`: when `Some`, skip the interactive stdin prompt and use this path.
/// `env_vars`: agent credential env vars to pass into the container.
/// `non_interactive`: when true, launch agent in print/non-interactive mode.
/// `plan`: when true, launch agent in plan (read-only) mode.
/// `allow_docker`: when true, mount the host Docker daemon socket into the container.
/// `mount_ssh`: when true, mount the host `~/.ssh` directory read-only into the container.
/// `yolo`: when true, append agent-specific skip-permissions flags and disallowed-tools config.
#[allow(clippy::too_many_arguments)]
pub async fn run_with_sink(
    out: &OutputSink,
    mount_override: Option<PathBuf>,
    env_vars: Vec<(String, String)>,
    non_interactive: bool,
    plan: bool,
    host_settings: Option<&docker::HostSettings>,
    allow_docker: bool,
    mount_ssh: bool,
    yolo: bool,
    runtime: &dyn crate::runtime::AgentRuntime,
) -> Result<()> {
    let git_root = find_git_root().context("Not inside a Git repository")?;
    let config = load_repo_config(&git_root)?;
    let agent = config.agent.as_deref().unwrap_or("claude").to_string();

    let mut entrypoint = if non_interactive {
        chat_entrypoint_non_interactive(&agent, plan)
    } else {
        chat_entrypoint(&agent, plan)
    };

    let disallowed_tools = if yolo { effective_yolo_disallowed_tools(&git_root) } else { vec![] };
    append_yolo_flags(&mut entrypoint, &agent, yolo, &disallowed_tools);

    run_agent_with_sink(
        entrypoint,
        &format!("Starting chat session with agent '{}'", agent),
        out,
        mount_override,
        env_vars,
        non_interactive,
        host_settings,
        allow_docker,
        mount_ssh,
        None,
        runtime,
    )
    .await
}

fn agent_name(git_root: &PathBuf) -> Result<&'static str> {
    let config = load_repo_config(git_root)?;
    Ok(match config.agent.as_deref().unwrap_or("claude") {
        "codex" => "codex",
        "opencode" => "opencode",
        _ => "claude",
    })
}

/// Build the entrypoint command for a chat session (interactive, no prompt).
pub fn chat_entrypoint(agent: &str, plan: bool) -> Vec<String> {
    let mut args = match agent {
        "claude" => vec!["claude".to_string()],
        "codex" => vec!["codex".to_string()],
        "opencode" => vec!["opencode".to_string()],
        _ => vec![agent.to_string()],
    };
    append_plan_flags(&mut args, agent, plan);
    args
}

/// Build the entrypoint command for a chat session in non-interactive mode.
pub fn chat_entrypoint_non_interactive(agent: &str, plan: bool) -> Vec<String> {
    let mut args = match agent {
        "claude" => vec!["claude".to_string(), "-p".to_string()],
        "codex" => vec!["codex".to_string(), "--quiet".to_string()],
        "opencode" => vec!["opencode".to_string()],
        _ => vec![agent.to_string()],
    };
    append_plan_flags(&mut args, agent, plan);
    args
}

/// Append agent-specific plan mode flags to the argument list.
///
/// - Claude: `--permission-mode plan`
/// - Codex: `--approval-mode plan`
/// - Opencode: no plan mode available (flag is silently ignored)
fn append_plan_flags(args: &mut Vec<String>, agent: &str, plan: bool) {
    if !plan {
        return;
    }
    match agent {
        "claude" => {
            args.push("--permission-mode".to_string());
            args.push("plan".to_string());
        }
        "codex" => {
            args.push("--approval-mode".to_string());
            args.push("plan".to_string());
        }
        // Opencode and unknown agents have no plan mode.
        _ => {}
    }
}

/// Append agent-specific yolo (skip-permissions) flags and disallowed-tools config.
///
/// - Claude: `--dangerously-skip-permissions`; if disallowed_tools non-empty, `--disallowedTools <t1>,<t2>,...`
/// - Codex: `--full-auto`; disallowed tools not supported (warning printed)
/// - Opencode: no equivalent — a warning is printed; disallowed tools not supported
pub fn append_yolo_flags(args: &mut Vec<String>, agent: &str, yolo: bool, disallowed_tools: &[String]) {
    if !yolo {
        return;
    }
    match agent {
        "claude" => {
            args.push("--dangerously-skip-permissions".to_string());
            if !disallowed_tools.is_empty() {
                args.push("--disallowedTools".to_string());
                args.push(disallowed_tools.join(","));
            }
        }
        "codex" => {
            args.push("--full-auto".to_string());
            if !disallowed_tools.is_empty() {
                eprintln!("WARNING: --yolo: codex does not support --disallowedTools; yoloDisallowedTools config will be ignored.");
            }
        }
        _ => {
            // Opencode and unknown agents have no skip-permissions equivalent.
            eprintln!("WARNING: --yolo: agent '{}' does not support a skip-permissions flag; proceeding without it.", agent);
            if !disallowed_tools.is_empty() {
                eprintln!("WARNING: --yolo: agent '{}' does not support --disallowedTools; yoloDisallowedTools config will be ignored.", agent);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chat_entrypoint_claude() {
        let args = chat_entrypoint("claude", false);
        assert_eq!(args.len(), 1);
        assert_eq!(args[0], "claude");
    }

    #[test]
    fn chat_entrypoint_codex() {
        let args = chat_entrypoint("codex", false);
        assert_eq!(args.len(), 1);
        assert_eq!(args[0], "codex");
    }

    #[test]
    fn chat_entrypoint_opencode() {
        let args = chat_entrypoint("opencode", false);
        assert_eq!(args.len(), 1);
        assert_eq!(args[0], "opencode");
    }

    #[test]
    fn chat_entrypoint_unknown_agent() {
        let args = chat_entrypoint("custom", false);
        assert_eq!(args.len(), 1);
        assert_eq!(args[0], "custom");
    }

    #[test]
    fn chat_entrypoint_non_interactive_claude() {
        let args = chat_entrypoint_non_interactive("claude", false);
        assert_eq!(args.len(), 2);
        assert_eq!(args[0], "claude");
        assert_eq!(args[1], "-p");
    }

    #[test]
    fn chat_entrypoint_non_interactive_codex() {
        let args = chat_entrypoint_non_interactive("codex", false);
        assert_eq!(args.len(), 2);
        assert_eq!(args[0], "codex");
        assert_eq!(args[1], "--quiet");
    }

    #[test]
    fn chat_entrypoint_non_interactive_opencode() {
        let args = chat_entrypoint_non_interactive("opencode", false);
        assert_eq!(args.len(), 1);
        assert_eq!(args[0], "opencode");
    }

    #[test]
    fn chat_entrypoint_has_no_prompt() {
        for agent in &["claude", "codex", "opencode"] {
            let args = chat_entrypoint(agent, false);
            // Chat should have no prompt argument — just the agent command.
            for arg in &args {
                assert!(
                    !arg.contains("Implement"),
                    "Chat entrypoint for {} should not contain a prompt, found: {}",
                    agent,
                    arg
                );
            }
        }
    }

    #[test]
    fn chat_entrypoint_non_interactive_has_no_prompt() {
        for agent in &["claude", "codex", "opencode"] {
            let args = chat_entrypoint_non_interactive(agent, false);
            for arg in &args {
                assert!(
                    !arg.contains("Implement"),
                    "Chat non-interactive entrypoint for {} should not contain a prompt, found: {}",
                    agent,
                    arg
                );
            }
        }
    }

    // --- Plan mode tests ---

    #[test]
    fn chat_entrypoint_plan_claude() {
        let args = chat_entrypoint("claude", true);
        assert_eq!(args, vec!["claude", "--permission-mode", "plan"]);
    }

    #[test]
    fn chat_entrypoint_plan_codex() {
        let args = chat_entrypoint("codex", true);
        assert_eq!(args, vec!["codex", "--approval-mode", "plan"]);
    }

    #[test]
    fn chat_entrypoint_plan_opencode() {
        // Opencode has no plan mode; flag is silently ignored.
        let args = chat_entrypoint("opencode", true);
        assert_eq!(args, vec!["opencode"]);
    }

    #[test]
    fn chat_entrypoint_plan_unknown_agent() {
        // Unknown agents have no plan mode; flag is silently ignored.
        let args = chat_entrypoint("custom", true);
        assert_eq!(args, vec!["custom"]);
    }

    #[test]
    fn chat_entrypoint_non_interactive_plan_claude() {
        let args = chat_entrypoint_non_interactive("claude", true);
        assert_eq!(args, vec!["claude", "-p", "--permission-mode", "plan"]);
    }

    #[test]
    fn chat_entrypoint_non_interactive_plan_codex() {
        let args = chat_entrypoint_non_interactive("codex", true);
        assert_eq!(args, vec!["codex", "--quiet", "--approval-mode", "plan"]);
    }

    #[test]
    fn chat_entrypoint_non_interactive_plan_opencode() {
        let args = chat_entrypoint_non_interactive("opencode", true);
        assert_eq!(args, vec!["opencode"]);
    }

    // --- append_yolo_flags tests ---

    #[test]
    fn append_yolo_flags_noop_when_yolo_false() {
        let mut args = vec!["claude".to_string()];
        append_yolo_flags(&mut args, "claude", false, &[]);
        assert_eq!(args, vec!["claude"]);
    }

    #[test]
    fn append_yolo_flags_claude_adds_skip_permissions() {
        let mut args = vec!["claude".to_string()];
        append_yolo_flags(&mut args, "claude", true, &[]);
        assert!(
            args.contains(&"--dangerously-skip-permissions".to_string()),
            "claude must receive --dangerously-skip-permissions"
        );
    }

    #[test]
    fn append_yolo_flags_claude_no_disallowed_tools_skips_flag() {
        let mut args = vec!["claude".to_string()];
        append_yolo_flags(&mut args, "claude", true, &[]);
        assert!(
            !args.contains(&"--disallowedTools".to_string()),
            "--disallowedTools must not appear when the list is empty"
        );
    }

    #[test]
    fn append_yolo_flags_claude_with_disallowed_tools() {
        let mut args = vec!["claude".to_string()];
        let tools = vec!["Bash".to_string(), "computer".to_string()];
        append_yolo_flags(&mut args, "claude", true, &tools);
        let dt_idx = args
            .iter()
            .position(|a| a == "--disallowedTools")
            .expect("--disallowedTools flag missing");
        assert_eq!(args[dt_idx + 1], "Bash,computer");
    }

    #[test]
    fn append_yolo_flags_codex_adds_full_auto() {
        let mut args = vec!["codex".to_string()];
        append_yolo_flags(&mut args, "codex", true, &[]);
        assert!(args.contains(&"--full-auto".to_string()));
        assert!(!args.contains(&"--dangerously-skip-permissions".to_string()));
    }

    #[test]
    fn append_yolo_flags_codex_no_disallowed_tools_flag() {
        // codex does not support --disallowedTools; the flag must never appear
        let mut args = vec!["codex".to_string()];
        let tools = vec!["Bash".to_string()];
        append_yolo_flags(&mut args, "codex", true, &tools);
        assert!(!args.contains(&"--disallowedTools".to_string()));
    }

    #[test]
    fn append_yolo_flags_opencode_no_skip_permissions_flag() {
        // opencode has no skip-permissions equivalent; args must be unchanged
        let mut args = vec!["opencode".to_string()];
        append_yolo_flags(&mut args, "opencode", true, &[]);
        assert_eq!(args, vec!["opencode"]);
    }

    #[test]
    fn append_yolo_flags_opencode_no_disallowed_tools_flag() {
        let mut args = vec!["opencode".to_string()];
        let tools = vec!["Bash".to_string()];
        append_yolo_flags(&mut args, "opencode", true, &tools);
        assert!(!args.contains(&"--disallowedTools".to_string()));
        assert_eq!(args, vec!["opencode"]);
    }
}
