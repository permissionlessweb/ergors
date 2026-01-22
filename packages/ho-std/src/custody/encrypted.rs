use rand_core::OsRng;
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
// use serde_with::{formats::Uppercase, hex::Hex};
#[cfg(feature = "rpc")]
use crate::custody::{soft_kms, terminal::Terminal};
#[cfg(feature = "rpc")]
use crate::types::ergors::custody::v1::{self as pb, AuthorizeResponse};
use serde_with::formats::Uppercase;
use serde_with::hex::Hex;
#[cfg(feature = "rpc")]
use tokio::sync::OnceCell;
#[cfg(feature = "rpc")]
use tonic::{async_trait, Request, Response, Status};

mod encryption {
    use anyhow::anyhow;
    use chacha20poly1305::{
        aead::{AeadInPlace, NewAead},
        ChaCha20Poly1305,
    };
    use rand_core::CryptoRngCore;

    /// Represents a password that has been validated for length, and won't cause argon2 errors
    #[derive(Clone, Copy)]
    pub struct Password<'a>(&'a str);

    impl<'a> Password<'a> {
        /// Create a new password, validating its length
        pub fn new(password: &'a str) -> anyhow::Result<Self> {
            anyhow::ensure!(password.len() < argon2::MAX_PWD_LEN, "password too long");
            Ok(Self(password))
        }
    }

    impl<'a> TryFrom<&'a str> for Password<'a> {
        type Error = anyhow::Error;

        fn try_from(value: &'a str) -> Result<Self, Self::Error> {
            Self::new(value)
        }
    }

    // These can be recomputed from the library, at the cost of importing 25 billion traits.
    const SALT_SIZE: usize = 32;
    const TAG_SIZE: usize = 16;
    const KEY_SIZE: usize = 32;

    fn derive_key(salt: &[u8; SALT_SIZE], password: Password<'_>) -> [u8; KEY_SIZE] {
        let mut key = [0u8; KEY_SIZE];
        // The only reason this function should fail is because of incorrect static parameters
        // we've chosen, since we've validated the length of the password.

        // Use lighter parameters for tests to avoid slow test runs
        #[cfg(test)]
        let params = argon2::Params::new(1 << 10, 1, 1, Some(KEY_SIZE))
            .expect("the parameters should be valid");

        // Production parameters following https://datatracker.ietf.org/doc/html/rfc9106
        #[cfg(not(test))]
        let params = argon2::Params::new(1 << 21, 1, 4, Some(KEY_SIZE))
            .expect("the parameters should be valid");

        argon2::Argon2::hash_password_into(
            &argon2::Argon2::new(
                argon2::Algorithm::Argon2id,
                argon2::Version::V0x13,
                params,
            ),
            password.0.as_bytes(),
            salt,
            &mut key,
        )
        .expect("password hashing should not fail with a small enough password");
        key
    }

    pub fn encrypt(rng: &mut impl CryptoRngCore, password: Password<'_>, data: &[u8]) -> Vec<u8> {
        // The scheme here is that we derive a new salt, used that to derive a new unique key
        // from the password, then store the salt alongside the ciphertext, and its tag.
        // The salt needs to go into the AD section, because we don't want it to be modified,
        // since we're not using a key-committing encryption scheme, and a different key may
        // successfully decrypt the ciphertext.
        let salt = {
            let mut out = [0u8; SALT_SIZE];
            rng.fill_bytes(&mut out);
            out
        };
        let key = derive_key(&salt, password);

        let mut ciphertext = Vec::with_capacity(TAG_SIZE + salt.len() + data.len());
        ciphertext.extend_from_slice(&[0u8; TAG_SIZE]);
        ciphertext.extend_from_slice(&salt);
        ciphertext.extend_from_slice(data);
        let tag = ChaCha20Poly1305::new(&key.into())
            .encrypt_in_place_detached(
                &Default::default(),
                &salt,
                &mut ciphertext[TAG_SIZE + SALT_SIZE..],
            )
            .expect("XChaCha20Poly1305 encryption should not fail");
        ciphertext[0..TAG_SIZE].copy_from_slice(&tag);
        ciphertext
    }

    pub fn decrypt(password: Password<'_>, data: &[u8]) -> anyhow::Result<Vec<u8>> {
        anyhow::ensure!(
            data.len() >= TAG_SIZE + SALT_SIZE,
            "provided ciphertext is too short"
        );
        let (header, message) = data.split_at(TAG_SIZE + SALT_SIZE);
        let mut message = message.to_owned();
        let tag = &header[..TAG_SIZE];
        let salt = &header[TAG_SIZE..TAG_SIZE + SALT_SIZE];
        let key = derive_key(
            &salt.try_into().expect("salt is the right length"),
            password,
        );
        ChaCha20Poly1305::new(&key.into())
            .decrypt_in_place_detached(&Default::default(), salt, &mut message, tag.into())
            .map_err(|_| anyhow!("failed to decrypt ciphertext"))?;
        Ok(message)
    }

    /// Encrypt API key using node's signing key (no password, direct key derivation)
    /// Uses raw 32-byte key material for flexibility with different ed25519 implementations
    pub fn encrypt_with_node_key(
        rng: &mut impl CryptoRngCore,
        node_key_bytes: &[u8; 32],
        data: &[u8],
    ) -> Vec<u8> {
        use sha2::{Digest, Sha256};

        let salt = {
            let mut out = [0u8; SALT_SIZE];
            rng.fill_bytes(&mut out);
            out
        };

        // Derive key directly from node key (no argon2)
        let key = {
            let mut hasher = Sha256::new();
            hasher.update(b"ERGORS_API_KEY_ENC_V1");
            hasher.update(node_key_bytes);
            hasher.update(salt);
            let hash = hasher.finalize();
            let mut key = [0u8; KEY_SIZE];
            key.copy_from_slice(&hash[..KEY_SIZE]);
            key
        };

        let mut ciphertext = Vec::with_capacity(TAG_SIZE + salt.len() + data.len());
        ciphertext.extend_from_slice(&[0u8; TAG_SIZE]);
        ciphertext.extend_from_slice(&salt);
        ciphertext.extend_from_slice(data);
        let tag = ChaCha20Poly1305::new(&key.into())
            .encrypt_in_place_detached(
                &Default::default(),
                &salt,
                &mut ciphertext[TAG_SIZE + SALT_SIZE..],
            )
            .expect("ChaCha20Poly1305 encryption should not fail");
        ciphertext[0..TAG_SIZE].copy_from_slice(&tag);
        ciphertext
    }

    pub fn decrypt_with_node_key(
        node_key_bytes: &[u8; 32],
        data: &[u8],
    ) -> anyhow::Result<Vec<u8>> {
        use sha2::{Digest, Sha256};

        anyhow::ensure!(
            data.len() >= TAG_SIZE + SALT_SIZE,
            "provided ciphertext is too short"
        );
        let (header, message) = data.split_at(TAG_SIZE + SALT_SIZE);
        let mut message = message.to_owned();
        let tag = &header[..TAG_SIZE];
        let salt = &header[TAG_SIZE..TAG_SIZE + SALT_SIZE];

        // Derive key using same method as encryption
        let key = {
            let mut hasher = Sha256::new();
            hasher.update(b"ERGORS_API_KEY_ENC_V1");
            hasher.update(node_key_bytes);
            hasher.update(salt);
            let hash = hasher.finalize();
            let mut key = [0u8; KEY_SIZE];
            key.copy_from_slice(&hash[..KEY_SIZE]);
            key
        };

        ChaCha20Poly1305::new(&key.into())
            .decrypt_in_place_detached(&Default::default(), salt, &mut message, tag.into())
            .map_err(|_| anyhow!("failed to decrypt ciphertext"))?;
        Ok(message)
    }

    #[cfg(test)]
    mod test {
        use rand_core::OsRng;

        use super::*;

        #[test]
        fn test_encryption_decryption_roundtrip() -> anyhow::Result<()> {
            let password = "password".try_into()?;
            let message = b"hello world";
            let encrypted = encrypt(&mut OsRng, password, message);
            let decrypted = decrypt(password, &encrypted)?;
            assert_eq!(decrypted.as_slice(), message);
            Ok(())
        }

        #[test]
        fn test_encryption_fails_with_different_password() -> anyhow::Result<()> {
            let password = "password".try_into()?;
            let message = b"hello world";
            let encrypted = encrypt(&mut OsRng, password, message);
            let decrypted = decrypt("not password".try_into()?, &encrypted);
            assert!(decrypted.is_err());
            Ok(())
        }
    }
}

