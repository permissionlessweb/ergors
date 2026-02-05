//! Bootstrap Transport Layer
//!
//! Secure file transfer over commonware Channel 4 (key_sharing channel).
//! Handles authenticated transmission of configs, custody files, mnemonics, and binaries
//! during node bootstrap operations.

use crate::error::{HoError, HoResult};
use crate::keys::commonware::NodePubkey;
use bytes::Bytes;
use commonware_codec::DecodeExt;
use commonware_cryptography::ed25519;
use commonware_p2p::{authenticated, Recipients, Sender};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, Mutex};

/// File types that can be transferred during bootstrap
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[repr(u8)]
pub enum FileType {
    /// config.toml file
    Config = 1,
    /// Encrypted custody file (contains node private key)
    Custody = 2,
    /// ergors binary (fallback if not using Docker)
    Binary = 3,
    /// Encrypted mnemonic for cosmos keys
    Mnemonic = 4,
}

impl FileType {
    fn from_u8(v: u8) -> HoResult<Self> {
        match v {
            1 => Ok(Self::Config),
            2 => Ok(Self::Custody),
            3 => Ok(Self::Binary),
            4 => Ok(Self::Mnemonic),
            _ => Err(HoError::BootstrapError(format!("Invalid file type: {}", v))),
        }
    }
}

/// Bootstrap file message
///
/// Format: [file_type:1][data_len:4][data:N]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BootstrapFileMessage {
    pub file_type: FileType,
    pub data: Vec<u8>,
}

impl BootstrapFileMessage {
    /// Encode to bytes for transmission
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.push(self.file_type as u8);
        buf.extend_from_slice(&(self.data.len() as u32).to_be_bytes());
        buf.extend_from_slice(&self.data);
        buf
    }

    /// Decode from bytes
    pub fn decode(data: &[u8]) -> HoResult<Self> {
        if data.len() < 5 {
            return Err(HoError::BootstrapError("Message too short".to_string()));
        }

        let file_type = FileType::from_u8(data[0])?;
        let data_len = u32::from_be_bytes([data[1], data[2], data[3], data[4]]) as usize;

        if data.len() != 5 + data_len {
            return Err(HoError::BootstrapError(format!(
                "Message length mismatch: expected {}, got {}",
                5 + data_len,
                data.len()
            )));
        }

        Ok(Self {
            file_type,
            data: data[5..].to_vec(),
        })
    }
}

/// Bootstrap transport over commonware Channel 4
///
/// Provides authenticated file transfer for bootstrap operations.
/// Uses the key_sharing channel (Channel 4) which is rate-limited for security.
pub struct BootstrapTransport {
    /// Sender for Channel 4 (key_sharing/bootstrap)
    sender: authenticated::lookup::Sender<ed25519::PublicKey>,
    /// Receiver queue for incoming messages
    /// We use an mpsc channel as a queue because commonware Receiver is used in a background task
    receive_queue: Arc<Mutex<mpsc::UnboundedReceiver<(ed25519::PublicKey, Bytes)>>>,
    /// Sender for the receive queue (to be used by background receiver task)
    queue_sender: mpsc::UnboundedSender<(ed25519::PublicKey, Bytes)>,
}

impl BootstrapTransport {
    /// Create a new bootstrap transport
    ///
    /// Takes ownership of the Channel 4 sender.
    /// The receiver should be handled by a background task that feeds into the queue.
    pub fn new(sender: authenticated::lookup::Sender<ed25519::PublicKey>) -> Self {
        let (queue_sender, receive_queue) = mpsc::unbounded_channel();

        Self {
            sender,
            receive_queue: Arc::new(Mutex::new(receive_queue)),
            queue_sender,
        }
    }

    /// Get the queue sender for feeding received messages
    ///
    /// This should be used by the network manager's message processing task
    /// to feed received bootstrap messages into the transport.
    pub fn queue_sender(&self) -> mpsc::UnboundedSender<(ed25519::PublicKey, Bytes)> {
        self.queue_sender.clone()
    }

