#![allow(unused_imports)]
//! Session Integration Tests
//!
//! Tests for session management using real Cnidarium storage.
//!
//! Note: Full SessionManager tests require network manifold setup, which is
//! tested in the integration/mod.rs. These tests focus on the storage-backed
//! session operations directly.
use crate::common::setup::{init_test_tracing, IntegrationTestHarness};
use ho_std::types::ergors::management::v1::{
    FractalSession, QuerySessionsRequest, SessionScope, SessionStatus, SessionType,
};

// ============================================================================
// Session Storage Integration Tests
// ============================================================================

/// Test creating and retrieving a session through storage
#[tokio::test]
async fn test_session_create_and_retrieve() {
    init_test_tracing();
    let harness = IntegrationTestHarness::new("session_create_retrieve")
        .await
        .unwrap();
    let storage = harness.storage();

    let now = pbjson_types::Timestamp {
        seconds: chrono::Utc::now().timestamp(),
        nanos: 0,
    };

    let mut labels = std::collections::HashMap::new();
    labels.insert("name".to_string(), "Integration Test Session".to_string());
    labels.insert("type".to_string(), "test".to_string());

    let session = FractalSession {
        session_id: "session-int-001".to_string(),
        session_type: SessionType::Agentic as i32,
        status: SessionStatus::Created as i32,
        scope: SessionScope::Local as i32,
        parent_session_id: String::new(),
        child_session_ids: vec![],
        fractal_depth: 0,
        root_session_id: "session-int-001".to_string(),
        owner_node_id: "test-node".to_string(),
        owner_node_type: 0,
        participants: vec![],
        created_at: Some(now),
        updated_at: Some(now),
        started_at: None,
        paused_at: None,
        completed_at: None,
        labels,
        metadata: std::collections::HashMap::new(),
        tags: vec!["integration".to_string(), "test".to_string()],
        state_snapshot: None,
        metrics: None,
        propagation: None,
        content: None,
    };

    // Store
    storage.put_fractal_session(&session).await.unwrap();

    // Retrieve and verify
    let retrieved = storage
        .get_fractal_session("session-int-001")
        .await
        .unwrap();
    assert!(retrieved.is_some());
    let retrieved = retrieved.unwrap();
    assert_eq!(retrieved.session_id, "session-int-001");
    assert_eq!(
        retrieved.labels.get("name"),
        Some(&"Integration Test Session".to_string())
    );
    assert_eq!(
        retrieved.tags,
        vec!["integration".to_string(), "test".to_string()]
    );
}

/// Test session lifecycle transitions
#[tokio::test]
async fn test_session_lifecycle_transitions() {
    init_test_tracing();
    let harness = IntegrationTestHarness::new("session_lifecycle")
        .await
        .unwrap();
    let storage = harness.storage();

    let now = pbjson_types::Timestamp {
        seconds: chrono::Utc::now().timestamp(),
        nanos: 0,
    };

    // Create session in Created state
    let mut session = FractalSession {
        session_id: "lifecycle-test".to_string(),
        session_type: SessionType::Agentic as i32,
        status: SessionStatus::Created as i32,
        scope: SessionScope::Local as i32,
        fractal_depth: 0,
        root_session_id: "lifecycle-test".to_string(),
        owner_node_id: "test-node".to_string(),
        created_at: Some(now),
        updated_at: Some(now),
        ..Default::default()
    };
    storage.put_fractal_session(&session).await.unwrap();

    // Transition to Active
    session.status = SessionStatus::Active as i32;
    session.started_at = Some(now);
    session.updated_at = Some(pbjson_types::Timestamp {
        seconds: now.seconds + 1,
        nanos: 0,
    });
    storage.put_fractal_session(&session).await.unwrap();

    let active = storage
        .get_fractal_session("lifecycle-test")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(active.status, SessionStatus::Active as i32);
    assert!(active.started_at.is_some());

    // Transition to Completed
    session.status = SessionStatus::Completed as i32;
    session.completed_at = Some(pbjson_types::Timestamp {
        seconds: now.seconds + 3,
        nanos: 0,
    });
    storage.put_fractal_session(&session).await.unwrap();

    let completed = storage
        .get_fractal_session("lifecycle-test")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(completed.status, SessionStatus::Completed as i32);
    assert!(completed.completed_at.is_some());
}

