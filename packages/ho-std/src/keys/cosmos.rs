//! Cosmos-SDK Compatible Key Management
//!
//! Implements BIP-39 mnemonic generation and BIP-32 HD key derivation
//! for cosmos-sdk chains (Akash, Cosmos Hub, etc.).
//!
//! Uses the standard cosmos derivation path: m/44'/118'/0'/0/{account_index}
//!
//! ## Key Types
//! - Mnemonic: 24-word BIP-39 seed phrase
//! - Private Key: secp256k1 private key derived via BIP-32
//! - Public Key: compressed secp256k1 public key (33 bytes)
//! - Address: bech32-encoded RIPEMD160(SHA256(pubkey))

use anyhow::{anyhow, Result};
use bip32::{DerivationPath, ExtendedPrivateKey, PrivateKey, PublicKey};
use bip39::{Language, Mnemonic};
use sha2::{Digest, Sha256};
use std::str::FromStr;
use zeroize::ZeroizeOnDrop;

/// Standard cosmos derivation path prefix: m/44'/118'/0'/0/
pub const COSMOS_HD_PATH_PREFIX: &str = "m/44'/118'/0'/0/";

/// Akash chain bech32 prefix
pub const AKASH_PREFIX: &str = "akash";

/// Cosmos Hub bech32 prefix
pub const COSMOS_PREFIX: &str = "cosmos";

/// A cosmos mnemonic with zeroization on drop
#[derive(ZeroizeOnDrop)]
pub struct CosmosMnemonic {
    /// The raw mnemonic phrase
    phrase: String,
}

impl CosmosMnemonic {
    /// Generate a new random 24-word mnemonic
    pub fn generate() -> Result<Self> {
        let mut rng = rand::thread_rng();
        let mnemonic = Mnemonic::generate_in_with(&mut rng, Language::English, 24)
            .map_err(|e| anyhow!("Failed to generate mnemonic: {}", e))?;
        Ok(Self {
            phrase: mnemonic.to_string(),
        })
    }

    /// Parse an existing mnemonic phrase
    pub fn from_phrase(phrase: &str) -> Result<Self> {
        // Validate the mnemonic
        Mnemonic::parse_in_normalized(Language::English, phrase)
            .map_err(|e| anyhow!("Invalid mnemonic phrase: {}", e))?;
        Ok(Self {
            phrase: phrase.trim().to_string(),
        })
    }

    /// Get the mnemonic phrase (use carefully - this exposes secret data)
    pub fn phrase(&self) -> &str {
        &self.phrase
    }

    /// Get the word count
    pub fn word_count(&self) -> usize {
        self.phrase.split_whitespace().count()
    }

    /// Derive a keypair at the given account index using default coin type (118)
    pub fn derive_keypair(&self, account_index: u32) -> Result<CosmosKeyPair> {
        self.derive_keypair_with_coin_type(account_index, 118)
    }

    /// Derive a keypair with custom coin type
    ///
    /// Common coin types:
    /// - 118: Cosmos/Akash (default)
    /// - 330: Terra
    /// - 60: Ethereum (for EVM chains)
    /// - 529: Secret Network
    pub fn derive_keypair_with_coin_type(
        &self,
        account_index: u32,
        coin_type: u32,
    ) -> Result<CosmosKeyPair> {
        let path_str = format!("m/44'/{}'/{}'/{}/{}", coin_type, 0, 0, account_index);
        let path = DerivationPath::from_str(&path_str)
            .map_err(|e| anyhow!("Invalid derivation path: {}", e))?;

        // Parse mnemonic and derive seed
        let mnemonic = Mnemonic::parse_in_normalized(Language::English, &self.phrase)
            .map_err(|e| anyhow!("Invalid mnemonic: {}", e))?;

        // Derive extended private key using BIP-32 (no passphrase for cosmos-sdk compatibility)
        let seed = mnemonic.to_seed("");
        let xpriv = ExtendedPrivateKey::<bip32::secp256k1::SecretKey>::derive_from_path(seed, &path)
            .map_err(|e| anyhow!("Key derivation failed: {}", e))?;

        // Extract the private key bytes
        let private_key_bytes = xpriv.private_key().to_bytes();

        // Get public key (compressed, 33 bytes)
        let public_key = xpriv.public_key();
        let public_key_bytes = public_key.to_bytes();

        Ok(CosmosKeyPair {
            private_key: private_key_bytes.to_vec(),
            public_key: public_key_bytes.to_vec(),
            hd_path: path_str,
            account_index,
        })
    }
}

