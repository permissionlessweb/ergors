//! Key Management CLI Commands
//!
//! Provides `ergors keys` subcommands for importing mnemonic seed phrases,
//! listing keys, setting defaults, and deleting keys.
//!
//! All keys are stored encrypted using Argon2id + ChaCha20Poly1305 via the
//! EncryptedCosmosKeyManager. Mnemonics are never persisted in plaintext.

use anyhow::{anyhow, Result};
use camino::Utf8Path;
use ho_std::keys::encrypted_cosmos::EncryptedCosmosKeyManager;

use crate::storage::ErgorsStorage;

/// CLI command for cosmos key management
#[derive(Debug, clap::Parser)]
pub struct KeysCmd {
    #[clap(subcommand)]
    pub subcmd: KeysSubCmd,
}

#[derive(Debug, Clone, clap::Subcommand)]
pub enum KeysSubCmd {
    /// Import a BIP-39 mnemonic seed phrase as a funding key
    #[clap(display_order = 100)]
    ImportMnemonic {
        /// The mnemonic phrase (24 words, space-separated)
        #[arg(long)]
        phrase: String,

        /// Human-readable label for this key
        #[arg(long)]
        label: String,

        /// Key name (internal identifier)
        #[arg(long, default_value = "default")]
        key_name: String,

        /// Chain ID (e.g. "akashnet-2")
        #[arg(long, default_value = "akashnet-2")]
        chain_id: String,

        /// Bech32 address prefix (e.g. "akash", "cosmos")
        #[arg(long, default_value = "akash")]
        address_prefix: String,

        /// Mark this key as the default for deployments
        #[arg(long)]
        make_default: bool,
    },

    /// List all stored keys (shows labels, addresses, default status)
    #[clap(display_order = 200)]
    List {},

    /// Delete a key by name
    #[clap(display_order = 300)]
    Delete {
        /// The key name to delete
        #[arg(long)]
        key_name: String,
    },

    /// Set a key as the default for deployments
    #[clap(display_order = 400)]
    SetDefault {
        /// The key name to make default
        #[arg(long)]
        key_name: String,
    },
}

impl KeysCmd {
    pub fn exec(&self, home_dir: &Utf8Path) -> Result<()> {
        // Create a tokio runtime for async storage operations
        let rt = tokio::runtime::Runtime::new()?;
        rt.block_on(self.exec_async(home_dir))
    }

    async fn exec_async(&self, home_dir: &Utf8Path) -> Result<()> {
        let data_dir = home_dir.join("data");

        // Open storage
        let storage = ErgorsStorage::new(&data_dir, vec![])
            .await
            .map_err(|e| anyhow!("Failed to open storage: {}", e))?;

        match &self.subcmd {
            KeysSubCmd::ImportMnemonic {
                phrase,
                label,
                key_name,
                chain_id,
                address_prefix,
                make_default,
            } => {
                self.import_mnemonic(
                    &storage,
                    phrase,
                    label,
                    key_name,
                    chain_id,
                    address_prefix,
                    *make_default,
                )
                .await
            }
            KeysSubCmd::List {} => self.list_keys(&storage).await,
            KeysSubCmd::Delete { key_name } => self.delete_key(&storage, key_name).await,
            KeysSubCmd::SetDefault { key_name } => self.set_default(&storage, key_name).await,
        }
    }

    async fn import_mnemonic(
        &self,
        storage: &ErgorsStorage,
        phrase: &str,
        label: &str,
        key_name: &str,
        chain_id: &str,
        address_prefix: &str,
        make_default: bool,
    ) -> Result<()> {
        // Prompt for password
        let password = rpassword::prompt_password("Enter encryption password: ")
            .map_err(|e| anyhow!("Failed to read password: {}", e))?;

        if password.is_empty() {
            return Err(anyhow!("Password cannot be empty"));
        }

        // Confirm password for new stores
        let confirm = rpassword::prompt_password("Confirm password: ")
            .map_err(|e| anyhow!("Failed to read password confirmation: {}", e))?;

        if password != confirm {
            return Err(anyhow!("Passwords do not match"));
        }

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

        // Check for duplicate key name
        if store.keys.iter().any(|k| k.key_name == key_name) {
            return Err(anyhow!(
                "Key with name '{}' already exists. Use a different --key-name.",
                key_name
            ));
        }

        // Import and encrypt the mnemonic
        let (encrypted, account_info) = manager.import_mnemonic_with_label(
            key_name,
            phrase,
            chain_id,
            address_prefix,
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

        println!("Key imported successfully:");
        println!("  Name:    {}", key_name);
        println!("  Label:   {}", label);
        println!("  Address: {}", account_info.address);
        println!("  Chain:   {}", chain_id);
        println!("  Default: {}", if make_default { "yes" } else { "no" });

        Ok(())
    }

    async fn list_keys(&self, storage: &ErgorsStorage) -> Result<()> {
        let store = match storage.get_cosmos_key_store().await {
            Ok(Some(s)) => s,
            Ok(None) => {
                println!("No keys stored.");
                return Ok(());
            }
            Err(e) => return Err(anyhow!("Failed to load key store: {}", e)),
        };

        if store.keys.is_empty() {
            println!("No keys stored.");
            return Ok(());
        }

        let default_name = EncryptedCosmosKeyManager::get_default_key_name(&store);

        println!("{:<15} {:<20} {:<45} {:<12} DEFAULT", "NAME", "LABEL", "ADDRESS", "CHAIN");
        println!("{}", "-".repeat(100));

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
                "{:<15} {:<20} {:<45} {:<12} {}",
                key.key_name,
                if key.label.is_empty() { "-" } else { &key.label },
                address,
                if key.chain_id.is_empty() { "-" } else { &key.chain_id },
                default_marker,
            );
        }

        Ok(())
    }

    async fn delete_key(&self, storage: &ErgorsStorage, key_name: &str) -> Result<()> {
        let mut store = match storage.get_cosmos_key_store().await {
            Ok(Some(s)) => s,
            Ok(None) => return Err(anyhow!("No key store found")),
            Err(e) => return Err(anyhow!("Failed to load key store: {}", e)),
        };

        EncryptedCosmosKeyManager::delete_key(&mut store, key_name)?;

        storage
            .put_cosmos_key_store(&store)
            .await
            .map_err(|e| anyhow!("Failed to save key store: {}", e))?;

        println!("Key '{}' deleted.", key_name);
        Ok(())
    }

    async fn set_default(&self, storage: &ErgorsStorage, key_name: &str) -> Result<()> {
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
