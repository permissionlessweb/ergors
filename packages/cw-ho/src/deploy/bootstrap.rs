use crate::types::ergors::orch::v1::*;

use axum::{
    extract::{Query, State},
    middleware, Json, Router,
};
use commonware_cryptography::{blake3, Hasher};

use crate::{
    error::{error_json, error_json_detailed, HoResult},
    network::AuthLayer,
    types::ergors::{orch::v1::*, storage::v1::*},
};

use tracing::{error, info};
use uuid::Uuid;

pub async fn handle_bootstrap(
    State(..): State<ErgorsAppState>,
    Json(..): Json<BootstrapRequest>,
) -> Json<serde_json::Value> {
    let start_time = Instant::now();
    let id = uuid::Uuid::new_v4();

    // TODO: handle bootstrap via method:
    // /ergors.network.v1.bootstrap.types: (transport connections for nodes,bootstrapping types used in functions for traits )
    // Create persistent SSH connection manager
    // info!("🚀 Starting bootstrap process for node: {}", target_node);
    // let mut ssh_manager = SSHConnectionManager::new(target_node.clone());

    // match ssh_manager.bootstrap_node().await {
    //     Ok(bootstrap_summary) => {
    //         info!(
    //             "✅ Bootstrap completed successfully for node: {}",
    //             target_node
    //         );

    //         // Close SSH connection before returning
    //         let _ = ssh_manager.close().await;

    //         let response = BootstrapResponse {
    //             id: id.to_string(),
    //             target_node: target_node.clone(),
    //             status: "success".to_string(),
    //             summary: bootstrap_summary,
    //             timestamp: Some(chrono::Utc::now().into()),
    //             duration_ms: start_time.elapsed().as_millis() as u64,
    //         };

    //         Json(serde_json::to_value(response).unwrap())
    //     }
    //     Err(e) => {
    //         error!("Bootstrap failed for node {}: {}", target_node, e);

    //         // Close SSH connection before returning error
    //         let _ = ssh_manager.close().await;

    //         Json(error_json(
    //             &format!("Bootstrap failed: {}", e),
    //             "BOOTSTRAP_ERROR",
    //         ))
    //     }
    // }
    unimplemented!()
}
