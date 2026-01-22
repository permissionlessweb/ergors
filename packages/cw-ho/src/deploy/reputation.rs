//! Provider Reputation System for Akash Deployments
//!
//! This module provides:
//! - Trusted provider list management
//! - Reputation scoring based on deployment history
//! - Provider selection algorithms
//! - Bid filtering and ranking

use anyhow::{anyhow, Result};
use ho_std::types::ergors::orch::v1::AkashProviderReputation;
use pbjson_types::Timestamp;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::SystemTime;

/// Minimum reputation score to be considered for deployments
pub const MIN_REPUTATION_SCORE: u32 = 50;

/// Default reputation score for new providers
pub const DEFAULT_REPUTATION_SCORE: u32 = 75;

/// Maximum reputation score
pub const MAX_REPUTATION_SCORE: u32 = 100;

/// Weight factors for reputation calculation
pub mod weights {
    pub const UPTIME_WEIGHT: f64 = 0.30;
    pub const SUCCESS_RATE_WEIGHT: f64 = 0.35;
    pub const RESPONSE_TIME_WEIGHT: f64 = 0.20;
    pub const TRUSTED_BONUS: f64 = 0.15;
}

/// Hardcoded trusted providers (fallback when contract unavailable)
pub const TRUSTED_PROVIDERS: &[&str] = &[
    "akash1u5cdg7k3gl43mukca4aeultuz8x2j68mgwn28e", // d3akash
    "akash1h4h33c8rv8e084el7e74f7pktz27pmxxt8nl9q", // overclock
    "akash15ksejj7g4su7ljufsg0a8eglvkje94z8qsh68a", // palmito
    "akash1kqzpqqhm39umt06wu8m4hx63v5hefhrfmjf9dj", // leet.haus
    "akash16yrzlu9cgxcf4d7k6qjax5fd3cll05p87qha4m", // dsm.hh
    "akash1efge8vzg376fnnfeyg5v8tdq9sg3elhgy42wvm", // marzrock
    "akash1tweev0k42guyv3a2jtgphmgfrl2h5y2884vh9d", // dcnorse
    "akash18ga02jzaq8cw52anyhzkwta5wygufgu6zsz6xc", // europlots
    "akash17l0f3kf7gv4kmgqjmgc0ksj3em6lqgcc4kl4dg", // pcgameservers
    "akash1ut3m97h62tty06qdq9lds85r34dxe3snjj0xfe", // akashgpu.com
];

/// Provider bid information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderBid {
    pub provider_address: String,
    pub price_uakt: u64,
    pub dseq: u64,
    pub gseq: u32,
    pub oseq: u32,
}

/// Selection criteria for provider ranking
#[derive(Debug, Clone, Default)]
pub struct SelectionCriteria {
    /// Minimum reputation score (0-100)
    pub min_reputation: Option<u32>,
    /// Maximum price in uakt
    pub max_price_uakt: Option<u64>,
    /// Only select from trusted providers
    pub trusted_only: bool,
    /// Minimum uptime percentage
    pub min_uptime_percent: Option<u32>,
    /// Weight for price vs reputation (0.0 = price only, 1.0 = reputation only)
    pub reputation_weight: f64,
}

/// Ranked provider with score
#[derive(Debug, Clone)]
pub struct RankedProvider {
    pub address: String,
    pub reputation: AkashProviderReputation,
    pub bid: Option<ProviderBid>,
    pub combined_score: f64,
}

/// Provider reputation manager
pub struct ProviderReputationManager {
    /// Cached reputation data
    reputations: HashMap<String, AkashProviderReputation>,
    /// Trusted provider list
    trusted_providers: Vec<String>,
}

impl ProviderReputationManager {
    pub fn new() -> Self {
        Self {
            reputations: HashMap::new(),
            trusted_providers: TRUSTED_PROVIDERS.iter().map(|s| s.to_string()).collect(),
        }
    }

    /// Create with custom trusted provider list
    pub fn with_trusted_providers(trusted: Vec<String>) -> Self {
        Self {
            reputations: HashMap::new(),
            trusted_providers: trusted,
        }
    }

    /// Check if a provider is in the trusted list
    pub fn is_trusted(&self, provider: &str) -> bool {
        self.trusted_providers.contains(&provider.to_string())
    }

    /// Get trusted providers list
    pub fn trusted_providers(&self) -> &[String] {
        &self.trusted_providers
    }

    /// Add a provider to trusted list
    pub fn add_trusted_provider(&mut self, provider: &str) {
        if !self.is_trusted(provider) {
            self.trusted_providers.push(provider.to_string());
        }
    }

    /// Remove a provider from trusted list
    pub fn remove_trusted_provider(&mut self, provider: &str) {
        self.trusted_providers.retain(|p| p != provider);
    }

    /// Get reputation for a provider
    pub fn get_reputation(&self, provider: &str) -> Option<&AkashProviderReputation> {
        self.reputations.get(provider)
    }

    /// Set reputation for a provider
    pub fn set_reputation(&mut self, reputation: AkashProviderReputation) {
        self.reputations.insert(reputation.provider_address.clone(), reputation);
    }

