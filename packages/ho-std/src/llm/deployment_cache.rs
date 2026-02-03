//! In-memory cache of active Akash deployments for fast inference routing.
//!
//! This cache maintains a registry of deployments that can serve inference requests,
//! enabling O(1) lookups by model name (deployment label).

use crate::error::HoResult;
use crate::types::ergors::orch::v1::{AkashDeploymentWorkflow, AkashWorkflowStatus};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Cached deployment endpoint information for inference routing.
#[derive(Debug, Clone)]
pub struct DeploymentEndpoint {
    /// Deployment session ID
    pub session_id: String,
    /// User-defined label (used as routing key)
    pub label: String,
    /// Actual model name for the inference server (e.g., "Qwen/Qwen3-235B-A22B-FP8")
    pub model_name: String,
    /// Service endpoints with external URIs
    pub endpoints: Vec<ServiceEndpoint>,
    /// Account address that owns this deployment
    pub owner: String,
    /// DSEQ of the deployment
    pub dseq: u64,
}

#[derive(Debug, Clone)]
pub struct ServiceEndpoint {
    pub service_name: String,
    pub external_uri: String,
    pub port: u16,
}

impl DeploymentEndpoint {
    /// Get the primary endpoint (first service endpoint).
    /// For single-service deployments, this is the inference endpoint.
    pub fn primary_endpoint(&self) -> Option<&ServiceEndpoint> {
        self.endpoints.first()
    }

    /// Get the base URL for inference requests.
    /// Appends OpenAI-compatible paths to this base.
    pub fn base_url(&self) -> Option<String> {
        self.primary_endpoint().map(|ep| ep.external_uri.clone())
    }

    /// Get the model name to send to the inference server.
    /// Returns model_name if set, otherwise falls back to label.
    pub fn model_name(&self) -> &str {
        if self.model_name.is_empty() {
            &self.label
        } else {
            &self.model_name
        }
    }
}

/// In-memory cache of active deployments for fast routing.
///
/// This cache is periodically refreshed from storage to stay in sync with
/// deployment lifecycle events (creation, completion, failure).
pub struct DeploymentProviderCache {
    /// Model name (label) -> Deployment endpoint mapping.
    /// Public for external refreshers that need direct cache access.
    pub cache: Arc<RwLock<HashMap<String, DeploymentEndpoint>>>,
}

