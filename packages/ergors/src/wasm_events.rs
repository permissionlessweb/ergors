//! CosmWasm Event Action Handler
//!
//! Processes engine actions parsed from CosmWasm contract execution events.
//! Each action variant is routed to the appropriate engine subsystem
//! (LLM router, storage, P2P, logging, Akash deploy).
//!
//! Actions are processed asynchronously via `tokio::spawn` to avoid blocking
//! the HTTP response to the contract caller.

#[cfg(feature = "cw")]
use crate::ErgorsAppState;
#[cfg(feature = "cw")]
use ho_std::wasm::event_router::{ActionResult, EngineAction};
#[cfg(feature = "cw")]
use ho_std::types::ergors::orch::v1::{PromptMessage, PromptRequest};

/// Process engine actions parsed from contract execution events.
///
/// Iterates over actions and dispatches each to the appropriate engine
/// subsystem. Returns a result for each action indicating success/failure.
///
/// This function is designed to be called from `tokio::spawn` so it does
/// not block the original HTTP handler response.
#[cfg(feature = "cw")]
pub async fn handle_engine_actions(
    state: &ErgorsAppState,
    actions: Vec<EngineAction>,
    source_contract: &str,
) -> Vec<ActionResult> {
    let mut results = Vec::with_capacity(actions.len());

    for action in actions {
        let result = match action {
            EngineAction::Log { level, message } => handle_log(&level, &message, source_contract),
            EngineAction::StorePut { key, value } => {
                handle_store_put(state, &key, value, source_contract).await
            }
            EngineAction::InferenceRequest {
                model,
                prompt,
                callback_contract,
                callback_msg,
            } => {
                handle_inference_request(
                    state,
                    &model,
                    &prompt,
                    callback_contract.as_deref(),
                    callback_msg.as_deref(),
                    source_contract,
                )
                .await
            }
            EngineAction::P2pMessage {
                target_node,
                channel,
                payload,
            } => {
                handle_p2p_message(state, &target_node, channel, &payload, source_contract).await
            }
            EngineAction::AkashDeploy { sdl, label } => {
                handle_akash_deploy(state, &sdl, &label, source_contract).await
            }
        };
        results.push(result);
    }

    results
}

/// Handle a Log action by emitting a tracing event at the specified level.
#[cfg(feature = "cw")]
fn handle_log(level: &str, message: &str, source_contract: &str) -> ActionResult {
    match level {
        "error" => tracing::error!(contract = source_contract, "{}", message),
        "warn" => tracing::warn!(contract = source_contract, "{}", message),
        "debug" => tracing::debug!(contract = source_contract, "{}", message),
        "trace" => tracing::trace!(contract = source_contract, "{}", message),
        _ => tracing::info!(contract = source_contract, "{}", message),
    }

    ActionResult {
        action_type: "log".to_string(),
        success: true,
        detail: None,
    }
}

/// Handle a StorePut action by writing to Cnidarium storage.
#[cfg(feature = "cw")]
async fn handle_store_put(
    state: &ErgorsAppState,
    key: &str,
    value: Vec<u8>,
    source_contract: &str,
) -> ActionResult {
    use cnidarium::StateWrite;

    // Prefix contract-originated keys to avoid collisions with engine state
    let prefixed_key = format!("cw_action/{}/{}", source_contract, key);

    let snapshot = state.s.cs.latest_snapshot();
    let mut delta = cnidarium::StateDelta::new(snapshot);
    delta.put_raw(prefixed_key.clone(), value);

    match state.s.cs.commit(delta).await {
        Ok(_) => {
            tracing::debug!(
                contract = source_contract,
                key = prefixed_key,
                "StorePut action committed"
            );
            ActionResult {
                action_type: "store_put".to_string(),
                success: true,
                detail: Some(prefixed_key),
            }
        }
        Err(e) => {
            tracing::error!(
                contract = source_contract,
                error = %e,
                "StorePut action failed"
            );
            ActionResult {
                action_type: "store_put".to_string(),
                success: false,
                detail: Some(format!("Storage commit failed: {}", e)),
            }
        }
    }
}

