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
            MsgExecuteContract, MsgInstantiateContract, MsgStoreCode,
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
                Ok((code_id, checksum)) => {
                    let response = MsgStoreCodeResponse {
                        code_id,
                        checksum: checksum.into(),  
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
    use ho_std::wasm::event_router::{parse_engine_actions, parse_response_attributes};

    match serde_json::from_value::<MsgExecuteContract>(r) {
        Ok(msg) => {
            let contract_addr = msg.contract.clone();

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
                    let data = match response.into_result() {
                        Ok(ref res) => {
                            // Parse events for engine actions before consuming data
                            let event_actions = parse_engine_actions(&res.events);
                            let attr_actions = parse_response_attributes(&res.attributes);

                            let all_actions: Vec<_> = event_actions
                                .into_iter()
                                .chain(attr_actions.into_iter())
                                .collect();

                            if !all_actions.is_empty() {
                                let state_clone = state.clone();
                                let contract = contract_addr.clone();
                                tokio::spawn(async move {
                                    crate::wasm_events::handle_engine_actions(
                                        &state_clone,
                                        all_actions,
                                        &contract,
                                    )
                                    .await;
                                });
                            }

                            res.data.as_ref().map(|b| b.to_vec()).unwrap_or_default()
                        }
                        Err(_) => Vec::new(),
                    };

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

// =============================================================================
// Generic CosmWasm Query Handler (HTTP API)
// =============================================================================

/// Request for querying a CosmWasm contract
#[derive(serde::Deserialize)]
pub struct CosmWasmQueryRequest {
    /// Contract address to query
    pub contract: String,
    /// Query message (JSON object matching the contract's QueryMsg)
    pub query: serde_json::Value,
}

/// Generic CosmWasm contract query endpoint
///
/// POST /api/cosmwasm/query
///
/// This is the single entry point for all CosmWasm contract queries.
/// The caller provides the contract address and query message, and receives
/// the raw contract response.
///
/// Example request body:
/// ```json
/// {
///   "contract": "ergors_abc123_sdl",
///   "query": { "get_template": {} }
/// }
/// ```
pub async fn handle_cosmwasm_query(
    State(state): State<ErgorsAppState>,
    Json(req): Json<CosmWasmQueryRequest>,
) -> Json<serde_json::Value> {
    #[cfg(not(feature = "cw"))]
    {
        return Json(error_json_detailed(&HoError::Anyhow(anyhow::format_err!(
            "CosmWasm support not enabled. Build with --features cw"
        ))));
    }

    #[cfg(feature = "cw")]
    {
        // Serialize query message to bytes
        let query_bytes = match serde_json::to_vec(&req.query) {
            Ok(bytes) => bytes,
            Err(e) => {
                return Json(error_json_detailed(&HoError::Anyhow(anyhow::format_err!(
                    "Failed to serialize query message: {}",
                    e
                ))));
            }
        };

        // Execute the query against the contract
        match state
            .wasm
            .query_contract(&state.s.cs, req.contract.clone(), query_bytes)
            .await
        {
            Ok(result) => {
                // Extract response from ContractResult
                match result {
                    cosmwasm_std::ContractResult::Ok(binary) => {
                        // Try to parse as JSON for better response format
                        match serde_json::from_slice::<serde_json::Value>(&binary) {
                            Ok(json_response) => Json(serde_json::json!({
                                "contract": req.contract,
                                "data": json_response
                            })),
                            Err(_) => {
                                // Return raw bytes as base64 if not valid JSON
                                Json(serde_json::json!({
                                    "contract": req.contract,
                                    "data_raw": base64::Engine::encode(
                                        &base64::engine::general_purpose::STANDARD,
                                        &binary
                                    )
                                }))
                            }
                        }
                    }
                    cosmwasm_std::ContractResult::Err(err) => Json(error_json_detailed(
                        &HoError::Anyhow(anyhow::format_err!("Contract query failed: {}", err)),
                    )),
                }
            }
            Err(e) => Json(error_json_detailed(&e)),
        }
    }
}

// =============================================================================
// Generic CosmWasm Execute Handler (HTTP API)
// =============================================================================

/// Coin type for execute requests
#[derive(serde::Deserialize, Default)]
pub struct CoinInput {
    pub denom: String,
    pub amount: String,
}

/// Request for executing a CosmWasm contract
#[derive(serde::Deserialize)]
pub struct CosmWasmExecuteRequest {
    /// Contract address to execute
    pub contract: String,
    /// Sender address
    pub sender: String,
    /// Execute message (JSON object matching the contract's ExecuteMsg)
    pub msg: serde_json::Value,
    /// Optional funds to send with the execution
    #[serde(default)]
    pub funds: Vec<CoinInput>,
}

/// Generic CosmWasm contract execute endpoint
///
/// POST /api/cosmwasm/execute
///
/// This is the single entry point for all CosmWasm contract executions.
/// The caller provides the contract address, sender, message, and optional funds.
///
/// Example request body:
/// ```json
/// {
///   "contract": "ergors_abc123_sdl",
///   "sender": "akash1...",
///   "msg": { "update_template": { "sdl": "..." } },
///   "funds": [{ "denom": "uakt", "amount": "1000000" }]
/// }
/// ```
pub async fn handle_cosmwasm_execute(
    State(state): State<ErgorsAppState>,
    Json(req): Json<CosmWasmExecuteRequest>,
) -> Json<serde_json::Value> {
    #[cfg(not(feature = "cw"))]
    {
        return Json(error_json_detailed(&HoError::Anyhow(anyhow::format_err!(
            "CosmWasm support not enabled. Build with --features cw"
        ))));
    }

    #[cfg(feature = "cw")]
    {
        // Serialize execute message to bytes
        let msg_bytes = match serde_json::to_vec(&req.msg) {
            Ok(bytes) => bytes,
            Err(e) => {
                return Json(error_json_detailed(&HoError::Anyhow(anyhow::format_err!(
                    "Failed to serialize execute message: {}",
                    e
                ))));
            }
        };

        // Convert funds to cosmwasm_std::Coin
        let funds: Vec<cosmwasm_std::Coin> = req
            .funds
            .into_iter()
            .map(|c| cosmwasm_std::Coin {
                denom: c.denom,
                amount: cosmwasm_std::Uint256::from_str(&c.amount).unwrap_or_default(),
            })
            .collect();

        // Execute the contract
        match state
            .wasm
            .execute_contract(
                &state.s.cs,
                req.contract.clone(),
                req.sender.clone(),
                msg_bytes,
                funds,
            )
            .await
        {
            Ok(response) => {
                // Extract data and events from response
                match response.into_result() {
                    Ok(sub_response) => {
                        // Parse engine actions from events before serializing
                        use ho_std::wasm::event_router::{
                            parse_engine_actions, parse_response_attributes,
                        };

                        let event_actions = parse_engine_actions(&sub_response.events);
                        let attr_actions = parse_response_attributes(&sub_response.attributes);

                        let all_actions: Vec<_> = event_actions
                            .into_iter()
                            .chain(attr_actions.into_iter())
                            .collect();

                        if !all_actions.is_empty() {
                            let state_clone = state.clone();
                            let contract = req.contract.clone();
                            tokio::spawn(async move {
                                crate::wasm_events::handle_engine_actions(
                                    &state_clone,
                                    all_actions,
                                    &contract,
                                )
                                .await;
                            });
                        }

                        let data = sub_response.data.map(|b| {
                            base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &b)
                        });

                        let events: Vec<serde_json::Value> = sub_response
                            .events
                            .into_iter()
                            .map(|e| {
                                serde_json::json!({
                                    "type": e.ty,
                                    "attributes": e.attributes.into_iter().map(|a| {
                                        serde_json::json!({
                                            "key": a.key,
                                            "value": a.value
                                        })
                                    }).collect::<Vec<_>>()
                                })
                            })
                            .collect();

                        Json(serde_json::json!({
                            "contract": req.contract,
                            "sender": req.sender,
                            "data": data,
                            "events": events
                        }))
                    }
                    Err(err) => Json(error_json_detailed(&HoError::Anyhow(anyhow::format_err!(
                        "Contract execution failed: {}",
                        err
                    )))),
                }
            }
            Err(e) => Json(error_json_detailed(&e)),
        }
    }
}

// =============================================================================
// Generic CosmWasm Store Handler (HTTP API)
// =============================================================================

/// Request for storing (uploading) WASM code
#[derive(serde::Deserialize)]
pub struct CosmWasmStoreRequest {
    /// Sender/creator address
    pub sender: String,
    /// WASM bytecode as base64-encoded string
    pub wasm_byte_code: String,
}

/// Generic CosmWasm code store (upload) endpoint
///
/// POST /api/cosmwasm/store
///
/// Uploads WASM bytecode to the VM and returns a code_id.
///
/// Example request body:
/// ```json
/// {
///   "sender": "akash1...",
///   "wasm_byte_code": "AGFzbQEAAAA..."
/// }
/// ```
pub async fn handle_cosmwasm_store(
    State(state): State<ErgorsAppState>,
    Json(req): Json<CosmWasmStoreRequest>,
) -> Json<serde_json::Value> {
    #[cfg(not(feature = "cw"))]
    {
        return Json(error_json_detailed(&HoError::Anyhow(anyhow::format_err!(
            "CosmWasm support not enabled. Build with --features cw"
        ))));
    }

    #[cfg(feature = "cw")]
    {
        // Decode base64 WASM bytecode
        let wasm_bytes = match base64::Engine::decode(
            &base64::engine::general_purpose::STANDARD,
            &req.wasm_byte_code,
        ) {
            Ok(bytes) => bytes,
            Err(e) => {
                return Json(error_json_detailed(&HoError::Anyhow(anyhow::format_err!(
                    "Failed to decode wasm_byte_code from base64: {}",
                    e
                ))));
            }
        };

        // Store the code
        match state
            .wasm
            .store_code(&state.s.cs, wasm_bytes, req.sender.clone())
            .await
        {
            Ok(code_id) => Json(serde_json::json!({
                "code_id": code_id,
                "sender": req.sender
            })),
            Err(e) => Json(error_json_detailed(&e)),
        }
    }
}

// =============================================================================
// Generic CosmWasm Instantiate Handler (HTTP API)
// =============================================================================

/// Request for instantiating a contract
#[derive(serde::Deserialize)]
pub struct CosmWasmInstantiateRequest {
    /// Code ID of the uploaded WASM
    pub code_id: u64,
    /// Sender/creator address
    pub sender: String,
    /// Optional admin address
    #[serde(default)]
    pub admin: Option<String>,
    /// Human-readable label for the contract
    pub label: String,
    /// Instantiate message (JSON object matching the contract's InstantiateMsg)
    pub msg: serde_json::Value,
    /// Optional funds to send with instantiation
    #[serde(default)]
    pub funds: Vec<CoinInput>,
}

/// Generic CosmWasm contract instantiate endpoint
///
/// POST /api/cosmwasm/instantiate
///
/// Instantiates a contract from uploaded code.
///
/// Example request body:
/// ```json
/// {
///   "code_id": 1,
///   "sender": "akash1...",
///   "admin": "akash1...",
///   "label": "my-contract",
///   "msg": { "owner": "akash1..." },
///   "funds": [{ "denom": "uakt", "amount": "1000000" }]
/// }
/// ```
pub async fn handle_cosmwasm_instantiate(
    State(state): State<ErgorsAppState>,
    Json(req): Json<CosmWasmInstantiateRequest>,
) -> Json<serde_json::Value> {
    #[cfg(not(feature = "cw"))]
    {
        return Json(error_json_detailed(&HoError::Anyhow(anyhow::format_err!(
            "CosmWasm support not enabled. Build with --features cw"
        ))));
    }

    #[cfg(feature = "cw")]
    {
        use ho_std::traits::{HoConfigTrait, NodeIdentityTrait};

        // Serialize instantiate message to bytes
        let msg_bytes = match serde_json::to_vec(&req.msg) {
            Ok(bytes) => bytes,
            Err(e) => {
                return Json(error_json_detailed(&HoError::Anyhow(anyhow::format_err!(
                    "Failed to serialize instantiate message: {}",
                    e
                ))));
            }
        };

        // Convert funds to cosmwasm_std::Coin
        let funds: Vec<cosmwasm_std::Coin> = req
            .funds
            .into_iter()
            .map(|c| cosmwasm_std::Coin {
                denom: c.denom,
                amount: cosmwasm_std::Uint256::from_str(&c.amount).unwrap_or_default(),
            })
            .collect();

        // Instantiate the contract
        match state
            .wasm
            .instantiate_contract(
                &state.s.cs,
                req.code_id,
                req.sender.clone(),
                req.admin.clone(),
                req.label.clone(),
                msg_bytes,
                funds,
                &state.c.identity().display_id(),
            )
            .await
        {
            Ok((contract_address, response)) => match response.into_result() {
                Ok(sub_response) => {
                    let data = sub_response.data.map(|b| {
                        base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &b)
                    });

                    let events: Vec<serde_json::Value> = sub_response
                        .events
                        .into_iter()
                        .map(|e| {
                            serde_json::json!({
                                "type": e.ty,
                                "attributes": e.attributes.into_iter().map(|a| {
                                    serde_json::json!({
                                        "key": a.key,
                                        "value": a.value
                                    })
                                }).collect::<Vec<_>>()
                            })
                        })
                        .collect();

                    Json(serde_json::json!({
                        "contract_address": contract_address,
                        "code_id": req.code_id,
                        "sender": req.sender,
                        "admin": req.admin,
                        "label": req.label,
                        "data": data,
                        "events": events
                    }))
                }
                Err(err) => Json(error_json_detailed(&HoError::Anyhow(anyhow::format_err!(
                    "Contract instantiation failed: {}",
                    err
                )))),
            },
            Err(e) => Json(error_json_detailed(&e)),
        }
    }
}

// =============================================================================
// Generic CosmWasm Instantiate2 Handler (HTTP API)
// =============================================================================

/// Request for instantiating a contract with predictable address (instantiate2)
#[derive(serde::Deserialize)]
pub struct CosmWasmInstantiate2Request {
    /// Code ID of the uploaded WASM
    pub code_id: u64,
    /// Sender/creator address
    pub sender: String,
    /// Optional admin address
    #[serde(default)]
    pub admin: Option<String>,
    /// Human-readable label for the contract
    pub label: String,
    /// Instantiate message (JSON object matching the contract's InstantiateMsg)
    pub msg: serde_json::Value,
    /// Optional funds to send with instantiation
    #[serde(default)]
    pub funds: Vec<CoinInput>,
    /// Salt for predictable address generation (base64-encoded)
    pub salt: String,
}

/// Generic CosmWasm contract instantiate2 endpoint
///
/// POST /api/cosmwasm/instantiate2
///
/// Instantiates a contract with a predictable address using a salt.
///
/// Example request body:
/// ```json
/// {
///   "code_id": 1,
///   "sender": "akash1...",
///   "admin": "akash1...",
///   "label": "my-contract",
///   "msg": { "owner": "akash1..." },
///   "funds": [],
///   "salt": "bXlzYWx0"
/// }
/// ```
pub async fn handle_cosmwasm_instantiate2(
    State(state): State<ErgorsAppState>,
    Json(req): Json<CosmWasmInstantiate2Request>,
) -> Json<serde_json::Value> {
    #[cfg(not(feature = "cw"))]
    {
        return Json(error_json_detailed(&HoError::Anyhow(anyhow::format_err!(
            "CosmWasm support not enabled. Build with --features cw"
        ))));
    }

    #[cfg(feature = "cw")]
    {
        use ho_std::traits::{HoConfigTrait, NodeIdentityTrait};
        use sha2::{Digest, Sha256};

        // Decode salt from base64
        let salt_bytes =
            match base64::Engine::decode(&base64::engine::general_purpose::STANDARD, &req.salt) {
                Ok(bytes) => bytes,
                Err(e) => {
                    return Json(error_json_detailed(&HoError::Anyhow(anyhow::format_err!(
                        "Failed to decode salt from base64: {}",
                        e
                    ))));
                }
            };

        // Serialize instantiate message to bytes
        let msg_bytes = match serde_json::to_vec(&req.msg) {
            Ok(bytes) => bytes,
            Err(e) => {
                return Json(error_json_detailed(&HoError::Anyhow(anyhow::format_err!(
                    "Failed to serialize instantiate message: {}",
                    e
                ))));
            }
        };

        // Convert funds to cosmwasm_std::Coin
        let funds: Vec<cosmwasm_std::Coin> = req
            .funds
            .into_iter()
            .map(|c| cosmwasm_std::Coin {
                denom: c.denom,
                amount: cosmwasm_std::Uint256::from_str(&c.amount).unwrap_or_default(),
            })
            .collect();

        // Generate predictable contract address using salt
        // Address = hash(code_id || sender || salt || label)
        let mut hasher = Sha256::new();
        hasher.update(req.code_id.to_be_bytes());
        hasher.update(req.sender.as_bytes());
        hasher.update(&salt_bytes);
        hasher.update(req.label.as_bytes());
        let hash = hasher.finalize();
        let predictable_suffix = hex::encode(&hash[..16]);

        // Create a modified label that includes the salt hash for predictable addressing
        let salted_label = format!("{}_{}", req.label, predictable_suffix);

        // Instantiate the contract (address generation in runtime uses label)
        match state
            .wasm
            .instantiate_contract(
                &state.s.cs,
                req.code_id,
                req.sender.clone(),
                req.admin.clone(),
                salted_label.clone(),
                msg_bytes,
                funds,
                &state.c.identity().display_id(),
            )
            .await
        {
            Ok((contract_address, response)) => match response.into_result() {
                Ok(sub_response) => {
                    let data = sub_response.data.map(|b| {
                        base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &b)
                    });

                    let events: Vec<serde_json::Value> = sub_response
                        .events
                        .into_iter()
                        .map(|e| {
                            serde_json::json!({
                                "type": e.ty,
                                "attributes": e.attributes.into_iter().map(|a| {
                                    serde_json::json!({
                                        "key": a.key,
                                        "value": a.value
                                    })
                                }).collect::<Vec<_>>()
                            })
                        })
                        .collect();

                    Json(serde_json::json!({
                        "contract_address": contract_address,
                        "code_id": req.code_id,
                        "sender": req.sender,
                        "admin": req.admin,
                        "label": req.label,
                        "salt": req.salt,
                        "data": data,
                        "events": events
                    }))
                }
                Err(err) => Json(error_json_detailed(&HoError::Anyhow(anyhow::format_err!(
                    "Contract instantiation failed: {}",
                    err
                )))),
            },
            Err(e) => Json(error_json_detailed(&e)),
        }
    }
}
