//! Generic Inbox HTTP Handlers
//!
//! Persistent message inbox backed by cnidarium storage.
//! Public endpoints allow any node to submit requests or check status.
//! Protected endpoints allow the operator to manage their inbox (accept/reject).
//!
//! The inbox is action-type agnostic — grant requests, bootstrap requests,
//! deploy requests, etc. all flow through the same inbox with typed payloads.

use axum::extract::{Path, Query, State};
use axum::Json;
use ho_std::error::error_json;
use ho_std::types::ergors::orch::v1::{
    GrantAcceptanceMode, GrantRequest, GrantRequestParams, GrantRequestStatus, GrantType,
    GranterConfig, InboxMessage, InboxMessageStatus,
};
use pbjson_types::Timestamp;
use prost::{Message, Name};
use serde::Deserialize;
use std::time::SystemTime;
use tracing::{info, warn};

use crate::ErgorsAppState;

/// Well-known action types for the inbox.
pub mod action_types {
    pub const GRANT_REQUEST: &str = "grant_request";
}

fn current_timestamp() -> Timestamp {
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap();
    Timestamp {
        seconds: now.as_secs() as i64,
        nanos: now.subsec_nanos() as i32,
    }
}

// ==================== Request Types ====================

#[derive(Deserialize)]
pub struct SubmitInboxRequest {
    pub action_type: String,
    pub sender_pubkey: String,
    pub summary: String,
    /// JSON-encoded payload (will be stored as bytes)
    pub payload: serde_json::Value,
    /// Proto type URL for the payload (optional, for typed deserialization)
    #[serde(default)]
    pub payload_type_url: String,
}

/// Convenience type for submitting grant requests specifically.
#[derive(Deserialize)]
pub struct SubmitGrantRequest {
    pub requester_pubkey: String,
    pub grantee_address: String,
    pub grant_type: i32,
    pub params: GrantRequestParamsInput,
}

#[derive(Deserialize)]
pub struct GrantRequestParamsInput {
    pub duration_seconds: u64,
    pub spend_limit_uakt: u64,
    #[serde(default)]
    pub msg_types: Vec<String>,
    #[serde(default)]
    pub purpose: String,
}

#[derive(Deserialize)]
pub struct RejectReason {
    #[serde(default)]
    pub reason: String,
}

#[derive(Deserialize)]
pub struct InboxListQuery {
    #[serde(default)]
    pub action_type: Option<String>,
}

// ==================== Public Handlers ====================

/// POST /api/inbox/submit — Submit a generic inbox message
pub async fn handle_submit(
    State(state): State<ErgorsAppState>,
    Json(input): Json<SubmitInboxRequest>,
) -> Json<serde_json::Value> {
    let sender_pubkey = match hex::decode(&input.sender_pubkey) {
        Ok(pk) => pk,
        Err(_) => {
            return Json(error_json("Invalid sender_pubkey hex", "INVALID_PUBKEY"));
        }
    };

    if input.action_type.is_empty() {
        return Json(error_json("action_type is required", "MISSING_ACTION_TYPE"));
    }

    let id = match state.s.next_inbox_id().await {
        Ok(id) => id,
        Err(e) => {
            warn!("Failed to allocate inbox ID: {}", e);
            return Json(error_json("Failed to allocate ID", "INTERNAL_ERROR"));
        }
    };

    let payload_bytes = serde_json::to_vec(&input.payload).unwrap_or_default();
    let now = current_timestamp();

    let msg = InboxMessage {
        id,
        action_type: input.action_type.clone(),
        sender_pubkey,
        payload_type_url: input.payload_type_url,
        payload: payload_bytes,
        status: InboxMessageStatus::Pending as i32,
        summary: input.summary,
        rejection_reason: String::new(),
        result: String::new(),
        created_at: Some(now),
        updated_at: Some(now),
    };

    if let Err(e) = state.s.save_inbox_message(&msg).await {
        warn!("Failed to save inbox message: {}", e);
        return Json(error_json("Failed to save message", "STORAGE_ERROR"));
    }

    info!("Inbox message {} submitted (action: {})", id, input.action_type);

    Json(serde_json::json!({
        "id": msg.id,
        "action_type": msg.action_type,
        "status": msg.status,
        "status_name": InboxMessageStatus::Pending.as_str_name(),
    }))
}

