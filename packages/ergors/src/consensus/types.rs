//! Core consensus types for the Ergors meta-ledger.
//!
//! The meta-ledger does not store actual node state (LLM outputs, API keys, configs).
//! It stores signed *commitments* to state transitions — proving that a node
//! transitioned from one Cnidarium Merkle root to another, without revealing
//! the transition data itself.

use commonware_codec::{DecodeExt, Encode};
use commonware_cryptography::{ed25519, Verifier};
use ho_std::keys::commonware::NodePrivKey;
use sha2::{Digest as _, Sha256};

/// Namespace for signing/verifying commitments.
/// Prevents cross-protocol signature replay.
pub const COMMITMENT_NS: Option<&[u8]> = Some(b"ergors-commitment");

/// What kind of state transition a commitment represents.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum CommitmentKind {
    /// LLM inference result stored
    Inference = 0,
    /// Orchestration queue mutation
    Orchestration = 1,
    /// API key rotation
    KeyRotation = 2,
    /// Config change
    ConfigUpdate = 3,
    /// CosmWasm contract execution
    ContractExec = 4,
    /// Anything else
    Generic = 5,
}

impl CommitmentKind {
    pub fn as_u8(self) -> u8 {
        self as u8
    }

    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::Inference),
            1 => Some(Self::Orchestration),
            2 => Some(Self::KeyRotation),
            3 => Some(Self::ConfigUpdate),
            4 => Some(Self::ContractExec),
            5 => Some(Self::Generic),
            _ => None,
        }
    }
}

impl std::fmt::Display for CommitmentKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Inference => write!(f, "inference"),
            Self::Orchestration => write!(f, "orchestration"),
            Self::KeyRotation => write!(f, "key_rotation"),
            Self::ConfigUpdate => write!(f, "config_update"),
            Self::ContractExec => write!(f, "contract_exec"),
            Self::Generic => write!(f, "generic"),
        }
    }
}

/// A single node's signed commitment to a state transition.
///
/// This is the "transaction" in the Ergors meta-ledger. Contains:
/// - Cryptographic proof (Ed25519 signature) that a node transitioned state
/// - Opaque hash of the transition data (privacy by default)
/// - Monotonic sequence number (replay protection)
///
/// Wire format: 201 bytes fixed size.
/// `[32 pubkey][32 prev_root][32 new_root][32 transition][1 kind][8 seq_le][64 sig]`
#[derive(Clone, Debug)]
pub struct NodeCommitment {
    /// The node that made this commitment
    pub node_id: ed25519::PublicKey,
    /// Cnidarium Merkle root BEFORE the transition
    pub prev_state_root: [u8; 32],
    /// Cnidarium Merkle root AFTER the transition
    pub new_state_root: [u8; 32],
    /// SHA-256(transition_data) — actual data stays on the node
    pub transition_digest: [u8; 32],
    /// Type of state transition
    pub kind: CommitmentKind,
    /// Per-node monotonic counter (prevents replay)
    pub sequence: u64,
    /// Ed25519 signature over the signing payload
    pub signature: ed25519::Signature,
}

impl NodeCommitment {
    /// Create and sign a new commitment.
    ///
    /// `transition_data` is hashed (never stored or transmitted).
    /// The commitment proves the transition happened without revealing what it was.
    pub fn new(
        signer: &NodePrivKey,
        prev_state_root: [u8; 32],
        new_state_root: [u8; 32],
        transition_data: &[u8],
        kind: CommitmentKind,
        sequence: u64,
    ) -> Self {
        let transition_digest: [u8; 32] = Sha256::digest(transition_data).into();
        let payload =
            Self::signing_payload(&prev_state_root, &new_state_root, &transition_digest, sequence);
        let signature = signer.sign(COMMITMENT_NS, &payload);

        Self {
            node_id: signer.id().0,
            prev_state_root,
            new_state_root,
            transition_digest,
            kind,
            sequence,
            signature,
        }
    }

    /// Verify the commitment signature.
    pub fn verify(&self) -> bool {
        let payload = Self::signing_payload(
            &self.prev_state_root,
            &self.new_state_root,
            &self.transition_digest,
            self.sequence,
        );
        self.node_id
            .verify(COMMITMENT_NS, &payload, &self.signature)
    }

