//! Storage Integration Tests
//!
//! Tests for real ErgorsStorage with Cnidarium backend.
//! These tests verify actual persistence, prefix scanning, atomic commits,
//! and snapshot isolation - NOT mocks.

use crate::common::{init_test_tracing, IntegrationTestHarness};
use ho_std::types::ergors::management::v1::{
    FractalSession, QuerySessionsRequest, SessionScope, SessionStatus, SessionType,
};
use ho_std::types::ergors::orch::v1::{
    AkashDeploymentWorkflow, AkashWorkflowStatus, AkashWorkflowStep,
};

// ============================================================================
// Basic Storage Tests
// ============================================================================

#[tokio::test]
async fn test_storage_health_check() {
    init_test_tracing();
    let harness = IntegrationTestHarness::new("health_check").await.unwrap();

    let result = harness.storage().health_check().await;
    assert!(result.is_ok(), "Health check failed: {:?}", result);
}

#[tokio::test]
async fn test_storage_snapshot_creation() {
    init_test_tracing();
    let harness = IntegrationTestHarness::new("snapshot_test").await.unwrap();

    // Create a snapshot - should not fail
    let result = harness.storage().create_snapshot().await;
    assert!(result.is_ok(), "Snapshot creation failed: {:?}", result);
}

// ============================================================================
// Akash Workflow Storage Tests
// ============================================================================

#[tokio::test]
async fn test_akash_workflow_crud() {
    init_test_tracing();
    let harness = IntegrationTestHarness::new("workflow_crud").await.unwrap();
    let storage = harness.storage();

    let session_id = "test-workflow-001";

    // Create workflow
    let workflow = AkashDeploymentWorkflow {
        session_id: session_id.to_string(),
        current_step: AkashWorkflowStep::KeySelection as i32,
        status: AkashWorkflowStatus::Pending as i32,
        selected_key_name: "test-key".to_string(),
        account_address: "akash1test".to_string(),
        hd_account_index: 0,
        chain_id: "akashnet-2".to_string(),
        node_endpoint: "https://rpc.akash.network".to_string(),
        ..Default::default()
    };

    // PUT
    let put_result = storage.put_akash_workflow(&workflow).await;
    assert!(put_result.is_ok(), "Failed to put workflow: {:?}", put_result);

    // GET
    let get_result = storage.get_akash_workflow(session_id).await;
    assert!(get_result.is_ok(), "Failed to get workflow: {:?}", get_result);
    let retrieved = get_result.unwrap();
    assert!(retrieved.is_some(), "Workflow not found after put");
    let retrieved = retrieved.unwrap();
    assert_eq!(retrieved.session_id, session_id);
    assert_eq!(retrieved.selected_key_name, "test-key");
    assert_eq!(retrieved.current_step, AkashWorkflowStep::KeySelection as i32);

    // UPDATE
    let mut updated = retrieved.clone();
    updated.current_step = AkashWorkflowStep::BalanceCheck as i32;
    updated.status = AkashWorkflowStatus::Running as i32;
    storage.put_akash_workflow(&updated).await.unwrap();

    let after_update = storage.get_akash_workflow(session_id).await.unwrap().unwrap();
    assert_eq!(after_update.current_step, AkashWorkflowStep::BalanceCheck as i32);
    assert_eq!(after_update.status, AkashWorkflowStatus::Running as i32);

    // DELETE
    let delete_result = storage.delete_akash_workflow(session_id).await;
    assert!(delete_result.is_ok(), "Failed to delete workflow: {:?}", delete_result);

    let after_delete = storage.get_akash_workflow(session_id).await.unwrap();
    assert!(after_delete.is_none(), "Workflow still exists after delete");
}

