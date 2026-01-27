//! Akash Deployment Integration Tests
//!
//! Comprehensive integration tests for the ERGORS Akash deployment workflow
//! using Akash's Kind-based development environment.
//!
//! ## Running Tests
//!
//! ```bash
//! # Run all integration tests
//! cargo test -p ergors --features testing --test akash_integration -- --nocapture
//!
//! # Run specific test
//! cargo test -p ergors --features testing test_happy_path_deployment -- --nocapture
//! ```
//!
//! ## Prerequisites
//!
//! - Docker running
//! - Kind installed (`brew install kind` on macOS)
//! - kubectl installed
//! - Sufficient disk space for container images

#[cfg(feature = "testing")]
mod tests {
    use ergors::deploy::testing::prelude::*;
    use std::collections::HashMap;
    use std::time::Duration;

    // ==================== Happy Path Tests ====================

    /// Test complete deployment workflow from key selection to completion
    #[tokio::test]
    #[ignore = "Requires Docker and Kind cluster"]
    async fn test_happy_path_deployment() {
        // Initialize test environment
        let env = AkashDevEnvironment::start().await.expect("Failed to start dev environment");

        // Start mock inference provider
        let mock = MockInferenceProvider::start().await.expect("Failed to start mock provider");

        // Verify environment is running
        assert!(env.is_running().await);
        assert!(mock.is_responsive().await);

        // Get deployer account
        let deployer = env.get_account("deployer").await.expect("No deployer account");
        assert!(deployer.balance_uakt > 0);

        // Create test SDL
        let sdl = create_test_sdl(&mock.base_url().unwrap());

        // Create and execute deployment
        let deployment = env.create_deployment(&deployer.name, &sdl).await
            .expect("Failed to create deployment");

        // Verify deployment
        assert_eq!(deployment.owner, deployer.name);

        // Query deployments
        let deployments = env.query_deployments(&deployer.name).await.unwrap();
        assert!(!deployments.is_empty());

        // Cleanup
        env.stop().await.expect("Failed to stop environment");
    }

    /// Test deployment with authz grant request workflow
    #[tokio::test]
    #[ignore = "Requires Docker and Kind cluster"]
    async fn test_deployment_with_grant_request() {
        let env = AkashDevEnvironment::start().await.expect("Failed to start dev environment");

        // Get grantee account (low balance, needs grants)
        let grantee = env.get_account("grantee").await.expect("No grantee account");
        let granter = env.get_account("granter").await.expect("No granter account");

        // Verify grantee has low balance
        assert!(grantee.balance_uakt < 10_000_000); // Less than 10 AKT

        // Setup network topology for grant request simulation
        let network = NetworkTopology::new();
        network.create_node("grantee_node").await.unwrap();
        network.create_node("granter_node").await.unwrap();

        // Set granter to accept-all mode for testing
        network.set_grant_mode("granter_node", GrantAcceptanceMode::AcceptAll).await.unwrap();

        // Submit grant request
        let request = network.submit_grant_request(
            "grantee_node",
            "granter_node",
            GrantTypeRequest::AuthzAndFeegrant,
            86400, // 24 hours
            5_000_000, // 5 AKT
            "Test deployment",
        ).await.expect("Failed to submit grant request");

        // Verify grant was approved
        assert_eq!(request.status, GrantRequestStatus::Approved);

        // Cleanup
        env.stop().await.expect("Failed to stop environment");
    }

    // ==================== Mock Inference Provider Tests ====================

