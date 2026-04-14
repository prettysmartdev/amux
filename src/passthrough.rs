use crate::commands::auth::{agent_keychain_credentials, AgentCredentials};
use crate::runtime::HostSettings;
use std::path::Path;

/// Handles agent-specific authentication and settings passthrough into Docker containers.
///
/// Three concerns are handled per-agent:
/// 1. **Keychain retrieval** — reads OAuth tokens from the system keychain.
/// 2. **Env var injection** — additional environment variables to pass to the container.
/// 3. **Settings folders** — agent config directories to copy and bind-mount read-only.
///
/// Cleanup of temporary directories is automatic (RAII via [`tempfile::TempDir`] inside
/// the returned [`HostSettings`]).
pub trait AgentPassthrough: Send + Sync {
    /// Returns agent credentials from the system keychain.
    ///
    /// Typically returns a single env var (e.g. `CLAUDE_CODE_OAUTH_TOKEN`).
    /// Default implementation: empty (most agents do not use the system keychain).
    fn keychain_credentials(&self) -> AgentCredentials {
        AgentCredentials::default()
    }

    /// Returns additional env vars to inject into the container beyond keychain credentials.
    ///
    /// Used for agents that embed static env vars rather than reading from the keychain.
    /// Default implementation: empty.
    fn extra_env_vars(&self) -> Vec<(String, String)> {
        vec![]
    }

    /// Prepares agent settings (config directories) for container injection into a temp dir.
    ///
    /// Returns `None` if the agent has no applicable settings on this host.
    /// The returned [`HostSettings`] holds a [`tempfile::TempDir`] that is automatically
    /// cleaned up when the value is dropped (i.e. when the container exits).
    fn prepare_host_settings(&self) -> Option<HostSettings>;

    /// Prepares agent settings into a caller-supplied stable directory.
    ///
    /// Used for persistent containers (e.g. worktrees, nanoclaw) that survive process
    /// restarts. The caller owns the directory and is responsible for cleanup.
    fn prepare_host_settings_to_dir(&self, dir: &Path) -> Option<HostSettings>;
}

// ─── Claude ─────────────────────────────────────────────────────────────────

/// Passthrough for the Claude Code agent.
///
/// - **Keychain**: reads `CLAUDE_CODE_OAUTH_TOKEN` from the macOS system keychain.
/// - **Env vars**: the keychain token is the only credential; no extra env vars.
/// - **Settings**: copies `~/.claude.json` (sanitized) and `~/.claude/` (filtered)
///   into a temp dir, mounting both at `<container_home>/.claude.json` and
///   `<container_home>/.claude`. Falls back to a minimal config (LSP suppressed only)
///   when `~/.claude.json` is absent.
pub struct ClaudePassthrough;

impl AgentPassthrough for ClaudePassthrough {
    fn keychain_credentials(&self) -> AgentCredentials {
        agent_keychain_credentials("claude")
    }

    fn prepare_host_settings(&self) -> Option<HostSettings> {
        HostSettings::prepare("claude")
            .or_else(|| HostSettings::prepare_minimal("claude"))
    }

    fn prepare_host_settings_to_dir(&self, dir: &Path) -> Option<HostSettings> {
        HostSettings::prepare_to_dir("claude", dir)
    }
}

// ─── Opencode ────────────────────────────────────────────────────────────────

/// Top-level entries in `~/.local/share/opencode/` to exclude from the container copy.
const OPENCODE_DIR_DENYLIST: &[&str] = &["logs"];

/// Passthrough for the Opencode agent.
///
/// - **Keychain**: none (opencode does not use the system keychain).
/// - **Env vars**: none (API keys should be passed via the `envPassthrough` config key).
/// - **Settings**: copies `~/.local/share/opencode/` into a temp dir and mounts it
///   (read-write) at `/root/.local/share/opencode` inside the container. The mount is
///   read-write because the source is a temp copy, not the live host directory.
///   Returns `None` if `~/.local/share/opencode/` does not exist on the host.
pub struct OpencodePassthrough;

