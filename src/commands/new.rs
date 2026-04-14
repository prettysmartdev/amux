use crate::commands::download;
use crate::commands::init::find_git_root_from;
use crate::commands::output::OutputSink;
use crate::config::{load_repo_config, save_repo_config, RepoConfig};
use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

/// Resolve the work items directory and template path for the given repo config.
///
/// Resolution order for directory:
/// 1. `repo_config.work_items.dir` (resolved relative to `git_root`)
/// 2. `git_root/aspec/work-items/` (legacy fallback if it exists and is a directory)
/// 3. `None` if neither exists
///
/// Resolution order for template:
/// 1. `repo_config.work_items.template` (resolved relative to `git_root`)
/// 2. `git_root/aspec/work-items/0000-template.md` (legacy path, if it exists)
/// 3. `None` (triggers auto-discovery in callers)
pub fn resolve_work_item_paths(
    git_root: &Path,
    repo_config: &RepoConfig,
) -> (Option<PathBuf>, Option<PathBuf>) {
    // ── Directory ──────────────────────────────────────────────────────────────
    let dir = if let Some(configured) = repo_config.work_items_dir(git_root) {
        if configured.is_dir() {
            Some(configured)
        } else {
            // Configured but not a valid directory — treat as missing.
            None
        }
    } else {
        // Fall back to legacy path.
        let legacy = git_root.join("aspec/work-items");
        if legacy.is_dir() { Some(legacy) } else { None }
    };

    // ── Template ───────────────────────────────────────────────────────────────
    let template = if let Some(configured) = repo_config.work_items_template(git_root) {
        if configured.is_file() {
            Some(configured)
        } else {
            // Configured but missing — fall through to auto-discovery.
            None
        }
    } else {
        // Fall back to legacy template path.
        let legacy = git_root.join("aspec/work-items/0000-template.md");
        if legacy.is_file() { Some(legacy) } else { None }
    };

    (dir, template)
}

/// Collect all files in `work_items_dir` whose name ends with `template.md`,
/// sorted lexicographically. Returns an empty `Vec` if the directory is absent
/// or unreadable.
fn all_templates(work_items_dir: &Path) -> Vec<PathBuf> {
    if !work_items_dir.is_dir() {
        return vec![];
    }
    let mut matches: Vec<PathBuf> = std::fs::read_dir(work_items_dir)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .filter(|e| {
            let name = e.file_name();
            let n = name.to_string_lossy();
            n.ends_with("template.md") && e.path().is_file()
        })
        .map(|e| e.path())
        .collect();
    matches.sort();
    matches
}

/// Scan `work_items_dir` for files whose name ends with `template.md`.
/// Returns the lexicographically first match, or `None` if none found.
pub fn discover_template(work_items_dir: &Path) -> Option<PathBuf> {
    all_templates(work_items_dir).into_iter().next()
}

/// Command-mode entry point: runs `new` interactively via stdin/stdout.
pub async fn run() -> Result<()> {
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    run_with_sink(&OutputSink::Stdout, None, None, &cwd).await
}

