//! Phase state machine for `ClawsEngine`.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClawsPhase {
    Preflight,
    AwaitingCloneDecision,
    CloningRepo,
    CheckingPermissions,
    BuildingImage,
    AwaitingAuditDecision,
    RunningAudit,
    Configuring,
    LaunchingController,
    AttachingChat,
    Complete,
    Failed(ClawsFailure),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "detail")]
pub enum ClawsFailure {
    Generic { phase: String, message: String },
    Cloning { message: String },
    Sudo { message: String },
    ImageBuild { tag: String, message: String },
    ChatAttach { controller: String, message: String },
    ControllerNotRunning { hint: String },
}

impl ClawsFailure {
    /// Phase label for the failure — preserved for log/UI surfaces.
    pub fn phase(&self) -> &str {
        match self {
            ClawsFailure::Generic { phase, .. } => phase,
            ClawsFailure::Cloning { .. } => "CloningRepo",
            ClawsFailure::Sudo { .. } => "CheckingPermissions",
            ClawsFailure::ImageBuild { .. } => "BuildingImage",
            ClawsFailure::ChatAttach { .. } => "AttachingChat",
            ClawsFailure::ControllerNotRunning { .. } => "Preflight",
        }
    }

    /// Human-readable failure message.
    pub fn message(&self) -> String {
        match self {
            ClawsFailure::Generic { message, .. } => message.clone(),
            ClawsFailure::Cloning { message } => format!("clone failed: {message}"),
            ClawsFailure::Sudo { message } => format!("permission check failed: {message}"),
            ClawsFailure::ImageBuild { tag, message } => {
                format!("image build for tag '{tag}' failed: {message}")
            }
            ClawsFailure::ChatAttach {
                controller,
                message,
            } => {
                format!("attaching chat to controller '{controller}' failed: {message}")
            }
            ClawsFailure::ControllerNotRunning { hint } => hint.clone(),
        }
    }
}
