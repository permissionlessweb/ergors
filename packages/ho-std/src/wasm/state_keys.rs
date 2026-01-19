//! Storage key structure for WASM module
//!
//! Key Design:
//! - `wasm/config` -> WasmModuleState/Config
//! - `wasm/code/{code_id}` -> WasmCode (full code bytes)
//! - `wasm/code_info/{code_id}` -> CodeInfo (metadata)
//! - `wasm/code_hash/{hash}` -> code_id
//! - `wasm/contract/{address}` -> ContractInfo
//! - `wasm/contract_by_code/{code_id}/{idx}` -> contract_address
//! - `wasm/state/{contract_address}/{key}` -> contract state value
//! - `wasm/contract_config/{contract_address}` -> ContractConfig (retention policy, etc.)

use hex::encode as hex_encode;

pub const WASM_PREFIX: &str = "wasm/";
pub const CONFIG_KEY: &str = "wasm/config";
pub const CODE_PREFIX: &str = "wasm/code/";
pub const CODE_INFO_PREFIX: &str = "wasm/code_info/";
pub const CODE_HASH_PREFIX: &str = "wasm/code_hash/";
pub const CONTRACT_PREFIX: &str = "wasm/contract/";
pub const CONTRACT_BY_CODE_PREFIX: &str = "wasm/contract_by_code/";
pub const CONTRACT_STATE_PREFIX: &str = "wasm/state/";
pub const CONTRACT_CONFIG_PREFIX: &str = "wasm/contract_config/";

/// Get the key for WASM module configuration
pub fn wasm_config_key() -> String {
    CONFIG_KEY.to_string()
}

/// Get the key for stored WASM code bytes by code ID
pub fn wasm_code_key(code_id: u64) -> String {
    format!("{}{}", CODE_PREFIX, code_id)
}

/// Get the key for code metadata (CodeInfo) by code ID
pub fn wasm_code_info_key(code_id: u64) -> String {
    format!("{}{}", CODE_INFO_PREFIX, code_id)
}

/// Get the key for code ID lookup by hash
pub fn wasm_code_hash_key(hash: &[u8]) -> String {
    format!("{}{}", CODE_HASH_PREFIX, hex_encode(hash))
}

/// Get the key for contract info by address
pub fn wasm_contract_key(address: &str) -> String {
    format!("{}{}", CONTRACT_PREFIX, address)
}

/// Get the key for contract address lookup by code ID and index
pub fn wasm_contract_by_code_key(code_id: u64, idx: u64) -> String {
    format!("{}{}/{}", CONTRACT_BY_CODE_PREFIX, code_id, idx)
}

/// Get the prefix for all contracts of a specific code ID
pub fn wasm_contracts_by_code_prefix(code_id: u64) -> String {
    format!("{}{}/", CONTRACT_BY_CODE_PREFIX, code_id)
}

/// Get the key for contract state value
pub fn wasm_contract_state_key(contract_address: &str, state_key: &[u8]) -> String {
    format!(
        "{}{}/{}",
        CONTRACT_STATE_PREFIX,
        contract_address,
        hex_encode(state_key)
    )
}

/// Get the prefix for all state of a specific contract
pub fn wasm_contract_state_prefix(contract_address: &str) -> String {
    format!("{}{}/", CONTRACT_STATE_PREFIX, contract_address)
}

/// Get the key for contract configuration (retention policy, etc.)
pub fn wasm_contract_config_key(contract_address: &str) -> String {
    format!("{}{}", CONTRACT_CONFIG_PREFIX, contract_address)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_key_generation() {
        assert_eq!(wasm_config_key(), "wasm/config");
        assert_eq!(wasm_code_key(1), "wasm/code/1");
        assert_eq!(wasm_code_info_key(1), "wasm/code_info/1");
        assert_eq!(wasm_contract_key("ergors1abc"), "wasm/contract/ergors1abc");
        assert_eq!(
            wasm_contract_by_code_key(1, 0),
            "wasm/contract_by_code/1/0"
        );
        assert_eq!(
            wasm_contract_state_key("ergors1abc", b"balance"),
            "wasm/state/ergors1abc/62616c616e6365"
        );
        assert_eq!(
            wasm_contract_config_key("ergors1abc"),
            "wasm/contract_config/ergors1abc"
        );
    }

    #[test]
    fn test_prefix_generation() {
        assert_eq!(wasm_contracts_by_code_prefix(1), "wasm/contract_by_code/1/");
        assert_eq!(
            wasm_contract_state_prefix("ergors1abc"),
            "wasm/state/ergors1abc/"
        );
    }
}
