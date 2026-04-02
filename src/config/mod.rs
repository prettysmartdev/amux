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
}

/// Global configuration stored at `$HOME/.amux/config.json`.
#[derive(Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct GlobalConfig {
    pub default_agent: Option<String>,
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
}
