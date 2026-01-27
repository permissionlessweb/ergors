//! Custody and Key Management Tests
//!
//! Tests for the custody system including:
//! - Key generation and serialization
//! - Ed25519 signing and verification
//! - Password-based encryption (Argon2 + ChaCha20Poly1305)
//! - Identity storage CRUD operations
//! - API key encryption with node key
//! - Request authentication validation

// Shared test imports - only compiled during test builds
use camino::Utf8PathBuf;
use ho_std::custody::encrypted::{decrypt, decrypt_with_node_key, encrypt, encrypt_with_node_key};
use ho_std::custody::node_identity::{PasswordEncryptedCustody, PlaintextCustody};
use ho_std::keys::commonware::{NodePrivKey, NodePubkey};
use ho_std::storage::identity::{EncryptedIdentityBuilder, IdentityStorage};
use ho_std::traits::NodeIdentityCustody;
use rand::rngs::OsRng;
use tempfile::TempDir;

// =============================================================================
// KEY GENERATION TESTS
// =============================================================================

#[cfg(test)]
mod key_generation {
    use super::*;

    #[test]
    fn test_generate_random_keypair() {
        let key1 = NodePrivKey::new(&mut OsRng);
        let key2 = NodePrivKey::new(&mut OsRng);

        // Two random keys should be different
        assert_ne!(key1.clone().into_bytes(), key2.clone().into_bytes());
        assert_ne!(key1.id().0.to_vec(), key2.id().0.to_vec());
    }

    #[test]
    fn test_deterministic_keypair_from_seed() {
        let key1 = NodePrivKey::from_seed(42);
        let key2 = NodePrivKey::from_seed(42);
        let key3 = NodePrivKey::from_seed(43);

        // Same seed = same key
        assert_eq!(key1.clone().into_bytes(), key2.clone().into_bytes());

        // Different seed = different key
        assert_ne!(key1.clone().into_bytes(), key3.clone().into_bytes());
    }

    #[test]
    fn test_key_serialization_roundtrip() {
        let original = NodePrivKey::new(&mut OsRng);
        let bytes = original.clone().into_bytes();

        // Must be 32 bytes
        assert_eq!(bytes.len(), 32);

        // Roundtrip
        let restored = NodePrivKey::from_bytes(&bytes).expect("valid key bytes");
        assert_eq!(original.id().0.to_vec(), restored.id().0.to_vec());
    }

    #[test]
    fn test_key_hex_roundtrip() {
        let original = NodePrivKey::new(&mut OsRng);
        let hex_str = hex::encode(original.clone().into_bytes());

        let restored = NodePrivKey::from_hex(&hex_str).expect("valid hex");
        assert_eq!(original.id().0.to_vec(), restored.id().0.to_vec());
    }

    #[test]
    fn test_pubkey_from_invalid_length_fails() {
        // Wrong length should fail
        let bad_bytes = vec![0u8; 31];
        assert!(NodePubkey::from_bytes(&bad_bytes).is_none());

        let bad_bytes = vec![0u8; 33];
        assert!(NodePubkey::from_bytes(&bad_bytes).is_none());

        // Empty bytes should fail
        let bad_bytes: Vec<u8> = vec![];
        assert!(NodePubkey::from_bytes(&bad_bytes).is_none());
    }

    #[test]
    fn test_pubkey_bech32_roundtrip() {
        let key = NodePrivKey::new(&mut OsRng);
        let pubkey = key.id();

        let bech32 = pubkey.to_bech32().expect("encoding should work");
        assert!(bech32.starts_with("ergo1"));

        let decoded = NodePubkey::from_bech32(&bech32).expect("decoding should work");
        assert_eq!(pubkey.0.to_vec(), decoded.0.to_vec());
    }

    #[test]
    fn test_pubkey_bech32_rejects_wrong_prefix() {
        // Valid bech32 but wrong HRP
        assert!(NodePubkey::from_bech32("cosmos1qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqnrql8a").is_none());
    }
}