    /// Test mock inference provider Ollama API compatibility
    #[tokio::test]
    async fn test_mock_inference_ollama_api() {
        let mut provider = MockInferenceProvider::start().await.expect("Failed to start provider");
        let base_url = provider.base_url().unwrap();

        let client = reqwest::Client::new();

        // Test /api/tags endpoint
        let tags_resp = client.get(format!("{}/api/tags", base_url))
            .send()
            .await
            .expect("Failed to get tags");

        assert!(tags_resp.status().is_success());
        let tags: serde_json::Value = tags_resp.json().await.unwrap();
        assert!(tags.get("models").is_some());

        // Test /api/generate endpoint
        let generate_resp = client.post(format!("{}/api/generate", base_url))
            .json(&serde_json::json!({
                "model": "llama2",
                "prompt": "Hello, how are you?",
                "stream": false
            }))
            .send()
            .await
            .expect("Failed to generate");

        assert!(generate_resp.status().is_success());
        let result: serde_json::Value = generate_resp.json().await.unwrap();
        assert!(result.get("response").is_some());
        assert_eq!(result["done"], true);

        // Test /api/chat endpoint
        let chat_resp = client.post(format!("{}/api/chat", base_url))
            .json(&serde_json::json!({
                "model": "llama2",
                "messages": [
                    {"role": "user", "content": "Hello!"}
                ],
                "stream": false
            }))
            .send()
            .await
            .expect("Failed to chat");

        assert!(chat_resp.status().is_success());

        // Verify request count
        assert!(provider.request_count().await >= 2);

        provider.stop().await;
    }

    /// Test mock inference provider OpenAI API compatibility
    #[tokio::test]
    async fn test_mock_inference_openai_api() {
        let mut provider = MockInferenceProvider::start().await.expect("Failed to start provider");
        let base_url = provider.base_url().unwrap();

        let client = reqwest::Client::new();

        // Test /v1/models endpoint
        let models_resp = client.get(format!("{}/v1/models", base_url))
            .send()
            .await
            .expect("Failed to get models");

        assert!(models_resp.status().is_success());

        // Test /v1/completions endpoint
        let completions_resp = client.post(format!("{}/v1/completions", base_url))
            .json(&serde_json::json!({
                "model": "llama2",
                "prompt": "What is 2+2?",
                "max_tokens": 100
            }))
            .send()
            .await
            .expect("Failed to complete");

        assert!(completions_resp.status().is_success());
        let result: serde_json::Value = completions_resp.json().await.unwrap();
        assert!(result.get("choices").is_some());
        assert!(result.get("usage").is_some());

        // Test /v1/chat/completions endpoint
        let chat_resp = client.post(format!("{}/v1/chat/completions", base_url))
            .json(&serde_json::json!({
                "model": "llama2",
                "messages": [
                    {"role": "system", "content": "You are a helpful assistant."},
                    {"role": "user", "content": "Hello!"}
                ]
            }))
            .send()
            .await
            .expect("Failed to chat");

        assert!(chat_resp.status().is_success());

        provider.stop().await;
    }

    /// Test mock inference provider TGI API compatibility
    #[tokio::test]
    async fn test_mock_inference_tgi_api() {
        let mut provider = MockInferenceProvider::start().await.expect("Failed to start provider");
        let base_url = provider.base_url().unwrap();

        let client = reqwest::Client::new();

        // Test /info endpoint
        let info_resp = client.get(format!("{}/info", base_url))
            .send()
            .await
            .expect("Failed to get info");

        assert!(info_resp.status().is_success());

        // Test /generate endpoint
        let generate_resp = client.post(format!("{}/generate", base_url))
            .json(&serde_json::json!({
                "inputs": "What is machine learning?",
                "parameters": {
                    "max_new_tokens": 100,
                    "temperature": 0.7
                }
            }))
            .send()
            .await
            .expect("Failed to generate");

        assert!(generate_resp.status().is_success());
        let result: serde_json::Value = generate_resp.json().await.unwrap();
        assert!(result.get("generated_text").is_some());

        provider.stop().await;
    }

    // ==================== Wallet Manager Tests ====================

    /// Test wallet creation and funding
    #[tokio::test]
    async fn test_wallet_manager_creation() {
        let manager = TestWalletManager::new();

        // Create wallets
        let deployer = manager.create_wallet("test_deployer", 100_000_000).await.unwrap();
        assert_eq!(deployer.name, "test_deployer");
        assert_eq!(deployer.balance_uakt, 100_000_000);
        assert!(deployer.address.starts_with("akash1"));

        // Fund wallet
        manager.fund_wallet("test_deployer", 50_000_000).await.unwrap();
        let updated = manager.get_wallet("test_deployer").await.unwrap();
        assert_eq!(updated.balance_uakt, 150_000_000);

        // Deduct balance
        manager.deduct_balance("test_deployer", 25_000_000).await.unwrap();
        let final_wallet = manager.get_wallet("test_deployer").await.unwrap();
        assert_eq!(final_wallet.balance_uakt, 125_000_000);
    }

