use crate::{
    middleware::record_operation, storage::ErgorsStorage, ErgorsAppState, ErgorsConfig,
    ErgorsNetworkManifold, LlmRouter,
};
use axum::{
    extract::{Query, State},
    middleware, Json, Router,
};
use commonware_cryptography::{blake3, Hasher};
use commonware_runtime::tokio::Context;
use ho_std::llm::HoError;
use ho_std::{error::error_json, network::AuthLayer};
use ho_std::{
    error::{error_json_detailed, HoResult},
    traits::{HoConfigTrait, NetworkTopologyTrait, NodeIdentityTrait},
    types::ergors::{orch::v1::*, storage::v1::*},
};
use std::{ops::Deref, sync::Arc, time::Instant};
use tokio::net::TcpListener;
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use tracing::{error, info};
use uuid::Uuid;

// implement logic for storing headatsh metadata into ipfs gateway/storage for retrieval
pub async fn handle_headstash_metadata_storage(
    State(state): State<ErgorsAppState>,
    Json(r): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    Json(error_json_detailed(&HoError::Anyhow(anyhow::format_err!(
        "not yet implemented: `handle_headstash_metadata_storage`"
    ))))
}
