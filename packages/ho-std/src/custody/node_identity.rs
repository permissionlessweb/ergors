//! Custody backends for node identity key management.
//!
//! This module provides implementations of `NodeIdentityCustody` trait
//! for different custody backends (password-encrypted, plaintext, etc.).

use crate::custody::encrypted::{decrypt, encrypt};
use crate::error::{HoError, HoResult};
use crate::keys::commonware::{NodePrivKey, NodePubkey};
use crate::storage::identity::IdentityStorage;
use crate::traits::{NodeIdentityCustody, NodeIdentityCustodyBackend};
use crate::types::ergors::network::v1::NodeIdentity;
use crate::types::ergors::storage::v1::NodeIdentityMetadata;
use async_trait::async_trait;
use camino::Utf8PathBuf;
use commonware_cryptography::ed25519;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info};

/// Password-encrypted custody backend for node identity.
///
/// This is the recommended backend for production use. It stores the private
/// key encrypted with a password-derived key (Argon2 + ChaCha20Poly1305).
pub struct PasswordEncryptedCustody {
    /// The underlying storage layer
    storage: IdentityStorage,
    /// Password for decryption (stored temporarily after unlock)
    password: Arc<RwLock<Option<String>>>,
}

impl PasswordEncryptedCustody {
    /// Create a new password-encrypted custody backend.
    ///
    /// # Arguments
    /// * `identity_path` - Path to the encrypted identity file
    pub fn new(identity_path: impl AsRef<camino::Utf8Path>) -> Self {
        Self {
            storage: IdentityStorage::new(identity_path),
            password: Arc::new(RwLock::new(None)),
        }
    }

    /// Create custody with custom cache TTL
    pub fn with_cache_ttl(
        identity_path: impl AsRef<camino::Utf8Path>,
        cache_ttl_secs: u64,
    ) -> Self {
        Self {
            storage: IdentityStorage::with_cache_ttl(identity_path, cache_ttl_secs),
            password: Arc::new(RwLock::new(None)),
        }
    }

    /// Check if an encrypted identity exists
    pub fn exists(&self) -> bool {
        self.storage.exists()
    }

    /// Get the identity storage path
    pub fn identity_path(&self) -> &camino::Utf8Path {
        self.storage.identity_path()
    }

    /// Create a new encrypted identity
    pub fn create_identity(
        &self,
        password: &str,
        metadata: Option<NodeIdentityMetadata>,
    ) -> HoResult<()> {
        self.storage.create_identity(password, metadata)?;
        Ok(())
    }

    /// Import an existing private key into encrypted storage
    pub fn import_identity(
        &self,
        private_key: &NodePrivKey,
        password: &str,
        metadata: Option<NodeIdentityMetadata>,
    ) -> HoResult<()> {
        self.storage
            .store_identity(private_key, password, metadata)?;
        Ok(())
    }

    /// Unlock the custody with a password
    ///
    /// This caches the password for subsequent operations.
    pub async fn unlock(&self, password: &str) -> HoResult<()> {
        // Verify password is correct
        if !self.storage.verify_password(password)? {
            return Err(HoError::Cfg("Invalid password".to_string()));
        }

        // Cache the password
        let mut pw = self.password.write().await;
        *pw = Some(password.to_string());

        // Pre-cache the private key
        let _ = self.storage.get_private_key(password).await?;

        info!("🔓 Unlocked password-encrypted custody");
        Ok(())
    }

    /// Change the encryption password
    pub async fn change_password(
        &self,
        current_password: &str,
        new_password: &str,
    ) -> HoResult<()> {
        self.storage
            .change_password(current_password, new_password)
            .await?;

        // Update cached password
        let mut pw = self.password.write().await;
        *pw = Some(new_password.to_string());

        Ok(())
    }

    async fn get_password(&self) -> HoResult<String> {
        let pw = self.password.read().await;
        pw.clone().ok_or_else(|| {
            HoError::Cfg("Custody is locked - call unlock() with password first".to_string())
        })
    }
}

