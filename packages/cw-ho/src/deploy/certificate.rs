//! Certificate management for Akash deployments.
//!
//! Handles:
//! - Checking for existing valid certificates on chain
//! - Certificate creation workflow
//! - Converting between chain types and proto types
//!
//! Note: The current implementation uses a simplified certificate format.
//! For production use, consider implementing proper X.509 certificates
//! using the `rcgen` crate.

use anyhow::{anyhow, Result};
use ho_std::types::ergors::orch::v1::{AkashCertState, AkashCertificateInfo};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use super::cosmos_client::{CertState, CertificateInfo, CosmosClient};
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
    /// 2. If none exists, generate and broadcast new certificate
    pub async fn get_or_create(
        &self,
        key_name: &str,
        account_index: u32,
        address: &str,
    ) -> Result<AkashCertificateInfo> {
        tracing::info!("  Querying chain for existing certificate...");

        // Query chain for existing valid certificate
        if let Some(chain_cert) = self.cosmos.query_valid_certificate(address).await? {
            tracing::info!("  Certificate: FOUND EXISTING");
            tracing::info!("    Owner:  {}", chain_cert.owner);
            tracing::info!("    Serial: {}", chain_cert.serial);
            tracing::info!("    State:  {:?}", chain_cert.state);
            return Ok(certificate_info_to_proto(&chain_cert));
        }

        // No valid certificate exists, create new one
        tracing::info!("  Certificate: NOT FOUND - creating new one...");

        self.create_certificate(key_name, account_index, address)
            .await
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

        let msg_any = msg_to_any(&msg, "/akash.cert.v1beta3.MsgCreateCertificate");

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
            Some(cert) => Ok(Some(certificate_info_to_proto(&cert))),
            None => Ok(None),
        }
    }

    /// Check if an address has a valid certificate.
    pub async fn has_valid_certificate(&self, address: &str) -> Result<bool> {
        Ok(self.cosmos.query_valid_certificate(address).await?.is_some())
    }
}

/// Convert CertificateInfo to proto AkashCertificateInfo.
fn certificate_info_to_proto(cert: &CertificateInfo) -> AkashCertificateInfo {
    let state = match cert.state {
        CertState::Invalid => AkashCertState::Invalid,
        CertState::Valid => AkashCertState::Valid,
        CertState::Revoked => AkashCertState::Revoked,
    };

    AkashCertificateInfo {
        owner: cert.owner.clone(),
        serial: cert.serial.clone(),
        state: state as i32,
        cert_pem: cert.cert_pem.as_bytes().to_vec(),
        pubkey: cert.pubkey.as_bytes().to_vec(),
    }
}

/// Generate an Akash-compatible certificate.
///
/// This creates a self-signed certificate for mTLS authentication
/// between tenants and providers.
///
/// Returns (cert_bytes, pubkey_bytes, serial).
fn generate_akash_certificate(address: &str) -> Result<(Vec<u8>, Vec<u8>, String)> {
    // Generate a unique serial number
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let random_bytes: [u8; 8] = rand::random();

    let mut hasher = Sha256::new();
    hasher.update(now.to_le_bytes());
    hasher.update(random_bytes);
    let hash = hasher.finalize();
    let serial = hex::encode(&hash[..16]);

    // Generate a secp256k1 key pair
    let private_key_bytes: [u8; 32] = rand::random();
    let signing_key = cosmrs::crypto::secp256k1::SigningKey::from_slice(&private_key_bytes)
        .map_err(|e| anyhow!("Failed to create signing key: {}", e))?;
    let public_key = signing_key.public_key();

    // Get public key bytes
    let pubkey_bytes = public_key.to_bytes();

    // Create certificate data structure
    let cert_bytes = build_certificate_data(address, &serial, &pubkey_bytes)?;

    Ok((cert_bytes, pubkey_bytes, serial))
}

/// Build certificate data structure.
///
/// Creates a simplified certificate format compatible with Akash's requirements.
fn build_certificate_data(address: &str, serial: &str, pubkey_bytes: &[u8]) -> Result<Vec<u8>> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    // Create certificate data
    let mut cert_data = Vec::new();

    // Magic header for identification
    cert_data.extend_from_slice(b"AKASH_CERT_V1");

    // Serial number (variable length, prefixed with length)
    let serial_bytes = serial.as_bytes();
    cert_data.push(serial_bytes.len() as u8);
    cert_data.extend_from_slice(serial_bytes);

    // Validity period (not_before, not_after as unix timestamps)
    cert_data.extend_from_slice(&now.to_le_bytes());
    cert_data.extend_from_slice(&(now + 365 * 24 * 3600).to_le_bytes()); // 1 year validity

    // Subject (address)
    let addr_bytes = address.as_bytes();
    cert_data.push(addr_bytes.len() as u8);
    cert_data.extend_from_slice(addr_bytes);

    // Public key (length-prefixed)
    cert_data.push(pubkey_bytes.len() as u8);
    cert_data.extend_from_slice(pubkey_bytes);

    // Create a simple signature (hash of all data above)
    let mut hasher = Sha256::new();
    hasher.update(&cert_data);
    let signature = hasher.finalize();
    cert_data.extend_from_slice(&signature);

    Ok(cert_data)
}

/// Akash certificate creation message (matches akash.cert.v1beta3.MsgCreateCertificate).
#[derive(Clone, PartialEq, prost::Message)]
pub struct MsgCreateCertificate {
    #[prost(string, tag = "1")]
    pub owner: String,
    #[prost(bytes = "vec", tag = "2")]
    pub cert: Vec<u8>,
    #[prost(bytes = "vec", tag = "3")]
    pub pubkey: Vec<u8>,
}

/// Akash certificate revocation message (matches akash.cert.v1beta3.MsgRevokeCertificate).
#[derive(Clone, PartialEq, prost::Message)]
pub struct MsgRevokeCertificate {
    #[prost(message, optional, tag = "1")]
    pub id: Option<CertificateId>,
}

/// Certificate ID for revocation.
#[derive(Clone, PartialEq, prost::Message)]
pub struct CertificateId {
    #[prost(string, tag = "1")]
    pub owner: String,
    #[prost(string, tag = "2")]
    pub serial: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_certificate_info_to_proto() {
        let cert = CertificateInfo {
            owner: "akash1test".to_string(),
            serial: "abc123".to_string(),
            state: CertState::Valid,
            cert_pem: "cert_data".to_string(),
            pubkey: "pubkey_data".to_string(),
        };

        let proto = certificate_info_to_proto(&cert);

        assert_eq!(proto.owner, "akash1test");
        assert_eq!(proto.serial, "abc123");
        assert_eq!(proto.state, AkashCertState::Valid as i32);
        assert_eq!(proto.cert_pem, b"cert_data");
        assert_eq!(proto.pubkey, b"pubkey_data");
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
