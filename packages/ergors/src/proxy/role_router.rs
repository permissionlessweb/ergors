//! Role-aware LLM router wrapper for RLM callbacks.
//!
//! Resolves engine role keywords (e.g. "rlm-primary") to assigned providers
//! via storage, then calls provider directly via LlmRouter::call_provider_by_name().

use async_trait::async_trait;
use ho_std::types::ergors::orch::v1::{EngineRole, PromptRequest, PromptResponse};
use std::sync::Arc;
use tracing::{debug, warn};

use crate::storage::ErgorsStorage;

/// Wraps LlmRouter to resolve engine roles for RLM callbacks.
pub struct RoleAwareLlmRouter {
    inner: Arc<ho_std::llm::LlmRouter>,
    storage: Arc<ErgorsStorage>,
}

impl RoleAwareLlmRouter {
    pub fn new(inner: Arc<ho_std::llm::LlmRouter>, storage: Arc<ErgorsStorage>) -> Self {
        Self { inner, storage }
    }

    /// Resolve an engine role to the first assigned provider name
    async fn resolve_role(&self, role: EngineRole) -> Option<String> {
        let config = self.storage.get_engine_role_config().await.ok()??;
        let role_i32 = role as i32;
        let mapping = config.mappings.iter().find(|m| m.role == role_i32)?;
        mapping.provider_ids.first().cloned()
    }
}

#[async_trait]
impl ergors_rlm::LlmRouterTrait for RoleAwareLlmRouter {
    async fn handle_request(&self, req: &PromptRequest, model: &str) -> anyhow::Result<PromptResponse> {
        // Map role keywords to EngineRole
        let role = match model {
            "rlm-primary" | "default" => Some(EngineRole::RlmPrimary),
            "rlm-secondary" => Some(EngineRole::RlmSecondary),
            _ => None,
        };

        if let Some(engine_role) = role {
            // Try the requested role
            if let Some(provider_name) = self.resolve_role(engine_role).await {
                debug!("RLM role '{}' resolved to provider '{}'", model, provider_name);
                return self.inner.call_provider_by_name(&provider_name, req).await
                    .map_err(|e| {
                        warn!(
                            "Role '{}' resolved to provider '{}' but call failed: {}",
                            model, provider_name, e
                        );
                        anyhow::anyhow!(
                            "Role '{}' -> provider '{}': {}. \
                             Hint: if this provider was added after server start, \
                             ensure it was registered with a base_url via 'provider add' \
                             or 'deploy register-providers'",
                            model, provider_name, e
                        )
                    });
            }
            // For rlm-secondary, fall back to rlm-primary
            if matches!(engine_role, EngineRole::RlmSecondary) {
                if let Some(provider_name) = self.resolve_role(EngineRole::RlmPrimary).await {
                    debug!("RLM role 'rlm-secondary' falling back to rlm-primary provider '{}'", provider_name);
                    return self.inner.call_provider_by_name(&provider_name, req).await
                        .map_err(|e| anyhow::anyhow!(
                            "Role 'rlm-secondary' (fallback to rlm-primary) -> provider '{}': {}",
                            provider_name, e
                        ));
                }
            }
            // No role assigned — fall through to model-pattern routing
            warn!(
                "No provider assigned for role '{}'. Use 'ergors provider assign <NAME> --role {}' to assign one.",
                model, model
            );
        }

        // Standard model-pattern routing
        self.inner.handle_request(req, model).await
            .map_err(|e| anyhow::anyhow!("{}", e))
    }
}
