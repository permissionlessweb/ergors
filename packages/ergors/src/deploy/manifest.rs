//! Manifest management for Akash deployments.
//!
//! This module demonstrates how to use the `akash-deploy-rs` library for manifest
//! building, combined with application-level JWT authentication and provider
//! communication.
//!
//! # Architecture
//!
//! - **Manifest building**: Delegated to `akash-deploy-rs` library
//!   - `ManifestBuilder` for SDL → manifest conversion
//!   - `to_canonical_json` for deterministic hash computation
//!   - All manifest types (ManifestService, ManifestGroup, etc.)
//!
//! - **Provider communication**: Application logic in this module
//!   - `JwtAuthClient` for ES256K JWT generation
//!   - `ManifestSender` for sending manifests to providers
//!   - Endpoint querying and status checking
//!
//! # JWT Authentication
//!
//! JWTs are self-attested by the client:
//! 1. Client creates JWT with claims (issuer = account address, timestamps)
//! 2. Client signs JWT with their secp256k1 private key (ES256K)
//! 3. Client sends JWT in `Authorization: Bearer` header
//! 4. Provider validates by fetching public key from on-chain account state
//!
//! There is NO challenge-response flow or registration step.

use anyhow::{anyhow, Result};
use reqwest::Client as HttpClient;
use std::collections::HashMap;
use std::time::Duration;

use ho_std::keys::cosmos::CosmosKeyPair;

// ============================================================================
// Re-export types from akash-deploy-rs library
// ============================================================================

// Manifest types
pub use akash_deploy_rs::{
    to_canonical_json, ManifestBuilder, ManifestCredentials, ManifestCpu, ManifestGroup,
    ManifestGpu, ManifestHttpOptions, ManifestMemory, ManifestResourceValue, ManifestResources,
    ManifestService, ManifestServiceExpose, ManifestServiceParams, ManifestStorage,
    ManifestStorageParams,
};

// JWT types - use library implementation instead of duplicating
pub use akash_deploy_rs::{CachedJwt, JwtBuilder, JwtClaims, JwtLeases};

// ============================================================================
// JWT Authentication (uses akash-deploy-rs library)
// ============================================================================

/// JWT authentication client for Akash providers.
///
/// JWTs are self-attested by the client:
/// 1. Client creates JWT with claims (issuer = account address, timestamps)
/// 2. Client signs JWT with their secp256k1 private key (ES256K)
/// 3. Client sends JWT in Authorization: Bearer header
/// 4. Provider validates by fetching public key from on-chain account state
///
/// There is NO challenge-response or registration - each request is independently validated.
pub struct JwtAuthClient {
    pub http: HttpClient,
    provider_uri: String,
    /// Cached token (refreshed when expired)
    cached_token: Option<CachedJwt>,
}

impl JwtAuthClient {
    /// Create a new JWT auth client for a provider.
    pub fn new(provider_uri: &str) -> Self {
        let http = HttpClient::builder()
            .timeout(Duration::from_secs(30))
            .danger_accept_invalid_certs(true) // Providers use self-signed certs
            .build()
            .expect("http client");

        Self {
            http,
            provider_uri: provider_uri.trim_end_matches('/').to_string(),
            cached_token: None,
        }
    }

    /// Generate JWT for provider authentication.
    ///
    /// Creates a self-attested JWT signed with the client's private key.
    /// Provider validates by fetching public key from on-chain account state.
    ///
    /// Token is valid for 15 minutes (900 seconds).
    pub fn generate_jwt(&mut self, address: &str, keypair: &CosmosKeyPair) -> Result<String> {
        // Check if we have a cached token that's still valid
        if let Some(cached) = &self.cached_token {
            if let Some(token) = cached.get_if_valid() {
                return Ok(token.to_string());
            }
        }

        // Build claims using akash-deploy-rs library types
        let claims = JwtClaims::new(address)
            .with_jti(&uuid::Uuid::new_v4().to_string());

        // Build and sign using akash-deploy-rs JwtBuilder
        let jwt = JwtBuilder::new().build_and_sign(&claims, |message| {
            keypair.sign_jwt_es256k(message)
        })?;

        // Cache the token (14 min buffer for 15 min validity)
        self.cached_token = Some(CachedJwt::new(jwt.clone(), Duration::from_secs(840)));

        Ok(jwt)
    }
}

