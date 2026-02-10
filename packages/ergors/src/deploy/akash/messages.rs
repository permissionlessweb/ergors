//! Akash Network message types and transaction broadcasting.
//!
//! This module provides:
//! - Message type URLs for Akash operations (used for authz grants)
//! - Transaction broadcasting helpers for layer-climb

use anyhow::{anyhow, Result};
use layer_climb::prelude::*;
use layer_climb_proto::Any as ClimbAny;
use prost::Message;

/// Akash message type URLs for authorization grants.
pub mod msg_types {
    use prost::Name;

    /// All deployment-related message types for full workflow authorization.
    ///
    /// Used when granting authz permissions for deployment operations.
    /// NOTE: Certificate messages removed - JWT authentication is used instead.
    pub fn all_deployment_msg_types() -> Vec<String> {
        vec![
            ho_std::types::ergors::akash::deployment::v1beta4::MsgCreateDeployment::type_url(),
            ho_std::types::ergors::akash::deployment::v1beta4::MsgUpdateDeployment::type_url(),
            ho_std::types::ergors::akash::deployment::v1beta4::MsgCloseDeployment::type_url(),
            ho_std::types::ergors::akash::market::v1beta5::MsgCreateLease::type_url(),
            ho_std::types::ergors::akash::market::v1beta5::MsgCloseBid::type_url(),
            ho_std::types::ergors::akash::market::v1beta5::MsgWithdrawLease::type_url(),
        ]
    }
}

/// Broadcast a single Akash message with standard error handling.
///
/// This helper:
/// - Encodes the message to protobuf
/// - Creates a TxBuilder with the provided memo
/// - Broadcasts and waits for confirmation
/// - Returns error if tx fails (code != 0)
/// - Logs success with tx metadata
pub async fn broadcast_akash_msg<M: Message>(
    client: &SigningClient,
    type_url: &str,
    msg: &M,
    memo: impl Into<String>,
) -> Result<layer_climb_proto::abci::TxResponse> {
    let msg_any = ClimbAny {
        type_url: type_url.to_string(),
        value: msg.encode_to_vec(),
    };

    tracing::debug!(
        "Preparing tx: type={}, size={} bytes",
        type_url,
        msg_any.value.len()
    );

    let mut tx_builder = client.tx_builder();
    tx_builder.set_memo(memo);

    let tx_resp = match tx_builder.broadcast(vec![msg_any]).await {
        Ok(resp) => resp,
        Err(e) => {
            tracing::error!("Broadcast failed for {}: {}", type_url, e);
            return Err(anyhow!("Failed to broadcast {}: {}", type_url, e));
        }
    };

    if tx_resp.code != 0 {
        return Err(anyhow!(
            "Akash tx failed (type: {}, code: {}): {}",
            type_url,
            tx_resp.code,
            tx_resp.raw_log
        ));
    }

    tracing::info!(
        "Akash tx success: type={}, hash={}, height={}, gas={}",
        type_url,
        tx_resp.txhash,
        tx_resp.height,
        tx_resp.gas_used
    );

    Ok(tx_resp)
}

/// Broadcast multiple Akash messages in a single transaction (atomic).
///
/// This allows batching multiple operations into one transaction for:
/// - Atomicity (all succeed or all fail)
/// - Lower gas costs
/// - Faster execution
///
/// # Arguments
/// * `client` - SigningClient to use for broadcasting
/// * `msgs` - Vector of (type_url, encoded_proto_bytes) tuples
/// * `memo` - Transaction memo
pub async fn broadcast_akash_msgs(
    client: &SigningClient,
    msgs: Vec<(&str, Vec<u8>)>,
    memo: impl Into<String>,
) -> Result<layer_climb_proto::abci::TxResponse> {
    let msg_anys: Vec<ClimbAny> = msgs
        .into_iter()
        .map(|(type_url, value)| ClimbAny {
            type_url: type_url.to_string(),
            value,
        })
        .collect();

    let mut tx_builder = client.tx_builder();
    tx_builder.set_memo(memo);
    let tx_resp = tx_builder.broadcast(msg_anys).await?;

    if tx_resp.code != 0 {
        return Err(anyhow!(
            "Akash batch tx failed (code: {}): {}",
            tx_resp.code,
            tx_resp.raw_log
        ));
    }

    tracing::info!(
        "Akash batch tx success: hash={}, height={}, gas={}",
        tx_resp.txhash,
        tx_resp.height,
        tx_resp.gas_used
    );

    Ok(tx_resp)
}
