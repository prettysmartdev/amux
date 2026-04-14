/// Integration tests for the amux CLI binary.
///
/// These tests invoke the compiled binary to validate end-to-end behaviour
/// across multiple components.
use std::process::Command;

fn amux() -> Command {
    Command::new(env!("CARGO_BIN_EXE_amux"))
}

#[test]
fn help_exits_successfully() {
    let output = amux().arg("--help").output().expect("failed to run amux");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("amux"));
}

#[test]
fn version_exits_successfully() {
    let output = amux().arg("--version").output().expect("failed to run amux");
    assert!(output.status.success());
}

#[test]
fn implement_missing_work_item_prints_error() {
    let output = amux()
        .args(["implement", "9999"])
        .output()
        .expect("failed to run amux");
    // Should fail (non-zero exit) because work item 9999 does not exist.
    assert!(!output.status.success());
}

#[test]
fn implement_accepts_four_digit_work_item() {
    let output = amux()
        .args(["implement", "0099"])
        .output()
        .expect("failed to run amux");
    // Should fail because work item 0099 doesn't exist, but the input should be accepted.
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    // Should report the work item is missing, not an invalid number error.
    assert!(
        stderr.contains("work item") || stderr.contains("0099") || stderr.contains("99"),
        "Expected work-item-not-found error, got: {}",
        stderr
    );
}

#[test]
fn ready_help_shows_refresh_flag() {
    let output = amux()
        .args(["ready", "--help"])
        .output()
        .expect("failed to run amux");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("--refresh"),
        "ready --help should mention --refresh flag"
    );
}

#[test]
fn ready_help_shows_non_interactive_flag() {
    let output = amux()
        .args(["ready", "--help"])
        .output()
        .expect("failed to run amux");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("--non-interactive"),
        "ready --help should mention --non-interactive flag"
    );
}

#[test]
fn implement_help_shows_non_interactive_flag() {
    let output = amux()
        .args(["implement", "--help"])
        .output()
        .expect("failed to run amux");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("--non-interactive"),
        "implement --help should mention --non-interactive flag"
    );
}

#[test]
fn new_help_shows_subcommand() {
    let output = amux()
        .args(["specs", "--help"])
        .output()
        .expect("failed to run amux");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("new"),
        "specs --help should mention 'new' subcommand"
    );
}

#[test]
fn chat_help_shows_subcommand() {
    let output = amux()
        .args(["--help"])
        .output()
        .expect("failed to run amux");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("chat"),
        "help should mention 'chat' subcommand"
    );
}

#[test]
fn chat_help_shows_non_interactive_flag() {
    let output = amux()
        .args(["chat", "--help"])
        .output()
        .expect("failed to run amux");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("--non-interactive"),
        "chat --help should mention --non-interactive flag"
    );
}

// ── config integration tests ──────────────────────────────────────────────────

use std::path::Path;
use tempfile::TempDir;

/// Build an `amux` Command with a controlled HOME directory so tests do not
/// touch the developer's real `~/.amux/config.json`.
fn amux_with_home(home: &Path) -> Command {
    let mut cmd = amux();
    cmd.env("HOME", home);
    cmd
}

/// Initialize a fresh git repo in a temp directory and return it.
fn make_git_repo() -> TempDir {
    let repo = TempDir::new().expect("TempDir::new");
    std::process::Command::new("git")
        .args(["init", "--quiet"])
        .current_dir(repo.path())
        .status()
        .expect("git init");
    repo
}

/// Write a JSON string to `<dir>/.amux/config.json`, creating dirs as needed.
fn write_repo_config(dir: &Path, json: &str) {
    let config_dir = dir.join(".amux");
    std::fs::create_dir_all(&config_dir).unwrap();
    std::fs::write(config_dir.join("config.json"), json).unwrap();
}

