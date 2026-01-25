//! Mock Storage for Testing
//!
//! In-memory storage for sessions, workflows, and grant requests.
//! Mirrors the storage patterns of the real Cnidarium-backed storage.

use super::types::{GrantRequestState, GrantRequestStatus};
use ho_std::types::ergors::management::v1::FractalSession;
use ho_std::types::ergors::orch::v1::AkashDeploymentWorkflow;
use ho_std::utils::IdGenerator;
use std::collections::HashMap;

/// In-memory storage for mock client testing.
pub struct MockStorage {
    sessions: HashMap<String, FractalSession>,
    workflows: HashMap<String, AkashDeploymentWorkflow>,
    grant_requests: HashMap<String, GrantRequestState>,
    kv_store: HashMap<Vec<u8>, Vec<u8>>,
}

impl Default for MockStorage {
    fn default() -> Self {
        Self::new()
    }
}

impl MockStorage {
    /// Create new empty storage.
    pub fn new() -> Self {
        Self {
            sessions: HashMap::new(),
            workflows: HashMap::new(),
            grant_requests: HashMap::new(),
            kv_store: HashMap::new(),
        }
    }

    // =========================================================================
    // Session Operations
    // =========================================================================

    /// Create a new session.
    pub fn create_session(&mut self, name: String) -> FractalSession {
        use ho_std::types::ergors::management::v1::{SessionStatus, SessionType, SessionScope};

        let session_id = IdGenerator::new_uuid_string();
        let now = chrono::Utc::now();

        let mut labels = std::collections::HashMap::new();
        labels.insert("name".to_string(), name);

        let session = FractalSession {
            session_id: session_id.clone(),
            session_type: SessionType::Agentic as i32,
            status: SessionStatus::Active as i32,
            scope: SessionScope::Local as i32,
            parent_session_id: String::new(),
            child_session_ids: Vec::new(),
            fractal_depth: 0,
            root_session_id: session_id.clone(),
            owner_node_id: String::new(),
            owner_node_type: 0,
            participants: Vec::new(),
            created_at: Some(pbjson_types::Timestamp {
                seconds: now.timestamp(),
                nanos: now.timestamp_subsec_nanos() as i32,
            }),
            updated_at: None,
            started_at: None,
            paused_at: None,
            completed_at: None,
            labels,
            metadata: std::collections::HashMap::new(),
            tags: Vec::new(),
            state_snapshot: None,
            metrics: None,
            propagation: None,
            content: None,
        };

        self.sessions.insert(session_id, session.clone());
        session
    }

    /// Get session by ID.
    pub fn get_session(&self, session_id: &str) -> Option<&FractalSession> {
        self.sessions.get(session_id)
    }

    /// Get mutable session by ID.
    pub fn get_session_mut(&mut self, session_id: &str) -> Option<&mut FractalSession> {
        self.sessions.get_mut(session_id)
    }

    /// Update session.
    pub fn put_session(&mut self, session: FractalSession) {
        self.sessions.insert(session.session_id.clone(), session);
    }

    /// Delete session.
    pub fn delete_session(&mut self, session_id: &str) -> Option<FractalSession> {
        self.sessions.remove(session_id)
    }

    /// List all sessions.
    pub fn list_sessions(&self) -> Vec<FractalSession> {
        self.sessions.values().cloned().collect()
    }

    /// Query sessions by parent.
    pub fn query_sessions_by_parent(&self, parent_id: &str) -> Vec<FractalSession> {
        self.sessions
            .values()
            .filter(|s| s.parent_session_id == parent_id)
            .cloned()
            .collect()
    }

    // =========================================================================
    // Workflow Operations
    // =========================================================================

    /// Store workflow.
    pub fn put_workflow(&mut self, workflow: AkashDeploymentWorkflow) {
        self.workflows
            .insert(workflow.session_id.clone(), workflow);
    }

    /// Get workflow by session ID.
    pub fn get_workflow(&self, session_id: &str) -> Option<&AkashDeploymentWorkflow> {
        self.workflows.get(session_id)
    }