    /// Create default reputation for a new provider
    pub fn create_default_reputation(&self, provider: &str) -> AkashProviderReputation {
        let now = current_timestamp();
        let is_trusted = self.is_trusted(provider);

        AkashProviderReputation {
            provider_address: provider.to_string(),
            score: if is_trusted { 90 } else { DEFAULT_REPUTATION_SCORE },
            successful_deployments: 0,
            failed_deployments: 0,
            avg_uptime_percent: 100,
            avg_response_time_ms: 0,
            is_trusted,
            updated_at: Some(now),
        }
    }

    /// Calculate reputation score from metrics
    pub fn calculate_score(
        &self,
        successful: u64,
        failed: u64,
        uptime_percent: u32,
        avg_response_ms: u64,
        is_trusted: bool,
    ) -> u32 {
        let total_deployments = successful + failed;

        // Success rate component (0-1)
        let success_rate = if total_deployments > 0 {
            successful as f64 / total_deployments as f64
        } else {
            1.0 // New provider gets benefit of doubt
        };

        // Uptime component (0-1)
        let uptime_factor = uptime_percent as f64 / 100.0;

        // Response time component (0-1, lower is better)
        // 0ms = 1.0, 5000ms+ = 0.0
        let response_factor = if avg_response_ms == 0 {
            1.0
        } else {
            (5000.0 - avg_response_ms.min(5000) as f64) / 5000.0
        };

        // Trusted bonus (0 or 0.15)
        let trusted_bonus = if is_trusted { weights::TRUSTED_BONUS } else { 0.0 };

        // Combined weighted score
        let score = (success_rate * weights::SUCCESS_RATE_WEIGHT
            + uptime_factor * weights::UPTIME_WEIGHT
            + response_factor * weights::RESPONSE_TIME_WEIGHT
            + trusted_bonus)
            * 100.0;

        score.round() as u32
    }

    /// Update reputation after a deployment
    pub fn update_after_deployment(
        &mut self,
        provider: &str,
        success: bool,
        response_time_ms: Option<u64>,
    ) {
        let mut reputation = self
            .reputations
            .get(provider)
            .cloned()
            .unwrap_or_else(|| self.create_default_reputation(provider));

        if success {
            reputation.successful_deployments += 1;
        } else {
            reputation.failed_deployments += 1;
        }

        // Update average response time
        if let Some(response_ms) = response_time_ms {
            let total = reputation.successful_deployments + reputation.failed_deployments;
            if total == 1 {
                reputation.avg_response_time_ms = response_ms;
            } else {
                // Rolling average
                reputation.avg_response_time_ms = ((reputation.avg_response_time_ms as f64
                    * (total - 1) as f64
                    + response_ms as f64)
                    / total as f64) as u64;
            }
        }

        // Recalculate score
        reputation.score = self.calculate_score(
            reputation.successful_deployments,
            reputation.failed_deployments,
            reputation.avg_uptime_percent,
            reputation.avg_response_time_ms,
            reputation.is_trusted,
        );

        reputation.updated_at = Some(current_timestamp());
        self.reputations.insert(provider.to_string(), reputation);
    }

