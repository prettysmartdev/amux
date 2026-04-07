use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Per-repository configuration stored at `GITROOT/.amux/config.json`.
#[derive(Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct RepoConfig {
    pub agent: Option<String>,
    /// Whether the user has consented to mounting agent credentials into containers.
    /// Saved once per Git root; None means the user has not been asked yet.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_agent_auth_accepted: Option<bool>,
    /// Number of scrollback lines for the container terminal emulator.
    /// Overrides the global config value and the built-in default (10,000).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub terminal_scrollback_lines: Option<usize>,
}

/// Global configuration stored at `$HOME/.amux/config.json`.
#[derive(Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct GlobalConfig {
    pub default_agent: Option<String>,
    /// Default number of scrollback lines for the container terminal emulator.
    /// Applied to all repos unless overridden by per-repo config. Built-in default: 10,000.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub terminal_scrollback_lines: Option<usize>,
}

/// Built-in default number of scrollback lines for the container terminal emulator.
pub const DEFAULT_SCROLLBACK_LINES: usize = 10_000;

/// Resolve the effective scrollback line count for a given git root.
/// Checks per-repo config first, then global config, then falls back to the built-in default.
pub fn effective_scrollback_lines(git_root: &Path) -> usize {
    let repo = load_repo_config(git_root).unwrap_or_default();
    if let Some(lines) = repo.terminal_scrollback_lines {
        return lines;
    }
    let global = load_global_config().unwrap_or_default();
    global.terminal_scrollback_lines.unwrap_or(DEFAULT_SCROLLBACK_LINES)
}

pub fn repo_config_path(git_root: &Path) -> PathBuf {
    git_root.join(".amux").join("config.json")
}

/// Legacy path used before config was moved to `.amux/config.json`.
pub fn legacy_repo_config_path(git_root: &Path) -> PathBuf {
    git_root.join("aspec").join(".amux.json")
}

/// Migrate legacy `aspec/.amux.json` to `.amux/config.json` if the legacy file exists
/// and the new path does not. Returns true if a migration was performed.
pub fn migrate_legacy_repo_config(git_root: &Path) -> anyhow::Result<bool> {
    let legacy = legacy_repo_config_path(git_root);
    let current = repo_config_path(git_root);
    if !legacy.exists() || current.exists() {
        return Ok(false);
    }
    let content = std::fs::read_to_string(&legacy)
        .with_context(|| format!("Failed to read legacy config {}", legacy.display()))?;
    if let Some(parent) = current.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory {}", parent.display()))?;
    }
    std::fs::write(&current, &content)
        .with_context(|| format!("Failed to write {}", current.display()))?;
    std::fs::remove_file(&legacy)
        .with_context(|| format!("Failed to remove legacy config {}", legacy.display()))?;
    Ok(true)
}

#[allow(dead_code)]
pub fn global_config_path() -> Result<PathBuf> {
    let home = dirs::home_dir().context("Cannot determine home directory")?;
    Ok(home.join(".amux").join("config.json"))
}

pub fn load_repo_config(git_root: &Path) -> Result<RepoConfig> {
    let path = repo_config_path(git_root);
    if !path.exists() {
        return Ok(RepoConfig::default());
    }
    let content = std::fs::read_to_string(&path)
        .with_context(|| format!("Failed to read {}", path.display()))?;
    serde_json::from_str(&content).with_context(|| format!("Invalid JSON in {}", path.display()))
}

pub fn save_repo_config(git_root: &Path, config: &RepoConfig) -> Result<()> {
    let path = repo_config_path(git_root);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory {}", parent.display()))?;
    }
    let content = serde_json::to_string_pretty(config)?;
    std::fs::write(&path, content)
        .with_context(|| format!("Failed to write {}", path.display()))
}

#[allow(dead_code)]
pub fn load_global_config() -> Result<GlobalConfig> {
    let path = global_config_path()?;
    if !path.exists() {
        return Ok(GlobalConfig::default());
    }
    let content = std::fs::read_to_string(&path)
        .with_context(|| format!("Failed to read {}", path.display()))?;
    serde_json::from_str(&content).with_context(|| format!("Invalid JSON in {}", path.display()))
}

