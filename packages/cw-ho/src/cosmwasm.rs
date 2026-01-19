//! CosmWasm HTTP handlers for ERGORS
//!
//! Provides HTTP endpoints for CosmWasm contract operations.

use crate::ErgorsAppState;
use axum::{extract::State, Json};
use ho_std::error::error_json_detailed;
use ho_std::llm::HoError;
use ho_std::traits::{HoConfigTrait, NodeIdentityTrait};
use ho_std::types::Name;
use std::str::FromStr;

/// Handle CosmWasm actions
///
/// This endpoint receives CosmWasm messages and routes them to the appropriate
/// runtime operations (store_code, instantiate, execute, query).
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
        use ho_std::types::cosmwasm::wasm::v1::{
            MsgExecuteContract, MsgExecuteContractResponse, MsgInstantiateContract,
            MsgInstantiateContractResponse, MsgStoreCode, MsgStoreCodeResponse,
        };

        // Extract the message type from the JSON
        let msg_type = match r.get("@type").and_then(|v| v.as_str()) {
            Some(t) => t,
            None => {
                return Json(error_json_detailed(&HoError::Anyhow(anyhow::format_err!(
                    "Missing @type field in message"
                ))));
            }
        };

        // Route based on message type using type_url() for type safety
        if msg_type == MsgStoreCode::type_url() {
            handle_store_code(state, r).await
        } else if msg_type == MsgInstantiateContract::type_url() {
            handle_instantiate_contract(state, r).await
        } else if msg_type == MsgExecuteContract::type_url() {
            handle_execute_contract(state, r).await
        } else {
            Json(error_json_detailed(&HoError::Anyhow(anyhow::format_err!(
                "Unsupported message type: {}",
                msg_type
            ))))
        }
    }
}

#[cfg(feature = "cw")]
async fn handle_store_code(state: ErgorsAppState, r: serde_json::Value) -> Json<serde_json::Value> {
    use ho_std::types::cosmwasm::wasm::v1::{MsgStoreCode, MsgStoreCodeResponse};

    match serde_json::from_value::<MsgStoreCode>(r) {
        Ok(msg) => {
            match state
                .wasm
                .store_code(&state.s.cs, msg.wasm_byte_code, msg.sender)
                .await
            {
                Ok(code_id) => {
                    let response = MsgStoreCodeResponse {
                        code_id,
                        checksum: vec![], // TODO: Return actual checksum from WasmRuntime
                    };
                    Json(
                        serde_json::to_value(response).unwrap_or_else(
                            |_| serde_json::json!({"error": "Serialization failed"}),
                        ),
                    )
                }
                Err(e) => Json(error_json_detailed(&e)),
            }
        }
        Err(e) => Json(error_json_detailed(&HoError::Anyhow(anyhow::format_err!(
            "Failed to parse MsgStoreCode: {}",
            e
        )))),
    }
}

#[cfg(feature = "cw")]
async fn handle_instantiate_contract(
    state: ErgorsAppState,
    r: serde_json::Value,
) -> Json<serde_json::Value> {
    use ho_std::types::cosmwasm::wasm::v1::{
        MsgInstantiateContract, MsgInstantiateContractResponse,
    };

    match serde_json::from_value::<MsgInstantiateContract>(r) {
        Ok(msg) => {
            // Convert funds from proto Coin to cosmwasm_std::Coin
            let funds: Vec<cosmwasm_std::Coin> = msg
                .funds
                .into_iter()
                .map(|c| cosmwasm_std::Coin {
                    denom: c.denom,
                    amount: cosmwasm_std::Uint256::from_str(&c.amount).unwrap_or_default(),
                })
                .collect();

            let admin = if msg.admin.is_empty() {
                None
            } else {
                Some(msg.admin)
            };

            match state
                .wasm
                .instantiate_contract(
                    &state.s.cs,
                    msg.code_id,
                    msg.sender,
                    admin,
                    msg.label,
                    msg.msg,
                    funds,
                    &state.c.identity().display_id(),
                )
                .await
            {
                Ok((contract_addr, response)) => {
                    let data = response
                        .into_result()
                        .ok()
                        .and_then(|r| r.data)
                        .map(|b| b.to_vec())
                        .unwrap_or_default();

                    let response_data = MsgInstantiateContractResponse {
                        address: contract_addr,
                        data,
                    };
                    Json(
                        serde_json::to_value(response_data).unwrap_or_else(
                            |_| serde_json::json!({"error": "Serialization failed"}),
                        ),
                    )
                }
                Err(e) => Json(error_json_detailed(&e)),
            }
        }
        Err(e) => Json(error_json_detailed(&HoError::Anyhow(anyhow::format_err!(
            "Failed to parse MsgInstantiateContract: {}",
            e
        )))),
    }
}

#[cfg(feature = "cw")]
async fn handle_execute_contract(
    state: ErgorsAppState,
    r: serde_json::Value,
) -> Json<serde_json::Value> {
    use ho_std::types::cosmwasm::wasm::v1::{MsgExecuteContract, MsgExecuteContractResponse};

    match serde_json::from_value::<MsgExecuteContract>(r) {
        Ok(msg) => {
            // Convert funds from proto Coin to cosmwasm_std::Coin
            let funds: Vec<cosmwasm_std::Coin> = msg
                .funds
                .into_iter()
                .map(|c| cosmwasm_std::Coin {
                    denom: c.denom,
                    amount: cosmwasm_std::Uint256::from_str(&c.amount).unwrap_or_default(),
                })
                .collect();

            match state
                .wasm
                .execute_contract(&state.s.cs, msg.contract, msg.sender, msg.msg, funds)
                .await
            {
                Ok(response) => {
                    let data = response
                        .into_result()
                        .ok()
                        .and_then(|r| r.data)
                        .map(|b| b.to_vec())
                        .unwrap_or_default();

                    let response_data = MsgExecuteContractResponse { data };
                    Json(
                        serde_json::to_value(response_data).unwrap_or_else(
                            |_| serde_json::json!({"error": "Serialization failed"}),
                        ),
                    )
                }
                Err(e) => Json(error_json_detailed(&e)),
            }
        }
        Err(e) => Json(error_json_detailed(&HoError::Anyhow(anyhow::format_err!(
            "Failed to parse MsgExecuteContract: {}",
            e
        )))),
    }
}
