/// Integration tests for the remote client module (`src/commands/remote.rs`).
///
/// Tests spin up an in-process headless HTTP server (the same infrastructure
/// used by `headless_integration.rs`) and exercise the public remote-client
/// functions against it over real HTTP.  No Docker daemon is required.
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use amux::commands::headless::db;
use amux::commands::headless::server::{AppState, build_router};
use amux::commands::output::OutputSink;
use amux::commands::remote::{
    fetch_sessions, run_remote_run, run_remote_session_kill, run_remote_session_start,
    stream_command_logs,
};
use amux::runtime::{AgentRuntime, ContainerStats, HostSettings, StoppedContainerInfo};
use tempfile::TempDir;
use tokio::sync::Mutex;

// ---------------------------------------------------------------------------
// Mock runtime
// ---------------------------------------------------------------------------

struct MockRuntime;

impl AgentRuntime for MockRuntime {
    fn is_available(&self) -> bool {
        false
    }
    fn check_socket(&self) -> anyhow::Result<PathBuf> {
        anyhow::bail!("MockRuntime: not implemented")
    }
    fn build_image_streaming(
        &self,
        _tag: &str,
        _dockerfile: &std::path::Path,
        _context: &std::path::Path,
        _no_cache: bool,
        _on_line: &mut dyn FnMut(&str),
    ) -> anyhow::Result<String> {
        anyhow::bail!("MockRuntime: not implemented")
    }
    fn image_exists(&self, _tag: &str) -> bool {
        false
    }
    fn run_container(
        &self,
        _image: &str,
        _host_path: &str,
        _entrypoint: &[&str],
        _env_vars: &[(String, String)],
        _host_settings: Option<&HostSettings>,
        _allow_docker: bool,
        _container_name: Option<&str>,
        _ssh_dir: Option<&std::path::Path>,
    ) -> anyhow::Result<()> {
        anyhow::bail!("MockRuntime: not implemented")
    }
    fn run_container_captured(
        &self,
        _image: &str,
        _host_path: &str,
        _entrypoint: &[&str],
        _env_vars: &[(String, String)],
        _host_settings: Option<&HostSettings>,
        _allow_docker: bool,
        _container_name: Option<&str>,
        _ssh_dir: Option<&std::path::Path>,
    ) -> anyhow::Result<(String, String)> {
        anyhow::bail!("MockRuntime: not implemented")
    }
    fn run_container_at_path(
        &self,
        _image: &str,
        _host_path: &str,
        _container_path: &str,
        _working_dir: &str,
        _entrypoint: &[&str],
        _env_vars: &[(String, String)],
        _host_settings: Option<&HostSettings>,
        _allow_docker: bool,
        _container_name: Option<&str>,
    ) -> anyhow::Result<()> {
        anyhow::bail!("MockRuntime: not implemented")
    }
    fn run_container_captured_at_path(
        &self,
        _image: &str,
        _host_path: &str,
        _container_path: &str,
        _working_dir: &str,
        _entrypoint: &[&str],
        _env_vars: &[(String, String)],
        _host_settings: Option<&HostSettings>,
        _allow_docker: bool,
    ) -> anyhow::Result<(String, String)> {
        anyhow::bail!("MockRuntime: not implemented")
    }
    fn run_container_detached(
        &self,
        _image: &str,
        _host_path: &str,
        _container_path: &str,
        _working_dir: &str,
        _container_name: Option<&str>,
        _env_vars: Vec<(String, String)>,
        _allow_docker: bool,
        _host_settings: Option<&HostSettings>,
    ) -> anyhow::Result<String> {
        anyhow::bail!("MockRuntime: not implemented")
    }
    fn start_container(&self, _container_id: &str) -> anyhow::Result<()> {
        anyhow::bail!("MockRuntime: not implemented")
    }
    fn stop_container(&self, _container_id: &str) -> anyhow::Result<()> {
        anyhow::bail!("MockRuntime: not implemented")
    }
    fn remove_container(&self, _container_id: &str) -> anyhow::Result<()> {
        anyhow::bail!("MockRuntime: not implemented")
    }
    fn is_container_running(&self, _container_id: &str) -> bool {
        false
    }
    fn find_stopped_container(&self, _name: &str, _image: &str) -> Option<StoppedContainerInfo> {
        None
    }
    fn list_running_containers_by_prefix(&self, _prefix: &str) -> Vec<String> {
        vec![]
    }
    fn list_running_containers_with_ids_by_prefix(
        &self,
        _prefix: &str,
    ) -> Vec<(String, String)> {
        vec![]
    }
    fn get_container_workspace_mount(&self, _container_name: &str) -> Option<String> {
        None
    }
    fn query_container_stats(&self, _name: &str) -> Option<ContainerStats> {
        None
    }
    fn build_run_args_pty(
        &self,
        _image: &str,
        _host_path: &str,
        _entrypoint: &[&str],
        _env_vars: &[(String, String)],
        _host_settings: Option<&HostSettings>,
        _allow_docker: bool,
        _container_name: Option<&str>,
        _ssh_dir: Option<&std::path::Path>,
    ) -> Vec<String> {
        vec![]
    }
    fn build_run_args_pty_display(
        &self,
        _image: &str,
        _host_path: &str,
        _entrypoint: &[&str],
        _env_vars: &[(String, String)],
        _host_settings: Option<&HostSettings>,
        _allow_docker: bool,
        _container_name: Option<&str>,
        _ssh_dir: Option<&std::path::Path>,
    ) -> Vec<String> {
        vec![]
    }
    fn build_run_args_pty_at_path(
        &self,
        _image: &str,
        _host_path: &str,
        _container_path: &str,
        _working_dir: &str,
        _entrypoint: &[&str],
        _env_vars: &[(String, String)],
        _host_settings: Option<&HostSettings>,
        _allow_docker: bool,
        _container_name: Option<&str>,
    ) -> Vec<String> {
        vec![]
    }
    fn build_exec_args_pty(
        &self,
        _container_id: &str,
        _working_dir: &str,
        _entrypoint: &[&str],
        _env_vars: &[(String, String)],
    ) -> Vec<String> {
        vec![]
    }
    fn build_run_args_display(
        &self,
        _image: &str,
        _host_path: &str,
        _entrypoint: &[&str],
        _env_vars: &[(String, String)],
        _host_settings: Option<&HostSettings>,
        _allow_docker: bool,
        _container_name: Option<&str>,
        _ssh_dir: Option<&std::path::Path>,
    ) -> Vec<String> {
        vec![]
    }
    fn name(&self) -> &'static str {
        "mock"
    }
    fn cli_binary(&self) -> &'static str {
        "mock"
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Start an in-process headless server bound to a random port.
///
/// Returns the temp-dir (must be kept alive for the duration of the test)
/// and the base URL (e.g. `"http://127.0.0.1:PORT"`).
async fn start_test_server(workdirs: Vec<PathBuf>) -> (TempDir, String) {
    let root_dir = TempDir::new().unwrap();
    let conn = db::open_db(root_dir.path()).unwrap();

    let state = Arc::new(AppState {
        db: Mutex::new(conn),
        workdirs,
        headless_root: root_dir.path().to_path_buf(),
        started_at: Instant::now(),
        runtime: Arc::new(MockRuntime),
        busy_sessions: Mutex::new(HashSet::new()),
        task_handles: Mutex::new(Vec::new()),
    });

    let app = build_router(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .unwrap();
    let port = listener.local_addr().unwrap().port();
    let base_url = format!("http://127.0.0.1:{port}");

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    (root_dir, base_url)
}

/// Seed a "done" command into the server's DB and write its log file.
///
/// Uses a separate SQLite connection (WAL mode allows concurrent access), so
/// the server sees the data as soon as the write is committed.
async fn seed_done_command(
    root_dir: &TempDir,
    workdir_canonical: &std::path::Path,
    session_id: &str,
    command_id: &str,
    log_content: &str,
) {
    let cmd_dir = root_dir
        .path()
        .join("sessions")
        .join(session_id)
        .join("commands")
        .join(command_id);
    std::fs::create_dir_all(&cmd_dir).unwrap();
    let log_path = cmd_dir.join("output.log");
    std::fs::write(&log_path, log_content).unwrap();

    let conn = db::open_db(root_dir.path()).unwrap();
    db::insert_session(
        &conn,
        session_id,
        workdir_canonical.to_str().unwrap(),
        "2024-01-01T00:00:00Z",
    )
    .unwrap();
    db::insert_command(
        &conn,
        command_id,
        session_id,
        "status",
        "[]",
        log_path.to_str().unwrap(),
    )
    .unwrap();
    db::update_command_started(&conn, command_id, "2024-01-01T00:00:01Z").unwrap();
    db::update_command_finished(
        &conn,
        command_id,
        "done",
        Some(0),
        "2024-01-01T00:00:02Z",
    )
    .unwrap();
}

// ---------------------------------------------------------------------------
// remote session start
// ---------------------------------------------------------------------------

#[tokio::test]
async fn run_remote_session_start_creates_session_and_returns_id() {
    let workdir = TempDir::new().unwrap();
    let canonical = std::fs::canonicalize(workdir.path()).unwrap();
    let (_root, base) = start_test_server(vec![canonical.clone()]).await;

    let session_id = run_remote_session_start(&base, canonical.to_str().unwrap())
        .await
        .unwrap();

    assert!(!session_id.is_empty(), "session ID must be non-empty");
}

#[tokio::test]
async fn run_remote_session_start_returns_error_for_unlisted_workdir() {
    // No workdirs in the allowlist.
    let (_root, base) = start_test_server(vec![]).await;

    let result = run_remote_session_start(&base, "/not/an/allowed/dir").await;

    assert!(result.is_err(), "unlisted workdir must be rejected");
    let msg = result.unwrap_err().to_string();
    // The server returns 400 or 404; the client should surface it.
    assert!(
        msg.contains("Remote host returned") || msg.contains("400") || msg.contains("403"),
        "error must describe the rejection; got: {msg}"
    );
}

// ---------------------------------------------------------------------------
// remote session kill
// ---------------------------------------------------------------------------

#[tokio::test]
async fn run_remote_session_kill_closes_an_existing_session() {
    let workdir = TempDir::new().unwrap();
    let canonical = std::fs::canonicalize(workdir.path()).unwrap();
    let (root_dir, base) = start_test_server(vec![canonical.clone()]).await;

    let session_id = run_remote_session_start(&base, canonical.to_str().unwrap())
        .await
        .unwrap();

    run_remote_session_kill(&base, &session_id).await.unwrap();

    // Verify the session is marked closed in the DB.
    let conn = db::open_db(root_dir.path()).unwrap();
    let row = db::get_session(&conn, &session_id).unwrap().unwrap();
    assert!(
        row.closed_at.is_some(),
        "session must be marked closed after kill; got row: {:?}",
        row.status
    );
}

#[tokio::test]
async fn run_remote_session_kill_returns_descriptive_error_for_unknown_session() {
    let (_root, base) = start_test_server(vec![]).await;

    let result = run_remote_session_kill(&base, "no-such-session-xyz").await;

    assert!(result.is_err(), "killing a non-existent session must return an error");
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("not found") || msg.contains("no-such-session-xyz"),
        "error must identify the missing session; got: {msg}"
    );
}