    /// Test authz grant management
    #[tokio::test]
    async fn test_wallet_manager_authz() {
        let manager = TestWalletManager::new();

        manager.create_wallet("granter", 100_000_000).await.unwrap();
        manager.create_wallet("grantee", 1_000_000).await.unwrap();

        // Create Akash deployment grants
        let grants = manager.create_akash_deployment_grants("granter", "grantee").await.unwrap();
        assert_eq!(grants.len(), 4); // 4 Akash message types

        // Verify permissions
        assert!(manager.has_authz_permission("grantee", "/akash.deployment.v1beta3.MsgCreateDeployment").await);
        assert!(manager.has_authz_permission("grantee", "/akash.market.v1beta4.MsgCreateLease").await);
        assert!(!manager.has_authz_permission("grantee", "/cosmos.bank.v1beta1.MsgSend").await);

        // Revoke a grant
        manager.revoke_authz_grant("granter", "grantee", "/akash.deployment.v1beta3.MsgCreateDeployment").await.unwrap();
        assert!(!manager.has_authz_permission("grantee", "/akash.deployment.v1beta3.MsgCreateDeployment").await);
    }

    /// Test feegrant management
    #[tokio::test]
    async fn test_wallet_manager_feegrant() {
        let manager = TestWalletManager::new();

        manager.create_wallet("granter", 100_000_000).await.unwrap();
        manager.create_wallet("grantee", 1_000_000).await.unwrap();

        // Create feegrant
        manager.create_feegrant("granter", "grantee", Some(5_000_000), None).await.unwrap();

        assert!(manager.has_feegrant("grantee").await);
        assert_eq!(manager.get_feegrant_limit("grantee").await, Some(5_000_000));

        // Revoke feegrant
        manager.revoke_feegrant("granter", "grantee").await.unwrap();
        assert!(!manager.has_feegrant("grantee").await);
    }

    // ==================== Network Topology Tests ====================

    /// Test network initialization and node creation
    #[tokio::test]
    async fn test_network_topology_init() {
        let network = NetworkTopology::with_config(NetworkTopologyConfig {
            node_count: 3,
            ..Default::default()
        });

        network.init().await.unwrap();

        let nodes = network.list_nodes().await;
        assert_eq!(nodes.len(), 3);

        // All nodes should be connected
        for node in &nodes {
            assert_eq!(node.status, NodeStatus::Online);
            assert_eq!(node.connected_peers.len(), 2); // Connected to other 2 nodes
        }
    }

