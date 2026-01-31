//! Certificate management for Akash deployments.
//!
//! Handles:
//! - Checking for existing valid certificates on chain
//! - Certificate creation workflow (X.509 PEM format)
//! - Private key encryption/decryption for secure storage
//! - mTLS authentication with Akash providers
//!
//! Uses `rcgen` for proper X.509 certificate generation compatible with
//! Akash provider mTLS requirements.

use anyhow::{anyhow, Result};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use ho_std::keys::encrypted_cosmos::EncryptedCosmosKeyManager;
use ho_std::types::akash::cert::v1::{Certificate, MsgCreateCertificate, MsgRevokeCertificate, State};
use ho_std::types::ergors::orch::v1::{AkashDeployConfig, CosmosKeyStore};
use prost::{Message, Name};
use rcgen::{CertificateParams, DistinguishedName, DnType, KeyPair, PKCS_ECDSA_P256_SHA256};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

use super::climb_signer::{chain_config_from_akash, create_signing_client};
use super::cosmos_client::CosmosClient;
use crate::storage::ErgorsStorage;
use layer_climb_proto::Any as ClimbAny;

/// Certificate with encrypted private key for mTLS.
/// The certificate is stored on-chain, but private key is encrypted locally.
#[derive(Debug, Clone)]
pub struct CertificateWithKey {
    /// Official Akash certificate (from chain or newly created)
    pub certificate: Certificate,
    /// Encrypted private key (ChaCha20Poly1305)
    /// Empty if certificate was fetched from chain without local key
    pub encrypted_private_key: Vec<u8>,
}

/// Certificate manager for Akash deployments.
/// Now uses layer-climb for robust transaction signing.
/// Persists encrypted private keys to storage for reuse across workflows.
pub struct CertificateManager {
    cosmos: Arc<CosmosClient>,
    key_manager: Arc<RwLock<EncryptedCosmosKeyManager>>,
    key_store: Arc<RwLock<CosmosKeyStore>>,
    akash_config: AkashDeployConfig,
    storage: Arc<ErgorsStorage>,
}

impl CertificateManager {
    /// Create a new certificate manager with layer-climb integration.
    pub fn new(
        cosmos: Arc<CosmosClient>,
        key_manager: Arc<RwLock<EncryptedCosmosKeyManager>>,
        key_store: Arc<RwLock<CosmosKeyStore>>,
        akash_config: AkashDeployConfig,
        storage: Arc<ErgorsStorage>,
    ) -> Self {
        Self {
            cosmos,
            key_manager,
            key_store,
            akash_config,
            storage,
        }
    }

