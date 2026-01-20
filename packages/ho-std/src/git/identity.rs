//! Git identity derived from node ED25519 keys
//!
//! Converts ERGORS node identity (ED25519 keys) to SSH format for git authentication
//! and commit signing.

use crate::error::HoResult;
use crate::keys::commonware::{NodePrivKey, NodePubkey};
use crate::llm::HoError;
use ssh_key::{
    private::{Ed25519Keypair, Ed25519PrivateKey, KeypairData},
    LineEnding, PrivateKey, PublicKey,
};
use std::path::Path;

/// Git identity derived from node identity
///
/// Provides SSH key conversion and git configuration for a node.
#[derive(Clone)]
pub struct GitIdentity {
    /// Node ID (hex-encoded public key)
    pub node_id: String,
    /// Short node ID for display (first 16 chars)
    pub short_id: String,
    /// SSH private key in OpenSSH format
    ssh_private_key: PrivateKey,
    /// SSH public key in OpenSSH format
    ssh_public_key: PublicKey,
}

impl GitIdentity {
    /// Create a minimal GitIdentity from node ID and type (for git commits without SSH)
    ///
    /// This creates an identity suitable for making commits but without SSH key support.
    /// Use `from_node_keys` if you need SSH authentication.
    pub fn minimal(node_id: &str, node_type: &str) -> Self {
        let short_id = if node_id.len() >= 16 {
            node_id.chars().take(16).collect::<String>()
        } else {
            format!("{}-{}", node_type, &node_id[..node_id.len().min(8)])
        };

        // Create a dummy key pair for the identity structure
        // These won't be used for SSH, just for the git author info
        let seed: [u8; 32] = [0u8; 32];
        let ed25519_private = Ed25519PrivateKey::from_bytes(&seed);
        let ed25519_keypair = Ed25519Keypair::from(ed25519_private);
        let keypair_data = KeypairData::Ed25519(ed25519_keypair);
        let ssh_private = PrivateKey::new(keypair_data, "ergors minimal identity").unwrap();
        let ssh_public = ssh_private.public_key().clone();

        Self {
            node_id: node_id.to_string(),
            short_id,
            ssh_private_key: ssh_private,
            ssh_public_key: ssh_public,
        }
    }

    /// Create a GitIdentity from node keys
    pub fn from_node_keys(private_key: &NodePrivKey, public_key: &NodePubkey) -> HoResult<Self> {
        let node_id = hex::encode(public_key.0.to_vec());
        let short_id = node_id.chars().take(16).collect::<String>();

        // Convert ED25519 keys to SSH format
        let (ssh_private_key, ssh_public_key) = Self::convert_to_ssh(private_key, public_key)?;

        Ok(Self {
            node_id,
            short_id,
            ssh_private_key,
            ssh_public_key,
        })
    }

    /// Convert ED25519 node keys to SSH format
    fn convert_to_ssh(
        private_key: &NodePrivKey,
        _public_key: &NodePubkey,
    ) -> HoResult<(PrivateKey, PublicKey)> {
        // Get raw bytes from the node private key
        let priv_bytes = private_key.clone().into_bytes();

        // ED25519 private key is 32 bytes, but ssh-key expects the seed (first 32 bytes)
        // The commonware key stores the full 64-byte expanded key, we need the seed
        let seed: [u8; 32] = priv_bytes[..32]
            .try_into()
            .map_err(|_| HoError::Cfg("Invalid private key length".into()))?;

        // Create ED25519 keypair from seed
        let ed25519_private = Ed25519PrivateKey::from_bytes(&seed);
        let ed25519_keypair = Ed25519Keypair::from(ed25519_private);

        // Create SSH keypair
        let keypair_data = KeypairData::Ed25519(ed25519_keypair.clone());
        let ssh_private = PrivateKey::new(keypair_data, "ergors node key")
            .map_err(|e| HoError::Cfg(format!("Failed to create SSH private key: {}", e)))?;

        // Extract public key
        let ssh_public = ssh_private.public_key().clone();

        Ok((ssh_private, ssh_public))
    }

    /// Get the SSH public key in OpenSSH authorized_keys format
    pub fn ssh_public_key_string(&self) -> String {
        self.ssh_public_key.to_openssh().unwrap_or_default()
    }

    /// Get the SSH private key in OpenSSH PEM format
    pub fn ssh_private_key_string(&self) -> HoResult<String> {
        self.ssh_private_key
            .to_openssh(LineEnding::LF)
            .map(|s| s.to_string())
            .map_err(|e| HoError::Cfg(format!("Failed to encode SSH private key: {}", e)))
    }

