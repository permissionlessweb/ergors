//! CosmWasm Event Attribute Router
//!
//! Parses CosmWasm contract execution response events and routes them
//! to engine actions. Contracts emit events with type "ergors_action"
//! containing attributes that describe which engine functionality to invoke.
//!
//! # Event Format
//!
//! Reserved event type: `ergors_action`
//!
//! The `type` attribute within the event determines the action variant:
//! - `inference_request` - Route through LLM router
//! - `store_put` - Write to Cnidarium storage
//! - `log` - Emit a tracing event
//! - `p2p_message` - Send a P2P message to another node
//! - `akash_deploy` - Deploy to Akash

use cosmwasm_std::{Attribute, Event};

/// Reserved event type name for engine action routing
pub const ERGORS_ACTION_EVENT: &str = "ergors_action";

/// Reserved attribute key indicating the action type
const ATTR_TYPE: &str = "type";

/// Actions that can be triggered by contract events.
///
/// Each variant maps to an engine subsystem. The router parses
/// event attributes into these variants for downstream handling.
#[derive(Debug, Clone, PartialEq)]
pub enum EngineAction {
    /// Route an inference request through the LLM router
    InferenceRequest {
        model: String,
        prompt: String,
        callback_contract: Option<String>,
        callback_msg: Option<String>,
    },
    /// Store data in Cnidarium storage
    StorePut { key: String, value: Vec<u8> },
    /// Send a P2P message to another node
    P2pMessage {
        target_node: String,
        channel: u8,
        payload: Vec<u8>,
    },
    /// Emit a log/tracing event
    Log { level: String, message: String },
    /// Deploy to Akash
    AkashDeploy { sdl: String, label: String },
}

/// Result of processing a single engine action
#[derive(Debug, Clone)]
pub struct ActionResult {
    /// The action that was processed
    pub action_type: String,
    /// Whether the action succeeded
    pub success: bool,
    /// Optional result data or error message
    pub detail: Option<String>,
}

/// Parse engine actions from CosmWasm execution response events.
///
/// Scans for events with type `ergors_action` and converts their
/// attributes into `EngineAction` variants. Unrecognized action types
/// are silently skipped.
pub fn parse_engine_actions(events: &[Event]) -> Vec<EngineAction> {
    let mut actions = Vec::new();

    for event in events {
        if event.ty != ERGORS_ACTION_EVENT {
            continue;
        }
        if let Some(action) = parse_action_from_attributes(&event.attributes) {
            actions.push(action);
        }
    }

    actions
}

/// Parse engine actions from flat response attributes.
///
/// Looks for an attribute with key `ergors_action` whose value is the
/// action type string (e.g., "log"). Remaining attributes on the response
/// provide parameters. This enables a simpler single-attribute trigger
/// pattern on the `Response` itself (not via `.add_event()`).
pub fn parse_response_attributes(attributes: &[Attribute]) -> Vec<EngineAction> {
    let action_type = attributes
        .iter()
        .find(|a| a.key == ERGORS_ACTION_EVENT)
        .map(|a| a.value.clone());

    let Some(action_type) = action_type else {
        return Vec::new();
    };

    let mut synthetic_attrs: Vec<Attribute> = vec![Attribute {
        key: ATTR_TYPE.to_string(),
        value: action_type,
    }];

    for attr in attributes {
        if attr.key != ERGORS_ACTION_EVENT {
            synthetic_attrs.push(attr.clone());
        }
    }

    match parse_action_from_attributes(&synthetic_attrs) {
        Some(action) => vec![action],
        None => Vec::new(),
    }
}