/// Core logic shared between command mode and TUI mode.
///
/// When `kind` or `title` are `None`, the user is prompted interactively
/// (stdin for command mode, or pre-supplied for TUI mode).
///
/// `cwd` is the working directory to search upward from for the Git root. In CLI mode this is
/// `std::env::current_dir()`; in TUI mode this is the active tab's `cwd`.
pub async fn run_with_sink(
    out: &OutputSink,
    kind: Option<WorkItemKind>,
    title: Option<String>,
    cwd: &std::path::Path,
) -> Result<()> {
    let git_root = find_git_root_from(cwd).context("Not inside a Git repository")?;
    let repo_config = load_repo_config(&git_root).unwrap_or_default();

    // If work_items.dir is configured but points to a non-directory, fail clearly.
    if let Some(configured_dir) = repo_config.work_items_dir(&git_root) {
        if !configured_dir.is_dir() {
            bail!(
                "Configured work_items.dir '{}' is not a directory.",
                configured_dir.display()
            );
        }
    }

    let (work_items_dir_opt, template_path_opt) = resolve_work_item_paths(&git_root, &repo_config);

    // Warn when a template was configured but the file is missing.
    if let Some(configured_template) = repo_config.work_items_template(&git_root) {
        if template_path_opt.is_none() {
            out.println(format!(
                "Warning: Configured template '{}' not found, falling back to auto-discovery.",
                configured_template.display()
            ));
        }
    }

    // Resolve the work items directory, downloading aspec as a legacy fallback.
    let work_items_dir = match work_items_dir_opt {
        Some(d) => d,
        None => {
            let has_custom_dir = repo_config
                .work_items
                .as_ref()
                .and_then(|w| w.dir.as_deref())
                .map(|s| !s.is_empty())
                .unwrap_or(false);
            if has_custom_dir {
                bail!(
                    "`specs new` requires a work items directory. \
                     Run `amux config set work_items.dir <path>` to configure one, \
                     or run `amux init --aspec` to set up the aspec folder."
                );
            }
            // No custom config — try downloading aspec for backward compatibility.
            out.println(
                "Template not found locally, downloading aspec folder from GitHub..."
                    .to_string(),
            );
            download::download_aspec_folder(&git_root, out)
                .await
                .context("Failed to download aspec folder for template")?;
            let (d2, _) = resolve_work_item_paths(&git_root, &repo_config);
            d2.ok_or_else(|| {
                anyhow::anyhow!(
                    "`specs new` requires a work items directory. \
                     Run `amux config set work_items.dir <path>` to configure one, \
                     or run `amux init --aspec` to set up the aspec folder."
                )
            })?
        }
    };

    // Re-resolve template path after potential download.
    let template_path_opt = if template_path_opt.is_none() {
        let (_, t) = resolve_work_item_paths(&git_root, &repo_config);
        t
    } else {
        template_path_opt
    };

    let next_number = next_work_item_number(&work_items_dir)?;

    // Get work item kind.
    let kind = match kind {
        Some(k) => k,
        None => prompt_kind(out)?,
    };

    // Get work item title.
    let title = match title {
        Some(t) => t,
        None => prompt_title(out)?,
    };

    // Determine file content from template, auto-discovery, or minimal stub.
    let content = match template_path_opt {
        Some(ref path) => {
            let tmpl =
                std::fs::read_to_string(path).context("Failed to read template file")?;
            apply_template(&tmpl, &kind, &title)
        }
        None => {
            // Template auto-discovery (command mode only — TUI mode skips stdin prompts).
            if out.supports_color() {
                let candidates = all_templates(&work_items_dir);
                match candidates.first().cloned() {
                    Some(candidate) => {
                        if candidates.len() > 1 {
                            out.println(format!(
                                "Found {} template candidates in {}.",
                                candidates.len(),
                                work_items_dir.display()
                            ));
                        }
                        let rel = candidate
                            .strip_prefix(&git_root)
                            .unwrap_or(&candidate)
                            .display()
                            .to_string();
                        out.println(format!(
                            "Found potential template: {}. Use it? [Y/n]",
                            rel
                        ));
                        let answer = out.read_line();
                        let confirmed =
                            matches!(answer.trim().to_lowercase().as_str(), "" | "y" | "yes");
                        if confirmed {
                            // Save template path to repo config.
                            let mut updated = load_repo_config(&git_root).unwrap_or_default();
                            let wi = updated
                                .work_items
                                .get_or_insert_with(crate::config::WorkItemsConfig::default);
                            wi.template = Some(rel);
                            save_repo_config(&git_root, &updated)?;
                            let tmpl = std::fs::read_to_string(&candidate)
                                .context("Failed to read template file")?;
                            apply_template(&tmpl, &kind, &title)
                        } else {
                            format!("# {}: {}\n", kind.as_str(), title)
                        }
                    }
                    None => format!("# {}: {}\n", kind.as_str(), title),
                }
            } else {
                // TUI mode: no stdin prompts, use minimal stub.
                format!("# {}: {}\n", kind.as_str(), title)
            }
        }
    };

    // Build the filename.
    let slug = slugify(&title);
    let filename = format!("{:04}-{}.md", next_number, slug);
    let file_path = work_items_dir.join(&filename);

    std::fs::write(&file_path, &content)
        .with_context(|| format!("Failed to write {}", file_path.display()))?;

    out.println(format!("Created work item: {}", file_path.display()));

    // Try to open in VS Code if running inside the VS Code terminal.
    #[cfg(not(test))]
    if is_vscode_terminal() {
        open_in_vscode(&file_path);
        out.println(format!("Opened {} in VS Code.", filename));
    }

    Ok(())
}

/// The four types of work items.
#[derive(Debug, Clone, PartialEq)]
pub enum WorkItemKind {
    Feature,
    Bug,
    Task,
    Enhancement,
}

impl WorkItemKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            WorkItemKind::Feature => "Feature",
            WorkItemKind::Bug => "Bug",
            WorkItemKind::Task => "Task",
            WorkItemKind::Enhancement => "Enhancement",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "feature" | "f" | "1" => Some(WorkItemKind::Feature),
            "bug" | "b" | "2" => Some(WorkItemKind::Bug),
            "task" | "t" | "3" => Some(WorkItemKind::Task),
            "enhancement" | "e" | "4" => Some(WorkItemKind::Enhancement),
            _ => None,
        }
    }
}

/// Find the work items template, searching the git root.
pub fn find_template(git_root: &Path) -> Result<PathBuf> {
    let path = git_root.join("aspec/work-items/0000-template.md");
    if path.exists() {
        return Ok(path);
    }
    bail!(
        "Template not found at {}. \
         Download it from https://github.com/cohix/aspec/raw/refs/heads/main/aspec/work-items/0000-template.md \
         and place it in your project's aspec/work-items/ directory.",
        path.display()
    )
}