/// POST /api/inbox/grant — Submit a grant request (convenience wrapper)
pub async fn handle_submit_grant(
    State(state): State<ErgorsAppState>,
    Json(input): Json<SubmitGrantRequest>,
) -> Json<serde_json::Value> {
    let requester_pubkey = match hex::decode(&input.requester_pubkey) {
        Ok(pk) => pk,
        Err(_) => {
            return Json(error_json("Invalid requester_pubkey hex", "INVALID_PUBKEY"));
        }
    };

    let grant_type = match GrantType::try_from(input.grant_type) {
        Ok(gt) if gt != GrantType::Unspecified => gt,
        _ => {
            return Json(error_json("Invalid grant_type", "INVALID_GRANT_TYPE"));
        }
    };

    // Load granter config to validate and determine acceptance
    let config = match state.s.get_granter_config().await {
        Ok(Some(c)) => c,
        Ok(None) => {
            return Json(error_json(
                "Granter not configured on this node",
                "GRANTER_NOT_CONFIGURED",
            ));
        }
        Err(e) => {
            warn!("Failed to load granter config: {}", e);
            return Json(error_json("Internal error loading config", "INTERNAL_ERROR"));
        }
    };

    if !config.enabled {
        return Json(error_json("Granter service is disabled", "GRANTER_DISABLED"));
    }

    // Validate against defaults
    if let Some(ref defaults) = config.defaults {
        if input.params.duration_seconds > defaults.max_duration_seconds {
            return Json(error_json(
                "Requested duration exceeds maximum",
                "DURATION_EXCEEDED",
            ));
        }
        if input.params.spend_limit_uakt > defaults.max_spend_limit_uakt {
            return Json(error_json(
                "Requested spend limit exceeds maximum",
                "SPEND_LIMIT_EXCEEDED",
            ));
        }
    }

    // Build the GrantRequest payload
    let grant_request = GrantRequest {
        id: 0, // will be set by inbox ID
        requester_pubkey: requester_pubkey.clone(),
        grantee_address: input.grantee_address.clone(),
        granter_pubkey: vec![],
        granter_address: String::new(),
        grant_type: grant_type as i32,
        params: Some(GrantRequestParams {
            duration_seconds: input.params.duration_seconds,
            spend_limit_uakt: input.params.spend_limit_uakt,
            msg_types: input.params.msg_types,
            purpose: input.params.purpose.clone(),
        }),
        status: GrantRequestStatus::Pending as i32,
        created_at: Some(current_timestamp()),
        updated_at: Some(current_timestamp()),
        tx_hash: String::new(),
        rejection_reason: String::new(),
    };

    // Serialize GrantRequest as proto bytes
    let payload_bytes = grant_request.encode_to_vec();

    let id = match state.s.next_inbox_id().await {
        Ok(id) => id,
        Err(e) => {
            warn!("Failed to allocate inbox ID: {}", e);
            return Json(error_json("Failed to allocate ID", "INTERNAL_ERROR"));
        }
    };

    let now = current_timestamp();
    let summary = format!(
        "Grant request: {} for {} ({}s, {} uakt)",
        GrantType::try_from(grant_type as i32)
            .map(|g| format!("{:?}", g))
            .unwrap_or_default(),
        input.grantee_address,
        input.params.duration_seconds,
        input.params.spend_limit_uakt,
    );

    // Determine initial status based on acceptance mode
    let mode = GrantAcceptanceMode::try_from(config.mode)
        .unwrap_or(GrantAcceptanceMode::RejectAll);

    let (initial_status, rejection_reason) = match mode {
        GrantAcceptanceMode::AcceptAll => (InboxMessageStatus::Accepted, String::new()),
        GrantAcceptanceMode::RejectAll => (
            InboxMessageStatus::Rejected,
            "Granter is in reject-all mode".to_string(),
        ),
        GrantAcceptanceMode::Whitelist => {
            let is_whitelisted = config
                .whitelist
                .iter()
                .any(|e| e.requester_pubkey == requester_pubkey);
            if is_whitelisted {
                (InboxMessageStatus::Accepted, String::new())
            } else {
                (
                    InboxMessageStatus::Rejected,
                    "Requester not whitelisted".to_string(),
                )
            }
        }
        GrantAcceptanceMode::Manual | GrantAcceptanceMode::Unspecified => {
            (InboxMessageStatus::Pending, String::new())
        }
    };

    let msg = InboxMessage {
        id,
        action_type: action_types::GRANT_REQUEST.to_string(),
        sender_pubkey: requester_pubkey,
        payload_type_url: GrantRequest::type_url().to_string(),
        payload: payload_bytes,
        status: initial_status as i32,
        summary,
        rejection_reason: rejection_reason.clone(),
        result: String::new(),
        created_at: Some(now),
        updated_at: Some(now),
    };

    if let Err(e) = state.s.save_inbox_message(&msg).await {
        warn!("Failed to save inbox message: {}", e);
        return Json(error_json("Failed to save message", "STORAGE_ERROR"));
    }

    // Move terminal states to history immediately
    if matches!(
        initial_status,
        InboxMessageStatus::Accepted | InboxMessageStatus::Rejected
    ) {
        let _ = state.s.move_to_inbox_history(id).await;
    }

    info!(
        "Grant inbox message {} submitted (status: {:?})",
        id, initial_status
    );

    Json(serde_json::json!({
        "id": msg.id,
        "action_type": msg.action_type,
        "status": msg.status,
        "status_name": initial_status.as_str_name(),
        "rejection_reason": rejection_reason,
    }))
}

