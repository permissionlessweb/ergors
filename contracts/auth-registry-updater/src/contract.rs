use cosmwasm_std::{
    entry_point, to_json_binary, Binary, Deps, DepsMut, Env, MessageInfo, Order, Response,
    StdResult,
};
use cw2::set_contract_version;

use crate::error::ContractError;
use crate::msg::{
    AdminResponse, ExecuteMsg, InfoResponse, InstantiateMsg, IsAuthorizedResponse,
    ListAuthorizedResponse, QueryMsg,
};
use crate::state::{Config, AUTHORIZED, CONFIG};

const CONTRACT_NAME: &str = "crates.io:auth-registry-updater";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

const DEFAULT_LIMIT: u32 = 100;
const MAX_LIMIT: u32 = 1000;

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    // Validate and set coordinator as admin
    let admin = deps.api.addr_validate(&msg.coordinator)?;

    let config = Config { admin: admin.clone() };
    CONFIG.save(deps.storage, &config)?;

    // Authorize the coordinator
    AUTHORIZED.save(deps.storage, admin.as_str(), &true)?;

    // Authorize initial addresses if provided
    if let Some(initial) = msg.initial_authorized {
        for addr in initial {
            let validated = deps.api.addr_validate(&addr)?;
            AUTHORIZED.save(deps.storage, validated.as_str(), &true)?;
        }
    }

    Ok(Response::new()
        .add_attribute("method", "instantiate")
        .add_attribute("admin", admin.to_string()))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::AddAuthorized { address } => execute_add_authorized(deps, info, address),
        ExecuteMsg::RemoveAuthorized { address } => execute_remove_authorized(deps, info, address),
        ExecuteMsg::TransferAdmin { new_admin } => execute_transfer_admin(deps, info, new_admin),
    }
}

fn check_admin(deps: &DepsMut, sender: &cosmwasm_std::Addr) -> Result<(), ContractError> {
    let config = CONFIG.load(deps.storage)?;
    if config.admin != sender {
        return Err(ContractError::Unauthorized {});
    }
    Ok(())
}

fn execute_add_authorized(
    deps: DepsMut,
    info: MessageInfo,
    address: String,
) -> Result<Response, ContractError> {
    check_admin(&deps, &info.sender)?;

    let validated = deps.api.addr_validate(&address)?;

    // Check if already authorized
    if AUTHORIZED.may_load(deps.storage, validated.as_str())?.unwrap_or(false) {
        return Err(ContractError::AlreadyAuthorized {
            address: address.clone(),
        });
    }

    AUTHORIZED.save(deps.storage, validated.as_str(), &true)?;

    Ok(Response::new()
        .add_attribute("method", "add_authorized")
        .add_attribute("address", address))
}

fn execute_remove_authorized(
    deps: DepsMut,
    info: MessageInfo,
    address: String,
) -> Result<Response, ContractError> {
    check_admin(&deps, &info.sender)?;

    let validated = deps.api.addr_validate(&address)?;
    let config = CONFIG.load(deps.storage)?;

    // Cannot remove admin
    if validated == config.admin {
        return Err(ContractError::CannotRemoveAdmin {});
    }

    // Check if authorized
    if !AUTHORIZED.may_load(deps.storage, validated.as_str())?.unwrap_or(false) {
        return Err(ContractError::NotAuthorized {
            address: address.clone(),
        });
    }

    AUTHORIZED.remove(deps.storage, validated.as_str());

    Ok(Response::new()
        .add_attribute("method", "remove_authorized")
        .add_attribute("address", address))
}

fn execute_transfer_admin(
    deps: DepsMut,
    info: MessageInfo,
    new_admin: String,
) -> Result<Response, ContractError> {
    check_admin(&deps, &info.sender)?;

    let new_admin_addr = deps.api.addr_validate(&new_admin)?;

    CONFIG.update(deps.storage, |mut config| -> StdResult<_> {
        config.admin = new_admin_addr.clone();
        Ok(config)
    })?;

    // Ensure new admin is authorized
    AUTHORIZED.save(deps.storage, new_admin_addr.as_str(), &true)?;

    Ok(Response::new()
        .add_attribute("method", "transfer_admin")
        .add_attribute("old_admin", info.sender)
        .add_attribute("new_admin", new_admin))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::IsAuthorized { address } => to_json_binary(&query_is_authorized(deps, address)?),
        QueryMsg::GetAdmin {} => to_json_binary(&query_admin(deps)?),
        QueryMsg::ListAuthorized { start_after, limit } => {
            to_json_binary(&query_list_authorized(deps, start_after, limit)?)
        }
        QueryMsg::GetInfo {} => to_json_binary(&query_info(deps, env)?),
    }
}