/// Scan the work-items directory and determine the next sequential number.
pub fn next_work_item_number(work_items_dir: &Path) -> Result<u32> {
    let mut max_number: u32 = 0;

    if work_items_dir.exists() {
        for entry in std::fs::read_dir(work_items_dir)
            .with_context(|| format!("Failed to read {}", work_items_dir.display()))?
        {
            let entry = entry?;
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if let Some(num) = parse_work_item_number(&name_str) {
                if num > max_number {
                    max_number = num;
                }
            }
        }
    }

    Ok(max_number + 1)
}

/// Parse a work item number from a filename like "0002-some-feature.md".
pub fn parse_work_item_number(filename: &str) -> Option<u32> {
    let prefix = filename.split('-').next()?;
    prefix.parse::<u32>().ok()
}

/// Convert a user-provided title to a filename slug.
/// Lowercase, replace spaces with hyphens, remove non-alphanumeric/non-hyphen chars.
pub fn slugify(title: &str) -> String {
    title
        .to_lowercase()
        .chars()
        .map(|c| if c == ' ' { '-' } else { c })
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
        .collect::<String>()
}

/// Apply substitutions to the template content.
pub fn apply_template(template: &str, kind: &WorkItemKind, title: &str) -> String {
    let mut result = String::new();
    for line in template.lines() {
        if line.starts_with("# Work Item:") {
            result.push_str(&format!("# Work Item: {}", kind.as_str()));
        } else if line.starts_with("Title:") {
            result.push_str(&format!("Title: {}", title));
        } else {
            result.push_str(line);
        }
        result.push('\n');
    }
    result
}

/// Check if we're running inside a VS Code integrated terminal.
pub fn is_vscode_terminal() -> bool {
    std::env::var("TERM_PROGRAM").map_or(false, |v| v == "vscode")
}

/// Open a file in VS Code using the `code` CLI.
pub fn open_in_vscode(path: &Path) {
    let _ = std::process::Command::new("code")
        .arg("--reuse-window")
        .arg(path)
        .spawn();
}

/// Prompt the user to select a work item kind (command mode via stdin).
pub fn prompt_kind(out: &OutputSink) -> Result<WorkItemKind> {
    out.println("Select work item type:");
    out.println("  1) Feature");
    out.println("  2) Bug");
    out.println("  3) Task");
    out.println("  4) Enhancement");
    out.print("Choice [1/2/3/4]: ");

    let mut input = String::new();
    std::io::stdin()
        .read_line(&mut input)
        .context("Failed to read input")?;

    WorkItemKind::from_str(&input).context("Invalid choice. Please enter 1, 2, 3, or 4.")
}

/// Prompt the user to provide a title (command mode via stdin).
pub fn prompt_title(out: &OutputSink) -> Result<String> {
    out.print("Work item title: ");

    let mut input = String::new();
    std::io::stdin()
        .read_line(&mut input)
        .context("Failed to read input")?;

    let title = input.trim().to_string();
    if title.is_empty() {
        bail!("Title cannot be empty.");
    }
    Ok(title)
}