    /// Get or create a valid certificate for the address.
    ///
    /// 1. Query chain for existing valid certificate
    /// 2. If found, try to load encrypted private key from storage
    /// 3. If none exists (or query fails), generate and broadcast new certificate
    /// 4. Save encrypted private key to storage for future use
    ///
    /// The `encryption_password` is used to encrypt the private key for secure storage.
    pub async fn get_or_create(
        &self,
        key_name: &str,
        account_index: u32,
        address: &str,
        encryption_password: &str,
    ) -> Result<CertificateWithKey> {
        tracing::info!("  Querying chain for existing certificate...");

        // Query chain for existing valid certificate
        match self.cosmos.query_valid_certificate(address).await {
            Ok(Some(chain_cert)) => {
                tracing::info!("  Certificate: FOUND EXISTING");
                tracing::info!("    State:  {:?}", State::try_from(chain_cert.state).ok());

                // Try to load encrypted private key from storage
                match self.storage.get_akash_cert_key(address).await {
                    Ok(Some(encrypted_key)) => {
                        tracing::info!("  Private key: LOADED FROM STORAGE ({} bytes)", encrypted_key.len());
                        return Ok(CertificateWithKey {
                            certificate: chain_cert,
                            encrypted_private_key: encrypted_key,
                        });
                    }
                    Ok(None) => {
                        tracing::warn!("  Private key: NOT IN STORAGE");
                        tracing::warn!("  Certificate exists on chain but private key is missing!");
                        tracing::warn!("  Run 'ergors deploy cert revoke' then 'ergors deploy cert create' to fix.");
                        return Ok(CertificateWithKey {
                            certificate: chain_cert,
                            encrypted_private_key: vec![],
                        });
                    }
                    Err(e) => {
                        tracing::warn!("  Failed to load private key from storage: {}", e);
                        return Ok(CertificateWithKey {
                            certificate: chain_cert,
                            encrypted_private_key: vec![],
                        });
                    }
                }
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
            .create_certificate(key_name, account_index, address, encryption_password)
            .await
        {
            Ok(cert_with_key) => {
                // Store encrypted private key for future use
                if !cert_with_key.encrypted_private_key.is_empty() {
                    if let Err(e) = self
                        .storage
                        .put_akash_cert_key(address, &cert_with_key.encrypted_private_key)
                        .await
                    {
                        tracing::warn!("  Failed to store encrypted private key: {}", e);
                    }
                }
                Ok(cert_with_key)
            }
            Err(e) => {
                let error_str = e.to_string();
                // Check for duplicate certificate error (certificate already exists)
                if error_str.contains("certificate exists")
                    || error_str.contains("already exists")
                    || error_str.contains("duplicate")
                {
                    tracing::info!("  Certificate already exists on chain (creation rejected)");

                    // Try to load stored key
                    let encrypted_key = self
                        .storage
                        .get_akash_cert_key(address)
                        .await
                        .ok()
                        .flatten()
                        .unwrap_or_default();

                    if encrypted_key.is_empty() {
                        tracing::warn!("  No stored private key - mTLS will fail!");
                    }

                    Ok(CertificateWithKey {
                        certificate: Certificate {
                            state: State::Valid as i32,
                            cert: vec![],
                            pubkey: vec![],
                        },
                        encrypted_private_key: encrypted_key,
                    })
                } else {
                    Err(e)
                }
            }
        }
    }

    /// Create a new certificate and broadcast to chain.
    ///
    /// Unlike `get_or_create`, this always creates a new certificate.
    /// Use this for explicit `cert create` CLI command.
    /// Returns (tx_hash, serial) on success.
    pub async fn create_new_certificate(
        &self,
        key_name: &str,
        account_index: u32,
        address: &str,
        encryption_password: &str,
    ) -> Result<(String, String)> {
        tracing::info!("Creating new certificate for {}", address);

        // Generate certificate and key pair
        let generated = generate_akash_certificate(address)?;
        tracing::info!("  Generated serial: {}", generated.serial);

        // Encrypt the private key for secure storage
        let encrypted_private_key = encrypt_private_key(&generated.privkey_pem, encryption_password)?;
        tracing::info!("  Encrypted key size: {} bytes", encrypted_private_key.len());

        // Build MsgCreateCertificate
        let msg = MsgCreateCertificate {
            owner: address.to_string(),
            cert: generated.cert_pem.clone(),
            pubkey: generated.pubkey_pem.clone(),
        };

        // Create signing client
        let chain_config = chain_config_from_akash(&self.akash_config)?;
        let client = create_signing_client(
            self.key_manager.clone(),
            self.key_store.clone(),
            key_name,
            account_index,
            chain_config,
        )
        .await?;

        // Broadcast
        let msg_any = ClimbAny {
            type_url: MsgCreateCertificate::type_url(),
            value: msg.encode_to_vec(),
        };

        tracing::info!("  Broadcasting MsgCreateCertificate...");
        let mut tx_builder = client.tx_builder();
        tx_builder.set_memo("ergors certificate creation");
        let tx_resp = tx_builder.broadcast(vec![msg_any]).await?;

        if tx_resp.code != 0 {
            return Err(anyhow!(
                "Certificate creation failed (code {}): {}",
                tx_resp.code,
                tx_resp.raw_log
            ));
        }

        tracing::info!("  Certificate created: tx_hash={}", tx_resp.txhash);

        // Store encrypted private key
        if let Err(e) = self
            .storage
            .put_akash_cert_key(address, &encrypted_private_key)
            .await
        {
            tracing::warn!("  Failed to store encrypted private key: {}", e);
        } else {
            tracing::info!("  Encrypted private key stored");
        }

        Ok((tx_resp.txhash, generated.serial))
    }

    /// Revoke a certificate on chain and delete stored private key.
    pub async fn revoke_certificate(
        &self,
        key_name: &str,
        account_index: u32,
        address: &str,
        serial: &str,
    ) -> Result<String> {
        tracing::info!("Revoking certificate for {} (serial: {})", address, serial);

        // Build MsgRevokeCertificate
        let msg = MsgRevokeCertificate {
            id: Some(ho_std::types::akash::cert::v1::Id {
                owner: address.to_string(),
                serial: serial.to_string(),
            }),
        };

        // Create signing client
        let chain_config = chain_config_from_akash(&self.akash_config)?;
        let client = create_signing_client(
            self.key_manager.clone(),
            self.key_store.clone(),
            key_name,
            account_index,
            chain_config,
        )
        .await?;

        // Broadcast
        let msg_any = ClimbAny {
            type_url: MsgRevokeCertificate::type_url(),
            value: msg.encode_to_vec(),
        };

        let mut tx_builder = client.tx_builder();
        tx_builder.set_memo("ergors certificate revocation");
        let tx_resp = tx_builder.broadcast(vec![msg_any]).await?;

        if tx_resp.code != 0 {
            return Err(anyhow!(
                "Certificate revocation failed (code {}): {}",
                tx_resp.code,
                tx_resp.raw_log
            ));
        }

        tracing::info!("  Certificate revoked: tx_hash={}", tx_resp.txhash);

        // Delete stored private key
        if let Err(e) = self.storage.delete_akash_cert_key(address).await {
            tracing::warn!("  Failed to delete stored private key: {}", e);
        }

        Ok(tx_resp.txhash)
    }

    /// Create a new certificate and broadcast to chain.
    /// Returns certificate with encrypted private key.
    async fn create_certificate(
        &self,
        key_name: &str,
        account_index: u32,
        address: &str,
        encryption_password: &str,
    ) -> Result<CertificateWithKey> {
        // Generate certificate and key pair
        tracing::info!("  Generating certificate keypair...");
        let generated = generate_akash_certificate(address)?;
        tracing::info!("    Serial: {}", generated.serial);

        // Encrypt the private key for secure storage
        tracing::info!("  Encrypting private key...");
        let encrypted_private_key = encrypt_private_key(&generated.privkey_pem, encryption_password)?;
        tracing::info!("    Encrypted key size: {} bytes", encrypted_private_key.len());

        // Build MsgCreateCertificate
        let msg = MsgCreateCertificate {
            owner: address.to_string(),
            cert: generated.cert_pem.clone(),
            pubkey: generated.pubkey_pem.clone(),
        };

        // Create layer-climb signing client
        let chain_config = chain_config_from_akash(&self.akash_config)?;
        let client = create_signing_client(
            self.key_manager.clone(),
            self.key_store.clone(),
            key_name,
            account_index,
            chain_config,
        )
        .await?;

        // Convert our prost message to layer-climb's Any
        let msg_any = ClimbAny {
            type_url: MsgCreateCertificate::type_url(),
            value: msg.encode_to_vec(),
        };

        // Broadcast with layer-climb
        tracing::info!("  Broadcasting MsgCreateCertificate...");
        let mut tx_builder = client.tx_builder();
        tx_builder.set_memo("ergors certificate creation");
        let tx_resp = tx_builder.broadcast(vec![msg_any]).await?;

        if tx_resp.code != 0 {
            tracing::error!("  FAILED: Certificate tx rejected (code {})", tx_resp.code);
            tracing::error!("  Error: {}", tx_resp.raw_log);
            return Err(anyhow!(
                "Certificate creation failed (code {}): {}",
                tx_resp.code,
                tx_resp.raw_log
            ));
        }

        tracing::info!("  Certificate: CREATED NEW");
        tracing::info!("    Tx Hash: {}", tx_resp.txhash);
        tracing::info!("    Height:  {}", tx_resp.height);
        tracing::info!("    Serial:  {}", generated.serial);

        Ok(CertificateWithKey {
            certificate: Certificate {
                state: State::Valid as i32,
                cert: generated.cert_pem,
                pubkey: generated.pubkey_pem,
            },
            encrypted_private_key,
        })
    }

    /// Query certificate from chain by address.
    /// Returns the official akash.cert.v1.Certificate if found.
    pub async fn query_certificate(&self, address: &str) -> Result<Option<Certificate>> {
        self.cosmos.query_valid_certificate(address).await
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

// NOTE: Removed certificate_to_workflow_info - now using official akash.cert.v1.Certificate directly

/// Generated certificate with all components for mTLS.
pub struct GeneratedCertificate {
    pub cert_pem: Vec<u8>,
    pub pubkey_pem: Vec<u8>,
    pub privkey_pem: Vec<u8>,
    pub serial: String,
}

/// Generate an Akash-compatible X.509 certificate in PEM format.
///
/// This creates a self-signed ECDSA P-256 certificate for mTLS authentication
/// between tenants and providers on Akash Network.
///
/// Returns GeneratedCertificate with cert, pubkey, privkey (all PEM), and serial.
pub fn generate_akash_certificate(address: &str) -> Result<GeneratedCertificate> {
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

    // Get public key in SEC1 EC format (Akash requirement)
    // rcgen produces SPKI format ("BEGIN PUBLIC KEY"), but Akash expects
    // SEC1 format ("BEGIN EC PUBLIC KEY") - same DER bytes, different PEM label
    let pubkey_bytes = convert_to_ec_public_key_pem(&key_pair)?;

    // Get private key PEM (for mTLS)
    let privkey_pem = key_pair.serialize_pem();
    let privkey_bytes = privkey_pem.as_bytes().to_vec();

    // Generate the self-signed certificate
    let cert = params
        .self_signed(&key_pair)
        .map_err(|e| anyhow!("Failed to generate certificate: {}", e))?;

    // Get PEM-encoded certificate
    let cert_pem = cert.pem();
    let cert_bytes = cert_pem.as_bytes().to_vec();

    tracing::debug!("Generated certificate PEM ({} bytes)", cert_bytes.len());
    tracing::debug!("Generated public key PEM ({} bytes)", pubkey_bytes.len());
    tracing::debug!("Generated private key PEM ({} bytes)", privkey_bytes.len());

    Ok(GeneratedCertificate {
        cert_pem: cert_bytes,
        pubkey_pem: pubkey_bytes,
        privkey_pem: privkey_bytes,
        serial,
    })
}

/// Convert SPKI public key to "EC PUBLIC KEY" PEM format for Akash.
///
/// rcgen produces `-----BEGIN PUBLIC KEY-----` (SPKI format)
/// Akash expects `-----BEGIN EC PUBLIC KEY-----` (same DER, different label)
fn convert_to_ec_public_key_pem(key_pair: &KeyPair) -> Result<Vec<u8>> {
    let spki_der = key_pair.public_key_der();

    // Encode as PEM with "EC PUBLIC KEY" header
    let b64 = BASE64.encode(&spki_der);
    let mut pem = String::with_capacity(b64.len() + 60);
    pem.push_str("-----BEGIN EC PUBLIC KEY-----\n");

    // Wrap at 64 characters per line (PEM standard)
    for chunk in b64.as_bytes().chunks(64) {
        pem.push_str(std::str::from_utf8(chunk).unwrap());
        pem.push('\n');
    }
    pem.push_str("-----END EC PUBLIC KEY-----\n");

    Ok(pem.into_bytes())
}

// ============ Private Key Encryption ============

use chacha20poly1305::{
    aead::{Aead, NewAead},
    ChaCha20Poly1305, Key, Nonce,
};
use argon2::Argon2;

const ENCRYPTION_KEY_SIZE: usize = 32;
const NONCE_SIZE: usize = 12;
const SALT_SIZE: usize = 32;

/// Encrypt private key using ChaCha20Poly1305 with Argon2id key derivation.
/// Format: salt (32 bytes) || nonce (12 bytes) || ciphertext
pub fn encrypt_private_key(privkey_pem: &[u8], password: &str) -> Result<Vec<u8>> {
    // Generate random salt and nonce
    let mut salt = [0u8; SALT_SIZE];
    let mut nonce_bytes = [0u8; NONCE_SIZE];
    rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut salt);
    rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut nonce_bytes);

    // Derive key from password using Argon2id
    let mut key = [0u8; ENCRYPTION_KEY_SIZE];
    let params = argon2::Params::new(1 << 16, 2, 2, Some(ENCRYPTION_KEY_SIZE))
        .map_err(|e| anyhow!("Argon2 params error: {}", e))?;
    let argon2 = Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params);
    argon2
        .hash_password_into(password.as_bytes(), &salt, &mut key)
        .map_err(|e| anyhow!("Key derivation failed: {}", e))?;

    // Encrypt
    let cipher = ChaCha20Poly1305::new(Key::from_slice(&key));
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ciphertext = cipher
        .encrypt(nonce, privkey_pem)
        .map_err(|e| anyhow!("Encryption failed: {}", e))?;

    // Combine: salt || nonce || ciphertext
    let mut result = Vec::with_capacity(SALT_SIZE + NONCE_SIZE + ciphertext.len());
    result.extend_from_slice(&salt);
    result.extend_from_slice(&nonce_bytes);
    result.extend_from_slice(&ciphertext);

    Ok(result)
}

/// Decrypt private key using ChaCha20Poly1305 with Argon2id key derivation.
/// Input format: salt (32 bytes) || nonce (12 bytes) || ciphertext
pub fn decrypt_private_key(encrypted: &[u8], password: &str) -> Result<Vec<u8>> {
    if encrypted.len() < SALT_SIZE + NONCE_SIZE + 16 {
        return Err(anyhow!("Encrypted data too short"));
    }

    // Extract components
    let salt = &encrypted[..SALT_SIZE];
    let nonce_bytes = &encrypted[SALT_SIZE..SALT_SIZE + NONCE_SIZE];
    let ciphertext = &encrypted[SALT_SIZE + NONCE_SIZE..];

    // Derive key from password using Argon2id
    let mut key = [0u8; ENCRYPTION_KEY_SIZE];
    let params = argon2::Params::new(1 << 16, 2, 2, Some(ENCRYPTION_KEY_SIZE))
        .map_err(|e| anyhow!("Argon2 params error: {}", e))?;
    let argon2 = Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params);
    argon2
        .hash_password_into(password.as_bytes(), salt, &mut key)
        .map_err(|e| anyhow!("Key derivation failed: {}", e))?;

