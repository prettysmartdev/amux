use crate::commands::agent::run_agent_with_sink;
use crate::commands::auth::resolve_auth;
use crate::commands::implement::confirm_mount_scope_stdin;
use crate::commands::init::find_git_root;
use crate::commands::output::OutputSink;
use crate::config::{effective_env_passthrough, effective_yolo_disallowed_tools, load_repo_config};
use crate::docker;
use anyhow::{Context, Result};
use std::path::PathBuf;

/// Command-mode entry point for `amux chat`.
pub async fn run(non_interactive: bool, plan: bool, allow_docker: bool, mount_ssh: bool, yolo: bool, auto: bool, runtime: std::sync::Arc<dyn crate::runtime::AgentRuntime>) -> Result<()> {
    let git_root = find_git_root().context("Not inside a Git repository")?;
    let mount_path = confirm_mount_scope_stdin(&git_root)?;
    let credentials = resolve_auth(&git_root, agent_name(&git_root)?)?;
    let config = load_repo_config(&git_root)?;
    let agent = config.agent.as_deref().unwrap_or("claude");
    let host_settings = docker::HostSettings::prepare(agent)
        .or_else(|| docker::HostSettings::prepare_minimal(agent));

    // Suppress the dangerous-mode permission dialog when running with --yolo.
    if yolo {
        if let Some(ref s) = host_settings {
            let _ = s.apply_yolo_settings();
        }
    }

    let mut env_vars = credentials.env_vars.clone();
    let passthrough_names = effective_env_passthrough(&git_root);
    for name in &passthrough_names {
        // Skip vars already supplied by keychain credentials — keychain takes precedence.
        if env_vars.iter().any(|(k, _)| k == name) {
            continue;
        }
        if let Ok(val) = std::env::var(name) {
            env_vars.push((name.clone(), val));
        }
    }

    run_with_sink(
        &OutputSink::Stdout,
        Some(mount_path),
        env_vars,
        non_interactive,
        plan,
        host_settings.as_ref(),
        allow_docker,
        mount_ssh,
        yolo,
        auto,
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
/// `yolo`: when true, append `--dangerously-skip-permissions` and disallowed-tools config.
/// `auto`: when true, append `--permission-mode auto` and disallowed-tools config.
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
    auto: bool,
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

    let disallowed_tools = if yolo || auto { effective_yolo_disallowed_tools(&git_root) } else { vec![] };
    append_yolo_flags(&mut entrypoint, &agent, yolo, auto, &disallowed_tools);

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
        "maki" => "maki",
        _ => "claude",
    })
}

/// Build the entrypoint command for a chat session (interactive, no prompt).
pub fn chat_entrypoint(agent: &str, plan: bool) -> Vec<String> {
    let mut args = match agent {
        "claude" => vec!["claude".to_string()],
        "codex" => vec!["codex".to_string()],
        "opencode" => vec!["opencode".to_string()],
        "maki" => vec!["maki".to_string()],
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
        "maki" => vec!["maki".to_string(), "--print".to_string()],
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
/// - Maki: no plan mode available (flag is silently ignored)
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
        // Maki has no plan mode.
        "maki" => {}
        // Opencode and unknown agents have no plan mode.
        _ => {}
    }
}

