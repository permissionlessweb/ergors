//! Orchestrator Session Integration
//!
//! Provides helper functions for integrating FractalSession with the CosmicOrchestrator.
//! This enables hierarchical tracking of orchestration tasks, prompt handling, and
//! fractal agent creation.

use crate::session::manager::SessionManager;
use ho_std::error::HoResult;
use ho_std::types::ergors::management::v1::{
    CreateSessionRequest, FractalSession, SessionScope, SessionType,
};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, info, warn};

/// Helper for creating orchestration-related sessions
pub struct OrchestratorSessionHelper {
    session_manager: Arc<SessionManager>,
}

impl OrchestratorSessionHelper {
    /// Create a new orchestrator session helper
    pub fn new(session_manager: Arc<SessionManager>) -> Self {
        Self { session_manager }
    }

    /// Create a session for a prompt request
    ///
    /// Returns the session ID if created, or None if session creation is skipped.
    pub async fn create_prompt_session(
        &self,
        model: &str,
        prompt_hash: &str,
        parent_session_id: Option<&str>,
    ) -> Option<String> {
        let mut labels = HashMap::new();
        labels.insert("model".to_string(), model.to_string());
        labels.insert("prompt_hash".to_string(), prompt_hash.to_string());
        labels.insert("operation".to_string(), "prompt".to_string());

        let request = CreateSessionRequest {
            session_type: SessionType::Agentic.into(),
            scope: SessionScope::Network.into(),
            parent_session_id: parent_session_id.unwrap_or_default().to_string(),
            labels,
            metadata: HashMap::new(),
            tags: vec!["prompt".to_string(), "orchestrator".to_string()],
            propagation: None,
            initial_content: None,
        };

        match self.session_manager.create_session(request).await {
            Ok(session) => {
                debug!("Created prompt session: {}", session.session_id);
                // Start the session
                if let Err(e) = self
                    .session_manager
                    .start_session(&session.session_id)
                    .await
                {
                    warn!("Failed to start prompt session: {}", e);
                }
                Some(session.session_id)
            }
            Err(e) => {
                warn!("Failed to create prompt session: {}", e);
                None
            }
        }
    }

    /// Complete a prompt session with success
    pub async fn complete_prompt_session(
        &self,
        session_id: &str,
        tokens_used: u64,
        cost: f64,
        latency_ms: u64,
    ) -> HoResult<FractalSession> {
        // Update metrics
        if let Ok(Some(mut session)) = self.session_manager.get_session(session_id).await {
            if let Some(ref mut metrics) = session.metrics {
                metrics.total_tokens_consumed = tokens_used;
                metrics.total_cost = cost;
                metrics.total_latency_ms = latency_ms;
            }
        }

        self.session_manager
            .complete_session(session_id, None)
            .await
    }

    /// Fail a prompt session with error
    pub async fn fail_prompt_session(
        &self,
        session_id: &str,
        error: &str,
    ) -> HoResult<FractalSession> {
        self.session_manager
            .fail_session(session_id, error, Some("PROMPT_ERROR"))
            .await
    }

    /// Create a session for a CosmicTask execution
    ///
    /// Used when the full orchestrator is enabled for tracking task execution.
    pub async fn create_task_session(
        &self,
        task_id: &str,
        task_type: &str,
        parent_session_id: Option<&str>,
    ) -> Option<String> {
        let mut labels = HashMap::new();
        labels.insert("task_id".to_string(), task_id.to_string());
        labels.insert("task_type".to_string(), task_type.to_string());
        labels.insert("operation".to_string(), "cosmic_task".to_string());

        let request = CreateSessionRequest {
            session_type: SessionType::Orchestration.into(),
            scope: SessionScope::Network.into(),
            parent_session_id: parent_session_id.unwrap_or_default().to_string(),
            labels,
            metadata: HashMap::new(),
            tags: vec!["task".to_string(), "orchestrator".to_string()],
            propagation: None,
            initial_content: None,
        };

        match self.session_manager.create_session(request).await {
            Ok(session) => {
                info!(
                    "Created task session {} for task {}",
                    session.session_id, task_id
                );
                if let Err(e) = self
                    .session_manager
                    .start_session(&session.session_id)
                    .await
                {
                    warn!("Failed to start task session: {}", e);
                }
                Some(session.session_id)
            }
            Err(e) => {
                warn!("Failed to create task session: {}", e);
                None
            }
        }
    }

    /// Create a child session for fractal task expansion
    ///
    /// When a CosmicTask spawns child tasks fractally, each child gets its own session
    /// linked to the parent task's session.
    pub async fn spawn_fractal_child_session(
        &self,
        parent_session_id: &str,
        child_task_id: &str,
        depth: u32,
    ) -> Option<String> {
        use ho_std::types::ergors::management::v1::SpawnChildRequest;

        let mut labels = HashMap::new();
        labels.insert("task_id".to_string(), child_task_id.to_string());
        labels.insert("fractal_depth".to_string(), depth.to_string());
        labels.insert("operation".to_string(), "fractal_child".to_string());

        let request = SpawnChildRequest {
            parent_session_id: parent_session_id.to_string(),
            child_type: SessionType::Orchestration.into(),
            child_scope: SessionScope::Network.into(),
            assigned_node_id: String::new(), // Let the system decide
            labels,
        };

        match self.session_manager.spawn_child(request).await {
            Ok(child) => {
                debug!(
                    "Spawned fractal child session {} (depth: {})",
                    child.session_id, depth
                );
                if let Err(e) = self.session_manager.start_session(&child.session_id).await {
                    warn!("Failed to start child session: {}", e);
                }
                Some(child.session_id)
            }
            Err(e) => {
                warn!("Failed to spawn fractal child session: {}", e);
                None
            }
        }
    }

    /// Get the session manager for direct access
    pub fn session_manager(&self) -> &Arc<SessionManager> {
        &self.session_manager
    }
}

#[cfg(test)]
mod tests {
    

    #[test]
    fn test_helper_construction() {
        // Just verify the struct compiles
        // Full tests require storage infrastructure
    }
}
