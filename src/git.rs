use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Verify that `git` is installed and version >= 2.5 (worktree support).
pub fn git_version_check() -> Result<()> {
    let output = Command::new("git")
        .args(["--version"])
        .output()
        .context("Failed to invoke `git --version`")?;
    let version_str = String::from_utf8_lossy(&output.stdout);
    // Parse "git version X.Y.Z"
    if let Some(ver) = version_str.trim().strip_prefix("git version ") {
        let parts: Vec<&str> = ver.split('.').collect();
        if let (Some(major), Some(minor)) = (
            parts.first().and_then(|s| s.parse::<u32>().ok()),
            parts.get(1).and_then(|s| s.parse::<u32>().ok()),
        ) {
            if major > 2 || (major == 2 && minor >= 5) {
                return Ok(());
            }
            bail!(
                "git >= 2.5 is required for --worktree support (found: {})",
                ver
            );
        }
    }
    bail!("Could not parse git version from: {}", version_str.trim())
}

/// Returns `~/.amux/worktrees/<repo-name>/<NNNN>/`.
///
/// `<repo-name>` is derived from the last path component of `git_root`.
pub fn worktree_path(git_root: &Path, work_item: u32) -> Result<PathBuf> {
    let home = dirs::home_dir().context("Cannot resolve home directory")?;
    let repo_name = git_root
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("repo");
    Ok(home
        .join(".amux")
        .join("worktrees")
        .join(repo_name)
        .join(format!("{:04}", work_item)))
}

/// Returns the deterministic branch name for a worktree: `"amux/work-item-NNNN"`.
pub fn worktree_branch_name(work_item: u32) -> String {
    format!("amux/work-item-{:04}", work_item)
}

/// Returns `~/.amux/worktrees/<repo-name>/wf-<name>/`.
///
/// Used by `exec workflow` when no `--work-item` is provided, so each distinct
/// workflow file gets its own worktree path rather than all sharing `0000`.
pub fn worktree_path_named(git_root: &Path, name: &str) -> Result<PathBuf> {
    let home = dirs::home_dir().context("Cannot resolve home directory")?;
    let repo_name = git_root
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("repo");
    Ok(home
        .join(".amux")
        .join("worktrees")
        .join(repo_name)
        .join(format!("wf-{}", name)))
}

/// Returns the deterministic branch name for a workflow worktree: `"amux/workflow-<name>"`.
pub fn worktree_branch_name_for_workflow(name: &str) -> String {
    format!("amux/workflow-{}", name)
}

/// Returns `true` if the branch exists in `git_root`.
pub fn branch_exists(git_root: &Path, branch: &str) -> bool {
    Command::new("git")
        .args(["rev-parse", "--verify", &format!("refs/heads/{}", branch)])
        .current_dir(git_root)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Returns `true` if `git_root` is in detached HEAD state.
pub fn is_detached_head(git_root: &Path) -> bool {
    !Command::new("git")
        .args(["symbolic-ref", "--quiet", "HEAD"])
        .current_dir(git_root)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Create a new Git worktree at `worktree_path` on `branch`.
///
/// - If `branch` does not exist: `git worktree add <path> -b <branch>`
/// - If `branch` already exists (no worktree dir): `git worktree add <path> <branch>`
///
/// The caller must ensure `worktree_path` does not already exist.
pub fn create_worktree(git_root: &Path, worktree_path: &Path, branch: &str) -> Result<()> {
    std::fs::create_dir_all(worktree_path.parent().unwrap_or(worktree_path))
        .context("Failed to create worktree parent directory")?;

    let wt_str = worktree_path.to_str().unwrap();
    let args: Vec<&str> = if branch_exists(git_root, branch) {
        vec!["worktree", "add", wt_str, branch]
    } else {
        vec!["worktree", "add", wt_str, "-b", branch]
    };

    let output = Command::new("git")
        .args(&args)
        .current_dir(git_root)
        .output()
        .context("Failed to invoke `git worktree add`")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("`git worktree add` failed: {}", stderr.trim());
    }
    Ok(())
}

/// Remove the worktree at `worktree_path` using `git worktree remove --force`.
pub fn remove_worktree(git_root: &Path, worktree_path: &Path) -> Result<()> {
    let wt_str = worktree_path.to_str().unwrap();
    let output = Command::new("git")
        .args(["worktree", "remove", "--force", wt_str])
        .current_dir(git_root)
        .output()
        .context("Failed to invoke `git worktree remove`")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("`git worktree remove` failed: {}", stderr.trim());
    }
    Ok(())
}

