use crate::cli::{Agent, ConfigAction};
use crate::commands::init::find_git_root;
use crate::config::{
    load_global_config, load_repo_config, migrate_legacy_repo_config, save_global_config,
    save_repo_config, GlobalConfig, RepoConfig, DEFAULT_SCROLLBACK_LINES,
};
use anyhow::{bail, Result};
use std::path::Path;
use std::sync::Arc;

/// Which scopes a config field belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldScope {
    GlobalOnly,
    RepoOnly,
    Both,
}

/// Metadata describing a single user-facing configuration field.
/// This is the single source of truth for field definitions used by both
/// CLI output and the TUI config dialog.
pub struct ConfigFieldDef {
    pub key: &'static str,
    pub scope: FieldScope,
    pub hint: &'static str,
    pub builtin_default: &'static str,
    pub settable: bool,
}

/// All user-facing config fields in canonical display order.
/// This table drives display, validation, and help text for both CLI and TUI.
pub static ALL_FIELDS: &[ConfigFieldDef] = &[
    ConfigFieldDef {
        key: "default_agent",
        scope: FieldScope::GlobalOnly,
        hint: "claude | codex | opencode | maki | gemini",
        builtin_default: "claude",
        settable: true,
    },
    ConfigFieldDef {
        key: "runtime",
        scope: FieldScope::GlobalOnly,
        hint: "docker | apple-containers",
        builtin_default: "docker",
        settable: true,
    },
    ConfigFieldDef {
        key: "terminal_scrollback_lines",
        scope: FieldScope::Both,
        hint: "positive integer (e.g. 10000)",
        builtin_default: "10000",
        settable: true,
    },
    ConfigFieldDef {
        key: "yolo_disallowed_tools",
        scope: FieldScope::Both,
        hint: "comma-separated tool names (e.g. Bash,computer); empty string clears",
        builtin_default: "(empty)",
        settable: true,
    },
    ConfigFieldDef {
        key: "env_passthrough",
        scope: FieldScope::Both,
        hint: "comma-separated env var names (e.g. HOME,PATH); empty string clears",
        builtin_default: "(empty)",
        settable: true,
    },
    ConfigFieldDef {
        key: "agent",
        scope: FieldScope::RepoOnly,
        hint: "claude | codex | opencode | maki | gemini",
        builtin_default: "(inherits default_agent)",
        settable: true,
    },
    ConfigFieldDef {
        key: "auto_agent_auth_accepted",
        scope: FieldScope::RepoOnly,
        hint: "managed by the agent auth flow; read-only here",
        builtin_default: "(not set)",
        settable: false,
    },
];

/// Look up a field definition by its CLI/TUI key.
pub fn find_field(key: &str) -> Option<&'static ConfigFieldDef> {
    ALL_FIELDS.iter().find(|f| f.key == key)
}

fn valid_field_names() -> String {
    ALL_FIELDS.iter().map(|f| f.key).collect::<Vec<_>>().join(", ")
}

fn valid_agent_values() -> Vec<&'static str> {
    Agent::all().iter().map(|a| a.as_str()).collect()
}

/// Parse a comma-separated string into a `Vec<String>`, trimming whitespace from each element.
/// An empty input yields an empty `Vec` (not `None`), actively overriding any global value.
pub fn parse_vec_value(value: &str) -> Vec<String> {
    if value.trim().is_empty() {
        return vec![];
    }
    value
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Format a `Vec<String>` for human display. Empty vec → `"(empty)"`.
pub fn format_vec(v: &[String]) -> String {
    if v.is_empty() {
        "(empty)".to_string()
    } else {
        v.join(", ")
    }
}

/// Truncate a string to at most `max` bytes, appending `"..."` when truncation occurs.
/// Values in config tables are ASCII so byte-level truncation is safe.
fn truncate_display(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}...", &s[..max.saturating_sub(3)])
    }
}

