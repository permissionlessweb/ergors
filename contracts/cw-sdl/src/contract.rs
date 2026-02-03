use cosmwasm_std::{
    entry_point, to_json_binary, Binary, Deps, DepsMut, Env, MessageInfo, Reply, Response,
    StdResult, StdError, SubMsg, WasmMsg, CosmosMsg,
};
use cw2::set_contract_version;

use crate::error::ContractError;
use crate::msg::{ExecuteMsg, InstantiateMsg, QueryMsg, InstantiateNewResponse};
use crate::state::{Config, CONFIG, SDL_TEMPLATE, VARIABLE_DEFAULTS, DEPLOYMENT_RESULTS, CHILD_CONTRACTS};
use crate::validation::validate_template_variables;

const CONTRACT_NAME: &str = "crates.io:cw-sdl";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Reply ID for instantiate new contract
const INSTANTIATE_REPLY_ID: u64 = 1;

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    // Validate SDL template is valid JSON
    serde_json::from_str::<serde_json::Value>(&msg.sdl_template)
        .map_err(|e| ContractError::InvalidTemplate {
            reason: format!("Invalid JSON: {}", e),
        })?;

    // Validate that all variables in template have defaults
    validate_template_variables(&msg.sdl_template, &msg.variable_defaults)
        .map_err(|missing| ContractError::MissingVariableDefaults { variables: missing })?;

    // Store SDL template
    SDL_TEMPLATE.save(deps.storage, &msg.sdl_template)?;

    // Store configuration
    let admin = match msg.admin {
        Some(addr) => Some(deps.api.addr_validate(&addr)?),
        None => Some(info.sender.clone()),
    };

    let config = Config {
        label: msg.label.clone(),
        admin,
    };
    CONFIG.save(deps.storage, &config)?;

    // Store variable defaults
    for (key, value) in msg.variable_defaults.iter() {
        VARIABLE_DEFAULTS.save(deps.storage, key, value)?;
    }

    Ok(Response::new()
        .add_attribute("method", "instantiate")
        .add_attribute("label", msg.label.unwrap_or_else(|| "unnamed".to_string()))
        .add_attribute("admin", config.admin.map(|a| a.to_string()).unwrap_or_else(|| "none".to_string()))
        .add_attribute("variable_count", msg.variable_defaults.len().to_string()))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::UpdateTemplate { sdl_template } => {
            execute_update_template(deps, info, sdl_template)
        }
        ExecuteMsg::UpdateDefaults { variable_defaults } => {
            execute_update_defaults(deps, info, variable_defaults)
        }
        ExecuteMsg::UpdateSingleDefault { key, value } => {
            execute_update_single_default(deps, info, key, value)
        }
        ExecuteMsg::RemoveDefault { key } => execute_remove_default(deps, info, key),
        ExecuteMsg::UpdateLabel { label } => execute_update_label(deps, info, label),
        ExecuteMsg::TransferAdmin { new_admin } => execute_transfer_admin(deps, info, new_admin),
        ExecuteMsg::InstantiateNew {
            instantiate_msg,
            label,
            parent_results,
        } => execute_instantiate_new(deps, env, info, instantiate_msg, label, parent_results),
        ExecuteMsg::RecordDeploymentResult { key, value } => {
            execute_record_deployment_result(deps, info, key, value)
        }
        ExecuteMsg::RecordDeploymentResults { results } => {
            execute_record_deployment_results(deps, info, results)
        }
    }
}

/// Check if sender is admin
fn check_admin(deps: &DepsMut, sender: &cosmwasm_std::Addr) -> Result<(), ContractError> {
    let config = CONFIG.load(deps.storage)?;
    match config.admin {
        Some(admin) if admin == sender => Ok(()),
        Some(_) => Err(ContractError::Unauthorized {}),
        None => Err(ContractError::AdminNotSet {}),
    }
}

fn execute_update_template(
    deps: DepsMut,
    info: MessageInfo,
    sdl_template: String,
) -> Result<Response, ContractError> {
    check_admin(&deps, &info.sender)?;

    // Validate SDL template is valid JSON
    serde_json::from_str::<serde_json::Value>(&sdl_template).map_err(|e| {
        ContractError::InvalidTemplate {
            reason: format!("Invalid JSON: {}", e),
        }
    })?;

    // Load current defaults and validate new template against them
    let defaults: std::collections::HashMap<String, String> = VARIABLE_DEFAULTS
        .range(deps.storage, None, None, cosmwasm_std::Order::Ascending)
        .collect::<StdResult<std::collections::HashMap<String, String>>>()?;

    validate_template_variables(&sdl_template, &defaults)
        .map_err(|missing| ContractError::MissingVariableDefaults { variables: missing })?;

    SDL_TEMPLATE.save(deps.storage, &sdl_template)?;

    Ok(Response::new()
        .add_attribute("method", "update_template")
        .add_attribute("sender", info.sender))
}

fn execute_update_defaults(
    deps: DepsMut,
    info: MessageInfo,
    variable_defaults: std::collections::HashMap<String, String>,
) -> Result<Response, ContractError> {
    check_admin(&deps, &info.sender)?;

    // Clear existing defaults
    let keys: Vec<String> = VARIABLE_DEFAULTS
        .keys(deps.storage, None, None, cosmwasm_std::Order::Ascending)
        .collect::<StdResult<Vec<String>>>()?;

    for key in keys {
        VARIABLE_DEFAULTS.remove(deps.storage, &key);
    }

    // Store new defaults
    for (key, value) in variable_defaults.iter() {
        VARIABLE_DEFAULTS.save(deps.storage, key, value)?;
    }

    Ok(Response::new()
        .add_attribute("method", "update_defaults")
        .add_attribute("sender", info.sender)
        .add_attribute("count", variable_defaults.len().to_string()))
}