#[tokio::test]
async fn test_akash_workflow_list() {
    init_test_tracing();
    let harness = IntegrationTestHarness::new("workflow_list").await.unwrap();
    let storage = harness.storage();

    // Create multiple workflows
    for i in 0..5 {
        let workflow = AkashDeploymentWorkflow {
            session_id: format!("workflow-{}", i),
            current_step: AkashWorkflowStep::KeySelection as i32,
            status: if i % 2 == 0 {
                AkashWorkflowStatus::Pending as i32
            } else {
                AkashWorkflowStatus::Running as i32
            },
            selected_key_name: format!("key-{}", i),
            account_address: format!("akash1test{}", i),
            ..Default::default()
        };
        storage.put_akash_workflow(&workflow).await.unwrap();
    }

    // List all
    let all_workflows = storage.list_akash_workflows().await.unwrap();
    assert_eq!(all_workflows.len(), 5, "Expected 5 workflows, got {}", all_workflows.len());

    // Verify all workflows are retrievable
    for workflow in &all_workflows {
        let retrieved = storage.get_akash_workflow(&workflow.session_id).await.unwrap();
        assert!(retrieved.is_some());
    }
}

// ============================================================================
// Fractal Session Storage Tests
// ============================================================================

#[tokio::test]
async fn test_fractal_session_crud() {
    init_test_tracing();
    let harness = IntegrationTestHarness::new("session_crud").await.unwrap();
    let storage = harness.storage();

    let session_id = "test-session-001";
    let now = pbjson_types::Timestamp {
        seconds: chrono::Utc::now().timestamp(),
        nanos: 0,
    };

    // Create session
    let mut labels = std::collections::HashMap::new();
    labels.insert("name".to_string(), "Test Session".to_string());
    labels.insert("env".to_string(), "test".to_string());

    let session = FractalSession {
        session_id: session_id.to_string(),
        session_type: SessionType::Agentic as i32,
        status: SessionStatus::Created as i32,
        scope: SessionScope::Local as i32,
        parent_session_id: String::new(),
        child_session_ids: vec![],
        fractal_depth: 0,
        root_session_id: session_id.to_string(),
        owner_node_id: "test-node".to_string(),
        owner_node_type: 0,
        participants: vec![],
        created_at: Some(now.clone()),
        updated_at: Some(now.clone()),
        started_at: None,
        paused_at: None,
        completed_at: None,
        labels,
        metadata: std::collections::HashMap::new(),
        tags: vec!["integration-test".to_string()],
        state_snapshot: None,
        metrics: None,
        propagation: None,
        content: None,
    };

    // PUT
    let put_result = storage.put_fractal_session(&session).await;
    assert!(put_result.is_ok(), "Failed to put session: {:?}", put_result);

    // GET
    let get_result = storage.get_fractal_session(session_id).await;
    assert!(get_result.is_ok(), "Failed to get session: {:?}", get_result);
    let retrieved = get_result.unwrap();
    assert!(retrieved.is_some(), "Session not found after put");
    let retrieved = retrieved.unwrap();
    assert_eq!(retrieved.session_id, session_id);
    assert_eq!(retrieved.labels.get("name"), Some(&"Test Session".to_string()));

    // UPDATE
    let mut updated = retrieved.clone();
    updated.status = SessionStatus::Active as i32;
    updated.labels.insert("updated".to_string(), "true".to_string());
    storage.put_fractal_session(&updated).await.unwrap();

    let after_update = storage.get_fractal_session(session_id).await.unwrap().unwrap();
    assert_eq!(after_update.status, SessionStatus::Active as i32);
    assert_eq!(after_update.labels.get("updated"), Some(&"true".to_string()));

    // DELETE
    let delete_result = storage.delete_fractal_session(session_id).await;
    assert!(delete_result.is_ok(), "Failed to delete session: {:?}", delete_result);

    let after_delete = storage.get_fractal_session(session_id).await.unwrap();
    assert!(after_delete.is_none(), "Session still exists after delete");
}

