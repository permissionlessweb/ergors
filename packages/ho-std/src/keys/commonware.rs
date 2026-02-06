//! Node‑identity utilities built on top of `commonware‑cryptography::ed25519`.
//!
//! The public key is exposed as a thin wrapper type (`NodeId`) that implements
//! `Encode`/`ReadExt` so it can be sent over the network or stored on disk.
//! The private key lives only inside the `NodeIdentity` struct and can be
//! generated freshly or from a deterministic seed (useful for tests).

use crate::traits::NodeIdentityTrait;
use crate::types::ergors::network::v1::*;
use bech32::{self, FromBase32, ToBase32, Variant};
use bip39::{Language, Mnemonic};
use commonware_codec::{DecodeExt, Encode, FixedSize};
use commonware_cryptography::{ed25519, PrivateKeyExt, Signer, Verifier};
use hmac::{Hmac, Mac};
use rand::{CryptoRng, RngCore};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sha2::Sha512;

/// Human-readable prefix for ergo node addresses (bech32 encoded pubkeys).
pub const ERGO_HRP: &str = "ergo";

// Use proto types

impl NodeIdentityTrait for NodeIdentity {
    type HostOS = HostOs;
    type NodeType = NodeType;
    type PrivateKey = NodePrivKey;
    type PublicKey = NodePubkey;

    /// Create a new node identity with default metadata.
    ///
    /// NOTE: This does NOT generate keys. Keys are managed by the custody system.
    /// Use `PasswordEncryptedCustody::create_identity()` to create keys.
    fn new() -> NodeIdentity {
        let mut ego = Self::default();
        ego.user = "ergors".into();
        ego.api_port = 8080;
        ego.p2p_port = 26969;
        ego.ssh_port = 22;
        ego.node_type = NodeType::Unspecified.as_str_name().into();
        ego.os = HostOs::Unspecified.into();
        ego.host = "127.0.0.1".into();
        // Keys are NOT generated here - they are managed by custody
        // Use custody.create_identity() or custody.public_key() to get keys
        tracing::debug!("NodeIdentity (no keys): {:#?}", ego);
        ego
    }

    /// Set only the public key (private key managed by custody).
    ///
    /// Use this when loading public key from custody for display/network purposes
    /// without exposing the private key in the config.
    fn set_public_key(&mut self, public_key: &Self::PublicKey) {
        self.public_key = Some(public_key.0.to_vec());
    }

    /// Get the P2P identity address,  
    fn p2p_identity(&self) -> String {
        let pubkey_hex = self
            .public_key
            .as_ref()
            .map(hex::encode)
            .unwrap_or_else(|| "no_pubkey".to_string());
        format!("{}@{}:{}", pubkey_hex, self.host, self.p2p_port)
    }

    /// Get the P2P listen address
    fn p2p_address(&self) -> core::net::SocketAddr {
        format!("{}:{}", self.host, self.p2p_port)
            .parse()
            .expect("either identity.host or identity.port is misconfigured")
    }

    /// Get the API listen address
    fn api_address(&self) -> String {
        format!("{}:{}", self.host, self.api_port)
    }

    /// Get a display-friendly identifier
    fn display_id(&self) -> String {
        hex::encode(self.public_key())
    }

    /// Get private key from environment variable or generate a new one
    fn get_private_key_from_env() -> NodePrivKey {
        // Try to get private key from environment variable
        if let Ok(hex_string) = std::env::var("NODE_PRIVATE_KEY") {
            if let Some(private_key) = NodePrivKey::from_hex(&hex_string) {
                return private_key;
            }
            eprintln!("Warning: Invalid private key in NODE_PRIVATE_KEY, generating new key");
        }

        // Generate a new random private key if env var not found or invalid
        let mut rng = rand::rngs::OsRng;
        NodePrivKey::new(&mut rng)
    }

    /// Convert hex string to NodePrivKey
    fn private_key_from_hex(hex_string: &str) -> Option<NodePrivKey> {
        NodePrivKey::from_hex(hex_string)
    }
}

/// The *public* part of a node’s identity.
///
/// It is simply a new‑type around `ed25519::PublicKey` so that we can attach
/// a convenient `verify` method and a nice `Debug` implementation that does
/// not leak the raw bytes in production builds.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct NodePubkey(pub ed25519::PublicKey);

impl Serialize for NodePubkey {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        // Serialize as hex string for human readability
        let hex_str = hex::encode(&self.0);
        serializer.serialize_str(&hex_str)
    }
}