/// Write a JSON string to `<home>/.amux/config.json`, creating dirs as needed.
fn write_global_config(home: &Path, json: &str) {
    let config_dir = home.join(".amux");
    std::fs::create_dir_all(&config_dir).unwrap();
    std::fs::write(config_dir.join("config.json"), json).unwrap();
}

// 1. config show — only global config present
#[test]
fn config_show_only_global_config() {
    let home = TempDir::new().unwrap();
    let repo = make_git_repo();
    write_global_config(home.path(), r#"{"default_agent":"gemini"}"#);

    let output = amux_with_home(home.path())
        .current_dir(repo.path())
        .args(["config", "show"])
        .output()
        .unwrap();

    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("gemini"),
        "global default_agent should appear; stdout: {}",
        stdout
    );
    assert!(
        stdout.contains("(not set)"),
        "repo column should show '(not set)' for shared fields; stdout: {}",
        stdout
    );
}

// 2. config show — only repo config present
#[test]
fn config_show_only_repo_config() {
    let home = TempDir::new().unwrap();
    let repo = make_git_repo();
    write_repo_config(repo.path(), r#"{"agent":"codex"}"#);

    let output = amux_with_home(home.path())
        .current_dir(repo.path())
        .args(["config", "show"])
        .output()
        .unwrap();

    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("codex"),
        "repo agent should appear; stdout: {}",
        stdout
    );
    assert!(
        stdout.contains("(built-in)"),
        "global column should show built-in defaults for unset global fields; stdout: {}",
        stdout
    );
}