/// Parse a single action from a set of attributes.
fn parse_action_from_attributes(attributes: &[Attribute]) -> Option<EngineAction> {
    let action_type = get_attr(attributes, ATTR_TYPE)?;

    match action_type.as_str() {
        "inference_request" => {
            let model = get_attr(attributes, "model")?;
            let prompt = get_attr(attributes, "prompt")?;
            let callback_contract = get_attr(attributes, "callback_contract");
            let callback_msg = get_attr(attributes, "callback_msg");
            Some(EngineAction::InferenceRequest {
                model,
                prompt,
                callback_contract,
                callback_msg,
            })
        }
        "store_put" => {
            let key = get_attr(attributes, "key")?;
            let value_b64 = get_attr(attributes, "value")?;
            let value = base64::Engine::decode(
                &base64::engine::general_purpose::STANDARD,
                &value_b64,
            )
            .ok()?;
            Some(EngineAction::StorePut { key, value })
        }
        "p2p_message" => {
            let target_node = get_attr(attributes, "target_node")?;
            let channel_str = get_attr(attributes, "channel").unwrap_or_else(|| "0".to_string());
            let channel = channel_str.parse::<u8>().unwrap_or(0);
            let payload_b64 = get_attr(attributes, "payload")?;
            let payload = base64::Engine::decode(
                &base64::engine::general_purpose::STANDARD,
                &payload_b64,
            )
            .ok()?;
            Some(EngineAction::P2pMessage {
                target_node,
                channel,
                payload,
            })
        }
        "log" => {
            let level = get_attr(attributes, "level").unwrap_or_else(|| "info".to_string());
            let message = get_attr(attributes, "message")?;
            Some(EngineAction::Log { level, message })
        }
        "akash_deploy" => {
            let sdl = get_attr(attributes, "sdl")?;
            let label = get_attr(attributes, "label")?;
            Some(EngineAction::AkashDeploy { sdl, label })
        }
        _ => None,
    }
}

