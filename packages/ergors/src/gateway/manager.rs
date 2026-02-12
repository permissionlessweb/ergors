//! Gateway Manager
//!
//! Orchestrates multiple gateway modules, routing messages to the LLM router
//! and responses back to the appropriate gateway.
//!
//! All gateways (Discord, Nostr, Element, etc.) should call `process_message`
//! for unified message handling and metrics tracking.

use crate::storage::ErgorsStorage;
use anyhow::{anyhow, Result};
use ho_std::{
    llm::LlmRouter,
    traits::gateway::{GatewayContext, GatewayEvent, GatewayModule},
    types::ergors::{
        gateway::v1::{GatewayConfig, GatewayResponse},
        orch::v1::{PromptContext, PromptMessage, PromptRequest, PromptResponse},
    },
};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};
use tokio::sync::{mpsc, RwLock};
use tracing::{error, info};

/// Per-gateway metrics for tracking message processing.
#[derive(Debug, Default)]
pub struct GatewayMetrics {
    /// Total messages processed through this gateway
    pub messages_processed: AtomicU64,
    /// Unix timestamp of the last processed message
    pub last_message_timestamp: AtomicU64,
}

impl GatewayMetrics {
    pub fn record_message(&self) {
        self.messages_processed.fetch_add(1, Ordering::Relaxed);
        self.last_message_timestamp.store(
            chrono::Utc::now().timestamp() as u64,
            Ordering::Relaxed,
        );
    }

    pub fn get_stats(&self) -> (u64, u64) {
        (
            self.messages_processed.load(Ordering::Relaxed),
            self.last_message_timestamp.load(Ordering::Relaxed),
        )
    }
}

/// Gateway Manager orchestrates multiple communication gateways.
///
/// Responsibilities:
/// - Register and manage gateway modules
/// - Start/stop gateways based on configuration
/// - Route incoming messages to the LLM router (unified `process_message`)
/// - Track per-gateway metrics
/// - Dispatch responses back to the originating gateway
pub struct GatewayManager {
    /// Registered gateway modules
    gateways: RwLock<HashMap<String, Arc<dyn GatewayModule<LlmRouter, ErgorsStorage> + Send + Sync>>>,
    /// Per-gateway metrics
    metrics: RwLock<HashMap<String, Arc<GatewayMetrics>>>,
    /// LLM router for processing prompts
    router: Arc<LlmRouter>,
    /// Storage for session and config management
    storage: Arc<ErgorsStorage>,
    /// Node public key for decrypting gateway secrets (needed for hot-start)
    node_pubkey: Option<Vec<u8>>,
    /// Channel for receiving gateway events
    event_rx: RwLock<Option<mpsc::UnboundedReceiver<GatewayEvent>>>,
    /// Channel for sending gateway events (cloned to gateways)
    event_tx: mpsc::UnboundedSender<GatewayEvent>,
    /// Running flag
    running: RwLock<bool>,
}

impl GatewayManager {
    /// Create a new gateway manager.
    pub fn new(router: Arc<LlmRouter>, storage: Arc<ErgorsStorage>, node_pubkey: Option<Vec<u8>>) -> Self {
        let (event_tx, event_rx) = mpsc::unbounded_channel();

        Self {
            gateways: RwLock::new(HashMap::new()),
            metrics: RwLock::new(HashMap::new()),
            router,
            storage,
            node_pubkey,
            event_rx: RwLock::new(Some(event_rx)),
            event_tx,
            running: RwLock::new(false),
        }
    }

    /// Register a gateway module.
    pub async fn register(
        &self,
        gateway: Arc<dyn GatewayModule<LlmRouter, ErgorsStorage> + Send + Sync>,
    ) {
        let id = gateway.gateway_id().to_string();
        info!("Registering gateway: {} ({})", gateway.name(), id);
        self.gateways.write().await.insert(id.clone(), gateway);
        self.metrics
            .write()
            .await
            .insert(id, Arc::new(GatewayMetrics::default()));
    }

    /// Check if a gateway is enabled in configuration.
    pub async fn is_enabled(&self, gateway_id: &str) -> Result<bool> {
        if let Some(config) = self.storage.get_gateway_config(gateway_id).await? {
            Ok(config.enabled)
        } else {
            Ok(false)
        }
    }

    /// Start all enabled gateways.
    pub async fn start_all(&self) -> Result<()> {
        let gateways = self.gateways.read().await;

        for (id, gateway) in gateways.iter() {
            if self.is_enabled(id).await? {
                let config = self
                    .storage
                    .get_gateway_config(id)
                    .await?
                    .unwrap_or_else(|| GatewayConfig {
                        gateway_id: id.clone(),
                        gateway_type: id.clone(),
                        enabled: true,
                        settings: HashMap::new(),
                    });

                let ctx = GatewayContext {
                    router: Arc::clone(&self.router),
                    storage: Arc::clone(&self.storage),
                    config,
                    event_tx: self.event_tx.clone(),
                };

                match gateway.start(ctx).await {
                    Ok(_) => info!("Gateway {} started successfully", id),
                    Err(e) => error!("Failed to start gateway {}: {}", id, e),
                }
            } else {
                info!("Gateway {} is disabled, skipping", id);
            }
        }

        *self.running.write().await = true;
        Ok(())
    }