/// Get the display string for the Global column.
/// Returns `"N/A"` for repo-only fields.
/// Appends `" (built-in)"` when the field is not set in the global config file.
pub fn global_display(field: &ConfigFieldDef, global: &GlobalConfig) -> String {
    match field.scope {
        FieldScope::RepoOnly => "N/A".to_string(),
        _ => match field.key {
            "default_agent" => global
                .default_agent
                .as_deref()
                .map(|v| v.to_string())
                .unwrap_or_else(|| format!("{} (built-in)", field.builtin_default)),
            "runtime" => global
                .runtime
                .as_deref()
                .map(|v| v.to_string())
                .unwrap_or_else(|| format!("{} (built-in)", field.builtin_default)),
            "terminal_scrollback_lines" => global
                .terminal_scrollback_lines
                .map(|v| v.to_string())
                .unwrap_or_else(|| format!("{} (built-in)", field.builtin_default)),
            "yolo_disallowed_tools" => global
                .yolo_disallowed_tools
                .as_ref()
                .map(|v| format_vec(v))
                .unwrap_or_else(|| format!("{} (built-in)", field.builtin_default)),
            "env_passthrough" => global
                .env_passthrough
                .as_ref()
                .map(|v| format_vec(v))
                .unwrap_or_else(|| format!("{} (built-in)", field.builtin_default)),
            _ => "N/A".to_string(),
        },
    }
}

/// Get the display string for the Repo column.
/// Returns `"N/A"` for global-only fields; `"(not set)"` when absent from repo config.
pub fn repo_display(field: &ConfigFieldDef, repo: Option<&RepoConfig>) -> String {
    match field.scope {
        FieldScope::GlobalOnly => "N/A".to_string(),
        _ => {
            let repo = match repo {
                None => return "(not set)".to_string(),
                Some(r) => r,
            };
            match field.key {
                "agent" => repo
                    .agent
                    .as_deref()
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "(not set)".to_string()),
                "auto_agent_auth_accepted" => repo
                    .auto_agent_auth_accepted
                    .map(|v| format!("{} (read-only)", v))
                    .unwrap_or_else(|| "(not set)".to_string()),
                "terminal_scrollback_lines" => repo
                    .terminal_scrollback_lines
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "(not set)".to_string()),
                "yolo_disallowed_tools" => repo
                    .yolo_disallowed_tools
                    .as_ref()
                    .map(|v| format_vec(v))
                    .unwrap_or_else(|| "(not set)".to_string()),
                "env_passthrough" => repo
                    .env_passthrough
                    .as_ref()
                    .map(|v| format_vec(v))
                    .unwrap_or_else(|| "(not set)".to_string()),
                _ => "(not set)".to_string(),
            }
        }
    }
}

/// Get the display string for the Effective column.
/// Resolves precedence (repo → global → built-in) using the supplied in-memory configs.
/// Callers must pass the already-loaded configs; this function never reads from disk.
pub fn effective_display(
    field: &ConfigFieldDef,
    global: &GlobalConfig,
    repo: Option<&RepoConfig>,
) -> String {
    match field.key {
        "default_agent" => global.default_agent.as_deref().unwrap_or("claude").to_string(),
        "runtime" => global.runtime.as_deref().unwrap_or("docker").to_string(),
        "terminal_scrollback_lines" => {
            if let Some(repo) = repo {
                if let Some(v) = repo.terminal_scrollback_lines {
                    return v.to_string();
                }
            }
            global
                .terminal_scrollback_lines
                .unwrap_or(DEFAULT_SCROLLBACK_LINES)
                .to_string()
        }
        "yolo_disallowed_tools" => {
            if let Some(repo) = repo {
                if let Some(ref v) = repo.yolo_disallowed_tools {
                    return format_vec(v);
                }
            }
            global
                .yolo_disallowed_tools
                .as_ref()
                .map(|v| format_vec(v))
                .unwrap_or_else(|| "(empty)".to_string())
        }
        "env_passthrough" => {
            if let Some(repo) = repo {
                if let Some(ref v) = repo.env_passthrough {
                    return format_vec(v);
                }
            }
            global
                .env_passthrough
                .as_ref()
                .map(|v| format_vec(v))
                .unwrap_or_else(|| "(empty)".to_string())
        }
        "agent" => {
            if let Some(repo) = repo {
                if let Some(ref a) = repo.agent {
                    return a.clone();
                }
            }
            global.default_agent.as_deref().unwrap_or("claude").to_string()
        }
        "auto_agent_auth_accepted" => {
            if let Some(repo) = repo {
                repo.auto_agent_auth_accepted
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "(not set)".to_string())
            } else {
                "(not set)".to_string()
            }
        }
        _ => "?".to_string(),
    }
}