/// Creates a work item file and returns its number. Does NOT open in VS Code.
/// Used by `specs new --interview` to create the file before launching the agent.
pub async fn create_file_return_number(
    out: &OutputSink,
    kind: WorkItemKind,
    title: String,
    cwd: &Path,
) -> Result<u32> {
    let git_root = find_git_root_from(cwd).context("Not inside a Git repository")?;
    let repo_config = load_repo_config(&git_root).unwrap_or_default();

    // If work_items.dir is configured but points to a non-directory, fail clearly.
    if let Some(configured_dir) = repo_config.work_items_dir(&git_root) {
        if !configured_dir.is_dir() {
            bail!(
                "Configured work_items.dir '{}' is not a directory.",
                configured_dir.display()
            );
        }
    }

    let (work_items_dir_opt, template_path_opt) = resolve_work_item_paths(&git_root, &repo_config);

    // Warn when a template was configured but the file is missing.
    if let Some(configured_template) = repo_config.work_items_template(&git_root) {
        if template_path_opt.is_none() {
            out.println(format!(
                "Warning: Configured template '{}' not found, falling back to auto-discovery.",
                configured_template.display()
            ));
        }
    }

    let work_items_dir = match work_items_dir_opt {
        Some(d) => d,
        None => {
            let has_custom_dir = repo_config
                .work_items
                .as_ref()
                .and_then(|w| w.dir.as_deref())
                .map(|s| !s.is_empty())
                .unwrap_or(false);
            if has_custom_dir {
                bail!(
                    "`specs new` requires a work items directory. \
                     Run `amux config set work_items.dir <path>` to configure one, \
                     or run `amux init --aspec` to set up the aspec folder."
                );
            }
            // No custom config — try downloading aspec for backward compatibility.
            out.println(
                "Template not found locally, downloading aspec folder from GitHub..."
                    .to_string(),
            );
            download::download_aspec_folder(&git_root, out)
                .await
                .context("Failed to download aspec folder for template")?;
            let (d2, _) = resolve_work_item_paths(&git_root, &repo_config);
            d2.context(
                "`specs new` requires a work items directory. \
                 Run `amux config set work_items.dir <path>` to configure one.",
            )?
        }
    };

    // Re-resolve template after possible download.
    let template_path_opt = if template_path_opt.is_none() {
        let (_, t) = resolve_work_item_paths(&git_root, &repo_config);
        t
    } else {
        template_path_opt
    };

    let next_number = next_work_item_number(&work_items_dir)?;

    // Determine content from template or minimal stub.
    let content = match template_path_opt {
        Some(ref path) => {
            let tmpl =
                std::fs::read_to_string(path).context("Failed to read template file")?;
            apply_template(&tmpl, &kind, &title)
        }
        None => format!("# {}: {}\n", kind.as_str(), title),
    };

    // Build the filename.
    let slug = slugify(&title);
    let filename = format!("{:04}-{}.md", next_number, slug);
    let file_path = work_items_dir.join(&filename);

    std::fs::write(&file_path, &content)
        .with_context(|| format!("Failed to write {}", file_path.display()))?;

    out.println(format!("Created work item: {}", file_path.display()));

    Ok(next_number)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use tokio::sync::mpsc::unbounded_channel;

    #[test]
    fn slugify_basic() {
        assert_eq!(slugify("My New Feature"), "my-new-feature");
    }

    #[test]
    fn slugify_removes_special_chars() {
        assert_eq!(slugify("Fix: the bug!"), "fix-the-bug");
    }

    #[test]
    fn slugify_preserves_numbers() {
        assert_eq!(slugify("Add step 2 support"), "add-step-2-support");
    }

    #[test]
    fn slugify_empty_string() {
        assert_eq!(slugify(""), "");
    }

    #[test]
    fn slugify_multiple_spaces() {
        assert_eq!(slugify("a  b   c"), "a--b---c");
    }

    #[test]
    fn parse_work_item_number_valid() {
        assert_eq!(parse_work_item_number("0001-some-feature.md"), Some(1));
        assert_eq!(parse_work_item_number("0042-fix-bug.md"), Some(42));
        assert_eq!(parse_work_item_number("0000-template.md"), Some(0));
    }

    #[test]
    fn parse_work_item_number_invalid() {
        assert_eq!(parse_work_item_number("readme.md"), None);
        assert_eq!(parse_work_item_number(""), None);
    }

    #[test]
    fn next_work_item_number_empty_dir() {
        let tmp = TempDir::new().unwrap();
        let num = next_work_item_number(tmp.path()).unwrap();
        assert_eq!(num, 1);
    }

    #[test]
    fn next_work_item_number_with_existing() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("0000-template.md"), "").unwrap();
        std::fs::write(tmp.path().join("0001-first.md"), "").unwrap();
        std::fs::write(tmp.path().join("0003-third.md"), "").unwrap();
        let num = next_work_item_number(tmp.path()).unwrap();
        assert_eq!(num, 4);
    }

    #[test]
    fn next_work_item_number_nonexistent_dir() {
        let tmp = TempDir::new().unwrap();
        let nonexistent = tmp.path().join("does-not-exist");
        let num = next_work_item_number(&nonexistent).unwrap();
        assert_eq!(num, 1);
    }

    #[test]
    fn apply_template_replaces_header_and_title() {
        let template = "# Work Item: [Feature | Bug | Task]\n\nTitle: title\nIssue: issuelink\n";
        let result = apply_template(template, &WorkItemKind::Bug, "Fix login crash");
        assert!(result.contains("# Work Item: Bug"));
        assert!(result.contains("Title: Fix login crash"));
        assert!(!result.contains("[Feature | Bug | Task]"));
        assert!(!result.contains("Title: title\n"));
    }

    #[test]
    fn apply_template_preserves_other_content() {
        let template = "# Work Item: [Feature | Bug | Task]\n\nTitle: title\n\n## Summary:\n- summary\n";
        let result = apply_template(template, &WorkItemKind::Task, "My Task");
        assert!(result.contains("## Summary:"));
        assert!(result.contains("- summary"));
    }

    #[test]
    fn work_item_kind_from_str_variants() {
        assert_eq!(WorkItemKind::from_str("feature"), Some(WorkItemKind::Feature));
        assert_eq!(WorkItemKind::from_str("Feature"), Some(WorkItemKind::Feature));
        assert_eq!(WorkItemKind::from_str("f"), Some(WorkItemKind::Feature));
        assert_eq!(WorkItemKind::from_str("1"), Some(WorkItemKind::Feature));
        assert_eq!(WorkItemKind::from_str("bug"), Some(WorkItemKind::Bug));
        assert_eq!(WorkItemKind::from_str("Bug"), Some(WorkItemKind::Bug));
        assert_eq!(WorkItemKind::from_str("b"), Some(WorkItemKind::Bug));
        assert_eq!(WorkItemKind::from_str("2"), Some(WorkItemKind::Bug));
        assert_eq!(WorkItemKind::from_str("task"), Some(WorkItemKind::Task));
        assert_eq!(WorkItemKind::from_str("Task"), Some(WorkItemKind::Task));
        assert_eq!(WorkItemKind::from_str("t"), Some(WorkItemKind::Task));
        assert_eq!(WorkItemKind::from_str("3"), Some(WorkItemKind::Task));
        assert_eq!(WorkItemKind::from_str("invalid"), None);
    }

    #[test]
    fn work_item_kind_enhancement_from_str() {
        assert_eq!(WorkItemKind::from_str("enhancement"), Some(WorkItemKind::Enhancement));
        assert_eq!(WorkItemKind::from_str("Enhancement"), Some(WorkItemKind::Enhancement));
        assert_eq!(WorkItemKind::from_str("e"), Some(WorkItemKind::Enhancement));
        assert_eq!(WorkItemKind::from_str("4"), Some(WorkItemKind::Enhancement));
    }

    #[test]
    fn work_item_kind_enhancement_as_str() {
        assert_eq!(WorkItemKind::Enhancement.as_str(), "Enhancement");
    }

    #[test]
    fn work_item_kind_as_str() {
        assert_eq!(WorkItemKind::Feature.as_str(), "Feature");
        assert_eq!(WorkItemKind::Bug.as_str(), "Bug");
        assert_eq!(WorkItemKind::Task.as_str(), "Task");
        assert_eq!(WorkItemKind::Enhancement.as_str(), "Enhancement");
    }

    #[tokio::test]
    async fn create_file_return_number_returns_correct_number() {
        let tmp = tempfile::TempDir::new().unwrap();
        let root = tmp.path();

        // Set up a git repo and template.
        std::fs::create_dir(root.join(".git")).unwrap();
        let work_items = root.join("aspec/work-items");
        std::fs::create_dir_all(&work_items).unwrap();
        std::fs::write(
            work_items.join("0000-template.md"),
            "# Work Item: [Feature | Bug | Task]\n\nTitle: title\nIssue: issuelink\n",
        )
        .unwrap();
        std::fs::write(work_items.join("0001-first.md"), "").unwrap();

        let (tx, _rx) = unbounded_channel();
        let sink = OutputSink::Channel(tx);

        let number = create_file_return_number(
            &sink,
            WorkItemKind::Enhancement,
            "My Enhancement".to_string(),
            root,
        )
        .await
        .unwrap();

        assert_eq!(number, 2);
        assert!(work_items.join("0002-my-enhancement.md").exists());
    }

    #[test]
    fn find_template_in_git_root() {
        let tmp = TempDir::new().unwrap();
        let work_items = tmp.path().join("aspec/work-items");
        std::fs::create_dir_all(&work_items).unwrap();
        std::fs::write(work_items.join("0000-template.md"), "# template").unwrap();
        let result = find_template(tmp.path());
        assert!(result.is_ok());
        assert!(result.unwrap().ends_with("0000-template.md"));
    }

    #[test]
    fn find_template_missing_returns_error() {
        let tmp = TempDir::new().unwrap();
        let result = find_template(tmp.path());
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Template not found"));
        assert!(err.contains("https://github.com"));
    }

    #[tokio::test]
    async fn run_with_sink_creates_work_item_file() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();

        // Set up a git repo and template.
        std::fs::create_dir(root.join(".git")).unwrap();
        let work_items = root.join("aspec/work-items");
        std::fs::create_dir_all(&work_items).unwrap();
        std::fs::write(
            work_items.join("0000-template.md"),
            "# Work Item: [Feature | Bug | Task]\n\nTitle: title\nIssue: issuelink\n",
        )
        .unwrap();

        // Use channel sink.
        let (tx, mut rx) = unbounded_channel();
        let sink = OutputSink::Channel(tx);

        let result = run_with_sink(
            &sink,
            Some(WorkItemKind::Feature),
            Some("My New Feature".to_string()),
            root,
        )
        .await;

        assert!(result.is_ok(), "run_with_sink failed: {:?}", result.err());

        // Verify file was created.
        let created = work_items.join("0001-my-new-feature.md");
        assert!(created.exists(), "Work item file should exist");

        let content = std::fs::read_to_string(&created).unwrap();
        assert!(content.contains("# Work Item: Feature"));
        assert!(content.contains("Title: My New Feature"));

        // Verify output was sent.
        let messages: Vec<String> = std::iter::from_fn(|| rx.try_recv().ok()).collect();
        assert!(
            messages.iter().any(|m| m.contains("Created work item")),
            "Expected creation message, got: {:?}",
            messages
        );
    }

    #[test]
    fn is_vscode_terminal_when_not_set() {
        // In test environment, TERM_PROGRAM is unlikely to be "vscode".
        // We just verify the function doesn't panic.
        let _ = is_vscode_terminal();
    }

    // ─── resolve_work_item_paths ──────────────────────────────────────────────

    #[test]
    fn resolve_work_item_paths_config_only() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        std::fs::create_dir(root.join(".git")).unwrap();

        let items_dir = root.join("custom/items");
        std::fs::create_dir_all(&items_dir).unwrap();
        let tmpl = items_dir.join("0000-template.md");
        std::fs::write(&tmpl, "# template").unwrap();

        let config = crate::config::RepoConfig {
            work_items: Some(crate::config::WorkItemsConfig {
                dir: Some("custom/items".to_string()),
                template: Some("custom/items/0000-template.md".to_string()),
            }),
            ..Default::default()
        };

        let (dir, template) = resolve_work_item_paths(root, &config);
        assert_eq!(dir, Some(items_dir));
        assert_eq!(template, Some(tmpl));
    }

    #[test]
    fn resolve_work_item_paths_legacy_only() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        std::fs::create_dir(root.join(".git")).unwrap();

        let legacy_dir = root.join("aspec/work-items");
        std::fs::create_dir_all(&legacy_dir).unwrap();
        let legacy_tmpl = legacy_dir.join("0000-template.md");
        std::fs::write(&legacy_tmpl, "# template").unwrap();

        let config = crate::config::RepoConfig::default();
        let (dir, template) = resolve_work_item_paths(root, &config);
        assert_eq!(dir, Some(legacy_dir));
        assert_eq!(template, Some(legacy_tmpl));
    }

    #[test]
    fn resolve_work_item_paths_config_wins_over_legacy() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        std::fs::create_dir(root.join(".git")).unwrap();

        // Legacy dir present.
        let legacy_dir = root.join("aspec/work-items");
        std::fs::create_dir_all(&legacy_dir).unwrap();
        std::fs::write(legacy_dir.join("0000-template.md"), "# legacy").unwrap();

        // Config dir present too.
        let config_dir = root.join("custom/items");
        std::fs::create_dir_all(&config_dir).unwrap();
        let config_tmpl = config_dir.join("my-template.md");
        std::fs::write(&config_tmpl, "# custom").unwrap();

        let config = crate::config::RepoConfig {
            work_items: Some(crate::config::WorkItemsConfig {
                dir: Some("custom/items".to_string()),
                template: Some("custom/items/my-template.md".to_string()),
            }),
            ..Default::default()
        };

        let (dir, template) = resolve_work_item_paths(root, &config);
        assert_eq!(dir, Some(config_dir), "configured dir should win over legacy aspec/work-items");
        assert_eq!(template, Some(config_tmpl), "configured template should win over legacy");
    }

    #[test]
    fn resolve_work_item_paths_neither_present() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        std::fs::create_dir(root.join(".git")).unwrap();

        let config = crate::config::RepoConfig::default();
        let (dir, template) = resolve_work_item_paths(root, &config);
        assert!(dir.is_none(), "dir should be None when neither config nor legacy present");
        assert!(template.is_none(), "template should be None when neither present");
    }

    // ─── discover_template ────────────────────────────────────────────────────

    #[test]
    fn discover_template_returns_none_for_nonexistent_dir() {
        let tmp = TempDir::new().unwrap();
        let nonexistent = tmp.path().join("does-not-exist");
        assert!(
            discover_template(&nonexistent).is_none(),
            "expected None for a directory that does not exist"
        );
    }

    #[test]
    fn discover_template_no_match() {
        let tmp = TempDir::new().unwrap();
        // Put a non-template file in the dir.
        std::fs::write(tmp.path().join("readme.md"), "# readme").unwrap();
        let result = discover_template(tmp.path());
        assert!(result.is_none(), "expected None when no *template.md file");
    }

    #[test]
    fn discover_template_single_match() {
        let tmp = TempDir::new().unwrap();
        let tmpl = tmp.path().join("0000-template.md");
        std::fs::write(&tmpl, "# template").unwrap();
        std::fs::write(tmp.path().join("readme.md"), "# not a template").unwrap();
        let result = discover_template(tmp.path());
        assert_eq!(result, Some(tmpl));
    }

    #[test]
    fn discover_template_multiple_matches_returns_lexicographically_first() {
        let tmp = TempDir::new().unwrap();
        let a = tmp.path().join("aaa-template.md");
        let b = tmp.path().join("bbb-template.md");
        let c = tmp.path().join("zzz-template.md");
        std::fs::write(&a, "# a").unwrap();
        std::fs::write(&b, "# b").unwrap();
        std::fs::write(&c, "# c").unwrap();
        let result = discover_template(tmp.path()).unwrap();
        assert_eq!(result, a, "expected lexicographically first match");
    }

    // ─── integration: run_with_sink with configured work_items.dir ───────────

    #[tokio::test]
    async fn run_with_sink_uses_configured_work_items_dir() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();

        std::fs::create_dir(root.join(".git")).unwrap();

        // Create a custom work items dir (not the legacy aspec/work-items).
        let items_dir = root.join("work");
        std::fs::create_dir_all(&items_dir).unwrap();
        std::fs::write(
            items_dir.join("0000-template.md"),
            "# Work Item: [Feature | Bug | Task]\n\nTitle: title\n",
        )
        .unwrap();

        // Write repo config pointing to the custom dir.
        let config = crate::config::RepoConfig {
            work_items: Some(crate::config::WorkItemsConfig {
                dir: Some("work".to_string()),
                template: Some("work/0000-template.md".to_string()),
            }),
            ..Default::default()
        };
        crate::config::save_repo_config(root, &config).unwrap();

        let (tx, _rx) = unbounded_channel();
        let sink = OutputSink::Channel(tx);

        let result = run_with_sink(
            &sink,
            Some(WorkItemKind::Task),
            Some("Custom Dir Task".to_string()),
            root,
        )
        .await;

        assert!(result.is_ok(), "run_with_sink failed: {:?}", result.err());

        // File must be in the configured dir, not in aspec/work-items.
        let created = items_dir.join("0001-custom-dir-task.md");
        assert!(created.exists(), "work item should be in configured dir: {}", created.display());
        assert!(
            !root.join("aspec/work-items").exists(),
            "legacy aspec/work-items should not be created"
        );
    }

    // ─── missing configured template → warning ────────────────────────────────

    #[tokio::test]
    async fn run_with_sink_warns_when_configured_template_missing() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();

        std::fs::create_dir(root.join(".git")).unwrap();

        let items_dir = root.join("items");
        std::fs::create_dir_all(&items_dir).unwrap();

        // Template is configured but the file does not exist.
        let config = crate::config::RepoConfig {
            work_items: Some(crate::config::WorkItemsConfig {
                dir: Some("items".to_string()),
                template: Some("items/nonexistent-template.md".to_string()),
            }),
            ..Default::default()
        };
        crate::config::save_repo_config(root, &config).unwrap();

        let (tx, mut rx) = unbounded_channel();
        let sink = OutputSink::Channel(tx);

        let result = run_with_sink(
            &sink,
            Some(WorkItemKind::Bug),
            Some("Test Bug".to_string()),
            root,
        )
        .await;

        assert!(result.is_ok(), "run_with_sink should succeed: {:?}", result.err());

        let messages: Vec<String> = std::iter::from_fn(|| rx.try_recv().ok()).collect();
        let output = messages.join("\n");
        assert!(
            output.contains("not found") || output.contains("falling back"),
            "expected template-missing warning; got: {}",
            output
        );
    }

    // ─── auto-discovery with MockInput ────────────────────────────────────────

    #[tokio::test]
    async fn run_with_sink_auto_discovery_confirm_uses_template_and_saves_config() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();

        std::fs::create_dir(root.join(".git")).unwrap();

        let items_dir = root.join("items");
        std::fs::create_dir_all(&items_dir).unwrap();
        std::fs::write(
            items_dir.join("0000-template.md"),
            "# Work Item: [Feature | Bug | Task]\n\nTitle: title\nIssue: issuelink\n",
        )
        .unwrap();

        // No configured template → auto-discovery should find the file above.
        let config = crate::config::RepoConfig {
            work_items: Some(crate::config::WorkItemsConfig {
                dir: Some("items".to_string()),
                template: None,
            }),
            ..Default::default()
        };
        crate::config::save_repo_config(root, &config).unwrap();

        // MockInput: "y" → confirm the discovery prompt.
        let (tx, mut rx) = unbounded_channel();
        let sink = OutputSink::mock_input(tx, vec!["y"]);

        let result = run_with_sink(
            &sink,
            Some(WorkItemKind::Feature),
            Some("Discovered Feature".to_string()),
            root,
        )
        .await;

        assert!(result.is_ok(), "run_with_sink failed: {:?}", result.err());

        // File should be created using full template content.
        let created = items_dir.join("0001-discovered-feature.md");
        assert!(created.exists(), "work item file should exist");
        let content = std::fs::read_to_string(&created).unwrap();
        assert!(content.contains("# Work Item: Feature"), "template should be applied");
        assert!(content.contains("Title: Discovered Feature"));

        // Config should have the template path saved.
        let updated = crate::config::load_repo_config(root).unwrap();
        let saved_tmpl = updated.work_items.as_ref().and_then(|w| w.template.as_deref());
        assert!(
            saved_tmpl.is_some(),
            "template path should be saved to config after confirming"
        );
        assert!(
            saved_tmpl.unwrap().contains("template.md"),
            "saved path should reference template.md; got {:?}",
            saved_tmpl
        );

        // Output should contain the discovery prompt.
        let messages: Vec<String> = std::iter::from_fn(|| rx.try_recv().ok()).collect();
        let output = messages.join("\n");
        assert!(
            output.contains("Found potential template"),
            "discovery prompt should appear in output; got: {}",
            output
        );
    }

    #[tokio::test]
    async fn run_with_sink_auto_discovery_decline_creates_minimal_stub() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();

        std::fs::create_dir(root.join(".git")).unwrap();

        let items_dir = root.join("items");
        std::fs::create_dir_all(&items_dir).unwrap();
        std::fs::write(
            items_dir.join("0000-template.md"),
            "# Work Item: [Feature | Bug | Task]\n\n## Summary:\n- summary\n",
        )
        .unwrap();

        let config = crate::config::RepoConfig {
            work_items: Some(crate::config::WorkItemsConfig {
                dir: Some("items".to_string()),
                template: None,
            }),
            ..Default::default()
        };
        crate::config::save_repo_config(root, &config).unwrap();

        // MockInput: "n" → decline the discovery prompt.
        let (tx, _rx) = unbounded_channel();
        let sink = OutputSink::mock_input(tx, vec!["n"]);

        let result = run_with_sink(
            &sink,
            Some(WorkItemKind::Bug),
            Some("Declined Bug".to_string()),
            root,
        )
        .await;

        assert!(result.is_ok(), "run_with_sink failed: {:?}", result.err());

        let created = items_dir.join("0001-declined-bug.md");
        assert!(created.exists(), "work item file should exist");
        let content = std::fs::read_to_string(&created).unwrap();
        assert!(
            content.contains("# Bug: Declined Bug"),
            "expected minimal stub; got: {}",
            content
        );
        assert!(
            !content.contains("## Summary"),
            "template should NOT be applied when declined"
        );

        // Config should NOT have a template path saved.
        let updated = crate::config::load_repo_config(root).unwrap();
        let saved_tmpl = updated.work_items.as_ref().and_then(|w| w.template.as_deref());
        assert!(saved_tmpl.is_none(), "template should not be saved when declined");
    }

    #[tokio::test]
    async fn run_with_sink_auto_discovery_shows_count_for_multiple_templates() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();

        std::fs::create_dir(root.join(".git")).unwrap();

        let items_dir = root.join("items");
        std::fs::create_dir_all(&items_dir).unwrap();
        // Two template files.
        std::fs::write(
            items_dir.join("aaa-template.md"),
            "# Work Item: [Feature | Bug | Task]\n\nTitle: title\n",
        )
        .unwrap();
        std::fs::write(items_dir.join("bbb-template.md"), "# template b").unwrap();

        let config = crate::config::RepoConfig {
            work_items: Some(crate::config::WorkItemsConfig {
                dir: Some("items".to_string()),
                template: None,
            }),
            ..Default::default()
        };
        crate::config::save_repo_config(root, &config).unwrap();

        // Decline so we don't need to check template content.
        let (tx, mut rx) = unbounded_channel();
        let sink = OutputSink::mock_input(tx, vec!["n"]);

        let _ = run_with_sink(
            &sink,
            Some(WorkItemKind::Task),
            Some("Count Test".to_string()),
            root,
        )
        .await;

        let messages: Vec<String> = std::iter::from_fn(|| rx.try_recv().ok()).collect();
        let output = messages.join("\n");
        assert!(
            output.contains("2") && output.contains("template"),
            "expected candidate count in output; got: {}",
            output
        );
        // The lexicographically first template (aaa-...) should be offered.
        assert!(
            output.contains("aaa-template.md"),
            "expected first template to be offered; got: {}",
            output
        );
    }

    #[tokio::test]
    async fn run_with_sink_no_template_channel_creates_minimal_stub() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();

        std::fs::create_dir(root.join(".git")).unwrap();

        // Create work items dir but no template file.
        let items_dir = root.join("items");
        std::fs::create_dir_all(&items_dir).unwrap();

        let config = crate::config::RepoConfig {
            work_items: Some(crate::config::WorkItemsConfig {
                dir: Some("items".to_string()),
                template: None,
            }),
            ..Default::default()
        };
        crate::config::save_repo_config(root, &config).unwrap();

        // Channel sink → supports_color() is false → auto-discovery skipped, minimal stub used.
        let (tx, _rx) = unbounded_channel();
        let sink = OutputSink::Channel(tx);

        let result = run_with_sink(
            &sink,
            Some(WorkItemKind::Bug),
            Some("Test Bug".to_string()),
            root,
        )
        .await;

        assert!(result.is_ok(), "run_with_sink failed: {:?}", result.err());

        let created = items_dir.join("0001-test-bug.md");
        assert!(created.exists(), "work item file should exist");
        let content = std::fs::read_to_string(&created).unwrap();
        assert!(
            content.contains("# Bug: Test Bug"),
            "expected minimal stub content, got: {}",
            content
        );
    }
}
