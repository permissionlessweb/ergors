//! Share types for secret sharing
//!
//! Provides types for handling secret data with manual memory clearing on drop.

/// Raw share before encryption (clears memory on drop)
#[derive(Clone)]
pub struct Share {
    /// Share index (1-indexed for Shamir, 1 for direct mode)
    pub index: u8,
    /// The share value
    pub value: Vec<u8>,
}

impl Share {
    /// Create a new share
    pub fn new(index: u8, value: Vec<u8>) -> Self {
        Self { index, value }
    }
}

impl Drop for Share {
    fn drop(&mut self) {
        // Securely clear the value
        for byte in &mut self.value {
            *byte = 0;
        }
    }
}

impl std::fmt::Debug for Share {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Share")
            .field("index", &self.index)
            .field("value_len", &self.value.len())
            .finish()
    }
}

/// Encrypted share for network transmission
#[derive(Clone, Debug)]
pub struct EncryptedShare {
    /// Share index (1-indexed for Shamir, 1 for direct mode)
    pub index: u8,
    /// Encrypted share value (encrypted for recipient's public key)
    pub encrypted_value: Vec<u8>,
    /// Public key of the intended recipient
    pub recipient_pubkey: Vec<u8>,
    /// Sharing mode used
    pub mode: SharingMode,
    /// Feldman commitment (Shamir only)
    pub commitment: Option<Vec<u8>>,
}

/// Decrypted share for reconstruction (clears memory on drop)
#[derive(Clone)]
pub struct DecryptedShare {
    /// Share index
    pub index: u8,
    /// Decrypted share value
    pub value: Vec<u8>,
}

impl DecryptedShare {
    /// Create a new decrypted share
    pub fn new(index: u8, value: Vec<u8>) -> Self {
        Self { index, value }
    }
}

impl Drop for DecryptedShare {
    fn drop(&mut self) {
        // Securely clear the value
        for byte in &mut self.value {
            *byte = 0;
        }
    }
}

impl std::fmt::Debug for DecryptedShare {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DecryptedShare")
            .field("index", &self.index)
            .field("value_len", &self.value.len())
            .finish()
    }
}

/// Secret with memory clearing on drop
#[derive(Clone)]
pub struct Secret(Vec<u8>);

impl Secret {
    /// Create a new secret from bytes
    pub fn new(data: Vec<u8>) -> Self {
        Self(data)
    }

    /// Create a secret from a string (e.g., API key)
    pub fn from_str(s: &str) -> Self {
        Self(s.as_bytes().to_vec())
    }

    /// Get the secret as bytes
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    /// Get the length of the secret
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Convert to string (for API keys)
    ///
    /// Returns None if the bytes are not valid UTF-8
    pub fn as_string(&self) -> Option<String> {
        String::from_utf8(self.0.clone()).ok()
    }
}

impl Drop for Secret {
    fn drop(&mut self) {
        // Securely clear the secret
        for byte in &mut self.0 {
            *byte = 0;
        }
    }
}

impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Secret")
            .field("len", &self.0.len())
            .finish()
    }
}

/// Sharing mode for secret distribution
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SharingMode {
    /// 1-to-1 encrypted sharing between two nodes
    Direct,
    /// n-of-m threshold secret sharing (Shamir)
    Shamir {
        /// Minimum shares needed to reconstruct
        threshold: u8,
        /// Total shares generated
        total: u8,
    },
}

impl SharingMode {
    /// Create a direct sharing mode
    pub fn direct() -> Self {
        Self::Direct
    }

    /// Create a Shamir sharing mode
    pub fn shamir(threshold: u8, total: u8) -> Self {
        Self::Shamir { threshold, total }
    }

    /// Check if this is direct mode
    pub fn is_direct(&self) -> bool {
        matches!(self, Self::Direct)
    }

    /// Check if this is Shamir mode
    pub fn is_shamir(&self) -> bool {
        matches!(self, Self::Shamir { .. })
    }

    /// Get threshold (returns 1 for direct mode)
    pub fn threshold(&self) -> u8 {
        match self {
            Self::Direct => 1,
            Self::Shamir { threshold, .. } => *threshold,
        }
    }

    /// Get total shares (returns 1 for direct mode)
    pub fn total(&self) -> u8 {
        match self {
            Self::Direct => 1,
            Self::Shamir { total, .. } => *total,
        }
    }
}
