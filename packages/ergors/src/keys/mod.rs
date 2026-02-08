//! Key Management CLI Commands
//!
//! Provides `ergors keys` subcommands for importing mnemonic seed phrases,
//! listing keys, setting defaults, and deleting keys.
//!
//! Keys are chain-agnostic — they store raw secp256k1 keypairs with a label.
//! Chain-specific addresses (bech32) are derived at usage time based on the
//! target chain's prefix.
//!
//! When the engine daemon is running (holding the storage lock), commands
//! are routed through gRPC. Otherwise, direct storage access is used.
//!
//! All keys are stored encrypted using Argon2id + ChaCha20Poly1305 via the
//! EncryptedCosmosKeyManager. Mnemonics are never persisted in plaintext.

use anyhow::{anyhow, Result};
use camino::Utf8Path;
use ho_std::constants::DATA_FOLDER_NAME;
use ho_std::keys::encrypted_cosmos::EncryptedCosmosKeyManager;

use crate::client::ManagementClient;
use crate::commands::responses::{KeyEntry, KeyImportResponse, KeyListResponse};
use crate::storage::ErgorsStorage;

/// Default gRPC address for the engine
const DEFAULT_GRPC_ADDR: &str = "http://localhost:50051";

/// Default bech32 prefix for stored keys (chain-agnostic)
const DEFAULT_ADDRESS_PREFIX: &str = "ergo";

/// CLI command for cosmos key management
#[derive(Debug, clap::Parser)]
pub struct KeysCmd {
    #[clap(subcommand)]
    pub subcmd: KeysSubCmd,

    /// Override gRPC address (uses daemon if available)
    #[arg(long, default_value = DEFAULT_GRPC_ADDR, env = "ERGORS_GRPC_ADDR", global = true)]
    pub grpc_addr: String,
}

#[derive(Debug, Clone, clap::Subcommand)]
pub enum KeysSubCmd {
    /// Import a BIP-39 mnemonic seed phrase as a funding key
    ///
    /// The mnemonic is entered interactively (hidden input) for security.
    /// It is never stored in shell history or visible in process listings.
    /// Keys are stored chain-agnostic — use `ergors node address` to derive
    /// chain-specific addresses at usage time.
    #[clap(display_order = 100)]
    ImportMnemonic {
        /// Human-readable label for this key (used as identifier)
        #[arg(long)]
        label: String,

        /// Mark this key as the default for deployments
        #[arg(long)]
        default: bool,
    },

    /// List all stored keys (shows labels, addresses, default status)
    #[clap(display_order = 200)]
    List {},

    /// Delete a key by label
    #[clap(display_order = 300)]
    Delete {
        /// The key label to delete
        #[arg(long)]
        label: String,
    },

    /// Set a key as the default for deployments
    #[clap(display_order = 400)]
    SetDefault {
        /// The key label to make default
        #[arg(long)]
        label: String,
    },
}

impl KeysCmd {
    pub fn exec(&self, home_dir: &Utf8Path, json: bool) -> Result<()> {
        let rt = tokio::runtime::Runtime::new()?;
        rt.block_on(self.exec_async(home_dir, json))
    }

    async fn exec_async(&self, home_dir: &Utf8Path, json: bool) -> Result<()> {
        // Try gRPC first (daemon might be running with storage lock)
        if let Ok(mut client) = ManagementClient::connect(&self.grpc_addr, None).await {
            return self.exec_via_grpc(&mut client, json).await;
        }

        // Fallback to direct storage access
        let data_dir = home_dir.join(DATA_FOLDER_NAME);
        let storage = ErgorsStorage::new(&data_dir, vec![])
            .await
            .map_err(|e| {
                // Give helpful error if lock is held
                let err_str = e.to_string();
                if err_str.contains("LOCK") || err_str.contains("Resource temporarily unavailable") {
                    anyhow!(
                        "Storage is locked by running engine. Either:\n\
                         1. Stop the engine: `ergors stop`\n\
                         2. Or the engine will handle this automatically via gRPC (check if it's healthy: `ergors status`)"
                    )
                } else {
                    anyhow!("Failed to open storage: {}", e)
                }
            })?;

        self.exec_via_storage(&storage, json).await
    }

