use auth_registry_updater::contract::{execute, instantiate, query};
use auth_registry_updater::msg::{
    AdminResponse, ExecuteMsg, InfoResponse, InstantiateMsg, IsAuthorizedResponse,
    ListAuthorizedResponse, QueryMsg,
};
use auth_registry_updater::ContractError;
use cosmwasm_std::{Addr, Coin, Empty};
use cw_multi_test::{App, Contract, ContractWrapper, Executor};

fn auth_registry_updater_contract() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new(execute, instantiate, query);
    Box::new(contract)
}

fn mock_app() -> App {
    App::default()
}

fn instantiate_contract(
    app: &mut App,
    sender: Addr,
    coordinator: String,
    initial_authorized: Option<Vec<String>>,
) -> Addr {
    let contract_code_id = app.store_code(auth_registry_updater_contract());

    let msg = InstantiateMsg {
        coordinator,
        initial_authorized,
    };

    app.instantiate_contract(
        contract_code_id,
        sender,
        &msg,
        &[],
        "cw-middleware-auth",
        None,
    )
    .unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    mod instantiate_tests {
        use super::*;

        #[test]
        fn test_instantiate_with_coordinator_only() {
            let mut app = mock_app();
            let sender = Addr::unchecked("creator");

            let coordinator = "coordinator_addr".to_string();
            let contract_addr = instantiate_contract(&mut app, sender, coordinator.clone(), None);

            // Verify coordinator is authorized
            let resp: IsAuthorizedResponse = app
                .wrap()
                .query_wasm_smart(
                    contract_addr.clone(),
                    &QueryMsg::IsAuthorized {
                        address: coordinator.clone(),
                    },
                )
                .unwrap();
            assert!(resp.authorized);

            // Verify admin is set correctly
            let resp: AdminResponse = app
                .wrap()
                .query_wasm_smart(contract_addr, &QueryMsg::GetAdmin {})
                .unwrap();
            assert_eq!(resp.admin, coordinator);
        }

        #[test]
        fn test_instantiate_with_initial_authorized() {
            let mut app = mock_app();
            let sender = Addr::unchecked("creator");

            let coordinator = "coordinator_addr".to_string();
            let initial_authorized = vec!["user1".to_string(), "user2".to_string()];
            let contract_addr = instantiate_contract(
                &mut app,
                sender,
                coordinator.clone(),
                Some(initial_authorized.clone()),
            );

            // Verify coordinator is authorized
            let resp: IsAuthorizedResponse = app
                .wrap()
                .query_wasm_smart(
                    contract_addr.clone(),
                    &QueryMsg::IsAuthorized {
                        address: coordinator,
                    },
                )
                .unwrap();
            assert!(resp.authorized);

            // Verify initial authorized addresses
            for addr in initial_authorized {
                let resp: IsAuthorizedResponse = app
                    .wrap()
                    .query_wasm_smart(
                        contract_addr.clone(),
                        &QueryMsg::IsAuthorized { address: addr },
                    )
                    .unwrap();
                assert!(resp.authorized);
            }

            // Verify unauthorized address
            let resp: IsAuthorizedResponse = app
                .wrap()
                .query_wasm_smart(
                    contract_addr,
                    &QueryMsg::IsAuthorized {
                        address: "unauthorized".to_string(),
                    },
                )
                .unwrap();
            assert!(!resp.authorized);
        }

        #[test]
        fn test_instantiate_with_invalid_coordinator() {
            let mut app = mock_app();
            let contract_code_id = app.store_code(auth_registry_updater_contract());
            let sender = Addr::unchecked("creator");

            let msg = InstantiateMsg {
                coordinator: "invalid_address".to_string(),
                initial_authorized: None,
            };

            let err = app
                .instantiate_contract(
                    contract_code_id,
                    sender,
                    &msg,
                    &[],
                    "cw-middleware-auth",
                    None,
                )
                .unwrap_err();
            assert!(err.to_string().contains("Invalid input"));
        }
    }

    mod execute_tests {
        use super::*;

        #[test]
        fn test_add_authorized_success() {
            let mut app = mock_app();
            let sender = Addr::unchecked("creator");

            let coordinator = "coordinator_addr".to_string();
            let contract_addr = instantiate_contract(&mut app, sender, coordinator.clone(), None);

            // Add a new authorized address as admin
            let new_addr = "new_authorized".to_string();
            app.execute_contract(
                Addr::unchecked(&coordinator),
                contract_addr.clone(),
                &ExecuteMsg::AddAuthorized {
                    address: new_addr.clone(),
                },
                &[],
            )
            .unwrap();

            // Verify the address is now authorized
            let resp: IsAuthorizedResponse = app
                .wrap()
                .query_wasm_smart(contract_addr, &QueryMsg::IsAuthorized { address: new_addr })
                .unwrap();
            assert!(resp.authorized);
        }

        #[test]
        fn test_add_authorized_unauthorized() {
            let mut app = mock_app();
            let sender = Addr::unchecked("creator");

            let coordinator = "coordinator_addr".to_string();
            let contract_addr = instantiate_contract(&mut app, sender, coordinator, None);

            // Try to add authorized as non-admin
            let err = app
                .execute_contract(
                    Addr::unchecked("unauthorized"),
                    contract_addr,
                    &ExecuteMsg::AddAuthorized {
                        address: "new_addr".to_string(),
                    },
                    &[],
                )
                .unwrap_err();
            assert!(err.to_string().contains("Unauthorized"));
        }

        #[test]
        fn test_add_authorized_already_authorized() {
            let mut app = mock_app();
            let sender = Addr::unchecked("creator");

            let coordinator = "coordinator_addr".to_string();
            let contract_addr = instantiate_contract(&mut app, sender, coordinator.clone(), None);

            // Try to add coordinator again
            let err = app
                .execute_contract(
                    Addr::unchecked(&coordinator),
                    contract_addr,
                    &ExecuteMsg::AddAuthorized {
                        address: coordinator,
                    },
                    &[],
                )
                .unwrap_err();
            assert!(err.to_string().contains("already authorized"));
        }

        #[test]
        fn test_remove_authorized_success() {
            let mut app = mock_app();
            let sender = Addr::unchecked("creator");

            let coordinator = "coordinator_addr".to_string();
            let initial_authorized = vec!["user1".to_string()];
            let contract_addr = instantiate_contract(
                &mut app,
                sender,
                coordinator.clone(),
                Some(initial_authorized),
            );

            // Remove the authorized address as admin
            app.execute_contract(
                Addr::unchecked(&coordinator),
                contract_addr.clone(),
                &ExecuteMsg::RemoveAuthorized {
                    address: "user1".to_string(),
                },
                &[],
            )
            .unwrap();

            // Verify the address is no longer authorized
            let resp: IsAuthorizedResponse = app
                .wrap()
                .query_wasm_smart(
                    contract_addr,
                    &QueryMsg::IsAuthorized {
                        address: "user1".to_string(),
                    },
                )
                .unwrap();
            assert!(!resp.authorized);
        }

        #[test]
        fn test_remove_authorized_admin() {
            let mut app = mock_app();
            let sender = Addr::unchecked("creator");

            let coordinator = "coordinator_addr".to_string();
            let contract_addr = instantiate_contract(&mut app, sender, coordinator.clone(), None);

            // Try to remove admin
            let err = app
                .execute_contract(
                    Addr::unchecked(&coordinator),
                    contract_addr,
                    &ExecuteMsg::RemoveAuthorized {
                        address: coordinator,
                    },
                    &[],
                )
                .unwrap_err();
            assert!(err.to_string().contains("Cannot remove admin"));
        }

        #[test]
        fn test_remove_authorized_not_authorized() {
            let mut app = mock_app();
            let sender = Addr::unchecked("creator");

            let coordinator = "coordinator_addr".to_string();
            let contract_addr = instantiate_contract(&mut app, sender, coordinator.clone(), None);

            // Try to remove non-authorized address
            let err = app
                .execute_contract(
                    Addr::unchecked(&coordinator),
                    contract_addr,
                    &ExecuteMsg::RemoveAuthorized {
                        address: "not_authorized".to_string(),
                    },
                    &[],
                )
                .unwrap_err();
            assert!(err.to_string().contains("not found in authorized list"));
        }

        #[test]
        fn test_transfer_admin_success() {
            let mut app = mock_app();
            let sender = Addr::unchecked("creator");

            let coordinator = "coordinator_addr".to_string();
            let contract_addr = instantiate_contract(&mut app, sender, coordinator.clone(), None);

            let new_admin = "new_admin_addr".to_string();

            // Transfer admin
            app.execute_contract(
                Addr::unchecked(&coordinator),
                contract_addr.clone(),
                &ExecuteMsg::TransferAdmin {
                    new_admin: new_admin.clone(),
                },
                &[],
            )
            .unwrap();

            // Verify new admin is set
            let resp: AdminResponse = app
                .wrap()
                .query_wasm_smart(contract_addr.clone(), &QueryMsg::GetAdmin {})
                .unwrap();
            assert_eq!(resp.admin, new_admin);

            // Verify new admin is authorized
            let resp: IsAuthorizedResponse = app
                .wrap()
                .query_wasm_smart(
                    contract_addr,
                    &QueryMsg::IsAuthorized { address: new_admin },
                )
                .unwrap();
            assert!(resp.authorized);
        }

        #[test]
        fn test_transfer_admin_unauthorized() {
            let mut app = mock_app();
            let sender = Addr::unchecked("creator");

            let coordinator = "coordinator_addr".to_string();
            let contract_addr = instantiate_contract(&mut app, sender, coordinator, None);

            // Try to transfer admin as non-admin
            let err = app
                .execute_contract(
                    Addr::unchecked("unauthorized"),
                    contract_addr,
                    &ExecuteMsg::TransferAdmin {
                        new_admin: "new_admin".to_string(),
                    },
                    &[],
                )
                .unwrap_err();
            assert!(err.to_string().contains("Unauthorized"));
        }
    }

    mod query_tests {
        use super::*;

        #[test]
        fn test_query_is_authorized() {
            let mut app = mock_app();
            let sender = Addr::unchecked("creator");

            let coordinator = "coordinator_addr".to_string();
            let initial_authorized = vec!["user1".to_string()];
            let contract_addr = instantiate_contract(
                &mut app,
                sender,
                coordinator.clone(),
                Some(initial_authorized),
            );

            // Test authorized addresses
            let test_cases = vec![
                (coordinator, true),
                ("user1".to_string(), true),
                ("unauthorized".to_string(), false),
                ("invalid_address".to_string(), false),
            ];

            for (address, expected) in test_cases {
                let resp: IsAuthorizedResponse = app
                    .wrap()
                    .query_wasm_smart(contract_addr.clone(), &QueryMsg::IsAuthorized { address })
                    .unwrap();
                assert_eq!(resp.authorized, expected);
            }
        }

        #[test]
        fn test_query_get_admin() {
            let mut app = mock_app();
            let sender = Addr::unchecked("creator");

            let coordinator = "coordinator_addr".to_string();
            let contract_addr = instantiate_contract(&mut app, sender, coordinator.clone(), None);

            let resp: AdminResponse = app
                .wrap()
                .query_wasm_smart(contract_addr, &QueryMsg::GetAdmin {})
                .unwrap();
            assert_eq!(resp.admin, coordinator);
        }

        #[test]
        fn test_query_list_authorized() {
            let mut app = mock_app();
            let sender = Addr::unchecked("creator");

            let coordinator = "coordinator_addr".to_string();
            let initial_authorized = vec![
                "user1".to_string(),
                "user2".to_string(),
                "user3".to_string(),
            ];
            let contract_addr = instantiate_contract(
                &mut app,
                sender,
                coordinator.clone(),
                Some(initial_authorized),
            );

            // Query all authorized addresses
            let resp: ListAuthorizedResponse = app
                .wrap()
                .query_wasm_smart(
                    contract_addr.clone(),
                    &QueryMsg::ListAuthorized {
                        start_after: None,
                        limit: None,
                    },
                )
                .unwrap();

            // Should include coordinator + initial authorized
            assert_eq!(resp.addresses.len(), 4);
            assert!(resp.addresses.contains(&coordinator));
            assert!(resp.addresses.contains(&"user1".to_string()));
            assert!(resp.addresses.contains(&"user2".to_string()));
            assert!(resp.addresses.contains(&"user3".to_string()));
        }

        #[test]
        fn test_query_list_authorized_with_pagination() {
            let mut app = mock_app();
            let sender = Addr::unchecked("creator");

            let coordinator = "coordinator_addr".to_string();
            let initial_authorized = vec![
                "user1".to_string(),
                "user2".to_string(),
                "user3".to_string(),
                "user4".to_string(),
            ];
            let contract_addr = instantiate_contract(
                &mut app,
                sender,
                coordinator.clone(),
                Some(initial_authorized),
            );

            // Query with limit
            let resp: ListAuthorizedResponse = app
                .wrap()
                .query_wasm_smart(
                    contract_addr.clone(),
                    &QueryMsg::ListAuthorized {
                        start_after: None,
                        limit: Some(2),
                    },
                )
                .unwrap();

            assert_eq!(resp.addresses.len(), 2);

            // Query with start_after
            let start_after = resp.addresses.last().unwrap().clone();
            let resp2: ListAuthorizedResponse = app
                .wrap()
                .query_wasm_smart(
                    contract_addr,
                    &QueryMsg::ListAuthorized {
                        start_after: Some(start_after),
                        limit: Some(2),
                    },
                )
                .unwrap();

            assert_eq!(resp2.addresses.len(), 2);
            // Ensure no overlap
            assert!(!resp
                .addresses
                .iter()
                .any(|addr| resp2.addresses.contains(addr)));
        }

        #[test]
        fn test_query_get_info() {
            let mut app = mock_app();
            let sender = Addr::unchecked("creator");

            let coordinator = "coordinator_addr".to_string();
            let initial_authorized = vec!["user1".to_string(), "user2".to_string()];
            let contract_addr = instantiate_contract(
                &mut app,
                sender,
                coordinator.clone(),
                Some(initial_authorized),
            );

            let resp: InfoResponse = app
                .wrap()
                .query_wasm_smart(contract_addr.clone(), &QueryMsg::GetInfo {})
                .unwrap();

            assert_eq!(resp.admin, coordinator);
            assert_eq!(resp.authorized_count, 3); // coordinator + 2 users
            assert_eq!(resp.contract_address, contract_addr.to_string());
        }
    }
}