// ---------------------------------------------------------------------------
// remote run (without --follow)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn run_remote_run_without_follow_submits_command_and_returns_ok() {
    let workdir = TempDir::new().unwrap();
    let canonical = std::fs::canonicalize(workdir.path()).unwrap();
    let (_root, base) = start_test_server(vec![canonical.clone()]).await;

    let session_id = run_remote_session_start(&base, canonical.to_str().unwrap())
        .await
        .unwrap();

    let command = vec!["status".to_string()];
    let sink = OutputSink::Null;
    let result = run_remote_run(&base, &session_id, &command, false, &sink).await;

    assert!(
        result.is_ok(),
        "run_remote_run without --follow must return Ok; got: {:?}",
        result
    );
}

#[tokio::test]
async fn run_remote_run_returns_error_for_unknown_session() {
    let (_root, base) = start_test_server(vec![]).await;

    let command = vec!["status".to_string()];
    let sink = OutputSink::Null;
    let result = run_remote_run(&base, "no-such-session-xyz", &command, false, &sink).await;

    assert!(result.is_err(), "unknown session must cause an error");
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("not found") || msg.contains("no-such-session-xyz"),
        "error must identify the missing session; got: {msg}"
    );
}

#[tokio::test]
async fn run_remote_run_returns_error_for_busy_session() {
    let workdir = TempDir::new().unwrap();
    let canonical = std::fs::canonicalize(workdir.path()).unwrap();
    let (root_dir, base) = start_test_server(vec![canonical.clone()]).await;

    let session_id = run_remote_session_start(&base, canonical.to_str().unwrap())
        .await
        .unwrap();

    // Seed a "pending" command for the session directly in the DB via a
    // separate connection.  The server's DB-level guard checks for pending/
    // running commands and returns 403, making this approach race-free.
    {
        let log_path = root_dir
            .path()
            .join("sessions")
            .join(&session_id)
            .join("commands")
            .join("cmd-fake-pending")
            .join("output.log");
        let conn = db::open_db(root_dir.path()).unwrap();
        db::insert_command(
            &conn,
            "cmd-fake-pending",
            &session_id,
            "status",
            "[]",
            log_path.to_str().unwrap(),
        )
        .unwrap();
        // Status stays "pending" — no update_command_started call.
    }

    let command = vec!["status".to_string()];
    let sink = OutputSink::Null;
    let result = run_remote_run(&base, &session_id, &command, false, &sink).await;

    assert!(result.is_err(), "command on a busy session must be rejected");
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("already has a running command") || msg.contains("403"),
        "error must describe the busy session; got: {msg}"
    );
}

