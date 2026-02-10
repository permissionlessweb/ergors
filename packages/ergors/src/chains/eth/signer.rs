//! Ethereum transaction signing via mnemonic-derived keys.
//!
//! Mirrors the Cosmos signing pattern in climb_signer.rs but for Ethereum
//! using the ethers crate. The same mnemonic stored in EncryptedCosmosKeyManager
//! derives Ethereum keys via derivation path m/44'/60'/0'/0/{index}.

use anyhow::{anyhow, Result};
use ethers::signers::{coins_bip39::English, MnemonicBuilder, LocalWallet, Signer};
use ethers::types::Address;
use ho_std::keys::encrypted_cosmos::EncryptedCosmosKeyManager;
use ho_std::types::ergors::orch::v1::CosmosKeyStore;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Default Ethereum derivation path prefix (BIP-44 coin type 60).
const ETH_DERIVATION_PREFIX: &str = "m/44'/60'/0'/0";

/// Create an ethers LocalWallet from a mnemonic phrase and account index.
///
/// Uses Ethereum derivation path: m/44'/60'/0'/0/{account_index}
pub fn create_eth_wallet(mnemonic: &str, account_index: u32) -> Result<LocalWallet> {
    let wallet = MnemonicBuilder::<English>::default()
        .phrase(mnemonic)
        .index(account_index)
        .map_err(|e| anyhow!("Failed to build wallet from mnemonic: {}", e))?
        .build()
        .map_err(|e| anyhow!("Failed to build wallet: {}", e))?;

    Ok(wallet)
}

/// Derive an Ethereum address from a mnemonic at the given account index.
///
/// Useful for checking balances or displaying addresses without creating
/// a full signing wallet.
pub fn derive_eth_address(mnemonic: &str, account_index: u32) -> Result<Address> {
    let wallet = create_eth_wallet(mnemonic, account_index)?;
    Ok(wallet.address())
}

/// Get an Ethereum address from the encrypted key store.
///
/// Decrypts the mnemonic from EncryptedCosmosKeyManager and derives
/// the Ethereum address at the given account index.
pub async fn eth_address_from_keystore(
    key_manager: Arc<RwLock<EncryptedCosmosKeyManager>>,
    key_store: Arc<RwLock<CosmosKeyStore>>,
    key_name: &str,
    account_index: u32,
) -> Result<Address> {
    let mnemonic = decrypt_mnemonic_from_store(key_manager, key_store, key_name).await?;
    derive_eth_address(&mnemonic, account_index)
}

/// Create a LocalWallet from the encrypted key store.
///
/// Decrypts the mnemonic and creates a signing wallet using the Ethereum
/// derivation path. The same mnemonic that signs Cosmos transactions can
/// sign Ethereum transactions via a different derivation path.
pub async fn eth_wallet_from_keystore(
    key_manager: Arc<RwLock<EncryptedCosmosKeyManager>>,
    key_store: Arc<RwLock<CosmosKeyStore>>,
    key_name: &str,
    account_index: u32,
) -> Result<LocalWallet> {
    let mnemonic = decrypt_mnemonic_from_store(key_manager, key_store, key_name).await?;
    create_eth_wallet(&mnemonic, account_index)
}

/// Decrypt the mnemonic from the key store (shared helper).
async fn decrypt_mnemonic_from_store(
    key_manager: Arc<RwLock<EncryptedCosmosKeyManager>>,
    key_store: Arc<RwLock<CosmosKeyStore>>,
    key_name: &str,
) -> Result<String> {
    let mut key_manager_guard = key_manager.write().await;
    let key_store_guard = key_store.read().await;

    if !key_manager_guard.is_unlocked() {
        return Err(anyhow!(
            "Key manager is locked - must unlock before signing"
        ));
    }

    let encrypted_key = key_store_guard
        .keys
        .iter()
        .find(|k| k.key_name == key_name)
        .ok_or_else(|| anyhow!("Key '{}' not found", key_name))?;

    key_manager_guard.decrypt_mnemonic(encrypted_key)
}

/// Format the full Ethereum derivation path for a given account index.
pub fn eth_derivation_path(account_index: u32) -> String {
    format!("{}/{}", ETH_DERIVATION_PREFIX, account_index)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Well-known test mnemonic (BIP-39 test vector).
    const TEST_MNEMONIC: &str =
        "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

    #[test]
    fn test_create_eth_wallet() {
        let wallet = create_eth_wallet(TEST_MNEMONIC, 0);
        assert!(wallet.is_ok(), "Should create wallet from valid mnemonic");
        let wallet = wallet.unwrap();
        assert_ne!(wallet.address(), Address::zero());
    }

    #[test]
    fn test_derive_eth_address() {
        let addr = derive_eth_address(TEST_MNEMONIC, 0).unwrap();
        assert_ne!(addr, Address::zero());

        // Different indices should produce different addresses
        let addr2 = derive_eth_address(TEST_MNEMONIC, 1).unwrap();
        assert_ne!(addr, addr2, "Different indices should yield different addresses");
    }

    #[test]
    fn test_derivation_path_format() {
        assert_eq!(eth_derivation_path(0), "m/44'/60'/0'/0/0");
        assert_eq!(eth_derivation_path(5), "m/44'/60'/0'/0/5");
    }

    #[test]
    fn test_invalid_mnemonic() {
        let result = create_eth_wallet("not a valid mnemonic", 0);
        assert!(result.is_err(), "Should reject invalid mnemonic");
    }

    #[test]
    fn test_deterministic_addresses() {
        let addr1 = derive_eth_address(TEST_MNEMONIC, 0).unwrap();
        let addr2 = derive_eth_address(TEST_MNEMONIC, 0).unwrap();
        assert_eq!(addr1, addr2, "Derivation should be deterministic");
    }
}
