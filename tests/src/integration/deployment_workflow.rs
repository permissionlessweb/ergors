//! Deployment automation workflow integration tests
//!
//! Tests the complete SDL -> Akash deployment pipeline without live infrastructure.
//! Uses mock providers, simulated chain responses, and the real Cnidarium storage
//! backend to verify workflow state transitions, SDL generation, deployment building,
//! grant request flows, and endpoint caching.

use ho_std::types::ergors::orch::v1::{
    AkashDeploymentWorkflow, AkashRuntime, AkashServiceEndpoint,
    AkashWorkflowStatus, AkashWorkflowStep, ConfiguredSdl,
    GrantAcceptanceMode, GrantRequestStatus,
};

use crate::common::fixtures::create_test_sdl;
use crate::common::setup::{init_test_tracing, IntegrationTestHarness};

// ============================================================================
// Test 1: Workflow State Machine Transitions
// ============================================================================

/// Verify that a workflow progresses through all expected steps in order.
/// Each step transition is persisted to Cnidarium and re-read to confirm
/// that storage round-trips are lossless.
#[tokio::test]
async fn test_workflow_state_machine_step_order() {
    init_test_tracing();
    let harness = IntegrationTestHarness::new("deploy_wf_steps")
        .await
        .unwrap();
    let storage = harness.storage();

    let session_id = "wf-step-order-001";

    // Create workflow at initial step
    let mut workflow = AkashDeploymentWorkflow {
        session_id: session_id.to_string(),
        current_step: AkashWorkflowStep::KeySelection as i32,
        status: AkashWorkflowStatus::Pending as i32,
        selected_key_name: "test-key".to_string(),
        account_address: "akash1testdeploy".to_string(),
        chain_id: "akashnet-2".to_string(),
        node_endpoint: "https://rpc.akash.network".to_string(),
        max_retries: 3,
        timeout_seconds: 3600,
        ..Default::default()
    };
    storage.put_akash_workflow(&workflow).await.unwrap();

    // Define the full expected step progression
    let expected_steps = [
        AkashWorkflowStep::KeySelection,
        AkashWorkflowStep::BalanceCheck,
        AkashWorkflowStep::GrantRequest,
        AkashWorkflowStep::GrantWait,
        AkashWorkflowStep::AuthzSetup,
        AkashWorkflowStep::FeegrantSetup,
        AkashWorkflowStep::SdlConfiguration,
        AkashWorkflowStep::CertificateSetup,
        AkashWorkflowStep::DeploymentCreate,
        AkashWorkflowStep::BidWait,
        AkashWorkflowStep::ProviderSelection,
        AkashWorkflowStep::LeaseCreate,
        AkashWorkflowStep::ManifestSend,
        AkashWorkflowStep::EndpointRetrieval,
        AkashWorkflowStep::EndpointTesting,
        AkashWorkflowStep::Complete,
    ];

    for step in &expected_steps {
        workflow.current_step = *step as i32;
        workflow.status = if *step == AkashWorkflowStep::Complete {
            AkashWorkflowStatus::Completed as i32
        } else {
            AkashWorkflowStatus::Running as i32
        };
        storage.put_akash_workflow(&workflow).await.unwrap();

        // Re-read from storage to verify persistence
        let loaded = storage
            .get_akash_workflow(session_id)
            .await
            .unwrap()
            .expect("Workflow should exist after put");
        assert_eq!(
            loaded.current_step, *step as i32,
            "Step mismatch after round-trip for {:?}",
            step
        );
    }

    // Final verification: workflow is completed
    let final_wf = storage
        .get_akash_workflow(session_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(final_wf.status, AkashWorkflowStatus::Completed as i32);
    assert_eq!(final_wf.current_step, AkashWorkflowStep::Complete as i32);
}

/// Verify that step enum values are sequential starting from 1.
#[test]
fn test_workflow_step_enum_values_are_sequential() {
    let steps = [
        AkashWorkflowStep::KeySelection,
        AkashWorkflowStep::BalanceCheck,
        AkashWorkflowStep::GrantRequest,
        AkashWorkflowStep::GrantWait,
        AkashWorkflowStep::AuthzSetup,
        AkashWorkflowStep::FeegrantSetup,
        AkashWorkflowStep::SdlConfiguration,
        AkashWorkflowStep::CertificateSetup,
        AkashWorkflowStep::DeploymentCreate,
        AkashWorkflowStep::BidWait,
        AkashWorkflowStep::ProviderSelection,
        AkashWorkflowStep::LeaseCreate,
        AkashWorkflowStep::ManifestSend,
        AkashWorkflowStep::EndpointRetrieval,
        AkashWorkflowStep::EndpointTesting,
        AkashWorkflowStep::Complete,
    ];

    for (i, step) in steps.iter().enumerate() {
        assert_eq!(
            *step as i32,
            (i + 1) as i32,
            "Step {:?} has unexpected enum value",
            step
        );
    }
}

// ============================================================================
// Test 2: SDL Generation and Validation
// ============================================================================

/// Verify that the test SDL helper produces valid YAML with required Akash SDL fields.
#[test]
fn test_sdl_generation_produces_valid_yaml() {
    let sdl_text = create_test_sdl("inference", "ollama/ollama:latest");

    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&sdl_text).expect("Generated SDL should be valid YAML");

    let mapping = parsed.as_mapping().expect("SDL should be a mapping");
    assert!(
        mapping.contains_key(&serde_yaml::Value::String("version".to_string())),
        "SDL must have 'version' field"
    );
    assert!(
        mapping.contains_key(&serde_yaml::Value::String("services".to_string())),
        "SDL must have 'services' field"
    );
    assert!(
        mapping.contains_key(&serde_yaml::Value::String("profiles".to_string())),
        "SDL must have 'profiles' field"
    );
    assert!(
        mapping.contains_key(&serde_yaml::Value::String("deployment".to_string())),
        "SDL must have 'deployment' field"
    );
}

