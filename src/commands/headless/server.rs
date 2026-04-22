use anyhow::{Context, Result};
use axum::{
    extract::{Path as AxumPath, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Mutex;
use tower_http::trace::TraceLayer;

use super::db;

/// Shared server state accessible from all handlers.
pub struct AppState {
    pub db: Mutex<Connection>,
    pub workdirs: Vec<PathBuf>,
    pub headless_root: PathBuf,
    pub started_at: Instant,
    pub runtime: Arc<dyn crate::runtime::AgentRuntime>,
    /// Tracks sessions that currently have a running/pending command.
    pub busy_sessions: Mutex<HashSet<String>>,
    /// Handles for spawned command-execution tasks; drained on graceful shutdown.
    pub task_handles: Mutex<Vec<tokio::task::JoinHandle<()>>>,
}

/// Build the axum router with all headless API routes.
pub fn build_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/v1/status", get(handle_status))
        .route("/v1/workdirs", get(handle_workdirs))
        .route("/v1/sessions", get(handle_list_sessions).post(handle_create_session))
        .route("/v1/sessions/:id", get(handle_get_session).delete(handle_close_session))
        .route("/v1/commands", post(handle_create_command))
        .route("/v1/commands/:id", get(handle_get_command))
        .route("/v1/commands/:id/logs", get(handle_get_command_logs))
        .route("/v1/commands/:id/logs/stream", get(handle_stream_command_logs))
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

// ---------------------------------------------------------------------------
// Request / Response types
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct CreateSessionRequest {
    workdir: String,
}

#[derive(Serialize)]
struct CreateSessionResponse {
    session_id: String,
}

#[derive(Serialize)]
struct SessionResponse {
    id: String,
    workdir: String,
    created_at: String,
    status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    closed_at: Option<String>,
}

#[derive(Deserialize)]
struct CreateCommandRequest {
    subcommand: String,
    args: Vec<String>,
}

#[derive(Serialize)]
struct CreateCommandResponse {
    command_id: String,
}

#[derive(Serialize)]
struct CommandResponse {
    id: String,
    session_id: String,
    subcommand: String,
    args: serde_json::Value,
    status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    exit_code: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    started_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    finished_at: Option<String>,
    log_path: String,
}

#[derive(Serialize)]
struct StatusResponse {
    status: String,
    pid: u32,
    uptime_seconds: u64,
    active_sessions: i64,
    running_commands: i64,
}

#[derive(Serialize)]
struct ErrorResponse {
    error: String,
}

fn error_json(msg: impl Into<String>) -> Json<ErrorResponse> {
    Json(ErrorResponse {
        error: msg.into(),
    })
}

// ---------------------------------------------------------------------------
// Known subcommands that can be dispatched
// ---------------------------------------------------------------------------

const KNOWN_SUBCOMMANDS: &[&str] = &[
    "implement", "chat", "ready", "init", "status", "specs", "config", "exec", "remote",
];

fn is_valid_subcommand(name: &str) -> bool {
    KNOWN_SUBCOMMANDS.contains(&name)
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

async fn handle_status(
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let db = state.db.lock().await;
    let active_sessions = db::count_active_sessions(&db).unwrap_or(0);
    let running_commands = db::count_running_commands(&db).unwrap_or(0);
    let uptime = state.started_at.elapsed().as_secs();

    Json(StatusResponse {
        status: "ok".to_string(),
        pid: std::process::id(),
        uptime_seconds: uptime,
        active_sessions,
        running_commands,
    })
}

async fn handle_workdirs(
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let dirs: Vec<String> = state.workdirs.iter().map(|p| p.display().to_string()).collect();
    Json(serde_json::json!({ "workdirs": dirs }))
}

async fn handle_create_session(
    State(state): State<Arc<AppState>>,
    Json(body): Json<CreateSessionRequest>,
) -> impl IntoResponse {
    // Canonicalize the requested workdir.
    let requested = match std::fs::canonicalize(&body.workdir) {
        Ok(p) => p,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                error_json(format!("Cannot resolve path: {}", body.workdir)),
            )
                .into_response();
        }
    };

    // Check allowlist.
    if !state.workdirs.iter().any(|allowed| *allowed == requested) {
        let allowed: Vec<String> = state.workdirs.iter().map(|p| p.display().to_string()).collect();
        return (
            StatusCode::FORBIDDEN,
            error_json(format!(
                "Workdir '{}' is not in the allowlist. Allowed: {:?}",
                requested.display(),
                allowed
            )),
        )
            .into_response();
    }

    let session_id = uuid::Uuid::new_v4().to_string();
    let created_at = chrono::Utc::now().to_rfc3339();

    // Create session directory structure using async I/O.
    let session_dir = state
        .headless_root
        .join("sessions")
        .join(&session_id);
    if let Err(e) = tokio::fs::create_dir_all(session_dir.join("commands")).await {
        tracing::error!(error = %e, "Failed to create session directory");
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            error_json("Failed to create session directory"),
        )
            .into_response();
    }
    let _ = tokio::fs::create_dir_all(session_dir.join("worktree")).await;
    let _ = tokio::fs::create_dir_all(session_dir.join("agent-settings")).await;

    let db = state.db.lock().await;
    if let Err(e) = db::insert_session(&db, &session_id, &requested.to_string_lossy(), &created_at) {
        tracing::error!(error = %e, "Failed to insert session into DB");
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            error_json("Failed to create session"),
        )
            .into_response();
    }

    tracing::info!(session_id = %session_id, workdir = %requested.display(), "Session created");

    (
        StatusCode::CREATED,
        Json(CreateSessionResponse { session_id }),
    )
        .into_response()
}

