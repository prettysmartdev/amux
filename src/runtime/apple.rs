//! Apple Containers runtime — macOS only.
//!
//! Uses Apple's `container` CLI (shipping with macOS 26+) as the container
//! backend. The `container` CLI is similar to Docker for basic run/build/exec
//! operations but differs in several areas:
//!
//! - Availability check: `container system status` (not `container info`)
//! - Listing: `container list` (not `container ps`); only supports
//!   `--format json` or `--format table`, **not** Go templates
//! - Inspect: `container inspect` outputs raw JSON; no `--format` flag
//! - Stats: `container stats --format json` outputs raw bytes/microseconds;
//!   CPU% must be derived from two time-separated samples
//! - Container ID == container name (no separate short hex ID)

use anyhow::{bail, Context, Result};
use std::path::Path;
use std::process::{Command, Stdio};

use crate::runtime::{AgentRuntime, ContainerStats, HostSettings, StoppedContainerInfo};

/// Apple Containers-backed implementation of `AgentRuntime`.
///
/// Available on macOS only. Uses the `container` CLI that ships with
/// macOS 26 (Apple Containers framework).
pub struct AppleContainersRuntime;

impl AppleContainersRuntime {
    pub fn new() -> Self {
        AppleContainersRuntime
    }
}

impl Default for AppleContainersRuntime {
    fn default() -> Self {
        AppleContainersRuntime::new()
    }
}

// ─── Private helpers (mirrors docker free functions but uses "container" CLI) ──

/// Prints a one-time warning when the Docker socket is mounted into an Apple
/// Containers container. The socket mount is passed through as-is; whether the
/// runtime honours it depends on Apple Containers support, which may vary.
fn warn_allow_docker_with_apple() {
    eprintln!(
        "Warning: --allow-docker with Apple Containers mounts the host Docker socket \
         into the container. This is experimental and may be unsupported by this runtime."
    );
}

fn append_ssh_mount(args: &mut Vec<String>, ssh_dir: Option<&Path>) {
    if let Some(path) = ssh_dir {
        args.push("-v".to_string());
        args.push(format!("{}:/root/.ssh:ro", path.display()));
    }
}