impl AgentPassthrough for OpencodePassthrough {
    fn prepare_host_settings(&self) -> Option<HostSettings> {
        let home = dirs::home_dir()?;
        let src = home.join(".local/share/opencode");
        if !src.exists() {
            return None;
        }
        let temp_dir = tempfile::TempDir::new().ok()?;
        let dst = temp_dir.path().join("opencode-data");
        crate::runtime::copy_dir_filtered(&src, &dst, OPENCODE_DIR_DENYLIST).ok()?;
        Some(HostSettings::new_agent_dir(
            Some(temp_dir),
            "/root".to_string(),
            Some((dst, "/root/.local/share/opencode".to_string())),
        ))
    }

    fn prepare_host_settings_to_dir(&self, dir: &Path) -> Option<HostSettings> {
        let home = dirs::home_dir()?;
        let src = home.join(".local/share/opencode");
        if !src.exists() {
            return None;
        }
        std::fs::create_dir_all(dir).ok()?;
        let dst = dir.join("opencode-data");
        crate::runtime::copy_dir_filtered(&src, &dst, OPENCODE_DIR_DENYLIST).ok()?;
        Some(HostSettings::new_agent_dir(
            None,
            "/root".to_string(),
            Some((dst, "/root/.local/share/opencode".to_string())),
        ))
    }
}

// ─── Codex ───────────────────────────────────────────────────────────────────

/// Top-level entries in `~/.codex/` to exclude from the container copy.
const CODEX_DIR_DENYLIST: &[&str] = &["logs"];

/// Passthrough for the OpenAI Codex agent.
///
/// - **Keychain**: none (codex uses `OPENAI_API_KEY` via the `envPassthrough` config key).
/// - **Env vars**: none.
/// - **Settings**: copies `~/.codex/` into a temp dir and mounts it (read-write) at
///   `/root/.codex` inside the container. The mount is read-write because the source is
///   a temp copy, not the live host directory.
///   Returns `None` if `~/.codex/` does not exist on the host.
pub struct CodexPassthrough;

impl AgentPassthrough for CodexPassthrough {
    fn prepare_host_settings(&self) -> Option<HostSettings> {
        let home = dirs::home_dir()?;
        let src = home.join(".codex");
        if !src.exists() {
            return None;
        }
        let temp_dir = tempfile::TempDir::new().ok()?;
        let dst = temp_dir.path().join("codex-data");
        crate::runtime::copy_dir_filtered(&src, &dst, CODEX_DIR_DENYLIST).ok()?;
        Some(HostSettings::new_agent_dir(
            Some(temp_dir),
            "/root".to_string(),
            Some((dst, "/root/.codex".to_string())),
        ))
    }

    fn prepare_host_settings_to_dir(&self, dir: &Path) -> Option<HostSettings> {
        let home = dirs::home_dir()?;
        let src = home.join(".codex");
        if !src.exists() {
            return None;
        }
        std::fs::create_dir_all(dir).ok()?;
        let dst = dir.join("codex-data");
        crate::runtime::copy_dir_filtered(&src, &dst, CODEX_DIR_DENYLIST).ok()?;
        Some(HostSettings::new_agent_dir(
            None,
            "/root".to_string(),
            Some((dst, "/root/.codex".to_string())),
        ))
    }
}

// ─── Gemini ──────────────────────────────────────────────────────────────────

/// Top-level entries in `~/.gemini/` to exclude from the container copy.
const GEMINI_DIR_DENYLIST: &[&str] = &["logs"];

/// Passthrough for the Google Gemini CLI agent.
///
/// - **Keychain**: none (gemini does not use the system keychain).
/// - **Env vars**: none (API keys passed via the `envPassthrough` config key).
/// - **Settings**: copies `~/.gemini/` into a temp dir and mounts it (read-write) at
///   `/root/.gemini` inside the container. The mount is read-write because the source is
///   a temp copy, not the live host directory.
///   If `~/.gemini/` does not exist on the host, creates an empty temp dir and mounts
///   that instead, so the container starts with a clean gemini state (gemini will prompt
///   for auth on first use).
pub struct GeminiPassthrough;