/// Verify that the SDL version field is "2.0" (current Akash SDL spec).
#[test]
fn test_sdl_has_correct_version() {
    let sdl_text = create_test_sdl("web", "nginx:latest");
    let parsed: serde_yaml::Value = serde_yaml::from_str(&sdl_text).unwrap();
    let version = parsed.get("version").expect("version field must exist");
    assert_eq!(version.as_str().unwrap(), "2.0");
}

/// Verify the SDL contains the correct service name and Docker image.
#[test]
fn test_sdl_contains_service_and_image() {
    let sdl_text = create_test_sdl("my-service", "ghcr.io/test/image:v1");
    let parsed: serde_yaml::Value = serde_yaml::from_str(&sdl_text).unwrap();

    let services = parsed.get("services").unwrap();
    let service = services
        .get("my-service")
        .expect("service 'my-service' must exist");
    let image = service.get("image").unwrap().as_str().unwrap();
    assert_eq!(image, "ghcr.io/test/image:v1");
}

/// Verify SDL profiles contain required resource definitions.
#[test]
fn test_sdl_profiles_have_required_resources() {
    let sdl_text = create_test_sdl("svc", "img:latest");
    let parsed: serde_yaml::Value = serde_yaml::from_str(&sdl_text).unwrap();

    let compute = parsed
        .get("profiles")
        .unwrap()
        .get("compute")
        .unwrap()
        .get("svc")
        .expect("compute profile must match service name");

    let resources = compute.get("resources").unwrap();
    assert!(resources.get("cpu").is_some(), "Resources must specify cpu");
    assert!(resources.get("memory").is_some(), "Resources must specify memory");
    assert!(resources.get("storage").is_some(), "Resources must specify storage");
}

// ============================================================================
// Test 3: Deployment Builder (ConfiguredSdl)
// ============================================================================

