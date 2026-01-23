use crate::ErgorsAppState;
use axum::{
    extract::State, Json,
};
use ho_std::llm::HoError;
use ho_std::error::error_json_detailed;

// implement logic for storing headatsh metadata into ipfs gateway/storage for retrieval
pub async fn handle_vote_extension(
    State(_state): State<ErgorsAppState>,
    Json(_r): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    // ensure request is coming from known validator

    // retrieve any pending nullifiers/claim details from storage/mempool to include in voteExt

    // respond to validator with these values.

    Json(error_json_detailed(&HoError::Anyhow(anyhow::format_err!(
        "not yet implemented: `handle_headstash_metadata_storage`"
    ))))
}
