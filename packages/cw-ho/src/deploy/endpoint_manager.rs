//! Multi-endpoint management with automatic failover.
//!
//! Provides resilient endpoint rotation when transport errors occur,
//! preventing workflow failures due to single endpoint unavailability.

use anyhow::{anyhow, Result};
use ho_std::types::ergors::orch::v1::AkashDeployConfig;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

/// Default retry configuration
const DEFAULT_MAX_RETRIES_PER_ENDPOINT: u32 = 2;
const DEFAULT_MAX_TOTAL_RETRIES: u32 = 6;
const DEFAULT_CONNECTION_TIMEOUT_SECONDS: u32 = 10;

/// Endpoint type for rotation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndpointType {
    Rpc,
    Grpc,
    Rest,
}

/// Manages multiple endpoints with automatic failover.
///
/// This provides:
/// - Round-robin rotation through available endpoints
/// - Configurable retry limits per endpoint and total
/// - Transport error detection and automatic failover
/// - Thread-safe endpoint selection
#[derive(Clone)]
pub struct EndpointManager {
    /// All available RPC endpoints
    rpc_endpoints: Vec<String>,
    /// All available gRPC endpoints
    grpc_endpoints: Vec<String>,
    /// All available REST endpoints
    rest_endpoints: Vec<String>,
    /// Current RPC endpoint index (atomic for thread-safety)
    rpc_index: Arc<AtomicUsize>,
    /// Current gRPC endpoint index
    grpc_index: Arc<AtomicUsize>,
    /// Current REST endpoint index
    rest_index: Arc<AtomicUsize>,
    /// Max retries per endpoint before moving to next
    max_retries_per_endpoint: u32,
    /// Total max retries across all endpoints
    max_total_retries: u32,
    /// Connection timeout in seconds
    connection_timeout_seconds: u32,
}

impl EndpointManager {
    /// Create endpoint manager from Akash config.
    pub fn from_config(config: &AkashDeployConfig) -> Self {
        let rpc_endpoints = config.rpc_endpoints.clone();
        let grpc_endpoints = config.grpc_endpoints.clone();
        let rest_endpoints = config.rest_endpoints.clone();

        let max_retries_per_endpoint = if config.max_retries_per_endpoint > 0 {
            config.max_retries_per_endpoint
        } else {
            DEFAULT_MAX_RETRIES_PER_ENDPOINT
        };

        let max_total_retries = if config.max_total_retries > 0 {
            config.max_total_retries
        } else {
            DEFAULT_MAX_TOTAL_RETRIES
        };

        let connection_timeout_seconds = if config.connection_timeout_seconds > 0 {
            config.connection_timeout_seconds
        } else {
            DEFAULT_CONNECTION_TIMEOUT_SECONDS
        };

        tracing::debug!(
            "EndpointManager initialized: rpc={}, grpc={}, rest={}, max_per={}, max_total={}",
            rpc_endpoints.len(),
            grpc_endpoints.len(),
            rest_endpoints.len(),
            max_retries_per_endpoint,
            max_total_retries
        );

        Self {
            rpc_endpoints,
            grpc_endpoints,
            rest_endpoints,
            rpc_index: Arc::new(AtomicUsize::new(0)),
            grpc_index: Arc::new(AtomicUsize::new(0)),
            rest_index: Arc::new(AtomicUsize::new(0)),
            max_retries_per_endpoint,
            max_total_retries,
            connection_timeout_seconds,
        }
    }

    /// Get current endpoint for the specified type.
    pub fn current_endpoint(&self, endpoint_type: EndpointType) -> Result<String> {
        let (endpoints, index) = match endpoint_type {
            EndpointType::Rpc => (&self.rpc_endpoints, &self.rpc_index),
            EndpointType::Grpc => (&self.grpc_endpoints, &self.grpc_index),
            EndpointType::Rest => (&self.rest_endpoints, &self.rest_index),
        };

        if endpoints.is_empty() {
            return Err(anyhow!("No {:?} endpoints configured", endpoint_type));
        }

        let idx = index.load(Ordering::Relaxed) % endpoints.len();
        Ok(endpoints[idx].clone())
    }

    /// Rotate to next endpoint after failure.
    ///
    /// Returns the new current endpoint.
    pub fn rotate_endpoint(&self, endpoint_type: EndpointType) -> Result<String> {
        let (endpoints, index) = match endpoint_type {
            EndpointType::Rpc => (&self.rpc_endpoints, &self.rpc_index),
            EndpointType::Grpc => (&self.grpc_endpoints, &self.grpc_index),
            EndpointType::Rest => (&self.rest_endpoints, &self.rest_index),
        };

        if endpoints.is_empty() {
            return Err(anyhow!("No {:?} endpoints configured", endpoint_type));
        }

        // Increment and wrap around
        let old_idx = index.fetch_add(1, Ordering::Relaxed);
        let new_idx = (old_idx + 1) % endpoints.len();

        tracing::info!(
            "Rotated {:?} endpoint: {} -> {}",
            endpoint_type,
            endpoints[old_idx % endpoints.len()],
            endpoints[new_idx]
        );

        Ok(endpoints[new_idx].clone())
    }