impl DeploymentProviderCache {
    pub fn new() -> Self {
        Self {
            cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Basic refresh - deprecated in favor of DeploymentCacheRefresher in cw-ho.
    ///
    /// For proper cache refresh with chain verification (lease status, escrow balance),
    /// use `crate::deploy::cache_refresher::DeploymentCacheRefresher` in cw-ho.
    ///
    /// This method only provides basic cache operations - for production use,
    /// the cw-ho refresher should be used which has access to CosmosClient.
    #[deprecated(note = "Use DeploymentCacheRefresher in cw-ho for chain-verified refresh")]
    pub async fn refresh<S>(&self, _storage: &S) -> HoResult<usize>
    where
        S: cnidarium::StateRead,
    {
        // This method is now a no-op.
        // Actual refresh is done by DeploymentCacheRefresher in cw-ho which has
        // access to CosmosClient for lease verification and escrow balance checks.
        Ok(self.cache.read().await.len())
    }

    /// Manually add a deployment to the cache.
    /// Used when a deployment completes to immediately make it available.
    pub async fn add_deployment(&self, workflow: &AkashDeploymentWorkflow) -> HoResult<()> {
        // Only cache completed deployments with labels and endpoints
        if workflow.label.is_empty() {
            return Ok(());
        }

        if workflow.status != AkashWorkflowStatus::Completed as i32 {
            return Ok(());
        }

        if workflow.service_endpoints.is_empty() {
            tracing::warn!(
                "Deployment {} has label '{}' but no service endpoints",
                workflow.session_id,
                workflow.label
            );
            return Ok(());
        }

        let endpoints: Vec<ServiceEndpoint> = workflow
            .service_endpoints
            .iter()
            .map(|ep| ServiceEndpoint {
                service_name: ep.service_name.clone(),
                external_uri: ep.external_uri.clone(),
                port: ep.external_port as u16,
            })
            .collect();

        let dseq = workflow
            .deployment
            .as_ref()
            .and_then(|d| d.deployment_sequence.parse::<u64>().ok())
            .unwrap_or(0);

        // Resolve model_name: prefer workflow-level, then first endpoint's model_name
        let model_name = if !workflow.model_name.is_empty() {
            workflow.model_name.clone()
        } else {
            workflow
                .service_endpoints
                .first()
                .map(|ep| ep.model_name.clone())
                .unwrap_or_default()
        };

        let endpoint = DeploymentEndpoint {
            session_id: workflow.session_id.clone(),
            label: workflow.label.clone(),
            model_name,
            endpoints,
            owner: workflow.account_address.clone(),
            dseq,
        };

        let mut cache = self.cache.write().await;
        cache.insert(workflow.label.clone(), endpoint);

        tracing::info!(
            "Added deployment '{}' to inference cache with {} endpoints",
            workflow.label,
            workflow.service_endpoints.len()
        );

        Ok(())
    }

    /// Remove a deployment from the cache.
    /// Used when a deployment is closed or fails.
    pub async fn remove_deployment(&self, label: &str) -> HoResult<()> {
        let mut cache = self.cache.write().await;
        if cache.remove(label).is_some() {
            tracing::info!("Removed deployment '{}' from inference cache", label);
        }
        Ok(())
    }

    /// Look up a deployment by model name (label).
    /// Returns None if no active deployment matches.
    pub async fn get(&self, model_name: &str) -> Option<DeploymentEndpoint> {
        let cache = self.cache.read().await;
        cache.get(model_name).cloned()
    }

    /// Get all active deployment model names.
    /// Used for listing available models.
    pub async fn list_models(&self) -> Vec<String> {
        let cache = self.cache.read().await;
        cache.keys().cloned().collect()
    }

    /// Get count of active deployments in cache.
    pub async fn count(&self) -> usize {
        let cache = self.cache.read().await;
        cache.len()
    }

    /// Clear the entire cache.
    /// Used for testing or manual reset.
    pub async fn clear(&self) {
        let mut cache = self.cache.write().await;
        cache.clear();
        tracing::debug!("Cleared deployment cache");
    }
}

impl Default for DeploymentProviderCache {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_cache_add_remove() {
        let cache = DeploymentProviderCache::new();

        let mut workflow = AkashDeploymentWorkflow::default();
        workflow.session_id = "test-session".to_string();
        workflow.label = "embed-model".to_string();
        workflow.status = AkashWorkflowStatus::Completed as i32;
        workflow.account_address = "akash1test".to_string();

        // Add service endpoint
        workflow.service_endpoints.push(
            crate::types::ergors::orch::v1::AkashServiceEndpoint {
                service_name: "sglang".to_string(),
                external_uri: "https://provider.akash.pub:30123".to_string(),
                internal_port: 8000,
                external_port: 30123,
                protocol: "TCP".to_string(),
                model_name: String::new(),
            },
        );

        // Add to cache
        cache.add_deployment(&workflow).await.unwrap();
        assert_eq!(cache.count().await, 1);

        // Lookup
        let endpoint = cache.get("embed-model").await;
        assert!(endpoint.is_some());
        assert_eq!(
            endpoint.unwrap().base_url().unwrap(),
            "https://provider.akash.pub:30123"
        );

        // Remove
        cache.remove_deployment("embed-model").await.unwrap();
        assert_eq!(cache.count().await, 0);
    }

    #[tokio::test]
    async fn test_deployment_inference_integration() {
        let cache = DeploymentProviderCache::new();

        // Create a completed deployment workflow
        let mut workflow = AkashDeploymentWorkflow::default();
        workflow.session_id = "integration-test-session".to_string();
        workflow.label = "qwen-inference".to_string();
        workflow.status = AkashWorkflowStatus::Completed as i32;
        workflow.account_address = "akash1integration".to_string();

        // Add deployment runtime with dseq
        workflow.deployment = Some(crate::types::ergors::orch::v1::AkashRuntime {
            deployment_sequence: "12345".to_string(),
            ..Default::default()
        });

        // Add service endpoint
        workflow.service_endpoints.push(
            crate::types::ergors::orch::v1::AkashServiceEndpoint {
                service_name: "qwen-server".to_string(),
                external_uri: "https://provider.test.akash:8443".to_string(),
                internal_port: 8000,
                external_port: 8443,
                protocol: "HTTPS".to_string(),
                model_name: String::new(),
            },
        );

        // 1. Add deployment to cache (simulates completion handler)
        cache.add_deployment(&workflow).await.unwrap();

        // 2. Verify cache state
        assert_eq!(cache.count().await, 1);
        let models = cache.list_models().await;
        assert_eq!(models.len(), 1);
        assert_eq!(models[0], "qwen-inference");

        // 3. Lookup deployment by model name (simulates router lookup)
        let endpoint = cache.get("qwen-inference").await;
        assert!(endpoint.is_some());

        let endpoint = endpoint.unwrap();
        assert_eq!(endpoint.session_id, "integration-test-session");
        assert_eq!(endpoint.label, "qwen-inference");
        assert_eq!(endpoint.owner, "akash1integration");
        assert_eq!(endpoint.dseq, 12345);
        assert_eq!(endpoint.endpoints.len(), 1);
        assert_eq!(
            endpoint.base_url().unwrap(),
            "https://provider.test.akash:8443"
        );

        // 4. Remove deployment (simulates close handler)
        cache.remove_deployment("qwen-inference").await.unwrap();

        // 5. Verify cleanup
        assert_eq!(cache.count().await, 0);
        assert!(cache.get("qwen-inference").await.is_none());
        assert_eq!(cache.list_models().await.len(), 0);
    }

    #[tokio::test]
    async fn test_label_collision_handling() {
        let cache = DeploymentProviderCache::new();

        // Create first deployment
        let mut workflow1 = AkashDeploymentWorkflow::default();
        workflow1.session_id = "session-1".to_string();
        workflow1.label = "my-model".to_string();
        workflow1.status = AkashWorkflowStatus::Completed as i32;
        workflow1.service_endpoints.push(
            crate::types::ergors::orch::v1::AkashServiceEndpoint {
                service_name: "service-1".to_string(),
                external_uri: "https://provider1.akash:8443".to_string(),
                internal_port: 8000,
                external_port: 8443,
                protocol: "HTTPS".to_string(),
                model_name: String::new(),
            },
        );

        // Add first deployment
        cache.add_deployment(&workflow1).await.unwrap();
        assert_eq!(cache.count().await, 1);

        // Create second deployment with same label (should replace)
        let mut workflow2 = AkashDeploymentWorkflow::default();
        workflow2.session_id = "session-2".to_string();
        workflow2.label = "my-model".to_string();
        workflow2.status = AkashWorkflowStatus::Completed as i32;
        workflow2.service_endpoints.push(
            crate::types::ergors::orch::v1::AkashServiceEndpoint {
                service_name: "service-2".to_string(),
                external_uri: "https://provider2.akash:8443".to_string(),
                internal_port: 8000,
                external_port: 8443,
                protocol: "HTTPS".to_string(),
                model_name: String::new(),
            },
        );

        // Add second deployment (replaces first in cache)
        cache.add_deployment(&workflow2).await.unwrap();
        assert_eq!(cache.count().await, 1); // Still 1 (replaced)

        // Verify it's the second deployment
        let endpoint = cache.get("my-model").await.unwrap();
        assert_eq!(endpoint.session_id, "session-2");

        // Note: Actual collision prevention happens at gRPC handler level
        // via check_label_collision() before creation
    }

    #[tokio::test]
    async fn test_model_name_propagation() {
        let cache = DeploymentProviderCache::new();

        // Deployment with model_name set on workflow
        let mut workflow = AkashDeploymentWorkflow::default();
        workflow.session_id = "model-name-test".to_string();
        workflow.label = "qwen-node-1".to_string();
        workflow.model_name = "Qwen/Qwen3-235B-A22B-FP8".to_string();
        workflow.status = AkashWorkflowStatus::Completed as i32;
        workflow.account_address = "akash1test".to_string();
        workflow.service_endpoints.push(
            crate::types::ergors::orch::v1::AkashServiceEndpoint {
                service_name: "sglang".to_string(),
                external_uri: "https://provider.akash.pub:30123".to_string(),
                internal_port: 8000,
                external_port: 30123,
                protocol: "TCP".to_string(),
                model_name: "Qwen/Qwen3-235B-A22B-FP8".to_string(),
            },
        );

        cache.add_deployment(&workflow).await.unwrap();

        let endpoint = cache.get("qwen-node-1").await.unwrap();
        // model_name() should return the actual model name, not the label
        assert_eq!(endpoint.model_name(), "Qwen/Qwen3-235B-A22B-FP8");
        assert_eq!(endpoint.label, "qwen-node-1");
    }

    #[tokio::test]
    async fn test_model_name_fallback_to_label() {
        let cache = DeploymentProviderCache::new();

        // Deployment without model_name (backwards compat)
        let mut workflow = AkashDeploymentWorkflow::default();
        workflow.session_id = "fallback-test".to_string();
        workflow.label = "my-legacy-deploy".to_string();
        workflow.status = AkashWorkflowStatus::Completed as i32;
        workflow.account_address = "akash1test".to_string();
        workflow.service_endpoints.push(
            crate::types::ergors::orch::v1::AkashServiceEndpoint {
                service_name: "inference".to_string(),
                external_uri: "https://provider.akash.pub:30456".to_string(),
                internal_port: 8000,
                external_port: 30456,
                protocol: "TCP".to_string(),
                model_name: String::new(), // empty = not set
            },
        );

        cache.add_deployment(&workflow).await.unwrap();

        let endpoint = cache.get("my-legacy-deploy").await.unwrap();
        // model_name() should fall back to label when model_name is empty
        assert_eq!(endpoint.model_name(), "my-legacy-deploy");
    }
}