    /// The signing payload: deterministic concatenation of the committed fields.
    /// `prev_state_root || new_state_root || transition_digest || sequence_le`
    fn signing_payload(
        prev: &[u8; 32],
        new: &[u8; 32],
        transition: &[u8; 32],
        seq: u64,
    ) -> Vec<u8> {
        let mut buf = Vec::with_capacity(32 + 32 + 32 + 8);
        buf.extend_from_slice(prev);
        buf.extend_from_slice(new);
        buf.extend_from_slice(transition);
        buf.extend_from_slice(&seq.to_le_bytes());
        buf
    }

    /// Encode to fixed-size wire format (201 bytes).
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(201);
        let pk_bytes = self.node_id.encode();
        buf.extend_from_slice(&pk_bytes);
        buf.extend_from_slice(&self.prev_state_root);
        buf.extend_from_slice(&self.new_state_root);
        buf.extend_from_slice(&self.transition_digest);
        buf.push(self.kind.as_u8());
        buf.extend_from_slice(&self.sequence.to_le_bytes());
        let sig_bytes = self.signature.encode();
        buf.extend_from_slice(&sig_bytes);
        buf
    }

    /// Decode from wire bytes. Returns None on invalid data.
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() != 201 {
            return None;
        }

        let node_id = ed25519::PublicKey::decode(&bytes[0..32]).ok()?;
        let prev_state_root: [u8; 32] = bytes[32..64].try_into().ok()?;
        let new_state_root: [u8; 32] = bytes[64..96].try_into().ok()?;
        let transition_digest: [u8; 32] = bytes[96..128].try_into().ok()?;
        let kind = CommitmentKind::from_u8(bytes[128])?;
        let sequence = u64::from_le_bytes(bytes[129..137].try_into().ok()?);
        let signature = ed25519::Signature::decode(&bytes[137..201]).ok()?;

        Some(Self {
            node_id,
            prev_state_root,
            new_state_root,
            transition_digest,
            kind,
            sequence,
            signature,
        })
    }

    /// Deterministic key for BTreeMap ordering in the mempool.
    /// Orders by (node_pubkey_bytes, sequence).
    pub fn mempool_key(&self) -> ([u8; 32], u64) {
        (Self::pubkey_bytes(&self.node_id), self.sequence)
    }

    /// Extract the raw 32-byte representation from an ed25519 public key.
    ///
    /// Uses the commonware codec encoding, which is guaranteed to be 32 bytes
    /// for ed25519 keys (FixedSize::SIZE = 32). Panics if this invariant
    /// is violated — which would indicate a breaking commonware API change.
    pub fn pubkey_bytes(pk: &ed25519::PublicKey) -> [u8; 32] {
        let encoded = pk.encode();
        debug_assert_eq!(
            encoded.len(),
            32,
            "ed25519 pubkey encoding must be 32 bytes, got {}",
            encoded.len()
        );
        let mut out = [0u8; 32];
        out.copy_from_slice(&encoded[..32]);
        out
    }
}

// --- Lifecycle event types ---

/// Consensus lifecycle event (ABCI-compatible).
/// Used for tracing, indexing, and CosmWasm hook responses.
#[derive(Clone, Debug, Default)]
pub struct Event {
    pub kind: String,
    pub attributes: Vec<EventAttribute>,
}

impl Event {
    pub fn new(kind: impl Into<String>) -> Self {
        Self {
            kind: kind.into(),
            attributes: Vec::new(),
        }
    }

    pub fn attr(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.attributes.push(EventAttribute {
            key: key.into(),
            value: value.into(),
            index: true,
        });
        self
    }

    pub fn attr_no_index(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.attributes.push(EventAttribute {
            key: key.into(),
            value: value.into(),
            index: false,
        });
        self
    }
}

#[derive(Clone, Debug)]
pub struct EventAttribute {
    pub key: String,
    pub value: String,
    /// Whether this attribute should be indexed for queries
    pub index: bool,
}

/// Response from end_block — may include validator set changes.
#[derive(Clone, Debug, Default)]
pub struct EndBlockResponse {
    pub events: Vec<Event>,
    /// Updated validator set (None = no change)
    pub validator_updates: Option<Vec<ValidatorUpdate>>,
}

/// A change to the validator set.
#[derive(Clone, Debug)]
pub struct ValidatorUpdate {
    pub pubkey: ed25519::PublicKey,
    /// Voting power. 0 = remove from validator set.
    pub power: u64,
}

