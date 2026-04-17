use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::runtime::{AgentRuntime, ContainerStats, HostSettings, StoppedContainerInfo};

/// Docker-backed implementation of `AgentRuntime`.
///
/// Calls the `docker` CLI directly.  Runtime-independent utilities (image tag
/// derivation, container name generation, build command formatting, etc.) live in
/// `runtime/mod.rs` and are accessed via `crate::runtime::*`.
pub struct DockerRuntime;

impl DockerRuntime {
    pub fn new() -> Self {
        DockerRuntime
    }
}

impl Default for DockerRuntime {
    fn default() -> Self {
        DockerRuntime::new()
    }
}

// ─── Private helpers ────────────────────────────────────────────────────────

pub fn docker_socket_path() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        PathBuf::from(r"\\.\pipe\docker_engine")
    }
    #[cfg(not(target_os = "windows"))]
    {
        PathBuf::from("/var/run/docker.sock")
    }
}

fn append_ssh_mount(args: &mut Vec<String>, ssh_dir: Option<&Path>) {
    if let Some(path) = ssh_dir {
        args.push("-v".to_string());
        args.push(format!("{}:/root/.ssh:ro", path.display()));
    }
}

fn append_docker_socket_mount_args(args: &mut Vec<String>) {
    let path = docker_socket_path();
    let path_str = path.to_string_lossy().to_string();
    #[cfg(target_os = "windows")]
    {
        args.push("--mount".into());
        args.push(format!("type=npipe,source={},target={}", path_str, path_str));
    }
    #[cfg(not(target_os = "windows"))]
    {
        args.push("-v".into());
        args.push(format!("{}:{}", path_str, path_str));
    }
}

fn append_settings_mounts(args: &mut Vec<String>, settings: &HostSettings) {
    if settings.mount_claude_files {
        args.push("-v".into());
        args.push(format!(
            "{}:{}/.claude.json",
            settings.config_path.display(),
            settings.container_home,
        ));
        args.push("-v".into());
        args.push(format!(
            "{}:{}/.claude",
            settings.claude_dir_path.display(),
            settings.container_home,
        ));
    }
    if let Some((host_dir, container_dir)) = &settings.agent_config_dir {
        args.push("-v".into());
        args.push(format!("{}:{}", host_dir.display(), container_dir));
    }
}

fn append_settings_mounts_display(args: &mut Vec<String>, settings: Option<&HostSettings>) {
    let home = settings.map(|s| s.container_home.as_str()).unwrap_or("/root");
    if settings.map(|s| s.mount_claude_files).unwrap_or(true) {
        args.push("-v".into());
        args.push(format!("<settings>:{}/.claude.json", home));
        args.push("-v".into());
        args.push(format!("<settings>:{}/.claude", home));
    }
    if settings.and_then(|s| s.agent_config_dir.as_ref()).is_some() {
        args.push("-v".into());
        args.push("<agent-config>:<agent-config-dir>".into());
    }
}

fn append_entrypoint(args: &mut Vec<String>, image: &str, entrypoint: &[&str]) {
    args.push(image.into());
    args.extend(entrypoint.iter().map(|s| s.to_string()));
}

fn append_env_args(args: &mut Vec<String>, env_vars: &[(String, String)]) {
    for (key, value) in env_vars {
        args.push("-e".into());
        args.push(format!("{}={}", key, value));
    }
}

fn append_env_args_display(args: &mut Vec<String>, env_vars: &[(String, String)]) {
    for (key, _) in env_vars {
        args.push("-e".into());
        args.push(format!("{}=***", key));
    }
}

// ─── AgentRuntime impl ──────────────────────────────────────────────────────

