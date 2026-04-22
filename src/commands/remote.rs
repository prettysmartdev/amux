//! Remote command module: execute commands on a remote headless amux instance.
//!
//! All interactive pickers live exclusively in the TUI. This module uses a
//! `RemoteUserInput` trait to abstract the boundary between "I need a value from
//! the user" and "how to get it." CLI and headless modes use `NonInteractiveRemoteInput`
//! which returns errors for missing required values. The TUI resolves values via
//! its own dialog system before calling the non-interactive execution functions.

use crate::cli::{RemoteAction, RemoteSessionAction};
use crate::commands::output::OutputSink;
use anyhow::Result;

// ---------------------------------------------------------------------------
// Shared HTTP client
// ---------------------------------------------------------------------------

/// Build a `reqwest::Client` with the timeouts required by the spec:
/// - connect timeout: 10 seconds
/// - read timeout: 60 seconds (covers "60s of connection silence")
fn make_client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(10))
        .read_timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(|e| anyhow::anyhow!("Failed to build HTTP client: {}", e))
}

// ---------------------------------------------------------------------------
// Public data types
// ---------------------------------------------------------------------------

/// A single session entry returned from the remote host.
#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
pub struct RemoteSessionEntry {
    pub id: String,
    pub workdir: String,
}

// ---------------------------------------------------------------------------
// User-input trait
// ---------------------------------------------------------------------------

/// Trait abstracting user interaction needed by remote commands.
/// CLI/headless modes use `NonInteractiveRemoteInput` which always returns
/// errors for missing required params. TUI mode never calls these — it gathers
/// values via modal dialogs before invoking the underlying execution functions.
pub trait RemoteUserInput {
    /// Called when `remote run` has no session.
    fn resolve_missing_session(&self) -> Result<String>;

    /// Called when `remote session start` has no directory.
    fn resolve_missing_dir(&self) -> Result<String>;

    /// Called when `remote session kill` has no session ID.
    fn resolve_missing_kill_target(&self) -> Result<String>;

    /// Called when `remote session start` uses a dir not in savedDirs.
    /// Returns true if the user wants to save it.
    fn offer_save_dir(&self, dir: &str) -> Result<bool>;
}

/// Non-interactive implementation: returns descriptive errors for any missing
/// required parameter. Used by CLI dispatch and headless server.
pub struct NonInteractiveRemoteInput;

impl RemoteUserInput for NonInteractiveRemoteInput {
    fn resolve_missing_session(&self) -> Result<String> {
        anyhow::bail!(
            "No session specified. Pass --session <ID> or set AMUX_REMOTE_SESSION.\n\
             Use `amux remote session start` to create a session, or list sessions \
             with `curl <remote-addr>/v1/sessions`."
        )
    }

    fn resolve_missing_dir(&self) -> Result<String> {
        anyhow::bail!(
            "No directory specified. Pass a directory argument.\n\
             To use saved directories interactively, run this command from the TUI."
        )
    }

    fn resolve_missing_kill_target(&self) -> Result<String> {
        anyhow::bail!(
            "No session ID specified. Pass a session ID argument.\n\
             To select a session interactively, run this command from the TUI."
        )
    }

    fn offer_save_dir(&self, _dir: &str) -> Result<bool> {
        // Non-interactive: never save. The user can add dirs via
        // `amux config set remote.savedDirs ...` manually.
        Ok(false)
    }
}

// ---------------------------------------------------------------------------
// Top-level dispatch
// ---------------------------------------------------------------------------