// =============================================================================
// ED25519 SIGNING TESTS
// =============================================================================

#[cfg(test)]
mod signing {
    use super::*;

    const TEST_NAMESPACE: &[u8] = b"ergors-test-namespace";

    #[test]
    fn test_sign_and_verify_with_namespace() {
        let key = NodePrivKey::new(&mut OsRng);
        let message = b"The quick brown fox jumps over the lazy dog";

        let signature = key.sign(Some(TEST_NAMESPACE), message);
        assert!(key.id().verify(Some(TEST_NAMESPACE), message, &signature));
    }

    #[test]
    fn test_sign_and_verify_without_namespace() {
        let key = NodePrivKey::new(&mut OsRng);
        let message = b"Hello, world!";

        let signature = key.sign(None, message);
        assert!(key.id().verify(None, message, &signature));
    }

    #[test]
    fn test_signature_rejects_wrong_message() {
        let key = NodePrivKey::new(&mut OsRng);
        let signature = key.sign(Some(TEST_NAMESPACE), b"correct message");

        assert!(!key
            .id()
            .verify(Some(TEST_NAMESPACE), b"wrong message", &signature));
    }

    #[test]
    fn test_signature_rejects_wrong_namespace() {
        let key = NodePrivKey::new(&mut OsRng);
        let message = b"test message";
        let signature = key.sign(Some(TEST_NAMESPACE), message);

        // Different namespace
        assert!(!key
            .id()
            .verify(Some(b"other-namespace"), message, &signature));

        // No namespace (different from empty namespace)
        assert!(!key.id().verify(None, message, &signature));

        // Empty namespace (different from no namespace)
        assert!(!key.id().verify(Some(&[]), message, &signature));
    }

    #[test]
    fn test_empty_vs_none_namespace_are_distinct() {
        let key = NodePrivKey::new(&mut OsRng);
        let message = b"same message";

        let sig_none = key.sign(None, message);
        let sig_empty = key.sign(Some(&[]), message);

        // Each verifies with its own namespace
        assert!(key.id().verify(None, message, &sig_none));
        assert!(key.id().verify(Some(&[]), message, &sig_empty));

        // But not cross-verified
        assert!(!key.id().verify(Some(&[]), message, &sig_none));
        assert!(!key.id().verify(None, message, &sig_empty));
    }

    #[test]
    fn test_signature_rejects_different_key() {
        let key1 = NodePrivKey::new(&mut OsRng);
        let key2 = NodePrivKey::new(&mut OsRng);
        let message = b"shared payload";

        let signature = key1.sign(Some(TEST_NAMESPACE), message);

        // Key1's signature does not verify with key2's public key
        assert!(!key2.id().verify(Some(TEST_NAMESPACE), message, &signature));
    }

    #[test]
    fn test_deterministic_signatures() {
        // Same seed = same key = same signature
        let key1 = NodePrivKey::from_seed(12345);
        let key2 = NodePrivKey::from_seed(12345);
        let message = b"deterministic test";

        let sig1 = key1.sign(Some(TEST_NAMESPACE), message);
        let sig2 = key2.sign(Some(TEST_NAMESPACE), message);

        assert_eq!(sig1, sig2);
    }

    #[test]
    fn test_sign_empty_message() {
        let key = NodePrivKey::new(&mut OsRng);
        let signature = key.sign(None, &[]);
        assert!(key.id().verify(None, &[], &signature));
    }

    #[test]
    fn test_sign_large_message() {
        let key = NodePrivKey::new(&mut OsRng);
        let message = vec![0xABu8; 1_000_000]; // 1MB message

        let signature = key.sign(None, &message);
        assert!(key.id().verify(None, &message, &signature));
    }
}

// =============================================================================
// PASSWORD ENCRYPTION TESTS
// =============================================================================

#[cfg(test)]
mod password_encryption {
    use super::*;