pub use encryption::{decrypt, decrypt_with_node_key, encrypt, encrypt_with_node_key};

/// The actual inner configuration used for an encrypted configuration.
#[cfg(feature = "rpc")]
#[derive(Serialize, Deserialize)]
pub enum InnerConfig {
    SoftKms(soft_kms::Config),
}

#[cfg(feature = "rpc")]
impl InnerConfig {
    pub fn from_bytes(data: &[u8]) -> anyhow::Result<Self> {
        Ok(serde_json::from_slice(data)?)
    }

    pub fn to_bytes(self) -> anyhow::Result<Vec<u8>> {
        Ok(serde_json::to_vec(&self)?)
    }
}

/// The configuration for the encrypted custody backend.
///
/// This holds a blob of encrypted data that needs to be further deserialized into another config.
#[cfg(feature = "rpc")]
#[serde_as]
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Config {
    #[serde_as(as = "Hex<Uppercase>")]
    data: Vec<u8>,
}

#[cfg(feature = "rpc")]
impl Config {
    /// Create a config from an inner config, with the actual params, and an encryption password.
    pub fn create(password: &str, inner: InnerConfig) -> anyhow::Result<Self> {
        let password = password.try_into()?;
        Ok(Self {
            data: encrypt(&mut OsRng, password, &inner.to_bytes()?),
        })
    }