/// A cosmos keypair with private key that zeroizes on drop
#[derive(ZeroizeOnDrop)]
pub struct CosmosKeyPair {
    #[zeroize(skip)]
    private_key: Vec<u8>,
    #[zeroize(skip)]
    public_key: Vec<u8>,
    hd_path: String,
    account_index: u32,
}

impl CosmosKeyPair {
    /// Get the private key bytes (use carefully)
    pub fn private_key(&self) -> &[u8] {
        &self.private_key
    }

    /// Get the compressed public key bytes (33 bytes)
    pub fn public_key(&self) -> &[u8] {
        &self.public_key
    }

    /// Get the HD derivation path
    pub fn hd_path(&self) -> &str {
        &self.hd_path
    }

    /// Get the account index
    pub fn account_index(&self) -> u32 {
        self.account_index
    }

    /// Generate the bech32 address with the given prefix
    pub fn address(&self, prefix: &str) -> Result<String> {
        cosmos_address_from_pubkey(&self.public_key, prefix)
    }

    /// Generate an Akash address (akash1...)
    pub fn akash_address(&self) -> Result<String> {
        self.address(AKASH_PREFIX)
    }

    /// Generate a Cosmos Hub address (cosmos1...)
    pub fn cosmos_address(&self) -> Result<String> {
        self.address(COSMOS_PREFIX)
    }

    /// Sign arbitrary message bytes with secp256k1.
    ///
    /// Returns the 64-byte compact signature (r || s).
    /// Used for JWT authentication with Akash providers.
    pub fn sign(&self, message: &[u8]) -> Result<Vec<u8>> {
        use k256::ecdsa::{signature::Signer, Signature, SigningKey};

        let signing_key = SigningKey::from_bytes((&self.private_key[..]).into())
            .map_err(|e| anyhow!("Invalid private key: {}", e))?;

        let signature: Signature = signing_key.sign(message);
        Ok(signature.to_bytes().to_vec())
    }

    /// Sign for ES256K JWT (single SHA256, then ECDSA).
    ///
    /// ES256K as defined in RFC 8812 uses standard ECDSA with secp256k1 curve
    /// and SINGLE SHA-256 hashing (NOT Bitcoin's double-SHA256).
    ///
    /// This is the correct implementation for Akash provider JWT authentication.
    ///
    /// Returns the 64-byte compact signature (r || s).
    pub fn sign_jwt_es256k(&self, message: &[u8]) -> Result<Vec<u8>> {
        use k256::ecdsa::{signature::hazmat::PrehashSigner, Signature, SigningKey};
        use sha2::{Digest, Sha256};

        let signing_key = SigningKey::from_bytes((&self.private_key[..]).into())
            .map_err(|e| anyhow!("Invalid private key: {}", e))?;

        // Single SHA256 (RFC 8812 ES256K standard for JWT)
        // NOT Bitcoin's double-SHA256!
        let hash = Sha256::digest(message);

        // Sign the hashed message
        let signature: Signature = signing_key
            .sign_prehash(&hash)
            .map_err(|e| anyhow!("Signing failed: {}", e))?;

        // Return compact 64-byte signature (r || s)
        Ok(signature.to_bytes().to_vec())
    }

    /// Sign a message and return base64-encoded signature.
    ///
    /// This is the format expected by Akash provider JWT auth.
    pub fn sign_base64(&self, message: &[u8]) -> Result<String> {
        use base64::{engine::general_purpose::STANDARD, Engine};
        let sig = self.sign(message)?;
        Ok(STANDARD.encode(sig))
    }
}