/// Compute the Override column indicator: `"yes"` when repo value shadows global, `"—"` otherwise.
pub fn override_indicator(
    field: &ConfigFieldDef,
    global: &GlobalConfig,
    repo: Option<&RepoConfig>,
) -> &'static str {
    let repo = match repo {
        None => return "—",
        Some(r) => r,
    };
    match field.key {
        "default_agent" | "runtime" => "—",
        "terminal_scrollback_lines" => {
            if let Some(rv) = repo.terminal_scrollback_lines {
                let gv = global
                    .terminal_scrollback_lines
                    .unwrap_or(DEFAULT_SCROLLBACK_LINES);
                if rv != gv { "yes" } else { "—" }
            } else {
                "—"
            }
        }
        "yolo_disallowed_tools" => {
            if let Some(ref rv) = repo.yolo_disallowed_tools {
                let gv = global
                    .yolo_disallowed_tools
                    .as_deref()
                    .unwrap_or(&[]);
                if rv.as_slice() != gv { "yes" } else { "—" }
            } else {
                "—"
            }
        }
        "env_passthrough" => {
            if let Some(ref rv) = repo.env_passthrough {
                let gv = global.env_passthrough.as_deref().unwrap_or(&[]);
                if rv.as_slice() != gv { "yes" } else { "—" }
            } else {
                "—"
            }
        }
        "agent" => {
            // Only flag an override when the user has *explicitly* set default_agent globally
            // AND the repo uses a different agent. If global is unset (None), the repo is just
            // providing a repo-specific preference, not overriding an explicit global choice.
            match (&repo.agent, &global.default_agent) {
                (Some(ra), Some(ga)) if ra.as_str() != ga.as_str() => "yes",
                _ => "—",
            }
        }
        "auto_agent_auth_accepted" => "—",
        _ => "—",
    }
}

/// Validate a value string for the given field.
/// Returns `Err` with a human-readable message for invalid input.
pub fn validate_value(field: &ConfigFieldDef, value: &str) -> Result<()> {
    match field.key {
        "default_agent" | "agent" => {
            let valid = valid_agent_values();
            if !valid.contains(&value) {
                bail!(
                    "Invalid value '{}' for '{}'. Valid values: {}",
                    value,
                    field.key,
                    valid.join(", ")
                );
            }
        }
        "runtime" => {
            if !["docker", "apple-containers"].contains(&value) {
                bail!(
                    "Invalid value '{}' for 'runtime'. Valid values: docker, apple-containers",
                    value
                );
            }
        }
        "terminal_scrollback_lines" => {
            let n: usize = value.trim().parse().map_err(|_| {
                anyhow::anyhow!(
                    "Invalid value '{}' for 'terminal_scrollback_lines'. Expected a positive integer.",
                    value
                )
            })?;
            if n == 0 {
                bail!(
                    "Invalid value '0' for 'terminal_scrollback_lines'. Must be a positive integer."
                );
            }
        }
        "yolo_disallowed_tools" | "env_passthrough" => {
            // Any comma-separated string is valid; empty string clears the field.
        }
        _ => {}
    }
    Ok(())
}

/// Apply a pre-validated value string to the relevant field of a `RepoConfig`.
/// The caller must invoke `validate_value` first.
pub fn apply_to_repo(field: &ConfigFieldDef, value: &str, repo: &mut RepoConfig) {
    match field.key {
        "agent" => repo.agent = Some(value.to_string()),
        "terminal_scrollback_lines" => {
            repo.terminal_scrollback_lines = Some(value.trim().parse().expect("validated"));
        }
        "yolo_disallowed_tools" => {
            repo.yolo_disallowed_tools = Some(parse_vec_value(value));
        }
        "env_passthrough" => {
            repo.env_passthrough = Some(parse_vec_value(value));
        }
        _ => {}
    }
}

/// Apply a pre-validated value string to the relevant field of a `GlobalConfig`.
/// The caller must invoke `validate_value` first.
pub fn apply_to_global(field: &ConfigFieldDef, value: &str, global: &mut GlobalConfig) {
    match field.key {
        "default_agent" => global.default_agent = Some(value.to_string()),
        "runtime" => global.runtime = Some(value.to_string()),
        "terminal_scrollback_lines" => {
            global.terminal_scrollback_lines = Some(value.trim().parse().expect("validated"));
        }
        "yolo_disallowed_tools" => {
            global.yolo_disallowed_tools = Some(parse_vec_value(value));
        }
        "env_passthrough" => {
            global.env_passthrough = Some(parse_vec_value(value));
        }
        _ => {}
    }
}