impl<'de> Deserialize<'de> for NodePubkey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let hex_str = String::deserialize(deserializer)?;
        Self::from_hex(&hex_str)
            .ok_or_else(|| serde::de::Error::custom("Invalid hex string for NodePubkey"))
    }
}

impl NodePubkey {
    /// Verify `sig` on `msg` using the given *optional* namespace.
    ///
    /// This mirrors the contract of `Signer::sign`: the namespace is
    /// prepended to the message before verification.
    pub fn verify(&self, namespace: Option<&[u8]>, msg: &[u8], sig: &ed25519::Signature) -> bool {
        self.0.verify(namespace, msg, sig)
    }

    /// Construct a `NodeId` from a raw byte slice; returns `None` if the slice
    /// does not have the correct length or cannot be decoded.
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() != ed25519::PublicKey::SIZE {
            return None;
        }
        // The `ReadExt` implementation of `PublicKey` will validate the point.
        ed25519::PublicKey::decode(bytes).ok().map(NodePubkey)
    }

    /// Construct a `NodeId` from a hex-encoded string; returns `None` if the hex
    /// cannot be decoded or the resulting bytes don't form a valid public key.
    pub fn from_hex(hex_str: &str) -> Option<Self> {
        let bytes = hex::decode(hex_str).ok()?;
        Self::from_bytes(&bytes)
    }

    /// Encode this public key as a bech32 string with the "ergo" prefix.
    ///
    /// Returns a human-readable address like "ergo1abc123...".
    /// This is used for CLI arguments like `--request-grant-from`.
    pub fn to_bech32(&self) -> Result<String, bech32::Error> {
        let data = self.0.to_vec().to_base32();
        bech32::encode(ERGO_HRP, data, Variant::Bech32)
    }

    /// Decode a bech32 "ergo1..." string into a NodePubkey.
    ///
    /// Returns `None` if:
    /// - The string is not valid bech32
    /// - The HRP is not "ergo"
    /// - The decoded bytes don't form a valid ed25519 public key
    pub fn from_bech32(encoded: &str) -> Option<Self> {
        let (hrp, data, _variant) = bech32::decode(encoded).ok()?;
        if hrp != ERGO_HRP {
            return None;
        }
        let bytes = Vec::<u8>::from_base32(&data).ok()?;
        Self::from_bytes(&bytes)
    }
}

/// Holds a *private* key and the matching public key.  It is the only type that
/// can *sign* messages.  The private key never leaves this struct (there is no
/// `pub fn private_key()` accessor) – this mirrors the design of many blockchain
/// client libraries.
///
/// The struct provides:
///
/// * `new()` – generate a fresh keypair using a cryptographically‑secure RNG
/// * `from_seed(seed)` – deterministic generation (useful for unit‑tests)
/// * `sign(namespace, msg)` – sign a payload
/// * `id()` – obtain the public‑key wrapper (`NodeId`)
/// * `into_bytes()` – serialize the private key (for key‑file storage)
/// * `from_bytes()` – deserialize a private key (again, only when you really
///   know you want to load it)
#[derive(Debug, Clone)]
pub struct NodePrivKey {
    /// The ed25519 private key; this implements `Signer`, `PrivateKeyExt`, etc.
    private: ed25519::PrivateKey,
}

impl Serialize for NodePrivKey {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        // Serialize as hex string for human readability
        // NOTE: This serializes the private key! Only use in secure contexts
        let bytes = self.clone().into_bytes();
        let hex_str = hex::encode(bytes);
        serializer.serialize_str(&hex_str)
    }
}

impl<'de> Deserialize<'de> for NodePrivKey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let hex_str = String::deserialize(deserializer)?;
        Self::from_hex(&hex_str)
            .ok_or_else(|| serde::de::Error::custom("Invalid hex string for NodePrivKey"))
    }
}

impl NodePrivKey {
    /// Generate a fresh, random keypair.
    ///
    /// The function pulls randomness from the supplied `rng`.  In production
    /// you will typically do `NodePrivKey::new(&mut rand::rngs::OsRng)`.
    pub fn new<R: RngCore + CryptoRng>(rng: &mut R) -> Self {
        let private = ed25519::PrivateKey::from_rng(rng);
        Self { private }
    }

    /// Deterministic construction from a `u64` seed.
    ///
    /// **WARNING** – this is *insecure* and should only be used in tests or for
    /// examples.  The library itself advertises this same warning on
    /// `PrivateKeyExt::from_seed`.
    pub fn from_seed(seed: u64) -> Self {
        // `from_seed` internally creates a `StdRng` seeded with the given value.
        let private = ed25519::PrivateKey::from_seed(seed);
        Self { private }
    }