impl AgentPassthrough for GeminiPassthrough {
    fn prepare_host_settings(&self) -> Option<HostSettings> {
        let home = dirs::home_dir()?;
        let src = home.join(".gemini");
        let temp_dir = tempfile::TempDir::new().ok()?;
        let dst = temp_dir.path().join("gemini-data");
        if src.exists() {
            crate::runtime::copy_dir_filtered(&src, &dst, GEMINI_DIR_DENYLIST).ok()?;
        } else {
            std::fs::create_dir_all(&dst).ok()?;
        }
        Some(HostSettings::new_agent_dir(
            Some(temp_dir),
            "/root".to_string(),
            Some((dst, "/root/.gemini".to_string())),
        ))
    }

    fn prepare_host_settings_to_dir(&self, dir: &Path) -> Option<HostSettings> {
        let home = dirs::home_dir()?;
        let src = home.join(".gemini");
        std::fs::create_dir_all(dir).ok()?;
        let dst = dir.join("gemini-data");
        if src.exists() {
            crate::runtime::copy_dir_filtered(&src, &dst, GEMINI_DIR_DENYLIST).ok()?;
        } else {
            std::fs::create_dir_all(&dst).ok()?;
        }
        Some(HostSettings::new_agent_dir(
            None,
            "/root".to_string(),
            Some((dst, "/root/.gemini".to_string())),
        ))
    }
}

// ─── Noop ─────────────────────────────────────────────────────────────────────

/// Passthrough for agents with no special auth or settings requirements.
///
/// Used for maki and any unrecognised agent. All three auth concerns return
/// empty / `None`. Authentication for these agents is handled via the `envPassthrough`
/// config key.
pub struct NoopPassthrough;

impl AgentPassthrough for NoopPassthrough {
    fn prepare_host_settings(&self) -> Option<HostSettings> {
        None
    }

    fn prepare_host_settings_to_dir(&self, _dir: &Path) -> Option<HostSettings> {
        None
    }
}

// ─── Factory ─────────────────────────────────────────────────────────────────

