use crate::commands::agent::{append_autonomous_flags, run_agent_with_sink};
use crate::commands::auth::resolve_auth;
use crate::commands::implement::confirm_mount_scope_stdin;
use crate::commands::init::find_git_root;
use crate::commands::output::OutputSink;
use crate::config::{effective_env_passthrough, effective_yolo_disallowed_tools, load_repo_config};
use crate::runtime::docker as docker;
use anyhow::{Context, Result};
use std::path::PathBuf;

/// Command-mode entry point for `amux chat`.
pub async fn run(non_interactive: bool, plan: bool, allow_docker: bool, mount_ssh: bool, yolo: bool, auto: bool, runtime: std::sync::Arc<dyn crate::runtime::AgentRuntime>) -> Result<()> {
    let git_root = find_git_root().context("Not inside a Git repository")?;
    let mount_path = confirm_mount_scope_stdin(&git_root)?;
    let credentials = resolve_auth(&git_root, agent_name(&git_root)?)?;
    let config = load_repo_config(&git_root)?;
    let agent = config.agent.as_deref().unwrap_or("claude");
    let host_settings = crate::passthrough::passthrough_for_agent(agent).prepare_host_settings();

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
    append_autonomous_flags(&mut entrypoint, &agent, yolo, auto, &disallowed_tools);

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
        "gemini" => "gemini",
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
        "gemini" => vec!["gemini".to_string()],
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
        // Gemini supports -p / --prompt for headless/non-interactive output.
        "gemini" => vec!["gemini".to_string(), "-p".to_string()],
        _ => vec![agent.to_string()],
    };
    append_plan_flags(&mut args, agent, plan);
    args
}

/// Append agent-specific plan mode flags to the argument list.
///
/// - Claude: `--permission-mode plan`
/// - Codex: `--approval-mode plan`
/// - Gemini: `--approval-mode=plan`
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
        "gemini" => {
            args.push("--approval-mode=plan".to_string());
        }
        // Maki has no plan mode.
        "maki" => {}
        // Opencode and unknown agents have no plan mode.
        _ => {}
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

    // --- gemini entrypoints ---

    #[test]
    fn chat_entrypoint_gemini() {
        let args = chat_entrypoint("gemini", false);
        assert_eq!(args, vec!["gemini"]);
    }

    #[test]
    fn chat_entrypoint_non_interactive_gemini() {
        let args = chat_entrypoint_non_interactive("gemini", false);
        assert_eq!(args, vec!["gemini", "-p"]);
    }

    #[test]
    fn chat_entrypoint_plan_gemini() {
        let args = chat_entrypoint("gemini", true);
        assert_eq!(args, vec!["gemini", "--approval-mode=plan"]);
    }

    #[test]
    fn chat_entrypoint_non_interactive_plan_gemini() {
        let args = chat_entrypoint_non_interactive("gemini", true);
        assert_eq!(args, vec!["gemini", "-p", "--approval-mode=plan"]);
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