    /// Filter and rank providers based on criteria
    pub fn rank_providers(
        &self,
        bids: &[ProviderBid],
        criteria: &SelectionCriteria,
    ) -> Vec<RankedProvider> {
        let min_rep = criteria.min_reputation.unwrap_or(MIN_REPUTATION_SCORE);

        let mut ranked: Vec<RankedProvider> = bids
            .iter()
            .filter_map(|bid| {
                let reputation = self
                    .reputations
                    .get(&bid.provider_address)
                    .cloned()
                    .unwrap_or_else(|| self.create_default_reputation(&bid.provider_address));

                // Apply filters
                if reputation.score < min_rep {
                    return None;
                }

                if criteria.trusted_only && !reputation.is_trusted {
                    return None;
                }

                if let Some(max_price) = criteria.max_price_uakt {
                    if bid.price_uakt > max_price {
                        return None;
                    }
                }

                if let Some(min_uptime) = criteria.min_uptime_percent {
                    if reputation.avg_uptime_percent < min_uptime {
                        return None;
                    }
                }

                // Calculate combined score
                // Normalize price (lower is better) and reputation (higher is better)
                let price_score = 1.0 - (bid.price_uakt as f64 / 100000.0).min(1.0);
                let rep_score = reputation.score as f64 / 100.0;

                let combined = price_score * (1.0 - criteria.reputation_weight)
                    + rep_score * criteria.reputation_weight;

                Some(RankedProvider {
                    address: bid.provider_address.clone(),
                    reputation,
                    bid: Some(bid.clone()),
                    combined_score: combined,
                })
            })
            .collect();

        // Sort by combined score (highest first)
        ranked.sort_by(|a, b| {
            b.combined_score
                .partial_cmp(&a.combined_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        ranked
    }

    /// Select the best provider from bids
    pub fn select_best_provider(
        &self,
        bids: &[ProviderBid],
        criteria: &SelectionCriteria,
    ) -> Result<RankedProvider> {
        let ranked = self.rank_providers(bids, criteria);
        ranked
            .into_iter()
            .next()
            .ok_or_else(|| anyhow!("No providers match the selection criteria"))
    }

    /// Get providers above minimum reputation threshold
    pub fn get_qualified_providers(&self, min_score: Option<u32>) -> Vec<&AkashProviderReputation> {
        let threshold = min_score.unwrap_or(MIN_REPUTATION_SCORE);
        self.reputations
            .values()
            .filter(|r| r.score >= threshold)
            .collect()
    }

    /// Export all reputation data
    pub fn export_reputations(&self) -> Vec<AkashProviderReputation> {
        self.reputations.values().cloned().collect()
    }

    /// Import reputation data
    pub fn import_reputations(&mut self, reputations: Vec<AkashProviderReputation>) {
        for rep in reputations {
            self.reputations.insert(rep.provider_address.clone(), rep);
        }
    }
}

impl Default for ProviderReputationManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Get current timestamp
fn current_timestamp() -> Timestamp {
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap();
    Timestamp {
        seconds: now.as_secs() as i64,
        nanos: now.subsec_nanos() as i32,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trusted_providers() {
        let manager = ProviderReputationManager::new();
        assert!(manager.is_trusted("akash1u5cdg7k3gl43mukca4aeultuz8x2j68mgwn28e"));
        assert!(!manager.is_trusted("akash1unknown"));
    }

    #[test]
    fn test_calculate_score() {
        let manager = ProviderReputationManager::new();

        // Perfect provider
        let score = manager.calculate_score(100, 0, 100, 100, true);
        assert!(score > 90);

        // Provider with some failures
        let score = manager.calculate_score(80, 20, 95, 500, false);
        assert!(score > 60 && score < 90);

        // Poor provider
        let score = manager.calculate_score(50, 50, 80, 3000, false);
        assert!(score < 60);
    }

    #[test]
    fn test_rank_providers() {
        let mut manager = ProviderReputationManager::new();

        // Add some reputation data
        manager.set_reputation(AkashProviderReputation {
            provider_address: "akash1good".to_string(),
            score: 90,
            successful_deployments: 100,
            failed_deployments: 5,
            avg_uptime_percent: 99,
            avg_response_time_ms: 200,
            is_trusted: true,
            updated_at: None,
        });

        manager.set_reputation(AkashProviderReputation {
            provider_address: "akash1cheap".to_string(),
            score: 70,
            successful_deployments: 50,
            failed_deployments: 10,
            avg_uptime_percent: 95,
            avg_response_time_ms: 500,
            is_trusted: false,
            updated_at: None,
        });

        let bids = vec![
            ProviderBid {
                provider_address: "akash1good".to_string(),
                price_uakt: 20000,
                dseq: 1,
                gseq: 1,
                oseq: 1,
            },
            ProviderBid {
                provider_address: "akash1cheap".to_string(),
                price_uakt: 10000,
                dseq: 1,
                gseq: 1,
                oseq: 1,
            },
        ];

        // Test with balanced criteria
        let criteria = SelectionCriteria {
            min_reputation: Some(60),
            reputation_weight: 0.5,
            ..Default::default()
        };

        let ranked = manager.rank_providers(&bids, &criteria);
        assert_eq!(ranked.len(), 2);

        // Test with trusted only
        let criteria = SelectionCriteria {
            trusted_only: true,
            ..Default::default()
        };

        let ranked = manager.rank_providers(&bids, &criteria);
        assert_eq!(ranked.len(), 1);
        assert_eq!(ranked[0].address, "akash1good");
    }

    #[test]
    fn test_update_after_deployment() {
        let mut manager = ProviderReputationManager::new();

        // Initial deployment
        manager.update_after_deployment("akash1test", true, Some(300));
        let rep = manager.get_reputation("akash1test").unwrap();
        assert_eq!(rep.successful_deployments, 1);
        assert_eq!(rep.failed_deployments, 0);

        // Failed deployment
        manager.update_after_deployment("akash1test", false, Some(5000));
        let rep = manager.get_reputation("akash1test").unwrap();
        assert_eq!(rep.successful_deployments, 1);
        assert_eq!(rep.failed_deployments, 1);
    }

    #[test]
    fn test_select_best_provider() {
        let mut manager = ProviderReputationManager::new();

        manager.set_reputation(AkashProviderReputation {
            provider_address: "akash1best".to_string(),
            score: 95,
            successful_deployments: 200,
            failed_deployments: 2,
            avg_uptime_percent: 99,
            avg_response_time_ms: 150,
            is_trusted: true,
            updated_at: None,
        });

        let bids = vec![ProviderBid {
            provider_address: "akash1best".to_string(),
            price_uakt: 15000,
            dseq: 1,
            gseq: 1,
            oseq: 1,
        }];

        let best = manager
            .select_best_provider(&bids, &SelectionCriteria::default())
            .unwrap();

        assert_eq!(best.address, "akash1best");
    }
}