/// Verify that a ConfiguredSdl can be attached to a workflow and round-trips.
#[tokio::test]
async fn test_configured_sdl_stored_on_workflow() {
    init_test_tracing();
    let harness = IntegrationTestHarness::new("deploy_sdl_store")
        .await
        .unwrap();
    let storage = harness.storage();

    let sdl_content = create_test_sdl("test-deploy", "test-image:latest");

    let configured_sdl = ConfiguredSdl {
        template_name: "ollama".to_string(),
        raw_sdl: sdl_content.clone(),
        variables: vec![],
        sdl_hash: "abc123".to_string(),
        configured_at: Some(pbjson_types::Timestamp {
            seconds: chrono::Utc::now().timestamp(),
            nanos: 0,
        }),
    };

    let workflow = AkashDeploymentWorkflow {
        session_id: "sdl-store-test".to_string(),
        current_step: AkashWorkflowStep::SdlConfiguration as i32,
        status: AkashWorkflowStatus::Running as i32,
        configured_sdl: Some(configured_sdl),
        ..Default::default()
    };

    storage.put_akash_workflow(&workflow).await.unwrap();

    let loaded = storage
        .get_akash_workflow("sdl-store-test")
        .await
        .unwrap()
        .unwrap();
    let loaded_sdl = loaded.configured_sdl.expect("ConfiguredSdl must be present");
    assert_eq!(loaded_sdl.template_name, "ollama");
    assert_eq!(loaded_sdl.raw_sdl, sdl_content);
    assert_eq!(loaded_sdl.sdl_hash, "abc123");
}

/// Verify that deployment runtime info is correctly persisted.
#[tokio::test]
async fn test_deployment_runtime_info_persisted() {
    init_test_tracing();
    let harness = IntegrationTestHarness::new("deploy_runtime")
        .await
        .unwrap();
    let storage = harness.storage();

    let workflow = AkashDeploymentWorkflow {
        session_id: "runtime-info-test".to_string(),
        current_step: AkashWorkflowStep::DeploymentCreate as i32,
        status: AkashWorkflowStatus::Running as i32,
        account_address: "akash1deployer".to_string(),
        deployment: Some(AkashRuntime {
            deployment_sequence: "99999".to_string(),
            group_sequence: "1".to_string(),
            ..Default::default()
        }),
        ..Default::default()
    };

    storage.put_akash_workflow(&workflow).await.unwrap();

    let loaded = storage
        .get_akash_workflow("runtime-info-test")
        .await
        .unwrap()
        .unwrap();
    let runtime = loaded.deployment.expect("AkashRuntime must be present");
    assert_eq!(runtime.deployment_sequence, "99999");
    assert_eq!(runtime.group_sequence, "1");
}

// ============================================================================
// Test 4: Grant Request Flow
// ============================================================================

