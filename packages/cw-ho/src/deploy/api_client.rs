//! Akash API Client
//!
//! This module provides gRPC clients for interacting with Akash Network services
//! directly, replacing CLI command execution.

use std::time::Duration;

use anyhow::Result;
use ho_std::types::akash::deployment::v1beta5::QueryDeploymentsResponse;
use reqwest::Client as HttpClient;
use serde_json::Value;


/// Configuration for Akash API connections
#[derive(Debug, Clone)]
pub struct AkashApiConfig {
    /// Akash node gRPC endpoint (e.g., "https://grpc-akash.ecostake.com")
    pub grpc_endpoint: String,
    /// HTTP REST endpoint for fallback queries
    pub rest_endpoint: String,
    /// Request timeout
    pub timeout: Duration,
    /// Chain ID
    pub chain_id: String,
}

impl Default for AkashApiConfig {
    fn default() -> Self {
        Self {
            grpc_endpoint: "https://grpc-akash.ecostake.com:443".to_string(),
            rest_endpoint: "https://rest-akash.ecostake.com".to_string(),
            timeout: Duration::from_secs(30),
            chain_id: "akashnet-2".to_string(),
        }
    }
}

/// Main Akash API client that provides access to all Akash services
pub struct AkashApiClient {
    config: AkashApiConfig,
    http_client: HttpClient,
}

impl AkashApiClient {
    /// Create a new Akash API client with the given configuration
    pub fn new(config: AkashApiConfig) -> Result<Self> {
        let http_client = HttpClient::builder().timeout(config.timeout).build()?;

        Ok(Self {
            config,
            http_client,
        })
    }

    /// Query deployment information
    pub async fn query_deployment(&mut self, owner: &str, dseq: u64) -> Result<Value> {
        // TODO: Implement actual gRPC query
        println!("[API] Querying deployment {} for owner {}", dseq, owner);
        Ok(serde_json::json!({
            "deployment": {
                "deployment_id": {
                    "owner": owner,
                    "dseq": dseq
                },
                "state": "active"
            }
        }))
    }

    /// Query deployments with filters
    pub async fn query_deployments(
        &mut self,
        _owner: Option<&str>,
        _state: Option<&str>,
    ) -> Result<QueryDeploymentsResponse> {

        // TODO: Implement actual gRPC query using tonic client
        // For now, return a stub response
        let response = QueryDeploymentsResponse {
            deployments: vec![], // Empty for now
            pagination: None,
        };

        Ok(response)
    }

    /// Get the current chain ID
    pub fn chain_id(&self) -> &str {
        &self.config.chain_id
    }

    /// Get the REST endpoint for fallback HTTP queries
    pub fn rest_endpoint(&self) -> &str {
        &self.config.rest_endpoint
    }

    /// Get the HTTP client for REST API calls
    pub fn http_client(&self) -> &HttpClient {
        &self.http_client
    }
}

/// Convenience function to create a client with default configuration
pub fn create_default_client() -> Result<AkashApiClient> {
    AkashApiClient::new(AkashApiConfig::default())
}
