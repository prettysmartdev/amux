use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

/// Returns the path to the PID file.
pub fn pid_file_path(root: &Path) -> PathBuf {
    root.join("amux.pid")
}

/// Returns the path to the log file.
pub fn log_file_path(root: &Path) -> PathBuf {
    root.join("amux.log")
}

/// Write the current process PID to the PID file.
pub fn write_pid_file(root: &Path) -> Result<()> {
    let path = pid_file_path(root);
    std::fs::create_dir_all(root)
        .with_context(|| format!("Failed to create {}", root.display()))?;
    std::fs::write(&path, std::process::id().to_string())
        .with_context(|| format!("Failed to write PID file {}", path.display()))
}

/// Read the PID from the PID file. Returns None if the file does not exist.
pub fn read_pid_file(root: &Path) -> Result<Option<u32>> {
    let path = pid_file_path(root);
    if !path.exists() {
        return Ok(None);
    }
    let content = std::fs::read_to_string(&path)
        .with_context(|| format!("Failed to read PID file {}", path.display()))?;
    let pid: u32 = content
        .trim()
        .parse()
        .with_context(|| format!("Invalid PID in {}", path.display()))?;
    Ok(Some(pid))
}

/// Remove the PID file.
pub fn remove_pid_file(root: &Path) -> Result<()> {
    let path = pid_file_path(root);
    if path.exists() {
        std::fs::remove_file(&path)
            .with_context(|| format!("Failed to remove PID file {}", path.display()))?;
    }
    Ok(())
}

/// Check whether a process with the given PID is alive.
pub fn is_process_alive(pid: u32) -> bool {
    // Send signal 0 to check existence without actually signaling.
    nix::sys::signal::kill(
        nix::unistd::Pid::from_raw(pid as i32),
        None,
    )
    .is_ok()
}

/// Check if the server is already running (PID file exists and process is alive).
pub fn check_already_running(root: &Path) -> Result<Option<u32>> {
    match read_pid_file(root)? {
        Some(pid) if is_process_alive(pid) => Ok(Some(pid)),
        Some(_) => {
            // Stale PID file — process is dead. Clean it up.
            remove_pid_file(root)?;
            Ok(None)
        }
        None => Ok(None),
    }
}

/// Kill the background server by reading the PID file and sending SIGTERM.
pub fn kill_server(root: &Path) -> Result<()> {
    let pid = match read_pid_file(root)? {
        Some(pid) => pid,
        None => bail!("No PID file found at {}. Is the headless server running?", pid_file_path(root).display()),
    };

    if !is_process_alive(pid) {
        remove_pid_file(root)?;
        bail!("Server process (PID {}) is not running. Removed stale PID file.", pid);
    }

    tracing::info!(pid, "Sending SIGTERM to headless server");
    nix::sys::signal::kill(
        nix::unistd::Pid::from_raw(pid as i32),
        nix::sys::signal::Signal::SIGTERM,
    )
    .with_context(|| format!("Failed to send SIGTERM to PID {}", pid))?;

    remove_pid_file(root)?;

    // On macOS, also try to unload the launchd plist.
    #[cfg(target_os = "macos")]
    {
        let plist_path = launchd_plist_path();
        if plist_path.exists() {
            let _ = std::process::Command::new("launchctl")
                .args(["unload", &plist_path.to_string_lossy()])
                .status();
            let _ = std::fs::remove_file(&plist_path);
        }
    }

    println!("Headless server (PID {}) stopped.", pid);
    Ok(())
}

/// Daemonize the server process via the OS process manager.
pub fn daemonize(port: u16, workdirs: &[String]) -> Result<()> {
    let amux_bin = std::env::current_exe().context("Cannot determine amux binary path")?;

    // Build the foreground command that the daemon will run.
    let mut args = vec![
        "headless".to_string(),
        "start".to_string(),
        "--port".to_string(),
        port.to_string(),
    ];
    for dir in workdirs {
        args.push("--workdirs".to_string());
        args.push(dir.clone());
    }

    #[cfg(target_os = "linux")]
    {
        if try_systemd_run(&amux_bin, &args)? {
            return Ok(());
        }
    }

    #[cfg(target_os = "macos")]
    {
        if try_launchd(&amux_bin, &args, port)? {
            return Ok(());
        }
    }

    // Fallback: double-fork.
    double_fork_daemonize(&amux_bin, &args)
}