/// GET /api/inbox/{id} — Get status of an inbox message
pub async fn handle_get_message(
    State(state): State<ErgorsAppState>,
    Path(id): Path<u64>,
) -> Json<serde_json::Value> {
    match state.s.get_inbox_message(id).await {
        Ok(Some(msg)) => Json(serde_json::json!({
            "id": msg.id,
            "action_type": msg.action_type,
            "sender_pubkey": hex::encode(&msg.sender_pubkey),
            "payload_type_url": msg.payload_type_url,
            "status": msg.status,
            "status_name": InboxMessageStatus::try_from(msg.status)
                .map(|s| s.as_str_name().to_string())
                .unwrap_or_default(),
            "summary": msg.summary,
            "rejection_reason": msg.rejection_reason,
            "result": msg.result,
            "created_at": msg.created_at,
            "updated_at": msg.updated_at,
        })),
        Ok(None) => Json(error_json("Message not found", "NOT_FOUND")),
        Err(e) => {
            warn!("Failed to get inbox message {}: {}", id, e);
            Json(error_json("Internal error", "INTERNAL_ERROR"))
        }
    }
}

// ==================== Protected Handlers (operator only) ====================

/// GET /api/inbox — List pending inbox messages
pub async fn handle_list_inbox(
    State(state): State<ErgorsAppState>,
    Query(query): Query<InboxListQuery>,
) -> Json<serde_json::Value> {
    let messages = if let Some(ref action_type) = query.action_type {
        state.s.list_inbox_by_action(action_type).await
    } else {
        state
            .s
            .list_inbox_by_status(InboxMessageStatus::Pending as i32)
            .await
    };

    match messages {
        Ok(msgs) => {
            let items: Vec<serde_json::Value> = msgs
                .iter()
                .map(|m| {
                    serde_json::json!({
                        "id": m.id,
                        "action_type": m.action_type,
                        "sender_pubkey": hex::encode(&m.sender_pubkey),
                        "status": m.status,
                        "status_name": InboxMessageStatus::try_from(m.status)
                            .map(|s| s.as_str_name().to_string())
                            .unwrap_or_default(),
                        "summary": m.summary,
                        "created_at": m.created_at,
                    })
                })
                .collect();
            Json(serde_json::json!({
                "count": items.len(),
                "messages": items,
            }))
        }
        Err(e) => {
            warn!("Failed to list inbox: {}", e);
            Json(error_json("Failed to list inbox", "INTERNAL_ERROR"))
        }
    }
}

/// POST /api/inbox/{id}/accept — Accept a pending inbox message
pub async fn handle_accept(
    State(state): State<ErgorsAppState>,
    Path(id): Path<u64>,
) -> Json<serde_json::Value> {
    let msg = match state.s.get_inbox_message(id).await {
        Ok(Some(m)) => m,
        Ok(None) => return Json(error_json("Message not found", "NOT_FOUND")),
        Err(e) => {
            warn!("Failed to get inbox message {}: {}", id, e);
            return Json(error_json("Internal error", "INTERNAL_ERROR"));
        }
    };

    if msg.status != InboxMessageStatus::Pending as i32 {
        return Json(error_json(
            "Message is not in pending state",
            "INVALID_STATUS",
        ));
    }

    match state
        .s
        .update_inbox_status(id, InboxMessageStatus::Accepted as i32, "")
        .await
    {
        Ok(Some(updated)) => {
            let _ = state.s.move_to_inbox_history(id).await;
            info!("Accepted inbox message {} (action: {})", id, updated.action_type);
            Json(serde_json::json!({
                "id": id,
                "status": InboxMessageStatus::Accepted as i32,
                "status_name": InboxMessageStatus::Accepted.as_str_name(),
                "action_type": updated.action_type,
            }))
        }
        Ok(None) => Json(error_json("Message not found", "NOT_FOUND")),
        Err(e) => {
            warn!("Failed to accept message {}: {}", id, e);
            Json(error_json("Failed to accept message", "INTERNAL_ERROR"))
        }
    }
}

