use anyhow::Result;
use std::path::{Path, PathBuf};

pub mod docker;

#[cfg(target_os = "macos")]
pub mod apple;

// Re-export the DockerRuntime for convenience.
#[allow(unused_imports)]
pub use docker::DockerRuntime;

/// Docker container stats returned by container stats commands.
#[derive(Debug, Clone)]
pub struct ContainerStats {
    pub name: String,
    pub cpu_percent: String,
    pub memory: String,
}

/// Parses a single formatted stats line into a [`ContainerStats`].
///
/// Expected format: `"name|cpu_percent|mem_usage/mem_limit"` — the format
/// produced by Docker when invoked with
/// `stats --no-stream --format "{{.Name}}|{{.CPUPerc}}|{{.MemUsage}}"`.
///
/// Note: Apple Containers does not support Go-template `--format`; its
/// stats path uses JSON and is handled directly in `apple::AppleContainersRuntime`.
///
/// Returns `None` for empty input or when the line cannot be split into
/// exactly three `|`-separated fields.
pub(crate) fn parse_stats_line(line: &str) -> Option<ContainerStats> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }
    let parts: Vec<&str> = line.split('|').collect();
    if parts.len() != 3 {
        return None;
    }
    Some(ContainerStats {
        name: parts[0].to_string(),
        cpu_percent: parts[1].trim().to_string(),
        memory: parts[2]
            .split('/')
            .next()
            .unwrap_or("?")
            .trim()
            .to_string(),
    })
}

/// Info about a stopped (non-running) container.
#[derive(Debug, Clone)]
pub struct StoppedContainerInfo {
    pub id: String,
    pub name: String,
    pub created: String,
}

/// Host-machine agent settings prepared for injection into a container.
///
/// Stores sanitized config files in a temporary directory. The temp directory is
/// automatically cleaned up when this struct is dropped (RAII via `tempfile::TempDir`).
///
/// For claude: bind-mounts `<container_home>/.claude.json` and `<container_home>/.claude`.
/// For other agents (e.g. opencode): only `agent_config_dir` is mounted.
pub struct HostSettings {
    /// Kept alive so the temp dir survives as long as the container runs.
    /// `None` when created via `from_paths` or `clone_view` (caller manages the directory).
    _temp_dir: Option<tempfile::TempDir>,
    /// Path to the sanitized `.claude.json` inside the temp dir (claude-only).
    /// Empty when `mount_claude_files` is false.
    pub config_path: PathBuf,
    /// Path to the copied `.claude/` directory inside the temp dir (claude-only).
    /// Empty when `mount_claude_files` is false.
    pub claude_dir_path: PathBuf,
    /// Home directory path inside the container for mounting agent settings.
    /// Defaults to `/root`. Set to `/home/<username>` when `Dockerfile.dev`
    /// specifies a non-root USER directive.
    pub container_home: String,
    /// When `true`, the `.claude.json` and `.claude/` bind-mounts are added to the
    /// container run args. Set to `false` for non-claude agents (e.g. opencode).
    pub mount_claude_files: bool,
    /// Optional agent-specific config directory to bind-mount (read-write).
    ///
    /// `(host_path, container_path)`. Example for opencode:
    /// `(~/.local/share/opencode/, /root/.local/share/opencode)`.
    /// Mounted read-write because the source is a temp copy, not the live host directory.
    pub agent_config_dir: Option<(PathBuf, String)>,
}

/// Top-level entries in `~/.claude/` that are large, host-specific, or
/// irrelevant inside a container. Everything else is copied.
const CLAUDE_DIR_DENYLIST: &[&str] = &[
    "projects",
    "sessions",
    "session-env",
    "debug",
    "file-history",
    "history.jsonl",
    "telemetry",
    "downloads",
    "ide",
    "shell-snapshots",
    "paste-cache",
];