// ---------------------------------------------------------------------------
// stream_command_logs
// ---------------------------------------------------------------------------

#[tokio::test]
async fn stream_command_logs_delivers_log_lines_and_returns_before_sentinel() {
    let workdir = TempDir::new().unwrap();
    let canonical = std::fs::canonicalize(workdir.path()).unwrap();
    let (root_dir, base) = start_test_server(vec![canonical.clone()]).await;

    let session_id = "sess-remote-stream";
    let command_id = "cmd-remote-stream-test";
    seed_done_command(
        &root_dir,
        &canonical,
        session_id,
        command_id,
        "line one\nline two\n",
    )
    .await;

    // Capture streamed lines via OutputSink::Channel.
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    let sink = OutputSink::Channel(tx);
    stream_command_logs(&base, command_id, &sink).await.unwrap();
    drop(sink); // close the sender

    // Drain all buffered messages.
    let mut lines: Vec<String> = Vec::new();
    while let Ok(line) = rx.try_recv() {
        lines.push(line);
    }
    let all_output = lines.join("\n");

    assert!(
        all_output.contains("line one"),
        "streamed output must include 'line one'; got: {all_output:?}"
    );
    assert!(
        all_output.contains("line two"),
        "streamed output must include 'line two'; got: {all_output:?}"
    );
    // The [amux:done] sentinel terminates the stream loop; it must NOT be
    // forwarded to the sink.
    assert!(
        !all_output.contains("[amux:done]"),
        "the [amux:done] sentinel must not appear in sink output; got: {all_output:?}"
    );
}

