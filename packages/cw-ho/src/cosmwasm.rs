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
use ho_std::{llm::HoError, types::Name};
use ho_std::{error::error_json, network::AuthLayer};
use ho_std::{
    error::{error_json_detailed, HoResult},
    traits::{HoConfigTrait, NetworkTopologyTrait, NodeIdentityTrait},
    types::ergors::{orch::v1::*, storage::v1::*},
};
#[cfg(feature = "cw")]
use std::str::FromStr;
use std::{ops::Deref, sync::Arc, time::Instant};
#[cfg(feature = "cw")]
use cnidarium::{State as CnidariumState, StateDelta, StateRead, StateWrite};
#[cfg(feature = "cw")]
use cosmwasm_std;
use tokio::net::TcpListener;
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use tracing::{error, info};
use uuid::Uuid;

/// `handle_cosmwasm_action`
pub async fn handle_cosmwasm_action(
    State(state): State<ErgorsAppState>,
    Json(r): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    #[cfg(not(feature = "cw"))]
    {
        return Json(error_json_detailed(&HoError::Anyhow(anyhow::format_err!(
            "CosmWasm support not enabled. Build with --features cw"
        ))));
    }

    #[cfg(feature = "cw")]
    {
        use ho_std::types::cosmwasm::wasm::v1::*;
        use ho_std::wasm::WasmRuntime;

        // Extract the message type from the JSON
        let msg_type = match r.get("@type").and_then(|v| v.as_str()) {
            Some(t) => t,
            None => {
                return Json(error_json_detailed(&HoError::Anyhow(anyhow::format_err!(
                    "Missing @type field in message"
                ))));
            }
        };

        match msg_type {
            "/cosmwasm.wasm.v1.MsgStoreCode" => {
                match serde_json::from_value::<MsgStoreCode>(r) {
                    Ok(msg) => {
                        match state.wasm.store_code(&mut state.s.cs, msg.wasm_byte_code, msg.sender).await {
                            Ok(code_id) => {
                                let response = MsgStoreCodeResponse {
                                    code_id,
                                    checksum: vec![], // TODO: Return actual checksum
                                };
                                Json(serde_json::to_value(response).unwrap_or_else(|_| serde_json::json!({"error": "Serialization failed"})))
                            }
                            Err(e) => Json(error_json_detailed(&e)),
                        }
                    }
                    Err(e) => Json(error_json_detailed(&HoError::Anyhow(anyhow::format_err!(
                        "Failed to parse MsgStoreCode: {}", e
                    )))),
                }
            }
            "/cosmwasm.wasm.v1.MsgInstantiateContract" => {
                match serde_json::from_value::<MsgInstantiateContract>(r) {
                    Ok(msg) => {
                        match state.wasm.instantiate_contract(
                            &mut state.s.cs,
                            msg.code_id,
                            msg.sender,
                            if msg.admin.is_empty() { None } else { Some(msg.admin) },
                            msg.label,
                            msg.msg,
                            msg.funds.into_iter().map(|c| {
                                cosmwasm_std::Coin {
                                    denom: c.denom,
                                    amount: cosmwasm_std::Uint256::from_str(&c.amount).unwrap_or_default(),
                                }
                            }).collect(),
                        ).await {
                            Ok((contract_addr, response)) => {
                                let response_data = MsgInstantiateContractResponse {
                                    address: contract_addr,
                                    data: response.into_result().unwrap_or_default().data.map(|b| b.into()).unwrap_or_default(),
                                };
                                Json(serde_json::to_value(response_data).unwrap_or_else(|_| serde_json::json!({"error": "Serialization failed"})))
                            }
                            Err(e) => Json(error_json_detailed(&e)),
                        }
                    }
                    Err(e) => Json(error_json_detailed(&HoError::Anyhow(anyhow::format_err!(
                        "Failed to parse MsgInstantiateContract: {}", e
                    )))),
                }
            }
            "/cosmwasm.wasm.v1.MsgExecuteContract" => {
                match serde_json::from_value::<MsgExecuteContract>(r) {
                    Ok(msg) => {
                        match state.wasm.execute_contract(
                            &mut state.s.cs,
                            msg.contract,
                            msg.sender,
                            msg.msg,
                            msg.funds.into_iter().map(|c| {
                                cosmwasm_std::Coin {
                                    denom: c.denom,
                                    amount: cosmwasm_std::Uint256::from_str(&c.amount).unwrap_or_default(),
                                }
                            }).collect(),
                        ).await {
                            Ok(response) => {
                                let response_data = MsgExecuteContractResponse {
                                    data: response.into_result().unwrap_or_default().data.map(|b| b.into()).unwrap_or_default(),
                                };
                                Json(serde_json::to_value(response_data).unwrap_or_else(|_| serde_json::json!({"error": "Serialization failed"})))
                            }
                            Err(e) => Json(error_json_detailed(&e)),
                        }
                    }
                    Err(e) => Json(error_json_detailed(&HoError::Anyhow(anyhow::format_err!(
                        "Failed to parse MsgExecuteContract: {}", e
                    )))),
                }
            }
            _ => Json(error_json_detailed(&HoError::Anyhow(anyhow::format_err!(
                "Unsupported message type: {}", msg_type
            )))),
        }
    }
}