    /// Get mutable workflow.
    pub fn get_workflow_mut(&mut self, session_id: &str) -> Option<&mut AkashDeploymentWorkflow> {
        self.workflows.get_mut(session_id)
    }

    /// Delete workflow.
    pub fn delete_workflow(&mut self, session_id: &str) -> Option<AkashDeploymentWorkflow> {
        self.workflows.remove(session_id)
    }

    /// List all workflows.
    pub fn list_workflows(&self) -> Vec<AkashDeploymentWorkflow> {
        self.workflows.values().cloned().collect()
    }

    /// Update workflow with closure.
    pub fn update_workflow<F>(&mut self, session_id: &str, updater: F) -> bool
    where
        F: FnOnce(&mut AkashDeploymentWorkflow),
    {
        if let Some(workflow) = self.workflows.get_mut(session_id) {
            updater(workflow);
            true
        } else {
            false
        }
    }

    // =========================================================================
    // Grant Request Operations
    // =========================================================================

    /// Add a grant request.
    pub fn add_grant_request(&mut self, request: GrantRequestState) {
        self.grant_requests
            .insert(request.request_id.clone(), request);
    }

    /// Get grant request by ID.
    pub fn get_grant_request(&self, request_id: &str) -> Option<&GrantRequestState> {
        self.grant_requests.get(request_id)
    }

    /// Update grant request.
    pub fn update_grant_request(&mut self, request: GrantRequestState) {
        self.grant_requests
            .insert(request.request_id.clone(), request);
    }

    /// List all grant requests.
    pub fn list_grant_requests(&self) -> Vec<GrantRequestState> {
        self.grant_requests.values().cloned().collect()
    }

    /// Query grant requests by status.
    pub fn query_grant_requests_by_status(
        &self,
        status: GrantRequestStatus,
    ) -> Vec<GrantRequestState> {
        self.grant_requests
            .values()
            .filter(|r| r.status == status)
            .cloned()
            .collect()
    }

    /// Query grant requests by granter.
    pub fn query_grant_requests_by_granter(&self, granter: &str) -> Vec<GrantRequestState> {
        self.grant_requests
            .values()
            .filter(|r| r.granter_address == granter)
            .cloned()
            .collect()
    }

    /// Query grant requests by grantee.
    pub fn query_grant_requests_by_grantee(&self, grantee: &str) -> Vec<GrantRequestState> {
        self.grant_requests
            .values()
            .filter(|r| r.grantee_address == grantee)
            .cloned()
            .collect()
    }

    // =========================================================================
    // Generic KV Operations
    // =========================================================================

    /// Put raw key-value.
    pub fn kv_put(&mut self, key: Vec<u8>, value: Vec<u8>) {
        self.kv_store.insert(key, value);
    }

    /// Get raw value.
    pub fn kv_get(&self, key: &[u8]) -> Option<&Vec<u8>> {
        self.kv_store.get(key)
    }

    /// Delete raw key.
    pub fn kv_delete(&mut self, key: &[u8]) -> Option<Vec<u8>> {
        self.kv_store.remove(key)
    }

    /// Check if key exists.
    pub fn kv_exists(&self, key: &[u8]) -> bool {
        self.kv_store.contains_key(key)
    }

    // =========================================================================
    // Statistics / Introspection
    // =========================================================================

    /// Get total number of sessions.
    pub fn session_count(&self) -> usize {
        self.sessions.len()
    }

    /// Get total number of workflows.
    pub fn workflow_count(&self) -> usize {
        self.workflows.len()
    }

    /// Get total number of grant requests.
    pub fn grant_request_count(&self) -> usize {
        self.grant_requests.len()
    }

