use cosmwasm_std::{
    entry_point, to_json_binary, Binary, Deps, DepsMut, Env, MessageInfo, Order, Response,
    StdResult,
};
use cw2::set_contract_version;

use crate::error::ContractError;
use crate::msg::{
    AddressCheckResult, AdminResponse, BatchCheckResponse, ConfigResponse, ExecuteMsg,
    InstantiateMsg, IsAllowedResponse, ListWhitelistResponse, QueryMsg,
};
use crate::state::{Config, CONFIG, WHITELIST};

const CONTRACT_NAME: &str = "crates.io:cw-auth";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

const DEFAULT_LIMIT: u32 = 100;
const MAX_LIMIT: u32 = 1000;
const BATCH_LIMIT: u32 = 100;

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    let admin = deps.api.addr_validate(&msg.admin)?;

    let config = Config {
        admin: admin.clone(),
        description: msg.description.clone(),
        default_allow: msg.default_allow.unwrap_or(false),
    };
    CONFIG.save(deps.storage, &config)?;

    // Add initial whitelist
    if let Some(addresses) = msg.initial_whitelist {
        for addr in addresses {
            let validated = deps.api.addr_validate(&addr)?;
            WHITELIST.save(deps.storage, validated.as_str(), &true)?;
        }
    }

    Ok(Response::new()
        .add_attribute("method", "instantiate")
        .add_attribute("admin", admin.to_string())
        .add_attribute("default_allow", config.default_allow.to_string()))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::AddAddress { address } => execute_add_address(deps, info, address),
        ExecuteMsg::RemoveAddress { address } => execute_remove_address(deps, info, address),
        ExecuteMsg::BatchAdd { addresses } => execute_batch_add(deps, info, addresses),
        ExecuteMsg::BatchRemove { addresses } => execute_batch_remove(deps, info, addresses),
        ExecuteMsg::UpdateDescription { description } => {
            execute_update_description(deps, info, description)
        }
        ExecuteMsg::SetDefaultPolicy { allow } => execute_set_default_policy(deps, info, allow),
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

fn execute_add_address(
    deps: DepsMut,
    info: MessageInfo,
    address: String,
) -> Result<Response, ContractError> {
    check_admin(&deps, &info.sender)?;

    let validated = deps.api.addr_validate(&address)?;

    if WHITELIST.may_load(deps.storage, validated.as_str())?.unwrap_or(false) {
        return Err(ContractError::AlreadyWhitelisted {
            address: address.clone(),
        });
    }

    WHITELIST.save(deps.storage, validated.as_str(), &true)?;

    Ok(Response::new()
        .add_attribute("method", "add_address")
        .add_attribute("address", address))
}

fn execute_remove_address(
    deps: DepsMut,
    info: MessageInfo,
    address: String,
) -> Result<Response, ContractError> {
    check_admin(&deps, &info.sender)?;

    let validated = deps.api.addr_validate(&address)?;

    if !WHITELIST.may_load(deps.storage, validated.as_str())?.unwrap_or(false) {
        return Err(ContractError::NotWhitelisted {
            address: address.clone(),
        });
    }

    WHITELIST.remove(deps.storage, validated.as_str());

    Ok(Response::new()
        .add_attribute("method", "remove_address")
        .add_attribute("address", address))
}

fn execute_batch_add(
    deps: DepsMut,
    info: MessageInfo,
    addresses: Vec<String>,
) -> Result<Response, ContractError> {
    check_admin(&deps, &info.sender)?;

    if addresses.len() > BATCH_LIMIT as usize {
        return Err(ContractError::BatchLimitExceeded {
            max: BATCH_LIMIT,
            got: addresses.len() as u32,
        });
    }

    let mut added = 0u32;
    for addr in &addresses {
        let validated = deps.api.addr_validate(addr)?;
        if !WHITELIST.may_load(deps.storage, validated.as_str())?.unwrap_or(false) {
            WHITELIST.save(deps.storage, validated.as_str(), &true)?;
            added += 1;
        }
    }

    Ok(Response::new()
        .add_attribute("method", "batch_add")
        .add_attribute("added", added.to_string())
        .add_attribute("total_requested", addresses.len().to_string()))
}