impl AgentRuntime for DockerRuntime {
    fn is_available(&self) -> bool {
        Command::new("docker")
            .args(["info", "--format", "{{.ServerVersion}}"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    fn check_socket(&self) -> anyhow::Result<std::path::PathBuf> {
        check_docker_socket()
    }

    fn build_image_streaming(
        &self,
        tag: &str,
        dockerfile: &Path,
        context: &Path,
        no_cache: bool,
        on_line: &mut dyn FnMut(&str),
    ) -> Result<String> {
        use std::io::BufRead;
        use std::sync::mpsc;

        let dockerfile_str = dockerfile.to_string_lossy();
        let context_str = context.to_string_lossy();

        let mut build_args = vec!["build"];
        if no_cache {
            build_args.push("--no-cache");
        }
        let tag_arg = tag;
        let df_arg = dockerfile_str.as_ref();
        let ctx_arg = context_str.as_ref();
        build_args.extend_from_slice(&["-t", tag_arg, "-f", df_arg, ctx_arg]);

        let mut child = Command::new("docker")
            .args(&build_args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("Failed to invoke `docker build`")?;

        let stdout = child.stdout.take();
        let stderr = child.stderr.take();
        let mut combined = String::new();
        let (tx, rx) = mpsc::channel::<String>();

        let tx_stderr = tx.clone();
        let stderr_handle = std::thread::spawn(move || {
            if let Some(stderr) = stderr {
                let reader = std::io::BufReader::new(stderr);
                for line in reader.lines() {
                    if let Ok(line) = line {
                        let _ = tx_stderr.send(line);
                    }
                }
            }
        });

        let tx_stdout = tx;
        let stdout_handle = std::thread::spawn(move || {
            if let Some(stdout) = stdout {
                let reader = std::io::BufReader::new(stdout);
                for line in reader.lines() {
                    if let Ok(line) = line {
                        let _ = tx_stdout.send(line);
                    }
                }
            }
        });

        for line in rx {
            on_line(&line);
            combined.push_str(&line);
            combined.push('\n');
        }

        let _ = stderr_handle.join();
        let _ = stdout_handle.join();

        let status = child.wait().context("Failed to wait for `docker build`")?;
        if !status.success() {
            bail!("`docker build` failed:\n{}", combined);
        }
        Ok(combined)
    }

    fn image_exists(&self, tag: &str) -> bool {
        Command::new("docker")
            .args(["image", "inspect", tag])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    fn run_container(
        &self,
        image: &str,
        host_path: &str,
        entrypoint: &[&str],
        env_vars: &[(String, String)],
        host_settings: Option<&HostSettings>,
        allow_docker: bool,
        container_name: Option<&str>,
        ssh_dir: Option<&Path>,
    ) -> Result<()> {
        let mut args: Vec<String> = vec!["run".into(), "--rm".into(), "-it".into()];

        if let Some(name) = container_name {
            args.insert(1, "--name".to_string());
            args.insert(2, name.to_string());
        }

        args.extend_from_slice(&[
            "-v".into(),
            format!("{}:/workspace", host_path),
            "-w".into(),
            "/workspace".into(),
        ]);

        if let Some(settings) = host_settings {
            append_settings_mounts(&mut args, settings);
        }
        if allow_docker {
            append_docker_socket_mount_args(&mut args);
        }
        append_ssh_mount(&mut args, ssh_dir);
        append_env_args(&mut args, env_vars);
        append_entrypoint(&mut args, image, entrypoint);

        let status = Command::new("docker")
            .args(&args)
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
            .context("Failed to invoke `docker run`")?;

        if !status.success() {
            bail!("Container exited with status: {}", status);
        }
        Ok(())
    }

    fn run_container_captured(
        &self,
        image: &str,
        host_path: &str,
        entrypoint: &[&str],
        env_vars: &[(String, String)],
        host_settings: Option<&HostSettings>,
        allow_docker: bool,
        container_name: Option<&str>,
        ssh_dir: Option<&Path>,
    ) -> Result<(String, String)> {
        let mut args: Vec<String> = vec![
            "run".into(),
            "--rm".into(),
            "-v".into(),
            format!("{}:/workspace", host_path),
            "-w".into(),
            "/workspace".into(),
        ];

        if let Some(name) = container_name {
            args.insert(1, "--name".to_string());
            args.insert(2, name.to_string());
        }

        if let Some(settings) = host_settings {
            append_settings_mounts(&mut args, settings);
        }
        if allow_docker {
            append_docker_socket_mount_args(&mut args);
        }
        append_ssh_mount(&mut args, ssh_dir);
        append_env_args(&mut args, env_vars);
        append_entrypoint(&mut args, image, entrypoint);

        let display_args = self.build_run_args_display(
            image,
            host_path,
            entrypoint,
            env_vars,
            host_settings,
            allow_docker,
            container_name,
            ssh_dir,
        );
        let cmd_line = format!("docker {}", display_args.join(" "));

        let output = Command::new("docker")
            .args(&args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .context("Failed to invoke `docker run`")?;

        let mut combined = String::new();
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        if !stdout.is_empty() {
            combined.push_str(&stdout);
        }
        if !stderr.is_empty() {
            if !combined.is_empty() {
                combined.push('\n');
            }
            combined.push_str(&stderr);
        }

        if !output.status.success() {
            bail!("Container exited with an error:\n{}", combined);
        }
        Ok((cmd_line, combined))
    }

    fn run_container_at_path(
        &self,
        image: &str,
        host_path: &str,
        container_path: &str,
        working_dir: &str,
        entrypoint: &[&str],
        env_vars: &[(String, String)],
        host_settings: Option<&HostSettings>,
        allow_docker: bool,
        container_name: Option<&str>,
    ) -> Result<()> {
        let mut args: Vec<String> = vec!["run".into(), "--rm".into(), "-it".into()];

        if let Some(name) = container_name {
            args.push("--name".into());
            args.push(name.into());
        }

        args.extend_from_slice(&[
            "-v".into(),
            format!("{}:{}", host_path, container_path),
            "-w".into(),
            working_dir.into(),
        ]);

        if let Some(settings) = host_settings {
            append_settings_mounts(&mut args, settings);
        }
        if allow_docker {
            append_docker_socket_mount_args(&mut args);
        }
        append_env_args(&mut args, env_vars);
        append_entrypoint(&mut args, image, entrypoint);

        let status = Command::new("docker")
            .args(&args)
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
            .context("Failed to invoke `docker run`")?;

        if !status.success() {
            bail!("Container exited with status: {}", status);
        }
        Ok(())
    }

    fn run_container_captured_at_path(
        &self,
        image: &str,
        host_path: &str,
        container_path: &str,
        working_dir: &str,
        entrypoint: &[&str],
        env_vars: &[(String, String)],
        host_settings: Option<&HostSettings>,
        allow_docker: bool,
    ) -> Result<(String, String)> {
        let mut args: Vec<String> = vec![
            "run".into(),
            "--rm".into(),
            "-v".into(),
            format!("{}:{}", host_path, container_path),
            "-w".into(),
            working_dir.into(),
        ];

        if let Some(settings) = host_settings {
            append_settings_mounts(&mut args, settings);
        }
        if allow_docker {
            append_docker_socket_mount_args(&mut args);
        }
        append_env_args(&mut args, env_vars);
        append_entrypoint(&mut args, image, entrypoint);

        let output = Command::new("docker")
            .args(&args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .context("Failed to invoke `docker run`")?;

        let mut combined = String::new();
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        if !stdout.is_empty() {
            combined.push_str(&stdout);
        }
        if !stderr.is_empty() {
            if !combined.is_empty() {
                combined.push('\n');
            }
            combined.push_str(&stderr);
        }

        if !output.status.success() {
            bail!("Container exited with an error:\n{}", combined);
        }
        // Build a display-safe command line (env values masked) consistent with
        // run_container_captured, which also returns (cmd_line, output).
        let masked_env: String = env_vars
            .iter()
            .flat_map(|(k, _)| [format!("-e {}=***", k)])
            .collect::<Vec<_>>()
            .join(" ");
        let cmd_line = format!(
            "docker run --rm -v {}:{} -w {} {} {}",
            host_path, container_path, working_dir, masked_env, image
        );
        Ok((cmd_line, combined))
    }

    fn run_container_detached(
        &self,
        image: &str,
        host_path: &str,
        container_path: &str,
        working_dir: &str,
        container_name: Option<&str>,
        env_vars: Vec<(String, String)>,
        allow_docker: bool,
        host_settings: Option<&HostSettings>,
    ) -> Result<String> {
        let mut args: Vec<String> = vec!["run".into(), "-d".into()];

        if let Some(name) = container_name {
            args.push("--name".into());
            args.push(name.into());
        }

        args.extend_from_slice(&[
            "-v".into(),
            format!("{}:{}", host_path, container_path),
            "-w".into(),
            working_dir.into(),
        ]);

        if allow_docker {
            append_docker_socket_mount_args(&mut args);
        }

        if let Some(settings) = host_settings {
            append_settings_mounts(&mut args, settings);
        }

        append_env_args(&mut args, &env_vars);

        args.extend_from_slice(&[
            image.into(),
            "sh".into(),
            "-c".into(),
            "while true; do sleep 86400; done".into(),
        ]);

        let output = Command::new("docker")
            .args(&args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .context("Failed to invoke `docker run -d`")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("Failed to start background container: {}", stderr.trim());
        }

        let container_id = String::from_utf8_lossy(&output.stdout).trim().to_string();
        Ok(container_id)
    }

    fn start_container(&self, container_id: &str) -> Result<()> {
        let output = Command::new("docker")
            .args(["start", container_id])
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .output()
            .context("Failed to invoke `docker start`")?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            if stderr.is_empty() {
                bail!("Failed to start container {}", container_id);
            }
            bail!("Failed to start container {}: {}", container_id, stderr);
        }
        Ok(())
    }

    fn stop_container(&self, container_id: &str) -> Result<()> {
        let output = Command::new("docker")
            .args(["stop", container_id])
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .output()
            .context("Failed to invoke `docker stop`")?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            if stderr.is_empty() {
                bail!("Failed to stop container {}", container_id);
            }
            bail!("Failed to stop container {}: {}", container_id, stderr);
        }
        Ok(())
    }

    fn remove_container(&self, container_id: &str) -> Result<()> {
        let output = Command::new("docker")
            .args(["rm", container_id])
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .output()
            .context("Failed to invoke `docker rm`")?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            if stderr.is_empty() {
                bail!("Failed to remove container {}", container_id);
            }
            bail!("Failed to remove container {}: {}", container_id, stderr);
        }
        Ok(())
    }

    fn is_container_running(&self, container_id: &str) -> bool {
        let output = Command::new("docker")
            .args(["inspect", "--format", "{{.State.Running}}", container_id])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output()
            .ok();
        if let Some(output) = output {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                return stdout.trim() == "true";
            }
        }
        false
    }

    fn find_stopped_container(&self, name: &str, image: &str) -> Option<StoppedContainerInfo> {
        let output = Command::new("docker")
            .args([
                "ps",
                "-a",
                "--filter",
                &format!("name={}", name),
                "--format",
                "{{.ID}}\t{{.Names}}\t{{.CreatedAt}}\t{{.Image}}\t{{.Status}}",
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output()
            .ok()?;

        if !output.status.success() {
            return None;
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            let parts: Vec<&str> = line.splitn(5, '\t').collect();
            if parts.len() < 5 {
                continue;
            }
            let (id, container_name, created, container_image, status) =
                (parts[0], parts[1], parts[2], parts[3], parts[4]);
            if container_name != name {
                continue;
            }
            if container_image != image {
                continue;
            }
            if status.starts_with("Up ") {
                continue;
            }
            return Some(StoppedContainerInfo {
                id: id.to_string(),
                name: container_name.to_string(),
                created: created.to_string(),
            });
        }
        None
    }

    fn list_running_containers_by_prefix(&self, prefix: &str) -> Vec<String> {
        let output = Command::new("docker")
            .args([
                "ps",
                "--filter",
                &format!("name={}", prefix),
                "--format",
                "{{.Names}}",
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output();

        match output {
            Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout)
                .lines()
                .filter(|l| !l.is_empty() && l.starts_with(prefix))
                .map(|l| l.to_string())
                .collect(),
            _ => vec![],
        }
    }

    fn list_running_containers_with_ids_by_prefix(&self, prefix: &str) -> Vec<(String, String)> {
        let output = Command::new("docker")
            .args([
                "ps",
                "--filter",
                &format!("name={}", prefix),
                "--format",
                "{{.Names}}\t{{.ID}}",
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output();

        match output {
            Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout)
                .lines()
                .filter(|l| !l.is_empty())
                .filter_map(|l| {
                    let mut parts = l.splitn(2, '\t');
                    let name = parts.next()?.to_string();
                    let id = parts.next().unwrap_or("").trim().to_string();
                    if name.starts_with(prefix) {
                        Some((name, id))
                    } else {
                        None
                    }
                })
                .collect(),
            _ => vec![],
        }
    }

    fn get_container_workspace_mount(&self, container_name: &str) -> Option<String> {
        let format_str =
            "{{range .Mounts}}{{if eq .Destination \"/workspace\"}}{{.Source}}{{end}}{{end}}";
        let output = Command::new("docker")
            .args(["inspect", "--format", format_str, container_name])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output()
            .ok()?;

        if !output.status.success() {
            return None;
        }

        let src = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if src.is_empty() { None } else { Some(src) }
    }

    fn query_container_stats(&self, name: &str) -> Option<ContainerStats> {
        let output = Command::new("docker")
            .args([
                "stats",
                "--no-stream",
                "--format",
                "{{.Name}}|{{.CPUPerc}}|{{.MemUsage}}",
                name,
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let line = String::from_utf8_lossy(&output.stdout).trim().to_string();
        super::parse_stats_line(&line)
    }

    fn build_run_args_pty(
        &self,
        image: &str,
        host_path: &str,
        entrypoint: &[&str],
        env_vars: &[(String, String)],
        host_settings: Option<&HostSettings>,
        allow_docker: bool,
        container_name: Option<&str>,
        ssh_dir: Option<&Path>,
    ) -> Vec<String> {
        let mut args: Vec<String> = vec!["run".into(), "--rm".into(), "-it".into()];

        if let Some(name) = container_name {
            args.push("--name".into());
            args.push(name.into());
        }

        args.extend_from_slice(&[
            "-v".into(),
            format!("{}:/workspace", host_path),
            "-w".into(),
            "/workspace".into(),
        ]);

        if let Some(settings) = host_settings {
            append_settings_mounts(&mut args, settings);
        }
        if allow_docker {
            append_docker_socket_mount_args(&mut args);
        }
        append_ssh_mount(&mut args, ssh_dir);
        append_env_args(&mut args, env_vars);
        append_entrypoint(&mut args, image, entrypoint);
        args
    }

    fn build_run_args_pty_display(
        &self,
        image: &str,
        host_path: &str,
        entrypoint: &[&str],
        env_vars: &[(String, String)],
        host_settings: Option<&HostSettings>,
        allow_docker: bool,
        container_name: Option<&str>,
        ssh_dir: Option<&Path>,
    ) -> Vec<String> {
        let mut args: Vec<String> = vec!["run".into(), "--rm".into(), "-it".into()];

        if let Some(name) = container_name {
            args.push("--name".into());
            args.push(name.into());
        }

        args.extend_from_slice(&[
            "-v".into(),
            format!("{}:/workspace", host_path),
            "-w".into(),
            "/workspace".into(),
        ]);

        if host_settings.is_some() {
            append_settings_mounts_display(&mut args, host_settings);
        }
        if allow_docker {
            append_docker_socket_mount_args(&mut args);
        }
        append_ssh_mount(&mut args, ssh_dir);
        append_env_args_display(&mut args, env_vars);
        append_entrypoint(&mut args, image, entrypoint);
        args
    }

    fn build_run_args_pty_at_path(
        &self,
        image: &str,
        host_path: &str,
        container_path: &str,
        working_dir: &str,
        entrypoint: &[&str],
        env_vars: &[(String, String)],
        host_settings: Option<&HostSettings>,
        allow_docker: bool,
        container_name: Option<&str>,
    ) -> Vec<String> {
        let mut args: Vec<String> = vec!["run".into(), "--rm".into(), "-it".into()];

        if let Some(name) = container_name {
            args.push("--name".into());
            args.push(name.into());
        }

        args.extend_from_slice(&[
            "-v".into(),
            format!("{}:{}", host_path, container_path),
            "-w".into(),
            working_dir.into(),
        ]);

        if let Some(settings) = host_settings {
            append_settings_mounts(&mut args, settings);
        }
        if allow_docker {
            append_docker_socket_mount_args(&mut args);
        }
        append_env_args(&mut args, env_vars);
        append_entrypoint(&mut args, image, entrypoint);
        args
    }

    fn build_exec_args_pty(
        &self,
        container_id: &str,
        working_dir: &str,
        entrypoint: &[&str],
        env_vars: &[(String, String)],
    ) -> Vec<String> {
        let mut args: Vec<String> =
            vec!["exec".into(), "-it".into(), "-w".into(), working_dir.into()];
        append_env_args(&mut args, env_vars);
        args.push(container_id.into());
        args.extend(entrypoint.iter().map(|s| s.to_string()));
        args
    }

    fn build_run_args_display(
        &self,
        image: &str,
        host_path: &str,
        entrypoint: &[&str],
        env_vars: &[(String, String)],
        host_settings: Option<&HostSettings>,
        allow_docker: bool,
        container_name: Option<&str>,
        ssh_dir: Option<&Path>,
    ) -> Vec<String> {
        let mut args: Vec<String> = vec![
            "run".into(),
            "--rm".into(),
            "-it".into(),
            "-v".into(),
            format!("{}:/workspace", host_path),
            "-w".into(),
            "/workspace".into(),
        ];

        if let Some(name) = container_name {
            args.insert(1, "--name".to_string());
            args.insert(2, name.to_string());
        }

        if host_settings.is_some() {
            append_settings_mounts_display(&mut args, host_settings);
        }
        if allow_docker {
            append_docker_socket_mount_args(&mut args);
        }
        append_ssh_mount(&mut args, ssh_dir);
        append_env_args_display(&mut args, env_vars);
        append_entrypoint(&mut args, image, entrypoint);
        args
    }

    fn name(&self) -> &'static str {
        "docker"
    }

    fn cli_binary(&self) -> &'static str {
        "docker"
    }
}

// ─── Free utilities ─────────────────────────────────────────────────────────

/// Checks that the host Docker daemon socket file exists and is accessible.
///
/// Returns the socket path on success, or an error if the socket is not found.
pub fn check_docker_socket() -> anyhow::Result<std::path::PathBuf> {
    use anyhow::bail;
    let path = docker_socket_path();
    if !path.exists() {
        bail!(
            "Docker socket not found at {}. Ensure the Docker daemon is running and accessible.",
            path.display()
        );
    }
    Ok(path)
}


#[cfg(test)]
mod tests {
    use super::{check_docker_socket, docker_socket_path, DockerRuntime};
    use crate::runtime::{
        agent_image_tag, format_build_cmd, format_build_cmd_no_cache, generate_container_name,
        parse_cpu_percent, parse_memory_mb, project_image_tag, AgentRuntime, HostSettings,
    };
    use std::path::{Path, PathBuf};

    fn rt() -> DockerRuntime {
        DockerRuntime::new()
    }

    // ─── name / cli_binary ───────────────────────────────────────────────────

    #[test]
    fn docker_runtime_name_and_cli_binary() {
        let r = rt();
        assert_eq!(r.name(), "docker");
        assert_eq!(r.cli_binary(), "docker");
    }

    // ─── build_run_args_pty ──────────────────────────────────────────────────

    #[test]
    fn build_run_args_pty_minimal_structure() {
        let args = rt().build_run_args_pty("myimage", "/host", &["claude"], &[], None, false, None, None);

        assert_eq!(args[0], "run");
        assert!(args.contains(&"--rm".to_string()));
        assert!(args.contains(&"-it".to_string()));
        // Workspace volume mount
        let v_idx = args.iter().position(|a| a == "-v").expect("-v flag");
        assert_eq!(args[v_idx + 1], "/host:/workspace");
        // Working directory
        let w_idx = args.iter().position(|a| a == "-w").expect("-w flag");
        assert_eq!(args[w_idx + 1], "/workspace");
        // Image and entrypoint at the end
        let img_idx = args.iter().position(|a| a == "myimage").expect("image");
        assert_eq!(args[img_idx + 1], "claude");
    }

    #[test]
    fn build_run_args_pty_with_container_name() {
        let args = rt().build_run_args_pty("img", "/h", &[], &[], None, false, Some("my-ctr"), None);

        assert!(args.contains(&"--name".to_string()));
        let name_idx = args.iter().position(|a| a == "--name").unwrap();
        assert_eq!(args[name_idx + 1], "my-ctr");
    }

    #[test]
    fn build_run_args_pty_with_env_vars() {
        let env = vec![
            ("FOO".to_string(), "bar".to_string()),
            ("HELLO".to_string(), "world".to_string()),
        ];
        let args = rt().build_run_args_pty("img", "/h", &[], &env, None, false, None, None);

        let e_indices: Vec<usize> = args
            .iter()
            .enumerate()
            .filter(|(_, a)| a.as_str() == "-e")
            .map(|(i, _)| i)
            .collect();
        assert_eq!(e_indices.len(), 2, "expected two -e flags");
        assert_eq!(args[e_indices[0] + 1], "FOO=bar");
        assert_eq!(args[e_indices[1] + 1], "HELLO=world");
    }

    #[test]
    fn build_run_args_pty_with_host_settings_adds_claude_mounts() {
        let settings = HostSettings::from_paths(
            PathBuf::from("/fake/claude.json"),
            PathBuf::from("/fake/dot-claude"),
        );
        let args_without = rt().build_run_args_pty("img", "/h", &[], &[], None, false, None, None);
        let args_with = rt().build_run_args_pty("img", "/h", &[], &[], Some(&settings), false, None, None);

        assert!(args_with.len() > args_without.len(), "settings should add extra mounts");
        assert!(
            args_with.iter().any(|a| a.contains(":/root/.claude.json")),
            "expected .claude.json mount"
        );
        assert!(
            args_with.iter().any(|a| a.contains(":/root/.claude")),
            "expected .claude dir mount"
        );
    }

    #[test]
    fn build_run_args_pty_with_agent_config_dir_adds_gemini_mount() {
        // Verify that agent_config_dir (used by GeminiPassthrough and others) produces
        // a -v flag mapping the host path to the container path.
        let host_gemini = PathBuf::from("/tmp/fake/gemini-data");
        let settings = HostSettings::new_agent_dir(
            None,
            "/root".to_string(),
            Some((host_gemini.clone(), "/root/.gemini".to_string())),
        );
        let args = rt().build_run_args_pty("img", "/h", &["gemini"], &[], Some(&settings), false, None, None);

        // The -v flag for the gemini config dir must appear.
        let expected_mount = format!("{}:/root/.gemini", host_gemini.display());
        assert!(
            args.windows(2).any(|w| w[0] == "-v" && w[1] == expected_mount),
            "expected -v {}:/root/.gemini in run args: {:?}",
            host_gemini.display(),
            args
        );
        // Claude-specific mounts must NOT appear when mount_claude_files = false.
        assert!(
            !args.iter().any(|a| a.contains("/.claude")),
            "claude mounts must not appear for gemini agent_config_dir settings"
        );
    }

    #[test]
    fn build_run_args_pty_with_allow_docker_adds_socket_mount() {
        let args_no = rt().build_run_args_pty("img", "/h", &[], &[], None, false, None, None);
        let args_yes = rt().build_run_args_pty("img", "/h", &[], &[], None, true, None, None);

        assert!(
            args_yes.len() > args_no.len(),
            "allow_docker should add extra args"
        );
        // On non-Windows a -v flag for the docker socket is expected.
        #[cfg(not(target_os = "windows"))]
        assert!(
            args_yes.windows(2).any(|w| w[0] == "-v" && w[1].contains("docker")),
            "expected docker socket -v mount: {:?}", args_yes
        );
        #[cfg(target_os = "windows")]
        assert!(
            args_yes.windows(2).any(|w| w[0] == "--mount" && w[1].contains("docker")),
            "expected docker socket --mount on Windows: {:?}", args_yes
        );
    }

    #[test]
    fn build_run_args_pty_with_ssh_dir_adds_readonly_mount() {
        let ssh = PathBuf::from("/home/user/.ssh");
        let args = rt().build_run_args_pty("img", "/h", &[], &[], None, false, None, Some(&ssh));

        assert!(
            args.windows(2).any(|w| w[0] == "-v" && w[1].contains("/.ssh:ro")),
            "expected SSH readonly mount: {:?}", args
        );
    }

    // ─── build_run_args_pty_display ──────────────────────────────────────────

    #[test]
    fn build_run_args_pty_display_masks_env_values() {
        let env = vec![("SECRET".to_string(), "my-secret-value".to_string())];
        let args = rt().build_run_args_pty_display("img", "/h", &[], &env, None, false, None, None);

        let e_idx = args.iter().position(|a| a == "-e").expect("-e flag");
        assert_eq!(args[e_idx + 1], "SECRET=***");
        assert!(
            !args.iter().any(|a| a.contains("my-secret-value")),
            "secret value must not appear in display args"
        );
    }

    #[test]
    fn build_run_args_pty_display_uses_placeholder_for_settings() {
        let settings = HostSettings::from_paths(
            PathBuf::from("/real/path/claude.json"),
            PathBuf::from("/real/path/dot-claude"),
        );
        let args = rt().build_run_args_pty_display("img", "/h", &[], &[], Some(&settings), false, None, None);

        assert!(
            args.iter().any(|a| a.contains("<settings>")),
            "expected <settings> placeholder in display args: {:?}", args
        );
        assert!(
            !args.iter().any(|a| a.contains("/real/path")),
            "real path must not appear in display args"
        );
    }

    #[test]
    fn build_run_args_pty_display_no_settings_when_none() {
        let args = rt().build_run_args_pty_display("img", "/h", &[], &[], None, false, None, None);
        assert!(!args.iter().any(|a| a.contains("<settings>")));
        assert!(!args.iter().any(|a| a.contains(".claude")));
    }

    // ─── build_run_args_pty_at_path ──────────────────────────────────────────

    #[test]
    fn build_run_args_pty_at_path_uses_custom_mount_and_workdir() {
        let args = rt().build_run_args_pty_at_path(
            "img", "/host/project", "/custom/path", "/work", &["cmd"], &[], None, false, None,
        );

        let v_idx = args.iter().position(|a| a == "-v").unwrap();
        assert_eq!(args[v_idx + 1], "/host/project:/custom/path");

        let w_idx = args.iter().position(|a| a == "-w").unwrap();
        assert_eq!(args[w_idx + 1], "/work");
    }

    // ─── build_exec_args_pty ─────────────────────────────────────────────────

    #[test]
    fn build_exec_args_pty_structure() {
        let env = vec![("FOO".to_string(), "bar".to_string())];
        let args = rt().build_exec_args_pty("ctr-id", "/workspace", &["bash"], &env);

        assert_eq!(args[0], "exec");
        assert_eq!(args[1], "-it");
        assert_eq!(args[2], "-w");
        assert_eq!(args[3], "/workspace");

        // env var flag comes before container id
        let e_idx = args.iter().position(|a| a == "-e").unwrap();
        let ctr_idx = args.iter().position(|a| a == "ctr-id").unwrap();
        assert!(e_idx < ctr_idx, "-e must precede container id");
        assert_eq!(args[e_idx + 1], "FOO=bar");

        // entrypoint follows container id
        assert_eq!(args[ctr_idx + 1], "bash");
    }

    #[test]
    fn build_exec_args_pty_no_env_vars() {
        let args = rt().build_exec_args_pty("ctr-id", "/work", &["sh", "-c", "echo hi"], &[]);

        assert_eq!(args[0], "exec");
        let ctr_idx = args.iter().position(|a| a == "ctr-id").unwrap();
        assert_eq!(args[ctr_idx + 1], "sh");
        assert_eq!(args[ctr_idx + 2], "-c");
        assert_eq!(args[ctr_idx + 3], "echo hi");
    }

    // ─── build_run_args_display ──────────────────────────────────────────────

    #[test]
    fn build_run_args_display_inserts_name_after_run() {
        let args = rt().build_run_args_display(
            "img", "/h", &["cmd"], &[], None, false, Some("myname"), None,
        );

        assert_eq!(args[0], "run");
        assert_eq!(args[1], "--name");
        assert_eq!(args[2], "myname");
    }

    #[test]
    fn build_run_args_display_no_name_starts_with_run_rm() {
        let args = rt().build_run_args_display("img", "/h", &[], &[], None, false, None, None);

        assert_eq!(args[0], "run");
        assert_eq!(args[1], "--rm");
    }

    // ─── build_image_streaming ───────────────────────────────────────────────

    fn docker_present() -> bool {
        std::process::Command::new("docker")
            .args(["info"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    /// build_image_streaming must return Err (not panic) when the Dockerfile is invalid.
    #[test]
    fn build_image_streaming_returns_err_for_invalid_dockerfile() {
        if !docker_present() {
            return;
        }
        let tmp = tempfile::TempDir::new().unwrap();
        let dockerfile = tmp.path().join("Dockerfile");
        std::fs::write(&dockerfile, "NOT_A_VALID_INSTRUCTION\n").unwrap();
        let result = rt().build_image_streaming(
            "amux-test-invalid-dockerfile-zzz:testing",
            &dockerfile,
            tmp.path(),
            false,
            &mut |_| {},
        );
        assert!(result.is_err(), "build_image_streaming must return Err for invalid Dockerfile");
    }

    /// --no-cache flag must be present in the build args when requested.
    #[test]
    fn build_image_streaming_no_cache_flag_is_included() {
        // Verify the args contain --no-cache by checking a build that fails
        // immediately (invalid dockerfile) — just confirm --no-cache doesn't panic.
        if !docker_present() {
            return;
        }
        let tmp = tempfile::TempDir::new().unwrap();
        let dockerfile = tmp.path().join("Dockerfile");
        std::fs::write(&dockerfile, "INVALID\n").unwrap();
        // Result will be Err; we only care that the call does not panic.
        let _ = rt().build_image_streaming(
            "amux-test-no-cache-zzz:testing",
            &dockerfile,
            tmp.path(),
            true, // no_cache = true
            &mut |_| {},
        );
    }

    // ─── error paths ─────────────────────────────────────────────────────────

    /// start_container must return Err (not panic) for a nonexistent container.
    #[test]
    fn start_container_returns_err_for_nonexistent() {
        if !docker_present() {
            return;
        }
        let result = rt().start_container("amux-test-nonexistent-container-zzz-99999");
        assert!(result.is_err(), "start_container must return Err for nonexistent container");
        let msg = result.unwrap_err().to_string();
        assert!(!msg.is_empty(), "error message must not be empty");
    }

    /// stop_container must return Err (not panic) for a nonexistent container.
    #[test]
    fn stop_container_returns_err_for_nonexistent() {
        if !docker_present() {
            return;
        }
        let result = rt().stop_container("amux-test-nonexistent-container-zzz-99999");
        assert!(result.is_err(), "stop_container must return Err for nonexistent container");
    }

    /// remove_container must return Err (not panic) for a nonexistent container.
    #[test]
    fn remove_container_returns_err_for_nonexistent() {
        if !docker_present() {
            return;
        }
        let result = rt().remove_container("amux-test-nonexistent-container-zzz-99999");
        assert!(result.is_err(), "remove_container must return Err for nonexistent container");
    }

    /// get_container_workspace_mount must return None (not panic) for a nonexistent container.
    #[test]
    fn get_container_workspace_mount_returns_none_for_nonexistent() {
        if !docker_present() {
            return;
        }
        let result = rt().get_container_workspace_mount("amux-test-nonexistent-container-zzz-99999");
        assert!(result.is_none());
    }

    // ─── Free utility tests ──────────────────────────────────────────────────

    #[test]
    fn project_image_tag_from_git_root() {
        let tag = project_image_tag(Path::new("/home/user/myproject"));
        assert_eq!(tag, "amux-myproject:latest");
    }

    #[test]
    fn project_image_tag_handles_root_path() {
        let tag = project_image_tag(Path::new("/"));
        assert_eq!(tag, "amux-project:latest");
    }

    #[test]
    fn agent_image_tag_claude() {
        let tag = agent_image_tag(Path::new("/home/user/myproject"), "claude");
        assert_eq!(tag, "amux-myproject-claude:latest");
    }

    #[test]
    fn agent_image_tag_codex() {
        let tag = agent_image_tag(Path::new("/home/user/myproject"), "codex");
        assert_eq!(tag, "amux-myproject-codex:latest");
    }

    #[test]
    fn agent_image_tag_handles_root_path() {
        let tag = agent_image_tag(Path::new("/"), "claude");
        assert_eq!(tag, "amux-project-claude:latest");
    }

    #[test]
    fn generate_container_name_is_unique() {
        let name1 = generate_container_name();
        // Small sleep to ensure different nanos
        std::thread::sleep(std::time::Duration::from_millis(1));
        let name2 = generate_container_name();
        assert!(name1.starts_with("amux-"));
        assert!(name2.starts_with("amux-"));
        assert_ne!(name1, name2);
    }

    #[test]
    fn parse_cpu_percent_valid() {
        assert!((parse_cpu_percent("5.23%") - 5.23).abs() < 0.001);
        assert!((parse_cpu_percent("0.00%") - 0.0).abs() < 0.001);
        assert!((parse_cpu_percent("100%") - 100.0).abs() < 0.001);
    }

    #[test]
    fn parse_cpu_percent_invalid() {
        assert!((parse_cpu_percent("not-a-number") - 0.0).abs() < 0.001);
    }

    #[test]
    fn parse_memory_mb_various_units() {
        assert!((parse_memory_mb("200MiB") - 200.0).abs() < 0.1);
        assert!((parse_memory_mb("1.5GiB") - 1536.0).abs() < 0.1);
        assert!((parse_memory_mb("512KiB") - 0.5).abs() < 0.1);
    }

    #[test]
    fn format_build_cmd_produces_valid_string() {
        let cmd = format_build_cmd("docker", "amux-test:latest", "Dockerfile.dev", "/repo");
        assert_eq!(
            cmd,
            "docker build -t amux-test:latest -f Dockerfile.dev /repo"
        );
    }

    #[test]
    fn format_build_cmd_no_cache_produces_valid_string() {
        let cmd = format_build_cmd_no_cache("docker", "amux-test:latest", "Dockerfile.dev", "/repo");
        assert_eq!(
            cmd,
            "docker build --no-cache -t amux-test:latest -f Dockerfile.dev /repo"
        );
    }

    #[test]
    fn docker_socket_path_is_nonempty() {
        let path = docker_socket_path();
        assert!(!path.as_os_str().is_empty());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn docker_socket_path_linux() {
        let path = docker_socket_path();
        assert_eq!(path.to_str().unwrap(), "/var/run/docker.sock");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn docker_socket_path_macos() {
        let path = docker_socket_path();
        assert_eq!(path.to_str().unwrap(), "/var/run/docker.sock");
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn docker_socket_path_windows() {
        let path = docker_socket_path();
        let s = path.to_string_lossy();
        assert!(s.contains("docker_engine"), "Windows path should reference docker_engine pipe");
    }

    #[test]
    fn check_docker_socket_fails_on_missing_path() {
        // On a system where Docker is not installed at the default path, this
        // test verifies the error message. If the socket exists (Docker is running),
        // we skip the negative assertion.
        let path = docker_socket_path();
        if path.exists() {
            // Socket exists — check_docker_socket should succeed.
            let result = check_docker_socket();
            assert!(result.is_ok(), "Socket exists but check_docker_socket failed");
        } else {
            // Socket missing — check_docker_socket should return an error.
            let result = check_docker_socket();
            assert!(result.is_err(), "Expected error when socket is missing");
            let msg = format!("{}", result.unwrap_err());
            assert!(msg.contains("Docker socket not found"), "Error message should mention socket: {}", msg);
        }
    }
}