/// Generate a cosmos bech32 address from a compressed public key
///
/// Address = bech32(prefix, RIPEMD160(SHA256(pubkey)))
pub fn cosmos_address_from_pubkey(pubkey: &[u8], prefix: &str) -> Result<String> {
    if pubkey.len() != 33 {
        return Err(anyhow!(
            "Public key must be 33 bytes (compressed), got {}",
            pubkey.len()
        ));
    }

    // SHA256 hash of public key
    let sha_hash = Sha256::digest(pubkey);

    // RIPEMD160 of SHA256 hash
    use ripemd::Ripemd160;
    let addr_bytes = Ripemd160::digest(sha_hash);

    // Bech32 encode (use Bech32 variant, not Bech32m, for cosmos addresses)
    let encoded = bech32::encode(
        prefix,
        bech32::ToBase32::to_base32(&addr_bytes.to_vec()),
        bech32::Variant::Bech32,
    )
    .map_err(|e| anyhow!("Bech32 encoding failed: {}", e))?;

    Ok(encoded)
}

/// A cosmos account with address and public key (non-sensitive info)
#[derive(Debug, Clone)]
pub struct CosmosAccountInfo {
    pub key_name: String,
    pub address: String,
    pub public_key: Vec<u8>,
    pub hd_path: String,
    pub account_index: u32,
}

impl CosmosAccountInfo {
    /// Create from a keypair and key name
    pub fn from_keypair(keypair: &CosmosKeyPair, key_name: &str, prefix: &str) -> Result<Self> {
        Ok(Self {
            key_name: key_name.to_string(),
            address: keypair.address(prefix)?,
            public_key: keypair.public_key().to_vec(),
            hd_path: keypair.hd_path().to_string(),
            account_index: keypair.account_index(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_mnemonic() {
        let mnemonic = CosmosMnemonic::generate().unwrap();
        assert_eq!(mnemonic.word_count(), 24);
    }

    #[test]
    fn test_parse_mnemonic() {
        // Standard test mnemonic (DO NOT USE IN PRODUCTION)
        let phrase = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon art";
        let mnemonic = CosmosMnemonic::from_phrase(phrase).unwrap();
        assert_eq!(mnemonic.word_count(), 24);
    }

    #[test]
    fn test_derive_keypair() {
        let phrase = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon art";
        let mnemonic = CosmosMnemonic::from_phrase(phrase).unwrap();

        let keypair = mnemonic.derive_keypair(0).unwrap();
        assert_eq!(keypair.public_key().len(), 33); // Compressed pubkey

        let address = keypair.akash_address().unwrap();
        assert!(address.starts_with("akash1"));

        let cosmos_addr = keypair.cosmos_address().unwrap();
        assert!(cosmos_addr.starts_with("cosmos1"));
    }

    #[test]
    fn test_deterministic_derivation() {
        let phrase = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon art";
        let mnemonic1 = CosmosMnemonic::from_phrase(phrase).unwrap();
        let mnemonic2 = CosmosMnemonic::from_phrase(phrase).unwrap();

        let keypair1 = mnemonic1.derive_keypair(0).unwrap();
        let keypair2 = mnemonic2.derive_keypair(0).unwrap();

        // Same mnemonic should produce same keys
        assert_eq!(keypair1.public_key(), keypair2.public_key());
        assert_eq!(keypair1.akash_address().unwrap(), keypair2.akash_address().unwrap());
    }

    #[test]
    fn test_different_account_indices() {
        let phrase = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon art";
        let mnemonic = CosmosMnemonic::from_phrase(phrase).unwrap();

        let keypair0 = mnemonic.derive_keypair(0).unwrap();
        let keypair1 = mnemonic.derive_keypair(1).unwrap();

        // Different indices should produce different keys
        assert_ne!(keypair0.public_key(), keypair1.public_key());
        assert_ne!(keypair0.akash_address().unwrap(), keypair1.akash_address().unwrap());
    }
}