/// Returns the passthrough implementation for the given agent name.
///
/// - `"claude"` → [`ClaudePassthrough`]
/// - `"opencode"` → [`OpencodePassthrough`]
/// - `"codex"` → [`CodexPassthrough`]
/// - `"gemini"` → [`GeminiPassthrough`]
/// - Any other agent → [`NoopPassthrough`]
pub fn passthrough_for_agent(agent: &str) -> Box<dyn AgentPassthrough> {
    match agent {
        "claude" => Box::new(ClaudePassthrough),
        "opencode" => Box::new(OpencodePassthrough),
        "codex" => Box::new(CodexPassthrough),
        "gemini" => Box::new(GeminiPassthrough),
        _ => Box::new(NoopPassthrough),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    // ─── NoopPassthrough ──────────────────────────────────────────────────────

    #[test]
    fn noop_passthrough_keychain_credentials_is_empty() {
        assert!(NoopPassthrough.keychain_credentials().env_vars.is_empty());
    }

    #[test]
    fn noop_passthrough_extra_env_vars_is_empty() {
        assert!(NoopPassthrough.extra_env_vars().is_empty());
    }

    #[test]
    fn noop_passthrough_prepare_host_settings_returns_none() {
        assert!(NoopPassthrough.prepare_host_settings().is_none());
    }

    #[test]
    fn noop_passthrough_prepare_host_settings_to_dir_returns_none() {
        let tmp = TempDir::new().unwrap();
        assert!(NoopPassthrough.prepare_host_settings_to_dir(tmp.path()).is_none());
    }

    #[test]
    fn passthrough_for_agent_returns_codex_impl() {
        let p = passthrough_for_agent("codex");
        // Codex passthrough has no keychain credentials.
        assert!(p.keychain_credentials().env_vars.is_empty());
        assert!(p.extra_env_vars().is_empty());
        // Returns None on machines without codex (CI), no panic.
        let _ = p.prepare_host_settings();
    }

    #[test]
    fn passthrough_for_agent_noop_for_maki() {
        let p = passthrough_for_agent("maki");
        assert!(p.prepare_host_settings().is_none());
        assert!(p.keychain_credentials().env_vars.is_empty());
    }

    #[test]
    fn passthrough_for_agent_noop_for_unknown() {
        let p = passthrough_for_agent("unknown-agent-xyz");
        assert!(p.prepare_host_settings().is_none());
        assert!(p.keychain_credentials().env_vars.is_empty());
    }

    // ─── ClaudePassthrough ────────────────────────────────────────────────────

    #[test]
    fn claude_passthrough_prepare_host_settings_always_returns_some() {
        // Falls back to prepare_minimal when ~/.claude.json is absent,
        // so it always returns Some on any machine.
        let settings = ClaudePassthrough.prepare_host_settings();
        assert!(settings.is_some(), "ClaudePassthrough must return Some (via prepare_minimal fallback)");
    }

    #[test]
    fn claude_passthrough_host_settings_has_mount_claude_files_true() {
        if let Some(s) = ClaudePassthrough.prepare_host_settings() {
            assert!(s.mount_claude_files, "Claude settings must have mount_claude_files = true");
        }
    }

    #[test]
    fn claude_passthrough_host_settings_no_agent_config_dir() {
        if let Some(s) = ClaudePassthrough.prepare_host_settings() {
            assert!(s.agent_config_dir.is_none(), "Claude settings must not set agent_config_dir");
        }
    }

    #[test]
    fn claude_passthrough_prepare_to_dir_does_not_panic() {
        // On CI (no ~/.claude.json), prepare_to_dir returns None. On dev, returns Some.
        let tmp = TempDir::new().unwrap();
        let _ = ClaudePassthrough.prepare_host_settings_to_dir(tmp.path());
    }

    #[test]
    fn passthrough_for_agent_returns_claude_impl() {
        let p = passthrough_for_agent("claude");
        // Claude passthrough always returns Some host settings.
        let settings = p.prepare_host_settings();
        assert!(settings.is_some(), "claude passthrough must return Some settings");
        assert!(settings.unwrap().mount_claude_files);
    }

    // ─── OpencodePassthrough ──────────────────────────────────────────────────

    #[test]
    fn opencode_passthrough_no_keychain_credentials() {
        assert!(OpencodePassthrough.keychain_credentials().env_vars.is_empty());
    }

    #[test]
    fn opencode_passthrough_no_extra_env_vars() {
        assert!(OpencodePassthrough.extra_env_vars().is_empty());
    }

    #[test]
    fn opencode_passthrough_returns_none_or_some_without_panic() {
        // If ~/.local/share/opencode does not exist, returns None without panicking.
        let _ = OpencodePassthrough.prepare_host_settings();
    }

    #[test]
    fn opencode_passthrough_settings_contract_when_some() {
        // If the host has opencode installed, verify the returned settings have the
        // correct shape. On CI/dev without opencode, this is a no-op.
        if let Some(settings) = OpencodePassthrough.prepare_host_settings() {
            assert!(
                !settings.mount_claude_files,
                "Opencode settings must have mount_claude_files = false"
            );
            let (_, container_path) = settings
                .agent_config_dir
                .expect("Opencode settings must set agent_config_dir");
            assert_eq!(container_path, "/root/.local/share/opencode");
        }
    }

    #[test]
    fn opencode_passthrough_prepare_to_dir_settings_contract() {
        // Same contract as prepare_host_settings but with a supplied dir.
        let tmp = TempDir::new().unwrap();
        if let Some(settings) = OpencodePassthrough.prepare_host_settings_to_dir(tmp.path()) {
            assert!(!settings.mount_claude_files);
            let (_, container_path) = settings
                .agent_config_dir
                .expect("Opencode settings must set agent_config_dir");
            assert_eq!(container_path, "/root/.local/share/opencode");
        }
    }

    #[test]
    fn opencode_passthrough_copy_excludes_logs() {
        use std::io::Write;

        // Build a fake opencode source directory.
        let fake_src = TempDir::new().unwrap();
        let auth_file = fake_src.path().join("auth.json");
        std::fs::File::create(&auth_file).unwrap().write_all(b"{}").unwrap();
        std::fs::create_dir(fake_src.path().join("logs")).unwrap();

        // Copy using the same denylist as OpencodePassthrough.
        let dst_tmp = TempDir::new().unwrap();
        let dst = dst_tmp.path().join("opencode-data");
        crate::runtime::copy_dir_filtered(fake_src.path(), &dst, OPENCODE_DIR_DENYLIST).unwrap();

        assert!(dst.join("auth.json").exists(), "auth.json must be copied");
        assert!(!dst.join("logs").exists(), "logs must be excluded by denylist");
    }

    #[test]
    fn passthrough_for_agent_returns_opencode_impl() {
        let p = passthrough_for_agent("opencode");
        // Opencode passthrough has no keychain credentials.
        assert!(p.keychain_credentials().env_vars.is_empty());
        // Returns None on machines without opencode (CI), no panic.
        let _ = p.prepare_host_settings();
    }

    // ─── CodexPassthrough ─────────────────────────────────────────────────────

    #[test]
    fn codex_passthrough_no_keychain_credentials() {
        assert!(CodexPassthrough.keychain_credentials().env_vars.is_empty());
    }

    #[test]
    fn codex_passthrough_no_extra_env_vars() {
        assert!(CodexPassthrough.extra_env_vars().is_empty());
    }

    #[test]
    fn codex_passthrough_returns_none_or_some_without_panic() {
        // If ~/.codex does not exist, returns None without panicking.
        let _ = CodexPassthrough.prepare_host_settings();
    }

    #[test]
    fn codex_passthrough_settings_contract_when_some() {
        // If the host has codex installed, verify the returned settings have the
        // correct shape. On CI/dev without codex, this is a no-op.
        if let Some(settings) = CodexPassthrough.prepare_host_settings() {
            assert!(
                !settings.mount_claude_files,
                "Codex settings must have mount_claude_files = false"
            );
            let (_, container_path) = settings
                .agent_config_dir
                .expect("Codex settings must set agent_config_dir");
            assert_eq!(container_path, "/root/.codex");
        }
    }

    #[test]
    fn codex_passthrough_prepare_to_dir_settings_contract() {
        // Same contract as prepare_host_settings but with a supplied dir.
        let tmp = TempDir::new().unwrap();
        if let Some(settings) = CodexPassthrough.prepare_host_settings_to_dir(tmp.path()) {
            assert!(!settings.mount_claude_files);
            let (_, container_path) = settings
                .agent_config_dir
                .expect("Codex settings must set agent_config_dir");
            assert_eq!(container_path, "/root/.codex");
        }
    }

    #[test]
    fn codex_passthrough_copy_excludes_logs() {
        use std::io::Write;

        // Build a fake codex source directory.
        let fake_src = TempDir::new().unwrap();
        let config_file = fake_src.path().join("config.toml");
        std::fs::File::create(&config_file).unwrap().write_all(b"[model]\n").unwrap();
        std::fs::create_dir(fake_src.path().join("logs")).unwrap();

        // Copy using the same denylist as CodexPassthrough.
        let dst_tmp = TempDir::new().unwrap();
        let dst = dst_tmp.path().join("codex-data");
        crate::runtime::copy_dir_filtered(fake_src.path(), &dst, CODEX_DIR_DENYLIST).unwrap();

        assert!(dst.join("config.toml").exists(), "config.toml must be copied");
        assert!(!dst.join("logs").exists(), "logs must be excluded by denylist");
    }

    // ─── GeminiPassthrough ────────────────────────────────────────────────────

    #[test]
    fn gemini_passthrough_no_keychain_credentials() {
        assert!(GeminiPassthrough.keychain_credentials().env_vars.is_empty());
    }

    #[test]
    fn gemini_passthrough_no_extra_env_vars() {
        assert!(GeminiPassthrough.extra_env_vars().is_empty());
    }

    #[test]
    fn gemini_passthrough_always_returns_some() {
        // GeminiPassthrough must always return Some — even when ~/.gemini/ does not exist
        // it falls back to an empty temp dir so the container gets a clean gemini state.
        let settings = GeminiPassthrough.prepare_host_settings();
        assert!(settings.is_some(), "GeminiPassthrough must always return Some");
    }

    #[test]
    fn gemini_passthrough_settings_contract_when_some() {
        let settings = GeminiPassthrough
            .prepare_host_settings()
            .expect("GeminiPassthrough must always return Some");
        assert!(
            !settings.mount_claude_files,
            "Gemini settings must have mount_claude_files = false"
        );
        let (_, container_path) = settings
            .agent_config_dir
            .expect("Gemini settings must set agent_config_dir");
        assert_eq!(container_path, "/root/.gemini");
    }

    #[test]
    fn gemini_passthrough_prepare_to_dir_settings_contract() {
        // Same contract as prepare_host_settings but with a caller-supplied stable dir.
        let tmp = TempDir::new().unwrap();
        let settings = GeminiPassthrough
            .prepare_host_settings_to_dir(tmp.path())
            .expect("GeminiPassthrough.prepare_host_settings_to_dir must always return Some");
        assert!(!settings.mount_claude_files);
        let (_, container_path) = settings
            .agent_config_dir
            .expect("Gemini settings must set agent_config_dir");
        assert_eq!(container_path, "/root/.gemini");
    }

    #[test]
    fn gemini_passthrough_copy_excludes_logs() {
        use std::io::Write;

        // Build a fake ~/.gemini source directory.
        let fake_src = TempDir::new().unwrap();
        let settings_file = fake_src.path().join("settings.json");
        std::fs::File::create(&settings_file).unwrap().write_all(b"{}").unwrap();
        std::fs::create_dir(fake_src.path().join("logs")).unwrap();

        // Copy using the same denylist as GeminiPassthrough.
        let dst_tmp = TempDir::new().unwrap();
        let dst = dst_tmp.path().join("gemini-data");
        crate::runtime::copy_dir_filtered(fake_src.path(), &dst, GEMINI_DIR_DENYLIST).unwrap();

        assert!(dst.join("settings.json").exists(), "settings.json must be copied");
        assert!(!dst.join("logs").exists(), "logs must be excluded by denylist");
    }

    #[test]
    fn passthrough_for_agent_returns_gemini_impl() {
        let p = passthrough_for_agent("gemini");
        // Gemini passthrough has no keychain credentials.
        assert!(p.keychain_credentials().env_vars.is_empty());
        assert!(p.extra_env_vars().is_empty());
        // Always returns Some (even without ~/.gemini/).
        let settings = p.prepare_host_settings();
        assert!(settings.is_some(), "gemini passthrough must always return Some");
        assert!(!settings.unwrap().mount_claude_files);
    }

    #[test]
    fn passthrough_for_agent_noop_for_maki_unchanged() {
        // maki continues to return NoopPassthrough after gemini was added.
        let p = passthrough_for_agent("maki");
        assert!(p.prepare_host_settings().is_none());
        assert!(p.keychain_credentials().env_vars.is_empty());
        assert!(p.extra_env_vars().is_empty());
    }

    // ─── envPassthrough: GEMINI_API_KEY injection ─────────────────────────────
    //
    // The generic passthrough injection loop (tested in chat.rs) handles any env
    // var listed in `envPassthrough`. The test below confirms that GEMINI_API_KEY
    // specifically reaches the injected vars, validating the gemini API-key path.

    #[test]
    fn passthrough_injection_gemini_api_key_reaches_env_vars() {
        use crate::config::{save_repo_config, RepoConfig};

        let tmp = TempDir::new().unwrap();
        let config = RepoConfig {
            agent: Some("gemini".to_string()),
            auto_agent_auth_accepted: None,
            terminal_scrollback_lines: None,
            yolo_disallowed_tools: None,
            env_passthrough: Some(vec!["AMUX_TEST_GEMINI_API_KEY_PT_999".to_string()]),
            work_items: None,
        };
        save_repo_config(tmp.path(), &config).unwrap();

        // SAFETY: test-only env mutation; unique var name avoids races.
        unsafe { std::env::set_var("AMUX_TEST_GEMINI_API_KEY_PT_999", "test-gemini-key") };

        let mut env_vars: Vec<(String, String)> = vec![];
        let passthrough_names = crate::config::effective_env_passthrough(tmp.path());
        for name in &passthrough_names {
            if let Ok(val) = std::env::var(name) {
                env_vars.push((name.clone(), val));
            }
        }

        // SAFETY: test-only env mutation.
        unsafe { std::env::remove_var("AMUX_TEST_GEMINI_API_KEY_PT_999") };

        assert!(
            env_vars.contains(&(
                "AMUX_TEST_GEMINI_API_KEY_PT_999".to_string(),
                "test-gemini-key".to_string()
            )),
            "GEMINI_API_KEY (simulated) must be injected via envPassthrough"
        );
    }
}