async fn handle_list_sessions(
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let db = state.db.lock().await;
    match db::list_sessions(&db) {
        Ok(sessions) => {
            let list: Vec<SessionResponse> = sessions
                .into_iter()
                .map(|s| SessionResponse {
                    id: s.id,
                    workdir: s.workdir,
                    created_at: s.created_at,
                    status: s.status,
                    closed_at: s.closed_at,
                })
                .collect();
            Json(serde_json::json!({ "sessions": list })).into_response()
        }
        Err(e) => {
            tracing::error!(error = %e, "Failed to list sessions");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                error_json("Failed to list sessions"),
            )
                .into_response()
        }
    }
}

async fn handle_get_session(
    State(state): State<Arc<AppState>>,
    AxumPath(id): AxumPath<String>,
) -> impl IntoResponse {
    let db = state.db.lock().await;
    match db::get_session(&db, &id) {
        Ok(Some(s)) => Json(SessionResponse {
            id: s.id,
            workdir: s.workdir,
            created_at: s.created_at,
            status: s.status,
            closed_at: s.closed_at,
        })
        .into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            error_json(format!("Session '{}' not found", id)),
        )
            .into_response(),
        Err(e) => {
            tracing::error!(error = %e, "Failed to get session");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                error_json("Failed to get session"),
            )
                .into_response()
        }
    }
}

async fn handle_close_session(
    State(state): State<Arc<AppState>>,
    AxumPath(id): AxumPath<String>,
) -> impl IntoResponse {
    let closed_at = chrono::Utc::now().to_rfc3339();
    let db = state.db.lock().await;

    // First check if session exists.
    match db::get_session(&db, &id) {
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                error_json(format!("Session '{}' not found", id)),
            )
                .into_response();
        }
        Err(e) => {
            tracing::error!(error = %e, "Failed to get session");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                error_json("Failed to close session"),
            )
                .into_response();
        }
        Ok(Some(s)) if s.status == "closed" => {
            return (
                StatusCode::OK,
                Json(SessionResponse {
                    id: s.id,
                    workdir: s.workdir,
                    created_at: s.created_at,
                    status: s.status,
                    closed_at: s.closed_at,
                }),
            )
                .into_response();
        }
        Ok(Some(_)) => {} // active — proceed to close
    }

    match db::close_session(&db, &id, &closed_at) {
        Ok(true) => {
            tracing::info!(session_id = %id, "Session closed");
            match db::get_session(&db, &id) {
                Ok(Some(s)) => Json(SessionResponse {
                    id: s.id,
                    workdir: s.workdir,
                    created_at: s.created_at,
                    status: s.status,
                    closed_at: s.closed_at,
                })
                .into_response(),
                _ => StatusCode::NO_CONTENT.into_response(),
            }
        }
        Ok(false) => (
            StatusCode::NOT_FOUND,
            error_json(format!("Session '{}' not found or already closed", id)),
        )
            .into_response(),
        Err(e) => {
            tracing::error!(error = %e, "Failed to close session");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                error_json("Failed to close session"),
            )
                .into_response()
        }
    }
}