    // ============ gRPC execution (daemon running) ============

    async fn exec_via_grpc(&self, client: &mut ManagementClient, json: bool) -> Result<()> {
        match &self.subcmd {
            KeysSubCmd::ImportMnemonic {
                label,
                default,
            } => {
                // Prompt for mnemonic (hidden input - never stored in history)
                let phrase = get_mnemonic()?;

                // When daemon is running, it uses its custody password for key encryption.
                // We pass empty string and the daemon handles it.
                let password = String::new();

                // Keys are chain-agnostic: use label as key_name, no chain_id, default prefix
                let resp = client
                    .import_cosmos_key(
                        &phrase,
                        label,
                        label, // use label as key_name
                        "",    // chain-agnostic
                        DEFAULT_ADDRESS_PREFIX,
                        *default,
                        &password,
                    )
                    .await?;

                if resp.success {
                    if let Some(key) = resp.key {
                        if json {
                            let resp = KeyImportResponse {
                                label: key.label.clone(),
                                address: key.address.clone(),
                                is_default: key.is_default,
                            };
                            println!("{}", serde_json::to_string_pretty(&resp)?);
                        } else {
                            println!("Key imported successfully:");
                            println!("  Label:   {}", key.label);
                            println!("  Address: {}", key.address);
                            println!("  Default: {}", if key.is_default { "yes" } else { "no" });
                        }
                    }
                } else {
                    return Err(anyhow!("Import failed: {}", resp.error_message));
                }
            }
            KeysSubCmd::List {} => {
                let keys = client.list_cosmos_keys().await?;

                if keys.is_empty() {
                    if json {
                        println!("{}", serde_json::to_string_pretty(&KeyListResponse { keys: vec![] })?);
                    } else {
                        println!("No keys stored.");
                    }
                    return Ok(());
                }

                if json {
                    let resp = KeyListResponse {
                        keys: keys
                            .iter()
                            .map(|k| KeyEntry {
                                label: k.label.clone(),
                                address: k.address.clone(),
                                is_default: k.is_default,
                            })
                            .collect(),
                    };
                    println!("{}", serde_json::to_string_pretty(&resp)?);
                } else {
                    println!(
                        "{:<20} {:<45} DEFAULT",
                        "LABEL", "ADDRESS"
                    );
                    println!("{}", "-".repeat(70));

                    for key in keys {
                        println!(
                            "{:<20} {:<45} {}",
                            if key.label.is_empty() { &key.key_name } else { &key.label },
                            key.address,
                            if key.is_default { "*" } else { "" },
                        );
                    }
                }
            }
            KeysSubCmd::Delete { label } => {
                let resp = client.delete_cosmos_key(label).await?;
                if resp.success {
                    println!("Key '{}' deleted.", label);
                } else {
                    return Err(anyhow!("{}", resp.message));
                }
            }
            KeysSubCmd::SetDefault { label } => {
                let resp = client.set_default_cosmos_key(label).await?;
                if resp.success {
                    println!("Key '{}' set as default.", label);
                } else {
                    return Err(anyhow!("{}", resp.message));
                }
            }
        }
        Ok(())
    }

    // ============ Direct storage execution (daemon not running) ============

    async fn exec_via_storage(&self, storage: &ErgorsStorage, json: bool) -> Result<()> {
        match &self.subcmd {
            KeysSubCmd::ImportMnemonic {
                label,
                default,
            } => {
                // Prompt for mnemonic (hidden input - never stored in history)
                let phrase = get_mnemonic()?;

                self.import_mnemonic_direct(
                    storage,
                    &phrase,
                    label,
                    *default,
                    json,
                )
                .await
            }
            KeysSubCmd::List {} => self.list_keys_direct(storage, json).await,
            KeysSubCmd::Delete { label } => self.delete_key_direct(storage, label).await,
            KeysSubCmd::SetDefault { label } => self.set_default_direct(storage, label).await,
        }
    }

