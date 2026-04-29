//! Shared entry-point for the top-level `amux new` subcommand.
//!
//! `new spec` aliases the existing `specs new` flow; `new workflow` and
//! `new skill` delegate to dedicated modules.

use crate::cli::NewAction;
use anyhow::Result;

pub async fn run(action: NewAction) -> Result<()> {
    match action {
        NewAction::Spec { interview } => crate::commands::specs::run_new(interview).await,
        NewAction::Workflow {
            interview,
            global,
            format,
        } => crate::commands::new_workflow::run_new_workflow(interview, global, format).await,
        NewAction::Skill { interview, global } => {
            crate::commands::new_skill::run_new_skill(interview, global).await
        }
    }
}