    // Decrypt
    let cipher = ChaCha20Poly1305::new(Key::from_slice(&key));
    let nonce = Nonce::from_slice(nonce_bytes);
    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|_| anyhow!("Decryption failed - wrong password or corrupted data"))?;

    Ok(plaintext)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_akash_certificate() {
        let result = generate_akash_certificate("akash1testaddress");
        assert!(result.is_ok());

        let generated = result.unwrap();
        assert!(!generated.cert_pem.is_empty());
        assert!(!generated.pubkey_pem.is_empty());
        assert!(!generated.privkey_pem.is_empty());
        assert_eq!(generated.serial.len(), 32); // 16 bytes hex encoded
    }

    #[test]
    fn test_encrypt_decrypt_private_key() {
        let privkey = b"-----BEGIN PRIVATE KEY-----\ntest data\n-----END PRIVATE KEY-----";
        let password = "test_password_123";

        // Encrypt
        let encrypted = encrypt_private_key(privkey, password).unwrap();
        assert!(encrypted.len() > privkey.len()); // Should be larger due to salt/nonce/tag

        // Decrypt
        let decrypted = decrypt_private_key(&encrypted, password).unwrap();
        assert_eq!(decrypted, privkey);

        // Wrong password should fail
        let wrong_result = decrypt_private_key(&encrypted, "wrong_password");
        assert!(wrong_result.is_err());
    }
}