fn execute_batch_remove(
    deps: DepsMut,
    info: MessageInfo,
    addresses: Vec<String>,
) -> Result<Response, ContractError> {
    check_admin(&deps, &info.sender)?;

    if addresses.len() > BATCH_LIMIT as usize {
        return Err(ContractError::BatchLimitExceeded {
            max: BATCH_LIMIT,
            got: addresses.len() as u32,
        });
    }

    let mut removed = 0u32;
    for addr in &addresses {
        let validated = deps.api.addr_validate(addr)?;
        if WHITELIST.may_load(deps.storage, validated.as_str())?.unwrap_or(false) {
            WHITELIST.remove(deps.storage, validated.as_str());
            removed += 1;
        }
    }

    Ok(Response::new()
        .add_attribute("method", "batch_remove")
        .add_attribute("removed", removed.to_string())
        .add_attribute("total_requested", addresses.len().to_string()))
}

fn execute_update_description(
    deps: DepsMut,
    info: MessageInfo,
    description: String,
) -> Result<Response, ContractError> {
    check_admin(&deps, &info.sender)?;

    CONFIG.update(deps.storage, |mut config| -> StdResult<_> {
        config.description = Some(description.clone());
        Ok(config)
    })?;

    Ok(Response::new()
        .add_attribute("method", "update_description")
        .add_attribute("description", description))
}

fn execute_set_default_policy(
    deps: DepsMut,
    info: MessageInfo,
    allow: bool,
) -> Result<Response, ContractError> {
    check_admin(&deps, &info.sender)?;

    CONFIG.update(deps.storage, |mut config| -> StdResult<_> {
        config.default_allow = allow;
        Ok(config)
    })?;

    Ok(Response::new()
        .add_attribute("method", "set_default_policy")
        .add_attribute("default_allow", allow.to_string()))
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

    Ok(Response::new()
        .add_attribute("method", "transfer_admin")
        .add_attribute("old_admin", info.sender)
        .add_attribute("new_admin", new_admin))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::IsAllowed { address } => to_json_binary(&query_is_allowed(deps, address)?),
        QueryMsg::GetAdmin {} => to_json_binary(&query_admin(deps)?),
        QueryMsg::ListWhitelist { start_after, limit } => {
            to_json_binary(&query_list_whitelist(deps, start_after, limit)?)
        }
        QueryMsg::GetConfig {} => to_json_binary(&query_config(deps)?),
        QueryMsg::BatchCheck { addresses } => to_json_binary(&query_batch_check(deps, addresses)?),
    }
}

fn query_is_allowed(deps: Deps, address: String) -> StdResult<IsAllowedResponse> {
    let config = CONFIG.load(deps.storage)?;

    // Attempt to validate; if invalid, use default policy
    let validated = match deps.api.addr_validate(&address) {
        Ok(addr) => addr,
        Err(_) => return Ok(IsAllowedResponse { allowed: config.default_allow }),
    };

    let in_whitelist = WHITELIST
        .may_load(deps.storage, validated.as_str())?
        .unwrap_or(false);

    // If default_allow is true, whitelist acts as blocklist
    // If default_allow is false, whitelist acts as allowlist
    let allowed = if config.default_allow {
        !in_whitelist // In blocklist mode, being in the list means blocked
    } else {
        in_whitelist // In allowlist mode, being in the list means allowed
    };

    Ok(IsAllowedResponse { allowed })
}

fn query_admin(deps: Deps) -> StdResult<AdminResponse> {
    let config = CONFIG.load(deps.storage)?;
    Ok(AdminResponse {
        admin: config.admin.to_string(),
    })
}

fn query_list_whitelist(
    deps: Deps,
    start_after: Option<String>,
    limit: Option<u32>,
) -> StdResult<ListWhitelistResponse> {
    let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;

    let start = start_after.as_deref().map(cosmwasm_std::Bound::exclusive);

    let addresses: Vec<String> = WHITELIST
        .range(deps.storage, start, None, Order::Ascending)
        .take(limit)
        .filter_map(|item| {
            item.ok().and_then(|(addr, is_whitelisted)| {
                if is_whitelisted {
                    Some(addr)
                } else {
                    None
                }
            })
        })
        .collect();

    let total = WHITELIST
        .range(deps.storage, None, None, Order::Ascending)
        .filter(|r| r.as_ref().map(|(_, v)| *v).unwrap_or(false))
        .count() as u64;

    Ok(ListWhitelistResponse { addresses, total })
}