// 3. config show — both configs set for terminal_scrollback_lines → Override = yes
#[test]
fn config_show_override_column_shows_yes_when_both_set_and_differ() {
    let home = TempDir::new().unwrap();
    let repo = make_git_repo();
    write_global_config(home.path(), r#"{"terminal_scrollback_lines": 10000}"#);
    write_repo_config(repo.path(), r#"{"terminal_scrollback_lines": 5000}"#);

    let output = amux_with_home(home.path())
        .current_dir(repo.path())
        .args(["config", "show"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("yes"),
        "Override column should show 'yes' for terminal_scrollback_lines; stdout: {}",
        stdout
    );
}

// 4. config show — outside a git repo
#[test]
fn config_show_outside_git_repo_succeeds_with_note() {
    let home = TempDir::new().unwrap();
    // Use a fresh temp dir that has NOT been git-initialized.
    let not_a_repo = TempDir::new().unwrap();

    let output = amux_with_home(home.path())
        .current_dir(not_a_repo.path())
        .args(["config", "show"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "config show must exit 0 outside a git repo; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("not inside a git repo") || stderr.contains("repo config is unavailable"),
        "should print a note about unavailable repo config; stderr: {}",
        stderr
    );
}

// 5. config get — repo overrides global annotation
#[test]
fn config_get_shows_repo_overrides_global_annotation() {
    let home = TempDir::new().unwrap();
    let repo = make_git_repo();
    // Global is at built-in (not set), repo = 5000.
    write_repo_config(repo.path(), r#"{"terminal_scrollback_lines": 5000}"#);

    let output = amux_with_home(home.path())
        .current_dir(repo.path())
        .args(["config", "get", "terminal_scrollback_lines"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("5000"),
        "effective value should be 5000; stdout: {}",
        stdout
    );
    assert!(
        stdout.contains("← repo overrides global"),
        "should annotate repo override; stdout: {}",
        stdout
    );
}

// 6. config get — neither set shows built-in default
#[test]
fn config_get_neither_set_shows_builtin_default() {
    let home = TempDir::new().unwrap();
    let repo = make_git_repo();

    let output = amux_with_home(home.path())
        .current_dir(repo.path())
        .args(["config", "get", "terminal_scrollback_lines"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    // Built-in default is 10000; should appear in Global, Repo shows (not set), Effective shows 10000.
    assert!(
        stdout.contains("10000"),
        "built-in default 10000 should appear; stdout: {}",
        stdout
    );
    assert!(
        !stdout.contains("← repo overrides"),
        "no override annotation expected; stdout: {}",
        stdout
    );
}

// 7. config set agent codex — round trip
#[test]
fn config_set_agent_writes_repo_config_and_get_returns_it() {
    let home = TempDir::new().unwrap();
    let repo = make_git_repo();

    let set_out = amux_with_home(home.path())
        .current_dir(repo.path())
        .args(["config", "set", "agent", "codex"])
        .output()
        .unwrap();
    assert!(
        set_out.status.success(),
        "config set should succeed; stderr: {}",
        String::from_utf8_lossy(&set_out.stderr)
    );

    // Verify the written JSON.
    let config_path = repo.path().join(".amux").join("config.json");
    let json = std::fs::read_to_string(&config_path).unwrap();
    assert!(
        json.contains(r#""agent""#) && json.contains("codex"),
        "config.json should contain agent=codex; got: {}",
        json
    );

    // config get should show the new value.
    let get_out = amux_with_home(home.path())
        .current_dir(repo.path())
        .args(["config", "get", "agent"])
        .output()
        .unwrap();
    assert!(get_out.status.success());
    let stdout = String::from_utf8_lossy(&get_out.stdout);
    assert!(stdout.contains("codex"), "config get agent should show codex; stdout: {}", stdout);
}

// 8. config set --global default_agent gemini — writes to global config
#[test]
fn config_set_global_default_agent_writes_to_global_config() {
    let home = TempDir::new().unwrap();
    let repo = make_git_repo();

    let output = amux_with_home(home.path())
        .current_dir(repo.path())
        .args(["config", "set", "--global", "default_agent", "gemini"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "config set --global should succeed; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let config_path = home.path().join(".amux").join("config.json");
    let json = std::fs::read_to_string(&config_path).unwrap();
    assert!(
        json.contains("default_agent") && json.contains("gemini"),
        "global config.json should contain default_agent=gemini; got: {}",
        json
    );
}

// 9. config set agent unknown_agent — exits non-zero, no file created
#[test]
fn config_set_invalid_agent_value_exits_nonzero_and_does_not_write() {
    let home = TempDir::new().unwrap();
    let repo = make_git_repo();

    let output = amux_with_home(home.path())
        .current_dir(repo.path())
        .args(["config", "set", "agent", "unknown_agent"])
        .output()
        .unwrap();

    assert!(
        !output.status.success(),
        "invalid agent value should exit non-zero"
    );
    assert!(
        !repo.path().join(".amux").join("config.json").exists(),
        "config file must not be created after a failed set"
    );
}

// 10. config set auto_agent_auth_accepted — exits non-zero, no file created
#[test]
fn config_set_auto_agent_auth_accepted_exits_nonzero() {
    let home = TempDir::new().unwrap();
    let repo = make_git_repo();

    let output = amux_with_home(home.path())
        .current_dir(repo.path())
        .args(["config", "set", "auto_agent_auth_accepted", "true"])
        .output()
        .unwrap();

    assert!(
        !output.status.success(),
        "setting auto_agent_auth_accepted should exit non-zero; stdout: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(
        !repo.path().join(".amux").join("config.json").exists(),
        "config file must not be created after a rejected set"
    );
}

// 11. config set --global runtime apple-containers on non-macOS emits a platform warning
#[cfg(not(target_os = "macos"))]
#[test]
fn config_set_global_runtime_apple_containers_warns_on_non_macos() {
    let home = TempDir::new().unwrap();
    let repo = make_git_repo();

    let output = amux_with_home(home.path())
        .current_dir(repo.path())
        .args(["config", "set", "--global", "runtime", "apple-containers"])
        .output()
        .unwrap();

    // Should still succeed (value is written).
    assert!(
        output.status.success(),
        "should exit 0 even with platform warning; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Warning") && stderr.contains("apple-containers"),
        "should emit a platform warning on non-macOS; stderr: {}",
        stderr
    );
    // Verify value was still written.
    let config_path = home.path().join(".amux").join("config.json");
    let json = std::fs::read_to_string(&config_path).unwrap();
    assert!(
        json.contains("apple-containers"),
        "value should still be written despite warning; got: {}",
        json
    );
}

// 12. config set --global default_agent — warns when repo already overrides via `agent`
#[test]
fn config_set_global_default_agent_warns_when_repo_already_sets_agent() {
    let home = TempDir::new().unwrap();
    let repo = make_git_repo();
    // Pre-populate repo config with agent=codex.
    write_repo_config(repo.path(), r#"{"agent":"codex"}"#);

    let output = amux_with_home(home.path())
        .current_dir(repo.path())
        .args(["config", "set", "--global", "default_agent", "gemini"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Warning") && stderr.contains("repo config overrides"),
        "should warn that repo overrides the new global value; stderr: {}",
        stderr
    );
}

// 13. config set env_passthrough "" — writes envPassthrough: [] (empty array, not omitted)
#[test]
fn config_set_env_passthrough_empty_string_writes_empty_array() {
    let home = TempDir::new().unwrap();
    let repo = make_git_repo();

    let output = amux_with_home(home.path())
        .current_dir(repo.path())
        .args(["config", "set", "env_passthrough", ""])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "config set env_passthrough '' should succeed; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let config_path = repo.path().join(".amux").join("config.json");
    let json = std::fs::read_to_string(&config_path).unwrap();
    assert!(
        json.contains("envPassthrough") && json.contains("[]"),
        "JSON must contain envPassthrough: [] (not omitted); got: {}",
        json
    );
}

// Round-trip: set → get → show
#[test]
fn config_round_trip_set_get_show() {
    let home = TempDir::new().unwrap();
    let repo = make_git_repo();

    // Set terminal_scrollback_lines to 7777 at repo level.
    let set_out = amux_with_home(home.path())
        .current_dir(repo.path())
        .args(["config", "set", "terminal_scrollback_lines", "7777"])
        .output()
        .unwrap();
    assert!(set_out.status.success(), "set failed: {}", String::from_utf8_lossy(&set_out.stderr));

    // get should reflect the new value.
    let get_out = amux_with_home(home.path())
        .current_dir(repo.path())
        .args(["config", "get", "terminal_scrollback_lines"])
        .output()
        .unwrap();
    assert!(get_out.status.success());
    assert!(
        String::from_utf8_lossy(&get_out.stdout).contains("7777"),
        "get should return 7777"
    );

    // show should reflect the new value.
    let show_out = amux_with_home(home.path())
        .current_dir(repo.path())
        .args(["config", "show"])
        .output()
        .unwrap();
    assert!(show_out.status.success());
    assert!(
        String::from_utf8_lossy(&show_out.stdout).contains("7777"),
        "show should display 7777"
    );
}

// ── --agent flag integration tests (work item 0049) ──────────────────────────

#[test]
fn chat_help_shows_agent_flag() {
    let output = amux()
        .args(["chat", "--help"])
        .output()
        .expect("failed to run amux");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("--agent"),
        "chat --help should mention --agent flag; got: {}",
        stdout
    );
}

#[test]
fn chat_unknown_agent_exits_nonzero_with_error() {
    let output = amux()
        .args(["chat", "--agent", "unknown"])
        .output()
        .expect("failed to run amux");
    assert!(
        !output.status.success(),
        "chat --agent unknown should exit non-zero"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("unknown") || stderr.contains("available agents"),
        "stderr should describe the unknown agent error; got: {}",
        stderr
    );
}

#[test]
fn implement_help_shows_agent_flag() {
    let output = amux()
        .args(["implement", "--help"])
        .output()
        .expect("failed to run amux");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("--agent"),
        "implement --help should mention --agent flag; got: {}",
        stdout
    );
}
