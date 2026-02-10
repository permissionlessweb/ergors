//! TestBackend: AkashBackend implementation for testing.
//!
//! Backs the real `DeploymentWorkflow` state machine with `MockCosmosChain` data,
//! enabling tests to exercise actual workflow logic instead of reimplementing it.

use super::chain::MockCosmosChain;
use akash_deploy_rs::{
    AkashBackend, Bid, BidId, CertificateInfo, DeployError, DeploymentState, EscrowInfo, LeaseId,
    LeaseInfo, ProviderInfo, ProviderLeaseStatus, Resources, ServiceEndpoint, TxResult,
};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// Configuration for a mock provider that will bid on deployments.
#[derive(Debug, Clone)]
pub struct MockProviderConfig {
    pub address: String,
    pub host_uri: String,
    pub bid_price_uakt: u64,
    pub auto_bid: bool,
}

/// Failure injection configuration.
#[derive(Debug, Clone)]
pub struct FailureConfig {
    /// Error message to return.
    pub message: String,
    /// Number of failures remaining. None = permanent.
    pub remaining: Option<u32>,
}

/// AkashBackend implementation backed by MockCosmosChain.
///
/// Supports failure injection for testing error paths.
pub struct TestBackend {
    chain: Arc<RwLock<MockCosmosChain>>,
    states: Arc<RwLock<HashMap<String, DeploymentState>>>,
    cert_keys: Arc<RwLock<HashMap<String, Vec<u8>>>>,
    certs: Arc<RwLock<HashMap<String, CertificateInfo>>>,
    provider_configs: Arc<RwLock<Vec<MockProviderConfig>>>,
    provider_cache: Arc<RwLock<HashMap<String, ProviderInfo>>>,
    failure_configs: Arc<RwLock<HashMap<String, FailureConfig>>>,
}

