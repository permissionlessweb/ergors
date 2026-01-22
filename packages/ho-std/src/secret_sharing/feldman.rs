//! Feldman Verifiable Secret Sharing (VSS) commitments
//!
//! Provides commitments that allow share holders to verify their shares
//! are consistent without revealing the secret.
//!
//! Note: This is a simplified implementation using hash-based commitments
//! rather than full elliptic curve Pedersen commitments. For production use
//! with malicious adversaries, use a proper curve-based implementation.

use sha2::{Digest, Sha256};
use tracing::debug;

/// Feldman commitment for a Shamir secret sharing polynomial
///
/// Contains hash commitments to each coefficient of the polynomial.
/// Share holders can verify their share is consistent with these commitments.
#[derive(Debug, Clone)]
pub struct FeldmanCommitment {
    /// Hash commitments to polynomial coefficients
    /// commitment[i] = SHA256(salt || coefficient_i)
    pub commitments: Vec<[u8; 32]>,
    /// Salt used for the commitments (prevents precomputation attacks)
    pub salt: [u8; 16],
}

impl FeldmanCommitment {
    /// Create a new Feldman commitment from polynomial coefficients
    ///
    /// # Arguments
    /// * `coefficients` - For each byte position, the polynomial coefficients
    ///                   (one polynomial per byte of the secret)
    /// * `salt` - Random salt to use for commitments
    pub fn new(coefficients: &[Vec<u8>], salt: [u8; 16]) -> Self {
        let commitments: Vec<[u8; 32]> = coefficients
            .iter()
            .map(|poly_coeffs| {
                let mut hasher = Sha256::new();
                hasher.update(&salt);
                for coeff in poly_coeffs {
                    hasher.update([*coeff]);
                }
                let result = hasher.finalize();
                let mut commitment = [0u8; 32];
                commitment.copy_from_slice(&result);
                commitment
            })
            .collect();

        Self { commitments, salt }
    }

    /// Serialize the commitment for network transmission
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(16 + 4 + self.commitments.len() * 32);
        bytes.extend_from_slice(&self.salt);
        bytes.extend_from_slice(&(self.commitments.len() as u32).to_le_bytes());
        for commitment in &self.commitments {
            bytes.extend_from_slice(commitment);
        }
        bytes
    }

    /// Deserialize a commitment from bytes
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < 20 {
            return None; // salt (16) + count (4)
        }

        let mut salt = [0u8; 16];
        salt.copy_from_slice(&bytes[0..16]);

        let count = u32::from_le_bytes(bytes[16..20].try_into().ok()?) as usize;

        if bytes.len() < 20 + count * 32 {
            return None;
        }

        let mut commitments = Vec::with_capacity(count);
        for i in 0..count {
            let start = 20 + i * 32;
            let mut commitment = [0u8; 32];
            commitment.copy_from_slice(&bytes[start..start + 32]);
            commitments.push(commitment);
        }

        Some(Self { commitments, salt })
    }

    /// Verify that a set of polynomial coefficients matches this commitment
    ///
    /// This is used by the coordinator to verify the commitment was created correctly.
    pub fn verify_coefficients(&self, coefficients: &[Vec<u8>]) -> bool {
        if coefficients.len() != self.commitments.len() {
            return false;
        }

        for (i, poly_coeffs) in coefficients.iter().enumerate() {
            let mut hasher = Sha256::new();
            hasher.update(&self.salt);
            for coeff in poly_coeffs {
                hasher.update([*coeff]);
            }
            let expected = hasher.finalize();

            if &self.commitments[i] != expected.as_slice() {
                debug!("Commitment verification failed at index {}", i);
                return false;
            }
        }

        true
    }
}

/// Generate a random salt for Feldman commitments
pub fn generate_salt(rng: &mut impl rand_core::CryptoRngCore) -> [u8; 16] {
    let mut salt = [0u8; 16];
    rng.fill_bytes(&mut salt);
    salt
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::rngs::OsRng;

    #[test]
    fn test_commitment_roundtrip() {
        let coefficients = vec![
            vec![1u8, 2, 3],
            vec![4u8, 5, 6],
            vec![7u8, 8, 9],
        ];
        let salt = generate_salt(&mut OsRng);

        let commitment = FeldmanCommitment::new(&coefficients, salt);

        // Verify original coefficients
        assert!(commitment.verify_coefficients(&coefficients));

        // Serialize and deserialize
        let bytes = commitment.to_bytes();
        let restored = FeldmanCommitment::from_bytes(&bytes).unwrap();

        // Verify restored commitment
        assert!(restored.verify_coefficients(&coefficients));
    }

    #[test]
    fn test_wrong_coefficients_fail() {
        let coefficients = vec![
            vec![1u8, 2, 3],
            vec![4u8, 5, 6],
        ];
        let salt = generate_salt(&mut OsRng);

        let commitment = FeldmanCommitment::new(&coefficients, salt);

        // Wrong coefficients should fail
        let wrong = vec![
            vec![1u8, 2, 3],
            vec![4u8, 5, 7], // Changed
        ];
        assert!(!commitment.verify_coefficients(&wrong));
    }
}