impl HostSettings {
    /// Reads and sanitizes host agent settings for container injection.
    ///
    /// - Reads `~/.claude.json`, strips `oauthAccount`, adds `/workspace` project trust
    /// - Copies `~/.claude/` (filtered) into a temp directory
    ///
    /// Returns `None` if the agent is not `claude` or the host has no config.
    pub fn prepare(agent: &str) -> Option<Self> {
        if agent != "claude" {
            return None;
        }

        let home = dirs::home_dir()?;
        let host_config_file = home.join(".claude.json");
        if !host_config_file.exists() {
            return None;
        }

        let temp_dir = tempfile::TempDir::new().ok()?;

        // Sanitize .claude.json — strip oauthAccount to prevent broken
        // OAuth state (tokens are in macOS keychain, inaccessible from container).
        let raw = std::fs::read_to_string(&host_config_file).ok()?;
        let mut parsed: serde_json::Value = serde_json::from_str(&raw).ok()?;
        if let Some(obj) = parsed.as_object_mut() {
            obj.remove("oauthAccount");

            // Ensure /workspace project trust for the container environment.
            // Without this, Claude Code shows the trust dialog inside the container.
            let projects = obj
                .entry("projects")
                .or_insert_with(|| serde_json::json!({}));
            if let Some(projects_obj) = projects.as_object_mut() {
                projects_obj.insert(
                    "/workspace".to_string(),
                    serde_json::json!({"hasTrustDialogAccepted": true}),
                );
            }
        }
        let config_json = serde_json::to_string(&parsed).ok()?;
        let config_path = temp_dir.path().join("claude.json");
        std::fs::write(&config_path, &config_json).ok()?;

        // Copy ~/.claude/ directory with denylist filter.
        let claude_dir_path = temp_dir.path().join("dot-claude");
        let host_claude_dir = home.join(".claude");
        if host_claude_dir.is_dir() {
            copy_dir_filtered(&host_claude_dir, &claude_dir_path, CLAUDE_DIR_DENYLIST).ok()?;
        } else {
            // Create an empty directory so the mount target exists.
            std::fs::create_dir_all(&claude_dir_path).ok()?;
        }
        disable_lsp_recommendations(&claude_dir_path).ok()?;

        Some(HostSettings {
            _temp_dir: Some(temp_dir),
            config_path,
            claude_dir_path,
            container_home: "/root".to_string(),
            mount_claude_files: true,
            agent_config_dir: None,
        })
    }

    /// Prepares host agent settings into a caller-supplied stable directory.
    ///
    /// Identical to `prepare` but writes into `dir` instead of a temp directory,
    /// so the bind-mount sources survive process restarts and container stops.
    /// Use this when the container may be stopped and restarted later.
    pub fn prepare_to_dir(agent: &str, dir: &Path) -> Option<Self> {
        if agent != "claude" {
            return None;
        }

        let home = dirs::home_dir()?;
        let host_config_file = home.join(".claude.json");
        if !host_config_file.exists() {
            return None;
        }

        std::fs::create_dir_all(dir).ok()?;

        let raw = std::fs::read_to_string(&host_config_file).ok()?;
        let mut parsed: serde_json::Value = serde_json::from_str(&raw).ok()?;
        if let Some(obj) = parsed.as_object_mut() {
            obj.remove("oauthAccount");
            let projects = obj
                .entry("projects")
                .or_insert_with(|| serde_json::json!({}));
            if let Some(projects_obj) = projects.as_object_mut() {
                projects_obj.insert(
                    "/workspace".to_string(),
                    serde_json::json!({"hasTrustDialogAccepted": true}),
                );
            }
        }
        let config_json = serde_json::to_string(&parsed).ok()?;
        let config_path = dir.join("claude.json");
        std::fs::write(&config_path, &config_json).ok()?;

        let claude_dir_path = dir.join("dot-claude");
        let host_claude_dir = home.join(".claude");
        if host_claude_dir.is_dir() {
            copy_dir_filtered(&host_claude_dir, &claude_dir_path, CLAUDE_DIR_DENYLIST).ok()?;
        } else {
            std::fs::create_dir_all(&claude_dir_path).ok()?;
        }
        disable_lsp_recommendations(&claude_dir_path).ok()?;

        Some(HostSettings {
            _temp_dir: None,
            config_path,
            claude_dir_path,
            container_home: "/root".to_string(),
            mount_claude_files: true,
            agent_config_dir: None,
        })
    }

    /// Creates a `HostSettings` pointing to existing files without owning a temp directory.
    ///
    /// Used when the backing directory is owned elsewhere (e.g. stored in `App::host_settings`
    /// across task boundaries). The caller must ensure the paths remain valid for the
    /// lifetime of this value and any container that references them.
    pub fn from_paths(config_path: PathBuf, claude_dir_path: PathBuf) -> Self {
        HostSettings {
            _temp_dir: None,
            config_path,
            claude_dir_path,
            container_home: "/root".to_string(),
            mount_claude_files: true,
            agent_config_dir: None,
        }
    }

    /// Creates a `HostSettings` for non-claude agents that use a single config directory.
    ///
    /// Sets `mount_claude_files = false` so the `.claude.json` and `.claude/` bind-mounts
    /// are skipped. The `agent_config_dir` mount is used instead (e.g. opencode's
    /// `~/.local/share/opencode/`). `config_path` and `claude_dir_path` are set to empty
    /// placeholder values that are never mounted.
    pub(crate) fn new_agent_dir(
        temp_dir: Option<tempfile::TempDir>,
        container_home: String,
        agent_config_dir: Option<(PathBuf, String)>,
    ) -> Self {
        HostSettings {
            _temp_dir: temp_dir,
            config_path: PathBuf::new(),
            claude_dir_path: PathBuf::new(),
            container_home,
            mount_claude_files: false,
            agent_config_dir,
        }
    }