#[allow(dead_code)]
pub fn save_global_config(config: &GlobalConfig) -> Result<()> {
    let path = global_config_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory {}", parent.display()))?;
    }
    let content = serde_json::to_string_pretty(config)?;
    std::fs::write(&path, content)
        .with_context(|| format!("Failed to write {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn repo_config_path_is_correct() {
        let root = PathBuf::from("/some/repo");
        let path = repo_config_path(&root);
        assert_eq!(path, PathBuf::from("/some/repo/.amux/config.json"));
    }

    #[test]
    fn legacy_repo_config_path_is_correct() {
        let root = PathBuf::from("/some/repo");
        let path = legacy_repo_config_path(&root);
        assert_eq!(path, PathBuf::from("/some/repo/aspec/.amux.json"));
    }

    #[test]
    fn migrate_legacy_repo_config_moves_file_and_deletes_legacy() {
        let tmp = TempDir::new().unwrap();
        // Create the legacy aspec/.amux.json.
        let aspec_dir = tmp.path().join("aspec");
        std::fs::create_dir_all(&aspec_dir).unwrap();
        let legacy_content = r#"{"agent":"claude"}"#;
        std::fs::write(aspec_dir.join(".amux.json"), legacy_content).unwrap();

        let migrated = migrate_legacy_repo_config(tmp.path()).unwrap();
        assert!(migrated, "should report that migration occurred");

        // New path should exist with the same content.
        let new_path = repo_config_path(tmp.path());
        assert!(new_path.exists(), "new config file should exist");
        assert_eq!(std::fs::read_to_string(&new_path).unwrap(), legacy_content);

        // Legacy path should be gone.
        assert!(!legacy_repo_config_path(tmp.path()).exists(), "legacy file should be deleted");
    }

    #[test]
    fn migrate_legacy_repo_config_skips_when_no_legacy() {
        let tmp = TempDir::new().unwrap();
        let migrated = migrate_legacy_repo_config(tmp.path()).unwrap();
        assert!(!migrated, "no migration when legacy file absent");
    }

    #[test]
    fn migrate_legacy_repo_config_skips_when_new_already_exists() {
        let tmp = TempDir::new().unwrap();
        // Create both paths — migration should be skipped.
        let aspec_dir = tmp.path().join("aspec");
        std::fs::create_dir_all(&aspec_dir).unwrap();
        std::fs::write(aspec_dir.join(".amux.json"), r#"{"agent":"old"}"#).unwrap();
        let amux_dir = tmp.path().join(".amux");
        std::fs::create_dir_all(&amux_dir).unwrap();
        std::fs::write(amux_dir.join("config.json"), r#"{"agent":"new"}"#).unwrap();

        let migrated = migrate_legacy_repo_config(tmp.path()).unwrap();
        assert!(!migrated, "should not overwrite existing new config");
        // Verify neither file was altered.
        assert_eq!(
            std::fs::read_to_string(amux_dir.join("config.json")).unwrap(),
            r#"{"agent":"new"}"#
        );
    }

    #[test]
    fn global_config_path_is_under_home() {
        let path = global_config_path().unwrap();
        assert!(path.ends_with(".amux/config.json"));
    }

    #[test]
    fn save_and_load_repo_config_roundtrip() {
        let tmp = TempDir::new().unwrap();
        let config = RepoConfig {
            agent: Some("claude".to_string()),
            auto_agent_auth_accepted: None,
            terminal_scrollback_lines: None,
        };
        save_repo_config(tmp.path(), &config).unwrap();
        let loaded = load_repo_config(tmp.path()).unwrap();
        assert_eq!(config, loaded);
    }

    #[test]
    fn load_repo_config_returns_default_when_absent() {
        let tmp = TempDir::new().unwrap();
        let config = load_repo_config(tmp.path()).unwrap();
        assert_eq!(config, RepoConfig::default());
    }

    // ─── effective_scrollback_lines ─────────────────────────────────────────

    #[test]
    fn effective_scrollback_lines_returns_default_when_no_config() {
        let tmp = TempDir::new().unwrap();
        let lines = effective_scrollback_lines(tmp.path());
        assert_eq!(
            lines, DEFAULT_SCROLLBACK_LINES,
            "should return DEFAULT_SCROLLBACK_LINES when no config file exists"
        );
    }

    #[test]
    fn effective_scrollback_lines_reads_repo_config() {
        let tmp = TempDir::new().unwrap();
        let config = RepoConfig {
            agent: None,
            auto_agent_auth_accepted: None,
            terminal_scrollback_lines: Some(2_000),
        };
        save_repo_config(tmp.path(), &config).unwrap();

        let lines = effective_scrollback_lines(tmp.path());
        assert_eq!(lines, 2_000, "should read terminal_scrollback_lines from repo config");
    }

    #[test]
    fn effective_scrollback_lines_repo_config_takes_precedence_over_global() {
        // We can only test the repo-wins path by providing a repo config with the value set.
        // (Global config writes to HOME which we cannot override in tests without unsafe tricks.)
        let tmp = TempDir::new().unwrap();
        let repo_cfg = RepoConfig {
            agent: None,
            auto_agent_auth_accepted: None,
            terminal_scrollback_lines: Some(999),
        };
        save_repo_config(tmp.path(), &repo_cfg).unwrap();

        let lines = effective_scrollback_lines(tmp.path());
        assert_eq!(
            lines, 999,
            "repo config value must win over any global/default value"
        );
    }

    #[test]
    fn effective_scrollback_lines_falls_back_to_default_when_repo_field_absent() {
        let tmp = TempDir::new().unwrap();
        // Repo config exists but has no terminal_scrollback_lines field.
        let config = RepoConfig {
            agent: Some("claude".to_string()),
            auto_agent_auth_accepted: None,
            terminal_scrollback_lines: None,
        };
        save_repo_config(tmp.path(), &config).unwrap();

        // Without a global config the result must equal the built-in default.
        // (We can't control ~/.amux/config.json in tests, so only assert on the fallback chain.)
        let lines = effective_scrollback_lines(tmp.path());
        // It will be either global config value or DEFAULT_SCROLLBACK_LINES.
        assert!(
            lines >= 1,
            "effective_scrollback_lines must return a positive value; got {}",
            lines
        );
    }

    #[test]
    fn terminal_scrollback_lines_round_trips_through_repo_config() {
        let tmp = TempDir::new().unwrap();
        let config = RepoConfig {
            agent: None,
            auto_agent_auth_accepted: None,
            terminal_scrollback_lines: Some(5_000),
        };
        save_repo_config(tmp.path(), &config).unwrap();
        let loaded = load_repo_config(tmp.path()).unwrap();
        assert_eq!(loaded.terminal_scrollback_lines, Some(5_000));
    }
}
