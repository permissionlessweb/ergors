//! Ed25519 Authentication Interceptor for gRPC
//!
//! Provides authentication middleware for the ManagementService gRPC server.
//! Remote (non-loopback) connections must present a valid Ed25519 signature
//! over the current timestamp. Local connections bypass auth entirely.

use std::collections::HashSet;
use std::sync::Arc;
use tonic::{Request, Status};

use commonware_codec::{DecodeExt, Encode};
use commonware_cryptography::{
    blake3::Blake3,
    ed25519::{PublicKey, Signature},
    Hasher, Verifier,
};

/// Thread-safe set of authorized Ed25519 public key hex strings.
/// Uses std::sync::RwLock (not tokio) so it works in sync tonic interceptors.
#[derive(Debug, Clone)]
pub struct AuthorizedCliKeys {
    keys: Arc<std::sync::RwLock<HashSet<String>>>,
}

impl AuthorizedCliKeys {
    pub fn new() -> Self {
        Self {
            keys: Arc::new(std::sync::RwLock::new(HashSet::new())),
        }
    }

    /// Load keys from stored entries
    pub fn load_from(
        &self,
        entries: &[ho_std::types::ergors::management::v1::CliKeyEntry],
    ) {
        let mut keys = self.keys.write().unwrap();
        keys.clear();
        for entry in entries {
            keys.insert(entry.public_key_hex.clone());
        }
    }

    pub fn add(&self, hex: &str) {
        self.keys.write().unwrap().insert(hex.to_string());
    }

    pub fn remove(&self, hex: &str) {
        self.keys.write().unwrap().remove(hex);
    }

    pub fn contains(&self, hex: &str) -> bool {
        self.keys.read().unwrap().contains(hex)
    }
}

/// Maximum age of a signed timestamp (5 minutes)
const MAX_AGE_SECONDS: u64 = 300;

/// Create a gRPC auth interceptor that validates Ed25519 signatures on remote connections.
///
/// Loopback connections (127.0.0.1, ::1) are allowed without authentication.
/// Remote connections must include `x-signature`, `x-timestamp`, and `x-public-key` metadata.
pub fn create_grpc_auth_interceptor(
    authorized_keys: AuthorizedCliKeys,
) -> impl Fn(Request<()>) -> Result<Request<()>, Status> + Clone {
    move |req: Request<()>| {
        // Check if connection is from loopback — allow without auth
        if let Some(addr) = req.remote_addr() {
            if addr.ip().is_loopback() {
                return Ok(req);
            }
        }

        // Extract required metadata headers
        let metadata = req.metadata();

        let timestamp = metadata
            .get("x-timestamp")
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| Status::unauthenticated("Missing x-timestamp header"))?
            .to_string();

        let signature_hex = metadata
            .get("x-signature")
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| Status::unauthenticated("Missing x-signature header"))?
            .to_string();

        let public_key_hex = metadata
            .get("x-public-key")
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| Status::unauthenticated("Missing x-public-key header"))?
            .to_string();

        // Validate timestamp window (5 min)
        let ts: u64 = timestamp
            .parse()
            .map_err(|_| Status::unauthenticated("Invalid timestamp format"))?;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        if now.abs_diff(ts) > MAX_AGE_SECONDS {
            return Err(Status::unauthenticated("Timestamp expired or too far in future"));
        }

        // Check if key is authorized
        if !authorized_keys.contains(&public_key_hex) {
            return Err(Status::permission_denied("Public key not authorized"));
        }

        // Parse public key
        let pk_bytes = hex::decode(&public_key_hex)
            .map_err(|_| Status::unauthenticated("Invalid public key hex"))?;
        let public_key = PublicKey::decode(&*pk_bytes)
            .map_err(|_| Status::unauthenticated("Invalid Ed25519 public key"))?;

        // Parse signature
        let sig_bytes = hex::decode(&signature_hex)
            .map_err(|_| Status::unauthenticated("Invalid signature hex"))?;
        let signature = Signature::decode(&*sig_bytes)
            .map_err(|_| Status::unauthenticated("Invalid Ed25519 signature"))?;

        // Verify: sign(blake3(timestamp))
        let message = Blake3::hash(timestamp.as_bytes());
        if !public_key.verify(None, &message, &signature) {
            return Err(Status::unauthenticated("Signature verification failed"));
        }

        Ok(req)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_authorized_cli_keys() {
        let store = AuthorizedCliKeys::new();

        assert!(!store.contains("abc123"));

        store.add("abc123");
        assert!(store.contains("abc123"));

        store.remove("abc123");
        assert!(!store.contains("abc123"));
    }

    #[test]
    fn test_load_from_entries() {
        use ho_std::types::ergors::management::v1::CliKeyEntry;

        let store = AuthorizedCliKeys::new();
        let entries = vec![
            CliKeyEntry {
                public_key_hex: "key1".to_string(),
                label: "test".to_string(),
                added_at: None,
            },
            CliKeyEntry {
                public_key_hex: "key2".to_string(),
                label: "test2".to_string(),
                added_at: None,
            },
        ];

        store.load_from(&entries);
        assert!(store.contains("key1"));
        assert!(store.contains("key2"));
        assert!(!store.contains("key3"));
    }
}
