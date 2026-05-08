//! `ClawsFrontend` impl for the TUI.

use std::path::Path;

use crate::engine::claws::frontend::ClawsFrontend;
use crate::engine::claws::phase::ClawsPhase;
use crate::engine::claws::summary::ClawsSummary;
use crate::engine::container::frontend::ContainerFrontend;
use crate::engine::error::EngineError;
use crate::engine::message::UserMessageSink;
use crate::engine::step_status::StepStatus;
use crate::frontend::tui::command_frontend::TuiCommandFrontend;
use crate::frontend::tui::dialogs::{DialogRequest, DialogResponse};

impl ClawsFrontend for TuiCommandFrontend {
    fn ask_replace_existing_clone(&mut self, path: &Path) -> Result<bool, EngineError> {
        let response = self
            .ask_dialog(DialogRequest::YesNo {
                title: "Replace clone?".into(),
                body: format!(
                    "An existing clone exists at {}. Replace it?",
                    path.display()
                ),
            })
            .map_err(|e| EngineError::Other(e.to_string()))?;
        Ok(matches!(
            response,
            DialogResponse::Yes | DialogResponse::Char('y')
        ))
    }

    fn ask_run_audit(&mut self) -> Result<bool, EngineError> {
        let response = self
            .ask_dialog(DialogRequest::YesNo {
                title: "Run audit?".into(),
                body: "Run the audit to set up the claws environment?".into(),
            })
            .map_err(|e| EngineError::Other(e.to_string()))?;
        Ok(matches!(
            response,
            DialogResponse::Yes | DialogResponse::Char('y')
        ))
    }

    fn report_phase(&mut self, phase: &ClawsPhase) {
        self.messages.info(format!("claws: {phase:?}"));
    }

    fn report_step_status(&mut self, step: &str, status: StepStatus) {
        self.messages.info(format!("  {step}: {status:?}"));
    }

    fn container_frontend(&mut self) -> Box<dyn ContainerFrontend> {
        // Claws launches a single interactive PTY container, so hand the
        // PTY-bridge channels straight to the engine.
        match self.container_io.take() {
            Some(io) => Box::new(super::TuiContainerProxy::with_io(
                self.status_log.clone(),
                io,
                self.container_name_shared.clone(),
            )),
            None => Box::new(super::TuiContainerProxy::new(self.status_log.clone())),
        }
    }

    fn report_summary(&mut self, _summary: &ClawsSummary) {
        self.messages.success("claws completed");
    }

    fn confirm_restart_stopped(&mut self) -> Result<bool, EngineError> {
        let response = self
            .ask_dialog(DialogRequest::YesNo {
                title: "Restart container?".into(),
                body: "A stopped container was found. Restart it?".into(),
            })
            .map_err(|e| EngineError::Other(e.to_string()))?;
        Ok(matches!(
            response,
            DialogResponse::Yes | DialogResponse::Char('y')
        ))
    }

    fn confirm_offer_init(&mut self) -> Result<bool, EngineError> {
        let response = self
            .ask_dialog(DialogRequest::YesNo {
                title: "Run init?".into(),
                body: "No amux setup found. Run init first?".into(),
            })
            .map_err(|e| EngineError::Other(e.to_string()))?;
        Ok(matches!(
            response,
            DialogResponse::Yes | DialogResponse::Char('y')
        ))
    }

    fn confirm_sudo_actions(&mut self, commands: &[String]) -> Result<bool, EngineError> {
        let body = format!(
            "The following commands require sudo:\n{}\n\nProceed?",
            commands.join("\n")
        );
        let response = self
            .ask_dialog(DialogRequest::YesNo {
                title: "Sudo required".into(),
                body,
            })
            .map_err(|e| EngineError::Other(e.to_string()))?;
        Ok(matches!(
            response,
            DialogResponse::Yes | DialogResponse::Char('y')
        ))
    }
}