    async fn import_mnemonic_direct(
        &self,
        storage: &ErgorsStorage,
        phrase: &str,
        label: &str,
        make_default: bool,
        json: bool,
    ) -> Result<()> {
        let password = get_password(true)?;

        // Load or create key store
        let mut store = match storage.get_cosmos_key_store().await {
            Ok(Some(s)) => s,
            Ok(None) => EncryptedCosmosKeyManager::create_empty_store(),
            Err(e) => return Err(anyhow!("Failed to load key store: {}", e)),
        };

        // Create key manager
        let mut manager = if store.keys.is_empty() {
            EncryptedCosmosKeyManager::new()
        } else {
            EncryptedCosmosKeyManager::from_store(&store)
        };

        // Unlock with password
        manager.unlock(&password)?;

        // Use label as key_name (chain-agnostic)
        let key_name = label;

        // Check for duplicate key name
        if store.keys.iter().any(|k| k.key_name == key_name) {
            return Err(anyhow!(
                "Key with label '{}' already exists. Use a different --label.",
                key_name
            ));
        }

        // Import and encrypt the mnemonic (chain-agnostic, default prefix)
        let (encrypted, account_info) = manager.import_mnemonic_with_label(
            key_name,
            phrase,
            "",  // chain-agnostic
            DEFAULT_ADDRESS_PREFIX,
            label,
            make_default,
        )?;

        // Check for duplicate address
        if EncryptedCosmosKeyManager::address_exists(&store, &account_info.address) {
            return Err(anyhow!(
                "Address {} already exists in the key store (duplicate mnemonic?)",
                account_info.address
            ));
        }

        // Add to store and persist
        manager.add_key_to_store(&mut store, encrypted, account_info.clone());
        storage
            .put_cosmos_key_store(&store)
            .await
            .map_err(|e| anyhow!("Failed to save key store: {}", e))?;

        if json {
            let resp = KeyImportResponse {
                label: label.to_string(),
                address: account_info.address.clone(),
                is_default: make_default,
            };
            println!("{}", serde_json::to_string_pretty(&resp)?);
        } else {
            println!("Key imported successfully:");
            println!("  Label:   {}", label);
            println!("  Address: {}", account_info.address);
            println!("  Default: {}", if make_default { "yes" } else { "no" });
        }

        Ok(())
    }

    async fn list_keys_direct(&self, storage: &ErgorsStorage, json: bool) -> Result<()> {
        let store = match storage.get_cosmos_key_store().await {
            Ok(Some(s)) => s,
            Ok(None) => {
                if json {
                    println!("{}", serde_json::to_string_pretty(&KeyListResponse { keys: vec![] })?);
                } else {
                    println!("No keys stored.");
                }
                return Ok(());
            }
            Err(e) => return Err(anyhow!("Failed to load key store: {}", e)),
        };

        if store.keys.is_empty() {
            if json {
                println!("{}", serde_json::to_string_pretty(&KeyListResponse { keys: vec![] })?);
            } else {
                println!("No keys stored.");
            }
            return Ok(());
        }

        let default_name = EncryptedCosmosKeyManager::get_default_key_name(&store);

        if json {
            let resp = KeyListResponse {
                keys: store
                    .keys
                    .iter()
                    .map(|key| {
                        let address = store
                            .derived_accounts
                            .iter()
                            .find(|a| a.key_name == key.key_name)
                            .map(|a| a.address.clone())
                            .unwrap_or_default();

                        KeyEntry {
                            label: if key.label.is_empty() {
                                key.key_name.clone()
                            } else {
                                key.label.clone()
                            },
                            address,
                            is_default: default_name == Some(key.key_name.as_str()),
                        }
                    })
                    .collect(),
            };
            println!("{}", serde_json::to_string_pretty(&resp)?);
        } else {
            println!(
                "{:<20} {:<45} DEFAULT",
                "LABEL", "ADDRESS"
            );
            println!("{}", "-".repeat(70));

            for key in &store.keys {
                let address = store
                    .derived_accounts
                    .iter()
                    .find(|a| a.key_name == key.key_name)
                    .map(|a| a.address.as_str())
                    .unwrap_or("(unknown)");

                let is_default = default_name == Some(key.key_name.as_str());
                let default_marker = if is_default { "*" } else { "" };

                println!(
                    "{:<20} {:<45} {}",
                    if key.label.is_empty() { &key.key_name } else { &key.label },
                    address,
                    default_marker,
                );
            }
        }

        Ok(())
    }

