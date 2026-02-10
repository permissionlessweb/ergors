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
use ho_std::constants::{DATA_FOLDER_NAME,DEFAULT_GRPC_ADDR};
use ho_std::keys::cosmos::cosmos_address_from_pubkey;
use ho_std::keys::encrypted_cosmos::EncryptedCosmosKeyManager;

use crate::client::ManagementClient;
use crate::commands::responses::{KeyEntry, KeyImportResponse, KeyListResponse};
use crate::storage::ErgorsStorage;

 

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
    /// Keys are stored with public key + default address, can derive
    /// chain-specific addresses at usage time.
    #[clap(display_order = 100)]
    ImportMnemonic {
        /// Human-readable label for this key (used as identifier)
        #[arg(long)]
        label: String,

        /// Mark this key as the default for deployments
        #[arg(long)]
        default: bool,

        /// Bech32 address prefix (ergo=Ergors, akash=Akash, cosmos=Cosmos Hub, etc.)
        #[arg(long, default_value = "ergo")]
        prefix: String,

        /// BIP-44 coin type for HD derivation path (118=Cosmos/Akash, 60=EVM, 330=Terra, 529=Secret)
        #[arg(long, default_value = "118")]
        coin_type: u32,
    },

    /// List all stored keys (shows labels, addresses, default status)
    ///
    /// Use --prefix to re-derive addresses with a different bech32 prefix
    /// from the stored public keys (e.g., show akash1 addresses instead of ergo1).
    /// Use --label to filter to a specific key, and -a/--address to output
    /// just the address string (useful for scripting).
    #[clap(display_order = 200)]
    List {
        /// Override bech32 prefix for displayed addresses (derives from stored public key)
        #[arg(long)]
        prefix: Option<String>,

        /// Filter by key label
        #[arg(long)]
        label: Option<String>,

        /// Output only the address string (for scripting)
        #[arg(short = 'a', long)]
        address: bool,
    },

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
                prefix,
                coin_type,
            } => {
                // Prompt for mnemonic (hidden input - never stored in history)
                let phrase = get_mnemonic()?;

                // When daemon is running, it uses its custody password for key encryption.
                // We pass empty string and the daemon handles it.
                let password = String::new();

                // Import with user-specified prefix and coin type
                let resp = client
                    .import_cosmos_key(
                        &phrase, label, label,  // use label as key_name
                        "",     // chain-agnostic
                        prefix, // user-specified prefix
                        *default, &password,
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
            KeysSubCmd::List {
                prefix,
                label: label_filter,
                address: address_only,
            } => {
                if prefix.is_some() {
                    eprintln!("Note: --prefix re-derivation requires direct storage access (stop daemon first)");
                    eprintln!("Showing stored addresses instead.");
                }

                let mut keys = client.list_cosmos_keys().await?;

                // Filter by label if specified
                if let Some(filter) = label_filter {
                    keys.retain(|k| {
                        let key_label = if k.label.is_empty() {
                            &k.key_name
                        } else {
                            &k.label
                        };
                        key_label == filter
                    });
                }

                // Address-only mode: output just the address string(s)
                if *address_only {
                    if keys.is_empty() {
                        return Err(anyhow!("No matching key found"));
                    }
                    for key in &keys {
                        println!("{}", key.address);
                    }
                    return Ok(());
                }

                if keys.is_empty() {
                    if json {
                        println!(
                            "{}",
                            serde_json::to_string_pretty(&KeyListResponse { keys: vec![] })?
                        );
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
                    println!("{:<20} {:<45} DEFAULT", "LABEL", "ADDRESS");
                    println!("{}", "-".repeat(70));

                    for key in keys {
                        println!(
                            "{:<20} {:<45} {}",
                            if key.label.is_empty() {
                                &key.key_name
                            } else {
                                &key.label
                            },
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
                prefix,
                coin_type,
            } => {
                // Prompt for mnemonic (hidden input - never stored in history)
                let phrase = get_mnemonic()?;

                self.import_mnemonic_direct(
                    storage, &phrase, label, *default, prefix, *coin_type, json,
                )
                .await
            }
            KeysSubCmd::List {
                prefix,
                label,
                address,
            } => {
                self.list_keys_direct(storage, prefix.as_deref(), label.as_deref(), *address, json)
                    .await
            }
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
        prefix: &str,
        coin_type: u32,
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

        // Import and encrypt the mnemonic with custom prefix and coin type
        let (encrypted, account_info) = manager.import_mnemonic_full(
            key_name,
            phrase,
            "",     // chain-agnostic
            prefix, // user-specified prefix
            label,
            make_default,
            coin_type,
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

    async fn list_keys_direct(
        &self,
        storage: &ErgorsStorage,
        prefix_override: Option<&str>,
        label_filter: Option<&str>,
        address_only: bool,
        json: bool,
    ) -> Result<()> {
        let store = match storage.get_cosmos_key_store().await {
            Ok(Some(s)) => s,
            Ok(None) => {
                if address_only {
                    return Err(anyhow!("No matching key found"));
                }
                if json {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&KeyListResponse { keys: vec![] })?
                    );
                } else {
                    println!("No keys stored.");
                }
                return Ok(());
            }
            Err(e) => return Err(anyhow!("Failed to load key store: {}", e)),
        };

        // Collect keys, applying label filter
        let keys: Vec<_> = store
            .keys
            .iter()
            .filter(|key| {
                if let Some(filter) = label_filter {
                    let key_label = if key.label.is_empty() {
                        &key.key_name
                    } else {
                        &key.label
                    };
                    key_label == filter
                } else {
                    true
                }
            })
            .collect();

        if keys.is_empty() {
            if address_only {
                return Err(anyhow!("No matching key found"));
            }
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&KeyListResponse { keys: vec![] })?
                );
            } else {
                println!("No keys stored.");
            }
            return Ok(());
        }

        let default_name = EncryptedCosmosKeyManager::get_default_key_name(&store);

        // Resolve address for a key, applying prefix override if set
        let resolve_address =
            |key: &ho_std::types::ergors::orch::v1::EncryptedCosmosMnemonic| -> String {
                let account = store
                    .derived_accounts
                    .iter()
                    .find(|a| a.key_name == key.key_name);

                match (prefix_override, account) {
                    (Some(pfx), Some(a)) => {
                        rederive_address(&a.public_key, pfx).unwrap_or_else(|_| a.address.clone())
                    }
                    (_, Some(a)) => a.address.clone(),
                    _ => "(unknown)".to_string(),
                }
            };

        // Address-only mode: output just the address string(s)
        if address_only {
            for key in &keys {
                println!("{}", resolve_address(key));
            }
            return Ok(());
        }

        if json {
            let resp = KeyListResponse {
                keys: keys
                    .iter()
                    .map(|key| KeyEntry {
                        label: if key.label.is_empty() {
                            key.key_name.clone()
                        } else {
                            key.label.clone()
                        },
                        address: resolve_address(key),
                        is_default: default_name == Some(key.key_name.as_str()),
                    })
                    .collect(),
            };
            println!("{}", serde_json::to_string_pretty(&resp)?);
        } else {
            println!("{:<20} {:<45} DEFAULT", "LABEL", "ADDRESS");
            println!("{}", "-".repeat(70));

            for key in &keys {
                let is_default = default_name == Some(key.key_name.as_str());
                println!(
                    "{:<20} {:<45} {}",
                    if key.label.is_empty() {
                        &key.key_name
                    } else {
                        &key.label
                    },
                    resolve_address(key),
                    if is_default { "*" } else { "" },
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
        manager
            .unlock(&password)
            .map_err(|_| anyhow!("Invalid password. Delete aborted."))?;

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

/// Re-derive a bech32 address from a stored public key with a different prefix.
///
/// The public key bytes are stored in derived_accounts. This function
/// re-encodes them with the given bech32 prefix, allowing runtime
/// address derivation without needing the mnemonic.
fn rederive_address(public_key: &[u8], prefix: &str) -> Result<String> {
    cosmos_address_from_pubkey(public_key, prefix)
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