#[async_trait]
impl NodeIdentityCustody for PasswordEncryptedCustody {
    fn backend(&self) -> NodeIdentityCustodyBackend {
        NodeIdentityCustodyBackend::PasswordEncrypted
    }

    fn public_key(&self) -> HoResult<NodePubkey> {
        self.storage.get_public_key()
    }

    async fn get_private_key(&self) -> HoResult<NodePrivKey> {
        let password = self.get_password().await?;
        self.storage.get_private_key(&password).await
    }

    async fn sign_ed25519(
        &self,
        namespace: Option<&[u8]>,
        message: &[u8],
    ) -> HoResult<ed25519::Signature> {
        let private_key = self.get_private_key().await?;
        Ok(private_key.sign(namespace, message))
    }

    async fn export_ssh_keys(&self, ssh_dir: &Path) -> HoResult<()> {
        let private_key = self.get_private_key().await?;
        let public_key = private_key.id();

        // Create SSH directory if it doesn't exist
        fs::create_dir_all(ssh_dir)?;

        // Write private key in OpenSSH format
        let private_key_path = ssh_dir.join("id_ed25519");
        let private_key_bytes = private_key.into_bytes();
        let openssh_private = format_openssh_private_key(&private_key_bytes, &public_key);
        fs::write(&private_key_path, openssh_private)?;

        // Set permissions (Unix only)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&private_key_path, fs::Permissions::from_mode(0o600))?;
        }

        // Write public key
        let public_key_path = ssh_dir.join("id_ed25519.pub");
        let openssh_public = format_openssh_public_key(&public_key, "ergors-node");
        fs::write(&public_key_path, openssh_public)?;

        info!("📤 Exported SSH keys to: {}", ssh_dir.display());
        Ok(())
    }

    fn is_unlocked(&self) -> bool {
        // Check if we have a password cached - this is sync so we can't await
        // Use try_read to avoid blocking
        self.password
            .try_read()
            .map(|guard| guard.is_some())
            .unwrap_or(false)
    }

    async fn lock(&self) {
        let mut pw = self.password.write().await;
        *pw = None;
        self.storage.lock().await;
        info!("🔒 Locked password-encrypted custody");
    }

    async fn get_key_bytes(&self) -> HoResult<[u8; 32]> {
        let password = self.get_password().await?;
        self.storage.get_key_bytes(&password).await
    }
}

/// Plaintext custody backend (insecure, for testing only).
///
/// WARNING: This backend stores the private key in plaintext. Only use
/// for development/testing purposes.
pub struct PlaintextCustody {
    private_key: NodePrivKey,
}

impl PlaintextCustody {
    /// Create a new plaintext custody with the given private key
    pub fn new(private_key: NodePrivKey) -> Self {
        Self { private_key }
    }

    /// Generate a new random identity
    pub fn generate() -> Self {
        Self {
            private_key: NodePrivKey::new(&mut rand::rngs::OsRng),
        }
    }
}

#[async_trait]
impl NodeIdentityCustody for PlaintextCustody {
    fn backend(&self) -> NodeIdentityCustodyBackend {
        NodeIdentityCustodyBackend::Plaintext
    }

    fn public_key(&self) -> HoResult<NodePubkey> {
        Ok(self.private_key.id())
    }

    async fn get_private_key(&self) -> HoResult<NodePrivKey> {
        Ok(self.private_key.clone())
    }

    async fn sign_ed25519(
        &self,
        namespace: Option<&[u8]>,
        message: &[u8],
    ) -> HoResult<ed25519::Signature> {
        Ok(self.private_key.sign(namespace, message))
    }

