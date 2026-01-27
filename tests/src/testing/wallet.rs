//! Test Wallet Manager
//!
//! Manages pre-funded test accounts for integration testing.
//! Supports authz grants, feegrants, and balance management.

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info};

/// Test wallet configuration
#[derive(Debug, Clone)]
pub struct TestWalletConfig {
    /// Chain ID for the test network
    pub chain_id: String,
    /// Node REST endpoint
    pub rest_endpoint: String,
    /// Default funding amount in uakt
    pub default_funding_uakt: u64,
    /// Keyring backend (test, file, os)
    pub keyring_backend: String,
}

impl Default for TestWalletConfig {
    fn default() -> Self {
        Self {
            chain_id: "localakash".to_string(),
            rest_endpoint: "http://localhost:1317".to_string(),
            default_funding_uakt: 100_000_000_000, // 100k AKT
            keyring_backend: "test".to_string(),
        }
    }
}

/// Test wallet with balance and grant tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestWallet {
    /// Wallet name/alias
    pub name: String,
    /// Bech32 address
    pub address: String,
    /// Public key (hex encoded)
    pub pubkey: String,
    /// Current balance in uakt
    pub balance_uakt: u64,
    /// Mnemonic (only for test wallets)
    pub mnemonic: Option<String>,
    /// HD derivation path
    pub hd_path: String,
    /// Authz grants given to this wallet
    pub authz_grants_received: Vec<AuthzGrant>,
    /// Authz grants given by this wallet
    pub authz_grants_given: Vec<AuthzGrant>,
    /// Feegrant allowances received
    pub feegrants_received: Vec<FeegrantAllowance>,
    /// Feegrant allowances given
    pub feegrants_given: Vec<FeegrantAllowance>,
}

/// Authz grant record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthzGrant {
    pub granter: String,
    pub grantee: String,
    pub msg_type: String,
    pub expiration: Option<String>,
}

/// Feegrant allowance record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeegrantAllowance {
    pub granter: String,
    pub grantee: String,
    pub allowance_type: String,
    pub spend_limit: Option<u64>,
    pub expiration: Option<String>,
}

/// Role-based test wallet presets
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WalletRole {
    /// Faucet wallet with unlimited funds
    Faucet,
    /// Deployer with sufficient funds
    Deployer,
    /// Granter node that provides authz/feegrants
    Granter,
    /// Grantee node that requests grants
    Grantee,
    /// Provider wallet for Akash providers
    Provider,
    /// Validator wallet
    Validator,
    /// Custom role
    Custom,
}

impl WalletRole {
    /// Get default funding amount for role
    pub fn default_funding(&self) -> u64 {
        match self {
            WalletRole::Faucet => 1_000_000_000_000_000, // 1B AKT
            WalletRole::Deployer => 100_000_000_000,     // 100k AKT
            WalletRole::Granter => 500_000_000_000,      // 500k AKT
            WalletRole::Grantee => 1_000_000,            // 1 AKT (needs grants)
            WalletRole::Provider => 100_000_000_000,     // 100k AKT
            WalletRole::Validator => 10_000_000_000,     // 10k AKT
            WalletRole::Custom => 10_000_000_000,        // 10k AKT
        }
    }

    /// Get wallet name prefix
    pub fn name_prefix(&self) -> &'static str {
        match self {
            WalletRole::Faucet => "faucet",
            WalletRole::Deployer => "deployer",
            WalletRole::Granter => "granter",
            WalletRole::Grantee => "grantee",
            WalletRole::Provider => "provider",
            WalletRole::Validator => "validator",
            WalletRole::Custom => "wallet",
        }
    }
}

/// Test wallet manager
///
/// Manages test wallets with pre-funded balances, authz grants,
/// and feegrant allowances for integration testing.
pub struct TestWalletManager {
    config: TestWalletConfig,
    wallets: Arc<RwLock<HashMap<String, TestWallet>>>,
    wallet_counter: Arc<RwLock<u32>>,
}

impl TestWalletManager {
    /// Create a new wallet manager with default configuration
    pub fn new() -> Self {
        Self::with_config(TestWalletConfig::default())
    }

    /// Create with custom configuration
    pub fn with_config(config: TestWalletConfig) -> Self {
        Self {
            config,
            wallets: Arc::new(RwLock::new(HashMap::new())),
            wallet_counter: Arc::new(RwLock::new(0)),
        }
    }

    /// Initialize standard test wallets
    pub async fn init_standard_wallets(&self) -> Result<()> {
        info!("Initializing standard test wallets...");

        // Create standard wallets for each role
        let roles = [
            WalletRole::Faucet,
            WalletRole::Deployer,
            WalletRole::Granter,
            WalletRole::Grantee,
            WalletRole::Provider,
        ];

        for role in roles {
            self.create_wallet_with_role(role).await?;
        }

        info!("Standard test wallets initialized");
        Ok(())
    }