async fn handle_create_command(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<CreateCommandRequest>,
) -> impl IntoResponse {
    // Extract session ID from header.
    let session_id = match headers.get("x-amux-session") {
        Some(val) => match val.to_str() {
            Ok(s) => s.to_string(),
            Err(_) => {
                return (
                    StatusCode::BAD_REQUEST,
                    error_json("Invalid x-amux-session header value"),
                )
                    .into_response();
            }
        },
        None => {
            return (
                StatusCode::BAD_REQUEST,
                error_json("Missing required header: x-amux-session"),
            )
                .into_response();
        }
    };

    // Validate subcommand name.
    if !is_valid_subcommand(&body.subcommand) {
        return (
            StatusCode::BAD_REQUEST,
            error_json(format!(
                "Unknown subcommand '{}'. Valid subcommands: {:?}",
                body.subcommand, KNOWN_SUBCOMMANDS
            )),
        )
            .into_response();
    }

    // DB check: validate session status and catch pending/running commands left over
    // from a previous crash (crash-recovery path).
    let workdir;
    {
        let db = state.db.lock().await;
        match db::get_session(&db, &session_id) {
            Ok(Some(s)) if s.status == "active" => {
                workdir = s.workdir.clone();
            }
            Ok(Some(_)) => {
                return (
                    StatusCode::NOT_FOUND,
                    error_json(format!("Session '{}' is closed", session_id)),
                )
                    .into_response();
            }
            Ok(None) => {
                return (
                    StatusCode::NOT_FOUND,
                    error_json(format!("Session '{}' not found", session_id)),
                )
                    .into_response();
            }
            Err(e) => {
                tracing::error!(error = %e, "Failed to get session");
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    error_json("Failed to validate session"),
                )
                    .into_response();
            }
        }

        // DB-level guard: catches commands stuck in pending/running after a server restart.
        match db::has_running_command_for_session(&db, &session_id) {
            Ok(true) => {
                return (
                    StatusCode::FORBIDDEN,
                    error_json(format!(
                        "Session '{}' already has a running command. Wait for it to finish before submitting another.",
                        session_id
                    )),
                )
                    .into_response();
            }
            Err(e) => {
                tracing::error!(error = %e, "Failed to check running commands");
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    error_json("Failed to check running commands"),
                )
                    .into_response();
            }
            Ok(false) => {}
        }
    }

    // Atomically check and mark the session as busy.  This is the authoritative
    // serialization point for concurrent requests within this server instance: the
    // check and the insert happen inside a single lock acquisition, so two requests
    // racing here will be serialized — exactly one sees `contains == false` and
    // inserts; the other sees `contains == true` and returns 403.
    {
        let mut busy = state.busy_sessions.lock().await;
        if busy.contains(&session_id) {
            return (
                StatusCode::FORBIDDEN,
                error_json(format!(
                    "Session '{}' already has a running command. Wait for it to finish before submitting another.",
                    session_id
                )),
            )
                .into_response();
        }
        busy.insert(session_id.clone());
    }
    // INVARIANT: session is now marked busy.
    // Every error path below MUST remove session_id from busy_sessions before returning.

    let command_id = uuid::Uuid::new_v4().to_string();
    let args_json = serde_json::to_string(&body.args).unwrap_or_else(|_| "[]".to_string());

    // Create per-command directory using async I/O.
    let cmd_dir = state
        .headless_root
        .join("sessions")
        .join(&session_id)
        .join("commands")
        .join(&command_id);
    if let Err(e) = tokio::fs::create_dir_all(&cmd_dir).await {
        tracing::error!(error = %e, "Failed to create command directory");
        state.busy_sessions.lock().await.remove(&session_id);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            error_json("Failed to create command directory"),
        )
            .into_response();
    }

    let log_path = cmd_dir.join("output.log");

    // Insert command row; release the DB lock before touching busy_sessions
    // to keep the lock-acquisition order consistent (DB → busy) and avoid deadlock.
    let insert_result = {
        let db = state.db.lock().await;
        db::insert_command(
            &db,
            &command_id,
            &session_id,
            &body.subcommand,
            &args_json,
            &log_path.to_string_lossy(),
        )
    };
    if let Err(e) = insert_result {
        tracing::error!(error = %e, "Failed to insert command into DB");
        state.busy_sessions.lock().await.remove(&session_id);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            error_json("Failed to create command"),
        )
            .into_response();
    }

    tracing::info!(
        command_id = %command_id,
        session_id = %session_id,
        subcommand = %body.subcommand,
        args = %args_json,
        log_path = %log_path.display(),
        "Command dispatched"
    );

    // Spawn the command execution task and track its handle for graceful shutdown.
    let state_clone = Arc::clone(&state);
    let cmd_id = command_id.clone();
    let sess_id = session_id.clone();
    let subcommand = body.subcommand.clone();
    let cmd_args = body.args.clone();
    let log_p = log_path.clone();
    let workdir_clone = workdir.clone();

    let handle = tokio::spawn(async move {
        execute_command(
            state_clone,
            cmd_id,
            sess_id,
            subcommand,
            cmd_args,
            log_p,
            workdir_clone,
        )
        .await;
    });
    state.task_handles.lock().await.push(handle);

    (
        StatusCode::ACCEPTED,
        Json(CreateCommandResponse { command_id }),
    )
        .into_response()
}