#[tokio::test]
async fn stream_command_logs_returns_error_for_unknown_command_id() {
    let (_root, base) = start_test_server(vec![]).await;

    let sink = OutputSink::Null;
    let result = stream_command_logs(&base, "no-such-command-xyz", &sink).await;

    assert!(result.is_err(), "unknown command ID must return an error");
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("404") || msg.contains("SSE stream returned"),
        "error must describe the failure; got: {msg}"
    );
}

// ---------------------------------------------------------------------------
// fetch_sessions
// ---------------------------------------------------------------------------

#[tokio::test]
async fn fetch_sessions_returns_empty_list_when_no_sessions_exist() {
    let (_root, base) = start_test_server(vec![]).await;

    let sessions = fetch_sessions(&base).await.unwrap();
    assert!(
        sessions.is_empty(),
        "fresh server must have no active sessions; got: {sessions:?}"
    );
}

#[tokio::test]
async fn fetch_sessions_lists_active_sessions() {
    let workdir = TempDir::new().unwrap();
    let canonical = std::fs::canonicalize(workdir.path()).unwrap();
    let (_root, base) = start_test_server(vec![canonical.clone()]).await;

    let session_id = run_remote_session_start(&base, canonical.to_str().unwrap())
        .await
        .unwrap();

    let sessions = fetch_sessions(&base).await.unwrap();
    assert_eq!(sessions.len(), 1, "exactly one session must be listed");
    assert_eq!(
        sessions[0].id, session_id,
        "listed session ID must match the created one"
    );
    assert_eq!(
        sessions[0].workdir,
        canonical.to_str().unwrap(),
        "workdir must match"
    );
}

#[tokio::test]
async fn fetch_sessions_includes_killed_sessions_with_their_id() {
    // The /v1/sessions endpoint returns all sessions (active and closed).
    // Closed sessions remain in the list; callers that want only active
    // sessions must filter by status themselves.
    let workdir = TempDir::new().unwrap();
    let canonical = std::fs::canonicalize(workdir.path()).unwrap();
    let (_root, base) = start_test_server(vec![canonical.clone()]).await;

    let session_id = run_remote_session_start(&base, canonical.to_str().unwrap())
        .await
        .unwrap();
    run_remote_session_kill(&base, &session_id).await.unwrap();

    let sessions = fetch_sessions(&base).await.unwrap();
    assert!(
        sessions.iter().any(|s| s.id == session_id),
        "killed session must still appear in the sessions list; got: {sessions:?}"
    );
}
