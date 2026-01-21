//! Capture and storage service for proxy sessions.
//!
//! Integrates with FractalSession system to provide hierarchical session tracking.

use crate::session::manager::SessionManager;
use crate::storage::ErgorsStorage;
use chrono::Utc;
use ho_std::types::ergors::management::v1::{CreateSessionRequest, SessionScope, SessionType};
use ho_std::types::ergors::proxy::v1::{ClientType, ProxyApiFormat, ProxySession, StreamChunk};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, error, info, warn};

/// Helper to create a pbjson timestamp from current time.
fn now_timestamp() -> Option<pbjson_types::Timestamp> {
    Some(pbjson_types::Timestamp::from(Utc::now()))
}

/// Events sent to the capture service.
#[derive(Debug)]
pub enum CaptureMessage {
    /// Start a new session with initial request data
    SessionStart {
        session_id: String,
        raw_request: Vec<u8>,
        api_format: ProxyApiFormat,
        client_type: ClientType,
        model: String,
    },
    /// Add a streaming chunk to a session
    Chunk {
        session_id: String,
        chunk: StreamChunk,
    },
    /// Complete a session with final response
    SessionComplete {
        session_id: String,
        final_response: Vec<u8>,
        input_tokens: u64,
        output_tokens: u64,
    },
    /// Mark a session as failed
    SessionError {
        session_id: String,
        error_message: String,
    },
}

/// Service that captures and stores proxy sessions.
///
/// Integrates with FractalSession for hierarchical session tracking.
pub struct CaptureService {
    storage: Arc<ErgorsStorage>,
    /// Buffer for in-progress sessions
    sessions: Arc<RwLock<HashMap<String, ProxySession>>>,
    /// Chunk buffer for streaming sessions
    chunk_buffers: Arc<RwLock<HashMap<String, Vec<StreamChunk>>>>,
    /// Session manager for fractal session integration
    session_manager: Option<Arc<SessionManager>>,
    /// Mapping from proxy session ID to fractal session ID
    fractal_session_ids: Arc<RwLock<HashMap<String, String>>>,
    /// Parent fractal session ID for nesting proxy sessions under an orchestration
    parent_session_id: Option<String>,
}

impl CaptureService {
    /// Create a new capture service.
    pub fn new(storage: Arc<ErgorsStorage>) -> Self {
        Self {
            storage,
            sessions: Arc::new(RwLock::new(HashMap::new())),
            chunk_buffers: Arc::new(RwLock::new(HashMap::new())),
            session_manager: None,
            fractal_session_ids: Arc::new(RwLock::new(HashMap::new())),
            parent_session_id: None,
        }
    }

    /// Create a new capture service with fractal session integration.
    pub fn with_session_manager(
        storage: Arc<ErgorsStorage>,
        session_manager: Arc<SessionManager>,
        parent_session_id: Option<String>,
    ) -> Self {
        Self {
            storage,
            sessions: Arc::new(RwLock::new(HashMap::new())),
            chunk_buffers: Arc::new(RwLock::new(HashMap::new())),
            session_manager: Some(session_manager),
            fractal_session_ids: Arc::new(RwLock::new(HashMap::new())),
            parent_session_id,
        }
    }

    /// Start the capture service background task.
    pub fn spawn(self, mut rx: mpsc::UnboundedReceiver<CaptureMessage>) {
        tokio::spawn(async move {
            info!("Capture service started");
            while let Some(msg) = rx.recv().await {
                if let Err(e) = self.handle_message(msg).await {
                    error!("Capture service error: {}", e);
                }
            }
            info!("Capture service stopped");
        });
    }

    /// Handle a capture message.
    async fn handle_message(&self, msg: CaptureMessage) -> anyhow::Result<()> {
        match msg {
            CaptureMessage::SessionStart {
                session_id,
                raw_request,
                api_format,
                client_type,
                model,
            } => {
                self.handle_session_start(session_id, raw_request, api_format, client_type, model)
                    .await
            }
            CaptureMessage::Chunk { session_id, chunk } => {
                self.handle_chunk(session_id, chunk).await
            }
            CaptureMessage::SessionComplete {
                session_id,
                final_response,
                input_tokens,
                output_tokens,
            } => {
                self.handle_session_complete(
                    session_id,
                    final_response,
                    input_tokens,
                    output_tokens,
                )
                .await
            }
            CaptureMessage::SessionError {
                session_id,
                error_message,
            } => self.handle_session_error(session_id, error_message).await,
        }
    }

