//! Shamir Secret Sharing implementation over GF(256)
//!
//! Implements threshold (k-of-n) secret sharing where:
//! - A secret is split into n shares
//! - Any k shares can reconstruct the secret
//! - Fewer than k shares reveal nothing about the secret

use super::share::{DecryptedShare, Secret, Share};
use crate::error::{HoError, HoResult};
use rand_core::CryptoRngCore;
use tracing::debug;

/// GF(256) irreducible polynomial: x^8 + x^4 + x^3 + x + 1
/// This is the AES polynomial 0x11B
const GF_POLY: u16 = 0x11B;

/// Multiply two elements in GF(256) using the Russian peasant algorithm
fn gf256_mul(mut a: u8, mut b: u8) -> u8 {
    let mut result: u8 = 0;
    while b > 0 {
        if b & 1 != 0 {
            result ^= a;
        }
        // Multiply a by x (left shift)
        let high_bit = a & 0x80;
        a <<= 1;
        // Reduce if overflow
        if high_bit != 0 {
            a ^= (GF_POLY & 0xFF) as u8;
        }
        b >>= 1;
    }
    result
}

/// Compute multiplicative inverse in GF(256) using extended Euclidean algorithm
/// Returns 0 for input 0 (which has no inverse)
fn gf256_inv(a: u8) -> u8 {
    if a == 0 {
        return 0;
    }
    // Use Fermat's little theorem: a^(-1) = a^(254) in GF(256)
    // 254 = 128 + 64 + 32 + 16 + 8 + 4 + 2 = 0xFE
    let mut result = 1u8;
    let mut base = a;
    let mut exp = 254u8;

    while exp > 0 {
        if exp & 1 != 0 {
            result = gf256_mul(result, base);
        }
        base = gf256_mul(base, base);
        exp >>= 1;
    }
    result
}

/// Divide a by b in GF(256)
fn gf256_div(a: u8, b: u8) -> u8 {
    if b == 0 {
        panic!("Division by zero in GF(256)");
    }
    gf256_mul(a, gf256_inv(b))
}

/// Addition in GF(256) (XOR)
#[inline]
fn gf256_add(a: u8, b: u8) -> u8 {
    a ^ b
}

/// Subtraction in GF(256) (same as addition in characteristic 2)
#[inline]
fn gf256_sub(a: u8, b: u8) -> u8 {
    a ^ b
}

/// Evaluate a polynomial at x using Horner's method
/// coeffs[0] is the constant term (the secret)
fn evaluate_polynomial(coeffs: &[u8], x: u8) -> u8 {
    if coeffs.is_empty() {
        return 0;
    }
    // Horner's method: ((a_n * x + a_{n-1}) * x + ... + a_1) * x + a_0
    let mut result = 0u8;
    for &coeff in coeffs.iter().rev() {
        result = gf256_add(gf256_mul(result, x), coeff);
    }
    result
}

/// Lagrange interpolation at x=0 to recover the secret
/// points is a slice of (x, y) pairs
fn lagrange_at_zero(points: &[(u8, u8)]) -> u8 {
    let mut result = 0u8;

    for (i, &(xi, yi)) in points.iter().enumerate() {
        // Calculate Lagrange basis polynomial L_i(0)
        let mut numerator = 1u8;
        let mut denominator = 1u8;

        for (j, &(xj, _)) in points.iter().enumerate() {
            if i != j {
                // L_i(0) = product(0 - xj) / product(xi - xj) for j != i
                // In GF(256), 0 - xj = xj (negation is same as value)
                numerator = gf256_mul(numerator, xj);
                denominator = gf256_mul(denominator, gf256_sub(xi, xj));
            }
        }

        // L_i(0) * yi
        let basis = gf256_div(numerator, denominator);
        let term = gf256_mul(basis, yi);
        result = gf256_add(result, term);
    }

    result
}

/// Split a secret into n shares with threshold k
///
/// # Arguments
/// * `rng` - Cryptographically secure random number generator
/// * `secret` - The secret to split
/// * `threshold` - Minimum shares needed to reconstruct (k)
/// * `total` - Total shares to generate (n)
///
/// # Returns
/// Vector of shares, each containing an index (1..=n) and value
pub fn split(
    rng: &mut impl CryptoRngCore,
    secret: &Secret,
    threshold: u8,
    total: u8,
) -> HoResult<Vec<Share>> {
    // Validate parameters
    if threshold == 0 {
        return Err(HoError::Cfg("Threshold must be at least 1".to_string()));
    }
    if total < threshold {
        return Err(HoError::Cfg(
            "Total shares must be >= threshold".to_string(),
        ));
    }
    if total > 255 {
        return Err(HoError::Cfg("Maximum 255 shares supported".to_string()));
    }
    if secret.is_empty() {
        return Err(HoError::Cfg("Cannot split empty secret".to_string()));
    }

    let secret_bytes = secret.as_bytes();
    let mut shares: Vec<Share> = (1..=total)
        .map(|i| Share::new(i, Vec::with_capacity(secret_bytes.len())))
        .collect();

    // For each byte of the secret, create a random polynomial and evaluate
    for &byte in secret_bytes {
        // Generate random polynomial coefficients
        // coeffs[0] = secret byte (constant term)
        // coeffs[1..k] = random values
        let mut coeffs = vec![byte];
        for _ in 1..threshold {
            let mut rand_byte = [0u8];
            rng.fill_bytes(&mut rand_byte);
            // Avoid zero coefficients for non-constant terms
            if rand_byte[0] == 0 {
                rand_byte[0] = 1;
            }
            coeffs.push(rand_byte[0]);
        }

        // Evaluate polynomial at each x-coordinate (1, 2, ..., n)
        for share in &mut shares {
            let y = evaluate_polynomial(&coeffs, share.index);
            share.value.push(y);
        }
    }

    debug!(
        "Split secret into {} shares with threshold {}",
        total, threshold
    );
    Ok(shares)
}

