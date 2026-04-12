use crate::commands::init::find_git_root;
use crate::commands::output::OutputSink;
use crate::config::load_repo_config;
use crate::runtime::docker as docker;
use anyhow::{Context, Result};
use std::path::PathBuf;
use dirs;

/// Shared logic for launching a containerized agent session.
///
/// Used by both `implement` (with a pre-configured prompt) and `chat` (no prompt).
///
/// `entrypoint`: the Docker entrypoint command (agent + optional prompt).
/// `status_message`: displayed to the user before launching.
/// `mount_override`: when `Some`, skip the interactive stdin prompt and use this path.
/// `env_vars`: agent credential env vars to pass into the container.
/// `non_interactive`: when true, launch agent in print/non-interactive mode.
/// `allow_docker`: when true, mount the host Docker daemon socket into the container.
/// `mount_ssh`: when true, mount the host `~/.ssh` directory read-only into the container.
pub async fn run_agent_with_sink(
    entrypoint: Vec<String>,
    status_message: &str,
    out: &OutputSink,
    mount_override: Option<PathBuf>,
    env_vars: Vec<(String, String)>,
    non_interactive: bool,
    host_settings: Option<&docker::HostSettings>,
    allow_docker: bool,
    mount_ssh: bool,
    container_name_override: Option<String>,
    runtime: &dyn crate::runtime::AgentRuntime,
) -> Result<()> {
    let git_root = find_git_root().context("Not inside a Git repository")?;
    let config = load_repo_config(&git_root)?;
    let agent = config.agent.as_deref().unwrap_or("claude").to_string();

    out.println(status_message);

    let mount_path = match mount_override {
        Some(p) => p,
        None => crate::commands::implement::confirm_mount_scope_stdin(&git_root)?,
    };

    // If --allow-docker, check the socket and print a warning before launching.
    if allow_docker {
        let socket_path = docker::check_docker_socket()
            .context("Cannot mount Docker socket")?;
        out.println(format!("{} socket: {} (found)", runtime.name(), socket_path.display()));
        out.println(format!(
            "WARNING: --allow-docker: mounting host Docker socket into container ({}:{}). \
             This grants the agent elevated host access.",
            socket_path.display(),
            socket_path.display()
        ));
    }

    // If --allow-ssh, resolve ~/.ssh, validate it exists, and warn before launching.
    let ssh_dir: Option<PathBuf> = if mount_ssh {
        let home = dirs::home_dir()
            .ok_or_else(|| anyhow::anyhow!("Cannot resolve home directory"))?;
        let ssh = home.join(".ssh");
        if !ssh.exists() {
            anyhow::bail!("Host ~/.ssh directory not found; cannot use --mount-ssh");
        }
        out.println(
            "WARNING: --mount-ssh: mounting host ~/.ssh into container (read-only). \
             SSH keys with incorrect permissions may be rejected by git inside the container — \
             verify host key permissions (e.g. chmod 600 ~/.ssh/id_*). \
             Ensure you trust the agent image."
                .to_string(),
        );
        Some(ssh)
    } else {
        None
    };

    let image_tag = docker::project_image_tag(&git_root);
    let entrypoint_refs: Vec<&str> = entrypoint.iter().map(String::as_str).collect();

    // Detect the last USER directive in Dockerfile.dev and update settings mounts
    // to target the correct home directory inside the container.
    let modified_settings: Option<crate::runtime::HostSettings> = host_settings.and_then(|settings| {
        let mut new_settings = settings.clone_view();
        let dockerfile = git_root.join("Dockerfile.dev");
        if let Some(msg) = crate::runtime::apply_dockerfile_user(&mut new_settings, &dockerfile) {
            out.println(msg);
            Some(new_settings)
        } else {
            None
        }
    });
    let effective_settings: Option<&crate::runtime::HostSettings> =
        modified_settings.as_ref().or(host_settings);

    // Show the full runtime CLI command being run (with masked env values).
    let display_args = runtime.build_run_args_display(
        &image_tag,
        mount_path.to_str().unwrap(),
        &entrypoint_refs,
        &env_vars,
        effective_settings,
        allow_docker,
        container_name_override.as_deref(),
        ssh_dir.as_deref(),
    );
    out.println(format!("$ {} {}", runtime.cli_binary(), display_args.join(" ")));

    if !non_interactive {
        crate::commands::ready::print_interactive_notice(out, &agent);
    } else {
        out.println("Tip: remove --non-interactive to interact with the agent directly.");
    }

    if non_interactive {
        let (_cmd, output) = runtime.run_container_captured(
            &image_tag,
            mount_path.to_str().unwrap(),
            &entrypoint_refs,
            &env_vars,
            effective_settings,
            allow_docker,
            container_name_override.as_deref(),
            ssh_dir.as_deref(),
        )
        .context("Container exited with an error")?;
        for line in output.lines() {
            out.println(line);
        }
    } else {
        runtime.run_container(
            &image_tag,
            mount_path.to_str().unwrap(),
            &entrypoint_refs,
            &env_vars,
            effective_settings,
            allow_docker,
            container_name_override.as_deref(),
            ssh_dir.as_deref(),
        )
        .context("Container exited with an error")?;
    }

    Ok(())
}

