/// Integration tests for the headless HTTP API (work item 0057).
///
/// Tests spin up an in-process axum server bound to a random OS-assigned port
/// and exercise the full HTTP API via `reqwest`. No Docker daemon is required.
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use amux::commands::headless::db;
use amux::commands::headless::server::{AppState, build_router};
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

/// Starts an in-process headless HTTP server bound to a random port.
///
/// Returns the temp-dir holding the server storage root (keeps it alive for
/// the duration of the test) and the base URL (e.g. `"http://127.0.0.1:PORT"`).
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
    assert!(logs_body["stdout"].is_string());
    assert!(logs_body["stderr"].is_string());

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
