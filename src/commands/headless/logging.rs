use anyhow::{Context, Result};
use std::path::Path;
use tracing_subscriber::{fmt, EnvFilter};

/// Initialize tracing for foreground mode: structured human-readable logs to stdout.
pub fn init_foreground() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    fmt()
        .with_env_filter(filter)
        .with_target(true)
        .with_thread_ids(false)
        .init();
}

/// Initialize tracing for background mode: JSON logs appended to the given log file.
pub fn init_background(log_path: &Path) -> Result<()> {
    if let Some(parent) = log_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create log directory {}", parent.display()))?;
    }

    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)
        .with_context(|| format!("Failed to open log file {}", log_path.display()))?;

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    fmt()
        .with_env_filter(filter)
        .json()
        .with_writer(std::sync::Mutex::new(file))
        .with_target(true)
        .init();

    Ok(())
}
