//! `ImplementCommandFrontend` impl for the CLI.

use crate::command::commands::exec_workflow::WorkflowSummary;
use crate::command::commands::implement::ImplementCommandFrontend;
use crate::engine::message::UserMessageSink;

use crate::frontend::cli::command_frontend::CliFrontend;

#[async_trait::async_trait]
impl ImplementCommandFrontend for CliFrontend {
    fn set_pty_active(&mut self, active: bool) {
        self.messages.set_pty_active(active);
    }

    fn report_implement_summary(&mut self, summary: &WorkflowSummary) {
        self.messages
            .write_message(crate::engine::message::UserMessage {
                level: crate::engine::message::MessageLevel::Info,
                text: format!(
                    "implement summary — {}/{} steps OK ({} failed)",
                    summary.steps_completed,
                    summary.steps_completed + summary.steps_failed,
                    summary.steps_failed
                ),
            });
    }
}