impl TestBackend {
    /// Create a new TestBackend wrapping the given chain.
    pub fn new(chain: Arc<RwLock<MockCosmosChain>>) -> Self {
        let providers = vec![MockProviderConfig {
            address: "akash1provider0testxyz".to_string(),
            host_uri: "https://provider0.test.akash.network:8443".to_string(),
            bid_price_uakt: 1000,
            auto_bid: true,
        }];

        Self {
            chain,
            states: Arc::new(RwLock::new(HashMap::new())),
            cert_keys: Arc::new(RwLock::new(HashMap::new())),
            certs: Arc::new(RwLock::new(HashMap::new())),
            provider_configs: Arc::new(RwLock::new(providers)),
            provider_cache: Arc::new(RwLock::new(HashMap::new())),
            failure_configs: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Add a mock provider.
    pub fn add_provider(&self, config: MockProviderConfig) {
        self.provider_configs.write().unwrap().push(config);
    }

    /// Inject a failure for a specific backend method.
    ///
    /// `method` should match one of: `query_balance`, `query_certificate`,
    /// `query_bids`, `broadcast_create_certificate`, `broadcast_create_deployment`,
    /// `broadcast_create_lease`, `send_manifest`, `query_provider_status`,
    /// `query_provider_info`.
    pub fn inject_failure(&self, method: &str, config: FailureConfig) {
        self.failure_configs
            .write()
            .unwrap()
            .insert(method.to_string(), config);
    }

    /// Clear a failure injection.
    pub fn clear_failure(&self, method: &str) {
        self.failure_configs.write().unwrap().remove(method);
    }

    /// Clear all failure injections.
    pub fn clear_all_failures(&self) {
        self.failure_configs.write().unwrap().clear();
    }

    /// Direct access to chain for pre-workflow operations (key selection, balance check).
    pub fn create_account(&self, address: &str) {
        self.chain.write().unwrap().create_account(address);
    }

    /// Get balance without failure injection (for pre-workflow balance check).
    pub fn get_balance(&self, address: &str, denom: &str) -> u64 {
        self.chain.read().unwrap().get_balance(address, denom)
    }

    /// Check and consume a failure if configured. Returns Err if should fail.
    fn check_failure(&self, method: &str) -> Result<(), DeployError> {
        let mut configs = self.failure_configs.write().unwrap();
        if let Some(config) = configs.get_mut(method) {
            let msg = config.message.clone();
            match &mut config.remaining {
                Some(0) => {
                    // No more failures, remove and succeed
                    configs.remove(method);
                    return Ok(());
                }
                Some(n) => {
                    *n -= 1;
                }
                None => {
                    // Permanent failure
                }
            }
            return Err(DeployError::Query(msg));
        }
        Ok(())
    }

    /// Get reference to the provider configs.
    pub fn provider_configs(&self) -> Arc<RwLock<Vec<MockProviderConfig>>> {
        Arc::clone(&self.provider_configs)
    }
}

/// Unit signer type for testing - no real signing needed.
pub struct TestSigner;

impl AkashBackend for TestBackend {
    type Signer = TestSigner;

    // ═══════════════════════════════════════════════════════════════
    // CHAIN QUERIES
    // ═══════════════════════════════════════════════════════════════

    async fn query_balance(&self, address: &str, denom: &str) -> Result<u128, DeployError> {
        self.check_failure("query_balance")?;
        let chain = self.chain.read().unwrap();
        Ok(chain.get_balance(address, denom) as u128)
    }

    async fn query_certificate(
        &self,
        address: &str,
    ) -> Result<Option<CertificateInfo>, DeployError> {
        self.check_failure("query_certificate")?;
        let certs = self.certs.read().unwrap();
        Ok(certs.get(address).cloned())
    }

    async fn query_provider_info(
        &self,
        provider: &str,
    ) -> Result<Option<ProviderInfo>, DeployError> {
        self.check_failure("query_provider_info")?;

        // Check cache first
        {
            let cache = self.provider_cache.read().unwrap();
            if let Some(info) = cache.get(provider) {
                return Ok(Some(info.clone()));
            }
        }

        // Look up from configured providers
        let configs = self.provider_configs.read().unwrap();
        let info = configs
            .iter()
            .find(|p| p.address == provider)
            .map(|p| ProviderInfo {
                address: p.address.clone(),
                host_uri: p.host_uri.clone(),
                email: format!("provider@{}.test", p.address),
                website: String::new(),
                attributes: Vec::new(),
                cached_at: 0,
            });

        Ok(info)
    }

    async fn query_bids(&self, owner: &str, dseq: u64) -> Result<Vec<Bid>, DeployError> {
        self.check_failure("query_bids")?;

        // Auto-submit bids from mock providers before querying
        {
            let configs = self.provider_configs.read().unwrap();
            let mut chain = self.chain.write().unwrap();
            for provider in configs.iter() {
                if provider.auto_bid {
                    // Ignore errors (e.g., bid already submitted)
                    let _ = chain.submit_bid(dseq, &provider.address, provider.bid_price_uakt);
                }
            }
        }

        let chain = self.chain.read().unwrap();
        let mock_bids = chain.query_bids(dseq);

        let bids: Vec<Bid> = mock_bids
            .into_iter()
            .map(|b| Bid {
                provider: b.provider.clone(),
                price_uakt: b.price_uakt,
                resources: Resources::default(),
            })
            .collect();

        // Filter by owner - the MockCosmosChain doesn't filter by owner in query_bids,
        // but we only return bids for deployments owned by this address
        let deployment_owned = chain
            .get_deployment(dseq)
            .map(|d| d.owner == owner)
            .unwrap_or(false);

        if deployment_owned {
            Ok(bids)
        } else {
            Ok(Vec::new())
        }
    }

    async fn query_lease(
        &self,
        _owner: &str,
        dseq: u64,
        _gseq: u32,
        _oseq: u32,
        _bseq: u32,
        _provider: &str,
    ) -> Result<LeaseInfo, DeployError> {
        self.check_failure("query_lease")?;
        let chain = self.chain.read().unwrap();
        let lease = chain
            .get_lease(dseq)
            .ok_or_else(|| DeployError::Query(format!("lease not found for dseq {}", dseq)))?;

        Ok(LeaseInfo {
            state: akash_deploy_rs::LeaseState::Active,
            price_uakt: lease.price_uakt,
        })
    }

    async fn query_escrow(&self, _owner: &str, _dseq: u64) -> Result<EscrowInfo, DeployError> {
        self.check_failure("query_escrow")?;
        Ok(EscrowInfo {
            balance_uakt: 5_000_000,
            deposited_uakt: 5_000_000,
        })
    }

    // ═══════════════════════════════════════════════════════════════
    // TRANSACTIONS
    // ═══════════════════════════════════════════════════════════════

    async fn broadcast_create_certificate(
        &self,
        _signer: &Self::Signer,
        owner: &str,
        cert_pem: &[u8],
        _pubkey_pem: &[u8],
    ) -> Result<TxResult, DeployError> {
        self.check_failure("broadcast_create_certificate")?;

        // Store cert in our in-memory store
        let cert_info = CertificateInfo {
            cert_pem: cert_pem.to_vec(),
            serial: format!("mock-serial-{}", owner),
        };
        self.certs
            .write()
            .unwrap()
            .insert(owner.to_string(), cert_info);

        Ok(TxResult {
            hash: format!(
                "CERT_TX_{}",
                hex::encode(&owner.as_bytes()[..8.min(owner.len())])
            ),
            code: 0,
            raw_log: String::new(),
            height: 100,
        })
    }

    async fn broadcast_create_deployment(
        &self,
        _signer: &Self::Signer,
        owner: &str,
        _sdl_content: &str,
        _deposit_uakt: u64,
    ) -> Result<(TxResult, u64), DeployError> {
        self.check_failure("broadcast_create_deployment")?;

        let mut chain = self.chain.write().unwrap();
        let deployment = chain
            .create_deployment(owner)
            .map_err(|e| DeployError::Transaction {
                code: 1,
                log: e.to_string(),
            })?;

        let dseq = deployment.dseq;
        Ok((
            TxResult {
                hash: format!("DEPLOY_TX_{}", dseq),
                code: 0,
                raw_log: String::new(),
                height: 101,
            },
            dseq,
        ))
    }

    async fn broadcast_create_lease(
        &self,
        _signer: &Self::Signer,
        bid: &BidId,
    ) -> Result<TxResult, DeployError> {
        self.check_failure("broadcast_create_lease")?;

        let mut chain = self.chain.write().unwrap();
        chain
            .create_lease(bid.dseq, &bid.provider)
            .map_err(|e| DeployError::Transaction {
                code: 1,
                log: e.to_string(),
            })?;

        Ok(TxResult {
            hash: format!("LEASE_TX_{}_{}", bid.dseq, bid.provider),
            code: 0,
            raw_log: String::new(),
            height: 102,
        })
    }

    async fn broadcast_deposit(
        &self,
        _signer: &Self::Signer,
        _owner: &str,
        _dseq: u64,
        _amount_uakt: u64,
    ) -> Result<TxResult, DeployError> {
        self.check_failure("broadcast_deposit")?;
        Ok(TxResult {
            hash: "DEPOSIT_TX".to_string(),
            code: 0,
            raw_log: String::new(),
            height: 103,
        })
    }

    async fn broadcast_close_deployment(
        &self,
        _signer: &Self::Signer,
        _owner: &str,
        dseq: u64,
    ) -> Result<TxResult, DeployError> {
        self.check_failure("broadcast_close_deployment")?;

        let mut chain = self.chain.write().unwrap();
        chain
            .close_deployment(dseq)
            .map_err(|e| DeployError::Transaction {
                code: 1,
                log: e.to_string(),
            })?;

        Ok(TxResult {
            hash: format!("CLOSE_TX_{}", dseq),
            code: 0,
            raw_log: String::new(),
            height: 104,
        })
    }

    // ═══════════════════════════════════════════════════════════════
    // PROVIDER COMMUNICATION
    // ═══════════════════════════════════════════════════════════════

    async fn send_manifest(
        &self,
        _provider_uri: &str,
        _lease: &LeaseId,
        _manifest: &[u8],
        _cert_pem: &[u8],
        _key_pem: &[u8],
    ) -> Result<(), DeployError> {
        self.check_failure("send_manifest")?;

        // No-op for unit tests - manifest structure validated by integration tests
        // (tests/scripts/jwt-verify validates against actual Go provider code)
        Ok(())
    }

    async fn query_provider_status(
        &self,
        _provider_uri: &str,
        _lease: &LeaseId,
        _cert_pem: &[u8],
        _key_pem: &[u8],
    ) -> Result<ProviderLeaseStatus, DeployError> {
        self.check_failure("query_provider_status")?;
        Ok(ProviderLeaseStatus {
            ready: true,
            endpoints: vec![
                ServiceEndpoint {
                    service: "web".to_string(),
                    uri: "http://mock-endpoint.akash.network".to_string(),
                    port: 80,
                },
                ServiceEndpoint {
                    service: "web".to_string(),
                    uri: "https://mock-endpoint.akash.network".to_string(),
                    port: 443,
                },
            ],
        })
    }

    // ═══════════════════════════════════════════════════════════════
    // STATE PERSISTENCE
    // ═══════════════════════════════════════════════════════════════

    async fn load_state(&self, session_id: &str) -> Result<Option<DeploymentState>, DeployError> {
        let states = self.states.read().unwrap();
        Ok(states.get(session_id).cloned())
    }

    async fn save_state(
        &self,
        session_id: &str,
        state: &DeploymentState,
    ) -> Result<(), DeployError> {
        self.states
            .write()
            .unwrap()
            .insert(session_id.to_string(), state.clone());
        Ok(())
    }

    // ═══════════════════════════════════════════════════════════════
    // CERTIFICATE KEY STORAGE
    // ═══════════════════════════════════════════════════════════════

    async fn load_cert_key(&self, owner: &str) -> Result<Option<Vec<u8>>, DeployError> {
        let keys = self.cert_keys.read().unwrap();
        Ok(keys.get(owner).cloned())
    }

    async fn save_cert_key(&self, owner: &str, key: &[u8]) -> Result<(), DeployError> {
        self.cert_keys
            .write()
            .unwrap()
            .insert(owner.to_string(), key.to_vec());
        Ok(())
    }

    async fn delete_cert_key(&self, owner: &str) -> Result<(), DeployError> {
        self.cert_keys.write().unwrap().remove(owner);
        Ok(())
    }

    // ═══════════════════════════════════════════════════════════════
    // PROVIDER INFO CACHE
    // ═══════════════════════════════════════════════════════════════

    async fn load_cached_provider(
        &self,
        provider: &str,
    ) -> Result<Option<ProviderInfo>, DeployError> {
        let cache = self.provider_cache.read().unwrap();
        Ok(cache.get(provider).cloned())
    }

    async fn cache_provider(&self, info: &ProviderInfo) -> Result<(), DeployError> {
        self.provider_cache
            .write()
            .unwrap()
            .insert(info.address.clone(), info.clone());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_backend() -> (TestBackend, Arc<RwLock<MockCosmosChain>>) {
        let chain = Arc::new(RwLock::new(MockCosmosChain::new()));
        let backend = TestBackend::new(Arc::clone(&chain));
        (backend, chain)
    }

    #[tokio::test]
    async fn test_query_balance_delegates_to_chain() {
        let (backend, chain) = make_backend();
        chain.write().unwrap().fund_account("akash1test", 1_000_000);

        let balance = backend.query_balance("akash1test", "uakt").await.unwrap();
        assert_eq!(balance, 1_000_000);
    }

    #[tokio::test]
    async fn test_query_balance_unfunded() {
        let (backend, _chain) = make_backend();
        let balance = backend.query_balance("akash1nobody", "uakt").await.unwrap();
        assert_eq!(balance, 0);
    }

    #[tokio::test]
    async fn test_broadcast_create_deployment() {
        let (backend, chain) = make_backend();
        chain
            .write()
            .unwrap()
            .fund_account("akash1owner", 1_000_000);

        let signer = TestSigner;
        let (tx, dseq) = backend
            .broadcast_create_deployment(&signer, "akash1owner", "version: 2.0", 5_000_000)
            .await
            .unwrap();

        assert!(tx.is_success());
        assert!(dseq > 0);

        // Chain should have the deployment
        let chain_r = chain.read().unwrap();
        assert!(chain_r.get_deployment(dseq).is_some());
    }

    #[tokio::test]
    async fn test_query_bids_auto_submits() {
        let (backend, chain) = make_backend();
        {
            let mut c = chain.write().unwrap();
            c.fund_account("akash1owner", 1_000_000);
            c.create_deployment("akash1owner").unwrap();
        }

        let bids = backend.query_bids("akash1owner", 1).await.unwrap();
        assert!(!bids.is_empty());
        assert_eq!(bids[0].provider, "akash1provider0testxyz");
    }

    #[tokio::test]
    async fn test_certificate_roundtrip() {
        let (backend, _chain) = make_backend();
        let signer = TestSigner;

        // No cert initially
        let cert = backend.query_certificate("akash1owner").await.unwrap();
        assert!(cert.is_none());

        // Create cert
        let tx = backend
            .broadcast_create_certificate(&signer, "akash1owner", b"cert-pem", b"pub-pem")
            .await
            .unwrap();
        assert!(tx.is_success());

        // Now cert should exist
        let cert = backend.query_certificate("akash1owner").await.unwrap();
        assert!(cert.is_some());
        assert_eq!(cert.unwrap().cert_pem, b"cert-pem");
    }

    #[tokio::test]
    async fn test_cert_key_roundtrip() {
        let (backend, _chain) = make_backend();

        assert!(backend
            .load_cert_key("akash1owner")
            .await
            .unwrap()
            .is_none());

        backend
            .save_cert_key("akash1owner", b"secret-key")
            .await
            .unwrap();

        let key = backend.load_cert_key("akash1owner").await.unwrap();
        assert_eq!(key, Some(b"secret-key".to_vec()));

        backend.delete_cert_key("akash1owner").await.unwrap();
        assert!(backend
            .load_cert_key("akash1owner")
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn test_failure_injection_permanent() {
        let (backend, _chain) = make_backend();
        backend.inject_failure(
            "query_balance",
            FailureConfig {
                message: "node unreachable".to_string(),
                remaining: None,
            },
        );

        let result = backend.query_balance("akash1test", "uakt").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("node unreachable"));

        // Still fails
        let result = backend.query_balance("akash1test", "uakt").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_failure_injection_transient() {
        let (backend, chain) = make_backend();
        chain.write().unwrap().fund_account("akash1test", 1000);

        backend.inject_failure(
            "query_balance",
            FailureConfig {
                message: "transient error".to_string(),
                remaining: Some(2),
            },
        );

        // First two calls fail
        assert!(backend.query_balance("akash1test", "uakt").await.is_err());
        assert!(backend.query_balance("akash1test", "uakt").await.is_err());

        // Third call succeeds
        let balance = backend.query_balance("akash1test", "uakt").await.unwrap();
        assert_eq!(balance, 1000);
    }

    #[tokio::test]
    async fn test_state_persistence() {
        let (backend, _chain) = make_backend();

        let state = DeploymentState::new("session-1", "akash1owner").with_sdl("version: 2.0");

        backend.save_state("session-1", &state).await.unwrap();

        let loaded = backend.load_state("session-1").await.unwrap();
        assert!(loaded.is_some());
        let loaded = loaded.unwrap();
        assert_eq!(loaded.session_id, "session-1");
        assert_eq!(loaded.owner, "akash1owner");
    }

    #[tokio::test]
    async fn test_create_lease_flow() {
        let (backend, chain) = make_backend();
        {
            let mut c = chain.write().unwrap();
            c.fund_account("akash1owner", 1_000_000);
            c.create_deployment("akash1owner").unwrap();
            c.submit_bid(1, "akash1provider0testxyz", 1000).unwrap();
        }

        let signer = TestSigner;
        let bid_id = BidId {
            owner: "akash1owner".to_string(),
            dseq: 1,
            gseq: 1,
            oseq: 1,
            provider: "akash1provider0testxyz".to_string(),
        };

        let tx = backend
            .broadcast_create_lease(&signer, &bid_id)
            .await
            .unwrap();
        assert!(tx.is_success());

        // Verify lease exists on chain
        let chain_r = chain.read().unwrap();
        assert!(chain_r.get_lease(1).is_some());
    }
}