/// Check whether a field has an explicit value set in the repo config.
/// For `default_agent` (a GlobalOnly field), the repo overrides it via the `agent` key.
fn repo_field_is_set(field: &ConfigFieldDef, repo: &RepoConfig) -> bool {
    match field.key {
        "agent" | "default_agent" => repo.agent.is_some(),
        "terminal_scrollback_lines" => repo.terminal_scrollback_lines.is_some(),
        "yolo_disallowed_tools" => repo.yolo_disallowed_tools.is_some(),
        "env_passthrough" => repo.env_passthrough.is_some(),
        _ => false,
    }
}

/// Return true when the given value (being written to repo) matches the effective global value.
fn values_match_global(field: &ConfigFieldDef, new_value: &str, global: &GlobalConfig) -> bool {
    match field.key {
        "terminal_scrollback_lines" => {
            if let Ok(n) = new_value.trim().parse::<usize>() {
                let g = global
                    .terminal_scrollback_lines
                    .unwrap_or(DEFAULT_SCROLLBACK_LINES);
                n == g
            } else {
                false
            }
        }
        "yolo_disallowed_tools" => {
            let nv = parse_vec_value(new_value);
            let gv = global.yolo_disallowed_tools.as_deref().unwrap_or(&[]);
            nv.as_slice() == gv
        }
        "env_passthrough" => {
            let nv = parse_vec_value(new_value);
            let gv = global.env_passthrough.as_deref().unwrap_or(&[]);
            nv.as_slice() == gv
        }
        _ => false,
    }
}

/// Annotation appended to the Effective line in `get` output.
fn scope_annotation(
    field: &ConfigFieldDef,
    global: &GlobalConfig,
    repo: Option<&RepoConfig>,
) -> &'static str {
    let repo = match repo {
        None => return "",
        Some(r) => r,
    };
    match field.key {
        "terminal_scrollback_lines" => {
            if let Some(rv) = repo.terminal_scrollback_lines {
                let gv = global
                    .terminal_scrollback_lines
                    .unwrap_or(DEFAULT_SCROLLBACK_LINES);
                if rv != gv {
                    "  ← repo overrides global"
                } else {
                    ""
                }
            } else if global.terminal_scrollback_lines.is_some() {
                "  ← global overrides built-in default"
            } else {
                ""
            }
        }
        "yolo_disallowed_tools" => {
            if let Some(ref rv) = repo.yolo_disallowed_tools {
                let gv = global.yolo_disallowed_tools.as_deref().unwrap_or(&[]);
                if rv.as_slice() != gv {
                    "  ← repo overrides global"
                } else {
                    ""
                }
            } else if global.yolo_disallowed_tools.is_some() {
                "  ← global overrides built-in default"
            } else {
                ""
            }
        }
        "env_passthrough" => {
            if let Some(ref rv) = repo.env_passthrough {
                let gv = global.env_passthrough.as_deref().unwrap_or(&[]);
                if rv.as_slice() != gv {
                    "  ← repo overrides global"
                } else {
                    ""
                }
            } else if global.env_passthrough.is_some() {
                "  ← global overrides built-in default"
            } else {
                ""
            }
        }
        "agent" => {
            if let Some(ref ra) = repo.agent {
                let ga = global.default_agent.as_deref().unwrap_or("claude");
                if ra.as_str() != ga {
                    "  ← repo overrides global"
                } else {
                    ""
                }
            } else if global.default_agent.is_some() {
                "  ← using default_agent from global config"
            } else {
                ""
            }
        }
        _ => "",
    }
}

// ── Command entry point ────────────────────────────────────────────────────────

pub async fn run(
    action: ConfigAction,
    _runtime: Arc<dyn crate::runtime::AgentRuntime>,
) -> Result<()> {
    let git_root = find_git_root();
    match action {
        ConfigAction::Show => show(git_root.as_deref()),
        ConfigAction::Get { field } => get(&field, git_root.as_deref()),
        ConfigAction::Set { field, value, global } => {
            set(&field, &value, global, git_root.as_deref())
        }
    }
}