    /// Send file to a specific peer
    ///
    /// Transmits the file using Channel 4 with authentication.
    /// The message is sent to a single recipient (bootstrap target).
    pub async fn send_file(
        &mut self,
        recipient: &NodePubkey,
        file_type: FileType,
        data: Vec<u8>,
    ) -> HoResult<()> {
        let message = BootstrapFileMessage { file_type, data };
        let encoded = message.encode();

        // Convert NodePubkey to ed25519::PublicKey
        let ed25519_pubkey = ed25519::PublicKey::decode(&recipient.0[..])
            .map_err(|_| HoError::BootstrapError("Invalid recipient public key".to_string()))?;

        // Send to single recipient using Channel 4
        // Third parameter is `reliable` - true for guaranteed delivery
        self.sender
            .send(Recipients::One(ed25519_pubkey), Bytes::from(encoded), true)
            .await
            .map_err(|e| {
                HoError::BootstrapError(format!("Failed to send file: {:?}", e))
            })?;

        Ok(())
    }

    /// Receive file from any peer with timeout
    ///
    /// Blocks until a file is received or timeout expires.
    /// Returns the file type, data, and sender's public key.
    pub async fn receive_file(
        &self,
        timeout: Duration,
    ) -> HoResult<(FileType, Vec<u8>, ed25519::PublicKey)> {
        let mut receiver = self.receive_queue.lock().await;

        // Wait for message with timeout
        let (sender_pubkey, data) = tokio::time::timeout(timeout, receiver.recv())
            .await
            .map_err(|_| HoError::BootstrapError("Receive timeout".to_string()))?
            .ok_or_else(|| HoError::BootstrapError("Receive channel closed".to_string()))?;

        // Decode message
        let message = BootstrapFileMessage::decode(&data)?;

        Ok((message.file_type, message.data, sender_pubkey))
    }

    /// Send API key share (using secret sharing protocol)
    ///
    /// This is for distributing API keys across nodes using Shamir secret sharing.
    pub async fn send_key_share(
        &mut self,
        recipient: &NodePubkey,
        share_data: Vec<u8>,
    ) -> HoResult<()> {
        // Key shares use a special message format
        // For now, we'll use the generic file transfer mechanism
        // TODO: Implement proper secret sharing wire protocol
        self.send_file(recipient, FileType::Mnemonic, share_data)
            .await
    }
}

impl Clone for BootstrapTransport {
    fn clone(&self) -> Self {
        Self {
            sender: self.sender.clone(),
            receive_queue: self.receive_queue.clone(),
            queue_sender: self.queue_sender.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_encoding() {
        let msg = BootstrapFileMessage {
            file_type: FileType::Config,
            data: b"test config data".to_vec(),
        };

        let encoded = msg.encode();
        let decoded = BootstrapFileMessage::decode(&encoded).unwrap();

        assert_eq!(decoded.file_type, FileType::Config);
        assert_eq!(decoded.data, b"test config data");
    }

    #[test]
    fn test_message_decode_invalid() {
        // Too short
        let result = BootstrapFileMessage::decode(&[1, 0, 0, 0]);
        assert!(result.is_err());

        // Length mismatch
        let result = BootstrapFileMessage::decode(&[1, 0, 0, 0, 10, 1, 2, 3]);
        assert!(result.is_err());
    }

    #[test]
    fn test_file_type_conversion() {
        assert_eq!(FileType::from_u8(1).unwrap(), FileType::Config);
        assert_eq!(FileType::from_u8(2).unwrap(), FileType::Custody);
        assert_eq!(FileType::from_u8(3).unwrap(), FileType::Binary);
        assert_eq!(FileType::from_u8(4).unwrap(), FileType::Mnemonic);
        assert!(FileType::from_u8(99).is_err());
    }
}