    /// Test grant request with different acceptance modes
    #[tokio::test]
    async fn test_network_grant_acceptance_modes() {
        let network = NetworkTopology::new();
        let requester = network.create_node("requester").await.unwrap();
        network.create_node("granter_accept").await.unwrap();
        network.create_node("granter_reject").await.unwrap();
        network.create_node("granter_whitelist").await.unwrap();
        network.create_node("granter_manual").await.unwrap();

        // Test AcceptAll
        network.set_grant_mode("granter_accept", GrantAcceptanceMode::AcceptAll).await.unwrap();
        let req1 = network.submit_grant_request(
            "requester", "granter_accept",
            GrantTypeRequest::AuthzOnly, 86400, 0, "Test"
        ).await.unwrap();
        assert_eq!(req1.status, GrantRequestStatus::Approved);

        // Test RejectAll
        network.set_grant_mode("granter_reject", GrantAcceptanceMode::RejectAll).await.unwrap();
        let req2 = network.submit_grant_request(
            "requester", "granter_reject",
            GrantTypeRequest::AuthzOnly, 86400, 0, "Test"
        ).await.unwrap();
        assert_eq!(req2.status, GrantRequestStatus::Rejected);

        // Test Whitelist (not whitelisted)
        network.set_grant_mode("granter_whitelist", GrantAcceptanceMode::Whitelist).await.unwrap();
        let req3 = network.submit_grant_request(
            "requester", "granter_whitelist",
            GrantTypeRequest::AuthzOnly, 86400, 0, "Test"
        ).await.unwrap();
        assert_eq!(req3.status, GrantRequestStatus::Rejected);

        // Test Whitelist (whitelisted)
        network.whitelist_add("granter_whitelist", &requester.pubkey).await.unwrap();
        let req4 = network.submit_grant_request(
            "requester", "granter_whitelist",
            GrantTypeRequest::AuthzOnly, 86400, 0, "Test"
        ).await.unwrap();
        assert_eq!(req4.status, GrantRequestStatus::Approved);

        // Test Manual
        network.set_grant_mode("granter_manual", GrantAcceptanceMode::Manual).await.unwrap();
        let req5 = network.submit_grant_request(
            "requester", "granter_manual",
            GrantTypeRequest::AuthzOnly, 86400, 0, "Test"
        ).await.unwrap();
        assert_eq!(req5.status, GrantRequestStatus::Pending);

        // Manually approve
        let approved = network.approve_request("granter_manual", req5.id).await.unwrap();
        assert_eq!(approved.status, GrantRequestStatus::Approved);
    }

    /// Test node status and communication
    #[tokio::test]
    async fn test_network_node_status() {
        let network = NetworkTopology::new();
        network.create_node("node_a").await.unwrap();
        network.create_node("node_b").await.unwrap();

        // Both online - should communicate
        assert!(network.can_communicate("node_a", "node_b").await);

        // Set node_b offline
        network.set_node_status("node_b", NodeStatus::Offline).await.unwrap();
        assert!(!network.can_communicate("node_a", "node_b").await);

        // Bring back online
        network.set_node_status("node_b", NodeStatus::Online).await.unwrap();
        assert!(network.can_communicate("node_a", "node_b").await);
    }

    /// Test network partitioning
    #[tokio::test]
    async fn test_network_partitioning() {
        let network = NetworkTopology::with_config(NetworkTopologyConfig {
            node_count: 4,
            enable_partitioning: true,
            ..Default::default()
        });

        network.init().await.unwrap();

        // Create partition: nodes 0,1 separated from 2,3
        network.create_partition(vec!["node_0".to_string(), "node_1".to_string()]).await.unwrap();

        // Nodes in same partition can communicate
        assert!(network.can_communicate("node_0", "node_1").await);
        assert!(network.can_communicate("node_2", "node_3").await);

        // Nodes in different partitions cannot communicate
        assert!(!network.can_communicate("node_0", "node_2").await);
        assert!(!network.can_communicate("node_1", "node_3").await);

        // Heal partitions
        network.heal_partitions().await.unwrap();
        assert!(network.can_communicate("node_0", "node_2").await);
    }

    /// Test network statistics
    #[tokio::test]
    async fn test_network_statistics() {
        let network = NetworkTopology::with_config(NetworkTopologyConfig {
            node_count: 3,
            ..Default::default()
        });

        network.init().await.unwrap();

        // Set up some grants
        network.set_grant_mode("node_1", GrantAcceptanceMode::AcceptAll).await.unwrap();
        network.set_grant_mode("node_2", GrantAcceptanceMode::RejectAll).await.unwrap();

        network.submit_grant_request("node_0", "node_1", GrantTypeRequest::AuthzOnly, 86400, 0, "Test").await.unwrap();
        network.submit_grant_request("node_0", "node_2", GrantTypeRequest::AuthzOnly, 86400, 0, "Test").await.unwrap();

        let stats = network.get_stats().await;
        assert_eq!(stats.total_nodes, 3);
        assert_eq!(stats.online_nodes, 3);
        assert!(stats.total_grants_approved >= 1);
        assert!(stats.total_grants_rejected >= 1);
    }

    // ==================== Integration Scenarios ====================