    async fn delete_key_direct(&self, storage: &ErgorsStorage, key_name: &str) -> Result<()> {
        let mut store = match storage.get_cosmos_key_store().await {
            Ok(Some(s)) => s,
            Ok(None) => return Err(anyhow!("No key store found")),
            Err(e) => return Err(anyhow!("Failed to load key store: {}", e)),
        };

        // Require password to delete keys (security measure)
        let password = get_password(false)?;
        let mut manager = EncryptedCosmosKeyManager::from_store(&store);
        manager.unlock(&password).map_err(|_| anyhow!("Invalid password. Delete aborted."))?;

        EncryptedCosmosKeyManager::delete_key(&mut store, key_name)?;

        storage
            .put_cosmos_key_store(&store)
            .await
            .map_err(|e| anyhow!("Failed to save key store: {}", e))?;

        println!("Key '{}' deleted.", key_name);
        Ok(())
    }

    async fn set_default_direct(&self, storage: &ErgorsStorage, key_name: &str) -> Result<()> {
        let mut store = match storage.get_cosmos_key_store().await {
            Ok(Some(s)) => s,
            Ok(None) => return Err(anyhow!("No key store found")),
            Err(e) => return Err(anyhow!("Failed to load key store: {}", e)),
        };

        EncryptedCosmosKeyManager::set_default_key(&mut store, key_name)?;

        storage
            .put_cosmos_key_store(&store)
            .await
            .map_err(|e| anyhow!("Failed to save key store: {}", e))?;

        println!("Key '{}' set as default.", key_name);
        Ok(())
    }
}

/// Get mnemonic phrase from environment or prompt (hidden input).
///
/// SECURITY: The mnemonic is never stored in shell history or visible in `ps`.
/// For automation, use ERGORS_MNEMONIC environment variable (also not in history
/// if set via `read -s` or similar).
pub fn get_mnemonic() -> Result<String> {
    // Check environment variable first (for scripting)
    if let Ok(env_mnemonic) = std::env::var("ERGORS_MNEMONIC") {
        if env_mnemonic.is_empty() {
            return Err(anyhow!("ERGORS_MNEMONIC is set but empty"));
        }
        // Clear the env var immediately after reading for extra security
        std::env::remove_var("ERGORS_MNEMONIC");
        return Ok(env_mnemonic);
    }

    // Prompt interactively (hidden input like password)
    let mnemonic = rpassword::prompt_password("Enter mnemonic (hidden): ")
        .map_err(|e| anyhow!("Failed to read mnemonic: {}", e))?;

    let mnemonic = mnemonic.trim().to_string();

    if mnemonic.is_empty() {
        return Err(anyhow!("Mnemonic cannot be empty"));
    }

    // Basic validation: should have 12, 15, 18, 21, or 24 words
    let word_count = mnemonic.split_whitespace().count();
    if ![12, 15, 18, 21, 24].contains(&word_count) {
        return Err(anyhow!(
            "Invalid mnemonic: expected 12, 15, 18, 21, or 24 words, got {}",
            word_count
        ));
    }

    Ok(mnemonic)
}

/// Get password from environment or prompt.
pub fn get_password(confirm: bool) -> Result<String> {
    // Check environment variable first
    if let Ok(env_password) = std::env::var("ERGORS_CUSTODY_PASSWORD") {
        if env_password.is_empty() {
            return Err(anyhow!("ERGORS_CUSTODY_PASSWORD is set but empty"));
        }
        return Ok(env_password);
    }

    // Prompt interactively
    let password = rpassword::prompt_password("Enter encryption password: ")
        .map_err(|e| anyhow!("Failed to read password: {}", e))?;

    if password.is_empty() {
        return Err(anyhow!("Password cannot be empty"));
    }

    if confirm {
        let confirmation = rpassword::prompt_password("Confirm password: ")
            .map_err(|e| anyhow!("Failed to read password confirmation: {}", e))?;

        if password != confirmation {
            return Err(anyhow!("Passwords do not match"));
        }
    }

    Ok(password)
}
