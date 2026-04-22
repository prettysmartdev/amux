/// Integration tests for the headless HTTP API (work item 0057).
///
/// Tests spin up an in-process axum server bound to a random OS-assigned port
/// and exercise the full HTTP API via `reqwest`. No Docker daemon is required.
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use amux::commands::headless::auth;
use amux::commands::headless::db;
use amux::commands::headless::server::{AppState, AuthMode, build_router};
use amux::runtime::{
    AgentRuntime, ContainerStats, HostSettings, StoppedContainerInfo,
};
use tempfile::TempDir;
use tokio::sync::Mutex;

// ---------------------------------------------------------------------------
// Mock runtime (none of the HTTP handlers actually invoke the runtime)
// ---------------------------------------------------------------------------

struct MockRuntime;

impl AgentRuntime for MockRuntime {
    fn is_available(&self) -> bool {
        false
    }
    fn check_socket(&self) -> anyhow::Result<PathBuf> {
        anyhow::bail!("MockRuntime: check_socket not implemented")
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
    fn list_running_containers_with_ids_by_prefix(&self, _prefix: &str) -> Vec<(String, String)> {
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
// Test server helpers
// ---------------------------------------------------------------------------

/// Starts an in-process headless HTTP server with a caller-supplied `AuthMode`.
///
/// Returns the temp-dir (caller must keep alive) and the base URL.
async fn start_test_server_with_auth(workdirs: Vec<PathBuf>, auth_mode: AuthMode) -> (TempDir, String) {
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
        auth_mode,
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

/// Starts an in-process headless HTTP server bound to a random port with auth disabled.
///
/// Returns the temp-dir holding the server storage root (keeps it alive for
/// the duration of the test) and the base URL (e.g. `"http://127.0.0.1:PORT"`).
async fn start_test_server(workdirs: Vec<PathBuf>) -> (TempDir, String) {
    start_test_server_with_auth(workdirs, AuthMode::Disabled).await
}

/// Polls a command endpoint until its status is no longer `"pending"`,
/// or until `timeout` elapses.  Returns the final JSON body.
async fn wait_for_command_completion(
    client: &reqwest::Client,
    url: &str,
    timeout: Duration,
) -> serde_json::Value {
    let deadline = Instant::now() + timeout;
    loop {
        let resp = client.get(url).send().await.unwrap();
        let body: serde_json::Value = resp.json().await.unwrap();
        let status = body["status"].as_str().unwrap_or("");
        if status == "done" || status == "error" {
            return body;
        }
        if Instant::now() >= deadline {
            panic!("Timed out waiting for command to reach terminal state. Last status: '{status}', body: {body}");
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

// ---------------------------------------------------------------------------
// Status endpoint
// ---------------------------------------------------------------------------

#[tokio::test]
async fn status_endpoint_returns_ok_shape() {
    let (_root, base) = start_test_server(vec![]).await;
    let client = reqwest::Client::new();

    let resp = client
        .get(format!("{base}/v1/status"))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();

    assert_eq!(body["status"], "ok");
    assert!(body["pid"].is_number(), "pid must be a number");
    assert!(body["uptime_seconds"].is_number(), "uptime_seconds must be a number");
    assert_eq!(body["active_sessions"], 0);
    assert_eq!(body["running_commands"], 0);
}

// ---------------------------------------------------------------------------
// Workdirs endpoint
// ---------------------------------------------------------------------------

#[tokio::test]
async fn workdirs_endpoint_lists_allowlisted_directories() {
    let workdir = TempDir::new().unwrap();
    let canonical = std::fs::canonicalize(workdir.path()).unwrap();
    let (_root, base) = start_test_server(vec![canonical.clone()]).await;
    let client = reqwest::Client::new();

    let resp = client
        .get(format!("{base}/v1/workdirs"))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    let dirs = body["workdirs"].as_array().expect("workdirs must be an array");
    assert_eq!(dirs.len(), 1);
    assert_eq!(dirs[0].as_str().unwrap(), canonical.to_str().unwrap());
}

#[tokio::test]
async fn workdirs_endpoint_returns_empty_array_when_no_allowlist() {
    let (_root, base) = start_test_server(vec![]).await;
    let client = reqwest::Client::new();

    let resp = client
        .get(format!("{base}/v1/workdirs"))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["workdirs"], serde_json::json!([]));
}

// ---------------------------------------------------------------------------
// Session lifecycle
// ---------------------------------------------------------------------------

#[tokio::test]
async fn create_session_returns_201_and_session_id() {
    let workdir = TempDir::new().unwrap();
    let canonical = std::fs::canonicalize(workdir.path()).unwrap();
    let (_root, base) = start_test_server(vec![canonical.clone()]).await;
    let client = reqwest::Client::new();

    let resp = client
        .post(format!("{base}/v1/sessions"))
        .json(&serde_json::json!({ "workdir": canonical.to_str().unwrap() }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 201);
    let body: serde_json::Value = resp.json().await.unwrap();
    let session_id = body["session_id"].as_str().expect("session_id must be a string");
    assert!(!session_id.is_empty());
}

#[tokio::test]
async fn get_session_returns_active_session_fields() {
    let workdir = TempDir::new().unwrap();
    let canonical = std::fs::canonicalize(workdir.path()).unwrap();
    let (_root, base) = start_test_server(vec![canonical.clone()]).await;
    let client = reqwest::Client::new();

    // Create
    let create_resp: serde_json::Value = client
        .post(format!("{base}/v1/sessions"))
        .json(&serde_json::json!({ "workdir": canonical.to_str().unwrap() }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let session_id = create_resp["session_id"].as_str().unwrap().to_string();

    // Fetch
    let resp = client
        .get(format!("{base}/v1/sessions/{session_id}"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();

    assert_eq!(body["id"], session_id.as_str());
    assert_eq!(body["status"], "active");
    assert!(body["created_at"].is_string());
    assert!(body["closed_at"].is_null() || !body.as_object().unwrap().contains_key("closed_at"));
}

#[tokio::test]
async fn list_sessions_includes_created_session() {
    let workdir = TempDir::new().unwrap();
    let canonical = std::fs::canonicalize(workdir.path()).unwrap();
    let (_root, base) = start_test_server(vec![canonical.clone()]).await;
    let client = reqwest::Client::new();

    // Create two sessions
    let mut created_ids = Vec::new();
    for _ in 0..2 {
        let r: serde_json::Value = client
            .post(format!("{base}/v1/sessions"))
            .json(&serde_json::json!({ "workdir": canonical.to_str().unwrap() }))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        created_ids.push(r["session_id"].as_str().unwrap().to_string());
    }

    let list_resp = client
        .get(format!("{base}/v1/sessions"))
        .send()
        .await
        .unwrap();
    assert_eq!(list_resp.status(), 200);
    let body: serde_json::Value = list_resp.json().await.unwrap();
    let sessions = body["sessions"].as_array().unwrap();
    let ids: Vec<&str> = sessions.iter().map(|s| s["id"].as_str().unwrap()).collect();
    for created_id in &created_ids {
        assert!(ids.contains(&created_id.as_str()), "created session {created_id} must appear in list");
    }
}

#[tokio::test]
async fn get_session_returns_404_for_unknown_id() {
    let (_root, base) = start_test_server(vec![]).await;
    let client = reqwest::Client::new();

    let resp = client
        .get(format!("{base}/v1/sessions/does-not-exist"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn close_session_sets_status_to_closed() {
    let workdir = TempDir::new().unwrap();
    let canonical = std::fs::canonicalize(workdir.path()).unwrap();
    let (_root, base) = start_test_server(vec![canonical.clone()]).await;
    let client = reqwest::Client::new();

    let create: serde_json::Value = client
        .post(format!("{base}/v1/sessions"))
        .json(&serde_json::json!({ "workdir": canonical.to_str().unwrap() }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let session_id = create["session_id"].as_str().unwrap().to_string();

    let delete_resp = client
        .delete(format!("{base}/v1/sessions/{session_id}"))
        .send()
        .await
        .unwrap();
    assert_eq!(delete_resp.status(), 200);
    let body: serde_json::Value = delete_resp.json().await.unwrap();
    assert_eq!(body["status"], "closed");
    assert!(body["closed_at"].is_string());
}

#[tokio::test]
async fn close_session_returns_404_for_unknown_id() {
    let (_root, base) = start_test_server(vec![]).await;
    let client = reqwest::Client::new();

    let resp = client
        .delete(format!("{base}/v1/sessions/does-not-exist"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}

// ---------------------------------------------------------------------------
// Allowlist enforcement
// ---------------------------------------------------------------------------

#[tokio::test]
async fn create_session_with_non_allowlisted_workdir_returns_403() {
    let allowed_dir = TempDir::new().unwrap();
    let allowed_canonical = std::fs::canonicalize(allowed_dir.path()).unwrap();
    let (_root, base) = start_test_server(vec![allowed_canonical]).await;
    let client = reqwest::Client::new();

    // Use a different existing directory that is not in the allowlist.
    let other_dir = TempDir::new().unwrap();
    let other_canonical = std::fs::canonicalize(other_dir.path()).unwrap();

    let resp = client
        .post(format!("{base}/v1/sessions"))
        .json(&serde_json::json!({ "workdir": other_canonical.to_str().unwrap() }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 403, "non-allowlisted workdir must be rejected with 403");
    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(body["error"].is_string(), "error field must be present in 403 response");
}

#[tokio::test]
async fn create_session_with_nonexistent_path_returns_400() {
    let (_root, base) = start_test_server(vec![]).await;
    let client = reqwest::Client::new();

    let resp = client
        .post(format!("{base}/v1/sessions"))
        .json(&serde_json::json!({ "workdir": "/this/path/definitely/does/not/exist/xyz123" }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 400);
}

// ---------------------------------------------------------------------------
// Full session + command lifecycle
// ---------------------------------------------------------------------------

#[tokio::test]
async fn full_session_command_lifecycle() {
    let workdir = TempDir::new().unwrap();
    let canonical = std::fs::canonicalize(workdir.path()).unwrap();
    let (root_dir, base) = start_test_server(vec![canonical.clone()]).await;
    let client = reqwest::Client::new();

    // Step 1 — create session.
    let create_resp: serde_json::Value = client
        .post(format!("{base}/v1/sessions"))
        .json(&serde_json::json!({ "workdir": canonical.to_str().unwrap() }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let session_id = create_resp["session_id"].as_str().unwrap().to_string();

    // Assert DB state matches HTTP response.
    {
        let conn = db::open_db(root_dir.path()).unwrap();
        let row = db::get_session(&conn, &session_id)
            .unwrap()
            .expect("session must exist in DB");
        assert_eq!(row.status, "active");
        assert_eq!(row.workdir, canonical.to_str().unwrap());
    }

    // Step 2 — submit a command ("status" exits quickly in any environment).
    let cmd_resp = client
        .post(format!("{base}/v1/commands"))
        .header("x-amux-session", &session_id)
        .json(&serde_json::json!({
            "subcommand": "status",
            "args": []
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(cmd_resp.status(), 202, "command creation must return 202 Accepted");
    let cmd_body: serde_json::Value = cmd_resp.json().await.unwrap();
    let command_id = cmd_body["command_id"].as_str().unwrap().to_string();

    // Step 3 — poll until the command finishes (it may succeed or error; either is fine).
    let final_cmd = wait_for_command_completion(
        &client,
        &format!("{base}/v1/commands/{command_id}"),
        Duration::from_secs(10),
    )
    .await;
    let final_status = final_cmd["status"].as_str().unwrap();
    assert!(
        final_status == "done" || final_status == "error",
        "command must reach a terminal state; got '{final_status}'"
    );

    // Step 4 — retrieve logs (endpoint must respond even if output files are empty).
    let logs_resp = client
        .get(format!("{base}/v1/commands/{command_id}/logs"))
        .send()
        .await
        .unwrap();
    assert_eq!(logs_resp.status(), 200);
    let logs_body: serde_json::Value = logs_resp.json().await.unwrap();
    assert_eq!(logs_body["command_id"], command_id.as_str());
    // The logs endpoint returns a single combined "output" field (stdout+stderr
    // are merged into one log file by the headless executor).
    assert!(logs_body["output"].is_string(), "logs response must contain an 'output' field");

    // Step 5 — assert DB state matches HTTP response.
    {
        let conn = db::open_db(root_dir.path()).unwrap();
        let row = db::get_command(&conn, &command_id)
            .unwrap()
            .expect("command must exist in DB");
        assert_eq!(row.status, final_status, "DB status must match HTTP response");
        assert_eq!(row.session_id, session_id);
    }

    // Step 6 — close the session.
    let close_resp = client
        .delete(format!("{base}/v1/sessions/{session_id}"))
        .send()
        .await
        .unwrap();
    assert_eq!(close_resp.status(), 200);

    // Assert DB reflects the close.
    {
        let conn = db::open_db(root_dir.path()).unwrap();
        let row = db::get_session(&conn, &session_id).unwrap().unwrap();
        assert_eq!(row.status, "closed");
        assert!(row.closed_at.is_some());
    }
}

// ---------------------------------------------------------------------------
// Command validation
// ---------------------------------------------------------------------------

#[tokio::test]
async fn create_command_without_session_header_returns_400() {
    let (_root, base) = start_test_server(vec![]).await;
    let client = reqwest::Client::new();

    let resp = client
        .post(format!("{base}/v1/commands"))
        // Intentionally omit x-amux-session header.
        .json(&serde_json::json!({ "subcommand": "status", "args": [] }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn create_command_with_unknown_subcommand_returns_400() {
    let workdir = TempDir::new().unwrap();
    let canonical = std::fs::canonicalize(workdir.path()).unwrap();
    let (_root, base) = start_test_server(vec![canonical.clone()]).await;
    let client = reqwest::Client::new();

    let create: serde_json::Value = client
        .post(format!("{base}/v1/sessions"))
        .json(&serde_json::json!({ "workdir": canonical.to_str().unwrap() }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let session_id = create["session_id"].as_str().unwrap();

    let resp = client
        .post(format!("{base}/v1/commands"))
        .header("x-amux-session", session_id)
        .json(&serde_json::json!({ "subcommand": "not-a-real-subcommand", "args": [] }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn create_command_for_nonexistent_session_returns_404() {
    let (_root, base) = start_test_server(vec![]).await;
    let client = reqwest::Client::new();

    let resp = client
        .post(format!("{base}/v1/commands"))
        .header("x-amux-session", "does-not-exist")
        .json(&serde_json::json!({ "subcommand": "status", "args": [] }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn get_command_returns_404_for_unknown_id() {
    let (_root, base) = start_test_server(vec![]).await;
    let client = reqwest::Client::new();

    let resp = client
        .get(format!("{base}/v1/commands/does-not-exist"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}

// ---------------------------------------------------------------------------
// Concurrent command rejection
// ---------------------------------------------------------------------------

#[tokio::test]
async fn second_command_on_busy_session_returns_403() {
    let workdir = TempDir::new().unwrap();
    let canonical = std::fs::canonicalize(workdir.path()).unwrap();
    let (_root, base) = start_test_server(vec![canonical.clone()]).await;
    let client = reqwest::Client::new();

    let create: serde_json::Value = client
        .post(format!("{base}/v1/sessions"))
        .json(&serde_json::json!({ "workdir": canonical.to_str().unwrap() }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let session_id = create["session_id"].as_str().unwrap().to_string();

    // Submit the first command (it will run in the background).
    let first = client
        .post(format!("{base}/v1/commands"))
        .header("x-amux-session", &session_id)
        .json(&serde_json::json!({ "subcommand": "status", "args": [] }))
        .send()
        .await
        .unwrap();
    assert_eq!(first.status(), 202);

    // Immediately submit a second command before the first can finish.
    // The server marks the session busy synchronously before spawning the task,
    // so this second request should always be rejected.
    let second = client
        .post(format!("{base}/v1/commands"))
        .header("x-amux-session", &session_id)
        .json(&serde_json::json!({ "subcommand": "status", "args": [] }))
        .send()
        .await
        .unwrap();
    assert_eq!(
        second.status(),
        403,
        "concurrent command on same session must be rejected with 403"
    );
}

// ---------------------------------------------------------------------------
// Closed session receives a command → 404
// ---------------------------------------------------------------------------

#[tokio::test]
async fn create_command_for_closed_session_returns_404() {
    let workdir = TempDir::new().unwrap();
    let canonical = std::fs::canonicalize(workdir.path()).unwrap();
    let (_root, base) = start_test_server(vec![canonical.clone()]).await;
    let client = reqwest::Client::new();

    // Create a session then immediately close it.
    let create: serde_json::Value = client
        .post(format!("{base}/v1/sessions"))
        .json(&serde_json::json!({ "workdir": canonical.to_str().unwrap() }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let session_id = create["session_id"].as_str().unwrap().to_string();

    let close = client
        .delete(format!("{base}/v1/sessions/{session_id}"))
        .send()
        .await
        .unwrap();
    assert_eq!(close.status(), 200);

    // Submitting a command to the closed session must return 404 with the
    // session UUID present in the error body.
    let resp = client
        .post(format!("{base}/v1/commands"))
        .header("x-amux-session", &session_id)
        .json(&serde_json::json!({ "subcommand": "status", "args": [] }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 404, "closed session must return 404");
    let body: serde_json::Value = resp.json().await.unwrap();
    let error_msg = body["error"].as_str().unwrap_or("");
    assert!(
        error_msg.contains(&session_id),
        "error body must include the session UUID; got: {error_msg}"
    );
}

// ---------------------------------------------------------------------------
// Truly concurrent commands — only one must be accepted
// ---------------------------------------------------------------------------

/// Fires two `POST /v1/commands` requests simultaneously via `tokio::join!`.
/// After fix for the TOCTOU race, the atomic check-and-mark in busy_sessions
/// guarantees exactly one request sees an empty slot and the other sees 403.
#[tokio::test]
async fn truly_concurrent_commands_on_same_session_exactly_one_accepted() {
    let workdir = TempDir::new().unwrap();
    let canonical = std::fs::canonicalize(workdir.path()).unwrap();
    let (_root, base) = start_test_server(vec![canonical.clone()]).await;
    let client = Arc::new(reqwest::Client::new());

    // Create a session.
    let create: serde_json::Value = client
        .post(format!("{base}/v1/sessions"))
        .json(&serde_json::json!({ "workdir": canonical.to_str().unwrap() }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let session_id = Arc::new(create["session_id"].as_str().unwrap().to_string());

    // Fire two requests simultaneously.  tokio::join! drives both futures on the
    // same runtime; because each future yields on the network send, both HTTP
    // requests are in-flight at the server concurrently.
    let make_request = |client: Arc<reqwest::Client>, session_id: Arc<String>, base: String| async move {
        client
            .post(format!("{base}/v1/commands"))
            .header("x-amux-session", session_id.as_str())
            .json(&serde_json::json!({ "subcommand": "status", "args": [] }))
            .send()
            .await
            .unwrap()
    };

    let (res_a, res_b) = tokio::join!(
        make_request(Arc::clone(&client), Arc::clone(&session_id), base.clone()),
        make_request(Arc::clone(&client), Arc::clone(&session_id), base.clone()),
    );

    let statuses = [res_a.status().as_u16(), res_b.status().as_u16()];
    let accepted = statuses.iter().filter(|&&s| s == 202).count();
    let rejected = statuses.iter().filter(|&&s| s == 403).count();

    assert_eq!(
        accepted, 1,
        "exactly one command must be accepted (202); got statuses {:?}",
        statuses
    );
    assert_eq!(
        rejected, 1,
        "exactly one command must be rejected (403); got statuses {:?}",
        statuses
    );
}

// ---------------------------------------------------------------------------
// exec subcommand accepted by is_valid_subcommand (work item 0058)
// ---------------------------------------------------------------------------

/// Submitting `subcommand = "exec"` with args `["prompt", "hello"]` must be
/// accepted (202 Accepted) by the headless server rather than rejected with 400.
/// This verifies that "exec" is present in KNOWN_SUBCOMMANDS.
#[tokio::test]
async fn create_command_with_exec_subcommand_is_accepted_not_rejected() {
    let workdir = TempDir::new().unwrap();
    let canonical = std::fs::canonicalize(workdir.path()).unwrap();
    let (_root, base) = start_test_server(vec![canonical.clone()]).await;
    let client = reqwest::Client::new();

    // Create a session in the allowlisted workdir.
    let create: serde_json::Value = client
        .post(format!("{base}/v1/sessions"))
        .json(&serde_json::json!({ "workdir": canonical.to_str().unwrap() }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let session_id = create["session_id"].as_str().unwrap().to_string();

    // Submit an exec command — the server must accept it (202) rather than
    // rejecting it as an unknown subcommand (400).
    let resp = client
        .post(format!("{base}/v1/commands"))
        .header("x-amux-session", &session_id)
        .json(&serde_json::json!({
            "subcommand": "exec",
            "args": ["prompt", "hello"]
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(
        resp.status(),
        202,
        "exec subcommand must be accepted with 202 (not rejected as unknown); \
         body: {}",
        resp.text().await.unwrap_or_default()
    );
}

// ---------------------------------------------------------------------------
// remote subcommand accepted (work item 0059)
// ---------------------------------------------------------------------------

/// Submitting `subcommand = "remote"` must be accepted (202) rather than
/// rejected (400) — "remote" was added to KNOWN_SUBCOMMANDS in WI 0059.
#[tokio::test]
async fn create_command_with_remote_subcommand_is_accepted_not_rejected() {
    let workdir = TempDir::new().unwrap();
    let canonical = std::fs::canonicalize(workdir.path()).unwrap();
    let (_root, base) = start_test_server(vec![canonical.clone()]).await;
    let client = reqwest::Client::new();

    let create: serde_json::Value = client
        .post(format!("{base}/v1/sessions"))
        .json(&serde_json::json!({ "workdir": canonical.to_str().unwrap() }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let session_id = create["session_id"].as_str().unwrap().to_string();

    let resp = client
        .post(format!("{base}/v1/commands"))
        .header("x-amux-session", &session_id)
        .json(&serde_json::json!({
            "subcommand": "remote",
            "args": ["run", "status"]
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(
        resp.status(),
        202,
        "remote subcommand must be accepted with 202; body: {}",
        resp.text().await.unwrap_or_default()
    );
}

// ---------------------------------------------------------------------------
// SSE log streaming endpoint (work item 0059)
// ---------------------------------------------------------------------------

/// Seed a "done" command into the same DB the test server uses (WAL mode allows
/// multiple connections to the same file), then exercise the SSE endpoint.
///
/// Uses caller-supplied IDs so the caller can later construct the endpoint URL.
/// Each test creates a fresh isolated DB via `start_test_server`, so fixed IDs
/// are safe across concurrent test runs.
async fn seed_done_command(
    root_dir: &TempDir,
    workdir_canonical: &std::path::Path,
    session_id: &str,
    command_id: &str,
) {
    let cmd_dir = root_dir
        .path()
        .join("sessions")
        .join(session_id)
        .join("commands")
        .join(command_id);
    std::fs::create_dir_all(&cmd_dir).unwrap();
    let log_path = cmd_dir.join("output.log");
    std::fs::write(&log_path, "hello from log\nworld\n").unwrap();

    let conn = db::open_db(root_dir.path()).unwrap();
    db::insert_session(&conn, session_id, workdir_canonical.to_str().unwrap(), "2024-01-01T00:00:00Z")
        .unwrap();
    db::insert_command(&conn, command_id, session_id, "status", "[]", log_path.to_str().unwrap())
        .unwrap();
    db::update_command_started(&conn, command_id, "2024-01-01T00:00:01Z").unwrap();
    db::update_command_finished(&conn, command_id, "done", Some(0), "2024-01-01T00:00:02Z").unwrap();
}

#[tokio::test]
async fn sse_returns_404_for_unknown_command_id() {
    let (_root, base) = start_test_server(vec![]).await;
    let client = reqwest::Client::new();

    let resp = client
        .get(format!("{base}/v1/commands/no-such-command-id/logs/stream"))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn sse_endpoint_returns_text_event_stream_content_type() {
    let workdir = TempDir::new().unwrap();
    let canonical = std::fs::canonicalize(workdir.path()).unwrap();
    let (root_dir, base) = start_test_server(vec![canonical.clone()]).await;
    let client = reqwest::Client::new();

    let command_id = "cmd-sse-content-type-test";
    seed_done_command(&root_dir, &canonical, "sess-sse-ct", command_id).await;

    let resp = client
        .get(format!("{base}/v1/commands/{command_id}/logs/stream"))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let ct = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(
        ct.contains("text/event-stream"),
        "SSE endpoint must return Content-Type: text/event-stream; got: {ct:?}"
    );
}

#[tokio::test]
async fn sse_done_command_sends_amux_done_sentinel() {
    let workdir = TempDir::new().unwrap();
    let canonical = std::fs::canonicalize(workdir.path()).unwrap();
    let (root_dir, base) = start_test_server(vec![canonical.clone()]).await;
    let client = reqwest::Client::new();

    let command_id = "cmd-sse-sentinel-test";
    seed_done_command(&root_dir, &canonical, "sess-sse-sentinel", command_id).await;

    let resp = client
        .get(format!("{base}/v1/commands/{command_id}/logs/stream"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let body = resp.text().await.unwrap();
    assert!(
        body.contains("[amux:done]"),
        "SSE stream for a completed command must contain the [amux:done] sentinel; got: {body:?}"
    );
}

#[tokio::test]
async fn sse_done_command_includes_log_content_before_sentinel() {
    let workdir = TempDir::new().unwrap();
    let canonical = std::fs::canonicalize(workdir.path()).unwrap();
    let (root_dir, base) = start_test_server(vec![canonical.clone()]).await;
    let client = reqwest::Client::new();

    let command_id = "cmd-sse-log-content-test";
    seed_done_command(&root_dir, &canonical, "sess-sse-log", command_id).await;

    let resp = client
        .get(format!("{base}/v1/commands/{command_id}/logs/stream"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let body = resp.text().await.unwrap();
    assert!(
        body.contains("hello from log"),
        "SSE stream must include existing log content before [amux:done]; got: {body:?}"
    );
    // Sentinel must appear after the log lines.
    let done_pos = body.find("[amux:done]").expect("must contain [amux:done]");
    let log_pos = body.find("hello from log").expect("must contain log content");
    assert!(
        log_pos < done_pos,
        "log content must appear before [amux:done] sentinel"
    );
}

// ---------------------------------------------------------------------------
// Auth middleware (work item 0060)
// ---------------------------------------------------------------------------

/// Generate a test API key and its hash, then spin up a server with AuthMode::Enabled.
/// Returns (root_dir, base_url, plaintext_key).
async fn start_authed_server() -> (TempDir, String, String) {
    let plaintext_key = auth::generate_api_key().unwrap();
    let key_hash = auth::hash_api_key(&plaintext_key);
    let (root_dir, base_url) = start_test_server_with_auth(
        vec![],
        AuthMode::Enabled { key_hash },
    )
    .await;
    (root_dir, base_url, plaintext_key)
}

#[tokio::test]
async fn auth_middleware_correct_key_returns_200() {
    let (_root, base, key) = start_authed_server().await;
    let client = reqwest::Client::new();

    let resp = client
        .get(format!("{base}/v1/status"))
        .header("Authorization", format!("Bearer {key}"))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200, "correct Bearer key must yield 200");
}

#[tokio::test]
async fn auth_middleware_no_header_returns_401_with_expected_message() {
    let (_root, base, _key) = start_authed_server().await;
    let client = reqwest::Client::new();

    let resp = client
        .get(format!("{base}/v1/status"))
        // No Authorization header.
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 401, "missing header must yield 401");
    let body: serde_json::Value = resp.json().await.unwrap();
    let msg = body["error"].as_str().unwrap_or("");
    assert!(
        msg.contains("API key required") || msg.contains("Authorization"),
        "error message must guide the caller; got: {msg}"
    );
}

#[tokio::test]
async fn auth_middleware_wrong_key_returns_401() {
    let (_root, base, _key) = start_authed_server().await;
    let client = reqwest::Client::new();

    let resp = client
        .get(format!("{base}/v1/status"))
        .header("Authorization", "Bearer wrong-key-value")
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 401, "wrong key must yield 401");
    let body: serde_json::Value = resp.json().await.unwrap();
    let msg = body["error"].as_str().unwrap_or("");
    assert!(
        msg.contains("Invalid API key") || msg.contains("invalid"),
        "error message must say the key is wrong; got: {msg}"
    );
}

#[tokio::test]
async fn auth_middleware_bearer_prefix_accepted() {
    let (_root, base, key) = start_authed_server().await;
    let client = reqwest::Client::new();

    let resp = client
        .get(format!("{base}/v1/status"))
        .header("Authorization", format!("Bearer {key}"))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200, "Bearer <key> must be accepted");
}

#[tokio::test]
async fn auth_middleware_bare_key_accepted() {
    let (_root, base, key) = start_authed_server().await;
    let client = reqwest::Client::new();

    let resp = client
        .get(format!("{base}/v1/status"))
        .header("Authorization", key.clone())
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200, "bare key (no 'Bearer' prefix) must be accepted");
}

#[tokio::test]
async fn auth_middleware_mixed_case_bearer_prefix_accepted() {
    let (_root, base, key) = start_authed_server().await;
    let client = reqwest::Client::new();

    // "BEARER" (all-caps) must be treated the same as "Bearer" — the prefix
    // stripping is case-insensitive per spec.
    let resp = client
        .get(format!("{base}/v1/status"))
        .header("Authorization", format!("BEARER {key}"))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200, "BEARER (uppercase) prefix must be accepted (case-insensitive)");
}

#[tokio::test]
async fn auth_mode_disabled_accepts_without_auth() {
    // start_test_server uses AuthMode::Disabled.
    let (_root, base) = start_test_server(vec![]).await;
    let client = reqwest::Client::new();

    // Send with no Authorization header — must succeed when auth is disabled.
    let resp = client
        .get(format!("{base}/v1/status"))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200, "AuthMode::Disabled must accept requests without any auth");
}

// ---------------------------------------------------------------------------
// Session status filter (work item 0060)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn list_sessions_status_active_filter_returns_only_active() {
    let workdir = TempDir::new().unwrap();
    let canonical = std::fs::canonicalize(workdir.path()).unwrap();
    let (root_dir, base) = start_test_server(vec![canonical.clone()]).await;
    let client = reqwest::Client::new();

    let workdir_str = canonical.to_str().unwrap().to_string();

    // Create session 1 (will stay active).
    let id_active: String = {
        let r: serde_json::Value = client
            .post(format!("{base}/v1/sessions"))
            .json(&serde_json::json!({ "workdir": workdir_str }))
            .send().await.unwrap().json().await.unwrap();
        r["session_id"].as_str().unwrap().to_string()
    };

    // Create session 2 (will be closed).
    let id_to_close: String = {
        let r: serde_json::Value = client
            .post(format!("{base}/v1/sessions"))
            .json(&serde_json::json!({ "workdir": workdir_str }))
            .send().await.unwrap().json().await.unwrap();
        r["session_id"].as_str().unwrap().to_string()
    };

    // Close the second session.
    client
        .delete(format!("{base}/v1/sessions/{id_to_close}"))
        .send()
        .await
        .unwrap();

    // Verify in DB.
    {
        let conn = db::open_db(root_dir.path()).unwrap();
        let active = db::list_sessions_by_status(&conn, Some("active")).unwrap();
        let active_ids: Vec<&str> = active.iter().map(|s| s.id.as_str()).collect();
        assert!(active_ids.contains(&id_active.as_str()), "active session must appear in DB active list");
        assert!(!active_ids.contains(&id_to_close.as_str()), "closed session must NOT appear in DB active list");
    }

    // Verify via HTTP with ?status=active.
    let resp = client
        .get(format!("{base}/v1/sessions?status=active"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    let sessions = body["sessions"].as_array().unwrap();
    let ids: Vec<&str> = sessions.iter().map(|s| s["id"].as_str().unwrap()).collect();

    assert!(
        ids.contains(&id_active.as_str()),
        "active session must appear in ?status=active response; got: {ids:?}"
    );
    assert!(
        !ids.contains(&id_to_close.as_str()),
        "closed session must NOT appear in ?status=active response; got: {ids:?}"
    );
}

#[tokio::test]
async fn list_sessions_status_closed_filter_returns_only_closed() {
    let workdir = TempDir::new().unwrap();
    let canonical = std::fs::canonicalize(workdir.path()).unwrap();
    let (_root, base) = start_test_server(vec![canonical.clone()]).await;
    let client = reqwest::Client::new();

    let workdir_str = canonical.to_str().unwrap().to_string();

    // Create two sessions.
    let id_open: String = {
        let r: serde_json::Value = client
            .post(format!("{base}/v1/sessions"))
            .json(&serde_json::json!({ "workdir": workdir_str }))
            .send().await.unwrap().json().await.unwrap();
        r["session_id"].as_str().unwrap().to_string()
    };

    let id_closed: String = {
        let r: serde_json::Value = client
            .post(format!("{base}/v1/sessions"))
            .json(&serde_json::json!({ "workdir": workdir_str }))
            .send().await.unwrap().json().await.unwrap();
        r["session_id"].as_str().unwrap().to_string()
    };

    // Close the second session.
    client
        .delete(format!("{base}/v1/sessions/{id_closed}"))
        .send()
        .await
        .unwrap();

    let resp = client
        .get(format!("{base}/v1/sessions?status=closed"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    let sessions = body["sessions"].as_array().unwrap();
    let ids: Vec<&str> = sessions.iter().map(|s| s["id"].as_str().unwrap()).collect();

    assert!(
        ids.contains(&id_closed.as_str()),
        "closed session must appear in ?status=closed response; got: {ids:?}"
    );
    assert!(
        !ids.contains(&id_open.as_str()),
        "open session must NOT appear in ?status=closed response; got: {ids:?}"
    );
}

#[tokio::test]
async fn list_sessions_no_filter_returns_all() {
    let workdir = TempDir::new().unwrap();
    let canonical = std::fs::canonicalize(workdir.path()).unwrap();
    let (_root, base) = start_test_server(vec![canonical.clone()]).await;
    let client = reqwest::Client::new();

    let workdir_str = canonical.to_str().unwrap().to_string();

    let id_open: String = {
        let r: serde_json::Value = client
            .post(format!("{base}/v1/sessions"))
            .json(&serde_json::json!({ "workdir": workdir_str }))
            .send().await.unwrap().json().await.unwrap();
        r["session_id"].as_str().unwrap().to_string()
    };

    let id_closed: String = {
        let r: serde_json::Value = client
            .post(format!("{base}/v1/sessions"))
            .json(&serde_json::json!({ "workdir": workdir_str }))
            .send().await.unwrap().json().await.unwrap();
        r["session_id"].as_str().unwrap().to_string()
    };

    // Close second session.
    client
        .delete(format!("{base}/v1/sessions/{id_closed}"))
        .send()
        .await
        .unwrap();

    // No ?status filter → all sessions.
    let resp = client
        .get(format!("{base}/v1/sessions"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    let sessions = body["sessions"].as_array().unwrap();
    let ids: Vec<&str> = sessions.iter().map(|s| s["id"].as_str().unwrap()).collect();

    assert!(
        ids.contains(&id_open.as_str()),
        "open session must appear when no filter; got: {ids:?}"
    );
    assert!(
        ids.contains(&id_closed.as_str()),
        "closed session must appear when no filter; got: {ids:?}"
    );
}