/// Execute a subcommand asynchronously, updating DB status as it progresses.
async fn execute_command(
    state: Arc<AppState>,
    command_id: String,
    session_id: String,
    subcommand: String,
    args: Vec<String>,
    log_path: PathBuf,
    workdir: String,
) {
    let started_at = chrono::Utc::now().to_rfc3339();

    // Update status to running.
    {
        let db = state.db.lock().await;
        let _ = db::update_command_started(&db, &command_id, &started_at);
    }

    // Write metadata.json.
    let metadata = serde_json::json!({
        "command_id": command_id,
        "session_id": session_id,
        "subcommand": subcommand,
        "args": args,
        "workdir": workdir,
        "started_at": started_at,
    });
    if let Some(parent) = log_path.parent() {
        let meta_path = parent.join("metadata.json");
        let _ = tokio::fs::write(&meta_path, serde_json::to_string_pretty(&metadata).unwrap_or_default()).await;
    }

    // Build the CLI command to execute. We spawn a child process of amux
    // with the requested subcommand, capturing stdout/stderr to a single log file.
    let amux_bin = match std::env::current_exe() {
        Ok(p) => p,
        Err(e) => {
            tracing::error!(error = %e, command_id = %command_id, "Failed to get amux binary path");
            let finished_at = chrono::Utc::now().to_rfc3339();
            let db = state.db.lock().await;
            let _ = db::update_command_finished(&db, &command_id, "error", None, &finished_at);
            let mut busy = state.busy_sessions.lock().await;
            busy.remove(&session_id);
            return;
        }
    };

    let mut cmd = tokio::process::Command::new(&amux_bin);
    cmd.arg(&subcommand);
    for arg in &args {
        cmd.arg(arg);
    }
    // The headless server has no TTY, so always run supported subcommands in
    // non-interactive mode.  The `headless.alwaysNonInteractive` config option
    // additionally applies this flag at the CLI dispatch layer (commands/mod.rs),
    // so direct CLI invocations also honour the setting.
    //
    // Guard against duplicates: if the client already included --non-interactive
    // in the args vector, don't append a second copy (clap tolerates it, but it
    // is cleaner to avoid).
    let supports_non_interactive = matches!(
        subcommand.as_str(),
        "implement" | "chat" | "ready" | "exec"
    );
    if supports_non_interactive && !args.contains(&"--non-interactive".to_string()) {
        cmd.arg("--non-interactive");
    }
    cmd.current_dir(&workdir);

    // Open a single log file for combined stdout+stderr. Using tokio::fs avoids
    // blocking the executor on the open syscall; we then convert to std::fs::File
    // and clone it so both stdio handles write to the same file.
    let log_file = match tokio::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&log_path)
        .await
    {
        Ok(f) => f.into_std().await,
        Err(e) => {
            tracing::error!(error = %e, command_id = %command_id, "Failed to create output log");
            let finished_at = chrono::Utc::now().to_rfc3339();
            let db = state.db.lock().await;
            let _ = db::update_command_finished(&db, &command_id, "error", None, &finished_at);
            let mut busy = state.busy_sessions.lock().await;
            busy.remove(&session_id);
            return;
        }
    };
    let stderr_file = match log_file.try_clone() {
        Ok(f) => f,
        Err(e) => {
            tracing::error!(error = %e, command_id = %command_id, "Failed to clone output log file handle");
            let finished_at = chrono::Utc::now().to_rfc3339();
            let db = state.db.lock().await;
            let _ = db::update_command_finished(&db, &command_id, "error", None, &finished_at);
            let mut busy = state.busy_sessions.lock().await;
            busy.remove(&session_id);
            return;
        }
    };

    cmd.stdout(log_file);
    cmd.stderr(stderr_file);

    let result = cmd.spawn();

    match result {
        Ok(mut child) => {
            let exit_status = child.wait().await;
            let finished_at = chrono::Utc::now().to_rfc3339();

            let (status, exit_code) = match exit_status {
                Ok(es) => {
                    let code = es.code().unwrap_or(-1);
                    if es.success() {
                        ("done", Some(code))
                    } else {
                        ("error", Some(code))
                    }
                }
                Err(e) => {
                    tracing::error!(error = %e, command_id = %command_id, "Command wait failed");
                    ("error", None)
                }
            };

            tracing::info!(
                command_id = %command_id,
                session_id = %session_id,
                subcommand = %subcommand,
                status = status,
                exit_code = ?exit_code,
                "Command completed"
            );

            // Update metadata.json with completion info.
            if let Some(parent) = log_path.parent() {
                let meta_path = parent.join("metadata.json");
                let metadata = serde_json::json!({
                    "command_id": command_id,
                    "session_id": session_id,
                    "subcommand": subcommand,
                    "args": args,
                    "workdir": workdir,
                    "started_at": started_at,
                    "finished_at": finished_at,
                    "exit_code": exit_code,
                    "status": status,
                });
                let _ = tokio::fs::write(&meta_path, serde_json::to_string_pretty(&metadata).unwrap_or_default()).await;
            }

            let db = state.db.lock().await;
            let _ = db::update_command_finished(&db, &command_id, status, exit_code, &finished_at);
        }
        Err(e) => {
            tracing::error!(error = %e, command_id = %command_id, "Failed to spawn command");
            let finished_at = chrono::Utc::now().to_rfc3339();
            let db = state.db.lock().await;
            let _ = db::update_command_finished(&db, &command_id, "error", None, &finished_at);
        }
    }

    // Unmark session as busy.
    let mut busy = state.busy_sessions.lock().await;
    busy.remove(&session_id);
}