    /// Test complete authz workflow across network
    #[tokio::test]
    async fn test_authz_workflow_integration() {
        // Setup wallet manager
        let wallet_manager = TestWalletManager::new();
        wallet_manager.init_standard_wallets().await.unwrap();

        // Setup network topology
        let network = NetworkTopology::new();
        let granter_node = network.create_node("granter").await.unwrap();
        let grantee_node = network.create_node("grantee").await.unwrap();

        // Configure granter to use whitelist mode
        network.set_grant_mode("granter", GrantAcceptanceMode::Whitelist).await.unwrap();
        network.whitelist_add("granter", &grantee_node.pubkey).await.unwrap();

        // Submit grant request
        let request = network.submit_grant_request(
            "grantee",
            "granter",
            GrantTypeRequest::AuthzAndFeegrant,
            86400,
            5_000_000,
            "Akash deployment for inference provider",
        ).await.unwrap();

        assert_eq!(request.status, GrantRequestStatus::Approved);

        // Create corresponding grants in wallet manager
        wallet_manager.create_wallet("granter_wallet", 100_000_000).await.unwrap();
        wallet_manager.create_wallet("grantee_wallet", 1_000_000).await.unwrap();

        wallet_manager.create_akash_deployment_grants("granter_wallet", "grantee_wallet").await.unwrap();
        wallet_manager.create_feegrant("granter_wallet", "grantee_wallet", Some(5_000_000), None).await.unwrap();

        // Verify grants
        assert!(wallet_manager.has_authz_permission("grantee_wallet", "/akash.deployment.v1beta3.MsgCreateDeployment").await);
        assert!(wallet_manager.has_feegrant("grantee_wallet").await);
    }

    /// Test mock inference with deployment simulation
    #[tokio::test]
    async fn test_mock_inference_deployment_simulation() {
        // Start mock inference provider
        let mut provider = MockInferenceProvider::start().await.unwrap();
        let endpoint = provider.base_url().unwrap();

        // Simulate what a deployed Akash workload would expose
        let client = reqwest::Client::new();

        // Health check
        let health = client.get(format!("{}/health", endpoint))
            .send()
            .await
            .unwrap();
        assert!(health.status().is_success());

        // Simulate agentic request
        let agentic_resp = client.post(format!("{}/api/agentic/execute", endpoint))
            .json(&serde_json::json!({
                "model": "llama2",
                "prompt": "Search for information about Akash Network",
                "tools": [{
                    "name": "web_search",
                    "description": "Search the web",
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "query": {"type": "string"}
                        }
                    }
                }]
            }))
            .send()
            .await
            .unwrap();

        assert!(agentic_resp.status().is_success());

        // Verify tool calls were recorded
        let tool_calls = provider.tool_calls().await;
        assert!(!tool_calls.is_empty());

        provider.stop().await;
    }

    // ==================== Helper Functions ====================

    /// Create a test SDL for mock inference deployment
    fn create_test_sdl(mock_endpoint: &str) -> String {
        format!(r#"---
version: "2.0"
services:
  mock-inference:
    image: curlimages/curl:latest
    expose:
      - port: 80
        as: 80
        to:
          - global: true
    env:
      - INFERENCE_ENDPOINT={}
    command:
      - /bin/sh
      - -c
      - |
        while true; do
          curl -s $INFERENCE_ENDPOINT/health || true
          sleep 30
        done

profiles:
  compute:
    mock-inference:
      resources:
        cpu:
          units: 1
        memory:
          size: 512Mi
        storage:
          - size: 1Gi
  placement:
    dcloud:
      pricing:
        mock-inference:
          denom: uakt
          amount: 1000

deployment:
  mock-inference:
    dcloud:
      profile: mock-inference
      count: 1
"#, mock_endpoint)
    }
}

// Required main function for bin target (tests are run via cargo test)
#[cfg(not(feature = "testing"))]
fn main() {
    eprintln!("Run with: cargo test -p ergors-tests --features testing");
}

#[cfg(feature = "testing")]
fn main() {
    eprintln!("Run tests with: cargo test -p ergors-tests --features testing");
}