fn append_docker_socket_mount_args(args: &mut Vec<String>) {
    // When running inside Apple Containers with allow_docker, mount the
    // Docker socket so nested Docker calls work.
    let path = crate::runtime::docker::docker_socket_path();
    let path_str = path.to_string_lossy().to_string();
    args.push("-v".into());
    args.push(format!("{}:{}", path_str, path_str));
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

/// Formats raw byte counts into human-readable IEC units (KiB / MiB / GiB).
fn format_bytes(bytes: u64) -> String {
    const KIB: u64 = 1024;
    const MIB: u64 = 1024 * KIB;
    const GIB: u64 = 1024 * MIB;
    if bytes >= GIB {
        format!("{:.1} GiB", bytes as f64 / GIB as f64)
    } else if bytes >= MIB {
        format!("{:.1} MiB", bytes as f64 / MIB as f64)
    } else if bytes >= KIB {
        format!("{:.1} KiB", bytes as f64 / KIB as f64)
    } else {
        format!("{} B", bytes)
    }
}

// ─── AgentRuntime impl ──────────────────────────────────────────────────────

impl AgentRuntime for AppleContainersRuntime {
    fn is_available(&self) -> bool {
        // `container info` does not exist in the Apple Container CLI.
        // The correct availability check is `container system status`.
        Command::new("container")
            .args(["system", "status"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
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

        let mut child = Command::new("container")
            .args(&build_args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("Failed to invoke `container build`")?;

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

        let status = child.wait().context("Failed to wait for `container build`")?;
        if !status.success() {
            bail!("`container build` failed:\n{}", combined);
        }
        Ok(combined)
    }

    fn image_exists(&self, tag: &str) -> bool {
        Command::new("container")
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
            warn_allow_docker_with_apple();
            append_docker_socket_mount_args(&mut args);
        }
        append_ssh_mount(&mut args, ssh_dir);
        append_env_args(&mut args, env_vars);
        append_entrypoint(&mut args, image, entrypoint);

        let status = Command::new("container")
            .args(&args)
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
            .context("Failed to invoke `container run`")?;

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
        let mut args: Vec<String> =
            vec!["run".into(), "--rm".into(), "-v".into(), format!("{}:/workspace", host_path), "-w".into(), "/workspace".into()];

        if let Some(name) = container_name {
            args.insert(1, "--name".to_string());
            args.insert(2, name.to_string());
        }

        if let Some(settings) = host_settings {
            append_settings_mounts(&mut args, settings);
        }
        if allow_docker {
            warn_allow_docker_with_apple();
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
        let cmd_line = format!("container {}", display_args.join(" "));

        let output = Command::new("container")
            .args(&args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .context("Failed to invoke `container run`")?;

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
            warn_allow_docker_with_apple();
            append_docker_socket_mount_args(&mut args);
        }
        append_env_args(&mut args, env_vars);
        append_entrypoint(&mut args, image, entrypoint);

        let status = Command::new("container")
            .args(&args)
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
            .context("Failed to invoke `container run`")?;

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
            warn_allow_docker_with_apple();
            append_docker_socket_mount_args(&mut args);
        }
        append_env_args(&mut args, env_vars);
        append_entrypoint(&mut args, image, entrypoint);

        let output = Command::new("container")
            .args(&args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .context("Failed to invoke `container run`")?;

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
        // Build a display-safe command line (env values masked), consistent with
        // run_container_captured which also returns (cmd_line, output).
        let masked_env: String = env_vars
            .iter()
            .flat_map(|(k, _)| [format!("-e {}=***", k)])
            .collect::<Vec<_>>()
            .join(" ");
        let cmd_line = format!(
            "container run --rm -v {}:{} -w {} {} {}",
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
            warn_allow_docker_with_apple();
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

        let output = Command::new("container")
            .args(&args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .context("Failed to invoke `container run -d`")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("Failed to start background container: {}", stderr.trim());
        }

        let container_id = String::from_utf8_lossy(&output.stdout).trim().to_string();
        Ok(container_id)
    }

    fn start_container(&self, container_id: &str) -> Result<()> {
        let output = Command::new("container")
            .args(["start", container_id])
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .output()
            .context("Failed to invoke `container start`")?;
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
        let output = Command::new("container")
            .args(["stop", container_id])
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .output()
            .context("Failed to invoke `container stop`")?;
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
        let output = Command::new("container")
            .args(["rm", "-f", container_id])
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .output()
            .context("Failed to invoke `container rm`")?;
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
        // Apple's `container inspect` does not support --format with Go templates.
        // It always outputs a raw JSON array of PrintableContainer objects.
        // The status field is a string: "running" | "stopped" | "stopping" | "unknown".
        let output = Command::new("container")
            .args(["inspect", container_id])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output()
            .ok();
        if let Some(output) = output {
            if output.status.success() {
                if let Ok(json) =
                    serde_json::from_slice::<serde_json::Value>(&output.stdout)
                {
                    if let Some(status) = json[0]["status"].as_str() {
                        return status == "running";
                    }
                }
            }
        }
        false
    }

    fn find_stopped_container(&self, name: &str, image: &str) -> Option<StoppedContainerInfo> {
        // Apple's CLI does not have `container ps` or Go-template --format.
        // Use `container list --all --format json` and parse the result.
        // JSON schema: [{status, configuration: {id, image: {...}}, startedDate: float|null}]
        let output = Command::new("container")
            .args(["list", "--all", "--format", "json"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output()
            .ok()?;

        if !output.status.success() {
            return None;
        }

        let json: serde_json::Value =
            serde_json::from_slice(&output.stdout).ok()?;
        let containers = json.as_array()?;

        for container in containers {
            let container_id =
                container["configuration"]["id"].as_str().unwrap_or("");
            let status = container["status"].as_str().unwrap_or("");

            if container_id != name {
                continue;
            }
            // Skip running or stopping containers.
            if status == "running" || status == "stopping" {
                continue;
            }
            // Match image: the image field is an object; do a loose substring
            // check on its serialized form so any reference format matches.
            let image_val = &container["configuration"]["image"];
            let image_json =
                serde_json::to_string(image_val).unwrap_or_default();
            if !image_json.contains(image) {
                continue;
            }

            let created = container["startedDate"]
                .as_f64()
                .map(|ts| format!("{:.0}", ts))
                .unwrap_or_else(|| "unknown".to_string());

            return Some(StoppedContainerInfo {
                id: container_id.to_string(),
                name: container_id.to_string(),
                created,
            });
        }
        None
    }

    fn list_running_containers_by_prefix(&self, prefix: &str) -> Vec<String> {
        // Apple's CLI has no `container ps` or Go-template --format.
        // Use `container list --format json` (shows only running by default).
        let output = Command::new("container")
            .args(["list", "--format", "json"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output();

        match output {
            Ok(out) if out.status.success() => {
                let json: serde_json::Value =
                    match serde_json::from_slice(&out.stdout) {
                        Ok(v) => v,
                        Err(_) => return vec![],
                    };
                let arr = match json.as_array() {
                    Some(a) => a,
                    None => return vec![],
                };
                arr.iter()
                    .filter_map(|c| {
                        let id = c["configuration"]["id"].as_str()?;
                        if id.starts_with(prefix) {
                            Some(id.to_string())
                        } else {
                            None
                        }
                    })
                    .collect()
            }
            _ => vec![],
        }
    }

    fn list_running_containers_with_ids_by_prefix(
        &self,
        prefix: &str,
    ) -> Vec<(String, String)> {
        // Apple's CLI has no `container ps` or Go-template --format.
        // In Apple containers the name IS the container identifier, so both
        // elements of the tuple carry the same value.
        let output = Command::new("container")
            .args(["list", "--format", "json"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output();

        match output {
            Ok(out) if out.status.success() => {
                let json: serde_json::Value =
                    match serde_json::from_slice(&out.stdout) {
                        Ok(v) => v,
                        Err(_) => return vec![],
                    };
                let arr = match json.as_array() {
                    Some(a) => a,
                    None => return vec![],
                };
                arr.iter()
                    .filter_map(|c| {
                        let id = c["configuration"]["id"].as_str()?;
                        if id.starts_with(prefix) {
                            Some((id.to_string(), id.to_string()))
                        } else {
                            None
                        }
                    })
                    .collect()
            }
            _ => vec![],
        }
    }

    fn get_container_workspace_mount(&self, container_name: &str) -> Option<String> {
        // Apple's `container inspect` does not support --format with Go templates.
        // It returns a raw JSON array of PrintableContainer objects.
        // Mounts are at configuration.mounts[].{source,destination}.
        let output = Command::new("container")
            .args(["inspect", container_name])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output()
            .ok()?;

        if !output.status.success() {
            return None;
        }

        let json: serde_json::Value =
            serde_json::from_slice(&output.stdout).ok()?;
        let mounts = json[0]["configuration"]["mounts"].as_array()?;
        for mount in mounts {
            if mount["destination"].as_str() == Some("/workspace") {
                let src = mount["source"].as_str().unwrap_or("").to_string();
                if !src.is_empty() {
                    return Some(src);
                }
            }
        }
        None
    }

    fn query_container_stats(&self, name: &str) -> Option<ContainerStats> {
        // Apple's `container stats` only accepts --format json or --format table,
        // not Go templates. The JSON output contains raw bytes and CPU microseconds.
        // CPU% requires two samples; we take them ~200 ms apart.
        let take_sample = |n: &str| -> Option<(u64, u64)> {
            let out = Command::new("container")
                .args(["stats", "--no-stream", "--format", "json", n])
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .output()
                .ok()?;
            if !out.status.success() {
                return None;
            }
            let json: serde_json::Value =
                serde_json::from_slice(&out.stdout).ok()?;
            let entry = json.as_array()?.first()?;
            let cpu = entry["cpuUsageUsec"].as_u64().unwrap_or(0);
            let mem = entry["memoryUsageBytes"].as_u64().unwrap_or(0);
            Some((cpu, mem))
        };

        let (cpu1, _) = take_sample(name)?;
        let t0 = std::time::Instant::now();
        std::thread::sleep(std::time::Duration::from_millis(200));
        let (cpu2, mem) = take_sample(name)?;
        let elapsed_usec = t0.elapsed().as_micros() as u64;

        let cpu_delta = cpu2.saturating_sub(cpu1);
        let cpu_percent = if elapsed_usec > 0 {
            (cpu_delta as f64 / elapsed_usec as f64) * 100.0
        } else {
            0.0
        };

        Some(ContainerStats {
            name: name.to_string(),
            cpu_percent: format!("{:.1}%", cpu_percent),
            memory: format_bytes(mem),
        })
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
        "apple-containers"
    }

    fn cli_binary(&self) -> &'static str {
        "container"
    }
}

// Unit tests for AppleContainersRuntime arg builders.
// These tests only compile on macOS (the module itself is cfg-gated).
#[cfg(test)]
mod tests {
    use super::AppleContainersRuntime;
    use crate::runtime::{AgentRuntime, HostSettings};
    use std::path::PathBuf;

    fn rt() -> AppleContainersRuntime {
        AppleContainersRuntime::new()
    }

    // ─── name / cli_binary ───────────────────────────────────────────────────

    #[test]
    fn apple_runtime_name_and_cli_binary() {
        let r = rt();
        assert_eq!(r.name(), "apple-containers");
        assert_eq!(r.cli_binary(), "container");
    }

    // ─── build_run_args_pty ──────────────────────────────────────────────────

    #[test]
    fn build_run_args_pty_minimal_structure() {
        let args = rt().build_run_args_pty("myimage", "/host", &["claude"], &[], None, false, None, None);

        assert_eq!(args[0], "run");
        assert!(args.contains(&"--rm".to_string()));
        assert!(args.contains(&"-it".to_string()));
        let v_idx = args.iter().position(|a| a == "-v").expect("-v flag");
        assert_eq!(args[v_idx + 1], "/host:/workspace");
        let w_idx = args.iter().position(|a| a == "-w").expect("-w flag");
        assert_eq!(args[w_idx + 1], "/workspace");
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
        assert_eq!(e_indices.len(), 2);
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

        assert!(args_with.len() > args_without.len());
        assert!(args_with.iter().any(|a| a.contains(":/root/.claude.json")));
        assert!(args_with.iter().any(|a| a.contains(":/root/.claude")));
    }

    #[test]
    fn build_run_args_pty_with_allow_docker_adds_socket_mount() {
        let args_no = rt().build_run_args_pty("img", "/h", &[], &[], None, false, None, None);
        let args_yes = rt().build_run_args_pty("img", "/h", &[], &[], None, true, None, None);

        assert!(args_yes.len() > args_no.len());
        // AppleContainersRuntime always uses -v (no Windows pipe variant)
        assert!(
            args_yes.windows(2).any(|w| w[0] == "-v" && w[1].contains("docker")),
            "expected docker socket -v mount: {:?}", args_yes
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
        let env = vec![("TOKEN".to_string(), "super-secret".to_string())];
        let args = rt().build_run_args_pty_display("img", "/h", &[], &env, None, false, None, None);

        let e_idx = args.iter().position(|a| a == "-e").unwrap();
        assert_eq!(args[e_idx + 1], "TOKEN=***");
        assert!(!args.iter().any(|a| a.contains("super-secret")));
    }

    #[test]
    fn build_run_args_pty_display_uses_placeholder_for_settings() {
        let settings = HostSettings::from_paths(
            PathBuf::from("/real/path/claude.json"),
            PathBuf::from("/real/path/dot-claude"),
        );
        let args = rt().build_run_args_pty_display("img", "/h", &[], &[], Some(&settings), false, None, None);

        assert!(args.iter().any(|a| a.contains("<settings>")));
        assert!(!args.iter().any(|a| a.contains("/real/path")));
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
        let env = vec![("VAR".to_string(), "val".to_string())];
        let args = rt().build_exec_args_pty("ctr-id", "/workspace", &["bash"], &env);

        assert_eq!(args[0], "exec");
        assert_eq!(args[1], "-it");
        assert_eq!(args[2], "-w");
        assert_eq!(args[3], "/workspace");
        let e_idx = args.iter().position(|a| a == "-e").unwrap();
        let ctr_idx = args.iter().position(|a| a == "ctr-id").unwrap();
        assert!(e_idx < ctr_idx);
        assert_eq!(args[ctr_idx + 1], "bash");
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

    // ─── error paths ─────────────────────────────────────────────────────────

    fn container_cli_present() -> bool {
        std::process::Command::new("container")
            .args(["system", "status"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    /// start_container must return Err (not panic) for a nonexistent container.
    #[test]
    fn start_container_returns_err_for_nonexistent() {
        if !container_cli_present() {
            return;
        }
        let result = rt().start_container("amux-test-nonexistent-container-zzz-99999");
        assert!(result.is_err(), "start_container must return Err for nonexistent container");
        let msg = result.unwrap_err().to_string();
        assert!(!msg.is_empty());
    }

    /// stop_container must return Err (not panic) for a nonexistent container.
    #[test]
    fn stop_container_returns_err_for_nonexistent() {
        if !container_cli_present() {
            return;
        }
        let result = rt().stop_container("amux-test-nonexistent-container-zzz-99999");
        assert!(result.is_err(), "stop_container must return Err for nonexistent container");
    }

    /// remove_container must return Err (not panic) for a nonexistent container.
    #[test]
    fn remove_container_returns_err_for_nonexistent() {
        if !container_cli_present() {
            return;
        }
        let result = rt().remove_container("amux-test-nonexistent-container-zzz-99999");
        assert!(result.is_err(), "remove_container must return Err for nonexistent container");
    }

    /// get_container_workspace_mount must return None (not panic) for a nonexistent container.
    #[test]
    fn get_container_workspace_mount_returns_none_for_nonexistent() {
        if !container_cli_present() {
            return;
        }
        let result =
            rt().get_container_workspace_mount("amux-test-nonexistent-container-zzz-99999");
        assert!(result.is_none());
    }
}