    /// Start a single gateway by ID (hot-start while engine is running).
    ///
    /// Used by `enable_gateway` to start a gateway without restarting the engine.
    /// For Discord: reloads config from storage (decrypting token) to pick up
    /// changes made since engine boot.
    pub async fn start_one(&self, gateway_id: &str) -> Result<()> {
        // Check if already connected
        {
            let gateways = self.gateways.read().await;
            if let Some(g) = gateways.get(gateway_id) {
                if g.is_connected() {
                    info!("Gateway {} already connected", gateway_id);
                    return Ok(());
                }
            }
        }

        let config = self
            .storage
            .get_gateway_config(gateway_id)
            .await?
            .unwrap_or_else(|| GatewayConfig {
                gateway_id: gateway_id.to_string(),
                gateway_type: gateway_id.to_string(),
                enabled: true,
                settings: HashMap::new(),
            });

        if !config.enabled {
            info!("Gateway {} is disabled, skipping hot-start", gateway_id);
            return Ok(());
        }

        // Reload gateway from storage to pick up config changes (e.g. newly set token)
        #[cfg(feature = "discord")]
        if gateway_id == "discord" {
            use crate::gateway::discord::DiscordGateway;

            let pubkey = self.node_pubkey.as_deref();
            let fresh = DiscordGateway::from_storage(&self.storage, pubkey).await?;
            let fresh = Arc::new(fresh);

            // Replace stale gateway with fresh one
            self.gateways
                .write()
                .await
                .insert(gateway_id.to_string(), fresh.clone());

            let ctx = GatewayContext {
                router: Arc::clone(&self.router),
                storage: Arc::clone(&self.storage),
                config,
                event_tx: self.event_tx.clone(),
            };

            match fresh.start(ctx).await {
                Ok(_) => info!("Gateway {} hot-started successfully", gateway_id),
                Err(e) => error!("Failed to hot-start gateway {}: {}", gateway_id, e),
            }
            return Ok(());
        }

        // Generic path for non-Discord gateways
        let gateways = self.gateways.read().await;
        if let Some(gateway) = gateways.get(gateway_id) {
            let ctx = GatewayContext {
                router: Arc::clone(&self.router),
                storage: Arc::clone(&self.storage),
                config,
                event_tx: self.event_tx.clone(),
            };

            match gateway.start(ctx).await {
                Ok(_) => info!("Gateway {} hot-started successfully", gateway_id),
                Err(e) => error!("Failed to hot-start gateway {}: {}", gateway_id, e),
            }
        } else {
            info!("Gateway {} not registered, skipping hot-start", gateway_id);
        }

        Ok(())
    }

    /// Stop all gateways.
    pub async fn stop_all(&self) -> Result<()> {
        *self.running.write().await = false;

        let gateways = self.gateways.read().await;
        for (id, gateway) in gateways.iter() {
            if gateway.is_connected() {
                match gateway.stop().await {
                    Ok(_) => info!("Gateway {} stopped", id),
                    Err(e) => error!("Error stopping gateway {}: {}", id, e),
                }
            }
        }

        Ok(())
    }

    /// Run the gateway manager event loop.
    ///
    /// This should be spawned in a separate task.
    pub async fn run(&self) -> Result<()> {
        let mut event_rx = self
            .event_rx
            .write()
            .await
            .take()
            .ok_or_else(|| anyhow!("Event receiver already taken"))?;

        info!("Gateway manager event loop starting");

        while let Some(event) = event_rx.recv().await {
            if !*self.running.read().await {
                break;
            }

            match event {
                GatewayEvent::MessageReceived(msg) => {
                    if let Err(e) = self.handle_message(msg).await {
                        error!("Error handling gateway message: {}", e);
                    }
                }
                GatewayEvent::MessageProcessed {
                    gateway_id,
                    session_id,
                    user_id,
                } => {
                    // Record metrics for gateways that process messages internally
                    if let Some(metrics) = self.metrics.read().await.get(&gateway_id) {
                        metrics.record_message();
                    }
                    info!(
                        gateway_id = %gateway_id,
                        session_id = %session_id,
                        user_id = %user_id,
                        "Gateway message processed (metrics recorded)"
                    );
                }
                GatewayEvent::ConnectionStateChanged {
                    gateway_id,
                    connected,
                } => {
                    info!(
                        "Gateway {} connection state: {}",
                        gateway_id,
                        if connected { "connected" } else { "disconnected" }
                    );
                }
                GatewayEvent::Error { gateway_id, error } => {
                    error!("Gateway {} error: {}", gateway_id, error);
                }
            }
        }

        info!("Gateway manager event loop stopped");
        Ok(())
    }

