//! Capture and storage service for proxy sessions.

use crate::storage::ErgorsStorage;
use chrono::Utc;
use ho_std::types::ergors::proxy::v1::{ClientType, ProxyApiFormat, ProxySession, StreamChunk};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, error, info};

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
pub struct CaptureService {
    storage: Arc<ErgorsStorage>,
    /// Buffer for in-progress sessions
    sessions: Arc<RwLock<HashMap<String, ProxySession>>>,
    /// Chunk buffer for streaming sessions
    chunk_buffers: Arc<RwLock<HashMap<String, Vec<StreamChunk>>>>,
}

impl CaptureService {
    /// Create a new capture service.
    pub fn new(storage: Arc<ErgorsStorage>) -> Self {
        Self {
            storage,
            sessions: Arc::new(RwLock::new(HashMap::new())),
            chunk_buffers: Arc::new(RwLock::new(HashMap::new())),
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

        Ok(())
    }

    async fn handle_session_error(
        &self,
        session_id: String,
        error_message: String,
    ) -> anyhow::Result<()> {
        debug!("Session error: {} - {}", session_id, error_message);

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