// ============================================================================
// Provider Communication
// ============================================================================

/// Manifest sender for provider communication with JWT authentication.
pub struct ManifestSender {
    auth: JwtAuthClient,
    provider_uri: String,
}

impl ManifestSender {
    /// Create a new manifest sender with JWT authentication.
    pub fn new(provider_uri: &str) -> Self {
        Self {
            auth: JwtAuthClient::new(provider_uri),
            provider_uri: provider_uri.trim_end_matches('/').to_string(),
        }
    }

    /// Send manifest to provider.
    ///
    /// The provider validates:
    /// 1. JWT signature against on-chain public key
    /// 2. Manifest hash matches on-chain deployment.version
    ///
    /// If hash mismatch: "manifest version validation failed"
    pub async fn send_manifest(
        &mut self,
        owner: &str,
        dseq: u64,
        _gseq: u32,
        _oseq: u32,
        manifest: &[ManifestGroup],
        address: &str,
        keypair: &CosmosKeyPair,
    ) -> Result<()> {
        // Provider recomputes SHA256(manifest_json) and compares to on-chain hash
        use crate::deploy::deployment_builder::to_canonical_json;
        let manifest_json = to_canonical_json(manifest)?;

        tracing::debug!(
            "Sending manifest to {}/deployment/{}/manifest",
            self.provider_uri,
            dseq
        );
        tracing::debug!("Manifest JSON (canonical): {}", manifest_json);

        let jwt = self.auth.generate_jwt(address, keypair)?;

        let url = format!("{}/deployment/{}/manifest", self.provider_uri, dseq);

        let response = self
            .auth
            .http
            .put(&url)
            .header("Authorization", format!("Bearer {}", jwt))
            .header("Content-Type", "application/json")
            .body(manifest_json)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(anyhow!(
                "Provider rejected manifest (HTTP {}): {}",
                status,
                body
            ));
        }

        tracing::info!(
            "Successfully sent manifest to provider for dseq {} (owner: {})",
            dseq,
            owner
        );

        Ok(())
    }

    /// Send manifest from SDL YAML.
    ///
    /// Convenience method that builds the manifest from SDL before sending.
    pub async fn send_manifest_from_sdl(
        &mut self,
        owner: &str,
        dseq: u64,
        gseq: u32,
        oseq: u32,
        sdl_yaml: &str,
        address: &str,
        keypair: &CosmosKeyPair,
    ) -> Result<()> {
        let builder = ManifestBuilder::new(owner, dseq);
        let manifest = builder.build_from_sdl(sdl_yaml)?;
        self.send_manifest(owner, dseq, gseq, oseq, &manifest, address, keypair)
            .await
    }
}

/// Service endpoint information from provider.
#[derive(Debug, Clone)]
pub struct ServiceEndpoint {
    pub external_uri: String,
    pub internal_port: u32,
    pub external_port: u32,
    pub protocol: String,
}

/// Query service endpoints from provider lease status.
///
/// Returns a map of service name → endpoint info.
pub async fn query_service_endpoints(
    provider_uri: &str,
    owner: &str,
    dseq: u64,
    gseq: u32,
    oseq: u32,
    address: &str,
    keypair: &CosmosKeyPair,
) -> Result<HashMap<String, ServiceEndpoint>> {
    let mut auth = JwtAuthClient::new(provider_uri);

    let path = format!("/lease/{}/{}/{}/{}/status", owner, dseq, gseq, oseq);

    tracing::info!("Querying endpoints from {}{}", provider_uri, path);

    let jwt = auth.generate_jwt(address, keypair)?;

    let response = auth
        .http
        .get(format!("{}{}", provider_uri, path))
        .header("Authorization", format!("Bearer {}", jwt))
        .send()
        .await?;

    if !response.status().is_success() {
        return Err(anyhow!(
            "Provider status query failed: {}",
            response.status()
        ));
    }

    let body = response.text().await?;
    parse_lease_status_endpoints_v2(&body)
}