/// Top-level dispatch for `amux remote` subcommands.
pub async fn run(action: RemoteAction) -> Result<()> {
    let input = NonInteractiveRemoteInput;
    let sink = OutputSink::Stdout;
    match action {
        RemoteAction::Run { command, remote_addr, session, follow } => {
            if command.is_empty() {
                anyhow::bail!(
                    "No command specified. Usage: amux remote run <command> [--session ID] [--follow]"
                );
            }
            let addr = resolve_remote_addr(remote_addr.as_deref())?;
            let session_id = match resolve_remote_session(session.as_deref()) {
                Some(s) => s,
                None => input.resolve_missing_session()?,
            };
            run_remote_run(&addr, &session_id, &command, follow, &sink).await
        }
        RemoteAction::Session { action } => match action {
            RemoteSessionAction::Start { dir, remote_addr } => {
                let addr = resolve_remote_addr(remote_addr.as_deref())?;
                let dir = match dir {
                    Some(d) => d,
                    None => input.resolve_missing_dir()?,
                };
                // Offer to save dir (non-interactive impl always returns false).
                let saved = crate::config::effective_remote_saved_dirs();
                if !saved.contains(&dir) {
                    if input.offer_save_dir(&dir)? {
                        save_dir_to_config(&dir)?;
                    }
                }
                let session_id = run_remote_session_start(&addr, &dir).await?;
                sink.println(format!("Session created: {}", session_id));
                Ok(())
            }
            RemoteSessionAction::Kill { session_id, remote_addr } => {
                let addr = resolve_remote_addr(remote_addr.as_deref())?;
                let sid = match session_id {
                    Some(s) => s,
                    None => input.resolve_missing_kill_target()?,
                };
                run_remote_session_kill(&addr, &sid).await?;
                sink.println(format!("Session {} killed.", sid));
                Ok(())
            }
        },
    }
}

// ---------------------------------------------------------------------------
// Core execution functions (called by all three modes once values are resolved)
// ---------------------------------------------------------------------------

/// Execute a command on the remote host.
///
/// Submits the command to `POST /v1/commands`, optionally streams logs until done,
/// then writes a summary table.
pub async fn run_remote_run(
    remote_addr: &str,
    session_id: &str,
    command: &[String],
    follow: bool,
    sink: &OutputSink,
) -> Result<()> {
    if command.is_empty() {
        anyhow::bail!("Command must not be empty");
    }

    let subcommand = &command[0];
    let args: Vec<&str> = command[1..].iter().map(|s| s.as_str()).collect();

    let client = make_client()?;

    // POST /v1/commands
    let body = serde_json::json!({
        "subcommand": subcommand,
        "args": args,
    });

    let response = client
        .post(format!("{}/v1/commands", remote_addr))
        .header("x-amux-session", session_id)
        .header("content-type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to connect to remote host: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        if status == reqwest::StatusCode::NOT_FOUND {
            anyhow::bail!(
                "Session '{}' not found on {}. \
                 Create one first with: amux remote session start <dir>",
                session_id, remote_addr
            );
        }
        if status == reqwest::StatusCode::FORBIDDEN {
            anyhow::bail!(
                "Session '{}' already has a running command. \
                 Wait for it to finish before submitting another.",
                session_id
            );
        }
        anyhow::bail!("Remote host returned {}: {}", status, text);
    }

    let create_resp: serde_json::Value = response.json().await
        .map_err(|e| anyhow::anyhow!("Failed to parse response: {}", e))?;

    let command_id = create_resp["command_id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Response missing command_id"))?
        .to_string();

    sink.println(format!("Command submitted: {}", command_id));

    if follow {
        sink.println("Streaming logs (waiting for command to complete)...".to_string());
        stream_command_logs(remote_addr, &command_id, sink).await?;
    }

    // Fetch final command status for the summary table.
    let cmd_response = client
        .get(format!("{}/v1/commands/{}", remote_addr, command_id))
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to get command status: {}", e))?;

    if !cmd_response.status().is_success() {
        let status = cmd_response.status();
        let text = cmd_response.text().await.unwrap_or_default();
        anyhow::bail!("Failed to get command status {}: {}", status, text);
    }

    let cmd_json: serde_json::Value = cmd_response.json().await
        .map_err(|e| anyhow::anyhow!("Failed to parse command status: {}", e))?;

    // Render summary table.
    render_summary_table(
        &command_id,
        session_id,
        &command.join(" "),
        cmd_json["status"].as_str().unwrap_or("unknown"),
        cmd_json["exit_code"].as_i64(),
        cmd_json["started_at"].as_str(),
        cmd_json["finished_at"].as_str(),
        sink,
    );

    Ok(())
}

