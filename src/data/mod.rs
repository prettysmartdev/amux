#![allow(unused_imports)]
//! Layer 0: data
//!
//! This layer owns every data definition, config concern, filesystem access,
//! and database concern. No business logic, no container interaction, no git
//! operations, no workflow execution, no command logic, and no frontend code
//! is permitted at this layer. See `aspec/architecture/2026-grand-architecture.md`.

pub mod config;
pub mod error;
pub mod fs;
pub mod session;
pub mod session_manager;

pub use error::DataError;
pub use session::{
    AgentName, CommandInvocation, CommandStatus, ContainerHandle, GitRootResolver, Session,
    SessionId, SessionLogEntry, SessionLogKind, SessionState, StepStatus, WorkflowInvocation,
    WorkflowStepRecord,
};
pub use session_manager::{InMemorySessionStore, SessionManager, SessionStore};
