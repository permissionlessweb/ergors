//! Certificate management for Akash deployments.
//!
//! Handles:
//! - Checking for existing valid certificates on chain
//! - Certificate creation workflow (X.509 PEM format)
//! - Converting between chain types and proto types
//!
//! Uses `rcgen` for proper X.509 certificate generation compatible with
//! Akash provider mTLS requirements.

use anyhow::{anyhow, Result};
use ho_std::types::akash::cert::v1::{Certificate, MsgCreateCertificate, State};
use ho_std::types::ergors::orch::v1::{AkashCertState, AkashCertificateInfo};
use prost::Name;
use rcgen::{CertificateParams, DistinguishedName, DnType, KeyPair, PKCS_ECDSA_P256_SHA256};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use std::time::Duration;

use super::cosmos_client::CosmosClient;
use super::signer::msg_to_any;
use super::tx_lifecycle::{TxLifecycle, DEFAULT_GAS_LIMIT, DEFAULT_GAS_PRICE};

/// Certificate manager for Akash deployments.
pub struct CertificateManager {
    cosmos: Arc<CosmosClient>,
    tx_lifecycle: Arc<TxLifecycle>,
}

impl CertificateManager {
    /// Create a new certificate manager.
    pub fn new(cosmos: Arc<CosmosClient>, tx_lifecycle: Arc<TxLifecycle>) -> Self {
        Self {
            cosmos,
            tx_lifecycle,
        }
    }

    /// Get or create a valid certificate for the address.
    ///
    /// 1. Query chain for existing valid certificate
    /// 2. If none exists (or query fails), generate and broadcast new certificate
    /// 3. Handle duplicate certificate errors gracefully
    pub async fn get_or_create(
        &self,
        key_name: &str,
        account_index: u32,
        address: &str,
    ) -> Result<AkashCertificateInfo> {
        tracing::info!("  Querying chain for existing certificate...");

        // Query chain for existing valid certificate
        match self.cosmos.query_valid_certificate(address).await {
            Ok(Some(chain_cert)) => {
                tracing::info!("  Certificate: FOUND EXISTING");
                tracing::info!("    State:  {:?}", State::try_from(chain_cert.state).ok());
                return Ok(certificate_to_workflow_info(address, &chain_cert, "existing"));
            }
            Ok(None) => {
                tracing::info!("  Certificate: NOT FOUND - creating new one...");
            }
            Err(e) => {
                // Some REST endpoints don't implement certificate queries (501 Not Implemented)
                // In this case, try to create a certificate - it will fail with a clear error
                // if one already exists
                tracing::warn!(
                    "  Certificate query failed: {} - attempting to create new certificate",
                    e
                );
            }
        }

        // Create new certificate
        match self
            .create_certificate(key_name, account_index, address)
            .await
        {
            Ok(cert) => Ok(cert),
            Err(e) => {
                let error_str = e.to_string();
                // Check for duplicate certificate error (certificate already exists)
                if error_str.contains("certificate exists")
                    || error_str.contains("already exists")
                    || error_str.contains("duplicate")
                {
                    tracing::info!("  Certificate already exists on chain (creation rejected)");
                    // Return a placeholder cert info - the actual cert exists on chain
                    // and will be used for mTLS by the provider
                    Ok(AkashCertificateInfo {
                        owner: address.to_string(),
                        serial: "existing".to_string(),
                        state: AkashCertState::Valid as i32,
                        cert_pem: vec![],
                        pubkey: vec![],
                    })
                } else {
                    Err(e)
                }
            }
        }
    }

    /// Create a new certificate and broadcast to chain.
    async fn create_certificate(
        &self,
        key_name: &str,
        account_index: u32,
        address: &str,
    ) -> Result<AkashCertificateInfo> {
        // Generate certificate and key pair
        tracing::info!("  Generating certificate keypair...");
        let (cert_bytes, pubkey_bytes, serial) = generate_akash_certificate(address)?;
        tracing::info!("    Serial: {}", serial);

        // Build MsgCreateCertificate
        let msg = MsgCreateCertificate {
            owner: address.to_string(),
            cert: cert_bytes.clone(),
            pubkey: pubkey_bytes.clone(),
        };

        let msg_any = msg_to_any(
            &msg,
            &MsgCreateCertificate::type_url(),
        );

        // Broadcast and wait for finality
        tracing::info!("  Broadcasting MsgCreateCertificate...");
        let result = self
            .tx_lifecycle
            .sign_broadcast_wait(
                key_name,
                account_index,
                msg_any,
                DEFAULT_GAS_LIMIT,
                DEFAULT_GAS_PRICE,
                Some("ergors certificate creation"),
            )
            .await?;

        if !result.is_success() {
            tracing::error!("  FAILED: Certificate tx rejected (code {})", result.code);
            tracing::error!("  Error: {}", result.raw_log);
            return Err(anyhow!(
                "Certificate creation failed (code {}): {}",
                result.code,
                result.raw_log
            ));
        }

        tracing::info!("  Certificate: CREATED NEW");
        tracing::info!("    Tx Hash: {}", result.hash);
        tracing::info!("    Height:  {}", result.height);
        tracing::info!("    Serial:  {}", serial);

        Ok(AkashCertificateInfo {
            owner: address.to_string(),
            serial,
            state: AkashCertState::Valid as i32,
            cert_pem: cert_bytes,
            pubkey: pubkey_bytes,
        })
    }