/// Parse lease status JSON to extract service endpoints (v2 - structured).
fn parse_lease_status_endpoints_v2(json_str: &str) -> Result<HashMap<String, ServiceEndpoint>> {
    let status: serde_json::Value = serde_json::from_str(json_str)?;

    let mut endpoints = HashMap::new();

    // Try new format: services[].uris[]
    if let Some(services) = status.get("services").and_then(|s| s.as_array()) {
        for service in services {
            if let (Some(name), Some(uris)) = (
                service.get("name").and_then(|n| n.as_str()),
                service.get("uris").and_then(|u| u.as_array()),
            ) {
                if let Some(uri) = uris.first().and_then(|u| u.as_str()) {
                    // Extract port from URI
                    let (external_port, protocol) = if uri.starts_with("https://") {
                        (443, "TCP".to_string())
                    } else if uri.starts_with("http://") {
                        if let Some(port_str) = uri.split(':').nth(2) {
                            (port_str.parse().unwrap_or(80), "TCP".to_string())
                        } else {
                            (80, "TCP".to_string())
                        }
                    } else {
                        (80, "TCP".to_string())
                    };

                    endpoints.insert(
                        name.to_string(),
                        ServiceEndpoint {
                            external_uri: uri.to_string(),
                            internal_port: external_port,
                            external_port,
                            protocol,
                        },
                    );
                }
            }
        }
    }

    // Fallback to old format: forwarded_ports
    if endpoints.is_empty() {
        if let Some(ports) = status.get("forwarded_ports").and_then(|p| p.as_object()) {
            for (service_name, port_info) in ports {
                if let Some(port_obj) = port_info.as_object() {
                    if let (Some(host), Some(port)) = (
                        port_obj.get("host").and_then(|h| h.as_str()),
                        port_obj.get("port").and_then(|p| p.as_u64()),
                    ) {
                        let uri = if port == 443 {
                            format!("https://{}", host)
                        } else if port == 80 {
                            format!("http://{}", host)
                        } else {
                            format!("http://{}:{}", host, port)
                        };
                        let proto = port_obj
                            .get("proto")
                            .and_then(|p| p.as_str())
                            .unwrap_or("TCP")
                            .to_uppercase();

                        endpoints.insert(
                            service_name.clone(),
                            ServiceEndpoint {
                                external_uri: uri,
                                internal_port: port as u32,
                                external_port: port as u32,
                                protocol: proto,
                            },
                        );
                    }
                }
            }
        }
    }

    if endpoints.is_empty() {
        return Err(anyhow!("No endpoints found in lease status"));
    }

    Ok(endpoints)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_SDL: &str = r#"
version: "2.0"
services:
  web:
    image: nginx:latest
    expose:
      - port: 80
        as: 80
        to:
          - global: true
profiles:
  compute:
    web:
      resources:
        cpu:
          units: 0.5
        memory:
          size: 512Mi
        storage:
          size: 1Gi
  placement:
    dcloud:
      attributes:
        datacenter: us-west
      pricing:
        web:
          denom: uakt
          amount: 1000
deployment:
  web:
    dcloud:
      count: 1
"#;

    #[test]
    fn test_build_manifest_from_sdl() {
        let builder = ManifestBuilder::new("akash1owner", 12345);
        let groups = builder.build_from_sdl(SAMPLE_SDL).unwrap();

        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].name, "dcloud");
        assert_eq!(groups[0].services.len(), 1);
        assert_eq!(groups[0].services[0].name, "web");
        assert_eq!(groups[0].services[0].image, "nginx:latest");
        assert_eq!(groups[0].services[0].count, 1);
    }

    #[test]
    fn test_parse_endpoints_new_format() {
        let json = r#"{
            "services": [
                {
                    "name": "web",
                    "uris": ["https://web.example.com"]
                }
            ]
        }"#;

        let endpoints = parse_lease_status_endpoints_v2(json).unwrap();
        let ep = endpoints.get("web").unwrap();
        assert_eq!(ep.external_uri, "https://web.example.com");
        assert_eq!(ep.external_port, 443);
    }

    #[test]
    fn test_parse_endpoints_old_format() {
        let json = r#"{
            "forwarded_ports": {
                "web": {
                    "host": "example.com",
                    "port": 80,
                    "proto": "TCP"
                }
            }
        }"#;

        let endpoints = parse_lease_status_endpoints_v2(json).unwrap();
        let ep = endpoints.get("web").unwrap();
        assert_eq!(ep.external_uri, "http://example.com");
        assert_eq!(ep.external_port, 80);
        assert_eq!(ep.protocol, "TCP");
    }
}