fn execute_update_single_default(
    deps: DepsMut,
    info: MessageInfo,
    key: String,
    value: String,
) -> Result<Response, ContractError> {
    check_admin(&deps, &info.sender)?;

    VARIABLE_DEFAULTS.save(deps.storage, &key, &value)?;

    Ok(Response::new()
        .add_attribute("method", "update_single_default")
        .add_attribute("key", key)
        .add_attribute("value", value))
}

fn execute_remove_default(
    deps: DepsMut,
    info: MessageInfo,
    key: String,
) -> Result<Response, ContractError> {
    check_admin(&deps, &info.sender)?;

    VARIABLE_DEFAULTS.remove(deps.storage, &key);

    Ok(Response::new()
        .add_attribute("method", "remove_default")
        .add_attribute("key", key))
}

fn execute_update_label(
    deps: DepsMut,
    info: MessageInfo,
    label: String,
) -> Result<Response, ContractError> {
    check_admin(&deps, &info.sender)?;

    CONFIG.update(deps.storage, |mut config| -> StdResult<_> {
        config.label = Some(label.clone());
        Ok(config)
    })?;

    Ok(Response::new()
        .add_attribute("method", "update_label")
        .add_attribute("label", label))
}

fn execute_transfer_admin(
    deps: DepsMut,
    info: MessageInfo,
    new_admin: String,
) -> Result<Response, ContractError> {
    check_admin(&deps, &info.sender)?;

    let new_admin_addr = deps.api.addr_validate(&new_admin)?;

    CONFIG.update(deps.storage, |mut config| -> StdResult<_> {
        config.admin = Some(new_admin_addr.clone());
        Ok(config)
    })?;

    Ok(Response::new()
        .add_attribute("method", "transfer_admin")
        .add_attribute("old_admin", info.sender)
        .add_attribute("new_admin", new_admin))
}

fn execute_instantiate_new(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    mut instantiate_msg: InstantiateMsg,
    label: String,
    parent_results: Option<std::collections::HashMap<String, String>>,
) -> Result<Response, ContractError> {
    // Merge parent deployment results into child's variable defaults
    // This allows chaining: NODE_A results feed into NODE_B's SDL variables
    if let Some(results) = parent_results {
        instantiate_msg.variable_defaults.extend(results);
    }

    // Get the current contract's code ID from contract info
    let contract_info = deps.querier.query_wasm_contract_info(&env.contract.address)?;
    let code_id = contract_info.code_id;

    // Create instantiate message for the new contract
    let instantiate_cosmos_msg = WasmMsg::Instantiate {
        admin: instantiate_msg.admin.clone(),
        code_id,
        msg: to_json_binary(&instantiate_msg)?,
        funds: info.funds.clone(),
        label: label.clone(),
    };

    // Create submessage with reply
    let sub_msg = SubMsg::reply_on_success(instantiate_cosmos_msg, INSTANTIATE_REPLY_ID);

    Ok(Response::new()
        .add_submessage(sub_msg)
        .add_attribute("method", "instantiate_new")
        .add_attribute("code_id", code_id.to_string())
        .add_attribute("child_label", label)
        .add_attribute("sender", info.sender))
}

/// Record a single deployment result (peer ID, endpoint, etc.)
fn execute_record_deployment_result(
    deps: DepsMut,
    info: MessageInfo,
    key: String,
    value: String,
) -> Result<Response, ContractError> {
    check_admin(&deps, &info.sender)?;

    DEPLOYMENT_RESULTS.save(deps.storage, &key, &value)?;

    Ok(Response::new()
        .add_attribute("method", "record_deployment_result")
        .add_attribute("key", &key)
        .add_attribute("value", &value))
}

/// Record multiple deployment results in one transaction
fn execute_record_deployment_results(
    deps: DepsMut,
    info: MessageInfo,
    results: std::collections::HashMap<String, String>,
) -> Result<Response, ContractError> {
    check_admin(&deps, &info.sender)?;

    for (key, value) in results.iter() {
        DEPLOYMENT_RESULTS.save(deps.storage, key, value)?;
    }

    Ok(Response::new()
        .add_attribute("method", "record_deployment_results")
        .add_attribute("count", results.len().to_string()))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn reply(deps: DepsMut, _env: Env, msg: Reply) -> Result<Response, ContractError> {
    match msg.id {
        INSTANTIATE_REPLY_ID => {
            // Parse the contract address from the instantiate reply
            let res = msg.result.into_result().map_err(StdError::generic_err)?;

            // Find the contract address and label from events
            let contract_address = res
                .events
                .iter()
                .find(|e| e.ty == "instantiate")
                .and_then(|e| e.attributes.iter().find(|a| a.key == "_contract_address"))
                .map(|a| a.value.clone())
                .ok_or_else(|| StdError::generic_err("contract address not found"))?;

            // Extract label from wasm events
            let label = res
                .events
                .iter()
                .find(|e| e.ty == "wasm")
                .and_then(|e| e.attributes.iter().find(|a| a.key == "child_label"))
                .map(|a| a.value.clone())
                .unwrap_or_else(|| "unknown".to_string());

            // Track the child contract by label
            let addr = deps.api.addr_validate(&contract_address)?;
            CHILD_CONTRACTS.save(deps.storage, &label, &addr)?;

            Ok(Response::new()
                .add_attribute("method", "reply_instantiate")
                .add_attribute("new_contract_address", contract_address)
                .add_attribute("child_label", label))
        }
        _ => Err(ContractError::Std(StdError::generic_err(
            "unknown reply id",
        ))),
    }
}
