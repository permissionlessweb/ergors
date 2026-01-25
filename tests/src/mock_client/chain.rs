//! Mock Cosmos Chain for Testing
//!
//! Simulates blockchain state including balances, authz grants, and feegrants
//! without requiring actual chain infrastructure or cw-multitest.

use super::types::*;
use anyhow::{anyhow, Result};
use std::collections::HashMap;

/// Mock blockchain state for testing.
///
/// Tracks accounts, balances, authz grants, feegrants, and Akash-specific state
/// like deployments, bids, and leases.
pub struct MockCosmosChain {
    accounts: HashMap<String, MockAccount>,
    authz_grants: Vec<MockAuthzGrant>,
    feegrants: Vec<MockFeegrant>,
    deployments: HashMap<u64, MockDeployment>,
    bids: Vec<MockBid>,
    leases: Vec<MockLease>,
    next_dseq: u64,
    block_height: u64,
    block_time_unix: u64,
}

impl Default for MockCosmosChain {
    fn default() -> Self {
        Self::new()
    }
}

impl MockCosmosChain {
    /// Create a new mock chain.
    pub fn new() -> Self {
        Self {
            accounts: HashMap::new(),
            authz_grants: Vec::new(),
            feegrants: Vec::new(),
            deployments: HashMap::new(),
            bids: Vec::new(),
            leases: Vec::new(),
            next_dseq: 1,
            block_height: 1,
            block_time_unix: chrono::Utc::now().timestamp() as u64,
        }
    }

    /// Advance block height (useful for expiration testing).
    pub fn advance_block(&mut self) {
        self.block_height += 1;
        self.block_time_unix += 6; // ~6 second blocks
    }

    /// Advance time by specified seconds.
    pub fn advance_time(&mut self, seconds: u64) {
        self.block_time_unix += seconds;
        self.block_height += seconds / 6;
    }

    /// Get current block height.
    pub fn block_height(&self) -> u64 {
        self.block_height
    }

    /// Get current block time.
    pub fn block_time(&self) -> u64 {
        self.block_time_unix
    }

    // =========================================================================
    // Account Management
    // =========================================================================

    /// Create or get account.
    pub fn create_account(&mut self, address: impl Into<String>) -> &mut MockAccount {
        let address = address.into();
        self.accounts
            .entry(address.clone())
            .or_insert_with(|| MockAccount::new(address))
    }

    /// Fund an account with tokens.
    pub fn fund_account(&mut self, address: impl Into<String>, amount: u64) {
        self.fund_account_denom(address, "uakt", amount);
    }

    /// Fund account with specific denom.
    pub fn fund_account_denom(
        &mut self,
        address: impl Into<String>,
        denom: impl Into<String>,
        amount: u64,
    ) {
        let account = self.create_account(address);
        let denom = denom.into();
        *account.balances.entry(denom).or_insert(0) += amount;
    }

    /// Get balance for address.
    pub fn get_balance(&self, address: &str, denom: &str) -> u64 {
        self.accounts
            .get(address)
            .and_then(|a| a.balances.get(denom))
            .copied()
            .unwrap_or(0)
    }

    /// Transfer tokens between accounts.
    pub fn bank_send(
        &mut self,
        from: &str,
        to: &str,
        denom: &str,
        amount: u64,
    ) -> Result<()> {
        let from_balance = self.get_balance(from, denom);
        if from_balance < amount {
            return Err(anyhow!(
                "Insufficient balance: have {}, need {}",
                from_balance,
                amount
            ));
        }

        // Deduct from sender
        if let Some(account) = self.accounts.get_mut(from) {
            if let Some(bal) = account.balances.get_mut(denom) {
                *bal -= amount;
            }
        }

        // Credit to receiver
        self.fund_account_denom(to, denom, amount);

        Ok(())
    }

    // =========================================================================
    // Authz Grants
    // =========================================================================

    /// Grant authz permission.
    pub fn grant_authz(
        &mut self,
        granter: &str,
        grantee: &str,
        msg_type: &str,
        duration_seconds: u64,
    ) -> Result<()> {
        // Ensure granter exists
        self.create_account(granter);
        self.create_account(grantee);

        // Check for existing grant and revoke it
        self.authz_grants
            .retain(|g| !(g.granter == granter && g.grantee == grantee && g.msg_type_url == msg_type));

        let grant = MockAuthzGrant {
            granter: granter.to_string(),
            grantee: grantee.to_string(),
            msg_type_url: msg_type.to_string(),
            expiration_unix: self.block_time_unix + duration_seconds,
            active: true,
        };

        self.authz_grants.push(grant);
        Ok(())
    }