async fn handle_get_command(
    State(state): State<Arc<AppState>>,
    AxumPath(id): AxumPath<String>,
) -> impl IntoResponse {
    let db = state.db.lock().await;
    match db::get_command(&db, &id) {
        Ok(Some(c)) => {
            let args: serde_json::Value =
                serde_json::from_str(&c.args).unwrap_or(serde_json::Value::Array(vec![]));
            Json(CommandResponse {
                id: c.id,
                session_id: c.session_id,
                subcommand: c.subcommand,
                args,
                status: c.status,
                exit_code: c.exit_code,
                started_at: c.started_at,
                finished_at: c.finished_at,
                log_path: c.log_path,
            })
            .into_response()
        }
        Ok(None) => (
            StatusCode::NOT_FOUND,
            error_json(format!("Command '{}' not found", id)),
        )
            .into_response(),
        Err(e) => {
            tracing::error!(error = %e, "Failed to get command");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                error_json("Failed to get command"),
            )
                .into_response()
        }
    }
}

async fn handle_get_command_logs(
    State(state): State<Arc<AppState>>,
    AxumPath(id): AxumPath<String>,
) -> impl IntoResponse {
    let db = state.db.lock().await;
    match db::get_command(&db, &id) {
        Ok(Some(c)) => {
            drop(db); // Release lock before file I/O.
            let output = tokio::fs::read_to_string(&c.log_path)
                .await
                .unwrap_or_default();
            Json(serde_json::json!({
                "command_id": c.id,
                "output": output,
            }))
            .into_response()
        }
        Ok(None) => (
            StatusCode::NOT_FOUND,
            error_json(format!("Command '{}' not found", id)),
        )
            .into_response(),
        Err(e) => {
            tracing::error!(error = %e, "Failed to get command");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                error_json("Failed to get command"),
            )
                .into_response()
        }
    }
}

