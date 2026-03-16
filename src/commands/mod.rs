pub mod auth;
pub mod implement;
pub mod init;
pub mod new;
pub mod output;
pub mod ready;

use crate::cli::Command;
use anyhow::Result;

pub async fn run(command: Command) -> Result<()> {
    match command {
        Command::Init { agent } => init::run(agent).await,
        Command::Ready {
            refresh,
            non_interactive,
        } => ready::run(refresh, non_interactive).await,
        Command::Implement {
            work_item,
            non_interactive,
        } => implement::run(&work_item, non_interactive).await,
        Command::New => new::run().await,
    }
}