    async fn handle_session_start(
        &self,
        session_id: String,
        raw_request: Vec<u8>,
        api_format: ProxyApiFormat,
        client_type: ClientType,
        model: String,
    ) -> anyhow::Result<()> {
        debug!("Starting capture session: {}", session_id);

        // Create FractalSession if session manager is available
        if let Some(sm) = &self.session_manager {
            let mut labels = HashMap::new();
            labels.insert("model".to_string(), model.clone());
            labels.insert("api_format".to_string(), format!("{:?}", api_format));
            labels.insert("client_type".to_string(), format!("{:?}", client_type));
            labels.insert("proxy_session_id".to_string(), session_id.clone());

            let request = CreateSessionRequest {
                session_type: SessionType::Proxy.into(),
                scope: SessionScope::Local.into(), // Proxy sessions are node-local by default
                parent_session_id: self.parent_session_id.clone().unwrap_or_default(),
                labels,
                metadata: HashMap::new(),
                tags: vec!["proxy".to_string()],
                propagation: None,
                initial_content: None,
            };

            match sm.create_session(request).await {
                Ok(fractal_session) => {
                    info!(
                        "Created fractal session {} for proxy session {}",
                        fractal_session.session_id, session_id
                    );
                    // Start the session immediately
                    if let Err(e) = sm.start_session(&fractal_session.session_id).await {
                        warn!("Failed to start fractal session: {}", e);
                    }
                    self.fractal_session_ids
                        .write()
                        .await
                        .insert(session_id.clone(), fractal_session.session_id);
                }
                Err(e) => {
                    warn!("Failed to create fractal session for proxy: {}", e);
                }
            }
        }

        let session = ProxySession {
            session_id: session_id.clone(),
            client_type: client_type.into(),
            api_format: api_format.into(),
            raw_request,
            anthropic_request: None,
            openai_request: None,
            is_streaming: false,
            chunks: vec![],
            final_response: vec![],
            started_at: now_timestamp(),
            completed_at: None,
            total_tokens_input: 0,
            total_tokens_output: 0,
            model,
            error_message: String::new(),
            tool_calls: vec![],
            tool_results: vec![],
            headers: HashMap::new(),
            upstream_provider: String::new(),
        };

        self.sessions
            .write()
            .await
            .insert(session_id.clone(), session);
        self.chunk_buffers.write().await.insert(session_id, vec![]);

        Ok(())
    }

    async fn handle_chunk(&self, session_id: String, chunk: StreamChunk) -> anyhow::Result<()> {
        debug!("Capturing chunk for session: {}", session_id);

        let mut buffers = self.chunk_buffers.write().await;
        if let Some(buffer) = buffers.get_mut(&session_id) {
            buffer.push(chunk);
        } else {
            // Session not found, create a new buffer
            buffers.insert(session_id, vec![chunk]);
        }

        Ok(())
    }