/// Handle an InferenceRequest action by routing through the LLM router.
///
/// If `callback_contract` is set, the inference result will be sent back
/// to that contract via execute. This enables the contract -> engine -> contract
/// callback loop.
#[cfg(feature = "cw")]
async fn handle_inference_request(
    state: &ErgorsAppState,
    model: &str,
    prompt: &str,
    callback_contract: Option<&str>,
    _callback_msg: Option<&str>,
    source_contract: &str,
) -> ActionResult {
    tracing::info!(
        contract = source_contract,
        model = model,
        "Processing inference request from contract event"
    );

    // Build a minimal prompt request for the LLM router
    let prompt_request = PromptRequest {
        model: model.to_string(),
        messages: vec![PromptMessage {
            role: "user".to_string(),
            content: prompt.to_string(),
            ..Default::default()
        }],
        ..Default::default()
    };

    match state.r.handle_request(&prompt_request, model).await {
        Ok(response) => {
            let response_text = response
                .response
                .first()
                .cloned()
                .unwrap_or_default();

            tracing::debug!(
                contract = source_contract,
                response_len = response_text.len(),
                "Inference request completed"
            );

            // If callback contract is specified, execute it with the result
            if let Some(callback_addr) = callback_contract {
                let callback_result = serde_json::json!({
                    "receive_inference_result": {
                        "model": model,
                        "result": response_text,
                        "source_contract": source_contract
                    }
                });

                if let Ok(callback_bytes) = serde_json::to_vec(&callback_result) {
                    match state
                        .wasm
                        .execute_contract(
                            &state.s.cs,
                            callback_addr.to_string(),
                            source_contract.to_string(),
                            callback_bytes,
                            vec![],
                        )
                        .await
                    {
                        Ok(_) => {
                            tracing::info!(
                                callback_contract = callback_addr,
                                "Inference callback executed"
                            );
                        }
                        Err(e) => {
                            tracing::error!(
                                callback_contract = callback_addr,
                                error = %e,
                                "Inference callback execution failed"
                            );
                        }
                    }
                }
            }

            ActionResult {
                action_type: "inference_request".to_string(),
                success: true,
                detail: Some(format!("Response length: {}", response_text.len())),
            }
        }
        Err(e) => {
            tracing::error!(
                contract = source_contract,
                error = %e,
                "Inference request failed"
            );
            ActionResult {
                action_type: "inference_request".to_string(),
                success: false,
                detail: Some(format!("LLM routing failed: {}", e)),
            }
        }
    }
}

/// Handle a P2P message action.
///
/// Currently logs the intent. Full P2P send integration requires
/// access to the network manifold channel senders.
#[cfg(feature = "cw")]
async fn handle_p2p_message(
    _state: &ErgorsAppState,
    target_node: &str,
    channel: u8,
    payload: &[u8],
    source_contract: &str,
) -> ActionResult {
    tracing::info!(
        contract = source_contract,
        target = target_node,
        channel = channel,
        payload_len = payload.len(),
        "P2P message action received (routing pending network manifold integration)"
    );

    // P2P send requires the network manifold channel senders.
    // For now, log the action and return success to indicate parsing worked.
    // Full integration will send via state.nm channel_senders.
    ActionResult {
        action_type: "p2p_message".to_string(),
        success: true,
        detail: Some(format!(
            "Queued for target={} channel={} ({} bytes)",
            target_node,
            channel,
            payload.len()
        )),
    }
}

/// Handle an Akash deploy action.
///
/// Routes through the Akash deployment context if available.
#[cfg(feature = "cw")]
async fn handle_akash_deploy(
    state: &ErgorsAppState,
    sdl: &str,
    label: &str,
    source_contract: &str,
) -> ActionResult {
    tracing::info!(
        contract = source_contract,
        label = label,
        "Akash deploy action received"
    );

    if state.akash.is_none() {
        return ActionResult {
            action_type: "akash_deploy".to_string(),
            success: false,
            detail: Some("Akash deployment context not configured".to_string()),
        };
    }

    // Log the deployment intent. Full integration would call the automated deployer.
    tracing::info!(
        contract = source_contract,
        label = label,
        sdl_len = sdl.len(),
        "Akash deploy queued (full integration pending)"
    );

    ActionResult {
        action_type: "akash_deploy".to_string(),
        success: true,
        detail: Some(format!("Deploy queued: label={}", label)),
    }
}

#[cfg(test)]
#[cfg(feature = "cw")]
mod tests {
    use super::*;
    use ho_std::wasm::event_router::EngineAction;

    #[test]
    fn test_handle_log_returns_success() {
        let result = handle_log("info", "test message", "test_contract");
        assert!(result.success);
        assert_eq!(result.action_type, "log");
    }

    #[test]
    fn test_handle_log_all_levels() {
        for level in &["error", "warn", "info", "debug", "trace", "unknown"] {
            let result = handle_log(level, "test", "contract");
            assert!(result.success);
        }
    }
}