    /// Revoke authz permission.
    pub fn revoke_authz(&mut self, granter: &str, grantee: &str, msg_type: &str) -> Result<()> {
        let found = self.authz_grants.iter_mut().find(|g| {
            g.granter == granter && g.grantee == grantee && g.msg_type_url == msg_type && g.active
        });

        match found {
            Some(grant) => {
                grant.active = false;
                Ok(())
            }
            None => Err(anyhow!("No active grant found to revoke")),
        }
    }

    /// Query authz grants for a granter/grantee pair.
    pub fn query_authz_grants(&self, granter: &str, grantee: &str) -> Vec<&MockAuthzGrant> {
        self.authz_grants
            .iter()
            .filter(|g| {
                g.granter == granter
                    && g.grantee == grantee
                    && g.active
                    && g.expiration_unix > self.block_time_unix
            })
            .collect()
    }

    /// Check if authz grant exists for msg type.
    pub fn has_authz(&self, granter: &str, grantee: &str, msg_type: &str) -> bool {
        self.authz_grants.iter().any(|g| {
            g.granter == granter
                && g.grantee == grantee
                && g.msg_type_url == msg_type
                && g.active
                && g.expiration_unix > self.block_time_unix
        })
    }

    // =========================================================================
    // Feegrants
    // =========================================================================

    /// Create feegrant allowance.
    pub fn create_feegrant(
        &mut self,
        granter: &str,
        grantee: &str,
        spend_limit_uakt: u64,
        duration_seconds: u64,
    ) -> Result<()> {
        // Ensure accounts exist
        self.create_account(granter);
        self.create_account(grantee);

        // Remove existing feegrant if any
        self.feegrants
            .retain(|f| !(f.granter == granter && f.grantee == grantee));

        let feegrant = MockFeegrant {
            granter: granter.to_string(),
            grantee: grantee.to_string(),
            spend_limit_uakt,
            spent_uakt: 0,
            expiration_unix: self.block_time_unix + duration_seconds,
            allowed_messages: Vec::new(), // All messages allowed
            active: true,
        };

        self.feegrants.push(feegrant);
        Ok(())
    }

    /// Revoke feegrant.
    pub fn revoke_feegrant(&mut self, granter: &str, grantee: &str) -> Result<()> {
        let found = self
            .feegrants
            .iter_mut()
            .find(|f| f.granter == granter && f.grantee == grantee && f.active);

        match found {
            Some(fg) => {
                fg.active = false;
                Ok(())
            }
            None => Err(anyhow!("No active feegrant found to revoke")),
        }
    }

    /// Query feegrant.
    pub fn query_feegrant(&self, granter: &str, grantee: &str) -> Option<&MockFeegrant> {
        self.feegrants.iter().find(|f| {
            f.granter == granter
                && f.grantee == grantee
                && f.active
                && f.expiration_unix > self.block_time_unix
        })
    }

    /// Check if feegrant exists and has remaining allowance.
    pub fn has_feegrant(&self, granter: &str, grantee: &str) -> bool {
        self.feegrants.iter().any(|f| {
            f.granter == granter
                && f.grantee == grantee
                && f.active
                && f.expiration_unix > self.block_time_unix
                && f.spent_uakt < f.spend_limit_uakt
        })
    }

    /// Use feegrant to pay fees (simulates fee deduction via granter).
    pub fn use_feegrant(&mut self, granter: &str, grantee: &str, fee_uakt: u64) -> Result<()> {
        let fg = self
            .feegrants
            .iter_mut()
            .find(|f| {
                f.granter == granter
                    && f.grantee == grantee
                    && f.active
                    && f.expiration_unix > self.block_time_unix
            })
            .ok_or_else(|| anyhow!("No valid feegrant found"))?;

        let remaining = fg.spend_limit_uakt.saturating_sub(fg.spent_uakt);
        if remaining < fee_uakt {
            return Err(anyhow!(
                "Feegrant allowance exceeded: remaining {}, need {}",
                remaining,
                fee_uakt
            ));
        }

        fg.spent_uakt += fee_uakt;

        // Deduct from granter's balance
        self.bank_send(granter, "fee_collector", "uakt", fee_uakt)?;

        Ok(())
    }