    fn decrypt(self, password: &str) -> anyhow::Result<InnerConfig> {
        let decrypted_data = decrypt(password.try_into()?, &self.data)?;
        InnerConfig::from_bytes(&decrypted_data)
    }
}

/// Represents a custody service that uses an encrypted configuration.
///
/// This service wraps either the threshold or solo custody service.
#[cfg(feature = "rpc")]
pub struct Encrypted<T> {
    config: Config,
    terminal: T,
    inner: OnceCell<anyhow::Result<Box<dyn pb::custody_service_server::CustodyService>>>,
}

#[cfg(feature = "rpc")]
impl<T: Terminal + Clone + Send + Sync + 'static> Encrypted<T> {
    /// Create a new encrypted config, using the terminal to ask for a password
    pub fn new(config: Config, terminal: T) -> Self {
        Self {
            config,
            terminal,
            inner: Default::default(),
        }
    }

    async fn get_inner(&self) -> Result<&dyn pb::custody_service_server::CustodyService, Status> {
        Ok(self
            .inner
            .get_or_init(|| async {
                let password = self.terminal.get_password().await?;

                let inner = self.config.clone().decrypt(&password)?;
                let out: Box<dyn pb::custody_service_server::CustodyService> = match inner {
                    InnerConfig::SoftKms(c) => Box::new(soft_kms::SoftKms::new(c)),
                };
                Ok(out)
            })
            .await
            .as_ref()
            .map_err(|e| Status::unauthenticated(format!("failed to initialize custody {e}")))?
            .as_ref())
    }
}

#[cfg(feature = "rpc")]
#[async_trait]
impl<T: Terminal + Clone + Send + Sync + 'static> pb::custody_service_server::CustodyService
    for Encrypted<T>
{
    async fn authorize(
        &self,
        request: Request<pb::AuthorizeRequest>,
    ) -> Result<Response<AuthorizeResponse>, Status> {
        self.get_inner().await?.authorize(request).await
    }

    //     async fn authorize_validator_definition(
    //         &self,
    //         request: Request<pb::AuthorizeValidatorDefinitionRequest>,
    //     ) -> Result<Response<pb::AuthorizeValidatorDefinitionResponse>, Status> {
    //         self.get_inner()
    //             .await?
    //             .authorize_validator_definition(request)
    //             .await
    //     }

    async fn decrypt_api_key(
        &self,
        r: Request<pb::DecryptApiKeyRequest>,
    ) -> Result<Response<pb::DecryptApiKeyResponse>, Status> {
        self.get_inner().await?.decrypt_api_key(r).await
    }

    async fn export_full_viewing_key(
        &self,
        request: Request<pb::ExportFullViewingKeyRequest>,
    ) -> Result<Response<pb::ExportFullViewingKeyResponse>, Status> {
        self.get_inner()
            .await?
            .export_full_viewing_key(request)
            .await
    }

    //     async fn confirm_address(
    //         &self,
    //         request: Request<pb::ConfirmAddressRequest>,
    //     ) -> Result<Response<pb::ConfirmAddressResponse>, Status> {
    //         self.get_inner().await?.confirm_address(request).await
    //     }
}