    /// Creates a non-owning view of `self` for use in closures.
    ///
    /// The backing `TempDir` is NOT included — the caller must ensure that the
    /// original `HostSettings` (which owns the `TempDir`) stays alive for as long as
    /// this view is used. Preserves `mount_claude_files` and `agent_config_dir`.
    pub fn clone_view(&self) -> Self {
        HostSettings {
            _temp_dir: None,
            config_path: self.config_path.clone(),
            claude_dir_path: self.claude_dir_path.clone(),
            container_home: self.container_home.clone(),
            mount_claude_files: self.mount_claude_files,
            agent_config_dir: self.agent_config_dir.clone(),
        }
    }

    /// Sets `skipDangerousModePermissionPrompt: true` in the container's `settings.json`.
    ///
    /// Claude Code shows a one-time confirmation dialog when first launched with
    /// `--dangerously-skip-permissions`. Setting this key suppresses the dialog so
    /// unattended `--yolo` runs are not blocked waiting for user input.
    ///
    /// No-op for non-claude agents (`mount_claude_files == false`).
    pub fn apply_yolo_settings(&self) -> std::io::Result<()> {
        if !self.mount_claude_files {
            return Ok(()); // Not a Claude agent; no yolo settings file to modify.
        }
        let settings_path = self.claude_dir_path.join("settings.json");
        let mut settings: serde_json::Value = if settings_path.exists() {
            let raw = std::fs::read_to_string(&settings_path)?;
            serde_json::from_str(&raw).unwrap_or(serde_json::json!({}))
        } else {
            serde_json::json!({})
        };
        if let Some(obj) = settings.as_object_mut() {
            obj.insert("skipDangerousModePermissionPrompt".to_string(), serde_json::json!(true));
        }
        std::fs::write(&settings_path, serde_json::to_string(&settings)?)
    }

    /// Creates a minimal `HostSettings` with only LSP recommendations disabled.
    ///
    /// Used as a fallback when the host has no `~/.claude.json` (e.g. the user has never
    /// run Claude Code on this machine). This ensures LSP recommendation dialogs are always
    /// suppressed inside containers, even without a full host config. Only applies to the
    /// `claude` agent — returns `None` for all others.
    pub fn prepare_minimal(agent: &str) -> Option<Self> {
        if agent != "claude" {
            return None;
        }
        let temp_dir = tempfile::TempDir::new().ok()?;
        let config_path = temp_dir.path().join("claude.json");
        // Write a minimal valid config so the bind-mount target exists.
        std::fs::write(&config_path, "{}").ok()?;
        let claude_dir_path = temp_dir.path().join("dot-claude");
        std::fs::create_dir_all(&claude_dir_path).ok()?;
        disable_lsp_recommendations(&claude_dir_path).ok()?;
        Some(HostSettings {
            _temp_dir: Some(temp_dir),
            config_path,
            claude_dir_path,
            container_home: "/root".to_string(),
            mount_claude_files: true,
            agent_config_dir: None,
        })
    }
}

/// Scans a Dockerfile from the end upwards to find the last `USER` directive.
///
/// Returns the username as a `String` if a `USER` instruction is found,
/// or `None` if no `USER` directive exists in the file.
///
/// Dockerfile instructions are case-insensitive per the spec, so both
/// `USER agent` and `user agent` are recognised.
pub fn parse_dockerfile_user(dockerfile_path: &Path) -> Option<String> {
    let content = std::fs::read_to_string(dockerfile_path).ok()?;
    for line in content.lines().rev() {
        let trimmed = line.trim();
        let upper = trimmed.to_ascii_uppercase();
        if upper.starts_with("USER") {
            let rest = trimmed[4..].trim();
            if !rest.is_empty() {
                return Some(rest.to_string());
            }
        }
    }
    None
}

/// Returns the container home directory path for a given username.
///
/// `"root"` maps to `/root`; any other username maps to `/home/<username>`.
pub fn container_home_for_user(username: &str) -> String {
    if username == "root" {
        "/root".to_string()
    } else {
        format!("/home/{}", username)
    }
}

/// Detects the last USER directive in `Dockerfile.dev` and, if it names a non-root user,
/// updates `container_home` in `settings` and returns a message to display to the user.
///
/// Also remaps any `agent_config_dir` container path that starts with `/root/` to the
/// correct home directory for the detected user (e.g. `/home/agent/.local/share/opencode`).
///
/// Returns `None` if the file has no USER directive, if the effective user is `root`, or
/// if the file cannot be read. Settings are mutated only when a non-root user is found.
pub fn apply_dockerfile_user(settings: &mut HostSettings, dockerfile_path: &Path) -> Option<String> {
    let user = parse_dockerfile_user(dockerfile_path)?;
    if user == "root" {
        return None;
    }
    let home = container_home_for_user(&user);
    settings.container_home = home.clone();
    // Remap agent_config_dir container path from /root/ to the correct home.
    if let Some((host_path, container_path)) = settings.agent_config_dir.take() {
        let new_container_path = if let Some(relative) = container_path.strip_prefix("/root/") {
            format!("{}/{}", home, relative)
        } else {
            container_path
        };
        settings.agent_config_dir = Some((host_path, new_container_path));
    }
    Some(format!(
        "Dockerfile.dev sets USER to '{}'; mounting agent settings at {}",
        user, home
    ))
}

