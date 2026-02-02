#[cfg(test)]
mod tests {
    use cosmwasm_std::testing::{mock_dependencies, mock_env, mock_info, MockApi};
    use cosmwasm_std::{from_json, to_json_binary, Addr, CosmosMsg, SubMsg, WasmMsg, Reply, SubMsgResponse, SubMsgResult, Event, Attribute};
    use cw_multi_test::{App, ContractWrapper, Executor};
    use std::collections::HashMap;

    use crate::contract::{execute, instantiate, reply};
    use crate::error::ContractError;
    use crate::query as query_fn;
    use crate::msg::{
        DefaultsResponse, ExecuteMsg, InfoResponse, InstantiateMsg, KeysResponse,
        RenderedSdlResponse, RenderedJsonResponse, SingleDefaultResponse, TemplateResponse, QueryMsg,
    };
    use crate::query::{
        query_defaults, query_info, query_keys, query_render_sdl, query_rendered_json,
        query_single_default, query_template,
    };

    // Helper to create a valid address for testing
    fn mock_address(name: &str) -> String {
        MockApi::default().addr_make(name).to_string()
    }

    // Helper to create contract wrapper for cw-multi-test
    fn contract_template() -> Box<dyn cw_multi_test::Contract<cosmwasm_std::Empty>> {
        let contract = ContractWrapper::new(execute, instantiate, crate::query).with_reply(reply);
        Box::new(contract)
    }

