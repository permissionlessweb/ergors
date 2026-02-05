//! Bootstrap Module
//!
//! Orchestrates the complete bootstrap workflow for new ergors nodes.

pub mod orchestrator;
pub mod receiver;
pub mod state_machine;

pub use orchestrator::{BootstrapOrchestrator, NodeBootstrapParams};
pub use receiver::BootstrapReceiver;
pub use state_machine::{BootstrapState, BootstrapStep, StepResult};