// ── show ──────────────────────────────────────────────────────────────────────

fn show(git_root: Option<&Path>) -> Result<()> {
    if let Some(root) = git_root {
        let _ = migrate_legacy_repo_config(root);
    }

    let global = load_global_config()?;
    let repo = git_root
        .map(|r| load_repo_config(r))
        .transpose()?
        .unwrap_or_default();
    let repo_opt: Option<&RepoConfig> = if git_root.is_some() { Some(&repo) } else { None };

    if git_root.is_none() {
        eprintln!("Note: not inside a git repo; repo config is unavailable.");
    }

    // Fixed column widths — wide enough for all field keys and typical values.
    let cw_field = 26usize;
    let cw_global = 22usize;
    let cw_repo = 18usize;
    let cw_effective = 20usize;

    println!(
        "{:<cw_field$}  {:<cw_global$}  {:<cw_repo$}  {:<cw_effective$}  {}",
        "Field", "Global", "Repo", "Effective", "Override",
        cw_field = cw_field,
        cw_global = cw_global,
        cw_repo = cw_repo,
        cw_effective = cw_effective,
    );
    println!(
        "{}  {}  {}  {}  {}",
        "\u{2500}".repeat(cw_field),
        "\u{2500}".repeat(cw_global),
        "\u{2500}".repeat(cw_repo),
        "\u{2500}".repeat(cw_effective),
        "\u{2500}".repeat(8),
    );

    for field in ALL_FIELDS {
        let gv = global_display(field, &global);
        let rv = repo_display(field, repo_opt);
        let ev = effective_display(field, &global, repo_opt);
        let ov = override_indicator(field, &global, repo_opt);

        println!(
            "{:<cw_field$}  {:<cw_global$}  {:<cw_repo$}  {:<cw_effective$}  {}",
            truncate_display(field.key, cw_field),
            truncate_display(&gv, cw_global),
            truncate_display(&rv, cw_repo),
            truncate_display(&ev, cw_effective),
            ov,
            cw_field = cw_field,
            cw_global = cw_global,
            cw_repo = cw_repo,
            cw_effective = cw_effective,
        );
    }

    Ok(())
}

// ── get ───────────────────────────────────────────────────────────────────────

fn get(field_key: &str, git_root: Option<&Path>) -> Result<()> {
    let field = find_field(field_key).ok_or_else(|| {
        anyhow::anyhow!(
            "Unknown config field '{}'. Valid fields: {}",
            field_key,
            valid_field_names()
        )
    })?;

    if let Some(root) = git_root {
        let _ = migrate_legacy_repo_config(root);
    }

    let global = load_global_config()?;
    let repo = git_root
        .map(|r| load_repo_config(r))
        .transpose()?
        .unwrap_or_default();
    let repo_opt: Option<&RepoConfig> = if git_root.is_some() { Some(&repo) } else { None };

    let gv = global_display(field, &global);
    let rv = repo_display(field, repo_opt);
    let ev = effective_display(field, &global, repo_opt);
    let annotation = scope_annotation(field, &global, repo_opt);

    println!("Field: {}", field.key);

    // Always show all three lines; use N/A for inapplicable scopes.
    let global_line = if field.scope == FieldScope::RepoOnly {
        "N/A".to_string()
    } else {
        gv
    };
    let repo_line = if field.scope == FieldScope::GlobalOnly {
        "N/A".to_string()
    } else {
        rv
    };

    println!("  Global:     {}", global_line);
    println!("  Repo:       {}", repo_line);
    println!("  Effective:  {}{}", ev, annotation);

    Ok(())
}

// ── set (pub(crate) so unit tests can call it directly) ───────────────────────