    /// Execute operation with automatic endpoint failover.
    ///
    /// This will:
    /// 1. Try current endpoint up to `max_retries_per_endpoint` times
    /// 2. On transport error, rotate to next endpoint and retry
    /// 3. Stop after `max_total_retries` total attempts
    ///
    /// # Arguments
    /// * `endpoint_type` - Type of endpoint to use
    /// * `operation` - Async function that takes endpoint URL and returns Result
    ///
    /// # Returns
    /// * `Ok(T)` - Operation succeeded
    /// * `Err` - All retries exhausted or non-transport error
    pub async fn execute_with_failover<T, F, Fut>(
        &self,
        endpoint_type: EndpointType,
        mut operation: F,
    ) -> Result<T>
    where
        F: FnMut(String) -> Fut,
        Fut: std::future::Future<Output = Result<T>>,
    {
        let mut total_attempts = 0;
        let mut per_endpoint_attempts = 0;
        let mut last_error: Option<anyhow::Error> = None;

        loop {
            if total_attempts >= self.max_total_retries {
                let error_msg = match &last_error {
                    Some(e) => e.to_string(),
                    None => "unknown".to_string(),
                };
                return Err(anyhow!(
                    "All retry attempts exhausted ({} total attempts across {} endpoints). Last error: {}",
                    total_attempts,
                    self.endpoint_count(endpoint_type),
                    error_msg
                ));
            }

            let endpoint = self.current_endpoint(endpoint_type)?;
            total_attempts += 1;
            per_endpoint_attempts += 1;

            tracing::debug!(
                "Attempting {:?} operation: endpoint={}, attempt={}/{}",
                endpoint_type,
                endpoint,
                total_attempts,
                self.max_total_retries
            );

            match operation(endpoint.clone()).await {
                Ok(result) => {
                    if total_attempts > 1 {
                        tracing::info!(
                            "Operation succeeded after {} attempts on endpoint: {}",
                            total_attempts,
                            endpoint
                        );
                    }
                    return Ok(result);
                }
                Err(e) => {
                    let is_transport_error = Self::is_transport_error(&e);

                    tracing::warn!(
                        "Operation failed on endpoint {}: {} (transport_error={}, attempt={}/{})",
                        endpoint,
                        e,
                        is_transport_error,
                        per_endpoint_attempts,
                        self.max_retries_per_endpoint
                    );

                    last_error = Some(e);

                    // If transport error and haven't exhausted per-endpoint retries, retry same endpoint
                    if is_transport_error && per_endpoint_attempts < self.max_retries_per_endpoint {
                        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                        continue;
                    }

                    // If we have more endpoints available, rotate to next
                    if self.endpoint_count(endpoint_type) > 1 {
                        self.rotate_endpoint(endpoint_type)?;
                        per_endpoint_attempts = 0; // Reset counter for new endpoint
                        tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
                        continue;
                    }

                    // No more endpoints to try, return error
                    if !is_transport_error {
                        // Non-transport errors shouldn't trigger retries
                        return Err(last_error.unwrap());
                    }
                }
            }
        }
    }

    /// Check if error is a transport/network error (should trigger failover).
    fn is_transport_error(error: &anyhow::Error) -> bool {
        let error_str = error.to_string().to_lowercase();

        // Common transport error patterns
        error_str.contains("transport error")
            || error_str.contains("connection refused")
            || error_str.contains("connection reset")
            || error_str.contains("connection timeout")
            || error_str.contains("broken pipe")
            || error_str.contains("network unreachable")
            || error_str.contains("host unreachable")
            || error_str.contains("dns")
            || error_str.contains("tls handshake")
            || error_str.contains("certificate")
            || error_str.contains("timeout")
            || error_str.contains("connect")
            || error_str.contains("tonic::status")
    }

    /// Get number of configured endpoints for a type.
    pub fn endpoint_count(&self, endpoint_type: EndpointType) -> usize {
        match endpoint_type {
            EndpointType::Rpc => self.rpc_endpoints.len(),
            EndpointType::Grpc => self.grpc_endpoints.len(),
            EndpointType::Rest => self.rest_endpoints.len(),
        }
    }

    /// Get all endpoints for a type (for logging/debugging).
    pub fn all_endpoints(&self, endpoint_type: EndpointType) -> &[String] {
        match endpoint_type {
            EndpointType::Rpc => &self.rpc_endpoints,
            EndpointType::Grpc => &self.grpc_endpoints,
            EndpointType::Rest => &self.rest_endpoints,
        }
    }

    /// Get retry configuration.
    pub fn retry_config(&self) -> (u32, u32, u32) {
        (
            self.max_retries_per_endpoint,
            self.max_total_retries,
            self.connection_timeout_seconds,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_transport_error() {
        assert!(EndpointManager::is_transport_error(&anyhow!(
            "transport error"
        )));
        assert!(EndpointManager::is_transport_error(&anyhow!(
            "connection refused"
        )));
        assert!(EndpointManager::is_transport_error(&anyhow!(
            "DNS resolution failed"
        )));
        assert!(!EndpointManager::is_transport_error(&anyhow!(
            "invalid message format"
        )));
        assert!(!EndpointManager::is_transport_error(&anyhow!(
            "unauthorized"
        )));
    }

    #[test]
    fn test_endpoint_rotation() {
        let config = AkashDeployConfig {
            rpc_endpoints: vec![
                "http://rpc1.example.com".to_string(),
                "http://rpc2.example.com".to_string(),
                "http://rpc3.example.com".to_string(),
            ],
            ..Default::default()
        };

        let manager = EndpointManager::from_config(&config);

        // Should cycle through endpoints
        let ep1 = manager.current_endpoint(EndpointType::Rpc).unwrap();
        assert_eq!(ep1, "http://rpc1.example.com");

        let ep2 = manager.rotate_endpoint(EndpointType::Rpc).unwrap();
        assert_eq!(ep2, "http://rpc2.example.com");

        let ep3 = manager.rotate_endpoint(EndpointType::Rpc).unwrap();
        assert_eq!(ep3, "http://rpc3.example.com");

        // Should wrap around
        let ep1_again = manager.rotate_endpoint(EndpointType::Rpc).unwrap();
        assert_eq!(ep1_again, "http://rpc1.example.com");
    }
}
