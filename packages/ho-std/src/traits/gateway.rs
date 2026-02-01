//! Gateway Module Traits
//!
//! Traits for communication gateway modules (Discord, Nostr, Element, etc.)
//! that provide user interaction interfaces to the ERGORS engine.

use crate::error::HoResult;
use crate::types::ergors::gateway::v1::{GatewayConfig, GatewayMessage, GatewayResponse};
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::mpsc;

/// Events emitted by gateway modules for processing
#[derive(Debug, Clone)]
pub enum GatewayEvent {
    /// A message was received from a user (triggers LLM processing)
    MessageReceived(GatewayMessage),
    /// A message was processed (metrics tracking only, no LLM call)
    /// Used by gateways that handle routing internally (e.g., Discord slash commands)
    MessageProcessed {
        gateway_id: String,
        session_id: String,
        user_id: String,
    },
    /// Gateway connection state changed
    ConnectionStateChanged {
        gateway_id: String,
        connected: bool,
    },
    /// An error occurred in the gateway
    Error {
        gateway_id: String,
        error: String,
    },
}

/// Context provided to gateway modules for accessing engine services
pub struct GatewayContext<R, S> {
    /// LLM router for processing prompts
    pub router: Arc<R>,
    /// Storage for session management
    pub storage: Arc<S>,
    /// Configuration for this gateway
    pub config: GatewayConfig,
    /// Channel to send events back to the gateway manager
    pub event_tx: mpsc::UnboundedSender<GatewayEvent>,
}

impl<R, S> Clone for GatewayContext<R, S> {
    fn clone(&self) -> Self {
        Self {
            router: Arc::clone(&self.router),
            storage: Arc::clone(&self.storage),
            config: self.config.clone(),
            event_tx: self.event_tx.clone(),
        }
    }
}

/// Core trait for communication gateway modules.
///
/// Implement this trait to add support for new communication platforms
/// (Discord, Nostr, Element, Telegram, IRC, etc.).
///
/// # Lifecycle
///
/// 1. Gateway is created with configuration
/// 2. `start()` is called with context containing router/storage access
/// 3. Gateway listens for messages and sends `GatewayEvent::MessageReceived`
/// 4. Gateway manager routes to LLM and calls `send_response()`
/// 5. `stop()` is called during shutdown
///
/// # Example
///
/// ```rust,ignore
/// pub struct DiscordGateway {
///     config: DiscordGatewayConfig,
///     connected: AtomicBool,
/// }
///
/// #[async_trait]
/// impl<R, S> GatewayModule<R, S> for DiscordGateway
/// where
///     R: LlmRouterTrait + Send + Sync + 'static,
///     S: GatewayStorageTrait + Send + Sync + 'static,
/// {
///     fn gateway_id(&self) -> &str { "discord" }
///     // ... other methods
/// }
/// ```
#[async_trait]
pub trait GatewayModule<R, S>: Send + Sync
where
    R: Send + Sync + 'static,
    S: Send + Sync + 'static,
{
    /// Unique identifier for this gateway type (e.g., "discord", "nostr")
    fn gateway_id(&self) -> &str;

    /// Human-readable name for display
    fn name(&self) -> &str;

    /// Start the gateway, connecting to the platform
    ///
    /// # Arguments
    /// * `ctx` - Context providing access to router, storage, and event channel
    async fn start(&self, ctx: GatewayContext<R, S>) -> HoResult<()>;

    /// Stop the gateway gracefully
    async fn stop(&self) -> HoResult<()>;

    /// Check if the gateway is currently connected
    fn is_connected(&self) -> bool;

    /// Send a response back through this gateway
    ///
    /// # Arguments
    /// * `response` - The response to send, including channel and recipient info
    async fn send_response(&self, response: GatewayResponse) -> HoResult<()>;
}

/// Storage trait for gateway session management
#[async_trait]
pub trait GatewayStorageTrait: Send + Sync {
    /// Get gateway configuration
    async fn get_gateway_config(&self, gateway_id: &str) -> HoResult<Option<GatewayConfig>>;

    /// Store gateway configuration
    async fn put_gateway_config(&self, config: &GatewayConfig) -> HoResult<()>;

    /// List all gateway configurations
    async fn list_gateway_configs(&self) -> HoResult<Vec<GatewayConfig>>;

    /// Get session ID for a gateway thread/channel
    async fn get_gateway_session(&self, gateway_id: &str, thread_id: &str) -> HoResult<Option<String>>;

    /// Get or create a session for a gateway thread
    async fn get_or_create_gateway_session(&self, gateway_id: &str, thread_id: &str) -> HoResult<String>;

    /// Create a new session for a gateway thread
    async fn create_gateway_session(&self, gateway_id: &str, thread_id: &str) -> HoResult<String>;

    /// Store an encrypted gateway token (e.g., Discord bot token)
    async fn store_encrypted_gateway_token(
        &self,
        gateway_id: &str,
        encrypted_token: &[u8],
    ) -> HoResult<()>;

    /// Get encrypted gateway token
    async fn get_encrypted_gateway_token(&self, gateway_id: &str) -> HoResult<Option<Vec<u8>>>;
}

/// Type alias for boxed gateway modules with type erasure
pub type BoxedGatewayModule<R, S> = Box<dyn GatewayModule<R, S>>;