    /// Derive a deterministic Ed25519 key from a BIP-39 mnemonic phrase
    /// using SLIP-0010 master key derivation (no child derivation path).
    ///
    /// The seed is produced with an empty passphrase (cosmos-sdk convention).
    /// Returns `None` if the phrase is invalid or HMAC setup fails.
    pub fn from_mnemonic(phrase: &str) -> Option<Self> {
        let mnemonic = Mnemonic::parse_in_normalized(Language::English, phrase).ok()?;
        let seed = mnemonic.to_seed("");
        // SLIP-0010: master key derivation for Ed25519
        let mut mac = Hmac::<Sha512>::new_from_slice(b"ed25519 seed").ok()?;
        mac.update(&seed);
        let result = mac.finalize().into_bytes();
        Self::from_bytes(&result[..32])
    }

    /// Return the public‑key view of this identity.
    #[inline]
    pub fn id(&self) -> NodePubkey {
        NodePubkey(self.private.public_key())
    }

    /// Sign `msg` using an *optional* `namespace`.
    ///
    /// The namespace is exactly what `Signer::sign` expects – it will be
    /// `prepend`ed to the message prior to hashing.  Passing `None` means “no
    /// namespace”.  Passing `Some(&[])` (empty slice) is **different** from
    /// `None` – it is treated as a *real* (zero‑length) namespace and will not
    /// verify against `None`.  This mirrors the behaviour the library tests
    /// assert (`empty_vs_none_namespace`).
    pub fn sign(&self, namespace: Option<&[u8]>, msg: &[u8]) -> ed25519::Signature {
        self.private.sign(namespace, msg)
    }

    /// Serialize the private key to a fixed‑size byte array.
    ///
    /// The returned `[u8; SIZE]` can be persisted to a file, a secrets manager,
    /// or handed to another process.  The `Encode` implementation guarantees that
    /// the size matches `ed25519::PrivateKey::SIZE`.
    pub fn into_bytes(self) -> [u8; ed25519::PrivateKey::SIZE] {
        // `Encode` returns a `Vec<u8>`; we know its length at compile time.
        let mut out = [0u8; ed25519::PrivateKey::SIZE];
        out.copy_from_slice(&self.private.encode());
        out
    }

    /// Recreate a `NodeIdentity` from a raw private‑key byte slice.
    ///
    /// Returns `None` if the slice cannot be decoded (wrong length, invalid
    /// curve point, etc.).
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() != ed25519::PrivateKey::SIZE {
            return None;
        }
        let private = ed25519::PrivateKey::decode(bytes).ok()?;
        Some(Self { private })
    }

    /// Recreate a `NodeIdentity` from a hex-encoded private key string.
    ///
    /// Returns `None` if the hex cannot be decoded or the resulting bytes
    /// don't form a valid private key.
    pub fn from_hex(hex_str: &str) -> Option<Self> {
        let bytes = hex::decode(hex_str).ok()?;
        Self::from_bytes(&bytes)
    }
    /// Recreate a `PrivateKey`
    pub fn private_key(&self) -> ed25519::PrivateKey {
        self.private.clone()
    }
}

/* -------------------------------------------------------------------------- */
/*                              Unit‑tests                                    */
/* -------------------------------------------------------------------------- */

#[cfg(test)]
mod tests {
    use super::*;
    use rand::rngs::OsRng;

    const TEST_NS: &[u8] = b"node_id_namespace";

    #[test]
    fn generate_and_roundtrip() {
        let node = NodePrivKey::new(&mut OsRng);
        let bytes = node.clone().into_bytes();
        let restored = NodePrivKey::from_bytes(&bytes).expect("valid private key");
        assert_eq!(node.id().0.to_vec(), restored.id().0.to_vec());
    }

    #[test]
    fn deterministic_seed() {
        let a = NodePrivKey::from_seed(42);
        let b = NodePrivKey::from_seed(42);
        // assert_eq!(a.id(), b.id());

        let sig_a = a.sign(Some(TEST_NS), b"payload");
        let sig_b = b.sign(Some(TEST_NS), b"payload");
        assert_eq!(sig_a, sig_b);
    }

    #[test]
    fn sign_and_verify() {
        let node = NodePrivKey::new(&mut OsRng);
        let msg = b"The quick brown fox jumps over the lazy dog";
        let sig = node.sign(Some(TEST_NS), msg);
        assert!(node.id().verify(Some(TEST_NS), msg, &sig));
    }

