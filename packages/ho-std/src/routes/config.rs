//! Route configuration and definitions

/// Represents a single API route with metadata
#[derive(Debug, Clone)]
pub struct Route {
    pub path: &'static str,
    pub name: &'static str,
    pub method: &'static str,
}

/// All available routes in the CW-HO API
pub mod routes {
    use super::Route;

    // Public routes (no authentication required)
    pub mod public {
        use super::Route;

        pub const HEALTH: Route = Route {
            path: "/health",
            name: "health",
            method: "GET",
        };
    }

    // Protected routes (authentication required)
    pub mod protected {
        use super::Route;

        pub const API_PROMPTS: Route = Route {
            path: "/api/prompts",
            name: "query",
            method: "GET",
        };

        pub const ORCHESTRATE_BOOTSTRAP: Route = Route {
            path: "/orchestrate/bootstrap",
            name: "bootstrap",
            method: "POST",
        };

        pub const API_PROMPT: Route = Route {
            path: "/api/prompt",
            name: "prompt",
            method: "POST",
        };

        pub const ORCHESTRATE_FRACTAL: Route = Route {
            path: "/orchestrate/fractal",
            name: "fractal_creation",
            method: "POST",
        };

        pub const ORCHESTRATE_PRUNE: Route = Route {
            path: "/orchestrate/prune",
            name: "prune",
            method: "POST",
        };

        pub const NETWORK_TOPOLOGY: Route = Route {
            path: "/network/topology",
            name: "network_topology",
            method: "GET",
        };
    }
}

/// Configure routes with nested router pattern for authentication
///
/// This function creates a router with two sections:
/// - Public routes: No authentication required
/// - Protected routes: Authentication middleware applied
///
/// # Example
/// ```rust
/// use ho_std::routes::configure_routes;
/// use axum::{Router, routing::{get, post}};
///
/// let app = configure_routes(Router::new())
///     .layer(cors_layer)
///     .with_state(app_state);
/// ```
/// Configure routes without authentication (for development or when auth is handled elsewhere)
pub fn configure_routes_no_auth<S>() -> axum::Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    use axum::routing::{get, post};

    axum::Router::new()
        // Public routes
        .route(
            routes::public::HEALTH.path,
            get(|| async { "Health endpoint - implement in your crate" }),
        )
        // Protected routes (without auth middleware for development)
        .route(
            routes::protected::API_PROMPTS.path,
            get(|| async { "Query endpoint - implement in your crate" }),
        )
        .route(
            routes::protected::ORCHESTRATE_BOOTSTRAP.path,
            post(|| async { "Bootstrap endpoint - implement in your crate" }),
        )
        .route(
            routes::protected::API_PROMPT.path,
            post(|| async { "Prompt endpoint - implement in your crate" }),
        )
        .route(
            routes::protected::ORCHESTRATE_FRACTAL.path,
            post(|| async { "Fractal endpoint - implement in your crate" }),
        )
        .route(
            routes::protected::ORCHESTRATE_PRUNE.path,
            post(|| async { "Prune endpoint - implement in your crate" }),
        )
        .route(
            routes::protected::NETWORK_TOPOLOGY.path,
            get(|| async { "Topology endpoint - implement in your crate" }),
        )
}

/// Configure routes with authentication (production use)
/// Note: This is a template - you'll need to implement your own middleware
pub fn configure_routes_with_auth<S>() -> axum::Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    // For now, just return the no-auth version
    // Users can add their own middleware after calling this function
    configure_routes_no_auth()
}

/// Default route configuration (no auth for development ease)
pub fn configure_routes<S>() -> axum::Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    configure_routes_no_auth()
}

/// Helper macro to create routes with custom handlers
///
/// # Example
/// ```rust
/// use ho_std::create_routes;
///
/// let app = create_routes! {
///     public: {
///         health => handle_health,
///     },
///     protected: {
///         api_prompts => handle_query,
///         orchestrate_bootstrap => handle_bootstrap,
///         api_prompt => handle_prompt,
///         orchestrate_fractal => handle_fractal_hoe_creation,
///         orchestrate_prune => handle_prune,
///         network_topology => handle_network_topology,
///     }
/// };
/// ```
#[macro_export]
macro_rules! create_routes {
    (
        public: {
            $($pub_route:ident => $pub_handler:expr),* $(,)?
        },
        protected: {
            $($prot_route:ident => $prot_handler:expr),* $(,)?
        } $(,)?
    ) => {{
        use axum::{routing::{get, post}, middleware, Router};
        use $crate::routes::routes::{public, protected};

        let public_router = Router::new()
            $(.route(public::HEALTH.path, get($pub_handler)))*;

        let protected_router = Router::new()
            $(.route(match stringify!($prot_route) {
                "api_prompts" => protected::API_PROMPTS.path,
                "orchestrate_bootstrap" => protected::ORCHESTRATE_BOOTSTRAP.path,
                "api_prompt" => protected::API_PROMPT.path,
                "orchestrate_fractal" => protected::ORCHESTRATE_FRACTAL.path,
                "orchestrate_prune" => protected::ORCHESTRATE_PRUNE.path,
                "network_topology" => protected::NETWORK_TOPOLOGY.path,
                _ => panic!("Unknown route: {}", stringify!($prot_route)),
            }, match stringify!($prot_route) {
                "api_prompts" | "network_topology" => get($prot_handler),
                _ => post($prot_handler),
            }))*
            // Auth middleware can be added separately by the user
            ;

        Router::new()
            .merge(public_router)
            .merge(protected_router)
    }};
}
