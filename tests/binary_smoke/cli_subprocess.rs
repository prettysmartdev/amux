//! Binary smoke tests — invokes the compiled `amux` binary as a subprocess.

use std::process::Command;

/// Path to the compiled `amux` binary. Cargo sets this for integration tests.
fn amux_bin() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_BIN_EXE_amux"))
}

fn run_amux(args: &[&str]) -> std::process::Output {
    Command::new(amux_bin())
        .args(args)
        .output()
        .expect("failed to run amux binary")
}

// ─── Help flags ───────────────────────────────────────────────────────────────

#[test]
fn amux_help_exits_zero() {
    let out = run_amux(&["--help"]);
    assert!(
        out.status.success(),
        "`amux --help` should exit 0; got {:?}\nstderr: {}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn amux_help_stdout_mentions_amux() {
    let out = run_amux(&["--help"]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("amux"),
        "`amux --help` stdout should mention 'amux'; got:\n{stdout}"
    );
}

#[test]
fn amux_version_flag_exits_zero_or_one() {
    // --version may not be defined; exit code 0 (version printed) or 2 (unrecognised flag)
    // are both acceptable; the test just ensures the binary runs.
    let out = run_amux(&["--version"]);
    let code = out.status.code().unwrap_or(0);
    assert!(
        code == 0 || code == 1 || code == 2,
        "unexpected exit code {code} for --version"
    );
}

// ─── Subcommand help ──────────────────────────────────────────────────────────

#[test]
fn amux_init_help_exits_zero() {
    let out = run_amux(&["init", "--help"]);
    assert!(
        out.status.success(),
        "`amux init --help` should exit 0; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn amux_ready_help_exits_zero() {
    let out = run_amux(&["ready", "--help"]);
    assert!(out.status.success());
}

#[test]
fn amux_exec_help_exits_zero() {
    let out = run_amux(&["exec", "--help"]);
    assert!(out.status.success());
}

#[test]
fn amux_headless_help_exits_zero() {
    let out = run_amux(&["headless", "--help"]);
    assert!(out.status.success());
}

#[test]
fn amux_status_help_exits_zero() {
    let out = run_amux(&["status", "--help"]);
    assert!(out.status.success());
}

#[test]
fn amux_config_help_exits_zero() {
    let out = run_amux(&["config", "--help"]);
    assert!(out.status.success());
}

#[test]
fn amux_new_help_exits_zero() {
    let out = run_amux(&["new", "--help"]);
    assert!(out.status.success());
}

#[test]
fn amux_remote_help_exits_zero() {
    let out = run_amux(&["remote", "--help"]);
    assert!(out.status.success());
}

// ─── Unknown command error handling ──────────────────────────────────────────

#[test]
fn amux_unknown_subcommand_exits_nonzero() {
    let out = run_amux(&["definitely-not-a-command"]);
    assert!(
        !out.status.success(),
        "amux with unknown subcommand should exit non-zero"
    );
}

// ─── Subcommand help text contains flag names ─────────────────────────────────

#[test]
fn amux_init_help_mentions_agent_flag() {
    let out = run_amux(&["init", "--help"]);
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        text.contains("agent") || text.contains("--agent"),
        "`amux init --help` should mention the --agent flag;\ngot:\n{text}"
    );
}

#[test]
fn amux_ready_help_mentions_build_flag() {
    let out = run_amux(&["ready", "--help"]);
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        text.contains("build") || text.contains("--build"),
        "`amux ready --help` should mention --build;\ngot:\n{text}"
    );
}

#[test]
fn amux_headless_start_help_mentions_port_flag() {
    let out = run_amux(&["headless", "start", "--help"]);
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        text.contains("port") || text.contains("--port"),
        "`amux headless start --help` should mention --port;\ngot:\n{text}"
    );
}
