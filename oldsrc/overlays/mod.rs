pub mod directory;
pub mod parser;

pub use directory::{DirectoryOverlay, MountPermission, Overlay};
pub use parser::{parse_overlay_list, TypedOverlay};

use crate::config::{load_global_config, load_repo_config, DirectoryOverlayConfig};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Expand a leading `~` in a path string to the user's home directory.
///
/// Called internally by `make_host_path_absolute`. Exported for use in tests.
pub fn expand_tilde(path: &str) -> PathBuf {
    if path == "~" {
        return dirs::home_dir().unwrap_or_else(|| PathBuf::from("~"));
    }
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }
    PathBuf::from(path)
}

/// Expand `~` and resolve relative paths to an absolute path.
///
/// - Paths starting with `~` are expanded to the user's home directory.
/// - Paths that are still relative after tilde expansion are resolved against
///   the process's current working directory (i.e. where the CLI was launched).
/// - Absolute paths are returned unchanged.
pub fn make_host_path_absolute(path: &str) -> PathBuf {
    let expanded = expand_tilde(path);
    if expanded.is_absolute() {
        expanded
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(expanded)
    }
}

/// Convert a `DirectoryOverlayConfig` (from JSON config) into a `DirectoryOverlay`.
///
/// Returns an error if the `permission` field contains an unrecognised value.
/// An absent or empty `permission` field defaults to `ReadOnly`.
fn config_to_overlay(cfg: &DirectoryOverlayConfig) -> anyhow::Result<DirectoryOverlay> {
    let permission = match cfg.permission.as_deref() {
        None | Some("") => MountPermission::default(),
        Some(p) => MountPermission::from_str_opt(p).ok_or_else(|| {
            anyhow::anyhow!(
                "invalid overlay permission {:?} for host {:?} → container {:?}; \
                 expected \"ro\" or \"rw\"",
                p,
                cfg.host,
                cfg.container
            )
        })?,
    };
    Ok(DirectoryOverlay {
        host_path: make_host_path_absolute(&cfg.host),
        container_path: PathBuf::from(&cfg.container),
        permission,
    })
}

/// Extract `DirectoryOverlay` entries from a list of `TypedOverlay`.
fn extract_directory_overlays(typed: &[TypedOverlay]) -> Vec<DirectoryOverlay> {
    typed
        .iter()
        .filter_map(|t| match t {
            TypedOverlay::Directory(d) => Some(d.clone()),
        })
        .collect()
}

