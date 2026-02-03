use crate::ErgorsAppState;
use axum::{extract::State, Json};
use ho_std::error::error_json_detailed;
use ho_std::llm::HoError;

// define api used to route headstash claims via vote-extensions

// - define MsgClaimHeadstash proto: all proof public inputs + proof (and headstash contract addr)
// - validate proof, save verified details to storage/mempool (msg prooved, prood details)

pub async fn handle_headstash_claim(
    State(_state): State<ErgorsAppState>,
    Json(_r): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    // parse MsgClaimHeadstash
    let _suite = zk_headstash::deploy::HeadstashSuite::new();
    // verify halo2-proof

    // save pending claim to mempool for use (inside dedicated storage layer of node)

    Json(error_json_detailed(&HoError::Anyhow(anyhow::format_err!(
        "not yet implemented"
    ))))
}