/// Linux: attempt to use systemd-run --user for a transient unit.
#[cfg(target_os = "linux")]
fn try_systemd_run(amux_bin: &Path, args: &[String]) -> Result<bool> {
    // Check if systemd-run is available.
    let check = std::process::Command::new("systemd-run")
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();

    match check {
        Ok(s) if s.success() => {}
        _ => return Ok(false), // systemd not available, fall through.
    }

    let mut cmd = std::process::Command::new("systemd-run");
    cmd.args(["--user", "--unit=amux-headless", "--"])
        .arg(amux_bin)
        .args(args);

    let status = cmd.status().context("Failed to run systemd-run")?;
    if !status.success() {
        // systemd-run failed (maybe user session not available), fall through.
        return Ok(false);
    }

    println!("Headless server started via systemd (transient unit: amux-headless).");
    Ok(true)
}

/// Escape XML special characters so that arbitrary strings can be safely
/// embedded inside plist `<string>` elements.
#[cfg(target_os = "macos")]
fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// macOS: write a launchd plist and load it.
#[cfg(target_os = "macos")]
fn launchd_plist_path() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
    home.join("Library/LaunchAgents/io.amux.headless.plist")
}

#[cfg(target_os = "macos")]
fn try_launchd(amux_bin: &Path, args: &[String], _port: u16) -> Result<bool> {
    let plist_path = launchd_plist_path();
    if let Some(parent) = plist_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // XML-escape every string inserted into the plist to handle paths that
    // contain special characters such as '&', '<', '>', '"', or "'".
    let mut program_args = format!(
        "    <string>{}</string>\n",
        xml_escape(&amux_bin.to_string_lossy())
    );
    for arg in args {
        program_args.push_str(&format!("    <string>{}</string>\n", xml_escape(arg)));
    }

    let root = super::db::headless_root()?;
    let log_path = log_file_path(&root);

    let plist = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>io.amux.headless</string>
    <key>ProgramArguments</key>
    <array>
{program_args}    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>StandardOutPath</key>
    <string>{log}</string>
    <key>StandardErrorPath</key>
    <string>{log}</string>
</dict>
</plist>
"#,
        log = xml_escape(&log_path.to_string_lossy())
    );

    std::fs::write(&plist_path, plist)?;

    let status = std::process::Command::new("launchctl")
        .args(["load", &plist_path.to_string_lossy()])
        .status()
        .context("Failed to run launchctl load")?;

    if !status.success() {
        let _ = std::fs::remove_file(&plist_path);
        return Ok(false);
    }

    println!("Headless server started via launchd (io.amux.headless).");
    Ok(true)
}