/// Squash-merge `branch` into the current branch in `git_root` and create a single commit.
///
/// Uses `git merge --squash` to stage all changes from `branch` without preserving its
/// commit history, then commits them as one dedicated commit.
pub fn merge_branch(git_root: &Path, branch: &str) -> Result<()> {
    let output = Command::new("git")
        .args(["merge", "--squash", branch])
        .current_dir(git_root)
        .output()
        .context("Failed to invoke `git merge --squash`")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("`git merge --squash` failed: {}", stderr.trim());
    }

    let message = format!("Implement {}", branch);
    let output = Command::new("git")
        .args(["commit", "-m", &message])
        .current_dir(git_root)
        .output()
        .context("Failed to invoke `git commit`")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("`git commit` failed: {}", stderr.trim());
    }
    Ok(())
}

/// Stage all changes with `git add -A` and commit them with the given message.
pub fn commit_all(path: &Path, message: &str) -> Result<()> {
    let add_output = Command::new("git")
        .args(["add", "-A"])
        .current_dir(path)
        .output()
        .context("Failed to invoke `git add -A`")?;
    if !add_output.status.success() {
        let stderr = String::from_utf8_lossy(&add_output.stderr);
        bail!("`git add -A` failed: {}", stderr.trim());
    }

    let commit_output = Command::new("git")
        .args(["commit", "-m", message])
        .current_dir(path)
        .output()
        .context("Failed to invoke `git commit`")?;
    if !commit_output.status.success() {
        let stderr = String::from_utf8_lossy(&commit_output.stderr);
        bail!("`git commit` failed: {}", stderr.trim());
    }
    Ok(())
}

/// Returns a list of uncommitted file status lines in the given worktree path.
///
/// Runs `git status --porcelain` and returns each non-empty line (e.g. `" M src/foo.rs"`).
/// Returns an empty `Vec` when the worktree is clean.
pub fn uncommitted_files(worktree_path: &Path) -> Result<Vec<String>> {
    let output = Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(worktree_path)
        .output()
        .context("Failed to invoke `git status --porcelain`")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("`git status --porcelain` failed: {}", stderr.trim());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let files: Vec<String> = stdout
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.to_string())
        .collect();

    Ok(files)
}

