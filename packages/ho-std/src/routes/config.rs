//! Route configuration and definitions
//!
//! This module provides a type-safe way to define routes using proto-generated types
//! following the type/value tuple pattern for standardized transport layer communication.
use crate::prelude::*;
use prost::Name;

/// Generic route definition using proto types
/// This follows the type/value pattern where request/response types are proto messages
#[derive(Debug, Default, Clone)]
pub struct RouteDefinition {
    pub requires_auth: bool,
    pub request_type: String,  // Proto type URL
    pub response_type: String, // Proto type URL
}

/// Registry of all available routes
pub struct RouteRegistry {
    routes: Vec<RouteDefinition>,
}

impl RouteRegistry {
    pub fn new() -> Self {
        Self {
            routes: Self::default_routes(),
        }
    }

    pub fn routes(&self) -> &[RouteDefinition] {
        &self.routes
    }

    /// Default routes for the CW-HO system
    fn default_routes() -> Vec<RouteDefinition> {
        vec![
            // Public routes
            RouteDefinition {
                requires_auth: false,
                request_type: HealthRequest::type_url(),
                response_type: HealthResponse::type_url(),
            },
            // Protected routes
            RouteDefinition {
                requires_auth: true,
                request_type: QueryPromptsRequest::type_url(),
                response_type: QueryPromptsResponse::type_url(),
            },
            RouteDefinition {
                requires_auth: true,
                request_type: PromptRequest::type_url(),
                response_type: PromptResponse::type_url(),
            },
            RouteDefinition {
                requires_auth: true,
                request_type: BootstrapNodeRequest::type_url(),
                response_type: BootstrapNodeResponse::type_url(),
            },
            RouteDefinition {
                requires_auth: true,
                request_type: CreateFractalRequest::type_url(),
                response_type: CreateFractalResponse::type_url(),
            },
            RouteDefinition {
                requires_auth: true,
                request_type: PruneNodeRequest::type_url(),
                response_type: PruneNodeResponse::type_url(),
            },
            RouteDefinition {
                requires_auth: true,
                request_type: GetTopologyRequest::type_url(),
                response_type: GetTopologyResponse::type_url(),
            },
        ]
    }
}

/// Generic macro to create routes with custom handlers
///
/// This macro allows you to define routes with complete flexibility:
/// - No hardcoded route names or paths
/// - Routes explicitly separated into public and protected
/// - Returns tuple of (public_router, protected_router) for external auth layer application
///
/// # Example
/// ```rust
/// use ho_std::define_routes;
/// use axum::middleware;
///
/// let (public_router, protected_router) = define_routes! {
///     public_routes: [
///         { path: "/health", method: get, handler: handle_health },
///     ],
///     protected_routes: [
///         { path: "/api/prompts", method: get, handler: handle_query },
///         { path: "/orchestrate/bootstrap", method: post, handler: handle_bootstrap },
///         { path: "/api/prompt", method: post, handler: handle_prompt },
///         { path: "/orchestrate/fractal", method: post, handler: handle_fractal },
///         { path: "/orchestrate/prune", method: post, handler: handle_prune },
///         { path: "/network/topology", method: get, handler: handle_topology },
///     ]
/// };
///
/// let auth_layer = middleware::from_fn(auth_middleware);
/// let app = Router::new()
///     .merge(public_router)
///     .merge(protected_router.layer(auth_layer));
/// ```
#[macro_export]
macro_rules! define_routes {
    (
        public_routes: [
            $(
                { path: $pub_path:expr, method: $pub_method:ident, handler: $pub_handler:expr }
            ),* $(,)?
        ],
        protected_routes: [
            $(
                { path: $prot_path:expr, method: $prot_method:ident, handler: $prot_handler:expr }
            ),* $(,)?
        ]
    ) => {{
        use axum::{routing::{get, post, put, delete, patch}, Router};

        // Build public routes
        let public_router = Router::new()
            $(.route($pub_path, $pub_method($pub_handler)))*;

        // Build protected routes (no layer applied)
        let protected_router = Router::new()
            $(.route($prot_path, $prot_method($prot_handler)))*;

        (public_router, protected_router)
    }};
    (
        routes: [
            $(
                { path: $path:expr, method: $method:ident, handler: $handler:expr }
            ),* $(,)?
        ]
    ) => {{
        use axum::{routing::{get, post, put, delete, patch}, Router};

        Router::new()
            $(.route($path, $method($handler)))*
    }};
}
