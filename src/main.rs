#![allow(dead_code)]

mod cli;
mod commands;
mod config;
mod git;
mod passthrough;
mod runtime;
mod tui;
mod workflow;

use anyhow::Result;
use clap::Parser;
use cli::Cli;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    let global_config = crate::config::load_global_config().unwrap_or_default();
    let runtime = crate::runtime::resolve_runtime(&global_config)?;

    match cli.command {
        Some(cmd) => commands::run(cmd, runtime).await,
        None => {
            let startup_ready_flags = tui::StartupReadyFlags {
                build: cli.build,
                no_cache: cli.no_cache,
                refresh: cli.refresh,
            };
            tui::run(startup_ready_flags, runtime).await
        }
    }
}
