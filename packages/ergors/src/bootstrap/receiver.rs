//! Bootstrap Receiver
//!
//! Runs on newly bootstrapped nodes to receive and process bootstrap data
//! sent by the coordinator (configs, custody files, API keys).

use crate::storage::ErgorsStorage;
use anyhow::{anyhow, Result};
use ho_std::bootstrap::{BootstrapTransport, FileType};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tokio::fs;
use tracing::{debug, info, warn};

/// Default timeout for waiting for bootstrap messages (5 minutes)
const BOOTSTRAP_RECEIVE_TIMEOUT: Duration = Duration::from_secs(300);

/// Bootstrap receiver for new nodes
///
/// This runs on nodes that are being bootstrapped. It listens for
/// incoming files from the coordinator and saves them to disk.
pub struct BootstrapReceiver {
    transport: Arc<BootstrapTransport>,
    storage: Arc<ErgorsStorage>,
    /// Directory where configs are stored
    config_dir: String,
}

impl BootstrapReceiver {
    /// Create a new bootstrap receiver
    pub fn new(
        transport: Arc<BootstrapTransport>,
        storage: Arc<ErgorsStorage>,
        config_dir: String,
    ) -> Self {
        Self {
            transport,
            storage,
            config_dir,
        }
    }

    /// Listen for incoming bootstrap data
    ///
    /// This is the main entry point for bootstrapped nodes.
    /// It runs until all required files are received or timeout occurs.
    pub async fn listen_for_bootstrap(&self) -> Result<()> {
        info!("🎧 Listening for bootstrap data...");

        let mut received_config = false;
        let mut received_custody = false;

        // Keep receiving until we have all required files
        while !received_config || !received_custody {
            match self
                .transport
                .receive_file(BOOTSTRAP_RECEIVE_TIMEOUT)
                .await
            {
                Ok((file_type, data, _sender)) => {
                    debug!(
                        "Received {} bytes of {:?} from sender",
                        data.len(),
                        file_type
                    );

                    match file_type {
                        FileType::Config => {
                            self.save_config(data).await?;
                            received_config = true;
                            info!("✅ Received config file");
                        }
                        FileType::Custody => {
                            self.save_custody(data).await?;
                            received_custody = true;
                            info!("✅ Received custody file");
                        }
                        FileType::Mnemonic => {
                            self.import_mnemonic(data).await?;
                            info!("✅ Received mnemonic");
                        }
                        FileType::Binary => {
                            self.update_binary(data).await?;
                            info!("✅ Received binary update");
                        }
                    }
                }
                Err(e) => {
                    if e.to_string().contains("timeout") {
                        warn!("Bootstrap receive timeout, retrying...");
                        continue;
                    } else {
                        return Err(e.into());
                    }
                }
            }
        }

        info!("✅ Bootstrap complete - all required files received");

        // Send acknowledgment
        self.send_bootstrap_ack().await?;

        Ok(())
    }

    /// Save received config file
    async fn save_config(&self, data: Vec<u8>) -> Result<()> {
        let config_path = Path::new(&self.config_dir).join("config.toml");

        // Validate it's valid TOML before saving
        let config_str = String::from_utf8(data)?;
        toml::from_str::<toml::Value>(&config_str)
            .map_err(|e| anyhow!("Invalid TOML config: {}", e))?;

        // Write to file
        fs::write(&config_path, config_str).await?;
        info!("💾 Saved config to: {}", config_path.display());

        Ok(())
    }

    /// Save received custody file
    async fn save_custody(&self, data: Vec<u8>) -> Result<()> {
        let custody_path = Path::new(&self.config_dir).join("identity.custody");

        // Write encrypted custody data
        fs::write(&custody_path, data).await?;
        info!("🔐 Saved custody to: {}", custody_path.display());

        Ok(())
    }

    /// Import received mnemonic
    async fn import_mnemonic(&self, data: Vec<u8>) -> Result<()> {
        // Decrypt and import mnemonic into cosmos key store
        // TODO: Implement mnemonic import via key manager
        debug!("Received mnemonic ({} bytes)", data.len());

        // For now, just save to a temporary file
        let mnemonic_path = Path::new(&self.config_dir).join("bootstrap_mnemonic.enc");
        fs::write(&mnemonic_path, data).await?;
        info!("🔑 Saved mnemonic to: {}", mnemonic_path.display());

        Ok(())
    }

    /// Update ergors binary
    async fn update_binary(&self, data: Vec<u8>) -> Result<()> {
        // For security, we should verify signature before replacing binary
        // For now, this is a placeholder

        warn!("Binary update received ({} bytes) - not applying automatically", data.len());

        // Save to a staging location
        let binary_path = Path::new(&self.config_dir).join("ergors.staged");
        fs::write(&binary_path, data).await?;
        info!("📦 Staged binary at: {}", binary_path.display());

        Ok(())
    }

    /// Send bootstrap acknowledgment
    async fn send_bootstrap_ack(&self) -> Result<()> {
        // TODO: Send ack message back to coordinator via P2P
        debug!("Sending bootstrap acknowledgment");
        Ok(())
    }

    /// Check if this node is in bootstrap mode
    ///
    /// Detects if the node should wait for bootstrap data by checking
    /// if essential files are missing.
    pub async fn is_bootstrap_mode(config_dir: &str) -> bool {
        let config_path = Path::new(config_dir).join("config.toml");
        let custody_path = Path::new(config_dir).join("identity.custody");

        // Bootstrap mode if either file is missing
        !config_path.exists() || !custody_path.exists()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_is_bootstrap_mode() {
        let temp_dir = std::env::temp_dir().join("ergors_test_bootstrap");
        let _ = fs::remove_dir_all(&temp_dir).await;
        fs::create_dir_all(&temp_dir).await.unwrap();

        // Should be in bootstrap mode when files don't exist
        assert!(BootstrapReceiver::is_bootstrap_mode(temp_dir.to_str().unwrap()).await);

        // Create config file
        let config_path = temp_dir.join("config.toml");
        fs::write(&config_path, "# test config").await.unwrap();

        // Still in bootstrap mode (custody missing)
        assert!(BootstrapReceiver::is_bootstrap_mode(temp_dir.to_str().unwrap()).await);

        // Create custody file
        let custody_path = temp_dir.join("identity.custody");
        fs::write(&custody_path, b"encrypted custody data")
            .await
            .unwrap();

        // Should NOT be in bootstrap mode now
        assert!(!BootstrapReceiver::is_bootstrap_mode(temp_dir.to_str().unwrap()).await);

        // Cleanup
        let _ = fs::remove_dir_all(&temp_dir).await;
    }
}
