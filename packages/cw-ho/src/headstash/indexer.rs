// define logic that poweres registering a websocket subcription

use crate::ErgorsAppState;
use axum::{
    extract::State, Json,
};
use ho_std::llm::HoError;
use ho_std::error::error_json_detailed;

// implement logic for storing headatsh metadata into ipfs gateway/storage for retrieval
pub async fn handle_indexer_instructions(
    State(_state): State<ErgorsAppState>,
    Json(_r): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    // register/remove/update websocket subscription with smart-contract
    // if register, ensure we do not already have subscription for exact same (contract,action)
    // if remove, ensure headstash contract is not active

    Json(error_json_detailed(&HoError::Anyhow(anyhow::format_err!(
        "not yet implemented: `handle_headstash_metadata_storage`"
    ))))
}

// if new headstash is created:
// - download local metadata and ipfs information regarding distribution