/// Reconstruct a secret from shares using Lagrange interpolation
///
/// # Arguments
/// * `shares` - At least k shares (k = threshold)
/// * `threshold` - The threshold used when splitting
///
/// # Returns
/// The reconstructed secret
pub fn reconstruct(shares: &[DecryptedShare], threshold: u8) -> HoResult<Secret> {
    if shares.len() < threshold as usize {
        return Err(HoError::Cfg(format!(
            "Need at least {} shares, got {}",
            threshold,
            shares.len()
        )));
    }
    if shares.is_empty() {
        return Err(HoError::Cfg("Cannot reconstruct from empty shares".to_string()));
    }

    // All shares must have the same length
    let len = shares[0].value.len();
    if shares.iter().any(|s| s.value.len() != len) {
        return Err(HoError::Cfg("All shares must have same length".to_string()));
    }

    // Reconstruct each byte of the secret
    let mut result = Vec::with_capacity(len);
    let shares_to_use: Vec<_> = shares.iter().take(threshold as usize).collect();

    for i in 0..len {
        // Collect (x, y) points for this byte position
        let points: Vec<(u8, u8)> = shares_to_use
            .iter()
            .map(|s| (s.index, s.value[i]))
            .collect();

        // Interpolate at x=0 to get the secret byte
        let byte = lagrange_at_zero(&points);
        result.push(byte);
    }

    debug!(
        "Reconstructed secret from {} shares (threshold {})",
        shares_to_use.len(),
        threshold
    );
    Ok(Secret::new(result))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::rngs::OsRng;

    #[test]
    fn test_gf256_operations() {
        // Identity
        assert_eq!(gf256_add(0, 0), 0);
        assert_eq!(gf256_mul(0, 1), 0);
        assert_eq!(gf256_mul(1, 1), 1);

        // Self-inverse property of addition
        for a in 0..=255u8 {
            assert_eq!(gf256_add(a, a), 0);
        }

        // Multiplicative inverse
        for a in 1..=255u8 {
            let inv = gf256_inv(a);
            assert_eq!(gf256_mul(a, inv), 1, "a={}, inv={}", a, inv);
        }
    }

    #[test]
    fn test_polynomial_evaluation() {
        // f(x) = 3 + 2x + x^2 evaluated at x=2 in GF(256)
        // f(2) = 3 ^ (2*2) ^ (2*2) = 3 ^ 4 ^ 4 = 3
        let coeffs = vec![3u8, 2, 1];
        let result = evaluate_polynomial(&coeffs, 2);
        // In GF(256): 3 + 2*2 + 1*4 = 3 + 4 + 4 = 3
        assert_eq!(result, 3);
    }

    #[test]
    fn test_basic_split_reconstruct() {
        let secret = Secret::from_str("hello");
        let shares = split(&mut OsRng, &secret, 2, 3).unwrap();

        assert_eq!(shares.len(), 3);
        assert_eq!(shares[0].value.len(), 5);

        // Convert to DecryptedShare
        let decrypted: Vec<DecryptedShare> = shares
            .iter()
            .map(|s| DecryptedShare::new(s.index, s.value.clone()))
            .collect();

        // Reconstruct with any 2 shares
        let reconstructed = reconstruct(&decrypted[0..2], 2).unwrap();
        assert_eq!(reconstructed.as_string().unwrap(), "hello");

        let reconstructed2 = reconstruct(&decrypted[1..3], 2).unwrap();
        assert_eq!(reconstructed2.as_string().unwrap(), "hello");
    }

    #[test]
    fn test_threshold_3_of_5() {
        let secret = Secret::from_str("sk-test-api-key-1234567890");
        let shares = split(&mut OsRng, &secret, 3, 5).unwrap();

        assert_eq!(shares.len(), 5);

        let decrypted: Vec<DecryptedShare> = shares
            .iter()
            .map(|s| DecryptedShare::new(s.index, s.value.clone()))
            .collect();

        // Any 3 shares should work
        let reconstructed = reconstruct(&[decrypted[0].clone(), decrypted[2].clone(), decrypted[4].clone()], 3).unwrap();
        assert_eq!(
            reconstructed.as_string().unwrap(),
            "sk-test-api-key-1234567890"
        );
    }

    #[test]
    fn test_insufficient_shares() {
        let secret = Secret::from_str("test");
        let shares = split(&mut OsRng, &secret, 3, 5).unwrap();

        let decrypted: Vec<DecryptedShare> = shares
            .iter()
            .take(2)
            .map(|s| DecryptedShare::new(s.index, s.value.clone()))
            .collect();

        // Should fail with only 2 shares when threshold is 3
        let result = reconstruct(&decrypted, 3);
        assert!(result.is_err());
    }

    #[test]
    fn test_binary_data() {
        let binary_data: Vec<u8> = (0..=255).collect();
        let secret = Secret::new(binary_data.clone());
        let shares = split(&mut OsRng, &secret, 2, 5).unwrap();

        let decrypted: Vec<DecryptedShare> = shares
            .iter()
            .map(|s| DecryptedShare::new(s.index, s.value.clone()))
            .collect();

        let reconstructed = reconstruct(&decrypted[0..2], 2).unwrap();
        assert_eq!(reconstructed.as_bytes(), &binary_data);
    }
}