    /// Get the SSH fingerprint (SHA256)
    pub fn ssh_fingerprint(&self) -> String {
        self.ssh_public_key
            .fingerprint(Default::default())
            .to_string()
    }

    /// Get git author name
    pub fn git_author_name(&self) -> String {
        format!("ergors-{}", self.short_id)
    }

    /// Get git author email
    pub fn git_author_email(&self) -> String {
        format!("{}@ergors.local", self.short_id)
    }

    /// Write SSH keys to files
    ///
    /// Creates:
    /// - `{ssh_dir}/id_ed25519` - Private key
    /// - `{ssh_dir}/id_ed25519.pub` - Public key
    pub fn write_ssh_keys(&self, ssh_dir: &Path) -> HoResult<()> {
        use std::fs;
        use std::os::unix::fs::PermissionsExt;

        // Create directory if it doesn't exist
        fs::create_dir_all(ssh_dir)
            .map_err(|e| HoError::Cfg(format!("Failed to create SSH directory: {}", e)))?;

        let private_path = ssh_dir.join("id_ed25519");
        let public_path = ssh_dir.join("id_ed25519.pub");

        // Write private key with restrictive permissions
        let private_key_str = self.ssh_private_key_string()?;
        fs::write(&private_path, &private_key_str)
            .map_err(|e| HoError::Cfg(format!("Failed to write SSH private key: {}", e)))?;

        // Set permissions to 600 (owner read/write only)
        #[cfg(unix)]
        fs::set_permissions(&private_path, fs::Permissions::from_mode(0o600))
            .map_err(|e| HoError::Cfg(format!("Failed to set private key permissions: {}", e)))?;

        // Write public key
        let public_key_str = self.ssh_public_key_string();
        fs::write(&public_path, &public_key_str)
            .map_err(|e| HoError::Cfg(format!("Failed to write SSH public key: {}", e)))?;

        tracing::info!("SSH keys written to {}", ssh_dir.display());

        Ok(())
    }

    /// Configure git repository with this identity
    ///
    /// Sets user.name, user.email, and optionally gpgsign settings.
    pub fn configure_git_repo(&self, repo_path: &Path) -> HoResult<()> {
        use git2::Repository;

        let repo = Repository::open(repo_path)
            .map_err(|e| HoError::Cfg(format!("Failed to open repository: {}", e)))?;

        let mut config = repo
            .config()
            .map_err(|e| HoError::Cfg(format!("Failed to get repository config: {}", e)))?;

        config
            .set_str("user.name", &self.git_author_name())
            .map_err(|e| HoError::Cfg(format!("Failed to set user.name: {}", e)))?;

        config
            .set_str("user.email", &self.git_author_email())
            .map_err(|e| HoError::Cfg(format!("Failed to set user.email: {}", e)))?;

        tracing::debug!(
            "Configured git identity for {} as {}",
            repo_path.display(),
            self.git_author_name()
        );

        Ok(())
    }
}

impl std::fmt::Debug for GitIdentity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GitIdentity")
            .field("node_id", &self.node_id)
            .field("short_id", &self.short_id)
            .field("fingerprint", &self.ssh_fingerprint())
            .field("author_name", &self.git_author_name())
            .field("author_email", &self.git_author_email())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::rngs::OsRng;

    #[test]
    fn test_git_identity_creation() {
        let private_key = NodePrivKey::new(&mut OsRng);
        let public_key = private_key.id();

        let identity = GitIdentity::from_node_keys(&private_key, &public_key)
            .expect("Should create GitIdentity");

        assert!(!identity.node_id.is_empty());
        assert_eq!(identity.short_id.len(), 16);
        assert!(identity.ssh_public_key_string().starts_with("ssh-ed25519"));
        assert!(identity.ssh_fingerprint().starts_with("SHA256:"));
    }

    #[test]
    fn test_ssh_key_format() {
        let private_key = NodePrivKey::new(&mut OsRng);
        let public_key = private_key.id();

        let identity = GitIdentity::from_node_keys(&private_key, &public_key)
            .expect("Should create GitIdentity");

        let private_str = identity
            .ssh_private_key_string()
            .expect("Should format private key");
        assert!(private_str.contains("-----BEGIN OPENSSH PRIVATE KEY-----"));
        assert!(private_str.contains("-----END OPENSSH PRIVATE KEY-----"));

        let public_str = identity.ssh_public_key_string();
        assert!(public_str.starts_with("ssh-ed25519 "));
    }
}