fn query_is_authorized(deps: Deps, address: String) -> StdResult<IsAuthorizedResponse> {
    // Attempt to validate; if invalid, return not authorized
    let validated = match deps.api.addr_validate(&address) {
        Ok(addr) => addr,
        Err(_) => return Ok(IsAuthorizedResponse { authorized: false }),
    };

    let authorized = AUTHORIZED
        .may_load(deps.storage, validated.as_str())?
        .unwrap_or(false);

    Ok(IsAuthorizedResponse { authorized })
}

fn query_admin(deps: Deps) -> StdResult<AdminResponse> {
    let config = CONFIG.load(deps.storage)?;
    Ok(AdminResponse {
        admin: config.admin.to_string(),
    })
}

fn query_list_authorized(
    deps: Deps,
    start_after: Option<String>,
    limit: Option<u32>,
) -> StdResult<ListAuthorizedResponse> {
    let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;

    let start = start_after.as_deref().map(cosmwasm_std::Bound::exclusive);

    let addresses: Vec<String> = AUTHORIZED
        .range(deps.storage, start, None, Order::Ascending)
        .take(limit)
        .filter_map(|item| {
            item.ok().and_then(|(addr, is_authorized)| {
                if is_authorized {
                    Some(addr)
                } else {
                    None
                }
            })
        })
        .collect();

    Ok(ListAuthorizedResponse { addresses })
}

fn query_info(deps: Deps, env: Env) -> StdResult<InfoResponse> {
    let config = CONFIG.load(deps.storage)?;

    let authorized_count = AUTHORIZED
        .range(deps.storage, None, None, Order::Ascending)
        .filter(|r| r.as_ref().map(|(_, v)| *v).unwrap_or(false))
        .count() as u64;

    Ok(InfoResponse {
        admin: config.admin.to_string(),
        authorized_count,
        contract_address: env.contract.address.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use cosmwasm_std::testing::{mock_dependencies, mock_env, mock_info};

    #[test]
    fn proper_initialization() {
        let mut deps = mock_dependencies();
        let msg = InstantiateMsg {
            coordinator: "coordinator".to_string(),
            initial_authorized: Some(vec!["user1".to_string()]),
        };
        let info = mock_info("creator", &[]);
        let res = instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();
        assert_eq!(1, res.attributes.len());

        // Query if coordinator is authorized
        let res = query_is_authorized(deps.as_ref(), "coordinator".to_string()).unwrap();
        assert!(res.authorized);

        // Query if user1 is authorized
        let res = query_is_authorized(deps.as_ref(), "user1".to_string()).unwrap();
        assert!(res.authorized);

        // Query if unknown user is not authorized
        let res = query_is_authorized(deps.as_ref(), "unknown".to_string()).unwrap();
        assert!(!res.authorized);
    }

    #[test]
    fn add_and_remove_authorized() {
        let mut deps = mock_dependencies();
        let msg = InstantiateMsg {
            coordinator: "coordinator".to_string(),
            initial_authorized: None,
        };
        let info = mock_info("creator", &[]);
        instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

        // Add a new authorized address
        let info = mock_info("coordinator", &[]);
        execute_add_authorized(deps.as_mut(), info, "newuser".to_string()).unwrap();

        // Verify it's authorized
        let res = query_is_authorized(deps.as_ref(), "newuser".to_string()).unwrap();
        assert!(res.authorized);

        // Remove the address
        let info = mock_info("coordinator", &[]);
        execute_remove_authorized(deps.as_mut(), info, "newuser".to_string()).unwrap();

        // Verify it's no longer authorized
        let res = query_is_authorized(deps.as_ref(), "newuser".to_string()).unwrap();
        assert!(!res.authorized);
    }

    #[test]
    fn unauthorized_cannot_add() {
        let mut deps = mock_dependencies();
        let msg = InstantiateMsg {
            coordinator: "coordinator".to_string(),
            initial_authorized: None,
        };
        let info = mock_info("creator", &[]);
        instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

        // Try to add from non-admin
        let info = mock_info("random", &[]);
        let err = execute_add_authorized(deps.as_mut(), info, "newuser".to_string()).unwrap_err();
        assert!(matches!(err, ContractError::Unauthorized {}));
    }

    #[test]
    fn cannot_remove_admin() {
        let mut deps = mock_dependencies();
        let msg = InstantiateMsg {
            coordinator: "coordinator".to_string(),
            initial_authorized: None,
        };
        let info = mock_info("creator", &[]);
        instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

        // Try to remove admin
        let info = mock_info("coordinator", &[]);
        let err =
            execute_remove_authorized(deps.as_mut(), info, "coordinator".to_string()).unwrap_err();
        assert!(matches!(err, ContractError::CannotRemoveAdmin {}));
    }
}