/// Helper to extract an attribute value by key
fn get_attr(attributes: &[Attribute], key: &str) -> Option<String> {
    attributes
        .iter()
        .find(|a| a.key == key)
        .map(|a| a.value.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use cosmwasm_std::Event;

    #[test]
    fn test_parse_inference_request() {
        let events = vec![Event::new(ERGORS_ACTION_EVENT)
            .add_attribute("type", "inference_request")
            .add_attribute("model", "gpt-4")
            .add_attribute("prompt", "Hello world")
            .add_attribute("callback_contract", "ergors_abc123")];

        let actions = parse_engine_actions(&events);
        assert_eq!(actions.len(), 1);

        match &actions[0] {
            EngineAction::InferenceRequest {
                model,
                prompt,
                callback_contract,
                callback_msg,
            } => {
                assert_eq!(model, "gpt-4");
                assert_eq!(prompt, "Hello world");
                assert_eq!(callback_contract.as_deref(), Some("ergors_abc123"));
                assert!(callback_msg.is_none());
            }
            other => panic!("Expected InferenceRequest, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_log_action() {
        let events = vec![Event::new(ERGORS_ACTION_EVENT)
            .add_attribute("type", "log")
            .add_attribute("level", "warn")
            .add_attribute("message", "something happened")];

        let actions = parse_engine_actions(&events);
        assert_eq!(actions.len(), 1);

        match &actions[0] {
            EngineAction::Log { level, message } => {
                assert_eq!(level, "warn");
                assert_eq!(message, "something happened");
            }
            other => panic!("Expected Log, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_log_default_level() {
        let events = vec![Event::new(ERGORS_ACTION_EVENT)
            .add_attribute("type", "log")
            .add_attribute("message", "default level test")];

        let actions = parse_engine_actions(&events);
        assert_eq!(actions.len(), 1);

        match &actions[0] {
            EngineAction::Log { level, message } => {
                assert_eq!(level, "info");
                assert_eq!(message, "default level test");
            }
            other => panic!("Expected Log, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_store_put() {
        let value_b64 = base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            b"hello bytes",
        );

        let events = vec![Event::new(ERGORS_ACTION_EVENT)
            .add_attribute("type", "store_put")
            .add_attribute("key", "my/storage/key")
            .add_attribute("value", &value_b64)];

        let actions = parse_engine_actions(&events);
        assert_eq!(actions.len(), 1);

        match &actions[0] {
            EngineAction::StorePut { key, value } => {
                assert_eq!(key, "my/storage/key");
                assert_eq!(value, b"hello bytes");
            }
            other => panic!("Expected StorePut, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_p2p_message() {
        let payload_b64 = base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            b"raw payload",
        );

        let events = vec![Event::new(ERGORS_ACTION_EVENT)
            .add_attribute("type", "p2p_message")
            .add_attribute("target_node", "node_xyz")
            .add_attribute("channel", "4")
            .add_attribute("payload", &payload_b64)];

        let actions = parse_engine_actions(&events);
        assert_eq!(actions.len(), 1);

        match &actions[0] {
            EngineAction::P2pMessage {
                target_node,
                channel,
                payload,
            } => {
                assert_eq!(target_node, "node_xyz");
                assert_eq!(*channel, 4);
                assert_eq!(payload, b"raw payload");
            }
            other => panic!("Expected P2pMessage, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_akash_deploy() {
        let events = vec![Event::new(ERGORS_ACTION_EVENT)
            .add_attribute("type", "akash_deploy")
            .add_attribute("sdl", "version: \"2.0\"")
            .add_attribute("label", "my-deploy")];

        let actions = parse_engine_actions(&events);
        assert_eq!(actions.len(), 1);

        match &actions[0] {
            EngineAction::AkashDeploy { sdl, label } => {
                assert_eq!(sdl, "version: \"2.0\"");
                assert_eq!(label, "my-deploy");
            }
            other => panic!("Expected AkashDeploy, got {:?}", other),
        }
    }

    #[test]
    fn test_ignores_non_ergors_events() {
        let events = vec![
            Event::new("wasm")
                .add_attribute("action", "transfer")
                .add_attribute("amount", "1000"),
            Event::new("other_event").add_attribute("type", "inference_request"),
        ];

        let actions = parse_engine_actions(&events);
        assert!(actions.is_empty());
    }

    #[test]
    fn test_ignores_unknown_action_type() {
        let events = vec![Event::new(ERGORS_ACTION_EVENT)
            .add_attribute("type", "unknown_action_type")
            .add_attribute("data", "something")];

        let actions = parse_engine_actions(&events);
        assert!(actions.is_empty());
    }

    #[test]
    fn test_ignores_missing_required_fields() {
        let events = vec![Event::new(ERGORS_ACTION_EVENT)
            .add_attribute("type", "inference_request")
            .add_attribute("model", "gpt-4")];

        let actions = parse_engine_actions(&events);
        assert!(actions.is_empty());
    }

    #[test]
    fn test_multiple_actions_in_response() {
        let events = vec![
            Event::new(ERGORS_ACTION_EVENT)
                .add_attribute("type", "log")
                .add_attribute("message", "first log"),
            Event::new(ERGORS_ACTION_EVENT)
                .add_attribute("type", "log")
                .add_attribute("level", "error")
                .add_attribute("message", "second log"),
        ];

        let actions = parse_engine_actions(&events);
        assert_eq!(actions.len(), 2);
    }

    #[test]
    fn test_parse_response_attributes_log() {
        let attributes = vec![
            Attribute {
                key: ERGORS_ACTION_EVENT.to_string(),
                value: "log".to_string(),
            },
            Attribute {
                key: "level".to_string(),
                value: "debug".to_string(),
            },
            Attribute {
                key: "message".to_string(),
                value: "contract log via flat attrs".to_string(),
            },
        ];

        let actions = parse_response_attributes(&attributes);
        assert_eq!(actions.len(), 1);

        match &actions[0] {
            EngineAction::Log { level, message } => {
                assert_eq!(level, "debug");
                assert_eq!(message, "contract log via flat attrs");
            }
            other => panic!("Expected Log, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_response_attributes_no_ergors() {
        let attributes = vec![
            Attribute {
                key: "action".to_string(),
                value: "transfer".to_string(),
            },
            Attribute {
                key: "amount".to_string(),
                value: "1000".to_string(),
            },
        ];

        let actions = parse_response_attributes(&attributes);
        assert!(actions.is_empty());
    }

    #[test]
    fn test_invalid_base64_in_store_put_skipped() {
        let events = vec![Event::new(ERGORS_ACTION_EVENT)
            .add_attribute("type", "store_put")
            .add_attribute("key", "some_key")
            .add_attribute("value", "not-valid-base64!!!")];

        let actions = parse_engine_actions(&events);
        assert!(actions.is_empty());
    }

    #[test]
    fn test_inference_request_with_callback_msg() {
        let events = vec![Event::new(ERGORS_ACTION_EVENT)
            .add_attribute("type", "inference_request")
            .add_attribute("model", "llama-3")
            .add_attribute("prompt", "Summarize this")
            .add_attribute("callback_contract", "ergors_contract_1")
            .add_attribute("callback_msg", r#"{"receive_answer":{}}"#)];

        let actions = parse_engine_actions(&events);
        assert_eq!(actions.len(), 1);

        match &actions[0] {
            EngineAction::InferenceRequest {
                model,
                prompt,
                callback_contract,
                callback_msg,
            } => {
                assert_eq!(model, "llama-3");
                assert_eq!(prompt, "Summarize this");
                assert_eq!(callback_contract.as_deref(), Some("ergors_contract_1"));
                assert_eq!(
                    callback_msg.as_deref(),
                    Some(r#"{"receive_answer":{}}"#)
                );
            }
            other => panic!("Expected InferenceRequest, got {:?}", other),
        }
    }
}
