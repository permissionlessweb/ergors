pub mod contract;
pub mod error;
pub mod msg;
pub mod query;
pub mod state;
pub mod validation;

#[cfg(not(feature = "library"))]
use cosmwasm_std::{entry_point, to_json_binary, Binary, Deps, Env, StdResult};

use crate::msg::QueryMsg;

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::GetTemplate {} => to_json_binary(&query::query_template(deps).map_err(|e| cosmwasm_std::StdError::generic_err(e.to_string()))?),
        QueryMsg::GetDefaults {} => to_json_binary(&query::query_defaults(deps).map_err(|e| cosmwasm_std::StdError::generic_err(e.to_string()))?),
        QueryMsg::GetDefault { key } => to_json_binary(&query::query_single_default(deps, key).map_err(|e| cosmwasm_std::StdError::generic_err(e.to_string()))?),
        QueryMsg::GetInfo {} => to_json_binary(&query::query_info(deps, env).map_err(|e| cosmwasm_std::StdError::generic_err(e.to_string()))?),
        QueryMsg::ListKeys {} => to_json_binary(&query::query_keys(deps).map_err(|e| cosmwasm_std::StdError::generic_err(e.to_string()))?),
        QueryMsg::RenderSdl { variables } => {
            to_json_binary(&query::query_render_sdl(deps, variables).map_err(|e| cosmwasm_std::StdError::generic_err(e.to_string()))?)
        }
        QueryMsg::GetRenderedJson { variables } => {
            to_json_binary(&query::query_rendered_json(deps, variables).map_err(|e| cosmwasm_std::StdError::generic_err(e.to_string()))?)
        }
        QueryMsg::GetDeploymentResult { key } => {
            to_json_binary(&query::query_deployment_result(deps, key).map_err(|e| cosmwasm_std::StdError::generic_err(e.to_string()))?)
        }
        QueryMsg::ListDeploymentResults {} => {
            to_json_binary(&query::query_deployment_results(deps).map_err(|e| cosmwasm_std::StdError::generic_err(e.to_string()))?)
        }
        QueryMsg::ListChildContracts {} => {
            to_json_binary(&query::query_child_contracts(deps).map_err(|e| cosmwasm_std::StdError::generic_err(e.to_string()))?)
        }
    }
}

#[cfg(test)]
mod tests;