/// POST /api/inbox/{id}/reject — Reject a pending inbox message
pub async fn handle_reject(
    State(state): State<ErgorsAppState>,
    Path(id): Path<u64>,
    Json(input): Json<RejectReason>,
) -> Json<serde_json::Value> {
    let msg = match state.s.get_inbox_message(id).await {
        Ok(Some(m)) => m,
        Ok(None) => return Json(error_json("Message not found", "NOT_FOUND")),
        Err(e) => {
            warn!("Failed to get inbox message {}: {}", id, e);
            return Json(error_json("Internal error", "INTERNAL_ERROR"));
        }
    };

    if msg.status != InboxMessageStatus::Pending as i32 {
        return Json(error_json(
            "Message is not in pending state",
            "INVALID_STATUS",
        ));
    }

    let reason = if input.reason.is_empty() {
        "Rejected by operator".to_string()
    } else {
        input.reason
    };

    match state
        .s
        .update_inbox_status(id, InboxMessageStatus::Rejected as i32, &reason)
        .await
    {
        Ok(Some(updated)) => {
            let _ = state.s.move_to_inbox_history(id).await;
            info!("Rejected inbox message {} (action: {}): {}", id, updated.action_type, reason);
            Json(serde_json::json!({
                "id": id,
                "status": InboxMessageStatus::Rejected as i32,
                "status_name": InboxMessageStatus::Rejected.as_str_name(),
                "reason": reason,
            }))
        }
        Ok(None) => Json(error_json("Message not found", "NOT_FOUND")),
        Err(e) => {
            warn!("Failed to reject message {}: {}", id, e);
            Json(error_json("Failed to reject message", "INTERNAL_ERROR"))
        }
    }
}

/// GET /api/inbox/config — Get granter configuration
pub async fn handle_get_granter_config(
    State(state): State<ErgorsAppState>,
) -> Json<serde_json::Value> {
    match state.s.get_granter_config().await {
        Ok(Some(config)) => Json(serde_json::json!({
            "enabled": config.enabled,
            "mode": config.mode,
            "defaults": config.defaults,
            "whitelist_count": config.whitelist.len(),
        })),
        Ok(None) => Json(serde_json::json!({
            "enabled": false,
            "mode": GrantAcceptanceMode::RejectAll as i32,
            "message": "No granter config set",
        })),
        Err(e) => {
            warn!("Failed to get granter config: {}", e);
            Json(error_json("Failed to get config", "INTERNAL_ERROR"))
        }
    }
}

/// POST /api/inbox/config — Update granter configuration
pub async fn handle_update_granter_config(
    State(state): State<ErgorsAppState>,
    Json(config): Json<GranterConfig>,
) -> Json<serde_json::Value> {
    match state.s.save_granter_config(&config).await {
        Ok(()) => {
            info!(
                "Updated granter config: enabled={}, mode={}",
                config.enabled, config.mode
            );
            Json(serde_json::json!({
                "status": "ok",
                "enabled": config.enabled,
                "mode": config.mode,
            }))
        }
        Err(e) => {
            warn!("Failed to save granter config: {}", e);
            Json(error_json("Failed to save config", "STORAGE_ERROR"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_current_timestamp() {
        let ts = current_timestamp();
        assert!(ts.seconds > 0);
    }

    #[test]
    fn test_submit_request_deserialization() {
        let json_str = r#"{
            "action_type": "grant_request",
            "sender_pubkey": "0102030405",
            "summary": "test grant",
            "payload": {"test": true}
        }"#;
        let req: SubmitInboxRequest = serde_json::from_str(json_str).unwrap();
        assert_eq!(req.action_type, "grant_request");
        assert_eq!(req.sender_pubkey, "0102030405");
    }

    #[test]
    fn test_grant_request_deserialization() {
        let json_str = r#"{
            "requester_pubkey": "0102030405",
            "grantee_address": "akash1grantee...",
            "grant_type": 3,
            "params": {
                "duration_seconds": 3600,
                "spend_limit_uakt": 1000000,
                "msg_types": [],
                "purpose": "test deployment"
            }
        }"#;
        let req: SubmitGrantRequest = serde_json::from_str(json_str).unwrap();
        assert_eq!(req.requester_pubkey, "0102030405");
        assert_eq!(req.grant_type, 3);
        assert_eq!(req.params.duration_seconds, 3600);
    }

    #[test]
    fn test_reject_reason_deserialization() {
        let json_str = r#"{"reason": "not authorized"}"#;
        let r: RejectReason = serde_json::from_str(json_str).unwrap();
        assert_eq!(r.reason, "not authorized");

        let json_str = r#"{}"#;
        let r: RejectReason = serde_json::from_str(json_str).unwrap();
        assert_eq!(r.reason, "");
    }

    #[test]
    fn test_inbox_message_status_enum() {
        assert_eq!(InboxMessageStatus::Pending as i32, 1);
        assert_eq!(InboxMessageStatus::Accepted as i32, 2);
        assert_eq!(InboxMessageStatus::Rejected as i32, 3);
    }
}