/// Fallback: double-fork daemonization.
fn double_fork_daemonize(amux_bin: &Path, args: &[String]) -> Result<()> {
    let child = std::process::Command::new(amux_bin)
        .args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .context("Failed to spawn background server process")?;

    println!("Headless server started in background (PID {}).", child.id());
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    // ── Path helpers ────────────────────────────────────────────────────────

    #[test]
    fn pid_file_path_is_amux_pid_inside_root() {
        let root = std::path::Path::new("/some/root");
        assert_eq!(pid_file_path(root), root.join("amux.pid"));
    }

    #[test]
    fn log_file_path_is_amux_log_inside_root() {
        let root = std::path::Path::new("/some/root");
        assert_eq!(log_file_path(root), root.join("amux.log"));
    }

    // ── PID file write / read / delete ─────────────────────────────────────

    #[test]
    fn write_and_read_pid_file_round_trips_current_pid() {
        let tmp = TempDir::new().unwrap();
        write_pid_file(tmp.path()).unwrap();

        let pid = read_pid_file(tmp.path()).unwrap();
        assert_eq!(pid, Some(std::process::id()));
    }

    #[test]
    fn read_pid_file_returns_none_when_file_absent() {
        let tmp = TempDir::new().unwrap();
        // No file written — must return None, not an error.
        let pid = read_pid_file(tmp.path()).unwrap();
        assert!(pid.is_none());
    }

    #[test]
    fn remove_pid_file_deletes_the_file() {
        let tmp = TempDir::new().unwrap();
        write_pid_file(tmp.path()).unwrap();
        assert!(pid_file_path(tmp.path()).exists());

        remove_pid_file(tmp.path()).unwrap();
        assert!(!pid_file_path(tmp.path()).exists());
    }

    #[test]
    fn remove_pid_file_is_idempotent_when_absent() {
        let tmp = TempDir::new().unwrap();
        // Should not return an error when the file doesn't exist.
        remove_pid_file(tmp.path()).unwrap();
    }

    #[test]
    fn write_pid_file_creates_parent_directory_if_missing() {
        let outer = TempDir::new().unwrap();
        let nested = outer.path().join("deeply").join("nested").join("dir");
        // Directory does not exist yet — write_pid_file must create it.
        write_pid_file(&nested).unwrap();
        assert!(pid_file_path(&nested).exists());
    }

    // ── is_process_alive ────────────────────────────────────────────────────

    #[test]
    fn is_process_alive_true_for_current_process() {
        let pid = std::process::id();
        assert!(is_process_alive(pid), "current process must report as alive");
    }

    #[test]
    fn is_process_alive_false_for_reaped_process() {
        // Spawn a trivial child and wait for it to exit (reap it).
        let mut child = std::process::Command::new("sh")
            .args(["-c", "exit 0"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("sh must be available on the test host");
        let pid = child.id();
        child.wait().unwrap(); // reap — PID is released by the kernel

        // After reaping, the PID is no longer valid (signal 0 returns ESRCH).
        // PID recycling within microseconds is theoretically possible but
        // extremely rare in practice; this assertion is stable in CI.
        assert!(!is_process_alive(pid), "reaped process must report as not alive");
    }

    // ── check_already_running ───────────────────────────────────────────────

    #[test]
    fn check_already_running_returns_none_without_pid_file() {
        let tmp = TempDir::new().unwrap();
        let result = check_already_running(tmp.path()).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn check_already_running_returns_pid_when_process_is_live() {
        let tmp = TempDir::new().unwrap();
        write_pid_file(tmp.path()).unwrap(); // writes current PID

        let result = check_already_running(tmp.path()).unwrap();
        assert_eq!(result, Some(std::process::id()));
        // File must NOT be removed — the process is alive.
        assert!(pid_file_path(tmp.path()).exists());
    }

    #[test]
    fn check_already_running_removes_stale_pid_file_and_returns_none() {
        let tmp = TempDir::new().unwrap();

        // Spawn a child, capture its PID, wait for it to exit, then write its
        // (now stale) PID into the PID file.
        let mut child = std::process::Command::new("sh")
            .args(["-c", "exit 0"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("sh must be available");
        let dead_pid = child.id();
        child.wait().unwrap();

        std::fs::create_dir_all(tmp.path()).unwrap();
        std::fs::write(pid_file_path(tmp.path()), dead_pid.to_string()).unwrap();

        // check_already_running must detect the dead process and clean up.
        let result = check_already_running(tmp.path()).unwrap();
        assert!(result.is_none(), "dead process must not be reported as running");
        assert!(
            !pid_file_path(tmp.path()).exists(),
            "stale PID file must be removed"
        );
    }
}

/// Stream the log file to stdout, following new content like `tail -f`.
pub async fn stream_logs(root: &Path) -> Result<()> {
    let log_path = log_file_path(root);
    if !log_path.exists() {
        bail!(
            "Log file not found at {}. Start the server with --background first.",
            log_path.display()
        );
    }

    use tokio::io::{AsyncBufReadExt, BufReader};
    let file = tokio::fs::File::open(&log_path)
        .await
        .with_context(|| format!("Failed to open log file {}", log_path.display()))?;
    let mut reader = BufReader::new(file);
    let mut line = String::new();

    loop {
        line.clear();
        let n = reader.read_line(&mut line).await?;
        if n == 0 {
            // No new data — wait a bit before polling again.
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            continue;
        }
        print!("{}", line);
    }
}