    /// Combined test for password-based encryption to reduce Argon2 overhead.
    /// Runs all cases serially within a single test.
    /// finishes in ~ 204.19s
    #[test]
    fn test_password_encryption_cases() {
        // Case 1: Basic roundtrip
        {
            let password = "correct_horse_battery_staple";
            let plaintext = b"super secret data";
            let encrypted = encrypt(
                &mut OsRng,
                password.try_into().expect("valid password"),
                plaintext,
            );
            assert!(encrypted.len() > plaintext.len(), "encrypted should be larger");
            let decrypted = decrypt(password.try_into().expect("valid password"), &encrypted)
                .expect("decryption should succeed");
            assert_eq!(decrypted, plaintext, "roundtrip failed");
        }

        // Case 2: Wrong password fails
        {
            let plaintext = b"secret";
            let encrypted = encrypt(&mut OsRng, "correct_password".try_into().unwrap(), plaintext);
            let result = decrypt("wrong_password".try_into().unwrap(), &encrypted);
            assert!(result.is_err(), "wrong password should fail");
        }

        // Case 3: Corrupted ciphertext fails
        {
            let password = "test_password";
            let plaintext = b"secret data";
            let mut encrypted = encrypt(&mut OsRng, password.try_into().unwrap(), plaintext);
            if let Some(byte) = encrypted.last_mut() {
                *byte ^= 0xFF;
            }
            let result = decrypt(password.try_into().unwrap(), &encrypted);
            assert!(result.is_err(), "corrupted ciphertext should fail");
        }

        // Case 4: Truncated ciphertext fails
        {
            let password = "test_password";
            let plaintext = b"secret data";
            let encrypted = encrypt(&mut OsRng, password.try_into().unwrap(), plaintext);
            let truncated = &encrypted[..40];

            let result = decrypt(password.try_into().unwrap(), truncated);
            assert!(result.is_err(), "truncated ciphertext should fail");
        }

        // Case 5: Empty data roundtrip
        {
            let password = "password";
            let plaintext = b"";
            let encrypted = encrypt(&mut OsRng, password.try_into().unwrap(), plaintext);
            let decrypted =
                decrypt(password.try_into().unwrap(), &encrypted).expect("should decrypt empty");
            assert_eq!(decrypted, plaintext, "empty data roundtrip failed");
        }

        // Case 6: Different encryptions produce different ciphertext (random salt)
        {
            let password = "password";
            let plaintext = b"same data";
            let encrypted1 = encrypt(&mut OsRng, password.try_into().unwrap(), plaintext);
            let encrypted2 = encrypt(&mut OsRng, password.try_into().unwrap(), plaintext);
            assert_ne!(encrypted1, encrypted2, "should have different salts");
            let decrypted1 =
                decrypt(password.try_into().unwrap(), &encrypted1).expect("decrypt 1");
            let decrypted2 =
                decrypt(password.try_into().unwrap(), &encrypted2).expect("decrypt 2");
            assert_eq!(decrypted1, decrypted2, "both should decrypt to same");
        }
    }
}

// =============================================================================
// NODE KEY ENCRYPTION TESTS (for API keys)
// =============================================================================

#[cfg(test)]
mod node_key_encryption {
    use super::*;