    /// Query certificate from chain by address.
    pub async fn query_certificate(&self, address: &str) -> Result<Option<AkashCertificateInfo>> {
        match self.cosmos.query_valid_certificate(address).await? {
            Some(cert) => Ok(Some(certificate_to_workflow_info(address, &cert, "unknown"))),
            None => Ok(None),
        }
    }

    /// Check if an address has a valid certificate.
    pub async fn has_valid_certificate(&self, address: &str) -> Result<bool> {
        Ok(self
            .cosmos
            .query_valid_certificate(address)
            .await?
            .is_some())
    }
}

/// Convert Akash Certificate (prost type) to workflow AkashCertificateInfo.
///
/// This keeps certificate details minimal - users only need to know the cert exists and its state.
fn certificate_to_workflow_info(
    owner: &str,
    cert: &Certificate,
    serial: &str,
) -> AkashCertificateInfo {
    // Map Akash chain state enum to our workflow state enum
    let state = match State::try_from(cert.state) {
        Ok(State::Valid) => AkashCertState::Valid,
        Ok(State::Revoked) => AkashCertState::Revoked,
        _ => AkashCertState::Invalid,
    };

    AkashCertificateInfo {
        owner: owner.to_string(),
        serial: serial.to_string(),
        state: state as i32,
        cert_pem: cert.cert.clone(),
        pubkey: cert.pubkey.clone(),
    }
}

/// Generate an Akash-compatible X.509 certificate in PEM format.
///
/// This creates a self-signed ECDSA P-256 certificate for mTLS authentication
/// between tenants and providers on Akash Network.
///
/// Returns (cert_pem_bytes, pubkey_pem_bytes, serial).
fn generate_akash_certificate(address: &str) -> Result<(Vec<u8>, Vec<u8>, String)> {
    // Generate ECDSA P-256 key pair (compatible with Akash)
    let key_pair = KeyPair::generate_for(&PKCS_ECDSA_P256_SHA256)
        .map_err(|e| anyhow!("Failed to generate key pair: {}", e))?;

    // Generate a unique serial number from hash
    let random_bytes: [u8; 16] = rand::random();
    let mut hasher = Sha256::new();
    hasher.update(random_bytes);
    hasher.update(address.as_bytes());
    let hash = hasher.finalize();
    let serial = hex::encode(&hash[..16]);

    // Build certificate parameters
    let mut params = CertificateParams::default();

    // Set subject with Common Name = address (required by Akash)
    let mut distinguished_name = DistinguishedName::new();
    distinguished_name.push(DnType::CommonName, address);
    params.distinguished_name = distinguished_name;

    // Set validity period (1 year from now)
    params.not_before = time::OffsetDateTime::now_utc();
    params.not_after = time::OffsetDateTime::now_utc() + Duration::from_secs(365 * 24 * 60 * 60);

    // Get public key PEM before moving key_pair
    let pubkey_pem = key_pair.public_key_pem();
    let pubkey_bytes = pubkey_pem.as_bytes().to_vec();

    // Generate the self-signed certificate
    let cert = params
        .self_signed(&key_pair)
        .map_err(|e| anyhow!("Failed to generate certificate: {}", e))?;

    // Get PEM-encoded certificate
    let cert_pem = cert.pem();
    let cert_bytes = cert_pem.as_bytes().to_vec();

    tracing::debug!("Generated certificate PEM ({} bytes)", cert_bytes.len());
    tracing::debug!("Generated public key PEM ({} bytes)", pubkey_bytes.len());

    Ok((cert_bytes, pubkey_bytes, serial))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_certificate_to_workflow_info() {
        let cert = Certificate {
            state: State::Valid as i32,
            cert: b"cert_data".to_vec(),
            pubkey: b"pubkey_data".to_vec(),
        };

        let workflow_info = certificate_to_workflow_info("akash1test", &cert, "abc123");

        assert_eq!(workflow_info.owner, "akash1test");
        assert_eq!(workflow_info.serial, "abc123");
        assert_eq!(workflow_info.state, AkashCertState::Valid as i32);
        assert_eq!(workflow_info.cert_pem, b"cert_data");
        assert_eq!(workflow_info.pubkey, b"pubkey_data");
    }

    #[test]
    fn test_generate_akash_certificate() {
        let result = generate_akash_certificate("akash1testaddress");
        assert!(result.is_ok());

        let (cert, pubkey, serial) = result.unwrap();
        assert!(!cert.is_empty());
        assert!(!pubkey.is_empty());
        assert_eq!(serial.len(), 32); // 16 bytes hex encoded
    }
}