    /// Create a wallet with a specific role
    pub async fn create_wallet_with_role(&self, role: WalletRole) -> Result<TestWallet> {
        let mut counter = self.wallet_counter.write().await;
        *counter += 1;
        let index = *counter;

        let name = format!("{}_{}", role.name_prefix(), index);
        let funding = role.default_funding();

        self.create_wallet(&name, funding).await
    }

    /// Create a new test wallet
    pub async fn create_wallet(&self, name: &str, funding_uakt: u64) -> Result<TestWallet> {
        info!("Creating test wallet '{}' with {} uAKT", name, funding_uakt);

        // Generate deterministic test mnemonic based on name
        let mnemonic = generate_test_mnemonic(name);

        // Derive address from mnemonic (simplified for testing)
        let (address, pubkey) = derive_test_address(name);

        let wallet = TestWallet {
            name: name.to_string(),
            address: address.clone(),
            pubkey,
            balance_uakt: funding_uakt,
            mnemonic: Some(mnemonic),
            hd_path: "m/44'/118'/0'/0/0".to_string(),
            authz_grants_received: Vec::new(),
            authz_grants_given: Vec::new(),
            feegrants_received: Vec::new(),
            feegrants_given: Vec::new(),
        };

        self.wallets
            .write()
            .await
            .insert(name.to_string(), wallet.clone());

        debug!("Created wallet '{}' at address {}", name, address);
        Ok(wallet)
    }

    /// Get a wallet by name
    pub async fn get_wallet(&self, name: &str) -> Option<TestWallet> {
        self.wallets.read().await.get(name).cloned()
    }

    /// Get a wallet by address
    pub async fn get_wallet_by_address(&self, address: &str) -> Option<TestWallet> {
        self.wallets
            .read()
            .await
            .values()
            .find(|w| w.address == address)
            .cloned()
    }

    /// List all wallets
    pub async fn list_wallets(&self) -> Vec<TestWallet> {
        self.wallets.read().await.values().cloned().collect()
    }

    /// Fund a wallet
    pub async fn fund_wallet(&self, name: &str, amount_uakt: u64) -> Result<()> {
        let mut wallets = self.wallets.write().await;
        let wallet = wallets
            .get_mut(name)
            .ok_or_else(|| anyhow!("Wallet '{}' not found", name))?;

        wallet.balance_uakt += amount_uakt;
        info!(
            "Funded wallet '{}' with {} uAKT (new balance: {})",
            name, amount_uakt, wallet.balance_uakt
        );

        Ok(())
    }

    /// Deduct from wallet balance
    pub async fn deduct_balance(&self, name: &str, amount_uakt: u64) -> Result<()> {
        let mut wallets = self.wallets.write().await;
        let wallet = wallets
            .get_mut(name)
            .ok_or_else(|| anyhow!("Wallet '{}' not found", name))?;

        if wallet.balance_uakt < amount_uakt {
            return Err(anyhow!(
                "Insufficient balance: {} < {}",
                wallet.balance_uakt,
                amount_uakt
            ));
        }

        wallet.balance_uakt -= amount_uakt;
        debug!(
            "Deducted {} uAKT from '{}' (new balance: {})",
            amount_uakt, name, wallet.balance_uakt
        );

        Ok(())
    }

    /// Create an authz grant between wallets
    pub async fn create_authz_grant(
        &self,
        granter_name: &str,
        grantee_name: &str,
        msg_type: &str,
        expiration: Option<&str>,
    ) -> Result<AuthzGrant> {
        let mut wallets = self.wallets.write().await;

        let granter_address = wallets
            .get(granter_name)
            .ok_or_else(|| anyhow!("Granter '{}' not found", granter_name))?
            .address
            .clone();

        let grantee_address = wallets
            .get(grantee_name)
            .ok_or_else(|| anyhow!("Grantee '{}' not found", grantee_name))?
            .address
            .clone();

        let grant = AuthzGrant {
            granter: granter_address.clone(),
            grantee: grantee_address.clone(),
            msg_type: msg_type.to_string(),
            expiration: expiration.map(String::from),
        };

        // Add to granter's given grants
        if let Some(granter) = wallets.get_mut(granter_name) {
            granter.authz_grants_given.push(grant.clone());
        }

        // Add to grantee's received grants
        if let Some(grantee) = wallets.get_mut(grantee_name) {
            grantee.authz_grants_received.push(grant.clone());
        }

        info!(
            "Created authz grant: {} -> {} for {}",
            granter_name, grantee_name, msg_type
        );

        Ok(grant)
    }