/// Force-delete a local branch using `git branch -D`.
///
/// `-D` is required after a squash merge because git does not consider the branch
/// "fully merged" (there is no merge commit pointing back to it).
pub fn delete_branch(git_root: &Path, branch: &str) -> Result<()> {
    let output = Command::new("git")
        .args(["branch", "-D", branch])
        .current_dir(git_root)
        .output()
        .context("Failed to invoke `git branch -D`")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("`git branch -D` failed: {}", stderr.trim());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worktree_path_returns_correct_structure() {
        let git_root = Path::new("/home/user/myrepo");
        let path = worktree_path(git_root, 1).unwrap();
        let home = dirs::home_dir().unwrap();
        let expected = home
            .join(".amux")
            .join("worktrees")
            .join("myrepo")
            .join("0001");
        assert_eq!(path, expected);
    }

    #[test]
    fn worktree_path_pads_work_item_to_four_digits() {
        let git_root = Path::new("/some/repo");
        let path42 = worktree_path(git_root, 42).unwrap();
        assert_eq!(path42.file_name().unwrap().to_str().unwrap(), "0042");

        let path7 = worktree_path(git_root, 7).unwrap();
        assert_eq!(path7.file_name().unwrap().to_str().unwrap(), "0007");

        let path9999 = worktree_path(git_root, 9999).unwrap();
        assert_eq!(path9999.file_name().unwrap().to_str().unwrap(), "9999");
    }

    #[test]
    fn worktree_path_uses_repo_name_from_git_root() {
        let git_root = Path::new("/projects/awesome-app");
        let path = worktree_path(git_root, 1).unwrap();
        // The component just before NNNN should be the repo name.
        let parent = path.parent().unwrap();
        let repo_component = parent.file_name().unwrap().to_str().unwrap();
        assert_eq!(repo_component, "awesome-app");
    }

    #[test]
    fn worktree_branch_name_formats_correctly() {
        assert_eq!(worktree_branch_name(1), "amux/work-item-0001");
        assert_eq!(worktree_branch_name(42), "amux/work-item-0042");
        assert_eq!(worktree_branch_name(100), "amux/work-item-0100");
        assert_eq!(worktree_branch_name(9999), "amux/work-item-9999");
    }

    #[test]
    fn worktree_branch_name_prefix_is_amux_slash() {
        let name = worktree_branch_name(30);
        assert!(name.starts_with("amux/work-item-"), "Expected 'amux/work-item-' prefix, got: {}", name);
    }

    // ── worktree_path_named / worktree_branch_name_for_workflow (work item 0058) ──

    #[test]
    fn worktree_path_named_uses_wf_prefix_and_name() {
        let git_root = Path::new("/home/user/myrepo");
        let path = worktree_path_named(git_root, "implement-feature").unwrap();
        let home = dirs::home_dir().unwrap();
        let expected = home
            .join(".amux")
            .join("worktrees")
            .join("myrepo")
            .join("wf-implement-feature");
        assert_eq!(path, expected);
    }

    #[test]
    fn worktree_path_named_differs_from_work_item_path() {
        // Ensure no collision between a named workflow worktree and a work-item worktree.
        let git_root = Path::new("/some/repo");
        let named = worktree_path_named(git_root, "my-workflow").unwrap();
        let numbered = worktree_path(git_root, 0).unwrap(); // the old "0000" sentinel
        assert_ne!(
            named, numbered,
            "named worktree path must not collide with work-item-0000 path"
        );
    }

    #[test]
    fn worktree_path_named_uses_repo_name_from_git_root() {
        let git_root = Path::new("/projects/my-proj");
        let path = worktree_path_named(git_root, "wf").unwrap();
        let parent = path.parent().unwrap();
        let repo_component = parent.file_name().unwrap().to_str().unwrap();
        assert_eq!(repo_component, "my-proj");
    }

    #[test]
    fn worktree_branch_name_for_workflow_formats_correctly() {
        assert_eq!(
            worktree_branch_name_for_workflow("implement-feature"),
            "amux/workflow-implement-feature"
        );
        assert_eq!(worktree_branch_name_for_workflow("wf"), "amux/workflow-wf");
    }

    #[test]
    fn worktree_branch_name_for_workflow_differs_from_work_item_branch() {
        // A workflow branch must not collide with a work-item-0000 branch.
        let wf_branch = worktree_branch_name_for_workflow("workflow");
        let wi_branch = worktree_branch_name(0);
        assert_ne!(wf_branch, wi_branch);
    }

    #[test]
    fn create_worktree_errors_gracefully_on_non_git_dir() {
        let tmp = tempfile::TempDir::new().unwrap();
        // tmp is NOT a git repository — `git worktree add` should fail.
        let wt_path = tmp.path().join("worktree-out");
        let result = create_worktree(tmp.path(), &wt_path, "amux/work-item-0001");
        assert!(
            result.is_err(),
            "Expected create_worktree to return an error when git_root is not a git repo"
        );
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("worktree add") || msg.contains("git"),
            "Error message should mention worktree or git, got: {}",
            msg
        );
    }
}