/// The `settings.json` key that tells Claude Code not to show LSP recommendation dialogs.
///
/// Confirmed empirically: after dismissing the LSP dialog inside a container,
/// Claude Code writes this key to `~/.claude/settings.json`.
pub(crate) const LSP_SETTINGS_KEY: &str = "hasShownLspRecommendation";

/// The dead key written by older versions of amux — has no effect in Claude Code.
pub(crate) const LSP_SETTINGS_KEY_DEAD: &str = "lspRecommendationDisabled";

/// Disables LSP recommendations in the `settings.json` inside the copied claude dir.
///
/// Claude Code prompts the user to install language servers when it detects
/// missing LSP support. Inside a container there is no IDE and no pre-installed
/// language servers, so these recommendations are noise. This sets
/// `hasShownLspRecommendation: true` in the container's `settings.json` to suppress them,
/// and removes the old dead key written by prior versions of amux.
pub(crate) fn disable_lsp_recommendations(claude_dir: &Path) -> std::io::Result<()> {
    let settings_path = claude_dir.join("settings.json");
    let mut settings: serde_json::Value = if settings_path.exists() {
        let raw = std::fs::read_to_string(&settings_path)?;
        serde_json::from_str(&raw).unwrap_or(serde_json::json!({}))
    } else {
        serde_json::json!({})
    };
    if let Some(obj) = settings.as_object_mut() {
        obj.insert(LSP_SETTINGS_KEY.to_string(), serde_json::json!(true));
        obj.remove(LSP_SETTINGS_KEY_DEAD);
    }
    std::fs::write(&settings_path, serde_json::to_string(&settings)?)
}

/// Recursively copy `src` to `dst`, skipping top-level entries in `denylist`.
pub(crate) fn copy_dir_filtered(src: &Path, dst: &Path, denylist: &[&str]) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if denylist.iter().any(|d| *d == name_str.as_ref()) {
            continue;
        }
        let src_path = entry.path();
        let dst_path = dst.join(&name);
        if src_path.is_dir() {
            // No denylist for nested directories — only filter at the top level.
            copy_dir_filtered(&src_path, &dst_path, &[])?;
        } else {
            std::fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}

/// The `AgentRuntime` trait abstracts over container runtimes (Docker, Apple Containers, etc.).
///
/// All methods are object-safe so the runtime can be stored as `Arc<dyn AgentRuntime>`.
pub trait AgentRuntime: Send + Sync {
    /// Returns true if the runtime daemon is available on this host.
    fn is_available(&self) -> bool;

    /// Builds a container image from the given Dockerfile and context directory,
    /// streaming output lines to `on_line` as they are produced.
    fn build_image_streaming(
        &self,
        tag: &str,
        dockerfile: &Path,
        context: &Path,
        no_cache: bool,
        on_line: &mut dyn FnMut(&str),
    ) -> Result<String>;

    /// Returns true if the given image tag exists locally.
    fn image_exists(&self, tag: &str) -> bool;

    /// Runs a container interactively (stdin/stdout/stderr inherited).
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
    ) -> Result<()>;

    /// Runs a container and captures stdout+stderr.
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
    ) -> Result<(String, String)>;

    /// Runs a container with a custom mount path (interactive).
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
    ) -> Result<()>;

    /// Runs a container with a custom mount path, capturing output.
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
    ) -> Result<(String, String)>;

    /// Runs a container in detached mode, returning the container ID.
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
    ) -> Result<String>;

    /// Starts a stopped container.
    fn start_container(&self, container_id: &str) -> Result<()>;

    /// Gracefully stops a running container.
    fn stop_container(&self, container_id: &str) -> Result<()>;

    /// Force-removes a container.
    fn remove_container(&self, container_id: &str) -> Result<()>;

    /// Returns true if the container is currently running.
    fn is_container_running(&self, container_id: &str) -> bool;

    /// Finds a stopped container matching the given name and image.
    fn find_stopped_container(&self, name: &str, image: &str) -> Option<StoppedContainerInfo>;

    /// Lists names of running containers whose name starts with `prefix`.
    fn list_running_containers_by_prefix(&self, prefix: &str) -> Vec<String>;

    /// Lists `(name, id)` pairs of running containers whose name starts with `prefix`.
    fn list_running_containers_with_ids_by_prefix(&self, prefix: &str) -> Vec<(String, String)>;

    /// Returns the host source path of the `/workspace` bind-mount for `container_name`.
    fn get_container_workspace_mount(&self, container_name: &str) -> Option<String>;

    /// Queries container stats (CPU, memory) for a named container.
    fn query_container_stats(&self, name: &str) -> Option<ContainerStats>;

    /// Builds PTY `run` args for the runtime CLI.
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
    ) -> Vec<String>;

    /// Builds display-safe PTY `run` args (env values masked).
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
    ) -> Vec<String>;

    /// Builds PTY `run` args with custom mount path.
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
    ) -> Vec<String>;

    /// Builds `exec` PTY args for attaching to a running container.
    fn build_exec_args_pty(
        &self,
        container_id: &str,
        working_dir: &str,
        entrypoint: &[&str],
        env_vars: &[(String, String)],
    ) -> Vec<String>;

    /// Builds display-safe `run` args.
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
    ) -> Vec<String>;

    /// Returns the human-readable name of this runtime (e.g. "docker").
    fn name(&self) -> &'static str;

    /// Returns the CLI binary name for this runtime (e.g. "docker" or "container").
    fn cli_binary(&self) -> &'static str;
}