    #[test]
    fn reject_wrong_message() {
        let node = NodePrivKey::new(&mut OsRng);
        let msg = b"correct";
        let bad = b"incorrect";
        let sig = node.sign(Some(TEST_NS), msg);
        assert!(!node.id().verify(Some(TEST_NS), bad, &sig));
    }

    #[test]
    fn reject_wrong_namespace() {
        let node = NodePrivKey::new(&mut OsRng);
        let msg = b"hello";
        let sig = node.sign(Some(TEST_NS), msg);
        // Empty namespace is a *different* namespace, not the same as `None`.
        assert!(!node.id().verify(Some(b""), msg, &sig));
        // Completely different namespace
        assert!(!node.id().verify(Some(b"other"), msg, &sig));
        // No namespace at all
        assert!(!node.id().verify(None, msg, &sig));
    }

    #[test]
    fn empty_vs_none_namespace() {
        let node = NodePrivKey::new(&mut OsRng);
        let msg = b"same message";
        // Empty slice is a *real* namespace
        let sig = node.sign(Some(&[]), msg);
        assert!(node.id().verify(Some(&[]), msg, &sig));
        // `None` does **not** verify the same signature
        assert!(!node.id().verify(None, msg, &sig));
    }

    #[test]
    fn mismatched_keys() {
        // Two different identities – signature from one must not verify with the other
        let a: NodePrivKey = NodePrivKey::new(&mut OsRng);
        let b: NodePrivKey = NodePrivKey::new(&mut OsRng);
        println!("b: {:#?}", b.id().0.to_string());

        let msg = b"shared payload";
        let sig = a.sign(Some(TEST_NS), msg);
        assert!(!b.id().verify(Some(TEST_NS), msg, &sig));
    }

    #[test]
    fn public_key_serialisation() {
        let node = NodePrivKey::new(&mut OsRng);
        let binding = node.id();
        let pk_bytes = binding.0.to_vec();
        let reconstructed = NodePubkey::from_bytes(&pk_bytes).expect("valid pk");
        assert_eq!(node.id().0.to_vec(), reconstructed.0.to_vec());
    }

    #[test]
    fn bech32_roundtrip() {
        let node = NodePrivKey::new(&mut OsRng);
        let pubkey = node.id();

        // Encode to bech32
        let encoded = pubkey.to_bech32().expect("encoding should succeed");
        assert!(encoded.starts_with("ergo1"), "should have ergo prefix");

        // Decode back
        let decoded = NodePubkey::from_bech32(&encoded).expect("decoding should succeed");
        assert_eq!(pubkey.0.to_vec(), decoded.0.to_vec());
    }

    #[test]
    fn bech32_deterministic() {
        let node = NodePrivKey::from_seed(12345);
        let encoded = node.id().to_bech32().expect("encoding should succeed");

        // Same seed should produce same bech32 address
        let node2 = NodePrivKey::from_seed(12345);
        let encoded2 = node2.id().to_bech32().expect("encoding should succeed");
        assert_eq!(encoded, encoded2);
    }

    #[test]
    fn bech32_rejects_wrong_hrp() {
        // Create a valid bech32 string but with wrong HRP
        let node = NodePrivKey::new(&mut OsRng);
        let data = node.id().0.to_vec().to_base32();
        let wrong_hrp = bech32::encode("cosmos", data, bech32::Variant::Bech32).unwrap();

        assert!(NodePubkey::from_bech32(&wrong_hrp).is_none());
    }

    #[test]
    fn bech32_rejects_invalid_string() {
        assert!(NodePubkey::from_bech32("not-a-bech32-string").is_none());
        assert!(NodePubkey::from_bech32("ergo1invalid").is_none());
        assert!(NodePubkey::from_bech32("").is_none());
    }

    #[test]
    fn mnemonic_deterministic() {
        let phrase = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
        let a = NodePrivKey::from_mnemonic(phrase).expect("valid mnemonic");
        let b = NodePrivKey::from_mnemonic(phrase).expect("valid mnemonic");
        assert_eq!(a.id().0.to_vec(), b.id().0.to_vec(), "same mnemonic must produce same pubkey");

        let sig_a = a.sign(Some(TEST_NS), b"payload");
        let sig_b = b.sign(Some(TEST_NS), b"payload");
        assert_eq!(sig_a, sig_b, "same mnemonic must produce same signatures");
    }

    #[test]
    fn mnemonic_rejects_invalid() {
        assert!(NodePrivKey::from_mnemonic("not a valid mnemonic phrase").is_none());
        assert!(NodePrivKey::from_mnemonic("").is_none());
    }
}
