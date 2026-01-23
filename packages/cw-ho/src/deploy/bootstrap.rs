use ho_std::types::ergors::orch::v1::*;

use axum::{
    extract::State, Json,
};
use std::time::Instant;

use crate::ErgorsAppState;


pub async fn handle_bootstrap(
    State(..): State<ErgorsAppState>,
    Json(..): Json<BootstrapRequest>,
) -> Json<serde_json::Value> {
    let _start_time = Instant::now();
    let _id = uuid::Uuid::new_v4();

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
