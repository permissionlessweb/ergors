//! LLM Proxy Module
//!
//! Provides transparent proxy capabilities for CLI tools like Claude Code and opencode.
//! Intercepts requests, captures prompts/responses for retention, and forwards to upstream providers.

pub mod capture;
pub mod endpoints;
pub mod session;
pub mod streaming;
pub mod upstream;

pub use endpoints::{handle_anthropic_proxy, handle_openai_proxy};
pub use endpoints::{handle_get_session, handle_query_sessions};