    async fn export_ssh_keys(&self, ssh_dir: &Path) -> HoResult<()> {
        let public_key = self.private_key.id();

        // Create SSH directory if it doesn't exist
        fs::create_dir_all(ssh_dir)?;

        // Write private key in OpenSSH format
        let private_key_path = ssh_dir.join("id_ed25519");
        let private_key_bytes = self.private_key.clone().into_bytes();
        let openssh_private = format_openssh_private_key(&private_key_bytes, &public_key);
        fs::write(&private_key_path, openssh_private)?;

        // Set permissions (Unix only)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&private_key_path, fs::Permissions::from_mode(0o600))?;
        }

        // Write public key
        let public_key_path = ssh_dir.join("id_ed25519.pub");
        let openssh_public = format_openssh_public_key(&public_key, "ergors-node");
        fs::write(&public_key_path, openssh_public)?;

        info!("📤 Exported SSH keys to: {}", ssh_dir.display());
        Ok(())
    }

    fn is_unlocked(&self) -> bool {
        true // Always unlocked for plaintext
    }

    async fn lock(&self) {
        // No-op for plaintext
        debug!("⚠️ lock() called on PlaintextCustody - no effect");
    }

    async fn get_key_bytes(&self) -> HoResult<[u8; 32]> {
        Ok(self.private_key.clone().into_bytes())
    }
}

/// Format ed25519 private key in OpenSSH format
fn format_openssh_private_key(private_key: &[u8; 32], public_key: &NodePubkey) -> String {
    use base64::{engine::general_purpose::STANDARD, Engine};

    // OpenSSH ed25519 private key format is complex, so we use a simplified approach
    // that creates a valid key file structure
    let mut key_data = Vec::new();

    // "openssh-key-v1" magic
    key_data.extend_from_slice(b"openssh-key-v1\0");

    // Cipher name (none = unencrypted)
    key_data.extend_from_slice(&[0, 0, 0, 4]); // length
    key_data.extend_from_slice(b"none");

    // KDF name (none)
    key_data.extend_from_slice(&[0, 0, 0, 4]); // length
    key_data.extend_from_slice(b"none");

    // KDF options (empty)
    key_data.extend_from_slice(&[0, 0, 0, 0]);

    // Number of keys
    key_data.extend_from_slice(&[0, 0, 0, 1]);

    // Public key section
    let pub_key_bytes = public_key.0.to_vec();
    let mut pub_section = Vec::new();
    // Key type
    pub_section.extend_from_slice(&[0, 0, 0, 11]); // length of "ssh-ed25519"
    pub_section.extend_from_slice(b"ssh-ed25519");
    // Public key data
    pub_section.extend_from_slice(&(pub_key_bytes.len() as u32).to_be_bytes());
    pub_section.extend_from_slice(&pub_key_bytes);

    key_data.extend_from_slice(&(pub_section.len() as u32).to_be_bytes());
    key_data.extend_from_slice(&pub_section);

    // Private key section
    let mut priv_section = Vec::new();
    // Check integers (random, but same for both)
    let check_int: u32 = rand::random();
    priv_section.extend_from_slice(&check_int.to_be_bytes());
    priv_section.extend_from_slice(&check_int.to_be_bytes());
    // Key type
    priv_section.extend_from_slice(&[0, 0, 0, 11]);
    priv_section.extend_from_slice(b"ssh-ed25519");
    // Public key
    priv_section.extend_from_slice(&(pub_key_bytes.len() as u32).to_be_bytes());
    priv_section.extend_from_slice(&pub_key_bytes);
    // Private key (ed25519 private key is 64 bytes: 32 seed + 32 public)
    let mut full_private = Vec::with_capacity(64);
    full_private.extend_from_slice(private_key);
    full_private.extend_from_slice(&pub_key_bytes);
    priv_section.extend_from_slice(&(full_private.len() as u32).to_be_bytes());
    priv_section.extend_from_slice(&full_private);
    // Comment
    priv_section.extend_from_slice(&[0, 0, 0, 11]);
    priv_section.extend_from_slice(b"ergors-node");
    // Padding
    for i in 1..=(8 - (priv_section.len() % 8)) % 8 + 1 {
        if priv_section.len() % 8 != 0 || i == 1 {
            priv_section.push(i as u8);
        }
    }
    // Ensure padding is correct
    while priv_section.len() % 8 != 0 {
        priv_section.push((priv_section.len() % 8 + 1) as u8);
    }

    key_data.extend_from_slice(&(priv_section.len() as u32).to_be_bytes());
    key_data.extend_from_slice(&priv_section);

    let encoded = STANDARD.encode(&key_data);
    let mut result = String::from("-----BEGIN OPENSSH PRIVATE KEY-----\n");
    for chunk in encoded.as_bytes().chunks(70) {
        result.push_str(&String::from_utf8_lossy(chunk));
        result.push('\n');
    }
    result.push_str("-----END OPENSSH PRIVATE KEY-----\n");
    result
}