/// Compute the effective overlay list by merging all sources.
///
/// Resolution order (additive, not replace):
/// 1. `global_config.overlays.directories` → priority 0 (lowest)
/// 2. `repo_config.overlays.directories` → priority 1
/// 3. `env_overlays` (parsed from `AMUX_OVERLAYS`) → priority 2
/// 4. `flag_overlays` (parsed from `--overlay` flags) → priority 3 (highest)
///
/// After collecting all entries, deduplicates by `conflict_key`:
/// - Walk entries in reverse priority order (highest first).
/// - If an entry's key has not been seen, keep it as-is.
/// - If seen, merge with the kept entry (higher priority wins on container_path,
///   most restrictive wins on permission).
///
/// Returns an error if any config overlay entry contains a malformed permission.
pub fn effective_overlays(
    git_root: &Path,
    env_overlays: &[DirectoryOverlay],
    flag_overlays: &[DirectoryOverlay],
) -> anyhow::Result<Vec<DirectoryOverlay>> {
    // Collect from all four sources in priority order (lowest first).
    let mut all_entries: Vec<(usize, DirectoryOverlay)> = Vec::new();

    // Priority 0: global config
    if let Ok(global) = load_global_config() {
        if let Some(overlays_cfg) = global.overlays {
            if let Some(dirs) = overlays_cfg.directories {
                for cfg in &dirs {
                    let overlay = config_to_overlay(cfg)
                        .map_err(|e| anyhow::anyhow!("{e} (in global config ~/.amux/config.json)"))?;
                    all_entries.push((0, overlay));
                }
            }
        }
    }

    // Priority 1: repo config
    if let Ok(repo) = load_repo_config(git_root) {
        if let Some(overlays_cfg) = repo.overlays {
            if let Some(dirs) = overlays_cfg.directories {
                for cfg in &dirs {
                    let overlay = config_to_overlay(cfg)
                        .map_err(|e| anyhow::anyhow!("{e} (in repo config .amux/config.json)"))?;
                    all_entries.push((1, overlay));
                }
            }
        }
    }

    // Priority 2: env overlays
    for overlay in env_overlays {
        all_entries.push((2, overlay.clone()));
    }

    // Priority 3: flag overlays (highest)
    for overlay in flag_overlays {
        all_entries.push((3, overlay.clone()));
    }

    // Deduplicate by conflict_key, highest priority first.
    // Sort by descending priority so we process highest first.
    all_entries.sort_by(|a, b| b.0.cmp(&a.0));

    let mut seen: HashMap<String, DirectoryOverlay> = HashMap::new();
    for (_priority, overlay) in &all_entries {
        let key = overlay.conflict_key();
        if let Some(existing) = seen.get(&key) {
            // `existing` was inserted first and is higher priority.
            // Merge: existing (high) merges with overlay (low).
            let merged = existing.merge_with_lower(overlay);
            seen.insert(key, merged);
        } else {
            seen.insert(key, overlay.clone());
        }
    }

    // Return in a stable order (sorted by conflict_key for determinism).
    let mut result: Vec<DirectoryOverlay> = seen.into_values().collect();
    result.sort_by(|a, b| a.conflict_key().cmp(&b.conflict_key()));

    // Warn about container_path collisions: different host paths mapping to the
    // same container path cause Docker to silently shadow one mount.
    // Track all conflicting host paths per container path for clear diagnostics.
    {
        let mut container_path_to_hosts: HashMap<String, Vec<String>> = HashMap::new();
        for overlay in &result {
            let cpath = overlay.container_path.to_string_lossy().to_string();
            let hpath = overlay.conflict_key();
            let hosts = container_path_to_hosts.entry(cpath.clone()).or_default();
            if !hosts.is_empty() {
                let all: Vec<String> = hosts
                    .iter()
                    .chain(std::iter::once(&hpath))
                    .map(|s| format!("'{s}'"))
                    .collect();
                tracing::warn!(
                    "overlay container path '{cpath}' is mapped from multiple host paths ({}); \
                     Docker will shadow one mount with the other",
                    all.join(", "),
                );
            }
            hosts.push(hpath);
        }
    }

    Ok(result)
}

/// Parse raw `--overlay` flag values into `DirectoryOverlay` entries.
///
/// Concatenates all repeated flag values with `,`, then parses as a single
/// comma-joined string. Returns an error for any malformed overlay expression
/// (per spec: malformed values are fatal, not silently skipped).
pub fn parse_flag_overlays(raw_flags: &[String]) -> anyhow::Result<Vec<DirectoryOverlay>> {
    if raw_flags.is_empty() {
        return Ok(vec![]);
    }
    let joined = raw_flags.join(",");
    let typed = parse_overlay_list(&joined)?;
    Ok(extract_directory_overlays(&typed))
}

/// Parse the `AMUX_OVERLAYS` environment variable into `DirectoryOverlay` entries.
///
/// Returns an error if the value is present but cannot be parsed.
/// An empty or absent `AMUX_OVERLAYS` returns an empty vec without error.
pub fn parse_env_overlays() -> anyhow::Result<Vec<DirectoryOverlay>> {
    let val = std::env::var("AMUX_OVERLAYS").unwrap_or_default();
    if val.is_empty() {
        return Ok(vec![]);
    }
    let typed = parse_overlay_list(&val)
        .map_err(|e| anyhow::anyhow!("invalid AMUX_OVERLAYS environment variable: {e}"))?;
    Ok(extract_directory_overlays(&typed))
}