    /// Create authz grants for Akash deployment operations
    pub async fn create_akash_deployment_grants(
        &self,
        granter_name: &str,
        grantee_name: &str,
    ) -> Result<Vec<AuthzGrant>> {
        let msg_types = [
            "/akash.deployment.v1beta3.MsgCreateDeployment",
            "/akash.deployment.v1beta3.MsgUpdateDeployment",
            "/akash.deployment.v1beta3.MsgCloseDeployment",
            "/akash.market.v1beta4.MsgCreateLease",
        ];

        let mut grants = Vec::new();
        for msg_type in msg_types {
            let grant = self
                .create_authz_grant(granter_name, grantee_name, msg_type, None)
                .await?;
            grants.push(grant);
        }

        Ok(grants)
    }

    /// Create a feegrant allowance between wallets
    pub async fn create_feegrant(
        &self,
        granter_name: &str,
        grantee_name: &str,
        spend_limit_uakt: Option<u64>,
        expiration: Option<&str>,
    ) -> Result<FeegrantAllowance> {
        let mut wallets = self.wallets.write().await;

        let granter_address = wallets
            .get(granter_name)
            .ok_or_else(|| anyhow!("Granter '{}' not found", granter_name))?
            .address
            .clone();

        let grantee_address = wallets
            .get(grantee_name)
            .ok_or_else(|| anyhow!("Grantee '{}' not found", grantee_name))?
            .address
            .clone();

        let allowance_type = if spend_limit_uakt.is_some() {
            "BasicAllowance"
        } else {
            "AllowedMsgAllowance"
        };

        let allowance = FeegrantAllowance {
            granter: granter_address.clone(),
            grantee: grantee_address.clone(),
            allowance_type: allowance_type.to_string(),
            spend_limit: spend_limit_uakt,
            expiration: expiration.map(String::from),
        };

        // Add to granter's given allowances
        if let Some(granter) = wallets.get_mut(granter_name) {
            granter.feegrants_given.push(allowance.clone());
        }

        // Add to grantee's received allowances
        if let Some(grantee) = wallets.get_mut(grantee_name) {
            grantee.feegrants_received.push(allowance.clone());
        }

        info!(
            "Created feegrant: {} -> {} (limit: {:?} uAKT)",
            granter_name, grantee_name, spend_limit_uakt
        );

        Ok(allowance)
    }

    /// Check if grantee has authz permission for a message type
    pub async fn has_authz_permission(&self, grantee_name: &str, msg_type: &str) -> bool {
        if let Some(wallet) = self.wallets.read().await.get(grantee_name) {
            wallet
                .authz_grants_received
                .iter()
                .any(|g| g.msg_type == msg_type)
        } else {
            false
        }
    }

    /// Check if grantee has feegrant allowance
    pub async fn has_feegrant(&self, grantee_name: &str) -> bool {
        if let Some(wallet) = self.wallets.read().await.get(grantee_name) {
            !wallet.feegrants_received.is_empty()
        } else {
            false
        }
    }

    /// Get available feegrant spend limit for grantee
    pub async fn get_feegrant_limit(&self, grantee_name: &str) -> Option<u64> {
        self.wallets
            .read()
            .await
            .get(grantee_name)
            .and_then(|w| w.feegrants_received.first())
            .and_then(|f| f.spend_limit)
    }

    /// Revoke an authz grant
    pub async fn revoke_authz_grant(
        &self,
        granter_name: &str,
        grantee_name: &str,
        msg_type: &str,
    ) -> Result<()> {
        let mut wallets = self.wallets.write().await;

        // Remove from granter's given grants
        if let Some(granter) = wallets.get_mut(granter_name) {
            granter
                .authz_grants_given
                .retain(|g| !(g.msg_type == msg_type && g.grantee == grantee_name));
        }

        // Remove from grantee's received grants
        if let Some(grantee) = wallets.get_mut(grantee_name) {
            grantee
                .authz_grants_received
                .retain(|g| !(g.msg_type == msg_type && g.granter == granter_name));
        }

        info!(
            "Revoked authz grant: {} -> {} for {}",
            granter_name, grantee_name, msg_type
        );

        Ok(())
    }

    /// Revoke a feegrant allowance
    pub async fn revoke_feegrant(&self, granter_name: &str, grantee_name: &str) -> Result<()> {
        let mut wallets = self.wallets.write().await;

        // Remove from granter's given allowances
        if let Some(granter) = wallets.get_mut(granter_name) {
            granter
                .feegrants_given
                .retain(|f| f.grantee != grantee_name);
        }

        // Remove from grantee's received allowances
        if let Some(grantee) = wallets.get_mut(grantee_name) {
            grantee
                .feegrants_received
                .retain(|f| f.granter != granter_name);
        }

        info!("Revoked feegrant: {} -> {}", granter_name, grantee_name);

        Ok(())
    }

    /// Get chain configuration
    pub fn config(&self) -> &TestWalletConfig {
        &self.config
    }