// --- Logging helper ---

/// Log events using tracing (mirrors Penumbra's trace_events).
pub fn trace_events(events: &[Event]) {
    for event in events {
        let span = tracing::debug_span!("event", kind = %event.kind);
        span.in_scope(|| {
            for attr in &event.attributes {
                tracing::debug!(k = %attr.key, v = %attr.value);
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_signer() -> NodePrivKey {
        NodePrivKey::from_seed(42)
    }

    #[test]
    fn commitment_sign_verify() {
        let signer = test_signer();
        let commitment = NodeCommitment::new(
            &signer,
            [0u8; 32],
            [1u8; 32],
            b"test inference result",
            CommitmentKind::Inference,
            1,
        );

        assert!(commitment.verify(), "valid commitment must verify");
        assert_eq!(commitment.sequence, 1);
        assert_eq!(commitment.kind, CommitmentKind::Inference);
    }

    #[test]
    fn commitment_rejects_tampered_data() {
        let signer = test_signer();
        let mut commitment = NodeCommitment::new(
            &signer,
            [0u8; 32],
            [1u8; 32],
            b"original data",
            CommitmentKind::Generic,
            1,
        );

        // Tamper with the new state root
        commitment.new_state_root = [0xff; 32];
        assert!(!commitment.verify(), "tampered commitment must fail verification");
    }

    #[test]
    fn commitment_rejects_wrong_signer() {
        let signer_a = NodePrivKey::from_seed(1);
        let signer_b = NodePrivKey::from_seed(2);

        let mut commitment = NodeCommitment::new(
            &signer_a,
            [0u8; 32],
            [1u8; 32],
            b"data",
            CommitmentKind::Generic,
            1,
        );

        // Replace node_id with a different key (signature won't match)
        commitment.node_id = signer_b.id().0;
        assert!(!commitment.verify(), "wrong signer must fail verification");
    }

    #[test]
    fn commitment_roundtrip_encoding() {
        let signer = test_signer();
        let original = NodeCommitment::new(
            &signer,
            [0xaa; 32],
            [0xbb; 32],
            b"roundtrip test",
            CommitmentKind::Orchestration,
            42,
        );

        let bytes = original.to_bytes();
        assert_eq!(bytes.len(), 201, "wire format must be 201 bytes");

        let decoded = NodeCommitment::from_bytes(&bytes).expect("valid bytes must decode");
        assert_eq!(decoded.sequence, 42);
        assert_eq!(decoded.kind, CommitmentKind::Orchestration);
        assert_eq!(decoded.prev_state_root, [0xaa; 32]);
        assert_eq!(decoded.new_state_root, [0xbb; 32]);
        assert!(decoded.verify(), "decoded commitment must verify");
    }

    #[test]
    fn commitment_rejects_short_bytes() {
        assert!(NodeCommitment::from_bytes(&[0u8; 200]).is_none());
        assert!(NodeCommitment::from_bytes(&[]).is_none());
    }

    #[test]
    fn commitment_kind_roundtrip() {
        for kind in [
            CommitmentKind::Inference,
            CommitmentKind::Orchestration,
            CommitmentKind::KeyRotation,
            CommitmentKind::ConfigUpdate,
            CommitmentKind::ContractExec,
            CommitmentKind::Generic,
        ] {
            let v = kind.as_u8();
            assert_eq!(CommitmentKind::from_u8(v), Some(kind));
        }
        assert_eq!(CommitmentKind::from_u8(255), None);
    }

    #[test]
    fn mempool_key_deterministic() {
        let signer = test_signer();
        let c1 = NodeCommitment::new(&signer, [0; 32], [1; 32], b"a", CommitmentKind::Generic, 1);
        let c2 = NodeCommitment::new(&signer, [0; 32], [2; 32], b"b", CommitmentKind::Generic, 1);
        // Same signer + same sequence = same mempool key
        assert_eq!(c1.mempool_key(), c2.mempool_key());
    }

    #[test]
    fn event_builder() {
        let event = Event::new("test_event")
            .attr("height", "100")
            .attr_no_index("data", "secret");

        assert_eq!(event.kind, "test_event");
        assert_eq!(event.attributes.len(), 2);
        assert!(event.attributes[0].index);
        assert!(!event.attributes[1].index);
    }
}