#[tokio::test]
async fn test_fractal_session_parent_child_hierarchy() {
    init_test_tracing();
    let harness = IntegrationTestHarness::new("session_hierarchy").await.unwrap();
    let storage = harness.storage();

    let now = pbjson_types::Timestamp {
        seconds: chrono::Utc::now().timestamp(),
        nanos: 0,
    };

    // Create root session
    let root_session = FractalSession {
        session_id: "root-session".to_string(),
        session_type: SessionType::Agentic as i32,
        status: SessionStatus::Active as i32,
        scope: SessionScope::Network as i32,
        parent_session_id: String::new(),
        child_session_ids: vec!["child-1".to_string(), "child-2".to_string()],
        fractal_depth: 0,
        root_session_id: "root-session".to_string(),
        owner_node_id: "coordinator".to_string(),
        owner_node_type: 1,
        created_at: Some(now.clone()),
        updated_at: Some(now.clone()),
        ..Default::default()
    };
    storage.put_fractal_session(&root_session).await.unwrap();

    // Create child sessions
    for i in 1..=2 {
        let child = FractalSession {
            session_id: format!("child-{}", i),
            session_type: SessionType::Agentic as i32,
            status: SessionStatus::Active as i32,
            scope: SessionScope::Local as i32,
            parent_session_id: "root-session".to_string(),
            child_session_ids: vec![],
            fractal_depth: 1,
            root_session_id: "root-session".to_string(),
            owner_node_id: format!("executor-{}", i),
            owner_node_type: 2,
            created_at: Some(now.clone()),
            updated_at: Some(now.clone()),
            ..Default::default()
        };
        storage.put_fractal_session(&child).await.unwrap();
    }

    // Query by parent
    let children = storage.get_sessions_by_parent("root-session").await.unwrap();
    assert_eq!(children.len(), 2, "Expected 2 children, got {}", children.len());

    // Query by root (includes root session itself + 2 children)
    let by_root = storage.get_sessions_by_root("root-session").await.unwrap();
    assert_eq!(by_root.len(), 3, "Expected 3 sessions by root (root + 2 children), got {}", by_root.len());

    // Verify hierarchy
    for child in &children {
        assert_eq!(child.parent_session_id, "root-session");
        assert_eq!(child.root_session_id, "root-session");
        assert_eq!(child.fractal_depth, 1);
    }
}

#[tokio::test]
async fn test_fractal_session_query_by_status() {
    init_test_tracing();
    let harness = IntegrationTestHarness::new("session_query_status").await.unwrap();
    let storage = harness.storage();

    let now = pbjson_types::Timestamp {
        seconds: chrono::Utc::now().timestamp(),
        nanos: 0,
    };

    // Create sessions with different statuses
    let statuses = [
        SessionStatus::Created,
        SessionStatus::Active,
        SessionStatus::Active,
        SessionStatus::Paused,
        SessionStatus::Completed,
    ];

    for (i, status) in statuses.iter().enumerate() {
        let session = FractalSession {
            session_id: format!("status-test-{}", i),
            session_type: SessionType::Agentic as i32,
            status: *status as i32,
            scope: SessionScope::Local as i32,
            fractal_depth: 0,
            root_session_id: format!("status-test-{}", i),
            owner_node_id: "test-node".to_string(),
            created_at: Some(now.clone()),
            updated_at: Some(now.clone()),
            ..Default::default()
        };
        storage.put_fractal_session(&session).await.unwrap();
    }

    // Query by status using single status field
    let query = QuerySessionsRequest {
        status: SessionStatus::Active as i32,
        ..Default::default()
    };
    let active_sessions = storage.query_fractal_sessions(&query).await.unwrap();
    assert_eq!(active_sessions.len(), 2, "Expected 2 active sessions, got {}", active_sessions.len());

    for session in &active_sessions {
        assert_eq!(session.status, SessionStatus::Active as i32);
    }
}

#[tokio::test]
async fn test_multiple_sequential_workflow_writes() {
    init_test_tracing();
    let harness = IntegrationTestHarness::new("sequential_writes").await.unwrap();
    let storage = harness.storage().clone();

    // Write multiple workflows sequentially
    // (cnidarium uses optimistic locking, so truly concurrent writes to different keys
    // still conflict at the commit level - this tests the expected pattern)
    for i in 0..10 {
        let workflow = AkashDeploymentWorkflow {
            session_id: format!("sequential-{}", i),
            current_step: AkashWorkflowStep::KeySelection as i32,
            status: AkashWorkflowStatus::Pending as i32,
            ..Default::default()
        };
        storage.put_akash_workflow(&workflow).await.unwrap();
    }

    // Verify all workflows exist
    let workflows = storage.list_akash_workflows().await.unwrap();
    assert_eq!(workflows.len(), 10, "Expected 10 workflows after sequential writes, got {}", workflows.len());

    // Verify each one is retrievable
    for i in 0..10 {
        let workflow = storage.get_akash_workflow(&format!("sequential-{}", i)).await.unwrap();
        assert!(workflow.is_some(), "Workflow sequential-{} should exist", i);
    }
}