pub(crate) fn set(field_key: &str, value: &str, use_global: bool, git_root: Option<&Path>) -> Result<()> {
    let field = find_field(field_key).ok_or_else(|| {
        anyhow::anyhow!(
            "Unknown config field '{}'. Valid fields: {}",
            field_key,
            valid_field_names()
        )
    })?;

    if !field.settable {
        bail!(
            "'{}' is managed by the agent auth flow and cannot be set via 'amux config set'.",
            field.key
        );
    }

    // Enforce scope.
    match (field.scope, use_global) {
        (FieldScope::GlobalOnly, false) => bail!(
            "'{}' is a global-only field. Use --global to set it:\n  amux config set --global {} {}",
            field.key,
            field.key,
            value
        ),
        (FieldScope::RepoOnly, true) => bail!(
            "'{}' is a repo-only field and cannot be set globally. Remove --global:\n  amux config set {} {}",
            field.key,
            field.key,
            value
        ),
        _ => {}
    }

    // Validate value before writing.
    validate_value(field, value)?;

    // Warn when setting apple-containers on a non-macOS host.
    #[cfg(not(target_os = "macos"))]
    if field.key == "runtime" && value == "apple-containers" {
        eprintln!(
            "Warning: 'apple-containers' is only supported on macOS. On this platform it will fall back to 'docker' at runtime."
        );
    }

    if use_global {
        let mut global = load_global_config()?;
        apply_to_global(field, value, &mut global);
        save_global_config(&global)?;

        // Warn when a repo config already overrides this field.
        if let Some(root) = git_root {
            if let Ok(repo) = load_repo_config(root) {
                if repo_field_is_set(field, &repo) {
                    eprintln!(
                        "Warning: repo config overrides this field; the new global value will not take effect in this repo."
                    );
                }
            }
        }

        let updated_global = load_global_config()?;
        let repo = git_root
            .map(|r| load_repo_config(r))
            .transpose()?
            .unwrap_or_default();
        let repo_opt: Option<&RepoConfig> = if git_root.is_some() { Some(&repo) } else { None };
        let eff = effective_display(field, &updated_global, repo_opt);
        println!("Set {} (global) = {}", field.key, value);
        println!("  Effective value: {}", eff);
    } else {
        let root = git_root.ok_or_else(|| {
            anyhow::anyhow!(
                "Not inside a git repository. Run inside a git repo or use --global to write a global value."
            )
        })?;

        let _ = migrate_legacy_repo_config(root);
        let mut repo = load_repo_config(root)?;
        apply_to_repo(field, value, &mut repo);
        save_repo_config(root, &repo)?;

        // Warn when the new repo value matches the effective global value.
        let global = load_global_config()?;
        if field.scope == FieldScope::Both && values_match_global(field, value, &global) {
            eprintln!("Note: repo value matches global; no override is active.");
        }

        let updated_repo = load_repo_config(root)?;
        let eff = effective_display(field, &global, Some(&updated_repo));
        println!("Set {} (repo) = {}", field.key, value);
        println!("  Effective value: {}", eff);
    }

    Ok(())
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{GlobalConfig, RepoConfig};

    // ── find_field ────────────────────────────────────────────────────────────

    #[test]
    fn find_field_returns_some_for_known_keys() {
        for key in &[
            "default_agent",
            "runtime",
            "terminal_scrollback_lines",
            "yolo_disallowed_tools",
            "env_passthrough",
            "agent",
            "auto_agent_auth_accepted",
        ] {
            assert!(find_field(key).is_some(), "expected Some for key '{}'", key);
        }
    }

    #[test]
    fn find_field_returns_none_for_unknown_keys() {
        assert!(find_field("nonexistent").is_none());
        assert!(find_field("").is_none());
        assert!(find_field("DEFAULT_AGENT").is_none()); // case-sensitive
    }

    // ── parse_vec_value ───────────────────────────────────────────────────────

    #[test]
    fn parse_vec_value_empty_string_yields_empty_vec() {
        // Critical: empty string must yield [] (not None), actively overriding global.
        assert_eq!(parse_vec_value(""), Vec::<String>::new());
    }

    #[test]
    fn parse_vec_value_whitespace_only_yields_empty_vec() {
        assert_eq!(parse_vec_value("   "), Vec::<String>::new());
    }

    #[test]
    fn parse_vec_value_single_item() {
        assert_eq!(parse_vec_value("Bash"), vec!["Bash"]);
    }

    #[test]
    fn parse_vec_value_trims_whitespace_around_items() {
        assert_eq!(parse_vec_value(" Bash , computer "), vec!["Bash", "computer"]);
    }

    #[test]
    fn parse_vec_value_filters_empty_segments_from_double_commas() {
        assert_eq!(parse_vec_value("Bash,,computer"), vec!["Bash", "computer"]);
    }

    // ── validate_value ────────────────────────────────────────────────────────

    #[test]
    fn validate_value_accepts_all_valid_agents() {
        let field = find_field("default_agent").unwrap();
        for agent in &["claude", "codex", "opencode", "maki", "gemini"] {
            assert!(validate_value(field, agent).is_ok(), "expected Ok for agent '{}'", agent);
        }
    }

    #[test]
    fn validate_value_rejects_invalid_agent() {
        let field = find_field("default_agent").unwrap();
        let err = validate_value(field, "unknown_agent").unwrap_err();
        assert!(err.to_string().contains("Invalid value"), "{}", err);
    }

    #[test]
    fn validate_value_agent_field_validates_same_set() {
        let field = find_field("agent").unwrap();
        assert!(validate_value(field, "codex").is_ok());
        assert!(validate_value(field, "bad").is_err());
    }

    #[test]
    fn validate_value_accepts_valid_runtimes() {
        let field = find_field("runtime").unwrap();
        assert!(validate_value(field, "docker").is_ok());
        assert!(validate_value(field, "apple-containers").is_ok());
    }

    #[test]
    fn validate_value_rejects_invalid_runtime() {
        let field = find_field("runtime").unwrap();
        let err = validate_value(field, "podman").unwrap_err();
        assert!(err.to_string().contains("Invalid value"), "{}", err);
    }

    #[test]
    fn validate_value_accepts_positive_integer_for_scrollback() {
        let field = find_field("terminal_scrollback_lines").unwrap();
        assert!(validate_value(field, "1").is_ok());
        assert!(validate_value(field, "10000").is_ok());
    }

    #[test]
    fn validate_value_rejects_zero_scrollback() {
        let field = find_field("terminal_scrollback_lines").unwrap();
        let err = validate_value(field, "0").unwrap_err();
        assert!(err.to_string().contains("positive integer"), "{}", err);
    }

    #[test]
    fn validate_value_rejects_non_numeric_scrollback() {
        let field = find_field("terminal_scrollback_lines").unwrap();
        assert!(validate_value(field, "abc").is_err());
        assert!(validate_value(field, "10.5").is_err());
        assert!(validate_value(field, "-1").is_err());
    }

    #[test]
    fn validate_value_accepts_any_string_for_vec_fields() {
        for key in &["yolo_disallowed_tools", "env_passthrough"] {
            let field = find_field(key).unwrap();
            assert!(validate_value(field, "Bash,computer").is_ok());
            assert!(validate_value(field, "").is_ok()); // empty string clears the field
            assert!(validate_value(field, "SINGLE").is_ok());
        }
    }

    // ── scope enforcement ─────────────────────────────────────────────────────

    #[test]
    fn set_global_only_field_without_global_flag_fails() {
        // "runtime" is GlobalOnly: must use --global.
        let err = set("runtime", "docker", false, None).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("global-only") || msg.contains("--global"),
            "expected scope error, got: {}",
            msg
        );
    }

    #[test]
    fn set_default_agent_without_global_flag_fails() {
        let err = set("default_agent", "claude", false, None).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("global-only") || msg.contains("--global"),
            "expected scope error, got: {}",
            msg
        );
    }

    #[test]
    fn set_repo_only_field_with_global_flag_fails() {
        // "agent" is RepoOnly: must NOT use --global.
        let err = set("agent", "claude", true, None).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("repo-only") || msg.contains("--global"),
            "expected scope error, got: {}",
            msg
        );
    }

    // ── read-only rejection ───────────────────────────────────────────────────

    #[test]
    fn set_auto_agent_auth_accepted_fails_without_global_flag() {
        let err = set("auto_agent_auth_accepted", "true", false, None).unwrap_err();
        assert!(
            err.to_string().contains("managed by the agent auth flow"),
            "{}",
            err
        );
    }

    #[test]
    fn set_auto_agent_auth_accepted_fails_with_global_flag() {
        // Read-only check fires before scope check.
        let err = set("auto_agent_auth_accepted", "true", true, None).unwrap_err();
        assert!(
            err.to_string().contains("managed by the agent auth flow"),
            "{}",
            err
        );
    }

    // ── override_indicator ────────────────────────────────────────────────────

    #[test]
    fn override_no_override_when_global_set_and_repo_agent_absent() {
        // (global=Some("claude"), repo=None) → no override
        let field = find_field("agent").unwrap();
        let global = GlobalConfig { default_agent: Some("claude".to_string()), ..Default::default() };
        let repo = RepoConfig::default(); // agent: None
        assert_eq!(override_indicator(field, &global, Some(&repo)), "—");
    }

    #[test]
    fn override_detected_when_both_explicitly_set_and_differ() {
        // (global=Some("claude"), repo=Some("codex")) → override detected
        let field = find_field("agent").unwrap();
        let global = GlobalConfig { default_agent: Some("claude".to_string()), ..Default::default() };
        let repo = RepoConfig { agent: Some("codex".to_string()), ..Default::default() };
        assert_eq!(override_indicator(field, &global, Some(&repo)), "yes");
    }

    #[test]
    fn override_no_override_when_global_not_set_even_if_repo_set() {
        // (global=None, repo=Some("codex")) → no override (global not set)
        // The repo is providing a repo-specific preference, not overriding an explicit global choice.
        let field = find_field("agent").unwrap();
        let global = GlobalConfig::default(); // default_agent: None
        let repo = RepoConfig { agent: Some("codex".to_string()), ..Default::default() };
        assert_eq!(override_indicator(field, &global, Some(&repo)), "—");
    }

    #[test]
    fn override_no_override_when_no_repo_config() {
        let field = find_field("agent").unwrap();
        let global = GlobalConfig { default_agent: Some("claude".to_string()), ..Default::default() };
        assert_eq!(override_indicator(field, &global, None), "—");
    }

    #[test]
    fn override_yes_for_scrollback_when_repo_differs_from_effective_global() {
        // For Both-scope fields the built-in default IS the baseline, so repo differing from
        // it still shows as "yes" (the repo is actively overriding the effective value).
        let field = find_field("terminal_scrollback_lines").unwrap();
        let global = GlobalConfig::default(); // terminal_scrollback_lines: None → built-in 10000
        let repo = RepoConfig { terminal_scrollback_lines: Some(5000), ..Default::default() };
        assert_eq!(override_indicator(field, &global, Some(&repo)), "yes");
    }

    #[test]
    fn override_no_for_scrollback_when_repo_matches_explicit_global() {
        let field = find_field("terminal_scrollback_lines").unwrap();
        let global = GlobalConfig { terminal_scrollback_lines: Some(5000), ..Default::default() };
        let repo = RepoConfig { terminal_scrollback_lines: Some(5000), ..Default::default() };
        assert_eq!(override_indicator(field, &global, Some(&repo)), "—");
    }

    // ── effective_display ─────────────────────────────────────────────────────

    #[test]
    fn effective_display_terminal_scrollback_repo_wins() {
        let field = find_field("terminal_scrollback_lines").unwrap();
        let global = GlobalConfig { terminal_scrollback_lines: Some(10000), ..Default::default() };
        let repo = RepoConfig { terminal_scrollback_lines: Some(5000), ..Default::default() };
        assert_eq!(effective_display(field, &global, Some(&repo)), "5000");
    }

    #[test]
    fn effective_display_terminal_scrollback_falls_back_to_builtin() {
        let field = find_field("terminal_scrollback_lines").unwrap();
        let global = GlobalConfig::default();
        let repo = RepoConfig::default();
        assert_eq!(
            effective_display(field, &global, Some(&repo)),
            "10000",
            "should return built-in default when neither config is set"
        );
    }

    #[test]
    fn effective_display_uses_passed_in_configs_not_disk() {
        // global has 9999 and repo is empty — effective should be 9999 from the passed-in global.
        let field = find_field("terminal_scrollback_lines").unwrap();
        let global = GlobalConfig { terminal_scrollback_lines: Some(9999), ..Default::default() };
        let repo = RepoConfig::default();
        assert_eq!(effective_display(field, &global, Some(&repo)), "9999");
    }

    #[test]
    fn effective_display_env_passthrough_repo_empty_vec_wins_over_global() {
        // An explicit empty Vec in repo must override a non-empty global list.
        let field = find_field("env_passthrough").unwrap();
        let global = GlobalConfig {
            env_passthrough: Some(vec!["GLOBAL_VAR".to_string()]),
            ..Default::default()
        };
        let repo = RepoConfig {
            env_passthrough: Some(vec![]), // explicit empty overrides global
            ..Default::default()
        };
        assert_eq!(effective_display(field, &global, Some(&repo)), "(empty)");
    }
}