fn query_config(deps: Deps) -> StdResult<ConfigResponse> {
    let config = CONFIG.load(deps.storage)?;

    let whitelist_count = WHITELIST
        .range(deps.storage, None, None, Order::Ascending)
        .filter(|r| r.as_ref().map(|(_, v)| *v).unwrap_or(false))
        .count() as u64;

    Ok(ConfigResponse {
        admin: config.admin.to_string(),
        description: config.description,
        default_allow: config.default_allow,
        whitelist_count,
    })
}

fn query_batch_check(deps: Deps, addresses: Vec<String>) -> StdResult<BatchCheckResponse> {
    let config = CONFIG.load(deps.storage)?;

    let results: Vec<AddressCheckResult> = addresses
        .into_iter()
        .map(|address| {
            let allowed = match deps.api.addr_validate(&address) {
                Ok(validated) => {
                    let in_whitelist = WHITELIST
                        .may_load(deps.storage, validated.as_str())
                        .unwrap_or(None)
                        .unwrap_or(false);
                    if config.default_allow {
                        !in_whitelist
                    } else {
                        in_whitelist
                    }
                }
                Err(_) => config.default_allow,
            };
            AddressCheckResult { address, allowed }
        })
        .collect();

    Ok(BatchCheckResponse { results })
}

#[cfg(test)]
mod tests {
    use super::*;
    use cosmwasm_std::testing::{mock_dependencies, mock_env, mock_info};

    #[test]
    fn proper_initialization() {
        let mut deps = mock_dependencies();
        let msg = InstantiateMsg {
            admin: "admin".to_string(),
            description: Some("Test authenticator".to_string()),
            initial_whitelist: Some(vec!["user1".to_string()]),
            default_allow: None, // Default to deny mode
        };
        let info = mock_info("creator", &[]);
        let res = instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();
        assert_eq!(3, res.attributes.len());

        // Whitelisted user should be allowed
        let res = query_is_allowed(deps.as_ref(), "user1".to_string()).unwrap();
        assert!(res.allowed);

        // Non-whitelisted user should be denied (default_allow = false)
        let res = query_is_allowed(deps.as_ref(), "unknown".to_string()).unwrap();
        assert!(!res.allowed);
    }

    #[test]
    fn blocklist_mode() {
        let mut deps = mock_dependencies();
        let msg = InstantiateMsg {
            admin: "admin".to_string(),
            description: None,
            initial_whitelist: Some(vec!["blocked".to_string()]),
            default_allow: Some(true), // Blocklist mode
        };
        let info = mock_info("creator", &[]);
        instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

        // Non-listed user should be allowed (default_allow = true)
        let res = query_is_allowed(deps.as_ref(), "random".to_string()).unwrap();
        assert!(res.allowed);

        // Listed user should be blocked
        let res = query_is_allowed(deps.as_ref(), "blocked".to_string()).unwrap();
        assert!(!res.allowed);
    }

    #[test]
    fn add_and_remove_whitelist() {
        let mut deps = mock_dependencies();
        let msg = InstantiateMsg {
            admin: "admin".to_string(),
            description: None,
            initial_whitelist: None,
            default_allow: None,
        };
        let info = mock_info("creator", &[]);
        instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

        // Add to whitelist
        let info = mock_info("admin", &[]);
        execute_add_address(deps.as_mut(), info, "newuser".to_string()).unwrap();

        // Should be allowed now
        let res = query_is_allowed(deps.as_ref(), "newuser".to_string()).unwrap();
        assert!(res.allowed);

        // Remove from whitelist
        let info = mock_info("admin", &[]);
        execute_remove_address(deps.as_mut(), info, "newuser".to_string()).unwrap();

        // Should be denied now
        let res = query_is_allowed(deps.as_ref(), "newuser".to_string()).unwrap();
        assert!(!res.allowed);
    }

    #[test]
    fn batch_operations() {
        let mut deps = mock_dependencies();
        let msg = InstantiateMsg {
            admin: "admin".to_string(),
            description: None,
            initial_whitelist: None,
            default_allow: None,
        };
        let info = mock_info("creator", &[]);
        instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

        // Batch add
        let info = mock_info("admin", &[]);
        execute_batch_add(
            deps.as_mut(),
            info,
            vec!["user1".to_string(), "user2".to_string()],
        )
        .unwrap();

        // Both should be allowed
        let res = query_batch_check(
            deps.as_ref(),
            vec!["user1".to_string(), "user2".to_string(), "user3".to_string()],
        )
        .unwrap();
        assert!(res.results[0].allowed);
        assert!(res.results[1].allowed);
        assert!(!res.results[2].allowed);
    }
}