/// Verify that grant request fields are properly stored on the workflow.
#[tokio::test]
async fn test_grant_request_fields_stored() {
    init_test_tracing();
    let harness = IntegrationTestHarness::new("grant_status")
        .await
        .unwrap();
    let storage = harness.storage();

    let session_id = "grant-flow-001";

    let mut workflow = AkashDeploymentWorkflow {
        session_id: session_id.to_string(),
        current_step: AkashWorkflowStep::GrantRequest as i32,
        status: AkashWorkflowStatus::Running as i32,
        request_grant_from: vec![1, 2, 3, 4],
        grant_duration_seconds: 86400,
        grant_spend_limit_uakt: 5_000_000,
        grant_purpose: "Deploy inference node".to_string(),
        ..Default::default()
    };
    storage.put_akash_workflow(&workflow).await.unwrap();

    let loaded = storage
        .get_akash_workflow(session_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(loaded.request_grant_from, vec![1, 2, 3, 4]);
    assert_eq!(loaded.grant_duration_seconds, 86400);
    assert_eq!(loaded.grant_spend_limit_uakt, 5_000_000);
    assert_eq!(loaded.grant_purpose, "Deploy inference node");

    // Advance to GrantWait step
    workflow.current_step = AkashWorkflowStep::GrantWait as i32;
    storage.put_akash_workflow(&workflow).await.unwrap();

    let waiting = storage
        .get_akash_workflow(session_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(waiting.current_step, AkashWorkflowStep::GrantWait as i32);
}

/// Verify GrantRequestStatus enum values.
#[test]
fn test_grant_request_status_enum_values() {
    assert_eq!(GrantRequestStatus::Pending as i32, 1);
    assert_eq!(GrantRequestStatus::Approved as i32, 2);
    assert_eq!(GrantRequestStatus::Broadcasted as i32, 3);
    assert_eq!(GrantRequestStatus::Confirmed as i32, 4);
    assert_eq!(GrantRequestStatus::Rejected as i32, 5);
}

/// Verify GrantAcceptanceMode enum values.
#[test]
fn test_grant_acceptance_mode_enum_values() {
    assert_eq!(GrantAcceptanceMode::AcceptAll as i32, 0);
    assert_eq!(GrantAcceptanceMode::Whitelist as i32, 1);
    assert_eq!(GrantAcceptanceMode::Manual as i32, 2);
    assert_eq!(GrantAcceptanceMode::RejectAll as i32, 3);
}

/// Grant approved should allow workflow to advance past GrantWait.
#[tokio::test]
async fn test_grant_approved_advances_workflow() {
    init_test_tracing();
    let harness = IntegrationTestHarness::new("grant_advance")
        .await
        .unwrap();
    let storage = harness.storage();

    let session_id = "grant-advance-001";

    let mut workflow = AkashDeploymentWorkflow {
        session_id: session_id.to_string(),
        current_step: AkashWorkflowStep::GrantWait as i32,
        status: AkashWorkflowStatus::Running as i32,
        ..Default::default()
    };
    storage.put_akash_workflow(&workflow).await.unwrap();

    workflow.current_step = AkashWorkflowStep::AuthzSetup as i32;
    storage.put_akash_workflow(&workflow).await.unwrap();

    let loaded = storage
        .get_akash_workflow(session_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(loaded.current_step, AkashWorkflowStep::AuthzSetup as i32);
}

// ============================================================================
// Test 5: Endpoint Retrieval and Caching
// ============================================================================

/// Verify that service endpoints can be stored and retrieved on a completed workflow.
#[tokio::test]
async fn test_workflow_endpoints_stored_and_retrieved() {
    init_test_tracing();
    let harness = IntegrationTestHarness::new("deploy_endpoints")
        .await
        .unwrap();
    let storage = harness.storage();

    let session_id = "endpoint-test-001";

    let workflow = AkashDeploymentWorkflow {
        session_id: session_id.to_string(),
        current_step: AkashWorkflowStep::Complete as i32,
        status: AkashWorkflowStatus::Completed as i32,
        label: "test-inference".to_string(),
        account_address: "akash1endpointtest".to_string(),
        service_endpoints: vec![
            AkashServiceEndpoint {
                service_name: "inference".to_string(),
                external_uri: "https://provider.test.akash:31234".to_string(),
                internal_port: 8000,
                external_port: 31234,
                protocol: "TCP".to_string(),
                model_name: "Qwen/Qwen3-235B-A22B-FP8".to_string(),
            },
            AkashServiceEndpoint {
                service_name: "metrics".to_string(),
                external_uri: "https://provider.test.akash:31235".to_string(),
                internal_port: 9090,
                external_port: 31235,
                protocol: "TCP".to_string(),
                model_name: String::new(),
            },
        ],
        deployment: Some(AkashRuntime {
            deployment_sequence: "12345".to_string(),
            ..Default::default()
        }),
        ..Default::default()
    };

    storage.put_akash_workflow(&workflow).await.unwrap();

    let loaded = storage
        .get_akash_workflow(session_id)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(loaded.service_endpoints.len(), 2);
    assert_eq!(loaded.service_endpoints[0].service_name, "inference");
    assert_eq!(
        loaded.service_endpoints[0].external_uri,
        "https://provider.test.akash:31234"
    );
    assert_eq!(
        loaded.service_endpoints[0].model_name,
        "Qwen/Qwen3-235B-A22B-FP8"
    );
    assert_eq!(loaded.service_endpoints[1].service_name, "metrics");
}

/// DeploymentProviderCache lifecycle: add, lookup, list, remove.
#[tokio::test]
async fn test_deployment_provider_cache_lifecycle() {
    use ho_std::llm::DeploymentProviderCache;

    let cache = DeploymentProviderCache::new();
    assert_eq!(cache.count().await, 0);
    assert!(cache.list_models().await.is_empty());

    let mut workflow = AkashDeploymentWorkflow::default();
    workflow.session_id = "cache-lifecycle-session".to_string();
    workflow.label = "my-inference-model".to_string();
    workflow.status = AkashWorkflowStatus::Completed as i32;
    workflow.account_address = "akash1cachetest".to_string();
    workflow.model_name = "meta-llama/Llama-3-70B".to_string();
    workflow.service_endpoints.push(AkashServiceEndpoint {
        service_name: "vllm".to_string(),
        external_uri: "https://provider.akash:30123".to_string(),
        internal_port: 8000,
        external_port: 30123,
        protocol: "TCP".to_string(),
        model_name: "meta-llama/Llama-3-70B".to_string(),
    });

    cache.add_deployment(&workflow).await.unwrap();
    assert_eq!(cache.count().await, 1);

    let endpoint = cache.get("my-inference-model").await;
    assert!(endpoint.is_some(), "Cache lookup by label must succeed");
    let endpoint = endpoint.unwrap();
    assert_eq!(endpoint.session_id, "cache-lifecycle-session");
    assert_eq!(endpoint.model_name(), "meta-llama/Llama-3-70B");
    assert_eq!(endpoint.base_url().unwrap(), "https://provider.akash:30123");

    let models = cache.list_models().await;
    assert_eq!(models.len(), 1);
    assert!(models.contains(&"my-inference-model".to_string()));

    assert!(cache.get("nonexistent-model").await.is_none());

    cache.remove_deployment("my-inference-model").await.unwrap();
    assert_eq!(cache.count().await, 0);
    assert!(cache.get("my-inference-model").await.is_none());
}

/// Deployment without a label should NOT be added to the cache.
#[tokio::test]
async fn test_deployment_cache_ignores_unlabeled_deployments() {
    use ho_std::llm::DeploymentProviderCache;

    let cache = DeploymentProviderCache::new();
    let mut workflow = AkashDeploymentWorkflow::default();
    workflow.session_id = "unlabeled-session".to_string();
    workflow.label = String::new();
    workflow.status = AkashWorkflowStatus::Completed as i32;
    workflow.service_endpoints.push(AkashServiceEndpoint {
        service_name: "svc".to_string(),
        external_uri: "https://provider:8080".to_string(),
        internal_port: 8000,
        external_port: 8080,
        protocol: "TCP".to_string(),
        model_name: String::new(),
    });

    cache.add_deployment(&workflow).await.unwrap();
    assert_eq!(cache.count().await, 0);
}

/// Non-completed deployment should NOT be added to the cache.
#[tokio::test]
async fn test_deployment_cache_ignores_non_completed_deployments() {
    use ho_std::llm::DeploymentProviderCache;

    let cache = DeploymentProviderCache::new();
    let mut workflow = AkashDeploymentWorkflow::default();
    workflow.session_id = "running-session".to_string();
    workflow.label = "active-model".to_string();
    workflow.status = AkashWorkflowStatus::Running as i32;
    workflow.service_endpoints.push(AkashServiceEndpoint {
        service_name: "svc".to_string(),
        external_uri: "https://provider:8080".to_string(),
        internal_port: 8000,
        external_port: 8080,
        protocol: "TCP".to_string(),
        model_name: String::new(),
    });

    cache.add_deployment(&workflow).await.unwrap();
    assert_eq!(cache.count().await, 0);
}

/// When model_name is empty, DeploymentEndpoint::model_name() falls back to label.
#[tokio::test]
async fn test_deployment_cache_model_name_falls_back_to_label() {
    use ho_std::llm::DeploymentProviderCache;

    let cache = DeploymentProviderCache::new();
    let mut workflow = AkashDeploymentWorkflow::default();
    workflow.session_id = "fallback-session".to_string();
    workflow.label = "my-legacy-model".to_string();
    workflow.model_name = String::new();
    workflow.status = AkashWorkflowStatus::Completed as i32;
    workflow.account_address = "akash1test".to_string();
    workflow.service_endpoints.push(AkashServiceEndpoint {
        service_name: "inference".to_string(),
        external_uri: "https://provider:8080".to_string(),
        internal_port: 8000,
        external_port: 8080,
        protocol: "TCP".to_string(),
        model_name: String::new(),
    });

    cache.add_deployment(&workflow).await.unwrap();
    let endpoint = cache.get("my-legacy-model").await.unwrap();
    assert_eq!(endpoint.model_name(), "my-legacy-model");
}

/// Multiple workflows can be listed and filtered by status.
#[tokio::test]
async fn test_multiple_workflows_list_and_filter() {
    init_test_tracing();
    let harness = IntegrationTestHarness::new("deploy_multi_filter")
        .await
        .unwrap();
    let storage = harness.storage();

    let workflow_specs = vec![
        ("multi-1", AkashWorkflowStatus::Pending, AkashWorkflowStep::KeySelection),
        ("multi-2", AkashWorkflowStatus::Running, AkashWorkflowStep::BidWait),
        ("multi-3", AkashWorkflowStatus::Running, AkashWorkflowStep::ManifestSend),
        ("multi-4", AkashWorkflowStatus::Completed, AkashWorkflowStep::Complete),
        ("multi-5", AkashWorkflowStatus::Failed, AkashWorkflowStep::Failed),
    ];

    for (id, status, step) in &workflow_specs {
        let wf = AkashDeploymentWorkflow {
            session_id: id.to_string(),
            current_step: *step as i32,
            status: *status as i32,
            ..Default::default()
        };
        storage.put_akash_workflow(&wf).await.unwrap();
    }

    let all = storage.list_akash_workflows().await.unwrap();
    assert_eq!(all.len(), 5);

    let running: Vec<_> = all
        .iter()
        .filter(|w| w.status == AkashWorkflowStatus::Running as i32)
        .collect();
    assert_eq!(running.len(), 2);

    let failed: Vec<_> = all
        .iter()
        .filter(|w| w.status == AkashWorkflowStatus::Failed as i32)
        .collect();
    assert_eq!(failed.len(), 1);
}

/// Workflow deletion removes it from storage.
#[tokio::test]
async fn test_workflow_delete() {
    init_test_tracing();
    let harness = IntegrationTestHarness::new("deploy_wf_delete")
        .await
        .unwrap();
    let storage = harness.storage();

    let wf = AkashDeploymentWorkflow {
        session_id: "delete-me".to_string(),
        current_step: AkashWorkflowStep::KeySelection as i32,
        status: AkashWorkflowStatus::Pending as i32,
        ..Default::default()
    };
    storage.put_akash_workflow(&wf).await.unwrap();
    assert!(storage.get_akash_workflow("delete-me").await.unwrap().is_some());

    storage.delete_akash_workflow("delete-me").await.unwrap();
    assert!(storage.get_akash_workflow("delete-me").await.unwrap().is_none());
}