/// SSE endpoint: stream the command log file line-by-line as Server-Sent Events.
/// Sends a `[amux:done]` event when the command finishes (or is already done).
async fn handle_stream_command_logs(
    State(state): State<Arc<AppState>>,
    AxumPath(id): AxumPath<String>,
) -> Response {
    use axum::response::sse::{Event, Sse};
    use tokio_stream::wrappers::UnboundedReceiverStream;

    // Look up the command once to get the log path.
    let (log_path, is_already_done) = {
        let db = state.db.lock().await;
        match db::get_command(&db, &id) {
            Ok(Some(c)) => {
                let done = matches!(c.status.as_str(), "done" | "error");
                (c.log_path, done)
            }
            Ok(None) => {
                return (StatusCode::NOT_FOUND, error_json(format!("Command '{}' not found", id)))
                    .into_response();
            }
            Err(e) => {
                tracing::error!(error = %e, "Failed to get command for SSE stream");
                return (StatusCode::INTERNAL_SERVER_ERROR, error_json("Failed to get command"))
                    .into_response();
            }
        }
    };

    let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<Result<Event, std::convert::Infallible>>();
    let stream = UnboundedReceiverStream::new(rx);

    if is_already_done {
        // Command already finished: stream the existing log content then send sentinel.
        tokio::spawn(async move {
            match tokio::fs::read_to_string(&log_path).await {
                Ok(content) => {
                    for line in content.lines() {
                        if tx.send(Ok(Event::default().data(line.to_string()))).is_err() {
                            return;
                        }
                    }
                }
                Err(e) => {
                    tracing::error!(error = %e, "Failed to read completed log for SSE");
                }
            }
            let _ = tx.send(Ok(Event::default().data("[amux:done]")));
        });
    } else {
        // Command still running: tail the log file, poll until command completes.
        let state_clone = Arc::clone(&state);
        let command_id = id.clone();
        tokio::spawn(async move {
            use tokio::io::AsyncReadExt;

            // The log file may not exist yet if the command was just submitted and
            // the executor task hasn't created it yet. Poll every 1s for up to 10s
            // before giving up with a 404-style error sentinel.
            const LOG_WAIT_SECS: u64 = 10;
            let mut file = {
                let mut waited = 0u64;
                loop {
                    match tokio::fs::File::open(&log_path).await {
                        Ok(f) => break f,
                        Err(_) if waited < LOG_WAIT_SECS => {
                            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                            waited += 1;
                        }
                        Err(e) => {
                            tracing::error!(
                                error = %e,
                                path = %log_path,
                                waited_secs = waited,
                                "Log file did not appear within {}s; aborting SSE stream",
                                LOG_WAIT_SECS,
                            );
                            let _ = tx.send(Ok(Event::default().data("[amux:done]")));
                            return;
                        }
                    }
                }
            };

            let mut leftover = String::new();

            loop {
                let mut chunk = vec![0u8; 4096];
                match file.read(&mut chunk).await {
                    Ok(0) => {
                        // No new data: check if the command is now done.
                        let done = {
                            let db = state_clone.db.lock().await;
                            match db::get_command(&db, &command_id) {
                                Ok(Some(c)) => matches!(c.status.as_str(), "done" | "error"),
                                _ => true,
                            }
                        };
                        if done {
                            // Flush remaining partial line if any.
                            if !leftover.is_empty() {
                                let line = std::mem::take(&mut leftover);
                                if tx.send(Ok(Event::default().data(line))).is_err() {
                                    return;
                                }
                            }
                            let _ = tx.send(Ok(Event::default().data("[amux:done]")));
                            return;
                        }
                        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                    }
                    Ok(n) => {
                        let text = String::from_utf8_lossy(&chunk[..n]);
                        leftover.push_str(&text);
                        // Emit complete lines.
                        while let Some(pos) = leftover.find('\n') {
                            let line = leftover[..pos].to_string();
                            leftover = leftover[pos + 1..].to_string();
                            if tx.send(Ok(Event::default().data(line))).is_err() {
                                return;
                            }
                        }
                    }
                    Err(e) => {
                        tracing::error!(error = %e, "SSE log read error");
                        let _ = tx.send(Ok(Event::default().data("[amux:done]")));
                        return;
                    }
                }
            }
        });
    }

    Sse::new(stream).into_response()
}

