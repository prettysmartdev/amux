pub mod agent;
pub mod auth;
pub mod chat;
pub mod claws;
pub mod config;
pub mod download;
pub mod exec;
pub mod headless;
pub mod implement;
pub mod init;
pub mod init_flow;
pub mod new;
pub mod output;
pub mod ready;
pub mod ready_flow;
pub mod spec;
pub mod specs;
pub mod status;

use crate::cli::{Command, ExecAction, SpecsAction};
use anyhow::Result;
use std::sync::Arc;

pub async fn run(mut command: Command, runtime: Arc<dyn crate::runtime::AgentRuntime>) -> Result<()> {
    // When `headless.alwaysNonInteractive` is set in global config, force non-interactive
    // mode on every command variant that carries that flag before dispatch.
    if crate::config::effective_always_non_interactive() {
        match &mut command {
            Command::Chat { non_interactive, .. } => *non_interactive = true,
            Command::Implement { non_interactive, .. } => *non_interactive = true,
            Command::Ready { non_interactive, .. } => *non_interactive = true,
            Command::Exec { action } => match action {
                ExecAction::Prompt { non_interactive, .. } => *non_interactive = true,
                ExecAction::Workflow { non_interactive, .. } => *non_interactive = true,
            },
            Command::Specs { action } => {
                if let SpecsAction::Amend { non_interactive, .. } = action {
                    *non_interactive = true;
                }
            }
            _ => {}
        }
    }

    match command {
        Command::Init { agent, aspec } => init::run(agent, aspec, runtime).await,
        Command::Ready {
            refresh,
            build,
            no_cache,
            non_interactive,
            allow_docker,
            json,
        } => {
            // --json implies --non-interactive.
            let effective_non_interactive = non_interactive || json;
            ready::run(refresh, build, no_cache, effective_non_interactive, allow_docker, json, runtime).await
        }
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
            model,
        } => implement::run(&work_item, non_interactive, plan, allow_docker, workflow.as_deref(), worktree, mount_ssh, yolo, auto, agent, model, runtime).await,
        Command::Chat { non_interactive, plan, allow_docker, mount_ssh, yolo, auto, agent, model } => {
            chat::run(non_interactive, plan, allow_docker, mount_ssh, yolo, auto, agent, model, runtime).await
        }
        Command::Exec { action } => match action {
            ExecAction::Prompt {
                prompt,
                non_interactive,
                plan,
                allow_docker,
                mount_ssh,
                yolo,
                auto,
                agent,
                model,
            } => {
                exec::run_prompt(&prompt, non_interactive, plan, allow_docker, mount_ssh, yolo, auto, agent, model, runtime).await
            }
            ExecAction::Workflow {
                workflow,
                work_item,
                non_interactive,
                plan,
                allow_docker,
                worktree,
                mount_ssh,
                yolo,
                auto,
                agent,
                model,
            } => {
                exec::run_exec_workflow(&workflow, work_item.as_deref(), non_interactive, plan, allow_docker, worktree, mount_ssh, yolo, auto, agent, model, runtime).await
            }
        },
        Command::Claws { action } => claws::run(action, runtime).await,
        Command::Status { watch } => status::run(watch, runtime.clone()).await,
        Command::Specs { action } => match action {
            SpecsAction::New { interview } => specs::run_new(interview).await,
            SpecsAction::Amend { work_item, non_interactive, allow_docker } => {
                specs::run_amend(&work_item, non_interactive, allow_docker, runtime).await
            },
        },
        Command::Config { action } => config::run(action, runtime).await,
        Command::Headless { action } => headless::run(action, runtime).await,
    }
}