    /// Combined test for node-key-based encryption (API keys).
    /// Runs all cases serially to reduce overhead.
    #[test]
    fn test_node_key_encryption_cases() {
        // Generate keys once for all cases
        let node_key = NodePrivKey::new(&mut OsRng);
        let key_bytes = node_key.into_bytes();

        // Case 1: Basic roundtrip
        {
            let api_key = b"sk-ant-api03-super-secret-key";
            let encrypted = encrypt_with_node_key(&mut OsRng, &key_bytes, api_key);
            let decrypted =
                decrypt_with_node_key(&key_bytes, &encrypted).expect("decryption should work");
            assert_eq!(decrypted, api_key, "roundtrip failed");
        }

        // Case 2: Wrong key fails
        {
            let other_key = NodePrivKey::new(&mut OsRng);
            let encrypted = encrypt_with_node_key(&mut OsRng, &key_bytes, b"secret");
            let result = decrypt_with_node_key(&other_key.into_bytes(), &encrypted);
            assert!(result.is_err(), "wrong key should fail");
        }

        // Case 3: Different salts produce different ciphertext
        {
            let plaintext = b"api_key_data";
            let enc1 = encrypt_with_node_key(&mut OsRng, &key_bytes, plaintext);
            let enc2 = encrypt_with_node_key(&mut OsRng, &key_bytes, plaintext);
            assert_ne!(enc1, enc2, "should have different salts");
            assert_eq!(
                decrypt_with_node_key(&key_bytes, &enc1).unwrap(),
                plaintext
            );
            assert_eq!(
                decrypt_with_node_key(&key_bytes, &enc2).unwrap(),
                plaintext
            );
        }
    }
}

// =============================================================================
// IDENTITY STORAGE TESTS
// =============================================================================

#[cfg(test)]
mod identity_storage {
    use super::*;

    fn setup_storage() -> (TempDir, IdentityStorage) {
        let temp_dir = TempDir::new().expect("create temp dir");
        let path = Utf8PathBuf::from_path_buf(temp_dir.path().join("identity.enc"))
            .expect("valid utf8 path");
        let storage = IdentityStorage::new(&path);
        (temp_dir, storage)
    }

    /// Combined sync test for identity storage to reduce Argon2 overhead.
    #[test]
    fn test_identity_storage_sync_cases() {
        // Case 1: Create and load identity
        {
            let (_temp, storage) = setup_storage();
            let password = "test_password_123";

            assert!(!storage.exists(), "should not exist initially");

            let encrypted = storage.create_identity(password, None).expect("create");
            assert!(storage.exists(), "should exist after create");
            assert!(!encrypted.public_key.is_empty());
            assert!(!encrypted.encrypted_private_key.is_empty());

            let loaded = storage.load_encrypted().expect("load");
            assert_eq!(loaded.public_key, encrypted.public_key);

            // Also test public key access without password
            let pubkey = storage.get_public_key().expect("get pubkey");
            assert!(!pubkey.0.to_vec().is_empty());
        }

        // Case 2: Verify password (correct and wrong)
        {
            let (_temp, storage) = setup_storage();
            let password = "verify_me";

            storage.create_identity(password, None).expect("create");

            assert!(storage.verify_password(password).expect("verify"), "correct pw");
            assert!(!storage.verify_password("wrong").expect("verify"), "wrong pw");
        }

        // Case 3: Store existing key
        {
            let (_temp, storage) = setup_storage();
            let password = "import_test";

            let external_key = NodePrivKey::new(&mut OsRng);
            let expected_pubkey = external_key.id().0.to_vec();

            storage.store_identity(&external_key, password).expect("store");

            let pubkey = storage.get_public_key().expect("get pubkey");
            assert_eq!(pubkey.0.to_vec(), expected_pubkey);
        }

        // Case 4: Metadata builder (no crypto, just struct)
        {
            let metadata = EncryptedIdentityBuilder::new()
                .user("testuser")
                .host("localhost")
                .p2p_port(26969)
                .api_port(8080)
                .ssh_port(22)
                .node_type("coordinator")
                .os("linux")
                .build();

            assert_eq!(metadata.user, "testuser");
            assert_eq!(metadata.host, "localhost");
            assert_eq!(metadata.p2p_port, 26969);
            assert_eq!(metadata.api_port, 8080);
            assert!(metadata.created_at.is_some());
        }
    }