/// Create a new session on the remote host.
/// Returns the session ID.
pub async fn run_remote_session_start(remote_addr: &str, dir: &str) -> Result<String> {
    let client = make_client()?;

    let body = serde_json::json!({ "workdir": dir });

    let response = client
        .post(format!("{}/v1/sessions", remote_addr))
        .header("content-type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to connect to remote host: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        anyhow::bail!("Remote host returned {}: {}", status, text);
    }

    let resp: serde_json::Value = response.json().await
        .map_err(|e| anyhow::anyhow!("Failed to parse response: {}", e))?;

    let session_id = resp["session_id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Response missing session_id"))?
        .to_string();

    Ok(session_id)
}

/// Kill (close) a session on the remote host.
pub async fn run_remote_session_kill(remote_addr: &str, session_id: &str) -> Result<()> {
    let client = make_client()?;

    let response = client
        .delete(format!("{}/v1/sessions/{}", remote_addr, session_id))
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to connect to remote host: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        if status == reqwest::StatusCode::NOT_FOUND {
            anyhow::bail!(
                "Session '{}' not found on {}.",
                session_id, remote_addr
            );
        }
        anyhow::bail!("Remote host returned {}: {}", status, text);
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Address and session resolution helpers
// ---------------------------------------------------------------------------

/// Resolve the remote address from: flag → `AMUX_REMOTE_ADDR` env → `remote.defaultAddr` config.
/// Returns a descriptive error if none found.
pub fn resolve_remote_addr(flag: Option<&str>) -> Result<String> {
    if let Some(addr) = flag {
        return Ok(addr.to_string());
    }
    if let Ok(addr) = std::env::var("AMUX_REMOTE_ADDR") {
        if !addr.is_empty() {
            return Ok(addr);
        }
    }
    if let Some(addr) = crate::config::effective_remote_default_addr() {
        return Ok(addr);
    }
    anyhow::bail!(
        "No remote address configured. Pass --remote-addr <URL>, set AMUX_REMOTE_ADDR, \
         or configure `remote.defaultAddr` via `amux config set --global remote.defaultAddr <URL>`."
    )
}

/// Resolve the remote session from: flag → `AMUX_REMOTE_SESSION` env.
/// Returns `None` if neither is set (caller decides whether to error or show picker).
pub fn resolve_remote_session(flag: Option<&str>) -> Option<String> {
    if let Some(session) = flag {
        return Some(session.to_string());
    }
    if let Ok(session) = std::env::var("AMUX_REMOTE_SESSION") {
        if !session.is_empty() {
            return Some(session);
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Session listing
// ---------------------------------------------------------------------------

/// Fetch the list of active sessions from the remote host.
pub async fn fetch_sessions(remote_addr: &str) -> Result<Vec<RemoteSessionEntry>> {
    let client = make_client()?;

    let response = client
        .get(format!("{}/v1/sessions", remote_addr))
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to connect to remote host: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        anyhow::bail!("Remote host returned {}: {}", status, text);
    }

    let resp: serde_json::Value = response.json().await
        .map_err(|e| anyhow::anyhow!("Failed to parse sessions response: {}", e))?;

    let sessions = resp["sessions"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("Response missing sessions array"))?;

    let entries: Vec<RemoteSessionEntry> = sessions
        .iter()
        .filter_map(|s| {
            let id = s["id"].as_str()?.to_string();
            let workdir = s["workdir"].as_str()?.to_string();
            Some(RemoteSessionEntry { id, workdir })
        })
        .collect();

    Ok(entries)
}

// ---------------------------------------------------------------------------
// SSE log streaming
// ---------------------------------------------------------------------------

/// Connect to the SSE endpoint for the given command and write each log line
/// to the output sink. Returns when the `[amux:done]` sentinel is received.
pub async fn stream_command_logs(
    remote_addr: &str,
    command_id: &str,
    sink: &OutputSink,
) -> Result<()> {
    use tokio_stream::StreamExt;

    let client = make_client()?;

    let response = client
        .get(format!("{}/v1/commands/{}/logs/stream", remote_addr, command_id))
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to connect to SSE stream: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        anyhow::bail!("SSE stream returned {}: {}", status, text);
    }

    let mut stream = response.bytes_stream();
    let mut buf = String::new();

    while let Some(chunk_result) = stream.next().await {
        let chunk = chunk_result
            .map_err(|e| anyhow::anyhow!("Stream read error: {}", e))?;
        let text = String::from_utf8_lossy(&chunk);
        buf.push_str(&text);

        // Process complete SSE events (separated by \n\n).
        while let Some(pos) = buf.find("\n\n") {
            let event = buf[..pos].to_string();
            buf = buf[pos + 2..].to_string();

            for line in event.lines() {
                if let Some(data) = line.strip_prefix("data: ") {
                    if data == "[amux:done]" {
                        return Ok(());
                    }
                    sink.println(data.to_string());
                }
            }
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Config helpers
// ---------------------------------------------------------------------------

/// Add `dir` to `remote.savedDirs` in global config if not already present.
pub fn save_dir_to_config(dir: &str) -> Result<()> {
    let mut global = crate::config::load_global_config()?;
    let remote = global.remote.get_or_insert_with(crate::config::RemoteConfig::default);
    let saved_dirs = remote.saved_dirs.get_or_insert_with(Vec::new);
    if !saved_dirs.contains(&dir.to_string()) {
        saved_dirs.push(dir.to_string());
        crate::config::save_global_config(&global)?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Summary table rendering
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn render_summary_table(
    command_id: &str,
    session_id: &str,
    subcommand: &str,
    status: &str,
    exit_code: Option<i64>,
    started_at: Option<&str>,
    finished_at: Option<&str>,
    sink: &OutputSink,
) {
    let col1_w = 14usize;
    let col2_w = 40usize;

    let top    = format!("┌{}┬{}┐", "─".repeat(col1_w + 2), "─".repeat(col2_w + 2));
    let mid    = format!("├{}┼{}┤", "─".repeat(col1_w + 2), "─".repeat(col2_w + 2));
    let bot    = format!("└{}┴{}┘", "─".repeat(col1_w + 2), "─".repeat(col2_w + 2));
    let header = format!("│ {:<col1_w$} │ {:<col2_w$} │", "Field", "Value", col1_w = col1_w, col2_w = col2_w);

    let rows: &[(&str, String)] = &[
        ("Command ID",  truncate(command_id,  col2_w)),
        ("Session ID",  truncate(session_id,  col2_w)),
        ("Subcommand",  truncate(subcommand,  col2_w)),
        ("Status",      truncate(status,      col2_w)),
        ("Exit Code",   exit_code.map(|c| c.to_string()).unwrap_or_else(|| "-".to_string())),
        ("Started",     truncate(started_at.unwrap_or("-"), col2_w)),
        ("Finished",    truncate(finished_at.unwrap_or("-"), col2_w)),
    ];

    sink.println(top);
    sink.println(header);
    sink.println(mid.clone());
    for (field, value) in rows {
        sink.println(format!(
            "│ {:<col1_w$} │ {:<col2_w$} │",
            field, value,
            col1_w = col1_w,
            col2_w = col2_w
        ));
    }
    sink.println(bot);
}

fn truncate(s: &str, max: usize) -> String {
    // Count Unicode scalar values, not bytes, so multi-byte characters
    // (rare in IDs/paths/timestamps but possible) don't cause a panic.
    let char_count = s.chars().count();
    if char_count <= max {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max.saturating_sub(1)).collect();
        format!("{}…", truncated)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// Serialise env-var mutations: env is process-global state, so tests that
    /// mutate `AMUX_REMOTE_ADDR`, `AMUX_REMOTE_SESSION`, or `AMUX_CONFIG_HOME`
    /// must hold this lock for the duration of the mutation.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    // ─── resolve_remote_addr ─────────────────────────────────────────────────

    #[test]
    fn resolve_remote_addr_flag_wins_over_env_and_config() {
        let _guard = ENV_LOCK.lock().unwrap();
        // SAFETY: test-only; serialised by ENV_LOCK.
        unsafe { std::env::set_var("AMUX_REMOTE_ADDR", "http://env-host:9876") };
        let result = resolve_remote_addr(Some("http://flag-host:9876"));
        unsafe { std::env::remove_var("AMUX_REMOTE_ADDR") };
        assert_eq!(result.unwrap(), "http://flag-host:9876", "flag must win over env var");
    }

    #[test]
    fn resolve_remote_addr_env_wins_when_no_flag() {
        let _guard = ENV_LOCK.lock().unwrap();
        unsafe { std::env::set_var("AMUX_REMOTE_ADDR", "http://env-host:9876") };
        let result = resolve_remote_addr(None);
        unsafe { std::env::remove_var("AMUX_REMOTE_ADDR") };
        assert_eq!(result.unwrap(), "http://env-host:9876", "env var must be used when no flag");
    }

    #[test]
    fn resolve_remote_addr_returns_helpful_error_when_all_missing() {
        let _guard = ENV_LOCK.lock().unwrap();
        unsafe { std::env::remove_var("AMUX_REMOTE_ADDR") };
        // Use an isolated temp dir so no global config exists.
        let tmp = tempfile::TempDir::new().unwrap();
        unsafe { std::env::set_var("AMUX_CONFIG_HOME", tmp.path().to_str().unwrap()) };
        let result = resolve_remote_addr(None);
        unsafe { std::env::remove_var("AMUX_CONFIG_HOME") };
        assert!(result.is_err(), "must error when flag, env, and config are all absent");
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("No remote address"),
            "error must be descriptive; got: {msg}"
        );
    }

    // ─── resolve_remote_session ──────────────────────────────────────────────

    #[test]
    fn resolve_remote_session_flag_wins_over_env() {
        let _guard = ENV_LOCK.lock().unwrap();
        unsafe { std::env::set_var("AMUX_REMOTE_SESSION", "env-session") };
        let result = resolve_remote_session(Some("flag-session"));
        unsafe { std::env::remove_var("AMUX_REMOTE_SESSION") };
        assert_eq!(result, Some("flag-session".to_string()));
    }

    #[test]
    fn resolve_remote_session_env_used_when_no_flag() {
        let _guard = ENV_LOCK.lock().unwrap();
        unsafe { std::env::set_var("AMUX_REMOTE_SESSION", "env-session-id") };
        let result = resolve_remote_session(None);
        unsafe { std::env::remove_var("AMUX_REMOTE_SESSION") };
        assert_eq!(result, Some("env-session-id".to_string()));
    }

    #[test]
    fn resolve_remote_session_returns_none_when_both_absent() {
        let _guard = ENV_LOCK.lock().unwrap();
        unsafe { std::env::remove_var("AMUX_REMOTE_SESSION") };
        let result = resolve_remote_session(None);
        assert_eq!(result, None, "must return None when flag and env are both absent");
    }

    // ─── NonInteractiveRemoteInput ───────────────────────────────────────────

    #[test]
    fn non_interactive_resolve_missing_session_returns_descriptive_error() {
        let input = NonInteractiveRemoteInput;
        let err = input.resolve_missing_session().unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("No session specified") || msg.contains("--session"),
            "error must mention how to fix the issue; got: {msg}"
        );
    }

    #[test]
    fn non_interactive_resolve_missing_dir_returns_descriptive_error() {
        let input = NonInteractiveRemoteInput;
        let err = input.resolve_missing_dir().unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("No directory specified"),
            "error must describe the missing param; got: {msg}"
        );
    }

    #[test]
    fn non_interactive_resolve_missing_kill_target_returns_descriptive_error() {
        let input = NonInteractiveRemoteInput;
        let err = input.resolve_missing_kill_target().unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("No session ID specified"),
            "error must describe the missing param; got: {msg}"
        );
    }

    #[test]
    fn non_interactive_offer_save_dir_always_returns_false() {
        let input = NonInteractiveRemoteInput;
        let result = input.offer_save_dir("/workspace/proj").unwrap();
        assert!(!result, "non-interactive must never offer to save a dir");
    }

    // ─── save_dir_to_config ──────────────────────────────────────────────────

    #[test]
    fn save_dir_to_config_adds_new_dir() {
        let _guard = ENV_LOCK.lock().unwrap();
        let tmp = tempfile::TempDir::new().unwrap();
        unsafe { std::env::set_var("AMUX_CONFIG_HOME", tmp.path().to_str().unwrap()) };

        save_dir_to_config("/workspace/myproject").unwrap();

        let global = crate::config::load_global_config().unwrap();
        let saved = global.remote.unwrap().saved_dirs.unwrap_or_default();
        assert!(
            saved.contains(&"/workspace/myproject".to_string()),
            "dir must appear in savedDirs after first call; got: {saved:?}"
        );

        unsafe { std::env::remove_var("AMUX_CONFIG_HOME") };
    }

    #[test]
    fn save_dir_to_config_skips_duplicate_dir() {
        let _guard = ENV_LOCK.lock().unwrap();
        let tmp = tempfile::TempDir::new().unwrap();
        unsafe { std::env::set_var("AMUX_CONFIG_HOME", tmp.path().to_str().unwrap()) };

        save_dir_to_config("/workspace/dup").unwrap();
        save_dir_to_config("/workspace/dup").unwrap(); // second call must be a no-op

        let global = crate::config::load_global_config().unwrap();
        let saved = global.remote.unwrap().saved_dirs.unwrap_or_default();
        let count = saved.iter().filter(|d| d.as_str() == "/workspace/dup").count();
        assert_eq!(count, 1, "duplicate dir must appear exactly once; got: {saved:?}");

        unsafe { std::env::remove_var("AMUX_CONFIG_HOME") };
    }

    // ─── empty command vector ────────────────────────────────────────────────

    /// An empty command slice must be rejected immediately — before any HTTP call
    /// is attempted.  We verify this via `run_remote_run` directly (the inner
    /// function), which returns an error synchronously without ever reaching the
    /// network.
    #[tokio::test]
    async fn run_remote_run_rejects_empty_command_before_network() {
        let sink = crate::commands::output::OutputSink::Null;
        let result = run_remote_run(
            "http://127.0.0.1:9", // port 9 is the discard port; connect should never happen
            "any-session",
            &[],                  // empty command ← the trigger
            false,
            &sink,
        ).await;
        assert!(result.is_err(), "empty command must return an error");
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("empty") || msg.contains("No command"),
            "error must describe the problem; got: {msg}"
        );
    }

    // ─── truncate ────────────────────────────────────────────────────────────

    #[test]
    fn truncate_short_string_unchanged() {
        assert_eq!(truncate("hello", 10), "hello");
    }

    #[test]
    fn truncate_exact_length_unchanged() {
        assert_eq!(truncate("hello", 5), "hello");
    }

    #[test]
    fn truncate_long_string_gets_ellipsis() {
        let result = truncate("abcdefghij", 5);
        assert!(result.ends_with('…'), "must end with ellipsis; got: {result}");
        assert!(
            result.chars().count() <= 5,
            "must not exceed max chars; got: {result}"
        );
    }

    #[test]
    fn truncate_multibyte_does_not_panic() {
        // "日本語" is 3 chars but 9 bytes; slicing at byte 2 would panic.
        let result = truncate("日本語テスト", 3);
        assert!(result.ends_with('…'), "must end with ellipsis; got: {result}");
        // Should not panic — that is the primary assertion.
    }

    #[test]
    fn truncate_empty_string_unchanged() {
        assert_eq!(truncate("", 5), "");
    }
}