    // =========================================================================
    // Akash Deployment Simulation
    // =========================================================================

    /// Create a deployment.
    pub fn create_deployment(&mut self, owner: &str) -> Result<MockDeployment> {
        let dseq = self.next_dseq;
        self.next_dseq += 1;

        let deployment = MockDeployment {
            dseq,
            owner: owner.to_string(),
            state: DeploymentState::Open,
            created_at_unix: self.block_time_unix,
        };

        self.deployments.insert(dseq, deployment.clone());
        Ok(deployment)
    }

    /// Get deployment by dseq.
    pub fn get_deployment(&self, dseq: u64) -> Option<&MockDeployment> {
        self.deployments.get(&dseq)
    }

    /// Close a deployment.
    pub fn close_deployment(&mut self, dseq: u64) -> Result<()> {
        let deployment = self
            .deployments
            .get_mut(&dseq)
            .ok_or_else(|| anyhow!("Deployment not found: {}", dseq))?;

        deployment.state = DeploymentState::Closed;

        // Close any active leases
        for lease in &mut self.leases {
            if lease.dseq == dseq {
                lease.state = LeaseState::Closed;
            }
        }

        Ok(())
    }

    /// Submit a bid (simulates provider bidding).
    pub fn submit_bid(&mut self, dseq: u64, provider: &str, price_uakt: u64) -> Result<MockBid> {
        // Check deployment exists and is open
        let deployment = self
            .deployments
            .get(&dseq)
            .ok_or_else(|| anyhow!("Deployment not found: {}", dseq))?;

        if deployment.state != DeploymentState::Open {
            return Err(anyhow!("Deployment not open for bids"));
        }

        let bid = MockBid {
            dseq,
            provider: provider.to_string(),
            price_uakt,
            state: BidState::Open,
        };

        self.bids.push(bid.clone());
        Ok(bid)
    }

    /// Query bids for a deployment.
    pub fn query_bids(&self, dseq: u64) -> Vec<&MockBid> {
        self.bids
            .iter()
            .filter(|b| b.dseq == dseq && b.state == BidState::Open)
            .collect()
    }

    /// Create lease by accepting a bid.
    pub fn create_lease(&mut self, dseq: u64, provider: &str) -> Result<MockLease> {
        // Find and match the bid
        let bid = self
            .bids
            .iter_mut()
            .find(|b| b.dseq == dseq && b.provider == provider && b.state == BidState::Open)
            .ok_or_else(|| anyhow!("No open bid found for provider"))?;

        bid.state = BidState::Matched;

        // Update deployment state
        let deployment = self
            .deployments
            .get_mut(&dseq)
            .ok_or_else(|| anyhow!("Deployment not found"))?;

        deployment.state = DeploymentState::Active;

        // Create lease
        let lease = MockLease {
            dseq,
            provider: provider.to_string(),
            owner: deployment.owner.clone(),
            price_uakt: bid.price_uakt,
            state: LeaseState::Active,
        };

        self.leases.push(lease.clone());
        Ok(lease)
    }