    /// Combined async test for identity storage operations requiring decryption.
    #[tokio::test]
    async fn test_identity_storage_async_cases() {
        // Case 1: Decrypt private key and verify it matches public key
        {
            let (_temp, storage) = setup_storage();
            let password = "decrypt_test";

            storage.create_identity(password, None).expect("create");

            let private_key = storage.get_private_key(password).await.expect("decrypt");
            let pubkey = storage.get_public_key().expect("get pubkey");

            assert_eq!(private_key.id().0.to_vec(), pubkey.0.to_vec(), "keys should match");
        }

        // Case 2: Wrong password rejected
        {
            let (_temp, storage) = setup_storage();
            let password = "correct";

            storage.create_identity(password, None).expect("create");

            let result = storage.get_private_key("wrong").await;
            assert!(result.is_err(), "wrong password should fail");
        }

        // Case 3: Change password
        {
            let (_temp, storage) = setup_storage();
            let old = "old_password";
            let new = "new_password";

            storage.create_identity(old, None).expect("create");
            let original_pubkey = storage.get_public_key().expect("get pubkey");

            storage.change_password(old, new).await.expect("change");

            assert!(!storage.verify_password(old).expect("verify old"), "old should fail");
            assert!(storage.verify_password(new).expect("verify new"), "new should work");

            let new_pubkey = storage.get_public_key().expect("get pubkey after change");
            assert_eq!(original_pubkey.0.to_vec(), new_pubkey.0.to_vec(), "pubkey unchanged");
        }

        // Case 4: Cache and lock behavior
        {
            let temp_dir = TempDir::new().expect("create temp dir");
            let path = Utf8PathBuf::from_path_buf(temp_dir.path().join("identity.enc"))
                .expect("valid utf8 path");

            let storage = IdentityStorage::with_cache_ttl(&path, 1);
            let password = "cache_test";

            storage.create_identity(password, None).expect("create");

            let _ = storage.get_private_key(password).await.expect("first call");
            assert!(storage.is_unlocked().await, "should be unlocked");

            storage.lock().await;
            assert!(!storage.is_unlocked().await, "should be locked");
        }

        // Case 5: Delete identity
        {
            let (_temp, storage) = setup_storage();
            let password = "delete_test";

            storage.create_identity(password, None).expect("create");
            assert!(storage.exists());

            storage.delete().expect("delete");
            assert!(!storage.exists());
        }
    }
}

// =============================================================================
// CUSTODY BACKEND TESTS
// =============================================================================

#[cfg(test)]
mod custody_backends {
    use super::*;

    /// Test plaintext custody (fast - no Argon2).
    #[tokio::test]
    async fn test_plaintext_custody() {
        let custody = PlaintextCustody::generate();

        // Always unlocked
        assert!(custody.is_unlocked());
        assert_eq!(
            custody.backend(),
            ho_std::traits::NodeIdentityCustodyBackend::Plaintext
        );

        // Sign and verify
        let message = b"test message";
        let signature = custody
            .sign_ed25519(Some(b"namespace"), message)
            .await
            .expect("sign");
        let pubkey = custody.public_key().expect("pubkey");
        assert!(pubkey.verify(Some(b"namespace"), message, &signature));
    }

