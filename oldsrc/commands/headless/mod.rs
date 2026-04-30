pub mod auth;
pub mod db;
pub mod logging;
pub mod process;
pub mod server;

use anyhow::{bail, Result};
use std::path::PathBuf;
use std::sync::Arc;

use crate::cli::HeadlessAction;

pub async fn run(
    action: HeadlessAction,
    runtime: Arc<dyn crate::runtime::AgentRuntime>,
) -> Result<()> {
    match action {
        HeadlessAction::Start {
            port,
            workdirs,
            background,
            refresh_key,
            dangerously_skip_auth,
        } => run_start(port, workdirs, background, refresh_key, dangerously_skip_auth, runtime).await,
        HeadlessAction::Kill => run_kill().await,
        HeadlessAction::Logs => run_logs().await,
        HeadlessAction::Status => run_status().await,
    }
}

async fn run_start(
    port: u16,
    cli_workdirs: Vec<String>,
    background: bool,
    refresh_key: bool,
    dangerously_skip_auth: bool,
    runtime: Arc<dyn crate::runtime::AgentRuntime>,
) -> Result<()> {
    let root = db::headless_root()?;

    // Check if server is already running FIRST — before the key lifecycle so
    // that `--refresh-key` does not overwrite the on-disk hash when the server
    // is already running (the running instance would then reject its own key).
    if let Some(pid) = process::check_already_running(&root)? {
        bail!(
            "Headless server is already running (PID {}). Use `amux headless kill` to stop it first.",
            pid
        );
    }

    // ── API key lifecycle (before logging init so the banner prints cleanly) ──
    let auth_mode = if dangerously_skip_auth {
        eprintln!("WARNING: authentication is disabled (--dangerously-skip-auth).");
        server::AuthMode::Disabled
    } else {
        // Decide whether we need to generate a new key.
        let existing_hash = auth::read_key_hash(&root)?;
        if refresh_key || existing_hash.is_none() {
            let key = auth::generate_api_key()?;
            let hash = auth::hash_api_key(&key);
            auth::write_key_hash(&root, &hash)?;
            auth::print_key_banner(&key);
            server::AuthMode::Enabled { key_hash: hash }
        } else {
            server::AuthMode::Enabled {
                key_hash: existing_hash.unwrap(),
            }
        }
    };

    // Merge CLI workdirs with config workdirs.
    let global_config = crate::config::load_global_config().unwrap_or_default();
    let mut all_workdirs: Vec<String> = cli_workdirs;
    if let Some(config_dirs) = global_config.headless.as_ref().and_then(|h| h.work_dirs.as_ref()) {
        for dir in config_dirs {
            if !all_workdirs.contains(dir) {
                all_workdirs.push(dir.clone());
            }
        }
    }

    // Resolve to canonical absolute paths.
    let mut canonical_dirs: Vec<PathBuf> = Vec::new();
    for dir in &all_workdirs {
        match std::fs::canonicalize(dir) {
            Ok(p) => canonical_dirs.push(p),
            Err(e) => {
                tracing::warn!(
                    path = %dir,
                    error = %e,
                    "Workdir does not exist or cannot be resolved; skipping"
                );
            }
        }
    }

    if background {
        // Daemonize and exit.
        process::daemonize(port, &all_workdirs)?;
        return Ok(());
    }

    // Foreground mode.
    logging::init_foreground();

    // Write PID file so `amux headless kill` and `amux headless status` can find us.
    process::write_pid_file(&root)?;

    let result = server::start_server(port, canonical_dirs, root.clone(), auth_mode, runtime).await;

    // Clean up PID file on exit.
    let _ = process::remove_pid_file(&root);

    result
}

async fn run_kill() -> Result<()> {
    let root = db::headless_root()?;
    process::kill_server(&root)
}

async fn run_logs() -> Result<()> {
    let root = db::headless_root()?;
    process::stream_logs(&root).await
}

async fn run_status() -> Result<()> {
    let root = db::headless_root()?;
    let pid_file = process::pid_file_path(&root);

    match process::read_pid_file(&root)? {
        Some(pid) if process::is_process_alive(pid) => {
            println!("Headless server is running.");
            println!("  PID:      {}", pid);
            println!("  PID file: {}", pid_file.display());

            // Try to read from the DB for more info.
            match db::open_db(&root) {
                Ok(conn) => {
                    let active = db::count_active_sessions(&conn).unwrap_or(0);
                    let running = db::count_running_commands(&conn).unwrap_or(0);
                    println!("  Active sessions:  {}", active);
                    println!("  Running commands: {}", running);
                }
                Err(_) => {
                    println!("  (Could not read database for session/command counts)");
                }
            }
        }
        Some(pid) => {
            println!("Headless server is NOT running (stale PID file for PID {}).", pid);
            process::remove_pid_file(&root)?;
        }
        None => {
            println!("Headless server is not running (no PID file found).");
        }
    }

    Ok(())
}
