//! `NewCommandFrontend` impl for the TUI.

use crate::command::commands::new::NewCommandFrontend;
use crate::command::error::CommandError;
use crate::frontend::tui::command_frontend::TuiCommandFrontend;
use crate::frontend::tui::dialogs::{DialogRequest, DialogResponse};

impl NewCommandFrontend for TuiCommandFrontend {
    fn ask_workflow_name(&mut self) -> Result<String, CommandError> {
        let response = self.ask_dialog(DialogRequest::TextInput {
            title: "Workflow name".into(),
            prompt: "Enter the workflow filename slug:".into(),
            default_text: None,
        })?;
        match response {
            DialogResponse::Text(t) if !t.is_empty() => Ok(t),
            _ => Ok("workflow".to_string()),
        }
    }

    fn ask_workflow_summary(&mut self) -> Result<String, CommandError> {
        let response = self.ask_dialog(DialogRequest::TextInput {
            title: "Workflow summary".into(),
            prompt: "Enter a one-line summary:".into(),
            default_text: None,
        })?;
        match response {
            DialogResponse::Text(t) => Ok(t),
            _ => Ok(String::new()),
        }
    }

    fn ask_skill_name(&mut self) -> Result<String, CommandError> {
        let response = self.ask_dialog(DialogRequest::TextInput {
            title: "Skill name".into(),
            prompt: "Enter the skill name:".into(),
            default_text: None,
        })?;
        match response {
            DialogResponse::Text(t) if !t.is_empty() => Ok(t),
            _ => Ok("skill".to_string()),
        }
    }

    fn ask_skill_summary(&mut self) -> Result<String, CommandError> {
        let response = self.ask_dialog(DialogRequest::TextInput {
            title: "Skill summary".into(),
            prompt: "Enter a one-line skill summary:".into(),
            default_text: None,
        })?;
        match response {
            DialogResponse::Text(t) => Ok(t),
            _ => Ok(String::new()),
        }
    }

    fn ask_skill_body(&mut self) -> Result<String, CommandError> {
        let response = self.ask_dialog(DialogRequest::MultilineInput {
            title: "Skill body".into(),
            prompt: "Enter the skill body content (Ctrl+Enter to submit):".into(),
        })?;
        match response {
            DialogResponse::Text(t) => Ok(t),
            _ => Ok(String::new()),
        }
    }
}
