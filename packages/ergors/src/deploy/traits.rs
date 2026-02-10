//! Deployment provider abstraction.
//!
//! Defines a trait for deployment providers (Akash, SSH, Docker, EC2, etc.)
//! enabling the bootstrap orchestrator to be generic over the deployment method.

use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Metadata returned by a deployment provider after deploying.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeploymentMetadata {
    /// Provider type identifier (e.g., "akash", "ssh", "docker")
    pub provider_type: String,
    /// Provider-specific deployment ID
    pub deployment_id: String,
    /// Accessible endpoints for the deployment
    pub endpoints: Vec<String>,
    /// Provider-specific metadata (flexible storage)
    pub metadata: serde_json::Value,
}

/// Trait for deployment providers.
///
/// Implementations handle the full lifecycle of a node deployment:
/// deploying, readiness checking, endpoint discovery, and cleanup.
#[async_trait]
pub trait DeploymentProvider: Send + Sync {
    /// Deploy a new node and return deployment metadata.
    async fn deploy(&self) -> Result<DeploymentMetadata>;

    /// Check if deployment is ready to receive traffic.
    async fn is_ready(&self, metadata: &DeploymentMetadata) -> Result<bool>;

    /// Get endpoint URLs for the deployment.
    async fn get_endpoints(&self, metadata: &DeploymentMetadata) -> Result<Vec<String>>;

    /// Clean up / tear down a deployment.
    async fn cleanup(&self, metadata: &DeploymentMetadata) -> Result<()>;

    /// Get the provider type name.
    fn provider_type(&self) -> &str;
}