/// Format ed25519 public key in OpenSSH format
fn format_openssh_public_key(public_key: &NodePubkey, comment: &str) -> String {
    use base64::{engine::general_purpose::STANDARD, Engine};

    let pub_key_bytes = public_key.0.to_vec();
    let mut key_blob = Vec::new();

    // Key type
    key_blob.extend_from_slice(&[0, 0, 0, 11]); // length of "ssh-ed25519"
    key_blob.extend_from_slice(b"ssh-ed25519");

    // Public key data
    key_blob.extend_from_slice(&(pub_key_bytes.len() as u32).to_be_bytes());
    key_blob.extend_from_slice(&pub_key_bytes);

    format!("ssh-ed25519 {} {}\n", STANDARD.encode(&key_blob), comment)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_password_encrypted_custody() {
        let temp_dir = TempDir::new().unwrap();
        let identity_path =
            Utf8PathBuf::from_path_buf(temp_dir.path().join("test_identity.enc")).unwrap();

        let custody = PasswordEncryptedCustody::new(&identity_path);
        let password = "test_password_123";

        // Create identity
        custody.create_identity(password, None).unwrap();
        assert!(custody.exists());

        // Should be locked initially
        assert!(!custody.is_unlocked());

        // Unlock
        custody.unlock(password).await.unwrap();
        assert!(custody.is_unlocked());

        // Get public key (doesn't require unlock)
        let pubkey = custody.public_key().unwrap();
        assert!(!pubkey.0.to_vec().is_empty());

        // Get private key (requires unlock)
        let privkey = custody.get_private_key().await.unwrap();
        assert_eq!(privkey.id().0.to_vec(), pubkey.0.to_vec());

        // Sign message
        let sig = custody.sign_ed25519(Some(b"test"), b"hello").await.unwrap();
        assert!(pubkey.verify(Some(b"test"), b"hello", &sig));

        // Lock
        custody.lock().await;
        assert!(!custody.is_unlocked());
    }

    #[tokio::test]
    async fn test_plaintext_custody() {
        let custody = PlaintextCustody::generate();

        // Always unlocked
        assert!(custody.is_unlocked());

        // Get keys
        let pubkey = custody.public_key().unwrap();
        let privkey = custody.get_private_key().await.unwrap();
        assert_eq!(privkey.id().0.to_vec(), pubkey.0.to_vec());

        // Sign and verify
        let message = b"test message";
        let sig = custody.sign_ed25519(None, message).await.unwrap();
        assert!(pubkey.verify(None, message, &sig));
    }

    #[tokio::test]
    async fn test_ssh_key_export() {
        let temp_dir = TempDir::new().unwrap();
        let ssh_dir = temp_dir.path().join("ssh");

        let custody = PlaintextCustody::generate();
        custody.export_ssh_keys(&ssh_dir).await.unwrap();

        assert!(ssh_dir.join("id_ed25519").exists());
        assert!(ssh_dir.join("id_ed25519.pub").exists());

        // Verify public key format
        let pub_contents = fs::read_to_string(ssh_dir.join("id_ed25519.pub")).unwrap();
        assert!(pub_contents.starts_with("ssh-ed25519 "));
    }
}