    async fn handle_session_complete(
        &self,
        session_id: String,
        final_response: Vec<u8>,
        input_tokens: u64,
        output_tokens: u64,
    ) -> anyhow::Result<()> {
        debug!("Completing capture session: {}", session_id);

        // Complete the FractalSession if session manager is available
        if let Some(sm) = &self.session_manager {
            let fractal_ids = self.fractal_session_ids.read().await;
            if let Some(fractal_session_id) = fractal_ids.get(&session_id) {
                // Update metrics on the fractal session before completing
                if let Ok(Some(mut fractal_session)) = sm.get_session(fractal_session_id).await {
                    if let Some(ref mut metrics) = fractal_session.metrics {
                        metrics.total_tokens_consumed = input_tokens + output_tokens;
                        // Estimate cost based on tokens (rough approximation)
                        metrics.total_cost =
                            (input_tokens as f64 * 0.00001) + (output_tokens as f64 * 0.00003);
                    }
                }

                match sm.complete_session(fractal_session_id, None).await {
                    Ok(_) => {
                        info!(
                            "Completed fractal session {} for proxy session {}",
                            fractal_session_id, session_id
                        );
                    }
                    Err(e) => {
                        warn!("Failed to complete fractal session: {}", e);
                    }
                }
            }
        }

        // Get the session and chunks
        let mut sessions = self.sessions.write().await;
        let mut buffers = self.chunk_buffers.write().await;

        if let Some(mut session) = sessions.remove(&session_id) {
            // Add chunks to session
            if let Some(chunks) = buffers.remove(&session_id) {
                session.chunks = chunks;
                session.is_streaming = !session.chunks.is_empty();
            }

            // Update completion data
            session.final_response = final_response;
            session.total_tokens_input = input_tokens;
            session.total_tokens_output = output_tokens;
            session.completed_at = now_timestamp();

            // Store to persistent storage
            if let Err(e) = self.storage.put_proxy_session(&session).await {
                error!("Failed to store proxy session {}: {}", session_id, e);
            } else {
                info!(
                    "Stored proxy session {} ({} chunks, {} input tokens, {} output tokens)",
                    session_id,
                    session.chunks.len(),
                    input_tokens,
                    output_tokens
                );
            }
        } else {
            error!("Session not found for completion: {}", session_id);
        }

        // Clean up fractal session mapping
        self.fractal_session_ids.write().await.remove(&session_id);

        Ok(())
    }

    async fn handle_session_error(
        &self,
        session_id: String,
        error_message: String,
    ) -> anyhow::Result<()> {
        debug!("Session error: {} - {}", session_id, error_message);

        // Fail the FractalSession if session manager is available
        if let Some(sm) = &self.session_manager {
            let fractal_ids = self.fractal_session_ids.read().await;
            if let Some(fractal_session_id) = fractal_ids.get(&session_id) {
                match sm
                    .fail_session(fractal_session_id, &error_message, Some("PROXY_ERROR"))
                    .await
                {
                    Ok(_) => {
                        info!(
                            "Failed fractal session {} for proxy session {}",
                            fractal_session_id, session_id
                        );
                    }
                    Err(e) => {
                        warn!("Failed to fail fractal session: {}", e);
                    }
                }
            }
        }

        let mut sessions = self.sessions.write().await;
        let mut buffers = self.chunk_buffers.write().await;

        if let Some(mut session) = sessions.remove(&session_id) {
            // Add any captured chunks
            if let Some(chunks) = buffers.remove(&session_id) {
                session.chunks = chunks;
            }

            session.error_message = error_message;
            session.completed_at = now_timestamp();

            // Store even failed sessions for debugging
            if let Err(e) = self.storage.put_proxy_session(&session).await {
                error!("Failed to store failed proxy session {}: {}", session_id, e);
            }
        }

        // Clean up fractal session mapping
        self.fractal_session_ids.write().await.remove(&session_id);

        Ok(())
    }
}

/// Create a capture service and return the sender channel.
pub fn create_capture_service(
    storage: Arc<ErgorsStorage>,
) -> mpsc::UnboundedSender<CaptureMessage> {
    let (tx, rx) = mpsc::unbounded_channel();
    let service = CaptureService::new(storage);
    service.spawn(rx);
    tx
}

/// Create a capture service with fractal session integration.
///
/// This allows proxy sessions to be tracked within the fractal session hierarchy,
/// enabling correlation with orchestration sessions and cross-node tracking.
pub fn create_capture_service_with_sessions(
    storage: Arc<ErgorsStorage>,
    session_manager: Arc<SessionManager>,
    parent_session_id: Option<String>,
) -> mpsc::UnboundedSender<CaptureMessage> {
    let (tx, rx) = mpsc::unbounded_channel();
    let service = CaptureService::with_session_manager(storage, session_manager, parent_session_id);
    service.spawn(rx);
    tx
}
