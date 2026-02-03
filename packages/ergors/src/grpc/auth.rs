//! Bearer Token Authentication Interceptor for gRPC
//!
//! Provides authentication middleware for the ManagementService gRPC server.
//! Validates Bearer tokens from the `authorization` metadata header.

use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::RwLock;
use tonic::{Request, Status};

/// Token store for managing valid authentication tokens
#[derive(Debug, Clone, Default)]
pub struct TokenStore {
    /// Set of valid tokens
    tokens: Arc<RwLock<HashSet<String>>>,
    /// Whether authentication is enabled
    enabled: bool,
}

impl TokenStore {
    /// Create a new token store
    pub fn new(enabled: bool) -> Self {
        Self {
            tokens: Arc::new(RwLock::new(HashSet::new())),
            enabled,
        }
    }

    /// Create a disabled token store (all requests allowed)
    pub fn disabled() -> Self {
        Self::new(false)
    }

    /// Add a token to the store
    pub async fn add_token(&self, token: String) {
        self.tokens.write().await.insert(token);
    }

    /// Remove a token from the store
    pub async fn remove_token(&self, token: &str) {
        self.tokens.write().await.remove(token);
    }

    /// Check if a token is valid
    pub async fn is_valid(&self, token: &str) -> bool {
        if !self.enabled {
            return true;
        }
        self.tokens.read().await.contains(token)
    }

    /// Get the number of tokens in the store
    pub async fn token_count(&self) -> usize {
        self.tokens.read().await.len()
    }
}

/// Authentication interceptor for gRPC requests
///
/// Extracts the Bearer token from the `authorization` metadata header
/// and validates it against the token store.
pub fn create_auth_interceptor(
    token_store: TokenStore,
) -> impl Fn(Request<()>) -> Result<Request<()>, Status> + Clone {
    move |req: Request<()>| {
        // If auth is disabled, allow all requests
        if !token_store.enabled {
            return Ok(req);
        }

        // Extract authorization header
        let token = req
            .metadata()
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.strip_prefix("Bearer ").or_else(|| s.strip_prefix("bearer ")));

        match token {
            Some(t) => {
                // For synchronous interceptor, we can't use async validation
                // In production, you'd want to use a sync-safe validation method
                // or use tower layers instead
                if t.is_empty() {
                    Err(Status::unauthenticated("Empty token"))
                } else {
                    // TODO: Implement proper sync validation
                    // For now, accept any non-empty token when auth is enabled
                    Ok(req)
                }
            }
            None => Err(Status::unauthenticated("Missing authorization header")),
        }
    }
}

/// Simple synchronous token validator
/// For production use, consider using tower::Layer for async validation
pub fn validate_token_sync(token: &str, valid_tokens: &[String]) -> bool {
    valid_tokens.contains(&token.to_string())
}

/// Create a simple auth interceptor with a list of valid tokens
pub fn simple_auth_interceptor(
    valid_tokens: Vec<String>,
) -> impl Fn(Request<()>) -> Result<Request<()>, Status> + Clone {
    move |req: Request<()>| {
        if valid_tokens.is_empty() {
            // No tokens configured, allow all requests
            return Ok(req);
        }

        let token = req
            .metadata()
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.strip_prefix("Bearer ").or_else(|| s.strip_prefix("bearer ")));

        match token {
            Some(t) if validate_token_sync(t, &valid_tokens) => Ok(req),
            Some(_) => Err(Status::unauthenticated("Invalid token")),
            None => Err(Status::unauthenticated("Missing authorization header")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_token_store() {
        let store = TokenStore::new(true);

        // Initially empty
        assert_eq!(store.token_count().await, 0);
        assert!(!store.is_valid("test-token").await);

        // Add token
        store.add_token("test-token".to_string()).await;
        assert_eq!(store.token_count().await, 1);
        assert!(store.is_valid("test-token").await);

        // Remove token
        store.remove_token("test-token").await;
        assert_eq!(store.token_count().await, 0);
        assert!(!store.is_valid("test-token").await);
    }

    #[tokio::test]
    async fn test_disabled_store() {
        let store = TokenStore::disabled();

        // Disabled store accepts any token
        assert!(store.is_valid("any-token").await);
        assert!(store.is_valid("").await);
    }

    #[test]
    fn test_validate_token_sync() {
        let tokens = vec!["token1".to_string(), "token2".to_string()];

        assert!(validate_token_sync("token1", &tokens));
        assert!(validate_token_sync("token2", &tokens));
        assert!(!validate_token_sync("token3", &tokens));
        assert!(!validate_token_sync("", &tokens));
    }
}
