use crate::commands::init::find_git_root;
use crate::commands::output::OutputSink;
use crate::config::load_repo_config;
use crate::docker;
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
    let modified_settings: Option<docker::HostSettings> = host_settings.and_then(|settings| {
        let mut new_settings = docker::HostSettings::from_paths(
            settings.config_path.clone(),
            settings.claude_dir_path.clone(),
        );
        let dockerfile = git_root.join("Dockerfile.dev");
        if let Some(msg) = crate::runtime::apply_dockerfile_user(&mut new_settings, &dockerfile) {
            out.println(msg);
            Some(new_settings)
        } else {
            None
        }
    });
    let effective_settings: Option<&docker::HostSettings> =
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

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc::unbounded_channel;

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