    /// Clear all wallets
    pub async fn clear(&self) {
        self.wallets.write().await.clear();
        *self.wallet_counter.write().await = 0;
    }
}

impl Default for TestWalletManager {
    fn default() -> Self {
        Self::new()
    }
}

// ==================== Helper Functions ====================

/// Generate a deterministic test mnemonic based on wallet name
fn generate_test_mnemonic(name: &str) -> String {
    // Use a deterministic but valid 24-word mnemonic for testing
    // In production, this would use proper BIP-39 generation
    let base_words = [
        "abandon", "ability", "able", "about", "above", "absent", "absorb", "abstract",
        "absurd", "abuse", "access", "accident", "account", "accuse", "achieve", "acid",
        "acoustic", "acquire", "across", "act", "action", "actor", "actress", "actual",
    ];

    // Create deterministic variation based on name
    let hash = simple_hash(name);
    let offset = (hash % 10) as usize;

    let mut words = base_words.to_vec();
    // Rotate words based on hash
    words.rotate_left(offset);

    words.join(" ")
}

/// Derive test address from wallet name (simplified for testing)
fn derive_test_address(name: &str) -> (String, String) {
    // Generate deterministic but valid-looking addresses
    let hash = simple_hash(name);

    // Create bech32-like address
    let address_bytes: Vec<u8> = (0..20)
        .map(|i| ((hash >> (i % 8)) & 0xFF) as u8)
        .collect();
    let address = format!("akash1{}", hex::encode(&address_bytes));

    // Create pubkey
    let pubkey_bytes: Vec<u8> = (0..33)
        .map(|i| ((hash >> ((i + 5) % 8)) & 0xFF) as u8)
        .collect();
    let pubkey = hex::encode(&pubkey_bytes);

    (address, pubkey)
}

/// Simple hash function for deterministic test data
fn simple_hash(s: &str) -> u64 {
    let mut hash: u64 = 5381;
    for c in s.bytes() {
        hash = hash.wrapping_mul(33).wrapping_add(c as u64);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_wallet_creation() {
        let manager = TestWalletManager::new();
        let wallet = manager.create_wallet("test1", 1_000_000).await.unwrap();

        assert_eq!(wallet.name, "test1");
        assert_eq!(wallet.balance_uakt, 1_000_000);
        assert!(wallet.address.starts_with("akash1"));
    }

    #[tokio::test]
    async fn test_wallet_funding() {
        let manager = TestWalletManager::new();
        manager.create_wallet("test1", 1_000_000).await.unwrap();

        manager.fund_wallet("test1", 500_000).await.unwrap();

        let wallet = manager.get_wallet("test1").await.unwrap();
        assert_eq!(wallet.balance_uakt, 1_500_000);
    }

    #[tokio::test]
    async fn test_authz_grant_creation() {
        let manager = TestWalletManager::new();
        manager.create_wallet("granter", 100_000_000).await.unwrap();
        manager.create_wallet("grantee", 1_000_000).await.unwrap();

        let grant = manager
            .create_authz_grant(
                "granter",
                "grantee",
                "/akash.deployment.v1beta3.MsgCreateDeployment",
                None,
            )
            .await
            .unwrap();

        assert!(manager
            .has_authz_permission("grantee", "/akash.deployment.v1beta3.MsgCreateDeployment")
            .await);

        let grantee = manager.get_wallet("grantee").await.unwrap();
        assert_eq!(grantee.authz_grants_received.len(), 1);
    }

    #[tokio::test]
    async fn test_feegrant_creation() {
        let manager = TestWalletManager::new();
        manager.create_wallet("granter", 100_000_000).await.unwrap();
        manager.create_wallet("grantee", 1_000_000).await.unwrap();

        manager
            .create_feegrant("granter", "grantee", Some(5_000_000), None)
            .await
            .unwrap();

        assert!(manager.has_feegrant("grantee").await);
        assert_eq!(manager.get_feegrant_limit("grantee").await, Some(5_000_000));
    }

    #[tokio::test]
    async fn test_wallet_role_funding() {
        assert_eq!(WalletRole::Faucet.default_funding(), 1_000_000_000_000_000);
        assert_eq!(WalletRole::Grantee.default_funding(), 1_000_000);
    }

    #[test]
    fn test_deterministic_mnemonic() {
        let m1 = generate_test_mnemonic("test");
        let m2 = generate_test_mnemonic("test");
        let m3 = generate_test_mnemonic("other");

        assert_eq!(m1, m2);
        assert_ne!(m1, m3);
    }

    #[test]
    fn test_deterministic_address() {
        let (addr1, _) = derive_test_address("test");
        let (addr2, _) = derive_test_address("test");
        let (addr3, _) = derive_test_address("other");

        assert_eq!(addr1, addr2);
        assert_ne!(addr1, addr3);
        assert!(addr1.starts_with("akash1"));
    }
}
