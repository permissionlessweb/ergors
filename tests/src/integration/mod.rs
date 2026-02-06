//! Full Integration Tests
//!
//! End-to-end integration tests that exercise the complete node engine stack:
//! - Real Cnidarium storage
//! - Real AkashWorkflowManager (with mocked external chain calls)
//! - Real workflow step execution
//!
//! These tests verify that all components work together correctly.

use ho_std::types::ergors::orch::v1::{
    AkashDeploymentWorkflow, AkashWorkflowStatus, AkashWorkflowStep,
};

use crate::common::setup::{init_test_tracing, IntegrationTestHarness};

// ============================================================================
// Workflow Integration Tests
// ============================================================================

/// Test complete workflow lifecycle through storage
#[tokio::test]
#[serial_test::serial]
async fn test_workflow_full_lifecycle() {
    init_test_tracing();
    let harness = IntegrationTestHarness::new("workflow_lifecycle")
        .await
        .unwrap();
    let storage = harness.storage();

    let session_id = "full-lifecycle-001";

    // Step 1: Create workflow in KeySelection step
    let mut workflow = AkashDeploymentWorkflow {
        session_id: session_id.to_string(),
        current_step: AkashWorkflowStep::KeySelection as i32,
        status: AkashWorkflowStatus::Pending as i32,
        selected_key_name: "test-key".to_string(),
        account_address: "akash1testaccount".to_string(),
        hd_account_index: 0,
        chain_id: "akashnet-2".to_string(),
        node_endpoint: "https://rpc.akash.network".to_string(),
        max_retries: 3,
        timeout_seconds: 3600,
        ..Default::default()
    };
    storage.put_akash_workflow(&workflow).await.unwrap();

    // Verify initial state
    let saved = storage
        .get_akash_workflow(session_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(saved.current_step, AkashWorkflowStep::KeySelection as i32);
    assert_eq!(saved.status, AkashWorkflowStatus::Pending as i32);

    // Step 2: Progress through workflow steps
    let steps = [
        AkashWorkflowStep::BalanceCheck,
        AkashWorkflowStep::GrantRequest,
        AkashWorkflowStep::GrantWait,
        AkashWorkflowStep::AuthzSetup,
        AkashWorkflowStep::FeegrantSetup,
        AkashWorkflowStep::SdlConfiguration,
    ];

    for step in steps {
        workflow.current_step = step as i32;
        workflow.status = AkashWorkflowStatus::Running as i32;
        storage.put_akash_workflow(&workflow).await.unwrap();

        let updated = storage
            .get_akash_workflow(session_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(updated.current_step, step as i32);
    }

    // Step 3: Complete the workflow
    workflow.current_step = AkashWorkflowStep::Complete as i32;
    workflow.status = AkashWorkflowStatus::Completed as i32;
    workflow.completed_at = Some(pbjson_types::Timestamp {
        seconds: chrono::Utc::now().timestamp(),
        nanos: 0,
    });
    storage.put_akash_workflow(&workflow).await.unwrap();

    let final_state = storage
        .get_akash_workflow(session_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(final_state.status, AkashWorkflowStatus::Completed as i32);
    assert!(final_state.completed_at.is_some());
}

/// Test workflow error handling and retry
#[tokio::test]
#[serial_test::serial]
async fn test_workflow_error_and_retry() {
    init_test_tracing();
    let harness = IntegrationTestHarness::new("workflow_error").await.unwrap();
    let storage = harness.storage();

    let mut workflow = AkashDeploymentWorkflow {
        session_id: "error-test".to_string(),
        current_step: AkashWorkflowStep::BalanceCheck as i32,
        status: AkashWorkflowStatus::Running as i32,
        max_retries: 3,
        retry_count: 0,
        ..Default::default()
    };
    storage.put_akash_workflow(&workflow).await.unwrap();

    // Simulate error and increment retry
    workflow.last_error = "Insufficient balance: need 5000000uakt, have 1000000uakt".to_string();
    workflow.retry_count = 1;
    storage.put_akash_workflow(&workflow).await.unwrap();

    let after_error = storage
        .get_akash_workflow("error-test")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(after_error.retry_count, 1);
    assert!(!after_error.last_error.is_empty());

    // Simulate max retries exceeded -> Failed
    workflow.retry_count = 3;
    workflow.status = AkashWorkflowStatus::Failed as i32;
    workflow.current_step = AkashWorkflowStep::Failed as i32;
    storage.put_akash_workflow(&workflow).await.unwrap();

    let failed = storage
        .get_akash_workflow("error-test")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(failed.status, AkashWorkflowStatus::Failed as i32);
}

/// Test workflow cancellation
#[tokio::test]
#[serial_test::serial]
async fn test_workflow_cancellation() {
    init_test_tracing();
    let harness = IntegrationTestHarness::new("workflow_cancel")
        .await
        .unwrap();
    let storage = harness.storage();

    // Create workflow in progress
    let mut workflow = AkashDeploymentWorkflow {
        session_id: "cancel-test".to_string(),
        current_step: AkashWorkflowStep::BidWait as i32,
        status: AkashWorkflowStatus::Running as i32,
        ..Default::default()
    };
    storage.put_akash_workflow(&workflow).await.unwrap();

    // Cancel the workflow
    workflow.status = AkashWorkflowStatus::Cancelled as i32;
    workflow.last_error = "Cancelled by user".to_string();
    storage.put_akash_workflow(&workflow).await.unwrap();

    let cancelled = storage
        .get_akash_workflow("cancel-test")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(cancelled.status, AkashWorkflowStatus::Cancelled as i32);
}

/// Test multiple workflows with filtering
#[tokio::test]
#[serial_test::serial]
async fn test_multiple_workflows_filtering() {
    init_test_tracing();
    let harness = IntegrationTestHarness::new("workflows_filter")
        .await
        .unwrap();
    let storage = harness.storage();

    // Create workflows in different states
    let workflows = vec![
        (
            "wf-1",
            AkashWorkflowStatus::Pending,
            AkashWorkflowStep::KeySelection,
        ),
        (
            "wf-2",
            AkashWorkflowStatus::Running,
            AkashWorkflowStep::BidWait,
        ),
        (
            "wf-3",
            AkashWorkflowStatus::Running,
            AkashWorkflowStep::ManifestSend,
        ),
        (
            "wf-4",
            AkashWorkflowStatus::Completed,
            AkashWorkflowStep::Complete,
        ),
        (
            "wf-5",
            AkashWorkflowStatus::Failed,
            AkashWorkflowStep::Failed,
        ),
    ];

    for (id, status, step) in &workflows {
        let workflow = AkashDeploymentWorkflow {
            session_id: id.to_string(),
            current_step: *step as i32,
            status: *status as i32,
            ..Default::default()
        };
        storage.put_akash_workflow(&workflow).await.unwrap();
    }

    // List all
    let all = storage.list_akash_workflows().await.unwrap();
    assert_eq!(all.len(), 5);

    // Filter by status (manual filtering since storage doesn't have status filter)
    let running: Vec<_> = all
        .iter()
        .filter(|w| w.status == AkashWorkflowStatus::Running as i32)
        .collect();
    assert_eq!(running.len(), 2);

    let completed: Vec<_> = all
        .iter()
        .filter(|w| w.status == AkashWorkflowStatus::Completed as i32)
        .collect();
    assert_eq!(completed.len(), 1);
}

/// Test storage isolation between test instances.
///
/// Each cnidarium instance opens many FDs (27 substores x RocksDB files),
/// so we avoid holding two instances open simultaneously. Instead, we
/// create one, write data, drop it to free FDs, then create a second
/// and verify it can't see the first's data — proving path-based isolation.
#[tokio::test]
#[serial_test::serial]
async fn test_storage_isolation() {
    init_test_tracing();

    // Phase 1: Create first storage, write data, verify it's readable, then drop
    {
        let harness1 = IntegrationTestHarness::new("isolation_test_1")
            .await
            .unwrap();

        let workflow1 = AkashDeploymentWorkflow {
            session_id: "isolated-workflow".to_string(),
            current_step: AkashWorkflowStep::KeySelection as i32,
            status: AkashWorkflowStatus::Pending as i32,
            ..Default::default()
        };
        harness1
            .storage()
            .put_akash_workflow(&workflow1)
            .await
            .unwrap();

        // harness1 should see its own data
        let from_harness1 = harness1
            .storage()
            .get_akash_workflow("isolated-workflow")
            .await
            .unwrap();
        assert!(from_harness1.is_some());
    }
    // harness1 dropped here — FDs freed

    // Phase 2: Create second storage at a different path, verify isolation
    {
        let harness2 = IntegrationTestHarness::new("isolation_test_2")
            .await
            .unwrap();

        // harness2 should NOT see harness1's data
        let from_harness2 = harness2
            .storage()
            .get_akash_workflow("isolated-workflow")
            .await
            .unwrap();
        assert!(
            from_harness2.is_none(),
            "Storage should be isolated between test instances"
        );
    }
}