/// Append agent-specific autonomous-mode flags and disallowed-tools config.
///
/// When `yolo` is true:
/// - Claude: `--dangerously-skip-permissions`
/// - Gemini: `--yolo` (gemini's own flag; skips all tool-call confirmations)
/// When `auto` is true (and not yolo):
/// - Claude: `--permission-mode auto`
/// - Gemini: `--approval-mode=auto_edit` (auto-approves file edits/writes; prompts for shell tools)
/// Both modes:
/// - Claude: if disallowed_tools non-empty, `--disallowedTools <t1>,<t2>,...`
/// - Codex: `--full-auto`; disallowed tools not supported (warning printed)
/// - Opencode: no equivalent — a warning is printed; disallowed tools not supported
/// - Maki: `--yolo` (maki's own flag to skip all permission prompts); disallowed tools not supported
/// - Gemini: disallowed tools not supported (warning printed)
pub fn append_autonomous_flags(args: &mut Vec<String>, agent: &str, yolo: bool, auto: bool, disallowed_tools: &[String]) {
    if !yolo && !auto {
        return;
    }
    let flag_name = if yolo { "--yolo" } else { "--auto" };
    match agent {
        "claude" => {
            if yolo {
                args.push("--dangerously-skip-permissions".to_string());
            } else {
                args.push("--permission-mode".to_string());
                args.push("auto".to_string());
            }
            if !disallowed_tools.is_empty() {
                args.push("--disallowedTools".to_string());
                args.push(disallowed_tools.join(","));
            }
        }
        "codex" => {
            args.push("--full-auto".to_string());
            if !disallowed_tools.is_empty() {
                eprintln!("WARNING: {}: codex does not support --disallowedTools; yoloDisallowedTools config will be ignored.", flag_name);
            }
        }
        "maki" => {
            // maki uses --yolo as its own autonomous flag (skips all permission prompts).
            // Note: the --yolo flag here is maki's flag, not amux's --yolo flag.
            args.push("--yolo".to_string());
            if !disallowed_tools.is_empty() {
                eprintln!(
                    "WARNING: {}: maki does not support --disallowedTools; yoloDisallowedTools config will be ignored.",
                    flag_name
                );
            }
        }
        "gemini" => {
            if yolo {
                // gemini's --yolo skips all tool-call confirmations.
                // Note: this is gemini's own flag, not amux's --yolo flag.
                args.push("--yolo".to_string());
            } else {
                // --auto maps to gemini's auto_edit approval mode (auto-approves file
                // edits/writes but prompts before shell tool calls — more conservative
                // than --yolo).
                args.push("--approval-mode=auto_edit".to_string());
            }
            if !disallowed_tools.is_empty() {
                eprintln!(
                    "WARNING: {}: gemini does not support --disallowedTools; yoloDisallowedTools config will be ignored.",
                    flag_name
                );
            }
        }
        _ => {
            // Opencode and unknown agents have no skip-permissions equivalent.
            eprintln!("WARNING: {}: agent '{}' does not support a skip-permissions flag; proceeding without it.", flag_name, agent);
            if !disallowed_tools.is_empty() {
                eprintln!("WARNING: {}: agent '{}' does not support --disallowedTools; yoloDisallowedTools config will be ignored.", flag_name, agent);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc::unbounded_channel;

    // --- append_autonomous_flags tests ---

    #[test]
    fn append_autonomous_flags_noop_when_yolo_false() {
        let mut args = vec!["claude".to_string()];
        append_autonomous_flags(&mut args, "claude", false, false, &[]);
        assert_eq!(args, vec!["claude"]);
    }

    #[test]
    fn append_autonomous_flags_claude_adds_skip_permissions() {
        let mut args = vec!["claude".to_string()];
        append_autonomous_flags(&mut args, "claude", true, false, &[]);
        assert!(
            args.contains(&"--dangerously-skip-permissions".to_string()),
            "claude must receive --dangerously-skip-permissions"
        );
    }

    #[test]
    fn append_autonomous_flags_claude_no_disallowed_tools_skips_flag() {
        let mut args = vec!["claude".to_string()];
        append_autonomous_flags(&mut args, "claude", true, false, &[]);
        assert!(
            !args.contains(&"--disallowedTools".to_string()),
            "--disallowedTools must not appear when the list is empty"
        );
    }

    #[test]
    fn append_autonomous_flags_claude_with_disallowed_tools() {
        let mut args = vec!["claude".to_string()];
        let tools = vec!["Bash".to_string(), "computer".to_string()];
        append_autonomous_flags(&mut args, "claude", true, false, &tools);
        let dt_idx = args
            .iter()
            .position(|a| a == "--disallowedTools")
            .expect("--disallowedTools flag missing");
        assert_eq!(args[dt_idx + 1], "Bash,computer");
    }

    #[test]
    fn append_autonomous_flags_codex_adds_full_auto() {
        let mut args = vec!["codex".to_string()];
        append_autonomous_flags(&mut args, "codex", true, false, &[]);
        assert!(args.contains(&"--full-auto".to_string()));
        assert!(!args.contains(&"--dangerously-skip-permissions".to_string()));
    }

    #[test]
    fn append_autonomous_flags_codex_no_disallowed_tools_flag() {
        // codex does not support --disallowedTools; the flag must never appear
        let mut args = vec!["codex".to_string()];
        let tools = vec!["Bash".to_string()];
        append_autonomous_flags(&mut args, "codex", true, false, &tools);
        assert!(!args.contains(&"--disallowedTools".to_string()));
    }

    #[test]
    fn append_autonomous_flags_opencode_no_skip_permissions_flag() {
        // opencode has no skip-permissions equivalent; args must be unchanged
        let mut args = vec!["opencode".to_string()];
        append_autonomous_flags(&mut args, "opencode", true, false, &[]);
        assert_eq!(args, vec!["opencode"]);
    }

    #[test]
    fn append_autonomous_flags_opencode_no_disallowed_tools_flag() {
        let mut args = vec!["opencode".to_string()];
        let tools = vec!["Bash".to_string()];
        append_autonomous_flags(&mut args, "opencode", true, false, &tools);
        assert!(!args.contains(&"--disallowedTools".to_string()));
        assert_eq!(args, vec!["opencode"]);
    }

    #[test]
    fn append_autonomous_flags_noop_when_both_false() {
        let mut args = vec!["claude".to_string()];
        append_autonomous_flags(&mut args, "claude", false, false, &[]);
        assert_eq!(args, vec!["claude"]);
    }

    #[test]
    fn append_autonomous_flags_auto_claude_adds_permission_mode_auto() {
        let mut args = vec!["claude".to_string()];
        append_autonomous_flags(&mut args, "claude", false, true, &[]);
        assert!(
            args.contains(&"--permission-mode".to_string()),
            "claude in auto mode must receive --permission-mode"
        );
        assert!(args.contains(&"auto".to_string()), "auto value must be present");
        assert!(
            !args.contains(&"--dangerously-skip-permissions".to_string()),
            "--dangerously-skip-permissions must NOT appear in auto mode"
        );
    }

    #[test]
    fn append_autonomous_flags_auto_claude_with_disallowed_tools() {
        let mut args = vec!["claude".to_string()];
        let tools = vec!["Bash".to_string()];
        append_autonomous_flags(&mut args, "claude", false, true, &tools);
        assert!(args.contains(&"--disallowedTools".to_string()));
    }

    #[test]
    fn append_autonomous_flags_yolo_takes_precedence_over_auto() {
        // When both are true, yolo wins (uses --dangerously-skip-permissions).
        let mut args = vec!["claude".to_string()];
        append_autonomous_flags(&mut args, "claude", true, true, &[]);
        assert!(args.contains(&"--dangerously-skip-permissions".to_string()));
        assert!(!args.contains(&"auto".to_string()));
    }

    #[test]
    fn append_autonomous_flags_maki_adds_yolo_flag() {
        let mut args = vec!["maki".to_string()];
        append_autonomous_flags(&mut args, "maki", true, false, &[]);
        assert!(args.contains(&"--yolo".to_string()), "maki must receive --yolo in yolo mode");
    }

    #[test]
    fn append_autonomous_flags_maki_never_adds_disallowed_tools_flag() {
        // maki does not support --disallowedTools; it must never appear regardless of the list.
        let mut args = vec!["maki".to_string()];
        let tools = vec!["Bash".to_string(), "computer".to_string()];
        append_autonomous_flags(&mut args, "maki", true, false, &tools);
        assert!(
            !args.contains(&"--disallowedTools".to_string()),
            "--disallowedTools must never appear for maki"
        );
        assert!(args.contains(&"--yolo".to_string()), "--yolo must still be appended");
    }

    #[test]
    fn append_autonomous_flags_maki_prints_warning_when_disallowed_tools_nonempty() {
        // The warning is emitted via eprintln! and cannot be trivially captured in a unit test
        // without a custom stderr-redirect harness. This test verifies the code path compiles
        // and does not panic.
        let mut args = vec!["maki".to_string()];
        let tools = vec!["Bash".to_string()];
        append_autonomous_flags(&mut args, "maki", true, false, &tools);
    }

    #[test]
    fn append_autonomous_flags_maki_no_disallowed_tools_exact_args() {
        // When disallowed_tools is empty, exactly ["maki", "--yolo"] must result.
        let mut args = vec!["maki".to_string()];
        append_autonomous_flags(&mut args, "maki", true, false, &[]);
        assert_eq!(args, vec!["maki", "--yolo"]);
    }

    // --- gemini autonomous flags ---

    #[test]
    fn append_autonomous_flags_gemini_yolo_adds_yolo_flag() {
        let mut args = vec!["gemini".to_string()];
        append_autonomous_flags(&mut args, "gemini", true, false, &[]);
        assert!(args.contains(&"--yolo".to_string()), "gemini must receive --yolo in yolo mode");
    }

    #[test]
    fn append_autonomous_flags_gemini_yolo_never_adds_disallowed_tools_flag() {
        // gemini does not support --disallowedTools; the flag must never appear.
        let mut args = vec!["gemini".to_string()];
        let tools = vec!["Bash".to_string(), "computer".to_string()];
        append_autonomous_flags(&mut args, "gemini", true, false, &tools);
        assert!(
            !args.contains(&"--disallowedTools".to_string()),
            "--disallowedTools must never appear for gemini"
        );
        assert!(args.contains(&"--yolo".to_string()), "--yolo must still be appended");
    }

    #[test]
    fn append_autonomous_flags_gemini_auto_adds_approval_mode_auto_edit() {
        let mut args = vec!["gemini".to_string()];
        append_autonomous_flags(&mut args, "gemini", false, true, &[]);
        assert!(
            args.contains(&"--approval-mode=auto_edit".to_string()),
            "gemini in auto mode must receive --approval-mode=auto_edit"
        );
        assert!(
            !args.contains(&"--dangerously-skip-permissions".to_string()),
            "--dangerously-skip-permissions must NOT appear for gemini"
        );
        assert!(
            !args.contains(&"--yolo".to_string()),
            "--yolo must NOT appear in auto mode"
        );
    }

    #[test]
    fn append_autonomous_flags_gemini_yolo_with_nonempty_disallowed_tools_prints_warning() {
        // Warning is emitted via eprintln! — verify the code path compiles and does not panic.
        let mut args = vec!["gemini".to_string()];
        let tools = vec!["Bash".to_string()];
        append_autonomous_flags(&mut args, "gemini", true, false, &tools);
        // --yolo must still be appended despite the warning.
        assert!(args.contains(&"--yolo".to_string()));
        assert!(!args.contains(&"--disallowedTools".to_string()));
    }

    #[test]
    fn append_autonomous_flags_gemini_yolo_takes_precedence_over_auto() {
        // When both yolo and auto are true, yolo wins for gemini.
        let mut args = vec!["gemini".to_string()];
        append_autonomous_flags(&mut args, "gemini", true, true, &[]);
        assert!(args.contains(&"--yolo".to_string()), "--yolo must appear when yolo=true");
        assert!(
            !args.contains(&"--approval-mode=auto_edit".to_string()),
            "--approval-mode=auto_edit must NOT appear when yolo=true"
        );
    }

    #[tokio::test]
    async fn run_agent_with_sink_fails_without_git_root() {
        let (tx, _rx) = unbounded_channel();
        let sink = OutputSink::Channel(tx);
        let entrypoint = vec!["claude".to_string()];
        // Run from a temp dir with no git repo.
        let tmp = tempfile::TempDir::new().unwrap();
        let original_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(tmp.path()).unwrap();

        let runtime = crate::runtime::docker::DockerRuntime::new();
        let result = run_agent_with_sink(
            entrypoint,
            "test",
            &sink,
            Some(tmp.path().to_path_buf()),
            vec![],
            false,
            None,
            false,
            false,
            None,
            &runtime,
        )
        .await;

        std::env::set_current_dir(original_dir).unwrap();
        assert!(result.is_err());
    }
}