/// Resolves the configured agent runtime.
///
/// Reads `config.runtime` and returns an `Arc<dyn AgentRuntime>` for the
/// appropriate backend.
///
/// # Errors
///
/// Returns `Err` if the configured runtime is not supported on the current
/// platform (e.g. `"apple-containers"` requested on Linux or Windows).
/// Returns `Ok(DockerRuntime)` with a warning on stderr for unknown runtime
/// strings, so a config typo never silently breaks an existing installation.
pub fn resolve_runtime(
    config: &crate::config::GlobalConfig,
) -> anyhow::Result<std::sync::Arc<dyn AgentRuntime>> {
    let rt: std::sync::Arc<dyn AgentRuntime> = match config.runtime.as_deref().unwrap_or("docker") {
        #[cfg(target_os = "macos")]
        "apple-containers" => std::sync::Arc::new(apple::AppleContainersRuntime::new()),
        #[cfg(not(target_os = "macos"))]
        "apple-containers" => {
            anyhow::bail!(
                "'apple-containers' runtime requires macOS. \
                 Update or remove the `runtime` field in ~/.amux/config.json."
            );
        }
        "docker" | "" => std::sync::Arc::new(docker::DockerRuntime::new()),
        unknown => {
            eprintln!(
                "Warning: unknown runtime {:?} in config — falling back to Docker.",
                unknown
            );
            std::sync::Arc::new(docker::DockerRuntime::new())
        }
    };
    Ok(rt)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::TempDir;

    // ─── parse_stats_line ────────────────────────────────────────────────────

    /// Table-driven tests covering normal Docker and Apple Containers stat output.
    #[test]
    fn parse_stats_line_table_driven() {
        let cases: &[(&str, &str, &str, &str)] = &[
            // (input, expected_name, expected_cpu, expected_memory)
            // Typical Docker output
            ("mycontainer| 2.34%|512MiB / 2GiB", "mycontainer", "2.34%", "512MiB"),
            // Apple Containers output (same format)
            ("agent-abc| 0.1%|256MiB / 1GiB", "agent-abc", "0.1%", "256MiB"),
            // Zero CPU
            ("idle-ctr| 0.00%|1.5GiB / 8GiB", "idle-ctr", "0.00%", "1.5GiB"),
            // High CPU with extra spaces around cpu field
            ("busy-ctr|  95.5% |128MiB / 512MiB", "busy-ctr", "95.5%", "128MiB"),
            // No limit shown (no slash in memory field)
            ("mycontainer|0%|512MiB", "mycontainer", "0%", "512MiB"),
            // amux-prefixed container name as produced in practice
            ("amux-myproject| 3.2%|200MiB / 4GiB", "amux-myproject", "3.2%", "200MiB"),
        ];
        for (input, exp_name, exp_cpu, exp_mem) in cases {
            let stats = parse_stats_line(input)
                .unwrap_or_else(|| panic!("parse_stats_line({:?}) returned None", input));
            assert_eq!(stats.name, *exp_name, "name mismatch for {:?}", input);
            assert_eq!(stats.cpu_percent, *exp_cpu, "cpu mismatch for {:?}", input);
            assert_eq!(stats.memory, *exp_mem, "memory mismatch for {:?}", input);
        }
    }

    /// Edge cases that must return `None`.
    #[test]
    fn parse_stats_line_invalid_inputs_return_none() {
        let cases: &[&str] = &[
            "",           // empty string
            "   ",        // whitespace only
            "name|cpu",   // missing memory field (2 parts)
            "name|cpu|mem|extra", // too many fields (4 parts)
            "|",          // only separator — yields 2 empty parts
        ];
        for input in cases {
            assert!(
                parse_stats_line(input).is_none(),
                "Expected None for {:?}, got Some",
                input
            );
        }
    }

    #[test]
    fn parse_stats_line_trims_surrounding_whitespace() {
        // Leading/trailing whitespace on the whole line is stripped.
        let stats = parse_stats_line("  mycontainer| 1.0%|100MiB / 2GiB  ").unwrap();
        assert_eq!(stats.name, "mycontainer");
        assert_eq!(stats.cpu_percent, "1.0%");
        assert_eq!(stats.memory, "100MiB");
    }

    #[test]
    fn parse_stats_line_memory_without_slash_returns_full_field() {
        let stats = parse_stats_line("c|0.5%|noSlashHere").unwrap();
        assert_eq!(stats.memory, "noSlashHere");
    }

    // ─── copy_dir_filtered ───────────────────────────────────────────────────

    #[test]
    fn copy_dir_filtered_copies_non_denied_files() {
        let src = TempDir::new().unwrap();
        let dst = TempDir::new().unwrap();

        std::fs::write(src.path().join("allowed.txt"), "content").unwrap();
        std::fs::write(src.path().join("denied.txt"), "secret").unwrap();

        copy_dir_filtered(src.path(), dst.path(), &["denied.txt"]).unwrap();

        assert!(dst.path().join("allowed.txt").exists(), "allowed.txt should be copied");
        assert!(!dst.path().join("denied.txt").exists(), "denied.txt should be skipped");
    }

    #[test]
    fn copy_dir_filtered_skips_denied_directories() {
        let src = TempDir::new().unwrap();
        let dst = TempDir::new().unwrap();

        std::fs::create_dir(src.path().join("projects")).unwrap();
        std::fs::write(src.path().join("projects").join("data.txt"), "data").unwrap();
        std::fs::write(src.path().join("keep.txt"), "keep").unwrap();

        copy_dir_filtered(src.path(), dst.path(), &["projects"]).unwrap();

        assert!(!dst.path().join("projects").exists(), "denied dir should be skipped");
        assert!(dst.path().join("keep.txt").exists(), "keep.txt should be copied");
    }

    #[test]
    fn copy_dir_filtered_copies_nested_dirs_without_nested_denylist() {
        // The denylist only applies at the top level; nested contents are always copied.
        let src = TempDir::new().unwrap();
        let dst = TempDir::new().unwrap();

        std::fs::create_dir(src.path().join("subdir")).unwrap();
        std::fs::write(src.path().join("subdir").join("file.txt"), "nested").unwrap();

        copy_dir_filtered(src.path(), dst.path(), &[]).unwrap();

        assert!(dst.path().join("subdir").join("file.txt").exists());
    }

    #[test]
    fn copy_dir_filtered_creates_destination_if_missing() {
        let src = TempDir::new().unwrap();
        let dst = TempDir::new().unwrap();
        let nested_dst = dst.path().join("new_subdir");

        std::fs::write(src.path().join("f.txt"), "x").unwrap();

        copy_dir_filtered(src.path(), &nested_dst, &[]).unwrap();

        assert!(nested_dst.join("f.txt").exists());
    }

    // ─── disable_lsp_recommendations ────────────────────────────────────────

    #[test]
    fn disable_lsp_recommendations_creates_settings_with_lsp_key() {
        let dir = TempDir::new().unwrap();
        disable_lsp_recommendations(dir.path()).unwrap();

        let settings_path = dir.path().join("settings.json");
        assert!(settings_path.exists());
        let content: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&settings_path).unwrap()).unwrap();
        assert_eq!(content[LSP_SETTINGS_KEY], serde_json::json!(true));
    }

    #[test]
    fn disable_lsp_recommendations_removes_dead_key() {
        let dir = TempDir::new().unwrap();
        let settings_path = dir.path().join("settings.json");

        let existing = serde_json::json!({
            LSP_SETTINGS_KEY_DEAD: true,
            "someOtherKey": "value"
        });
        std::fs::write(&settings_path, serde_json::to_string(&existing).unwrap()).unwrap();

        disable_lsp_recommendations(dir.path()).unwrap();

        let content: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&settings_path).unwrap()).unwrap();
        assert_eq!(content[LSP_SETTINGS_KEY], serde_json::json!(true));
        assert!(content.get(LSP_SETTINGS_KEY_DEAD).is_none(), "dead key should be removed");
        assert_eq!(content["someOtherKey"], "value");
    }

    #[test]
    fn disable_lsp_recommendations_preserves_existing_keys() {
        let dir = TempDir::new().unwrap();
        let settings_path = dir.path().join("settings.json");

        std::fs::write(
            &settings_path,
            serde_json::to_string(&serde_json::json!({"existingKey": "existingValue"})).unwrap(),
        )
        .unwrap();

        disable_lsp_recommendations(dir.path()).unwrap();

        let content: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&settings_path).unwrap()).unwrap();
        assert_eq!(content["existingKey"], "existingValue");
        assert_eq!(content[LSP_SETTINGS_KEY], serde_json::json!(true));
    }

    // ─── HostSettings ────────────────────────────────────────────────────────

    #[test]
    fn host_settings_from_paths_stores_paths() {
        let config_path = PathBuf::from("/tmp/test-claude.json");
        let claude_dir = PathBuf::from("/tmp/test-dot-claude");

        let settings = HostSettings::from_paths(config_path.clone(), claude_dir.clone());
        assert_eq!(settings.config_path, config_path);
        assert_eq!(settings.claude_dir_path, claude_dir);
    }

    #[test]
    fn host_settings_prepare_non_claude_agent_returns_none() {
        assert!(HostSettings::prepare("codex").is_none());
        assert!(HostSettings::prepare("gemini").is_none());
        assert!(HostSettings::prepare("").is_none());
    }

    #[test]
    fn host_settings_prepare_minimal_non_claude_returns_none() {
        assert!(HostSettings::prepare_minimal("codex").is_none());
        assert!(HostSettings::prepare_minimal("").is_none());
    }

    #[test]
    fn host_settings_prepare_minimal_claude_creates_settings_with_lsp_key() {
        // prepare_minimal always works (no ~/.claude.json required)
        if let Some(s) = HostSettings::prepare_minimal("claude") {
            assert!(s.config_path.exists(), "config path must exist");
            assert!(s.claude_dir_path.exists(), "claude dir must exist");
            let lsp_settings = s.claude_dir_path.join("settings.json");
            assert!(lsp_settings.exists(), "settings.json must exist");
            let content: serde_json::Value =
                serde_json::from_str(&std::fs::read_to_string(&lsp_settings).unwrap()).unwrap();
            assert_eq!(content[LSP_SETTINGS_KEY], serde_json::json!(true));
        }
    }

    // ─── resolve_runtime ─────────────────────────────────────────────────────

    #[test]
    fn resolve_runtime_none_defaults_to_docker() {
        let config = crate::config::GlobalConfig { runtime: None, ..Default::default() };
        let runtime = resolve_runtime(&config).unwrap();
        assert_eq!(runtime.name(), "docker");
        assert_eq!(runtime.cli_binary(), "docker");
    }

    #[test]
    fn resolve_runtime_explicit_docker_string() {
        let config = crate::config::GlobalConfig {
            runtime: Some("docker".into()),
            ..Default::default()
        };
        let runtime = resolve_runtime(&config).unwrap();
        assert_eq!(runtime.name(), "docker");
    }

    #[test]
    fn resolve_runtime_unknown_string_falls_back_to_docker_with_warning() {
        // Unknown strings fall back to Docker (warning printed to stderr).
        let config = crate::config::GlobalConfig {
            runtime: Some("podman".into()),
            ..Default::default()
        };
        let runtime = resolve_runtime(&config).unwrap();
        assert_eq!(runtime.name(), "docker", "unknown runtime should fall back to docker");
    }

    #[test]
    fn resolve_runtime_empty_string_falls_back_to_docker() {
        let config = crate::config::GlobalConfig {
            runtime: Some(String::new()),
            ..Default::default()
        };
        let runtime = resolve_runtime(&config).unwrap();
        assert_eq!(runtime.name(), "docker");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn resolve_runtime_apple_containers_on_macos_returns_apple_runtime() {
        let config = crate::config::GlobalConfig {
            runtime: Some("apple-containers".into()),
            ..Default::default()
        };
        let runtime = resolve_runtime(&config).unwrap();
        assert_eq!(runtime.name(), "apple-containers");
        assert_eq!(runtime.cli_binary(), "container");
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn resolve_runtime_apple_containers_on_non_macos_returns_err() {
        let config = crate::config::GlobalConfig {
            runtime: Some("apple-containers".into()),
            ..Default::default()
        };
        // apple-containers is macOS-only; must be rejected on other platforms.
        let result = resolve_runtime(&config);
        let err = result
            .err()
            .expect("apple-containers should be rejected on non-macOS, got Ok");
        let msg = err.to_string();
        assert!(
            msg.contains("macOS"),
            "error message should mention macOS: {}", msg
        );
    }

    // ─── parse_dockerfile_user ───────────────────────────────────────────────

    #[test]
    fn parse_dockerfile_user_returns_last_user_directive() {
        let tmp = TempDir::new().unwrap();
        let dockerfile = tmp.path().join("Dockerfile.dev");
        std::fs::write(&dockerfile, "FROM debian\nUSER root\nUSER agent\n").unwrap();
        assert_eq!(parse_dockerfile_user(&dockerfile).as_deref(), Some("agent"));
    }

    #[test]
    fn parse_dockerfile_user_case_insensitive() {
        let tmp = TempDir::new().unwrap();
        let dockerfile = tmp.path().join("Dockerfile.dev");
        std::fs::write(&dockerfile, "FROM debian\nuser agent\n").unwrap();
        assert_eq!(parse_dockerfile_user(&dockerfile).as_deref(), Some("agent"));
    }

    #[test]
    fn parse_dockerfile_user_no_user_directive_returns_none() {
        let tmp = TempDir::new().unwrap();
        let dockerfile = tmp.path().join("Dockerfile.dev");
        std::fs::write(&dockerfile, "FROM debian\nRUN echo hello\n").unwrap();
        assert!(parse_dockerfile_user(&dockerfile).is_none());
    }

    #[test]
    fn parse_dockerfile_user_nonexistent_file_returns_none() {
        let path = std::path::Path::new("/nonexistent/Dockerfile.dev");
        assert!(parse_dockerfile_user(path).is_none());
    }

    #[test]
    fn parse_dockerfile_user_root_user_returned() {
        let tmp = TempDir::new().unwrap();
        let dockerfile = tmp.path().join("Dockerfile.dev");
        std::fs::write(&dockerfile, "FROM debian\nUSER root\n").unwrap();
        assert_eq!(parse_dockerfile_user(&dockerfile).as_deref(), Some("root"));
    }

    // ─── container_home_for_user ─────────────────────────────────────────────

    #[test]
    fn container_home_for_root_is_slash_root() {
        assert_eq!(container_home_for_user("root"), "/root");
    }

    #[test]
    fn container_home_for_non_root_is_home_username() {
        assert_eq!(container_home_for_user("agent"), "/home/agent");
        assert_eq!(container_home_for_user("claude"), "/home/claude");
    }

    // ─── HostSettings::container_home default ────────────────────────────────

    #[test]
    fn host_settings_from_paths_default_container_home_is_root() {
        let settings = HostSettings::from_paths(
            PathBuf::from("/tmp/cfg.json"),
            PathBuf::from("/tmp/dot-claude"),
        );
        assert_eq!(settings.container_home, "/root");
    }

    #[test]
    fn host_settings_prepare_minimal_default_container_home_is_root() {
        if let Some(s) = HostSettings::prepare_minimal("claude") {
            assert_eq!(s.container_home, "/root");
        }
    }

    // ─── apply_dockerfile_user ────────────────────────────────────────────────

    #[test]
    fn apply_dockerfile_user_non_root_updates_container_home_and_returns_message() {
        let tmp = TempDir::new().unwrap();
        let dockerfile = tmp.path().join("Dockerfile.dev");
        std::fs::write(&dockerfile, "FROM debian\nUSER amux\n").unwrap();
        let mut settings = HostSettings::from_paths(
            PathBuf::from("/tmp/cfg.json"),
            PathBuf::from("/tmp/dot-claude"),
        );
        let msg = apply_dockerfile_user(&mut settings, &dockerfile);
        assert_eq!(settings.container_home, "/home/amux");
        assert!(msg.is_some());
        let msg = msg.unwrap();
        assert!(msg.contains("amux"), "message should mention the user");
        assert!(msg.contains("/home/amux"), "message should mention the home dir");
    }

    #[test]
    fn apply_dockerfile_user_root_returns_none_and_leaves_container_home() {
        let tmp = TempDir::new().unwrap();
        let dockerfile = tmp.path().join("Dockerfile.dev");
        std::fs::write(&dockerfile, "FROM debian\nUSER root\n").unwrap();
        let mut settings = HostSettings::from_paths(
            PathBuf::from("/tmp/cfg.json"),
            PathBuf::from("/tmp/dot-claude"),
        );
        let msg = apply_dockerfile_user(&mut settings, &dockerfile);
        assert!(msg.is_none());
        assert_eq!(settings.container_home, "/root");
    }

    #[test]
    fn apply_dockerfile_user_no_directive_returns_none() {
        let tmp = TempDir::new().unwrap();
        let dockerfile = tmp.path().join("Dockerfile.dev");
        std::fs::write(&dockerfile, "FROM debian\nRUN echo hello\n").unwrap();
        let mut settings = HostSettings::from_paths(
            PathBuf::from("/tmp/cfg.json"),
            PathBuf::from("/tmp/dot-claude"),
        );
        let msg = apply_dockerfile_user(&mut settings, &dockerfile);
        assert!(msg.is_none());
        assert_eq!(settings.container_home, "/root");
    }

    #[test]
    fn apply_dockerfile_user_uses_last_user_directive() {
        let tmp = TempDir::new().unwrap();
        let dockerfile = tmp.path().join("Dockerfile.dev");
        std::fs::write(&dockerfile, "FROM debian\nUSER root\nUSER agent\n").unwrap();
        let mut settings = HostSettings::from_paths(
            PathBuf::from("/tmp/cfg.json"),
            PathBuf::from("/tmp/dot-claude"),
        );
        let msg = apply_dockerfile_user(&mut settings, &dockerfile);
        assert_eq!(settings.container_home, "/home/agent");
        assert!(msg.is_some());
    }

    #[test]
    fn apply_dockerfile_user_missing_file_returns_none() {
        let mut settings = HostSettings::from_paths(
            PathBuf::from("/tmp/cfg.json"),
            PathBuf::from("/tmp/dot-claude"),
        );
        let msg = apply_dockerfile_user(&mut settings, Path::new("/nonexistent/Dockerfile.dev"));
        assert!(msg.is_none());
        assert_eq!(settings.container_home, "/root");
    }
}