    fn sample_sdl_template() -> String {
        r#"{
  "version": "2.0",
  "services": {
    "web": {
      "image": "nginx:latest",
      "expose": [
        {
          "port": 80,
          "as": 80,
          "to": [{"global": true}]
        }
      ]
    }
  },
  "profiles": {
    "compute": {
      "web": {
        "resources": {
          "cpu": {
            "units": "${CPU}"
          },
          "memory": {
            "size": "${MEMORY}"
          },
          "storage": {
            "size": "${STORAGE}"
          }
        }
      }
    }
  }
}"#
        .to_string()
    }

    fn sample_defaults() -> HashMap<String, String> {
        let mut defaults = HashMap::new();
        defaults.insert("CPU".to_string(), "1.0".to_string());
        defaults.insert("MEMORY".to_string(), "512Mi".to_string());
        defaults.insert("STORAGE".to_string(), "1Gi".to_string());
        defaults
    }

    #[test]
    fn proper_initialization() {
        let mut deps = mock_dependencies();

        let msg = InstantiateMsg {
            sdl_template: sample_sdl_template(),
            variable_defaults: sample_defaults(),
            label: Some("nginx-template".to_string()),
            admin: None,
        };

        let info = mock_info("creator", &[]);
        let res = instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

        assert_eq!(res.attributes.len(), 4);
        assert_eq!(res.attributes[0].key, "method");
        assert_eq!(res.attributes[0].value, "instantiate");
    }

    #[test]
    fn query_template_works() {
        let mut deps = mock_dependencies();

        let msg = InstantiateMsg {
            sdl_template: sample_sdl_template(),
            variable_defaults: sample_defaults(),
            label: Some("test-template".to_string()),
            admin: None,
        };

        let info = mock_info("creator", &[]);
        instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

        let res = query_template(deps.as_ref()).unwrap();
        assert_eq!(res.sdl_template, sample_sdl_template());
        assert!(res.template_json.is_object());
    }

    #[test]
    fn query_defaults_works() {
        let mut deps = mock_dependencies();

        let msg = InstantiateMsg {
            sdl_template: sample_sdl_template(),
            variable_defaults: sample_defaults(),
            label: None,
            admin: None,
        };

        let info = mock_info("creator", &[]);
        instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

        let res = query_defaults(deps.as_ref()).unwrap();
        assert_eq!(res.defaults.len(), 3);
        assert_eq!(res.defaults.get("CPU").unwrap(), "1.0");
    }

    #[test]
    fn query_single_default_works() {
        let mut deps = mock_dependencies();

        let msg = InstantiateMsg {
            sdl_template: sample_sdl_template(),
            variable_defaults: sample_defaults(),
            label: None,
            admin: None,
        };

        let info = mock_info("creator", &[]);
        instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

        let res = query_single_default(deps.as_ref(), "CPU".to_string()).unwrap();
        assert_eq!(res.key, "CPU");
        assert_eq!(res.value, Some("1.0".to_string()));

        let res = query_single_default(deps.as_ref(), "NONEXISTENT".to_string()).unwrap();
        assert_eq!(res.value, None);
    }

    #[test]
    fn query_keys_works() {
        let mut deps = mock_dependencies();

        let msg = InstantiateMsg {
            sdl_template: sample_sdl_template(),
            variable_defaults: sample_defaults(),
            label: None,
            admin: None,
        };

        let info = mock_info("creator", &[]);
        instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

        let res = query_keys(deps.as_ref()).unwrap();
        assert_eq!(res.keys.len(), 3);
        assert!(res.keys.contains(&"CPU".to_string()));
    }

    #[test]
    fn render_sdl_with_defaults() {
        let mut deps = mock_dependencies();

        let msg = InstantiateMsg {
            sdl_template: sample_sdl_template(),
            variable_defaults: sample_defaults(),
            label: None,
            admin: None,
        };

        let info = mock_info("creator", &[]);
        instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

        let res = query_render_sdl(deps.as_ref(), None).unwrap();
        assert!(res.rendered_sdl.contains("1.0"));
        assert!(res.rendered_sdl.contains("512Mi"));
        assert!(res.rendered_sdl.contains("1Gi"));
        assert_eq!(res.used_variables.len(), 3);
    }

    #[test]
    fn render_sdl_with_custom_variables() {
        let mut deps = mock_dependencies();

        let msg = InstantiateMsg {
            sdl_template: sample_sdl_template(),
            variable_defaults: sample_defaults(),
            label: None,
            admin: None,
        };

        let info = mock_info("creator", &[]);
        instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

        let mut custom_vars = HashMap::new();
        custom_vars.insert("CPU".to_string(), "2.0".to_string());

        let res = query_render_sdl(deps.as_ref(), Some(custom_vars)).unwrap();
        assert!(res.rendered_sdl.contains("2.0"));
        assert!(res.rendered_sdl.contains("512Mi")); // Default still used
    }

    #[test]
    fn get_rendered_json_with_defaults() {
        let mut deps = mock_dependencies();

        let msg = InstantiateMsg {
            sdl_template: sample_sdl_template(),
            variable_defaults: sample_defaults(),
            label: None,
            admin: None,
        };

        let info = mock_info("creator", &[]);
        instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

        let res = query_rendered_json(deps.as_ref(), None).unwrap();

        // Verify it's valid JSON
        assert!(res.sdl_json.is_object());

        // Verify variables were substituted
        assert_eq!(res.used_variables.len(), 3);
        assert_eq!(res.used_variables.get("CPU").unwrap(), "1.0");
        assert_eq!(res.used_variables.get("MEMORY").unwrap(), "512Mi");
        assert_eq!(res.used_variables.get("STORAGE").unwrap(), "1Gi");

        // Verify the JSON contains the substituted values
        let json_str = serde_json::to_string(&res.sdl_json).unwrap();
        assert!(json_str.contains("1.0"));
        assert!(json_str.contains("512Mi"));
        assert!(json_str.contains("1Gi"));
    }

    #[test]
    fn get_rendered_json_with_custom_variables() {
        let mut deps = mock_dependencies();

        let msg = InstantiateMsg {
            sdl_template: sample_sdl_template(),
            variable_defaults: sample_defaults(),
            label: None,
            admin: None,
        };

        let info = mock_info("creator", &[]);
        instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

        let mut custom_vars = HashMap::new();
        custom_vars.insert("CPU".to_string(), "4.0".to_string());
        custom_vars.insert("MEMORY".to_string(), "2Gi".to_string());

        let res = query_rendered_json(deps.as_ref(), Some(custom_vars)).unwrap();

        // Verify custom values were used
        assert_eq!(res.used_variables.get("CPU").unwrap(), "4.0");
        assert_eq!(res.used_variables.get("MEMORY").unwrap(), "2Gi");
        assert_eq!(res.used_variables.get("STORAGE").unwrap(), "1Gi"); // Default

        let json_str = serde_json::to_string(&res.sdl_json).unwrap();
        assert!(json_str.contains("4.0"));
        assert!(json_str.contains("2Gi"));
    }

    #[test]
    fn update_template_as_admin() {
        let mut deps = mock_dependencies();

        let msg = InstantiateMsg {
            sdl_template: sample_sdl_template(),
            variable_defaults: sample_defaults(),
            label: None,
            admin: None, // Sender will become admin
        };

        let info = mock_info("admin", &[]);
        instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

        let new_template = r#"{"version": "3.0"}"#.to_string();
        let msg = ExecuteMsg::UpdateTemplate {
            sdl_template: new_template.clone(),
        };

        let info = mock_info("admin", &[]);
        let res = execute(deps.as_mut(), mock_env(), info, msg).unwrap();
        assert_eq!(res.attributes[0].value, "update_template");

        let template = query_template(deps.as_ref()).unwrap();
        assert_eq!(template.sdl_template, new_template);
    }

    #[test]
    fn update_template_unauthorized() {
        let mut deps = mock_dependencies();

        let msg = InstantiateMsg {
            sdl_template: sample_sdl_template(),
            variable_defaults: sample_defaults(),
            label: None,
            admin: None, // Creator will be admin
        };

        let info = mock_info("admin", &[]);
        instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

        let msg = ExecuteMsg::UpdateTemplate {
            sdl_template: r#"{"version": "3.0"}"#.to_string(),
        };

        let info = mock_info("unauthorized", &[]);
        let err = execute(deps.as_mut(), mock_env(), info, msg).unwrap_err();
        assert_eq!(err, ContractError::Unauthorized {});
    }

    #[test]
    fn update_single_default() {
        let mut deps = mock_dependencies();

        let msg = InstantiateMsg {
            sdl_template: sample_sdl_template(),
            variable_defaults: sample_defaults(),
            label: None,
            admin: None,
        };

        let info = mock_info("creator", &[]);
        instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

        let msg = ExecuteMsg::UpdateSingleDefault {
            key: "CPU".to_string(),
            value: "4.0".to_string(),
        };

        let info = mock_info("creator", &[]);
        execute(deps.as_mut(), mock_env(), info, msg).unwrap();

        let res = query_single_default(deps.as_ref(), "CPU".to_string()).unwrap();
        assert_eq!(res.value, Some("4.0".to_string()));
    }

    #[test]
    fn remove_default() {
        let mut deps = mock_dependencies();

        let msg = InstantiateMsg {
            sdl_template: sample_sdl_template(),
            variable_defaults: sample_defaults(),
            label: None,
            admin: None,
        };

        let info = mock_info("creator", &[]);
        instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

        let msg = ExecuteMsg::RemoveDefault {
            key: "CPU".to_string(),
        };

        let info = mock_info("creator", &[]);
        execute(deps.as_mut(), mock_env(), info, msg).unwrap();

        let res = query_single_default(deps.as_ref(), "CPU".to_string()).unwrap();
        assert_eq!(res.value, None);
    }

    #[test]
    fn transfer_admin() {
        let mut deps = mock_dependencies();

        let admin1 = mock_address("admin1");
        let admin2 = mock_address("admin2");

        let msg = InstantiateMsg {
            sdl_template: sample_sdl_template(),
            variable_defaults: sample_defaults(),
            label: None,
            admin: None, // admin1 will be admin
        };

        let info = mock_info(&admin1, &[]);
        instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

        let msg = ExecuteMsg::TransferAdmin {
            new_admin: admin2.clone(),
        };

        let info = mock_info(&admin1, &[]);
        execute(deps.as_mut(), mock_env(), info, msg).unwrap();

        // Old admin can no longer update
        let msg = ExecuteMsg::UpdateLabel {
            label: "new-label".to_string(),
        };
        let info = mock_info(&admin1, &[]);
        let err = execute(deps.as_mut(), mock_env(), info, msg).unwrap_err();
        assert_eq!(err, ContractError::Unauthorized {});

        // New admin can update
        let msg = ExecuteMsg::UpdateLabel {
            label: "new-label".to_string(),
        };
        let info = mock_info(&admin2, &[]);
        execute(deps.as_mut(), mock_env(), info, msg).unwrap();
    }

    #[test]
    fn invalid_json_template_rejected() {
        let mut deps = mock_dependencies();

        let msg = InstantiateMsg {
            sdl_template: "not valid json".to_string(),
            variable_defaults: HashMap::new(),
            label: None,
            admin: None,
        };

        let info = mock_info("creator", &[]);
        let err = instantiate(deps.as_mut(), mock_env(), info, msg).unwrap_err();
        match err {
            ContractError::InvalidTemplate { .. } => {}
            _ => panic!("Expected InvalidTemplate error"),
        }
    }

    #[test]
    fn missing_variable_defaults_rejected() {
        let mut deps = mock_dependencies();

        let template = r#"{"cpu": "${CPU}", "memory": "${MEMORY}", "storage": "${STORAGE}"}"#;
        let mut defaults = HashMap::new();
        defaults.insert("CPU".to_string(), "1.0".to_string());
        // Missing MEMORY and STORAGE defaults

        let msg = InstantiateMsg {
            sdl_template: template.to_string(),
            variable_defaults: defaults,
            label: None,
            admin: None,
        };

        let info = mock_info("creator", &[]);
        let err = instantiate(deps.as_mut(), mock_env(), info, msg).unwrap_err();
        match err {
            ContractError::MissingVariableDefaults { variables } => {
                assert_eq!(variables.len(), 2);
                assert!(variables.contains(&"MEMORY".to_string()));
                assert!(variables.contains(&"STORAGE".to_string()));
            }
            _ => panic!("Expected MissingVariableDefaults error, got: {:?}", err),
        }
    }

    #[test]
    fn all_variable_defaults_provided() {
        let mut deps = mock_dependencies();

        let template = r#"{"cpu": "${CPU}", "memory": "${MEMORY}"}"#;
        let mut defaults = HashMap::new();
        defaults.insert("CPU".to_string(), "1.0".to_string());
        defaults.insert("MEMORY".to_string(), "512Mi".to_string());

        let msg = InstantiateMsg {
            sdl_template: template.to_string(),
            variable_defaults: defaults,
            label: None,
            admin: None,
        };

        let info = mock_info("creator", &[]);
        let res = instantiate(deps.as_mut(), mock_env(), info, msg);
        assert!(res.is_ok());
    }

    #[test]
    fn extra_defaults_allowed() {
        let mut deps = mock_dependencies();

        let template = r#"{"cpu": "${CPU}"}"#;
        let mut defaults = HashMap::new();
        defaults.insert("CPU".to_string(), "1.0".to_string());
        defaults.insert("MEMORY".to_string(), "512Mi".to_string());
        defaults.insert("STORAGE".to_string(), "1Gi".to_string());

        let msg = InstantiateMsg {
            sdl_template: template.to_string(),
            variable_defaults: defaults,
            label: None,
            admin: None,
        };

        let info = mock_info("creator", &[]);
        let res = instantiate(deps.as_mut(), mock_env(), info, msg);
        assert!(res.is_ok());
    }

    #[test]
    fn update_template_with_missing_defaults_rejected() {
        let mut deps = mock_dependencies();

        // Initialize with valid template
        let template = r#"{"cpu": "${CPU}"}"#;
        let mut defaults = HashMap::new();
        defaults.insert("CPU".to_string(), "1.0".to_string());

        let msg = InstantiateMsg {
            sdl_template: template.to_string(),
            variable_defaults: defaults,
            label: None,
            admin: None,
        };

        let info = mock_info("admin", &[]);
        instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

        // Try to update with template that requires more variables
        let new_template = r#"{"cpu": "${CPU}", "memory": "${MEMORY}", "storage": "${STORAGE}"}"#;
        let msg = ExecuteMsg::UpdateTemplate {
            sdl_template: new_template.to_string(),
        };

        let info = mock_info("admin", &[]);
        let err = execute(deps.as_mut(), mock_env(), info, msg).unwrap_err();
        match err {
            ContractError::MissingVariableDefaults { variables } => {
                assert_eq!(variables.len(), 2);
                assert!(variables.contains(&"MEMORY".to_string()));
                assert!(variables.contains(&"STORAGE".to_string()));
            }
            _ => panic!("Expected MissingVariableDefaults error, got: {:?}", err),
        }
    }

    #[test]
    fn update_template_with_subset_of_defaults() {
        let mut deps = mock_dependencies();

        // Initialize with template using 3 variables
        let template = r#"{"cpu": "${CPU}", "memory": "${MEMORY}", "storage": "${STORAGE}"}"#;
        let mut defaults = HashMap::new();
        defaults.insert("CPU".to_string(), "1.0".to_string());
        defaults.insert("MEMORY".to_string(), "512Mi".to_string());
        defaults.insert("STORAGE".to_string(), "1Gi".to_string());

        let msg = InstantiateMsg {
            sdl_template: template.to_string(),
            variable_defaults: defaults,
            label: None,
            admin: None,
        };

        let info = mock_info("admin", &[]);
        instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

        // Update to template using only 1 variable (should succeed)
        let new_template = r#"{"cpu": "${CPU}"}"#;
        let msg = ExecuteMsg::UpdateTemplate {
            sdl_template: new_template.to_string(),
        };

        let info = mock_info("admin", &[]);
        let res = execute(deps.as_mut(), mock_env(), info, msg);
        assert!(res.is_ok());
    }

    #[test]
    fn instantiate_new_creates_child_contract() {
        let mut app = App::default();

        // Store contract code
        let code_id = app.store_code(contract_template());

        // Instantiate parent contract
        let parent_msg = InstantiateMsg {
            sdl_template: sample_sdl_template(),
            variable_defaults: sample_defaults(),
            label: Some("parent".to_string()),
            admin: None,
        };

        let parent_addr = app
            .instantiate_contract(
                code_id,
                Addr::unchecked("creator"),
                &parent_msg,
                &[],
                "parent-contract",
                None,
            )
            .unwrap();

        // Create new instance via factory
        let new_template = r#"{"version": "2.0", "cpu": "${CPU}"}"#;
        let mut new_defaults = HashMap::new();
        new_defaults.insert("CPU".to_string(), "2.0".to_string());

        let new_instantiate_msg = InstantiateMsg {
            sdl_template: new_template.to_string(),
            variable_defaults: new_defaults.clone(),
            label: Some("child".to_string()),
            admin: None,
        };

        let factory_msg = ExecuteMsg::InstantiateNew {
            instantiate_msg: new_instantiate_msg,
            label: "factory-child".to_string(),
            parent_results: None,
        };

        // Execute factory instantiation
        let res = app
            .execute_contract(
                Addr::unchecked("creator"),
                parent_addr.clone(),
                &factory_msg,
                &[],
            )
            .unwrap();

        // Verify a new contract was instantiated
        // The response should contain events from the instantiation
        let instantiate_events: Vec<_> = res
            .events
            .iter()
            .filter(|e| e.ty == "instantiate")
            .collect();
        assert!(instantiate_events.len() >= 1, "Should have instantiate event");

        // Find the new contract address from events
        let child_addr_attr = instantiate_events
            .iter()
            .flat_map(|e| &e.attributes)
            .find(|a| a.key == "_contract_address" || a.key == "contract_address")
            .expect("Should have contract address");

        let child_addr = Addr::unchecked(&child_addr_attr.value);

        // Verify child contract was created with correct template
        let query_msg = QueryMsg::GetTemplate {};
        let template_res: TemplateResponse = app
            .wrap()
            .query_wasm_smart(child_addr.clone(), &query_msg)
            .unwrap();

        assert_eq!(template_res.sdl_template, new_template);

        // Verify defaults were set correctly
        let defaults_msg = QueryMsg::GetDefaults {};
        let defaults_res: DefaultsResponse = app
            .wrap()
            .query_wasm_smart(child_addr, &defaults_msg)
            .unwrap();

        assert_eq!(defaults_res.defaults.get("CPU").unwrap(), "2.0");
    }

    #[test]
    fn reply_handles_instantiate_success() {
        let mut deps = mock_dependencies();

        // Create a successful reply with contract address (must be valid bech32)
        let contract_addr = "cosmwasm1h34lmpywh4upnjdg90cjf4j70aee6z8qqfspugamjp42e4q28kqsksmtyp";
        let reply_msg = Reply {
            id: 1, // INSTANTIATE_REPLY_ID
            payload: cosmwasm_std::Binary::default(),
            gas_used: 0,
            result: SubMsgResult::Ok(SubMsgResponse {
                events: vec![
                    Event::new("instantiate")
                        .add_attribute("_contract_address", contract_addr)
                        .add_attribute("code_id", "1"),
                    Event::new("wasm")
                        .add_attribute("child_label", "test-child")
                ],
                data: None,
                msg_responses: vec![],
            }),
        };

        let res = reply(deps.as_mut(), mock_env(), reply_msg).unwrap();

        // Verify response attributes (now includes child_label)
        assert_eq!(res.attributes.len(), 3);
        assert_eq!(res.attributes[0].key, "method");
        assert_eq!(res.attributes[0].value, "reply_instantiate");
        assert_eq!(res.attributes[1].key, "new_contract_address");
        assert_eq!(res.attributes[1].value, contract_addr);
        assert_eq!(res.attributes[2].key, "child_label");
        assert_eq!(res.attributes[2].value, "test-child");
    }

    #[test]
    fn reply_handles_missing_contract_address() {
        let mut deps = mock_dependencies();

        // Create a reply without contract address
        let reply_msg = Reply {
            id: 1,
            payload: cosmwasm_std::Binary::default(),
            gas_used: 0,
            result: SubMsgResult::Ok(SubMsgResponse {
                events: vec![Event::new("instantiate")],
                data: None,
                msg_responses: vec![],
            }),
        };

        let err = reply(deps.as_mut(), mock_env(), reply_msg).unwrap_err();
        match err {
            ContractError::Std(_) => {}
            _ => panic!("Expected Std error for missing contract address"),
        }
    }

    #[test]
    fn reply_handles_unknown_reply_id() {
        let mut deps = mock_dependencies();

        let reply_msg = Reply {
            id: 999, // Unknown ID
            payload: cosmwasm_std::Binary::default(),
            gas_used: 0,
            result: SubMsgResult::Ok(SubMsgResponse {
                events: vec![],
                data: None,
                msg_responses: vec![],
            }),
        };

        let err = reply(deps.as_mut(), mock_env(), reply_msg).unwrap_err();
        match err {
            ContractError::Std(_) => {}
            _ => panic!("Expected Std error for unknown reply ID"),
        }
    }

    #[test]
    fn reply_handles_failed_instantiate() {
        let mut deps = mock_dependencies();

        let reply_msg = Reply {
            id: 1,
            payload: cosmwasm_std::Binary::default(),
            gas_used: 0,
            result: SubMsgResult::Err("instantiation failed".to_string()),
        };

        let err = reply(deps.as_mut(), mock_env(), reply_msg).unwrap_err();
        match err {
            ContractError::Std(_) => {}
            _ => panic!("Expected Std error for failed instantiation"),
        }
    }

    #[test]
    fn factory_rejects_invalid_child_contract() {
        let mut app = App::default();

        // Store contract code
        let code_id = app.store_code(contract_template());

        // Instantiate parent contract
        let parent_msg = InstantiateMsg {
            sdl_template: sample_sdl_template(),
            variable_defaults: sample_defaults(),
            label: Some("parent".to_string()),
            admin: None,
        };

        let parent_addr = app
            .instantiate_contract(
                code_id,
                Addr::unchecked("creator"),
                &parent_msg,
                &[],
                "parent-contract",
                None,
            )
            .unwrap();

        // Try to create child with missing defaults
        let new_template = r#"{"cpu": "${CPU}", "memory": "${MEMORY}", "storage": "${STORAGE}"}"#;
        let mut new_defaults = HashMap::new();
        new_defaults.insert("CPU".to_string(), "2.0".to_string());
        // Missing MEMORY and STORAGE defaults

        let new_instantiate_msg = InstantiateMsg {
            sdl_template: new_template.to_string(),
            variable_defaults: new_defaults,
            label: Some("child".to_string()),
            admin: None,
        };

        let factory_msg = ExecuteMsg::InstantiateNew {
            instantiate_msg: new_instantiate_msg,
            label: "factory-child".to_string(),
            parent_results: None,
        };

        // Execute should fail because child instantiation will fail validation
        let err = app
            .execute_contract(
                Addr::unchecked("creator"),
                parent_addr,
                &factory_msg,
                &[],
            )
            .unwrap_err();

        // Verify it's a validation error
        let err_msg = err.to_string();
        assert!(
            err_msg.contains("MissingVariableDefaults") || err_msg.contains("MEMORY") || err_msg.contains("STORAGE"),
            "Expected validation error for missing defaults, got: {}",
            err_msg
        );
    }

    #[test]
    fn multiple_child_contracts_can_be_created() {
        let mut app = App::default();

        // Store contract code
        let code_id = app.store_code(contract_template());

        // Instantiate parent contract
        let parent_msg = InstantiateMsg {
            sdl_template: sample_sdl_template(),
            variable_defaults: sample_defaults(),
            label: Some("parent".to_string()),
            admin: None,
        };

        let parent_addr = app
            .instantiate_contract(
                code_id,
                Addr::unchecked("creator"),
                &parent_msg,
                &[],
                "parent-contract",
                None,
            )
            .unwrap();

        // Create first child
        let child1_template = r#"{"cpu": "${CPU}"}"#;
        let mut child1_defaults = HashMap::new();
        child1_defaults.insert("CPU".to_string(), "1.0".to_string());

        let child1_msg = ExecuteMsg::InstantiateNew {
            instantiate_msg: InstantiateMsg {
                sdl_template: child1_template.to_string(),
                variable_defaults: child1_defaults,
                label: Some("child1".to_string()),
                admin: None,
            },
            label: "child1-contract".to_string(),
            parent_results: None,
        };

        app.execute_contract(Addr::unchecked("creator"), parent_addr.clone(), &child1_msg, &[])
            .unwrap();

        // Create second child with different config
        let child2_template = r#"{"memory": "${MEMORY}"}"#;
        let mut child2_defaults = HashMap::new();
        child2_defaults.insert("MEMORY".to_string(), "1Gi".to_string());

        let child2_msg = ExecuteMsg::InstantiateNew {
            instantiate_msg: InstantiateMsg {
                sdl_template: child2_template.to_string(),
                variable_defaults: child2_defaults,
                label: Some("child2".to_string()),
                admin: None,
            },
            label: "child2-contract".to_string(),
            parent_results: None,
        };

        let res = app
            .execute_contract(Addr::unchecked("creator"), parent_addr, &child2_msg, &[])
            .unwrap();

        // Should succeed - verify we got instantiate events
        let instantiate_events: Vec<_> = res
            .events
            .iter()
            .filter(|e| e.ty == "instantiate")
            .collect();
        assert!(!instantiate_events.is_empty());
    }
}
