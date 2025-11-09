//! Node‑identity utilities built on top of `commonware‑cryptography::ed25519`.
//!
//! The public key is exposed as a thin wrapper type (`NodeId`) that implements
//! `Encode`/`ReadExt` so it can be sent over the network or stored on disk.
//! The private key lives only inside the `NodeIdentity` struct and can be
//! generated freshly or from a deterministic seed (useful for tests).

use crate::prelude::NodeType;
use crate::traits::NodeIdentityTrait;
use crate::types::cw_ho::network::v1::{HostOs, NodeIdentity};
use commonware_codec::{DecodeExt, Encode, FixedSize};
use commonware_cryptography::{ed25519, PrivateKeyExt, Signer, Verifier};
use rand::{CryptoRng, RngCore};
use rand_core::OsRng;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

// Use proto types

impl NodeIdentityTrait for NodeIdentity {
    type HostOS = HostOs;
    type NodeType = NodeType;
    type PrivateKey = NodePrivKey;
    type PublicKey = NodePubkey;

    /// Create a new node identitNodeIdentity
    fn new() -> NodeIdentity {
        let mut ego = Self::default();
        ego.user = "ergors".into();
        ego.generate_keypair(&mut OsRng).expect("rand err");
        ego.api_port = 8080;
        ego.p2p_port = 26969;
        ego.ssh_port = 22;
        ego.node_type = NodeType::Unspecified.as_str_name().into();
        ego.os = HostOs::Unspecified.into();
        ego.host = "127.0.0.1".into();
        ego
    }

    /// Generate a fresh, random keypair.
    ///
    /// The function pulls randomness from the supplied `rng`.
    fn generate_keypair<R: RngCore + CryptoRng>(
        &mut self,
        rng: &mut R,
    ) -> super::error::CommonwareNetworkResult<()> {
        let private_key = NodePrivKey::new(rng);
        self.set_keypair(private_key);
        Ok(())
    }

    /// Set keypair from existing keys
    fn set_keypair(&mut self, private_key: Self::PrivateKey) {
        let npk = &private_key;
        self.public_key = Some(private_key.id().0.to_vec());
        self.private_key = Some(npk.private.to_vec());
    }

    /// Get the P2P identity address, includes an identifies via public key appended to the listening socketAddr
    fn p2p_identity(&self) -> String {
        let pubkey_hex = self
            .public_key
            .as_ref()
            .map(|pk| hex::encode(pk))
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
        let pubkey_hex = self
            .public_key
            .as_ref()
            .map(|pk| hex::encode(&pk[..8])) // Show first 8 bytes as hex
            .unwrap_or_else(|| "no_pubkey".to_string());
        format!("{}-{}", self.node_type, pubkey_hex)
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
        let hex_str = hex::encode(self.0.to_vec());
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

// impl fmt::Debug for NodeId {
//     fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
//         // Show the public key as a short hex string (first 8 bytes)
//         let bytes = self.0.as_ref();
//         write!(f, "NodeId({})", hex::encode(&bytes[..8]))
//     }
// }

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
}