    /// Clear all data (useful between tests).
    pub fn clear(&mut self) {
        self.sessions.clear();
        self.workflows.clear();
        self.grant_requests.clear();
        self.kv_store.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ho_std::types::ergors::orch::v1::{AkashWorkflowStatus, AkashWorkflowStep};

    #[test]
    fn test_session_crud() {
        let mut storage = MockStorage::new();

        // Create
        let session = storage.create_session("test-session".to_string());
        let session_id = session.session_id.clone();
        assert!(!session_id.is_empty());
        assert_eq!(session.labels.get("name"), Some(&"test-session".to_string()));

        // Read
        let retrieved = storage.get_session(&session_id).unwrap().clone();
        assert_eq!(retrieved.labels.get("name"), Some(&"test-session".to_string()));

        // Update
        let mut to_update = retrieved;
        to_update.labels.insert("name".to_string(), "updated-name".to_string());
        storage.put_session(to_update);

        let updated = storage.get_session(&session_id).unwrap();
        assert_eq!(updated.labels.get("name"), Some(&"updated-name".to_string()));

        // Delete
        storage.delete_session(&session_id);
        assert!(storage.get_session(&session_id).is_none());
    }

    #[test]
    fn test_workflow_crud() {
        let mut storage = MockStorage::new();

        let workflow = AkashDeploymentWorkflow {
            session_id: "test-workflow".to_string(),
            current_step: AkashWorkflowStep::KeySelection as i32,
            status: AkashWorkflowStatus::Pending as i32,
            ..Default::default()
        };

        // Create
        storage.put_workflow(workflow.clone());

        // Read
        let retrieved = storage.get_workflow("test-workflow").unwrap();
        assert_eq!(retrieved.current_step, AkashWorkflowStep::KeySelection as i32);

        // Update
        storage.update_workflow("test-workflow", |w| {
            w.current_step = AkashWorkflowStep::BalanceCheck as i32;
        });

        let updated = storage.get_workflow("test-workflow").unwrap();
        assert_eq!(updated.current_step, AkashWorkflowStep::BalanceCheck as i32);

        // Delete
        storage.delete_workflow("test-workflow");
        assert!(storage.get_workflow("test-workflow").is_none());
    }

    #[test]
    fn test_grant_request_operations() {
        let mut storage = MockStorage::new();

        let request = GrantRequestState {
            request_id: "req-1".to_string(),
            granter_address: "akash1granter".to_string(),
            grantee_address: "akash1grantee".to_string(),
            msg_types: vec!["/akash.deployment.v1beta3.MsgCreateDeployment".to_string()],
            spend_limit_uakt: 1_000_000,
            duration_seconds: 86400,
            status: GrantRequestStatus::Pending,
            created_at_unix: 0,
            rejection_reason: None,
        };

        storage.add_grant_request(request);

        // Query by status
        let pending = storage.query_grant_requests_by_status(GrantRequestStatus::Pending);
        assert_eq!(pending.len(), 1);

        // Query by granter
        let by_granter = storage.query_grant_requests_by_granter("akash1granter");
        assert_eq!(by_granter.len(), 1);

        // Update status
        let mut updated = storage.get_grant_request("req-1").unwrap().clone();
        updated.status = GrantRequestStatus::Confirmed;
        storage.update_grant_request(updated);

        let confirmed = storage.query_grant_requests_by_status(GrantRequestStatus::Confirmed);
        assert_eq!(confirmed.len(), 1);
    }

    #[test]
    fn test_kv_operations() {
        let mut storage = MockStorage::new();

        storage.kv_put(b"key1".to_vec(), b"value1".to_vec());

        assert!(storage.kv_exists(b"key1"));
        assert!(!storage.kv_exists(b"key2"));

        let value = storage.kv_get(b"key1").unwrap();
        assert_eq!(value, b"value1");

        storage.kv_delete(b"key1");
        assert!(!storage.kv_exists(b"key1"));
    }

    #[test]
    fn test_clear() {
        let mut storage = MockStorage::new();

        storage.create_session("session1".to_string());
        storage.kv_put(b"key".to_vec(), b"value".to_vec());

        assert_eq!(storage.session_count(), 1);

        storage.clear();

        assert_eq!(storage.session_count(), 0);
        assert!(!storage.kv_exists(b"key"));
    }
}