/// Attempt to discover the PID of the process holding the given TCP port.
/// Best-effort: returns `None` if the lookup is unsupported or fails.
fn find_port_owner(port: u16) -> Option<u32> {
    #[cfg(target_os = "linux")]
    {
        // `ss -tlnp sport = :PORT` includes `pid=NNN,` in the users column.
        if let Ok(out) = std::process::Command::new("ss")
            .args(["-tlnp", &format!("sport = :{port}")])
            .output()
        {
            let text = String::from_utf8_lossy(&out.stdout);
            for token in text.split_whitespace() {
                if let Some(rest) = token.strip_prefix("pid=") {
                    let pid_str = rest.split(',').next().unwrap_or("");
                    if let Ok(pid) = pid_str.parse::<u32>() {
                        return Some(pid);
                    }
                }
            }
        }
    }
    #[cfg(target_os = "macos")]
    {
        // `lsof -ti :PORT` prints PIDs (one per line) of processes with that port open.
        if let Ok(out) = std::process::Command::new("lsof")
            .args(["-ti", &format!(":{port}")])
            .output()
        {
            let text = String::from_utf8_lossy(&out.stdout);
            if let Ok(pid) = text
                .trim()
                .lines()
                .next()
                .unwrap_or("")
                .parse::<u32>()
            {
                return Some(pid);
            }
        }
    }
    None
}