/// Resolve overlays from all sources and validate host paths.
///
/// This is the single callsite function that commands should use.
/// It parses flags, parses the env var, calls `effective_overlays`,
/// and drops entries whose `host_path` does not exist on the host.
///
/// Returns an error if any overlay value is malformed — whether from
/// `--overlay` flags, `AMUX_OVERLAYS`, or either config file.
/// Missing host paths are non-fatal: a `warn!` is emitted and the entry is dropped.
pub fn resolve_overlays(
    git_root: &Path,
    raw_overlay_flags: &[String],
) -> anyhow::Result<Vec<DirectoryOverlay>> {
    let flag_overlays = parse_flag_overlays(raw_overlay_flags)?;
    let env_overlays = parse_env_overlays()?;
    let resolved = effective_overlays(git_root, &env_overlays, &flag_overlays)?;

    // Validate that each host_path exists; warn and drop if not.
    Ok(resolved
        .into_iter()
        .filter(|overlay| {
            if overlay.host_path.exists() {
                true
            } else {
                tracing::warn!(
                    "overlay host path '{}' does not exist; skipping",
                    overlay.host_path.display()
                );
                false
            }
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::TempDir;

    /// Serialise tests that mutate `AMUX_CONFIG_HOME` (process-global env var).
    /// Every effective_overlays test must hold this lock to prevent races.
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    // ─── helpers ─────────────────────────────────────────────────────────────

    fn ro(host: &str, container: &str) -> DirectoryOverlay {
        DirectoryOverlay {
            host_path: PathBuf::from(host),
            container_path: PathBuf::from(container),
            permission: MountPermission::ReadOnly,
        }
    }

    fn rw(host: &str, container: &str) -> DirectoryOverlay {
        DirectoryOverlay {
            host_path: PathBuf::from(host),
            container_path: PathBuf::from(container),
            permission: MountPermission::ReadWrite,
        }
    }

    /// Write a global config JSON with the given overlay entries to `home_dir/config.json`.
    fn write_global_overlays(home_dir: &Path, entries: &[(&str, &str, Option<&str>)]) {
        let dirs: Vec<serde_json::Value> = entries
            .iter()
            .map(|(h, c, p)| {
                let mut m = serde_json::json!({"host": h, "container": c});
                if let Some(perm) = p {
                    m["permission"] = serde_json::Value::String(perm.to_string());
                }
                m
            })
            .collect();
        let json = serde_json::json!({"overlays": {"directories": dirs}});
        std::fs::write(home_dir.join("config.json"), json.to_string()).unwrap();
    }

    /// Write a repo config JSON with the given overlay entries to `git_root/.amux/config.json`.
    fn write_repo_overlays(git_root: &Path, entries: &[(&str, &str, Option<&str>)]) {
        let amux_dir = git_root.join(".amux");
        std::fs::create_dir_all(&amux_dir).unwrap();
        let dirs: Vec<serde_json::Value> = entries
            .iter()
            .map(|(h, c, p)| {
                let mut m = serde_json::json!({"host": h, "container": c});
                if let Some(perm) = p {
                    m["permission"] = serde_json::Value::String(perm.to_string());
                }
                m
            })
            .collect();
        let json = serde_json::json!({"overlays": {"directories": dirs}});
        std::fs::write(amux_dir.join("config.json"), json.to_string()).unwrap();
    }

    // ─── parse_flag_overlays unit tests ──────────────────────────────────────

    #[test]
    fn parse_flag_overlays_empty_slice_returns_empty() {
        let result = parse_flag_overlays(&[]).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn parse_flag_overlays_single_flag_value() {
        let flags = vec!["dir(/data:/mnt/data:ro)".to_string()];
        let result = parse_flag_overlays(&flags).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].host_path, PathBuf::from("/data"));
        assert_eq!(result[0].container_path, PathBuf::from("/mnt/data"));
        assert_eq!(result[0].permission, MountPermission::ReadOnly);
    }

    #[test]
    fn parse_flag_overlays_multiple_repeated_flags_joined_by_comma() {
        // Two separate --overlay values are joined with ',' before parsing.
        let flags = vec![
            "dir(/a:/mnt/a:ro)".to_string(),
            "dir(/b:/mnt/b:rw)".to_string(),
        ];
        let result = parse_flag_overlays(&flags).unwrap();
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn parse_flag_overlays_malformed_value_is_fatal() {
        let flags = vec!["notvalid".to_string()];
        let result = parse_flag_overlays(&flags);
        assert!(result.is_err(), "malformed --overlay flag must be a fatal error");
    }

    // ─── effective_overlays resolution/merging tests ──────────────────────────

    /// Test 1: all four sources contribute without conflict → all entries present.
    #[test]
    fn no_conflict_all_four_sources_present() {
        let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let global_home = TempDir::new().unwrap();
        let repo_root = TempDir::new().unwrap();

        unsafe { std::env::set_var("AMUX_CONFIG_HOME", global_home.path().to_str().unwrap()) };
        write_global_overlays(global_home.path(), &[("/global", "/mnt/global", Some("ro"))]);
        write_repo_overlays(repo_root.path(), &[("/repo", "/mnt/repo", Some("ro"))]);

        let env_overlay = ro("/env", "/mnt/env");
        let flag_overlay = ro("/flag", "/mnt/flag");

        let result = effective_overlays(repo_root.path(), &[env_overlay], &[flag_overlay]).unwrap();
        unsafe { std::env::remove_var("AMUX_CONFIG_HOME") };

        let hosts: Vec<String> = result
            .iter()
            .map(|o| o.host_path.to_string_lossy().to_string())
            .collect();
        assert_eq!(result.len(), 4, "all 4 sources must contribute one entry; got {:?}", hosts);
        assert!(hosts.contains(&"/global".to_string()), "global entry missing; got {:?}", hosts);
        assert!(hosts.contains(&"/repo".to_string()), "repo entry missing; got {:?}", hosts);
        assert!(hosts.contains(&"/env".to_string()), "env entry missing; got {:?}", hosts);
        assert!(hosts.contains(&"/flag".to_string()), "flag entry missing; got {:?}", hosts);
    }

    /// Test 2: same host path in global and flag → flag wins on container path;
    /// permission merges to the more restrictive value.
    #[test]
    fn flag_wins_over_global_on_container_path() {
        let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let global_home = TempDir::new().unwrap();
        let repo_root = TempDir::new().unwrap();

        unsafe { std::env::set_var("AMUX_CONFIG_HOME", global_home.path().to_str().unwrap()) };
        // Global has rw and maps to /mnt/global-container.
        write_global_overlays(global_home.path(), &[("/data", "/mnt/global-container", Some("rw"))]);

        // Flag has rw and maps to /mnt/flag-container (different container path).
        let flag_overlay = rw("/data", "/mnt/flag-container");

        let result = effective_overlays(repo_root.path(), &[], &[flag_overlay]).unwrap();
        unsafe { std::env::remove_var("AMUX_CONFIG_HOME") };

        assert_eq!(result.len(), 1, "same host path must deduplicate to one entry; got {:?}", result);
        assert_eq!(
            result[0].container_path,
            PathBuf::from("/mnt/flag-container"),
            "higher-priority (flag) container path must win"
        );
        assert_eq!(result[0].permission, MountPermission::ReadWrite);
    }

    /// Test 3: same host path in project and env, both :rw → single :rw entry (no permission conflict).
    #[test]
    fn project_and_env_both_rw_deduplicates_to_single_rw() {
        let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let global_home = TempDir::new().unwrap();
        let repo_root = TempDir::new().unwrap();

        unsafe { std::env::set_var("AMUX_CONFIG_HOME", global_home.path().to_str().unwrap()) };
        // Repo config has rw.
        write_repo_overlays(repo_root.path(), &[("/data", "/mnt/data", Some("rw"))]);
        // Env overlay also rw, same container path.
        let env_overlay = rw("/data", "/mnt/data");

        let result = effective_overlays(repo_root.path(), &[env_overlay], &[]).unwrap();
        unsafe { std::env::remove_var("AMUX_CONFIG_HOME") };

        assert_eq!(result.len(), 1, "identical host+container+perm must deduplicate to one entry; got {:?}", result);
        assert_eq!(result[0].permission, MountPermission::ReadWrite);
    }

    /// Test 4: same host path, global :rw and flag :ro → :ro wins (most restrictive).
    /// A warning is emitted by merge_with_lower when permissions differ.
    #[test]
    fn global_rw_flag_ro_results_in_ro() {
        let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let global_home = TempDir::new().unwrap();
        let repo_root = TempDir::new().unwrap();

        unsafe { std::env::set_var("AMUX_CONFIG_HOME", global_home.path().to_str().unwrap()) };
        write_global_overlays(global_home.path(), &[("/data", "/mnt/data", Some("rw"))]);

        let flag_overlay = ro("/data", "/mnt/data");

        let result = effective_overlays(repo_root.path(), &[], &[flag_overlay]).unwrap();
        unsafe { std::env::remove_var("AMUX_CONFIG_HOME") };

        assert_eq!(result.len(), 1);
        assert_eq!(
            result[0].permission,
            MountPermission::ReadOnly,
            "ro must win over rw regardless of source priority"
        );
    }

    /// Test 5: same host path, global :ro and flag :rw → :ro wins (lower permission wins).
    /// A warning is emitted even though the lower-priority source is more restrictive.
    #[test]
    fn global_ro_flag_rw_results_in_ro() {
        let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let global_home = TempDir::new().unwrap();
        let repo_root = TempDir::new().unwrap();

        unsafe { std::env::set_var("AMUX_CONFIG_HOME", global_home.path().to_str().unwrap()) };
        write_global_overlays(global_home.path(), &[("/data", "/mnt/data", Some("ro"))]);

        // Flag (highest priority) requests rw, but global has ro → ro wins.
        let flag_overlay = rw("/data", "/mnt/data");

        let result = effective_overlays(repo_root.path(), &[], &[flag_overlay]).unwrap();
        unsafe { std::env::remove_var("AMUX_CONFIG_HOME") };

        assert_eq!(result.len(), 1);
        assert_eq!(
            result[0].permission,
            MountPermission::ReadOnly,
            "ro from lower-priority source must prevent rw escalation by higher-priority flag"
        );
    }

    /// Test 6: two entries with the same host path AND container path → de-duplicated to one.
    #[test]
    fn same_host_same_container_deduplicates_to_one_entry() {
        let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let global_home = TempDir::new().unwrap();
        let repo_root = TempDir::new().unwrap();

        unsafe { std::env::set_var("AMUX_CONFIG_HOME", global_home.path().to_str().unwrap()) };

        let env_overlay = ro("/data", "/mnt/data");
        let flag_overlay = ro("/data", "/mnt/data");

        let result = effective_overlays(repo_root.path(), &[env_overlay], &[flag_overlay]).unwrap();
        unsafe { std::env::remove_var("AMUX_CONFIG_HOME") };

        assert_eq!(result.len(), 1, "identical entries must deduplicate to one; got {:?}", result);
        assert_eq!(result[0].host_path, PathBuf::from("/data"));
        assert_eq!(result[0].container_path, PathBuf::from("/mnt/data"));
        assert_eq!(result[0].permission, MountPermission::ReadOnly);
    }

    /// Test 7: same host path, different container paths → higher-priority (flag) container wins.
    /// A warning is emitted by merge_with_lower when container paths differ.
    #[test]
    fn same_host_different_container_flag_container_wins() {
        let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let global_home = TempDir::new().unwrap();
        let repo_root = TempDir::new().unwrap();

        unsafe { std::env::set_var("AMUX_CONFIG_HOME", global_home.path().to_str().unwrap()) };

        // env has lower priority
        let env_overlay = ro("/data", "/mnt/env-container");
        // flag has higher priority
        let flag_overlay = ro("/data", "/mnt/flag-container");

        let result = effective_overlays(repo_root.path(), &[env_overlay], &[flag_overlay]).unwrap();
        unsafe { std::env::remove_var("AMUX_CONFIG_HOME") };

        assert_eq!(result.len(), 1, "same host path must deduplicate to one entry; got {:?}", result);
        assert_eq!(
            result[0].container_path,
            PathBuf::from("/mnt/flag-container"),
            "higher-priority (flag) container path must win over lower-priority (env)"
        );
    }

    /// Test 8: two entries with different host paths but the same container path
    /// → both are kept (they have different conflict keys), and a warning is logged.
    #[test]
    fn different_host_same_container_both_kept() {
        let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let global_home = TempDir::new().unwrap();
        let repo_root = TempDir::new().unwrap();

        unsafe { std::env::set_var("AMUX_CONFIG_HOME", global_home.path().to_str().unwrap()) };

        // Different host paths, same container path.
        let env_overlay = ro("/host-a", "/mnt/shared");
        let flag_overlay = ro("/host-b", "/mnt/shared");

        let result = effective_overlays(repo_root.path(), &[env_overlay], &[flag_overlay]).unwrap();
        unsafe { std::env::remove_var("AMUX_CONFIG_HOME") };

        // Different conflict keys → both entries are kept; effective_overlays warns about the collision.
        assert_eq!(result.len(), 2, "different host paths must both be kept; got {:?}", result);
        let containers: Vec<&str> = result
            .iter()
            .map(|o| o.container_path.to_str().unwrap())
            .collect();
        assert!(
            containers.iter().all(|c| *c == "/mnt/shared"),
            "both entries must target /mnt/shared; got {:?}",
            containers
        );
    }

    // ─── config malformed permission is fatal ─────────────────────────────────

    #[test]
    fn malformed_config_permission_is_fatal_error() {
        let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let global_home = TempDir::new().unwrap();
        let repo_root = TempDir::new().unwrap();

        unsafe { std::env::set_var("AMUX_CONFIG_HOME", global_home.path().to_str().unwrap()) };
        // Write a repo config with an invalid permission string.
        write_repo_overlays(repo_root.path(), &[("/data", "/mnt/data", Some("rwx"))]);

        let result = effective_overlays(repo_root.path(), &[], &[]);
        unsafe { std::env::remove_var("AMUX_CONFIG_HOME") };

        assert!(
            result.is_err(),
            "malformed permission in repo config must be a fatal error; got Ok"
        );
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("rwx") || msg.contains("permission"),
            "error must mention the bad value; got: {msg}"
        );
    }

    // ─── parse_env_overlays: malformed is now fatal ───────────────────────────

    #[test]
    fn parse_env_overlays_malformed_is_fatal() {
        let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let prev = std::env::var("AMUX_OVERLAYS").ok();
        unsafe { std::env::set_var("AMUX_OVERLAYS", "not-an-overlay") };

        let result = parse_env_overlays();

        if let Some(v) = prev {
            unsafe { std::env::set_var("AMUX_OVERLAYS", v) };
        } else {
            unsafe { std::env::remove_var("AMUX_OVERLAYS") };
        }

        assert!(result.is_err(), "malformed AMUX_OVERLAYS must be a fatal error");
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("AMUX_OVERLAYS"),
            "error must mention AMUX_OVERLAYS; got: {msg}"
        );
    }

    #[test]
    fn parse_env_overlays_empty_returns_ok_empty() {
        let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let prev = std::env::var("AMUX_OVERLAYS").ok();
        unsafe { std::env::remove_var("AMUX_OVERLAYS") };

        let result = parse_env_overlays();

        if let Some(v) = prev {
            unsafe { std::env::set_var("AMUX_OVERLAYS", v) };
        }

        assert!(result.unwrap().is_empty());
    }

    // ─── resolve_overlays: missing host path is non-fatal ────────────────────

    #[test]
    fn resolve_overlays_drops_nonexistent_host_path() {
        let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let global_home = TempDir::new().unwrap();
        let repo_root = TempDir::new().unwrap();

        unsafe { std::env::set_var("AMUX_CONFIG_HOME", global_home.path().to_str().unwrap()) };
        // Temporarily clear AMUX_OVERLAYS so it doesn't interfere.
        let prev = std::env::var("AMUX_OVERLAYS").ok();
        unsafe { std::env::remove_var("AMUX_OVERLAYS") };

        let flags = vec!["dir(/nonexistent-amux-test-xyz:/mnt/x:ro)".to_string()];
        let result = resolve_overlays(repo_root.path(), &flags).unwrap();

        // Restore env
        if let Some(v) = prev {
            unsafe { std::env::set_var("AMUX_OVERLAYS", v) };
        }
        unsafe { std::env::remove_var("AMUX_CONFIG_HOME") };

        assert!(
            result.is_empty(),
            "entry with missing host path must be dropped; got {:?}",
            result
        );
    }

    #[test]
    fn resolve_overlays_keeps_existing_host_path() {
        let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let global_home = TempDir::new().unwrap();
        let repo_root = TempDir::new().unwrap();
        let host_dir = TempDir::new().unwrap();

        unsafe { std::env::set_var("AMUX_CONFIG_HOME", global_home.path().to_str().unwrap()) };
        let prev = std::env::var("AMUX_OVERLAYS").ok();
        unsafe { std::env::remove_var("AMUX_OVERLAYS") };

        let flag_val = format!(
            "dir({}:/mnt/x:ro)",
            host_dir.path().to_str().unwrap()
        );
        let flags = vec![flag_val];
        let result = resolve_overlays(repo_root.path(), &flags).unwrap();

        if let Some(v) = prev {
            unsafe { std::env::set_var("AMUX_OVERLAYS", v) };
        }
        unsafe { std::env::remove_var("AMUX_CONFIG_HOME") };

        assert_eq!(result.len(), 1, "entry with existing host path must be kept");
        assert_eq!(result[0].container_path, PathBuf::from("/mnt/x"));
    }
}