/// Append agent-specific autonomous-mode flags and disallowed-tools config.
///
/// When `yolo` is true:
/// - Claude: `--dangerously-skip-permissions`
/// When `auto` is true (and not yolo):
/// - Claude: `--permission-mode auto`
/// Both modes:
/// - Claude: if disallowed_tools non-empty, `--disallowedTools <t1>,<t2>,...`
/// - Codex: `--full-auto`; disallowed tools not supported (warning printed)
/// - Opencode: no equivalent — a warning is printed; disallowed tools not supported
/// - Maki: `--yolo` (maki's own flag to skip all permission prompts); disallowed tools not supported
pub fn append_yolo_flags(args: &mut Vec<String>, agent: &str, yolo: bool, auto: bool, disallowed_tools: &[String]) {
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
        append_yolo_flags(&mut args, "claude", false, false, &[]);
        assert_eq!(args, vec!["claude"]);
    }

    #[test]
    fn append_yolo_flags_claude_adds_skip_permissions() {
        let mut args = vec!["claude".to_string()];
        append_yolo_flags(&mut args, "claude", true, false, &[]);
        assert!(
            args.contains(&"--dangerously-skip-permissions".to_string()),
            "claude must receive --dangerously-skip-permissions"
        );
    }

    #[test]
    fn append_yolo_flags_claude_no_disallowed_tools_skips_flag() {
        let mut args = vec!["claude".to_string()];
        append_yolo_flags(&mut args, "claude", true, false, &[]);
        assert!(
            !args.contains(&"--disallowedTools".to_string()),
            "--disallowedTools must not appear when the list is empty"
        );
    }

    #[test]
    fn append_yolo_flags_claude_with_disallowed_tools() {
        let mut args = vec!["claude".to_string()];
        let tools = vec!["Bash".to_string(), "computer".to_string()];
        append_yolo_flags(&mut args, "claude", true, false, &tools);
        let dt_idx = args
            .iter()
            .position(|a| a == "--disallowedTools")
            .expect("--disallowedTools flag missing");
        assert_eq!(args[dt_idx + 1], "Bash,computer");
    }

    #[test]
    fn append_yolo_flags_codex_adds_full_auto() {
        let mut args = vec!["codex".to_string()];
        append_yolo_flags(&mut args, "codex", true, false, &[]);
        assert!(args.contains(&"--full-auto".to_string()));
        assert!(!args.contains(&"--dangerously-skip-permissions".to_string()));
    }

    #[test]
    fn append_yolo_flags_codex_no_disallowed_tools_flag() {
        // codex does not support --disallowedTools; the flag must never appear
        let mut args = vec!["codex".to_string()];
        let tools = vec!["Bash".to_string()];
        append_yolo_flags(&mut args, "codex", true, false, &tools);
        assert!(!args.contains(&"--disallowedTools".to_string()));
    }

    #[test]
    fn append_yolo_flags_opencode_no_skip_permissions_flag() {
        // opencode has no skip-permissions equivalent; args must be unchanged
        let mut args = vec!["opencode".to_string()];
        append_yolo_flags(&mut args, "opencode", true, false, &[]);
        assert_eq!(args, vec!["opencode"]);
    }

    #[test]
    fn append_yolo_flags_opencode_no_disallowed_tools_flag() {
        let mut args = vec!["opencode".to_string()];
        let tools = vec!["Bash".to_string()];
        append_yolo_flags(&mut args, "opencode", true, false, &tools);
        assert!(!args.contains(&"--disallowedTools".to_string()));
        assert_eq!(args, vec!["opencode"]);
    }

    // --- append_yolo_flags --auto mode tests ---

    #[test]
    fn append_yolo_flags_noop_when_both_false() {
        let mut args = vec!["claude".to_string()];
        append_yolo_flags(&mut args, "claude", false, false, &[]);
        assert_eq!(args, vec!["claude"]);
    }

    #[test]
    fn append_yolo_flags_auto_claude_adds_permission_mode_auto() {
        let mut args = vec!["claude".to_string()];
        append_yolo_flags(&mut args, "claude", false, true, &[]);
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
    fn append_yolo_flags_auto_claude_with_disallowed_tools() {
        let mut args = vec!["claude".to_string()];
        let tools = vec!["Bash".to_string()];
        append_yolo_flags(&mut args, "claude", false, true, &tools);
        assert!(args.contains(&"--disallowedTools".to_string()));
    }

    #[test]
    fn append_yolo_flags_yolo_takes_precedence_over_auto() {
        // When both are true, yolo wins (uses --dangerously-skip-permissions).
        let mut args = vec!["claude".to_string()];
        append_yolo_flags(&mut args, "claude", true, true, &[]);
        assert!(args.contains(&"--dangerously-skip-permissions".to_string()));
        assert!(!args.contains(&"auto".to_string()));
    }

    // --- maki entrypoints ---

    #[test]
    fn chat_entrypoint_maki() {
        let args = chat_entrypoint("maki", false);
        assert_eq!(args, vec!["maki"]);
    }

    #[test]
    fn chat_entrypoint_non_interactive_maki() {
        let args = chat_entrypoint_non_interactive("maki", false);
        assert_eq!(args, vec!["maki", "--print"]);
    }

    #[test]
    fn chat_entrypoint_plan_maki() {
        // Maki has no plan mode; the flag is silently ignored.
        let args = chat_entrypoint("maki", true);
        assert_eq!(args, vec!["maki"]);
    }

    // --- append_yolo_flags maki tests ---

    #[test]
    fn append_yolo_flags_maki_adds_yolo_flag() {
        let mut args = vec!["maki".to_string()];
        append_yolo_flags(&mut args, "maki", true, false, &[]);
        assert!(args.contains(&"--yolo".to_string()), "maki must receive --yolo in yolo mode");
    }

    #[test]
    fn append_yolo_flags_maki_never_adds_disallowed_tools_flag() {
        // maki does not support --disallowedTools; it must never appear regardless of the list.
        let mut args = vec!["maki".to_string()];
        let tools = vec!["Bash".to_string(), "computer".to_string()];
        append_yolo_flags(&mut args, "maki", true, false, &tools);
        assert!(
            !args.contains(&"--disallowedTools".to_string()),
            "--disallowedTools must never appear for maki"
        );
        assert!(args.contains(&"--yolo".to_string()), "--yolo must still be appended");
    }

    #[test]
    fn append_yolo_flags_maki_prints_warning_when_disallowed_tools_nonempty() {
        // The warning is emitted via eprintln! and cannot be trivially captured in a unit test
        // without a custom stderr-redirect harness. This test verifies the code path compiles
        // and does not panic — the eprintln! path is exercised by calling it with a non-empty list.
        // If a stderr-capture crate (e.g. `gag`) is added as a dev-dependency, this test should
        // be upgraded to assert the warning string is present.
        let mut args = vec!["maki".to_string()];
        let tools = vec!["Bash".to_string()];
        // Must not panic; warning is a side effect on stderr.
        append_yolo_flags(&mut args, "maki", true, false, &tools);
    }

    #[test]
    fn append_yolo_flags_maki_no_disallowed_tools_exact_args() {
        // When disallowed_tools is empty, exactly ["maki", "--yolo"] must result.
        let mut args = vec!["maki".to_string()];
        append_yolo_flags(&mut args, "maki", true, false, &[]);
        assert_eq!(args, vec!["maki", "--yolo"]);
    }

    // --- passthrough injection tests ---

    #[test]
    fn passthrough_injection_adds_set_env_var_to_env_vars() {
        use crate::config::{save_repo_config, RepoConfig};
        use tempfile::TempDir;

        let tmp = TempDir::new().unwrap();
        let config = RepoConfig {
            agent: None,
            auto_agent_auth_accepted: None,
            terminal_scrollback_lines: None,
            yolo_disallowed_tools: None,
            env_passthrough: Some(vec!["AMUX_TEST_PT_INJECT_PRESENT".to_string()]),
        };
        save_repo_config(tmp.path(), &config).unwrap();

        // SAFETY: test-only env mutation; unique var name avoids races with other tests.
        unsafe { std::env::set_var("AMUX_TEST_PT_INJECT_PRESENT", "injected_value_99") };

        // Simulate the passthrough injection loop from chat::run.
        let mut env_vars: Vec<(String, String)> = vec![];
        let passthrough_names = effective_env_passthrough(tmp.path());
        for name in &passthrough_names {
            if let Ok(val) = std::env::var(name) {
                env_vars.push((name.clone(), val));
            }
        }

        // SAFETY: test-only env mutation.
        unsafe { std::env::remove_var("AMUX_TEST_PT_INJECT_PRESENT") };

        assert!(
            env_vars.contains(&("AMUX_TEST_PT_INJECT_PRESENT".to_string(), "injected_value_99".to_string())),
            "set env var must appear in env_vars after passthrough injection"
        );
    }

    #[test]
    fn passthrough_injection_skips_absent_env_var() {
        use crate::config::{save_repo_config, RepoConfig};
        use tempfile::TempDir;

        let tmp = TempDir::new().unwrap();
        // Use a var name that is very unlikely to be set in any test environment.
        let absent_var = "AMUX_TEST_PT_INJECT_DEFINITELY_NOT_SET_XYZ_999";
        std::env::remove_var(absent_var);

        let config = RepoConfig {
            agent: None,
            auto_agent_auth_accepted: None,
            terminal_scrollback_lines: None,
            yolo_disallowed_tools: None,
            env_passthrough: Some(vec![absent_var.to_string()]),
        };
        save_repo_config(tmp.path(), &config).unwrap();

        // Simulate the passthrough injection loop from chat::run.
        let mut env_vars: Vec<(String, String)> = vec![];
        let passthrough_names = effective_env_passthrough(tmp.path());
        for name in &passthrough_names {
            if let Ok(val) = std::env::var(name) {
                env_vars.push((name.clone(), val));
            }
        }

        assert!(
            env_vars.is_empty(),
            "absent env var must not be added to env_vars; no error or panic should occur"
        );
    }

    #[test]
    fn passthrough_injection_skips_var_already_in_credentials() {
        use crate::config::{save_repo_config, RepoConfig};
        use tempfile::TempDir;

        let tmp = TempDir::new().unwrap();
        let var_name = "AMUX_TEST_PT_DEDUP_VAR_UNIQUE_456";

        let config = RepoConfig {
            agent: None,
            auto_agent_auth_accepted: None,
            terminal_scrollback_lines: None,
            yolo_disallowed_tools: None,
            env_passthrough: Some(vec![var_name.to_string()]),
        };
        save_repo_config(tmp.path(), &config).unwrap();
        // SAFETY: test-only env mutation; unique var name avoids races with other tests.
        unsafe { std::env::set_var(var_name, "passthrough_value") };

        // Simulate starting with the var already present (e.g., from keychain credentials).
        let mut env_vars: Vec<(String, String)> = vec![(var_name.to_string(), "cred_value".to_string())];

        // Simulate the passthrough injection loop from chat::run (with skip-if-present guard).
        let passthrough_names = effective_env_passthrough(tmp.path());
        for name in &passthrough_names {
            if env_vars.iter().any(|(k, _)| k == name) {
                continue; // keychain takes precedence
            }
            if let Ok(val) = std::env::var(name) {
                env_vars.push((name.clone(), val));
            }
        }

        // SAFETY: test-only env mutation.
        unsafe { std::env::remove_var(var_name) };

        // Keychain credential must be present with its original value.
        let entry = env_vars.iter().find(|(k, _)| k == var_name);
        assert!(entry.is_some(), "credential var must remain in env_vars");
        assert_eq!(entry.unwrap().1, "cred_value", "keychain value must not be overwritten by passthrough");

        // Var must appear exactly once — passthrough entry was skipped.
        let count = env_vars.iter().filter(|(k, _)| k == var_name).count();
        assert_eq!(count, 1, "keychain takes precedence: no duplicate -e flag");
    }
}