/// Test fractal session hierarchy creation
#[tokio::test]
async fn test_fractal_hierarchy_creation() {
    init_test_tracing();
    let harness = IntegrationTestHarness::new("fractal_hierarchy")
        .await
        .unwrap();
    let storage = harness.storage();

    let now = pbjson_types::Timestamp {
        seconds: chrono::Utc::now().timestamp(),
        nanos: 0,
    };

    // Create root session
    let root = FractalSession {
        session_id: "root".to_string(),
        session_type: SessionType::Agentic as i32,
        status: SessionStatus::Active as i32,
        scope: SessionScope::Network as i32,
        parent_session_id: String::new(),
        child_session_ids: vec!["child-a".to_string(), "child-b".to_string()],
        fractal_depth: 0,
        root_session_id: "root".to_string(),
        owner_node_id: "coordinator".to_string(),
        owner_node_type: 1,
        created_at: Some(now),
        updated_at: Some(now),
        ..Default::default()
    };
    storage.put_fractal_session(&root).await.unwrap();

    // Create first-level children
    for id in ["child-a", "child-b"] {
        let child = FractalSession {
            session_id: id.to_string(),
            session_type: SessionType::Agentic as i32,
            status: SessionStatus::Active as i32,
            scope: SessionScope::Local as i32,
            parent_session_id: "root".to_string(),
            child_session_ids: vec![],
            fractal_depth: 1,
            root_session_id: "root".to_string(),
            owner_node_id: format!("executor-{}", id),
            owner_node_type: 2,
            created_at: Some(now),
            updated_at: Some(now),
            ..Default::default()
        };
        storage.put_fractal_session(&child).await.unwrap();
    }

    // Query and verify hierarchy
    let root_children = storage.get_sessions_by_parent("root").await.unwrap();
    assert_eq!(root_children.len(), 2);

    // get_sessions_by_root returns all sessions with this root_session_id (including root itself)
    let all_from_root = storage.get_sessions_by_root("root").await.unwrap();
    assert_eq!(all_from_root.len(), 3); // root + 2 children

    // Verify the root session is included with depth 0
    assert!(all_from_root
        .iter()
        .any(|s| s.session_id == "root" && s.fractal_depth == 0));

    // Verify children have depth 1
    let children: Vec<_> = all_from_root
        .iter()
        .filter(|s| s.fractal_depth == 1)
        .collect();
    assert_eq!(children.len(), 2);
}

/// Test session query with multiple filters
#[tokio::test]
async fn test_session_complex_query() {
    init_test_tracing();
    let harness = IntegrationTestHarness::new("session_complex_query")
        .await
        .unwrap();
    let storage = harness.storage();

    let now = pbjson_types::Timestamp {
        seconds: chrono::Utc::now().timestamp(),
        nanos: 0,
    };

    // Create sessions with various properties
    // Using Agentic, Orchestration, and Proxy types (no Background type exists)
    let sessions = vec![
        ("s1", SessionStatus::Active, SessionType::Agentic),
        ("s2", SessionStatus::Active, SessionType::Agentic),
        ("s3", SessionStatus::Paused, SessionType::Agentic),
        ("s4", SessionStatus::Completed, SessionType::Orchestration),
        ("s5", SessionStatus::Active, SessionType::Orchestration),
    ];

    for (id, status, stype) in sessions {
        let session = FractalSession {
            session_id: id.to_string(),
            session_type: stype as i32,
            status: status as i32,
            scope: SessionScope::Local as i32,
            fractal_depth: 0,
            root_session_id: id.to_string(),
            owner_node_id: "test-node".to_string(),
            created_at: Some(now),
            updated_at: Some(now),
            ..Default::default()
        };
        storage.put_fractal_session(&session).await.unwrap();
    }

    // Query active sessions using single status field
    let query = QuerySessionsRequest {
        status: SessionStatus::Active as i32,
        ..Default::default()
    };
    let active = storage.query_fractal_sessions(&query).await.unwrap();
    assert_eq!(active.len(), 3); // s1, s2, s5

    // Query by type using single session_type field
    let query = QuerySessionsRequest {
        session_type: SessionType::Agentic as i32,
        ..Default::default()
    };
    let agentic = storage.query_fractal_sessions(&query).await.unwrap();
    assert_eq!(agentic.len(), 3); // s1, s2, s3
}