    /// Process a message through the LLM router.
    ///
    /// This is the canonical method for ALL gateway message processing.
    /// All gateways (Discord, Nostr, Element, etc.) should call this method
    /// to ensure consistent metrics tracking and routing.
    ///
    /// Returns the LLM response for the caller to handle (e.g., chunking for Discord).
    pub async fn process_message(
        &self,
        gateway_id: &str,
        content: &str,
        session_id: &str,
        user_id: &str,
        thread_id: &str,
        model: Option<&str>,
    ) -> Result<PromptResponse> {
        let model = model.unwrap_or("default");

        // Build prompt request
        let prompt_req = PromptRequest {
            messages: vec![PromptMessage {
                role: "user".to_string(),
                content: content.to_string(),
                ..Default::default()
            }],
            model: model.to_string(),
            context: Some(PromptContext {
                session_id: session_id.to_string(),
                user_id: user_id.to_string(),
                thread_id: thread_id.to_string(),
            }),
            ..Default::default()
        };

        // Route to LLM
        let response = self.router.handle_request(&prompt_req, model).await?;

        // Record metrics
        if let Some(metrics) = self.metrics.read().await.get(gateway_id) {
            metrics.record_message();
        }

        info!(
            gateway_id = %gateway_id,
            session_id = %session_id,
            user_id = %user_id,
            "Processed gateway message"
        );

        Ok(response)
    }

    /// Handle an incoming message from event-based gateways.
    ///
    /// This wraps `process_message` and sends the response back through
    /// the gateway's `send_response` method. Used by gateways that send
    /// events instead of calling `process_message` directly.
    async fn handle_message(
        &self,
        msg: ho_std::types::ergors::gateway::v1::GatewayMessage,
    ) -> Result<()> {
        let gateway_id = msg.gateway_id.clone();
        let channel_id = msg.channel_id.clone();
        let thread_id = msg.thread_id.clone();
        let sender_id = msg.sender_id.clone();

        // Get or create session for this thread
        let session_id = if let Some(session) = msg.metadata.get("session_id") {
            session.clone()
        } else {
            self.storage
                .get_or_create_gateway_session(&gateway_id, &thread_id)
                .await?
        };

        let model = msg.metadata.get("model").map(|s| s.as_str());

        // Use unified process_message
        let response = self
            .process_message(&gateway_id, &msg.content, &session_id, &sender_id, &thread_id, model)
            .await?;

        // Send response back through gateway
        let gateways = self.gateways.read().await;
        let gateway = gateways
            .get(&gateway_id)
            .ok_or_else(|| anyhow!("Gateway not found: {}", gateway_id))?;

        let response_content = response.response.join("\n");

        let gateway_response = GatewayResponse {
            gateway_id: gateway_id.clone(),
            channel_id,
            recipient_id: sender_id,
            content: response_content,
            reply_to_id: msg.reply_to_id,
            embeds: vec![],
            attachments: vec![],
        };

        gateway.send_response(gateway_response).await?;

        Ok(())
    }

    /// Get metrics for a specific gateway.
    pub async fn get_gateway_metrics(&self, gateway_id: &str) -> Option<(u64, u64)> {
        self.metrics
            .read()
            .await
            .get(gateway_id)
            .map(|m| m.get_stats())
    }

    /// Get list of registered gateways with their status and metrics.
    pub async fn list_gateways(&self) -> Vec<GatewayInfo> {
        let gateways = self.gateways.read().await;
        let metrics = self.metrics.read().await;
        let mut result = Vec::new();

        for (id, gateway) in gateways.iter() {
            let (messages_processed, last_message_timestamp) = metrics
                .get(id)
                .map(|m| m.get_stats())
                .unwrap_or((0, 0));

            result.push(GatewayInfo {
                gateway_id: id.clone(),
                name: gateway.name().to_string(),
                connected: gateway.is_connected(),
                messages_processed,
                last_message_timestamp,
            });
        }

        result
    }

    /// Get a reference to the LLM router.
    pub fn router(&self) -> &Arc<LlmRouter> {
        &self.router
    }

    /// Get a reference to storage.
    pub fn storage(&self) -> &Arc<ErgorsStorage> {
        &self.storage
    }
}

/// Gateway status information with metrics
#[derive(Debug, Clone)]
pub struct GatewayInfo {
    pub gateway_id: String,
    pub name: String,
    pub connected: bool,
    pub messages_processed: u64,
    pub last_message_timestamp: u64,
}
