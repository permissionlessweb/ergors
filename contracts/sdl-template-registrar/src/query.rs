use cosmwasm_std::{Deps, Env, StdResult, Order};
use std::collections::HashMap;

use crate::error::ContractError;
use crate::msg::{
    DefaultsResponse, InfoResponse, KeysResponse, RenderedSdlResponse, RenderedJsonResponse,
    SingleDefaultResponse, TemplateResponse,
};
use crate::state::{CONFIG, SDL_TEMPLATE, VARIABLE_DEFAULTS};

pub fn query_template(deps: Deps) -> Result<TemplateResponse, ContractError> {
    let sdl_template = SDL_TEMPLATE.load(deps.storage)?;
    let template_json = serde_json::from_str(&sdl_template)
        .map_err(|e| ContractError::InvalidJson(e.to_string()))?;

    Ok(TemplateResponse {
        sdl_template,
        template_json,
    })
}

pub fn query_defaults(deps: Deps) -> Result<DefaultsResponse, ContractError> {
    let defaults: HashMap<String, String> = VARIABLE_DEFAULTS
        .range(deps.storage, None, None, Order::Ascending)
        .collect::<StdResult<HashMap<String, String>>>()?;

    Ok(DefaultsResponse { defaults })
}

pub fn query_single_default(deps: Deps, key: String) -> Result<SingleDefaultResponse, ContractError> {
    let value = VARIABLE_DEFAULTS.may_load(deps.storage, &key)?;

    Ok(SingleDefaultResponse { key, value })
}

pub fn query_info(deps: Deps, env: Env) -> Result<InfoResponse, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    let contract_info = deps.querier.query_wasm_contract_info(&env.contract.address)?;

    Ok(InfoResponse {
        label: config.label,
        admin: config.admin.map(|a| a.to_string()),
        code_id: contract_info.code_id,
    })
}

pub fn query_keys(deps: Deps) -> Result<KeysResponse, ContractError> {
    let keys: Vec<String> = VARIABLE_DEFAULTS
        .keys(deps.storage, None, None, Order::Ascending)
        .collect::<StdResult<Vec<String>>>()?;

    Ok(KeysResponse { keys })
}

pub fn query_render_sdl(
    deps: Deps,
    variables: Option<HashMap<String, String>>,
) -> Result<RenderedSdlResponse, ContractError> {
    let sdl_template = SDL_TEMPLATE.load(deps.storage)?;

    // Load all defaults
    let defaults: HashMap<String, String> = VARIABLE_DEFAULTS
        .range(deps.storage, None, None, Order::Ascending)
        .collect::<StdResult<HashMap<String, String>>>()?;

    // Merge provided variables with defaults (provided variables take precedence)
    let mut final_variables = defaults.clone();
    if let Some(vars) = variables {
        final_variables.extend(vars);
    }

    // Render the SDL template by replacing variables
    let mut rendered_sdl = sdl_template.clone();
    let mut used_variables = HashMap::new();

    for (key, value) in final_variables.iter() {
        let placeholder = format!("${{{}}}", key);
        if rendered_sdl.contains(&placeholder) {
            rendered_sdl = rendered_sdl.replace(&placeholder, value);
            used_variables.insert(key.clone(), value.clone());
        }
    }

    Ok(RenderedSdlResponse {
        rendered_sdl,
        used_variables,
    })
}

pub fn query_rendered_json(
    deps: Deps,
    variables: Option<HashMap<String, String>>,
) -> Result<RenderedJsonResponse, ContractError> {
    let sdl_template = SDL_TEMPLATE.load(deps.storage)?;

    // Load all defaults
    let defaults: HashMap<String, String> = VARIABLE_DEFAULTS
        .range(deps.storage, None, None, Order::Ascending)
        .collect::<StdResult<HashMap<String, String>>>()?;

    // Merge provided variables with defaults (provided variables take precedence)
    let mut final_variables = defaults.clone();
    if let Some(vars) = variables {
        final_variables.extend(vars);
    }

    // Render the SDL template by replacing variables
    let mut rendered_sdl = sdl_template.clone();
    let mut used_variables = HashMap::new();

    for (key, value) in final_variables.iter() {
        let placeholder = format!("${{{}}}", key);
        if rendered_sdl.contains(&placeholder) {
            rendered_sdl = rendered_sdl.replace(&placeholder, value);
            used_variables.insert(key.clone(), value.clone());
        }
    }

    // Parse the rendered SDL as JSON
    let sdl_json = serde_json::from_str(&rendered_sdl)
        .map_err(|e| ContractError::InvalidJson(e.to_string()))?;

    Ok(RenderedJsonResponse {
        sdl_json,
        used_variables,
    })
}
