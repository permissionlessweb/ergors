use cosmwasm_std::{
    entry_point, to_json_binary, Addr, Binary, Deps, DepsMut, Env, MessageInfo, Order, Response,
    StdResult,
};
use cw_storage_plus::Bound;

use crate::error::ContractError;
use crate::msg::{
    AdminResponse, ExecuteMsg, InfoResponse, InstantiateMsg, IsAuthorizedResponse,
    ListAuthorizedResponse, QueryMsg,
};
use crate::state::{Config, AUTHORIZED, CONFIG};

const CONTRACT_NAME: &str = "crates.io:cw-middleware-auth";
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

    let config = Config {
        admin: admin.clone(),
    };
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
    if AUTHORIZED
        .may_load(deps.storage, validated.as_str())?
        .unwrap_or(false)
    {
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
    if !AUTHORIZED
        .may_load(deps.storage, validated.as_str())?
        .unwrap_or(false)
    {
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

    let start = start_after.as_deref().map(Bound::exclusive);

    let addresses: Vec<String> = AUTHORIZED
        .range(deps.storage, start, None, Order::Ascending)
        .take(limit)
        .filter_map(|item| {
            item.ok().and_then(
                |(addr, is_authorized)| {
                    if is_authorized {
                        Some(addr)
                    } else {
                        None
                    }
                },
            )
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

    // Test addresses (valid bech32 format)
    const COORDINATOR: &str = "cosmos1coordinator";
    const USER1: &str = "cosmos1user1";
    const USER2: &str = "cosmos1user2";
    const USER3: &str = "cosmos1user3";
    const UNAUTHORIZED: &str = "cosmos1unauthorized";
    const CREATOR: &str = "cosmos1creator";

    #[test]
    fn proper_initialization() {
        let mut deps = mock_dependencies();
        let msg = InstantiateMsg {
            coordinator: COORDINATOR.to_string(),
            initial_authorized: Some(vec![USER1.to_string()]),
        };
        let info = mock_info(CREATOR, &[]);
        let res = instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();
        assert_eq!(1, res.attributes.len());

        // Query if coordinator is authorized
        let res = query_is_authorized(deps.as_ref(), COORDINATOR.to_string()).unwrap();
        assert!(res.authorized);

        // Query if user1 is authorized
        let res = query_is_authorized(deps.as_ref(), USER1.to_string()).unwrap();
        assert!(res.authorized);

        // Query if unknown user is not authorized
        let res = query_is_authorized(deps.as_ref(), UNAUTHORIZED.to_string()).unwrap();
        assert!(!res.authorized);
    }

    #[test]
    fn add_and_remove_authorized() {
        let mut deps = mock_dependencies();
        let msg = InstantiateMsg {
            coordinator: COORDINATOR.to_string(),
            initial_authorized: None,
        };
        let info = mock_info(CREATOR, &[]);
        instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

        // Add a new authorized address
        let info = mock_info(COORDINATOR, &[]);
        execute_add_authorized(deps.as_mut(), info, USER1.to_string()).unwrap();

        // Verify it's authorized
        let res = query_is_authorized(deps.as_ref(), USER1.to_string()).unwrap();
        assert!(res.authorized);

        // Remove the address
        let info = mock_info(COORDINATOR, &[]);
        execute_remove_authorized(deps.as_mut(), info, USER1.to_string()).unwrap();

        // Verify it's no longer authorized
        let res = query_is_authorized(deps.as_ref(), USER1.to_string()).unwrap();
        assert!(!res.authorized);
    }

    #[test]
    fn unauthorized_cannot_add() {
        let mut deps = mock_dependencies();
        let msg = InstantiateMsg {
            coordinator: COORDINATOR.to_string(),
            initial_authorized: None,
        };
        let info = mock_info(CREATOR, &[]);
        instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

        // Try to add from non-admin
        let info = mock_info(UNAUTHORIZED, &[]);
        let err = execute_add_authorized(deps.as_mut(), info, USER1.to_string()).unwrap_err();
        assert!(matches!(err, ContractError::Unauthorized {}));
    }

    #[test]
    fn cannot_remove_admin() {
        let mut deps = mock_dependencies();
        let msg = InstantiateMsg {
            coordinator: COORDINATOR.to_string(),
            initial_authorized: None,
        };
        let info = mock_info(CREATOR, &[]);
        instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

        // Try to remove admin
        let info = mock_info(COORDINATOR, &[]);
        let err =
            execute_remove_authorized(deps.as_mut(), info, COORDINATOR.to_string()).unwrap_err();
        assert!(matches!(err, ContractError::CannotRemoveAdmin {}));
    }

    #[test]
    fn instantiate_with_empty_initial_authorized() {
        let mut deps = mock_dependencies();
        let msg = InstantiateMsg {
            coordinator: COORDINATOR.to_string(),
            initial_authorized: Some(vec![]),
        };
        let info = mock_info(CREATOR, &[]);
        let res = instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();
        assert_eq!(1, res.attributes.len());

        // Only coordinator should be authorized
        let res = query_is_authorized(deps.as_ref(), COORDINATOR.to_string()).unwrap();
        assert!(res.authorized);

        let res = query_is_authorized(deps.as_ref(), UNAUTHORIZED.to_string()).unwrap();
        assert!(!res.authorized);
    }

    #[test]
    fn instantiate_invalid_coordinator_address() {
        let mut deps = mock_dependencies();
        let msg = InstantiateMsg {
            coordinator: "invalid_address!".to_string(),
            initial_authorized: None,
        };
        let info = mock_info(CREATOR, &[]);
        let err = instantiate(deps.as_mut(), mock_env(), info, msg).unwrap_err();
        assert!(matches!(err, ContractError::Std(_)));
    }

    #[test]
    fn instantiate_invalid_initial_authorized_address() {
        let mut deps = mock_dependencies();
        let msg = InstantiateMsg {
            coordinator: COORDINATOR.to_string(),
            initial_authorized: Some(vec!["invalid!".to_string()]),
        };
        let info = mock_info(CREATOR, &[]);
        let err = instantiate(deps.as_mut(), mock_env(), info, msg).unwrap_err();
        assert!(matches!(err, ContractError::Std(_)));
    }

    #[test]
    fn execute_add_authorized_invalid_address() {
        let mut deps = mock_dependencies();
        let msg = InstantiateMsg {
            coordinator: COORDINATOR.to_string(),
            initial_authorized: None,
        };
        let info = mock_info(CREATOR, &[]);
        instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

        let info = mock_info(COORDINATOR, &[]);
        let err = execute_add_authorized(deps.as_mut(), info, "invalid!".to_string()).unwrap_err();
        assert!(matches!(err, ContractError::Std(_)));
    }

    #[test]
    fn execute_remove_authorized_invalid_address() {
        let mut deps = mock_dependencies();
        let msg = InstantiateMsg {
            coordinator: COORDINATOR.to_string(),
            initial_authorized: None,
        };
        let info = mock_info(CREATOR, &[]);
        instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

        let info = mock_info(COORDINATOR, &[]);
        let err =
            execute_remove_authorized(deps.as_mut(), info, "invalid!".to_string()).unwrap_err();
        assert!(matches!(err, ContractError::Std(_)));
    }

    #[test]
    fn execute_transfer_admin_invalid_address() {
        let mut deps = mock_dependencies();
        let msg = InstantiateMsg {
            coordinator: COORDINATOR.to_string(),
            initial_authorized: None,
        };
        let info = mock_info(CREATOR, &[]);
        instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

        let info = mock_info(COORDINATOR, &[]);
        let err = execute_transfer_admin(deps.as_mut(), info, "invalid!".to_string()).unwrap_err();
        assert!(matches!(err, ContractError::Std(_)));
    }

    #[test]
    fn execute_remove_authorized_not_found() {
        let mut deps = mock_dependencies();
        let msg = InstantiateMsg {
            coordinator: COORDINATOR.to_string(),
            initial_authorized: None,
        };
        let info = mock_info(CREATOR, &[]);
        instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

        let info = mock_info(COORDINATOR, &[]);
        let err =
            execute_remove_authorized(deps.as_mut(), info, UNAUTHORIZED.to_string()).unwrap_err();
        assert!(matches!(err, ContractError::NotAuthorized { address } if address == UNAUTHORIZED));
    }

    #[test]
    fn execute_remove_authorized_invalid_address() {
        let mut deps = mock_dependencies();
        let msg = InstantiateMsg {
            coordinator: "coordinator".to_string(),
            initial_authorized: None,
        };
        let info = mock_info("creator", &[]);
        instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

        let info = mock_info("coordinator", &[]);
        let err =
            execute_remove_authorized(deps.as_mut(), info, "invalid!".to_string()).unwrap_err();
        assert!(matches!(err, ContractError::Std(_)));
    }

    #[test]
    fn execute_transfer_admin_invalid_address() {
        let mut deps = mock_dependencies();
        let msg = InstantiateMsg {
            coordinator: "coordinator".to_string(),
            initial_authorized: None,
        };
        let info = mock_info("creator", &[]);
        instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

        let info = mock_info("coordinator", &[]);
        let err = execute_transfer_admin(deps.as_mut(), info, "invalid!".to_string()).unwrap_err();
        assert!(matches!(err, ContractError::Std(_)));
    }

    #[test]
    fn execute_remove_authorized_not_found() {
        let mut deps = mock_dependencies();
        let msg = InstantiateMsg {
            coordinator: "coordinator".to_string(),
            initial_authorized: None,
        };
        let info = mock_info("creator", &[]);
        instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

        let info = mock_info("coordinator", &[]);
        let err = execute_remove_authorized(deps.as_mut(), info, "not_authorized".to_string())
            .unwrap_err();
        assert!(
            matches!(err, ContractError::NotAuthorized { address } if address == "not_authorized")
        );
    }

    #[test]
    fn execute_transfer_admin_to_existing_admin() {
        let mut deps = mock_dependencies();
        let msg = InstantiateMsg {
            coordinator: COORDINATOR.to_string(),
            initial_authorized: None,
        };
        let info = mock_info(CREATOR, &[]);
        instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

        // Transfer admin to the same address (should still work)
        let info = mock_info(COORDINATOR, &[]);
        let res = execute_transfer_admin(deps.as_mut(), info, COORDINATOR.to_string()).unwrap();

        // Verify admin is still the same
        let config = CONFIG.load(deps.as_ref().storage).unwrap();
        assert_eq!(config.admin.to_string(), COORDINATOR);
    }

    #[test]
    fn query_is_authorized_invalid_address() {
        let mut deps = mock_dependencies();
        let msg = InstantiateMsg {
            coordinator: COORDINATOR.to_string(),
            initial_authorized: None,
        };
        let info = mock_info(CREATOR, &[]);
        instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

        // Invalid address should return not authorized
        let res = query_is_authorized(deps.as_ref(), "invalid!".to_string()).unwrap();
        assert!(!res.authorized);
    }

    #[test]
    fn query_list_authorized_empty() {
        let mut deps = mock_dependencies();
        let msg = InstantiateMsg {
            coordinator: COORDINATOR.to_string(),
            initial_authorized: None,
        };
        let info = mock_info(CREATOR, &[]);
        instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

        let res = query_list_authorized(deps.as_ref(), None, None).unwrap();
        assert_eq!(res.addresses.len(), 1);
        assert_eq!(res.addresses[0], COORDINATOR);
    }

    #[test]
    fn query_list_authorized_with_limit() {
        let mut deps = mock_dependencies();
        let initial_authorized = vec![USER1.to_string(), USER2.to_string(), USER3.to_string()];
        let msg = InstantiateMsg {
            coordinator: COORDINATOR.to_string(),
            initial_authorized: Some(initial_authorized),
        };
        let info = mock_info(CREATOR, &[]);
        instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

        // Query with limit of 2
        let res = query_list_authorized(deps.as_ref(), None, Some(2)).unwrap();
        assert_eq!(res.addresses.len(), 2);

        // Query remaining with start_after
        let start_after = res.addresses.last().unwrap().clone();
        let res2 = query_list_authorized(deps.as_ref(), Some(start_after), Some(2)).unwrap();
        assert_eq!(res2.addresses.len(), 2);
    }

    #[test]
    fn query_list_authorized_max_limit() {
        let mut deps = mock_dependencies();
        let msg = InstantiateMsg {
            coordinator: COORDINATOR.to_string(),
            initial_authorized: None,
        };
        let info = mock_info(CREATOR, &[]);
        instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

        // Query with limit exceeding MAX_LIMIT should be capped
        let res = query_list_authorized(deps.as_ref(), None, Some(2000)).unwrap();
        assert!(res.addresses.len() <= MAX_LIMIT as usize);
    }

    #[test]
    fn query_info_comprehensive() {
        let mut deps = mock_dependencies();
        let initial_authorized = vec![USER1.to_string(), USER2.to_string()];
        let msg = InstantiateMsg {
            coordinator: COORDINATOR.to_string(),
            initial_authorized: Some(initial_authorized),
        };
        let info = mock_info(CREATOR, &[]);
        instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

        let env = mock_env();
        let res = query_info(deps.as_ref(), env.clone()).unwrap();

        assert_eq!(res.admin, COORDINATOR);
        assert_eq!(res.authorized_count, 3); // coordinator + 2 users
        assert_eq!(res.contract_address, env.contract.address.to_string());
    }

    #[test]
    fn query_list_authorized_empty() {
        let mut deps = mock_dependencies();
        let msg = InstantiateMsg {
            coordinator: "coordinator".to_string(),
            initial_authorized: None,
        };
        let info = mock_info("creator", &[]);
        instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

        let res = query_list_authorized(deps.as_ref(), None, None).unwrap();
        assert_eq!(res.addresses.len(), 1);
        assert_eq!(res.addresses[0], "coordinator");
    }

    #[test]
    fn query_list_authorized_with_limit() {
        let mut deps = mock_dependencies();
        let initial_authorized = vec![
            "user1".to_string(),
            "user2".to_string(),
            "user3".to_string(),
        ];
        let msg = InstantiateMsg {
            coordinator: "coordinator".to_string(),
            initial_authorized: Some(initial_authorized),
        };
        let info = mock_info("creator", &[]);
        instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

        // Query with limit of 2
        let res = query_list_authorized(deps.as_ref(), None, Some(2)).unwrap();
        assert_eq!(res.addresses.len(), 2);

        // Query remaining with start_after
        let start_after = res.addresses.last().unwrap().clone();
        let res2 = query_list_authorized(deps.as_ref(), Some(start_after), Some(2)).unwrap();
        assert_eq!(res2.addresses.len(), 2);
    }

    #[test]
    fn query_list_authorized_max_limit() {
        let mut deps = mock_dependencies();
        let msg = InstantiateMsg {
            coordinator: "coordinator".to_string(),
            initial_authorized: None,
        };
        let info = mock_info("creator", &[]);
        instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

        // Query with limit exceeding MAX_LIMIT should be capped
        let res = query_list_authorized(deps.as_ref(), None, Some(2000)).unwrap();
        assert!(res.addresses.len() <= MAX_LIMIT as usize);
    }

    #[test]
    fn query_info_comprehensive() {
        let mut deps = mock_dependencies();
        let initial_authorized = vec!["user1".to_string(), "user2".to_string()];
        let msg = InstantiateMsg {
            coordinator: "coordinator".to_string(),
            initial_authorized: Some(initial_authorized),
        };
        let info = mock_info("creator", &[]);
        instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

        let env = mock_env();
        let res = query_info(deps.as_ref(), env.clone()).unwrap();

        assert_eq!(res.admin, "coordinator");
        assert_eq!(res.authorized_count, 3); // coordinator + 2 users
        assert_eq!(res.contract_address, env.contract.address.to_string());
    }

    #[test]
    fn execute_multiple_operations() {
        let mut deps = mock_dependencies();
        let msg = InstantiateMsg {
            coordinator: COORDINATOR.to_string(),
            initial_authorized: None,
        };
        let info = mock_info(CREATOR, &[]);
        instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

        // Add multiple addresses
        let addresses = vec![USER1, USER2, USER3];
        for addr in &addresses {
            let info = mock_info(COORDINATOR, &[]);
            execute_add_authorized(deps.as_mut(), info, addr.to_string()).unwrap();
        }

        // Verify all are authorized
        for addr in &addresses {
            let res = query_is_authorized(deps.as_ref(), addr.to_string()).unwrap();
            assert!(res.authorized);
        }

        // Remove some addresses
        let to_remove = vec![USER1, USER3];
        for addr in &to_remove {
            let info = mock_info(COORDINATOR, &[]);
            execute_remove_authorized(deps.as_mut(), info, addr.to_string()).unwrap();
        }

        // Verify remaining addresses
        let res = query_is_authorized(deps.as_ref(), USER1.to_string()).unwrap();
        assert!(!res.authorized);
        let res = query_is_authorized(deps.as_ref(), USER2.to_string()).unwrap();
        assert!(res.authorized);
        let res = query_is_authorized(deps.as_ref(), USER3.to_string()).unwrap();
        assert!(!res.authorized);
    }

    #[test]
    fn admin_transfer_preserves_authorization() {
        let mut deps = mock_dependencies();
        let msg = InstantiateMsg {
            coordinator: COORDINATOR.to_string(),
            initial_authorized: Some(vec![USER1.to_string()]),
        };
        let info = mock_info(CREATOR, &[]);
        instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

        // Transfer admin to user1
        let info = mock_info(COORDINATOR, &[]);
        execute_transfer_admin(deps.as_mut(), info, USER1.to_string()).unwrap();

        // Verify new admin
        let config = CONFIG.load(deps.as_ref().storage).unwrap();
        assert_eq!(config.admin.to_string(), USER1);

        // Verify old admin is still authorized
        let res = query_is_authorized(deps.as_ref(), COORDINATOR.to_string()).unwrap();
        assert!(res.authorized);

        // Verify new admin can perform admin operations
        let info = mock_info(USER1, &[]);
        execute_add_authorized(deps.as_mut(), info, USER2.to_string()).unwrap();

        let res = query_is_authorized(deps.as_ref(), USER2.to_string()).unwrap();
        assert!(res.authorized);
    }
}
