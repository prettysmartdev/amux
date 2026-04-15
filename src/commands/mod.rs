pub mod agent;
pub mod auth;
pub mod chat;
pub mod claws;
pub mod config;
pub mod download;
pub mod implement;
pub mod init;
pub mod init_flow;
pub mod new;
pub mod output;
pub mod ready;
pub mod specs;
pub mod status;

use crate::cli::{Command, SpecsAction};
use anyhow::Result;
use std::sync::Arc;

pub async fn run(command: Command, runtime: Arc<dyn crate::runtime::AgentRuntime>) -> Result<()> {
    match command {
        Command::Init { agent, aspec } => init::run(agent, aspec, runtime).await,
        Command::Ready {
            refresh,
            build,
            no_cache,
            non_interactive,
            allow_docker,
        } => ready::run(refresh, build, no_cache, non_interactive, allow_docker, runtime).await,
        Command::Implement {
            work_item,
            non_interactive,
            plan,
            allow_docker,
            workflow,
            worktree,
            mount_ssh,
            yolo,
            auto,
            agent,
        } => implement::run(&work_item, non_interactive, plan, allow_docker, workflow.as_deref(), worktree, mount_ssh, yolo, auto, agent, runtime).await,
        Command::Chat { non_interactive, plan, allow_docker, mount_ssh, yolo, auto, agent } => {
            chat::run(non_interactive, plan, allow_docker, mount_ssh, yolo, auto, agent, runtime).await
        }
        Command::Claws { action } => claws::run(action, runtime).await,
        Command::Status { watch } => status::run(watch, runtime.clone()).await,
        Command::Specs { action } => match action {
            SpecsAction::New { interview } => specs::run_new(interview).await,
            SpecsAction::Amend { work_item, non_interactive, allow_docker } => {
                specs::run_amend(&work_item, non_interactive, allow_docker, runtime).await
            },
        },
        Command::Config { action } => config::run(action, runtime).await,
    }
}