    /// Combined test for password-encrypted custody to reduce Argon2 overhead.
    /// Runs all cases serially with a single identity file.
    #[tokio::test]
    async fn test_password_custody_cases() {
        let temp_dir = TempDir::new().expect("create temp dir");
        let path = Utf8PathBuf::from_path_buf(temp_dir.path().join("custody_test.enc"))
            .expect("valid path");

        let custody = PasswordEncryptedCustody::new(&path);
        let password = "test_password";

        // Case 1: Create identity
        custody.create_identity(password, None).expect("create");
        assert!(custody.exists(), "should exist after create");
        assert!(!custody.is_unlocked(), "should be locked initially");

        // Case 2: Wrong password fails
        {
            let result = custody.unlock("wrong_password").await;
            assert!(result.is_err(), "wrong password should fail");
        }

        // Case 3: Correct password unlocks
        custody.unlock(password).await.expect("unlock");
        assert!(custody.is_unlocked(), "should be unlocked");

        // Case 4: Sign while unlocked
        {
            let sig = custody
                .sign_ed25519(None, b"test")
                .await
                .expect("sign while unlocked");
            let pubkey = custody.public_key().expect("pubkey");
            assert!(pubkey.verify(None, b"test", &sig), "signature should verify");
        }

        // Case 5: Get key bytes
        {
            let key_bytes = custody.get_key_bytes().await.expect("get key bytes");
            assert_eq!(key_bytes.len(), 32);

            let restored = NodePrivKey::from_bytes(&key_bytes).expect("restore key");
            let original_pubkey = custody.public_key().expect("pubkey");
            assert_eq!(restored.id().0.to_vec(), original_pubkey.0.to_vec());
        }

        // Case 6: Lock and verify locked
        custody.lock().await;
        assert!(!custody.is_unlocked(), "should be locked");

        // Case 7: Sign fails while locked
        {
            let result = custody.sign_ed25519(None, b"test").await;
            assert!(result.is_err(), "sign should fail while locked");
        }

        // Case 8: Change password
        {
            let original_pubkey = custody.public_key().expect("pubkey");

            custody.unlock(password).await.expect("unlock for change");
            custody
                .change_password(password, "new_password")
                .await
                .expect("change");

            // Must re-unlock with new password
            custody.unlock("new_password").await.expect("unlock with new");

            let new_pubkey = custody.public_key().expect("pubkey after change");
            assert_eq!(
                original_pubkey.0.to_vec(),
                new_pubkey.0.to_vec(),
                "pubkey should be unchanged"
            );
        }
    }

    /// Test importing an existing key into custody.
    #[tokio::test]
    async fn test_custody_import_key() {
        let temp_dir = TempDir::new().expect("create temp dir");
        let path =
            Utf8PathBuf::from_path_buf(temp_dir.path().join("import.enc")).expect("valid path");

        let custody = PasswordEncryptedCustody::new(&path);
        let password = "import_test";

        let external_key = NodePrivKey::new(&mut OsRng);
        let expected_pubkey = external_key.id().0.to_vec();

        custody
            .import_identity(&external_key, password, None)
            .expect("import");

        let stored_pubkey = custody.public_key().expect("pubkey");
        assert_eq!(stored_pubkey.0.to_vec(), expected_pubkey);
    }
}

// =============================================================================
// SSH KEY EXPORT TESTS
// =============================================================================

#[cfg(test)]
mod ssh_export {
    use super::*;
    use std::fs;

    #[tokio::test]
    async fn test_export_ssh_keys() {
        let temp_dir = TempDir::new().expect("create temp dir");
        let ssh_dir = temp_dir.path().join("ssh");

        let custody = PlaintextCustody::generate();
        custody.export_ssh_keys(&ssh_dir).await.expect("export");

        // Check files exist
        assert!(ssh_dir.join("id_ed25519").exists());
        assert!(ssh_dir.join("id_ed25519.pub").exists());

        // Verify public key format
        let pub_contents = fs::read_to_string(ssh_dir.join("id_ed25519.pub")).expect("read pub");
        assert!(pub_contents.starts_with("ssh-ed25519 "));
        assert!(pub_contents.contains("ergors-node"));

        // Verify private key format
        let priv_contents = fs::read_to_string(ssh_dir.join("id_ed25519")).expect("read priv");
        assert!(priv_contents.contains("-----BEGIN OPENSSH PRIVATE KEY-----"));
        assert!(priv_contents.contains("-----END OPENSSH PRIVATE KEY-----"));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn test_ssh_private_key_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let temp_dir = TempDir::new().expect("create temp dir");
        let ssh_dir = temp_dir.path().join("ssh_perms");

        let custody = PlaintextCustody::generate();
        custody.export_ssh_keys(&ssh_dir).await.expect("export");

        let metadata = fs::metadata(ssh_dir.join("id_ed25519")).expect("metadata");
        let mode = metadata.permissions().mode() & 0o777;

        // Should be 600 (owner read/write only)
        assert_eq!(mode, 0o600);
    }
}