    /// Get lease for deployment.
    pub fn get_lease(&self, dseq: u64) -> Option<&MockLease> {
        self.leases
            .iter()
            .find(|l| l.dseq == dseq && l.state == LeaseState::Active)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_account_funding() {
        let mut chain = MockCosmosChain::new();
        chain.fund_account("akash1test", 1_000_000);

        assert_eq!(chain.get_balance("akash1test", "uakt"), 1_000_000);
        assert_eq!(chain.get_balance("akash1test", "uatom"), 0);
    }

    #[test]
    fn test_bank_send() {
        let mut chain = MockCosmosChain::new();
        chain.fund_account("akash1from", 1_000_000);

        chain.bank_send("akash1from", "akash1to", "uakt", 500_000).unwrap();

        assert_eq!(chain.get_balance("akash1from", "uakt"), 500_000);
        assert_eq!(chain.get_balance("akash1to", "uakt"), 500_000);
    }

    #[test]
    fn test_bank_send_insufficient() {
        let mut chain = MockCosmosChain::new();
        chain.fund_account("akash1from", 100);

        let result = chain.bank_send("akash1from", "akash1to", "uakt", 500);
        assert!(result.is_err());
    }

    #[test]
    fn test_authz_grant() {
        let mut chain = MockCosmosChain::new();
        chain
            .grant_authz("akash1granter", "akash1grantee", "/akash.deployment.v1beta3.MsgCreateDeployment", 86400)
            .unwrap();

        assert!(chain.has_authz(
            "akash1granter",
            "akash1grantee",
            "/akash.deployment.v1beta3.MsgCreateDeployment"
        ));

        let grants = chain.query_authz_grants("akash1granter", "akash1grantee");
        assert_eq!(grants.len(), 1);
    }

    #[test]
    fn test_authz_revoke() {
        let mut chain = MockCosmosChain::new();
        let msg = "/akash.deployment.v1beta3.MsgCreateDeployment";
        chain.grant_authz("akash1granter", "akash1grantee", msg, 86400).unwrap();

        assert!(chain.has_authz("akash1granter", "akash1grantee", msg));

        chain.revoke_authz("akash1granter", "akash1grantee", msg).unwrap();

        assert!(!chain.has_authz("akash1granter", "akash1grantee", msg));
    }

    #[test]
    fn test_feegrant() {
        let mut chain = MockCosmosChain::new();
        chain.fund_account("akash1granter", 10_000_000);
        chain
            .create_feegrant("akash1granter", "akash1grantee", 1_000_000, 86400)
            .unwrap();

        assert!(chain.has_feegrant("akash1granter", "akash1grantee"));

        let fg = chain.query_feegrant("akash1granter", "akash1grantee").unwrap();
        assert_eq!(fg.spend_limit_uakt, 1_000_000);
        assert_eq!(fg.spent_uakt, 0);
    }

    #[test]
    fn test_feegrant_usage() {
        let mut chain = MockCosmosChain::new();
        chain.fund_account("akash1granter", 10_000_000);
        chain
            .create_feegrant("akash1granter", "akash1grantee", 1_000_000, 86400)
            .unwrap();

        chain.use_feegrant("akash1granter", "akash1grantee", 100_000).unwrap();

        let fg = chain.query_feegrant("akash1granter", "akash1grantee").unwrap();
        assert_eq!(fg.spent_uakt, 100_000);
    }

    #[test]
    fn test_deployment_workflow() {
        let mut chain = MockCosmosChain::new();
        chain.fund_account("akash1owner", 10_000_000);
        chain.fund_account("akash1provider", 0);

        // Create deployment
        let deployment = chain.create_deployment("akash1owner").unwrap();
        assert_eq!(deployment.state, DeploymentState::Open);

        // Provider submits bid
        let bid = chain.submit_bid(deployment.dseq, "akash1provider", 1000).unwrap();
        assert_eq!(bid.state, BidState::Open);

        // Query bids
        let bids = chain.query_bids(deployment.dseq);
        assert_eq!(bids.len(), 1);

        // Create lease
        let lease = chain.create_lease(deployment.dseq, "akash1provider").unwrap();
        assert_eq!(lease.state, LeaseState::Active);

        // Check deployment is now active
        let deployment = chain.get_deployment(deployment.dseq).unwrap();
        assert_eq!(deployment.state, DeploymentState::Active);
    }

    #[test]
    fn test_time_advance() {
        let mut chain = MockCosmosChain::new();
        let initial_time = chain.block_time();
        let initial_height = chain.block_height();

        chain.advance_time(60);

        assert_eq!(chain.block_time(), initial_time + 60);
        assert_eq!(chain.block_height(), initial_height + 10); // 60/6 = 10 blocks
    }

    #[test]
    fn test_grant_expiration() {
        let mut chain = MockCosmosChain::new();
        let msg = "/akash.deployment.v1beta3.MsgCreateDeployment";

        // Grant for 60 seconds
        chain.grant_authz("akash1granter", "akash1grantee", msg, 60).unwrap();
        assert!(chain.has_authz("akash1granter", "akash1grantee", msg));

        // Advance past expiration
        chain.advance_time(120);
        assert!(!chain.has_authz("akash1granter", "akash1grantee", msg));
    }
}