/// Start the HTTP server and block until shutdown.
pub async fn start_server(
    port: u16,
    workdirs: Vec<PathBuf>,
    headless_root: PathBuf,
    runtime: Arc<dyn crate::runtime::AgentRuntime>,
) -> Result<()> {
    let db_conn = db::open_db(&headless_root)?;

    let state = Arc::new(AppState {
        db: Mutex::new(db_conn),
        workdirs: workdirs.clone(),
        headless_root: headless_root.clone(),
        started_at: Instant::now(),
        runtime,
        busy_sessions: Mutex::new(HashSet::new()),
        task_handles: Mutex::new(Vec::new()),
    });

    let app = build_router(state.clone());

    let addr = std::net::SocketAddr::from(([0, 0, 0, 0], port));

    tracing::info!(
        port = port,
        workdirs = ?workdirs.iter().map(|p| p.display().to_string()).collect::<Vec<_>>(),
        pid = std::process::id(),
        storage_root = %headless_root.display(),
        "Headless server starting"
    );

    // Spawn heartbeat task.
    let heartbeat_state = Arc::clone(&state);
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(600));
        loop {
            interval.tick().await;
            let db = heartbeat_state.db.lock().await;
            let active = db::count_active_sessions(&db).unwrap_or(0);
            let running = db::count_running_commands(&db).unwrap_or(0);
            tracing::info!(
                active_sessions = active,
                running_commands = running,
                "Heartbeat"
            );
        }
    });

    let listener = match tokio::net::TcpListener::bind(addr).await {
        Ok(l) => l,
        Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => {
            let pid_hint = find_port_owner(port)
                .map(|pid| format!(" (held by PID {pid})"))
                .unwrap_or_default();
            return Err(anyhow::anyhow!(
                "Port {port} is already in use{pid_hint}. \
                 Use --port to choose a different port or stop the conflicting process."
            ));
        }
        Err(e) => {
            return Err(anyhow::anyhow!(e)
                .context(format!("Failed to bind to port {port}")));
        }
    };

    tracing::info!(port = port, "Headless server listening");

    // Set up graceful shutdown on SIGTERM/SIGINT.
    let shutdown = async {
        let ctrl_c = tokio::signal::ctrl_c();
        #[cfg(unix)]
        {
            let mut sigterm =
                tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                    .expect("Failed to install SIGTERM handler");
            tokio::select! {
                _ = ctrl_c => { tracing::info!("Received SIGINT, shutting down"); }
                _ = sigterm.recv() => { tracing::info!("Received SIGTERM, shutting down"); }
            }
        }
        #[cfg(not(unix))]
        {
            ctrl_c.await.expect("Failed to listen for ctrl-c");
            tracing::info!("Received SIGINT, shutting down");
        }
    };

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown)
        .await
        .context("Server error")?;

    // After HTTP shutdown, wait for any still-running command tasks (grace period: 30 s).
    // This ensures subprocesses can write their final output and update the DB before
    // the process exits.
    const GRACE_SECS: u64 = 30;
    let handles: Vec<_> = state.task_handles.lock().await.drain(..).collect();
    if !handles.is_empty() {
        tracing::info!(
            count = handles.len(),
            grace_seconds = GRACE_SECS,
            "Waiting for running commands to finish before exiting"
        );
        let deadline = tokio::time::Instant::now()
            + std::time::Duration::from_secs(GRACE_SECS);
        for handle in handles {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                handle.abort();
            } else {
                let _ = tokio::time::timeout(remaining, handle).await;
            }
        }
    }

    tracing::info!("Headless server stopped");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── is_valid_subcommand (work item 0058) ─────────────────────────────────
    //
    // Verify that "exec" was added to KNOWN_SUBCOMMANDS so that headless clients
    // can dispatch `exec prompt` and `exec workflow` requests.

    #[test]
    fn is_valid_subcommand_exec_is_accepted() {
        assert!(
            is_valid_subcommand("exec"),
            "'exec' must be in KNOWN_SUBCOMMANDS so headless clients can dispatch exec commands; \
             current list: {KNOWN_SUBCOMMANDS:?}"
        );
    }

    #[test]
    fn is_valid_subcommand_all_known_subcommands_are_valid() {
        for &name in KNOWN_SUBCOMMANDS {
            assert!(
                is_valid_subcommand(name),
                "'{name}' is in KNOWN_SUBCOMMANDS but is_valid_subcommand returned false"
            );
        }
    }

    #[test]
    fn is_valid_subcommand_unknown_name_is_rejected() {
        assert!(!is_valid_subcommand("unknown"), "unknown subcommand must be rejected");
        assert!(!is_valid_subcommand(""), "empty string must be rejected");
        // Two-level paths like "exec prompt" are not valid at this layer;
        // only the top-level "exec" token is validated here.
        assert!(
            !is_valid_subcommand("exec prompt"),
            "two-level path must be rejected; the server splits on subcommand + args"
        );
    }

    // ── remote subcommand (work item 0059) ───────────────────────────────────

    /// "remote" was added to KNOWN_SUBCOMMANDS so that headless clients can
    /// dispatch `remote run` and `remote session start/kill` requests.
    #[test]
    fn is_valid_subcommand_remote_is_accepted() {
        assert!(
            is_valid_subcommand("remote"),
            "'remote' must be in KNOWN_SUBCOMMANDS so headless clients can dispatch \
             remote commands; current list: {KNOWN_SUBCOMMANDS:?}"
        );
    }
}
