use crate::ErgorsAppState;
use axum::{
    extract::State, Json,
};
use ho_std::llm::HoError;
use ho_std::error::error_json_detailed;

// implement logic for storing headatsh metadata into ipfs gateway/storage for retrieval
pub async fn handle_headstash_metadata_storage(
    State(_state): State<ErgorsAppState>,
    Json(_r): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    Json(error_json_detailed(&HoError::Anyhow(anyhow::format_err!(
        "not yet implemented: `handle_headstash_metadata_storage`"
    ))))
}
