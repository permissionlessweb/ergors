//! LLM Proxy Module
//!
//! Provides transparent proxy capabilities for CLI tools like Claude Code and opencode.
//! Intercepts requests, captures prompts/responses for retention, and forwards to upstream providers.
//!
//! ## Features
//!
//! - **Zero Configuration**: Tools just change base URL to use the proxy
//! - **SSE Streaming**: Real-time token streaming passthrough with capture
//! - **Provider Routing**: Route to different providers based on model or configuration
//! - **Full Capture**: Store all requests/responses for observability
//!
//! ## Fractal Session Integration
//!
//! Proxy sessions can be integrated with the FractalSession system for hierarchical tracking.
//! Use `create_capture_service_with_sessions` to enable this integration.
//!
//! ## Usage
//!
//! ```bash
//! # Set environment variable to use proxy
//! ANTHROPIC_API_BASE=http://localhost:8080 claude
//! OPENAI_API_BASE=http://localhost:8080 opencode
//! ```

pub mod capture;
pub mod endpoints;
pub mod error;
pub mod open_responses;
pub mod rag;
pub mod router;
pub mod session;
pub mod streaming;
pub mod upstream;

pub use capture::{create_capture_service, create_capture_service_with_sessions, CaptureMessage};
pub use endpoints::{handle_anthropic_proxy, handle_ollama_proxy, handle_openai_proxy};
pub use endpoints::{handle_get_proxy_config, handle_get_session, handle_list_models, handle_query_sessions, handle_update_proxy_config};
pub use router::{ProxyRouter, RouteTarget};
pub use ho_std::types::ergors::orch::v1::{InferenceProviderConfig, InferenceProviderType, ProxyRouterConfig};
