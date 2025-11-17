# WASM-VM Integration Implementation Plan for ERGORS Framework

Based on my analysis of your codebase, I've designed a comprehensive plan to integrate a WASM-VM layer with secure API key management into your existing ERGORS framework. This plan leverages your current architecture using `cnidarium` for JMT storage, proto3 type definitions, and trait-based design patterns.

## Executive Summary

This implementation will add:
1. **WASM-VM Runtime** using CosmWasm for smart contract execution
2. **Encrypted API Key Storage** in dedicated `cnidarium` tree branches
3. **Mesh Integration** allowing server routing to WASM contracts
4. **IBC-compatible** smart contract layer for cross-chain communication

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────┐
│                    ERGORS Server Layer                      │
│  (Existing: Axum Router, LlmRouter, Network Node)          │
└──────────────────┬──────────────────────────────────────────┘
                   │
         ┌─────────┴─────────┐
         │                   │
    ┌────▼────┐      ┌──────▼──────┐
    │   LLM   │      │  WASM-VM    │ ◄── New Layer
    │ Routing │      │   Router    │
    └────┬────┘      └──────┬──────┘
         │                  │
         │          ┌───────┴────────┐
         │          │                │
         │    ┌─────▼─────┐   ┌─────▼──────┐
         │    │  CosmWasm │   │   Smart    │
         │    │  Runtime  │   │  Contract  │
         │    │  (wasmvm) │   │  Manager   │
         │    └─────┬─────┘   └─────┬──────┘
         │          │               │
    ┌────▼──────────▼───────────────▼─────┐
    │      Cnidarium Storage (JMT)        │
    │  ┌──────────┬──────────┬──────────┐ │
    │  │   LLM    │  Encrypted│  WASM    │ │
    │  │ Configs  │   Keys    │  State   │ │
    │  └──────────┴──────────┴──────────┘ │
    └─────────────────────────────────────┘
```

## 1. Dependencies & Crate Additions

### 1.1 Update `packages/ho-std/Cargo.toml`

Add the following dependencies:

```toml
# WASM Runtime
cosmwasm-vm = "2.2.0"
wasmvm = "2.2.0"  # CosmWasm VM for contract execution

# Encryption
chacha20poly1305 = { workspace = true }
aes-gcm = "0.10"
argon2 = "0.5"

# Signature verification for node keys
ed25519-dalek = "2.1"

# Additional utilities
hex = { workspace = true }
```

### 1.2 Update workspace `Cargo.toml`

```toml
[workspace.dependencies]
cosmwasm-vm = "2.2.0"
wasmvm = "2.2.0"
aes-gcm = "0.10"
argon2 = "0.5"
ed25519-dalek = "2.1"
```

## 2. Proto3 Type Definitions

### 2.1 Create `proto/ergors/wasm/v1/wasm.proto`

```protobuf
syntax = "proto3";

package ergors.wasm.v1;

import "google/protobuf/timestamp.proto";
import "google/protobuf/any.proto";

// WASM Contract Management

message WasmCode {
  uint64 code_id = 1;
  bytes code_hash = 2;
  bytes creator = 3;
  google.protobuf.Timestamp created_at = 4;
  optional string source = 5;  // URL or reference
  optional string builder = 6;  // Builder version
}

message WasmContract {
  string address = 1;
  uint64 code_id = 2;
  bytes creator = 3;
  bytes admin = 4;
  string label = 5;
  google.protobuf.Timestamp created_at = 6;
}

message WasmConfig {
  // Max WASM binary size (default: 800KB)
  uint64 max_wasm_code_size = 1;
  // Max contract state size
  uint64 max_contract_state_size = 2;
  // Gas limit for instantiation
  uint64 instantiate_default_gas = 3;
  // Gas limit for execution
  uint64 execute_default_gas = 4;
  // Gas limit for queries
  uint64 query_default_gas = 5;
  // Memory limit in bytes
  uint64 memory_limit = 6;
}

// Encrypted API Key Storage

message EncryptedApiKey {
  string provider_name = 1;
  bytes encrypted_data = 2;  // ChaCha20-Poly1305 encrypted
  bytes nonce = 3;           // 12 bytes nonce
  bytes salt = 4;            // For key derivation
  google.protobuf.Timestamp created_at = 5;
  google.protobuf.Timestamp updated_at = 6;
  uint32 version = 7;        // For key rotation
}

message NodeEncryptionKey {
  bytes public_key = 1;      // Ed25519 public key
  bytes private_key_encrypted = 2;  // Encrypted with password
  bytes key_salt = 3;
  google.protobuf.Timestamp created_at = 4;
}

// WASM Execution Messages

message StoreCodeRequest {
  bytes wasm_byte_code = 1;
  bytes sender = 2;
  optional string source = 3;
  optional string builder = 4;
}

message StoreCodeResponse {
  uint64 code_id = 1;
  bytes checksum = 2;
}

message InstantiateContractRequest {
  bytes sender = 2;
  bytes admin = 3;
  uint64 code_id = 1;
  string label = 4;
  bytes msg = 5;  // JSON instantiate message
  repeated Coin funds = 6;
  optional uint64 gas_limit = 7;
}

message InstantiateContractResponse {
  string address = 1;
  bytes data = 2;
}

message ExecuteContractRequest {
  bytes sender = 1;
  string contract_address = 2;
  bytes msg = 3;  // JSON execute message
  repeated Coin funds = 4;
  optional uint64 gas_limit = 5;
}

message ExecuteContractResponse {
  bytes data = 1;
  repeated ContractEvent events = 2;
}

message QueryContractRequest {
  string contract_address = 1;
  bytes msg = 2;  // JSON query message
}

message QueryContractResponse {
  bytes data = 1;
}

message MigrateContractRequest {
  bytes sender = 1;
  string contract_address = 2;
  uint64 new_code_id = 3;
  bytes msg = 4;  // JSON migrate message
}

message MigrateContractResponse {
  bytes data = 1;
}

// Supporting Types

message Coin {
  string denom = 1;
  string amount = 2;
}

message ContractEvent {
  string type = 1;
  repeated EventAttribute attributes = 2;
}

message EventAttribute {
  string key = 1;
  string value = 2;
}

// API Key Management

message EncryptApiKeyRequest {
  string provider_name = 1;
  string api_key = 2;  // Plaintext key to encrypt
}

message EncryptApiKeyResponse {
  bool success = 1;
  optional string error = 2;
}

message DecryptApiKeyRequest {
  string provider_name = 1;
}

message DecryptApiKeyResponse {
  string api_key = 1;
}

message ListEncryptedKeysRequest {}

message ListEncryptedKeysResponse {
  repeated string provider_names = 1;
}
```

### 2.2 Create `proto/ergors/wasm/v1/state.proto`

```protobuf
syntax = "proto3";

package ergors.wasm.v1;

// Storage layout for WASM module in JMT

message WasmModuleState {
  uint64 next_code_id = 1;
  uint64 total_contracts = 2;
  WasmConfig config = 3;
}

message CodeInfo {
  uint64 code_id = 1;
  bytes code_hash = 2;
  bytes creator = 3;
  uint64 instantiate_count = 4;
}

message ContractInfo {
  string address = 1;
  uint64 code_id = 2;
  bytes creator = 3;
  bytes admin = 4;
  string label = 5;
}
```

## 3. Storage Layer Implementation

### 3.1 Storage Key Design

Create `packages/ho-std/src/wasm/state_keys.rs`:

```rust
//! Storage key structure for WASM module
//!
//! Key Design:
//! - `wasm/config` -> WasmConfig
//! - `wasm/code/{code_id}` -> WasmCode
//! - `wasm/code_hash/{hash}` -> code_id
//! - `wasm/contract/{address}` -> WasmContract
//! - `wasm/contract_by_code/{code_id}/{idx}` -> contract_address
//! - `wasm/state/{contract_address}/{key}` -> contract state
//! - `encrypted_keys/{provider_name}` -> EncryptedApiKey
//! - `encryption/node_key` -> NodeEncryptionKey

use std::fmt::Display;

pub fn wasm_config_key() -> String {
    "wasm/config".to_string()
}

pub fn wasm_code_key(code_id: u64) -> String {
    format!("wasm/code/{}", code_id)
}

pub fn wasm_code_hash_key(hash: &[u8]) -> String {
    format!("wasm/code_hash/{}", hex::encode(hash))
}

pub fn wasm_contract_key(address: &str) -> String {
    format!("wasm/contract/{}", address)
}

pub fn wasm_contract_by_code_key(code_id: u64, idx: u64) -> String {
    format!("wasm/contract_by_code/{}/{}", code_id, idx)
}

pub fn wasm_contract_state_key(address: &str, key: &[u8]) -> String {
    format!("wasm/state/{}/{}", address, hex::encode(key))
}

pub fn encrypted_api_key_key(provider_name: &str) -> String {
    format!("encrypted_keys/{}", provider_name)
}

pub fn node_encryption_key() -> String {
    "encryption/node_key".to_string()
}

pub fn encrypted_keys_prefix() -> String {
    "encrypted_keys/".to_string()
}
```

### 3.2 State Extension Traits

Create `packages/ho-std/src/wasm/state_ext.rs`:

```rust
use crate::error::{HoError, HoResult};
use crate::traits::{StateWrite};
use crate::types::ergors::wasm::v1::*;
use async_trait::async_trait;
use cnidarium::StateRead;
use prost::Message;

mod state_keys;
pub use state_keys::*;

/// Extension trait for reading WASM state from storage
#[async_trait]
pub trait WasmStateReadExt: StateRead {
    /// Get WASM module configuration
    async fn get_wasm_config(&self) -> HoResult<Option<WasmConfig>> {
        let key = wasm_config_key();
        self.get_proto(&key).await
    }

    /// Get WASM code by ID
    async fn get_wasm_code(&self, code_id: u64) -> HoResult<Option<WasmCode>> {
        let key = wasm_code_key(code_id);
        self.get_proto(&key).await
    }

    /// Get code ID by hash
    async fn get_code_id_by_hash(&self, hash: &[u8]) -> HoResult<Option<u64>> {
        let key = wasm_code_hash_key(hash);
        match self.get_raw(&key).await? {
            Some(bytes) => {
                let id = u64::from_le_bytes(bytes.try_into().map_err(|_| {
                    HoError::Storage("Invalid code_id bytes".to_string())
                })?);
                Ok(Some(id))
            }
            None => Ok(None),
        }
    }

    /// Get contract info by address
    async fn get_wasm_contract(&self, address: &str) -> HoResult<Option<WasmContract>> {
        let key = wasm_contract_key(address);
        self.get_proto(&key).await
    }

    /// Get contract state value
    async fn get_contract_state(&self, address: &str, state_key: &[u8]) -> HoResult<Option<Vec<u8>>> {
        let key = wasm_contract_state_key(address, state_key);
        self.get_raw(&key).await
    }

    /// Get encrypted API key
    async fn get_encrypted_api_key(&self, provider: &str) -> HoResult<Option<EncryptedApiKey>> {
        let key = encrypted_api_key_key(provider);
        self.get_proto(&key).await
    }

    /// Get node encryption key
    async fn get_node_encryption_key(&self) -> HoResult<Option<NodeEncryptionKey>> {
        let key = node_encryption_key();
        self.get_proto(&key).await
    }

    /// List all encrypted key provider names
    async fn list_encrypted_key_providers(&self) -> HoResult<Vec<String>> {
        let prefix = encrypted_keys_prefix();
        let mut providers = Vec::new();

        self.prefix_keys(&prefix)
            .await?
            .into_iter()
            .for_each(|key| {
                if let Some(name) = key.strip_prefix(&prefix) {
                    providers.push(name.to_string());
                }
            });

        Ok(providers)
    }
}

impl<T: StateRead + ?Sized> WasmStateReadExt for T {}

/// Extension trait for writing WASM state to storage
pub trait WasmStateWriteExt: StateWrite {
    /// Put WASM module configuration
    fn put_wasm_config(&mut self, config: &WasmConfig) {
        let key = wasm_config_key();
        self.put_proto(key, config.clone());
    }

    /// Put WASM code
    fn put_wasm_code(&mut self, code: &WasmCode) {
        let key = wasm_code_key(code.code_id);
        self.put_proto(key, code.clone());

        // Also store hash -> code_id mapping
        let hash_key = wasm_code_hash_key(&code.code_hash);
        self.put_raw(hash_key, code.code_id.to_le_bytes().to_vec());
    }

    /// Put WASM contract
    fn put_wasm_contract(&mut self, contract: &WasmContract) {
        let key = wasm_contract_key(&contract.address);
        self.put_proto(key, contract.clone());
    }

    /// Put contract state value
    fn put_contract_state(&mut self, address: &str, state_key: &[u8], value: Vec<u8>) {
        let key = wasm_contract_state_key(address, state_key);
        self.put_raw(key, value);
    }

    /// Delete contract state value
    fn delete_contract_state(&mut self, address: &str, state_key: &[u8]) {
        let key = wasm_contract_state_key(address, state_key);
        self.delete(key);
    }

    /// Put encrypted API key
    fn put_encrypted_api_key(&mut self, key_data: &EncryptedApiKey) {
        let key = encrypted_api_key_key(&key_data.provider_name);
        self.put_proto(key, key_data.clone());
    }

    /// Delete encrypted API key
    fn delete_encrypted_api_key(&mut self, provider: &str) {
        let key = encrypted_api_key_key(provider);
        self.delete(key);
    }

    /// Put node encryption key
    fn put_node_encryption_key(&mut self, node_key: &NodeEncryptionKey) {
        let key = node_encryption_key();
        self.put_proto(key, node_key.clone());
    }
}

impl<T: StateWrite + ?Sized> WasmStateWriteExt for T {}

// Helper trait for proto encoding/decoding
#[async_trait]
trait ProtoExt: StateRead {
    async fn get_proto<T: Message + Default>(&self, key: &str) -> HoResult<Option<T>> {
        match self.get_raw(key).await? {
            Some(bytes) => {
                let msg = T::decode(&*bytes).map_err(|e| {
                    HoError::Storage(format!("Failed to decode proto: {}", e))
                })?;
                Ok(Some(msg))
            }
            None => Ok(None),
        }
    }
}

impl<T: StateRead + ?Sized> ProtoExt for T {}

trait ProtoWriteExt: StateWrite {
    fn put_proto<T: Message>(&mut self, key: String, value: T) {
        let mut buf = Vec::new();
        value.encode(&mut buf).expect("proto encoding should not fail");
        self.put_raw(key, buf);
    }
}

impl<T: StateWrite + ?Sized> ProtoWriteExt for T {}
```

## 4. Encryption Module

Create `packages/ho-std/src/crypto/encryption.rs`:

```rust
use crate::error::{HoError, HoResult};
use crate::types::ergors::wasm::v1::{EncryptedApiKey, NodeEncryptionKey};
use chacha20poly1305::{
    aead::{Aead, KeyInit, OsRng},
    ChaCha20Poly1305, Nonce,
};
use ed25519_consensus::{SigningKey, VerificationKey};
use rand::RngCore;
use sha2::{Digest, Sha256};
use std::time::SystemTime;

pub struct KeyManager {
    node_signing_key: SigningKey,
    node_verification_key: VerificationKey,
}

impl KeyManager {
    /// Create new key manager from node signing key
    pub fn new(node_signing_key: SigningKey) -> Self {
        let node_verification_key = node_signing_key.verification_key();
        Self {
            node_signing_key,
            node_verification_key,
        }
    }

    /// Derive encryption key from node signing key and salt
    fn derive_encryption_key(&self, salt: &[u8]) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(self.node_signing_key.as_bytes());
        hasher.update(salt);
        hasher.finalize().into()
    }

    /// Encrypt API key with node public key
    pub fn encrypt_api_key(&self, provider_name: &str, api_key: &str) -> HoResult<EncryptedApiKey> {
        // Generate random salt and nonce
        let mut salt = [0u8; 32];
        let mut nonce_bytes = [0u8; 12];
        OsRng.fill_bytes(&mut salt);
        OsRng.fill_bytes(&mut nonce_bytes);

        // Derive encryption key
        let encryption_key = self.derive_encryption_key(&salt);
        let cipher = ChaCha20Poly1305::new(&encryption_key.into());
        let nonce = Nonce::from_slice(&nonce_bytes);

        // Encrypt the API key
        let ciphertext = cipher
            .encrypt(nonce, api_key.as_bytes())
            .map_err(|e| HoError::Crypto(format!("Encryption failed: {}", e)))?;

        let timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap();

        Ok(EncryptedApiKey {
            provider_name: provider_name.to_string(),
            encrypted_data: ciphertext,
            nonce: nonce_bytes.to_vec(),
            salt: salt.to_vec(),
            created_at: Some(pbjson_types::Timestamp {
                seconds: timestamp.as_secs() as i64,
                nanos: timestamp.subsec_nanos() as i32,
            }),
            updated_at: Some(pbjson_types::Timestamp {
                seconds: timestamp.as_secs() as i64,
                nanos: timestamp.subsec_nanos() as i32,
            }),
            version: 1,
        })
    }

    /// Decrypt API key with node private key
    pub fn decrypt_api_key(&self, encrypted_key: &EncryptedApiKey) -> HoResult<String> {
        // Derive encryption key using stored salt
        let encryption_key = self.derive_encryption_key(&encrypted_key.salt);
        let cipher = ChaCha20Poly1305::new(&encryption_key.into());

        let nonce = Nonce::from_slice(&encrypted_key.nonce);

        // Decrypt the API key
        let plaintext = cipher
            .decrypt(nonce, encrypted_key.encrypted_data.as_ref())
            .map_err(|e| HoError::Crypto(format!("Decryption failed: {}", e)))?;

        String::from_utf8(plaintext)
            .map_err(|e| HoError::Crypto(format!("Invalid UTF-8 in decrypted key: {}", e)))
    }

    /// Get node public key bytes
    pub fn public_key_bytes(&self) -> &[u8; 32] {
        self.node_verification_key.as_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let signing_key = SigningKey::new(OsRng);
        let manager = KeyManager::new(signing_key);

        let provider = "openai";
        let api_key = "sk-test123456789";

        let encrypted = manager.encrypt_api_key(provider, api_key).unwrap();
        let decrypted = manager.decrypt_api_key(&encrypted).unwrap();

        assert_eq!(api_key, decrypted);
        assert_eq!(provider, encrypted.provider_name);
    }
}
```

## 5. WASM Runtime Integration

Create `packages/ho-std/src/wasm/runtime.rs`:

```rust
use crate::error::{HoError, HoResult};
use crate::traits::StateWrite;
use crate::types::ergors::wasm::v1::*;
use crate::wasm::state_ext::{WasmStateReadExt, WasmStateWriteExt};
use cnidarium::StateRead;
use cosmwasm_std::{Addr, Binary, ContractResult, Response};
use cosmwasm_vm::{
    capabilities_from_csv, call_execute, call_instantiate, call_query,
    Backend, BackendApi, Cache, Instance, InstanceOptions, Storage,
};
use std::sync::Arc;
use tracing::{debug, info};

/// WASM VM Runtime for executing CosmWasm contracts
pub struct WasmRuntime {
    cache: Arc<Cache<Backend>>,
    config: WasmConfig,
}

impl WasmRuntime {
    /// Create new WASM runtime
    pub fn new(config: WasmConfig, cache_dir: impl Into<std::path::PathBuf>) -> HoResult<Self> {
        let cache = unsafe {
            Cache::new(cache_dir)
                .map_err(|e| HoError::Wasm(format!("Failed to create WASM cache: {}", e)))?
        };

        Ok(Self {
            cache: Arc::new(cache),
            config,
        })
    }

    /// Store WASM code
    pub async fn store_code<S: StateRead + StateWrite>(
        &self,
        state: &mut S,
        wasm_code: Vec<u8>,
        sender: Vec<u8>,
        source: Option<String>,
        builder: Option<String>,
    ) -> HoResult<StoreCodeResponse> {
        // Validate code size
        if wasm_code.len() as u64 > self.config.max_wasm_code_size {
            return Err(HoError::Wasm(format!(
                "WASM code size {} exceeds limit {}",
                wasm_code.len(),
                self.config.max_wasm_code_size
            )));
        }

        // Compute code hash
        let code_hash = sha2::Sha256::digest(&wasm_code).to_vec();

        // Check if code already exists
        if let Some(existing_id) = state.get_code_id_by_hash(&code_hash).await? {
            return Ok(StoreCodeResponse {
                code_id: existing_id,
                checksum: code_hash,
            });
        }

        // Get next code ID
        let config = state
            .get_wasm_config()
            .await?
            .unwrap_or_else(|| self.config.clone());
        let code_id = config.next_code_id.unwrap_or(1);

        // Validate WASM code by trying to compile it
        self.cache
            .save_wasm(&wasm_code)
            .map_err(|e| HoError::Wasm(format!("Invalid WASM code: {}", e)))?;

        // Store code
        let wasm_code_obj = WasmCode {
            code_id,
            code_hash: code_hash.clone(),
            creator: sender,
            created_at: Some(current_timestamp()),
            source,
            builder,
        };

        state.put_wasm_code(&wasm_code_obj);

        // Update config with next code ID
        let mut new_config = config;
        new_config.next_code_id = Some(code_id + 1);
        state.put_wasm_config(&new_config);

        info!("Stored WASM code with ID: {}", code_id);

        Ok(StoreCodeResponse {
            code_id,
            checksum: code_hash,
        })
    }

    /// Instantiate a contract
    pub async fn instantiate_contract<S: StateRead + StateWrite>(
        &self,
        state: &mut S,
        request: InstantiateContractRequest,
    ) -> HoResult<InstantiateContractResponse> {
        // Get code
        let code = state
            .get_wasm_code(request.code_id)
            .await?
            .ok_or_else(|| HoError::Wasm(format!("Code ID {} not found", request.code_id)))?;

        // Generate contract address (simplified - in production use proper address derivation)
        let address = self.generate_contract_address(request.code_id, &request.sender, &request.label);

        // Create contract info
        let contract = WasmContract {
            address: address.clone(),
            code_id: request.code_id,
            creator: request.sender.clone(),
            admin: request.admin,
            label: request.label,
            created_at: Some(current_timestamp()),
        };

        state.put_wasm_contract(&contract);

        // Execute instantiate message
        // Note: This is simplified. Full implementation would create proper CosmWasm environment
        let response_data = vec![]; // Placeholder for actual instantiate response

        info!("Instantiated contract at address: {}", address);

        Ok(InstantiateContractResponse {
            address,
            data: response_data,
        })
    }

    /// Execute a contract
    pub async fn execute_contract<S: StateRead + StateWrite>(
        &self,
        state: &mut S,
        request: ExecuteContractRequest,
    ) -> HoResult<ExecuteContractResponse> {
        // Get contract info
        let contract = state
            .get_wasm_contract(&request.contract_address)
            .await?
            .ok_or_else(|| {
                HoError::Wasm(format!("Contract {} not found", request.contract_address))
            })?;

        // Get code
        let code = state
            .get_wasm_code(contract.code_id)
            .await?
            .ok_or_else(|| HoError::Wasm(format!("Code ID {} not found", contract.code_id)))?;

        // Execute contract (simplified - full implementation would use CosmWasm VM)
        debug!(
            "Executing contract {} with message: {:?}",
            request.contract_address,
            String::from_utf8_lossy(&request.msg)
        );

        // Placeholder response
        Ok(ExecuteContractResponse {
            data: vec![],
            events: vec![],
        })
    }

    /// Query a contract
    pub async fn query_contract<S: StateRead>(
        &self,
        state: &S,
        request: QueryContractRequest,
    ) -> HoResult<QueryContractResponse> {
        // Get contract info
        let contract = state
            .get_wasm_contract(&request.contract_address)
            .await?
            .ok_or_else(|| {
                HoError::Wasm(format!("Contract {} not found", request.contract_address))
            })?;

        debug!(
            "Querying contract {} with message: {:?}",
            request.contract_address,
            String::from_utf8_lossy(&request.msg)
        );

        // Placeholder query response
        Ok(QueryContractResponse { data: vec![] })
    }

    /// Generate deterministic contract address
    fn generate_contract_address(&self, code_id: u64, creator: &[u8], label: &str) -> String {
        use sha2::Digest;
        let mut hasher = sha2::Sha256::new();
        hasher.update(code_id.to_le_bytes());
        hasher.update(creator);
        hasher.update(label.as_bytes());
        let hash = hasher.finalize();
        format!("ergors{}", hex::encode(&hash[..20]))
    }
}

fn current_timestamp() -> pbjson_types::Timestamp {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap();
    pbjson_types::Timestamp {
        seconds: now.as_secs() as i64,
        nanos: now.subsec_nanos() as i32,
    }
}
```

## 6. Server Integration & Routing

Create `packages/ho-std/src/wasm/router.rs`:

```rust
use crate::crypto::KeyManager;
use crate::error::{HoError, HoResult};
use crate::traits::StateWrite;
use crate::types::ergors::wasm::v1::*;
use crate::wasm::runtime::WasmRuntime;
use crate::wasm::state_ext::{WasmStateReadExt, WasmStateWriteExt};
use cnidarium::StateRead;
use std::sync::Arc;
use tracing::info;

/// Router for WASM operations - integrates with existing server
pub struct WasmRouter {
    runtime: Arc<WasmRuntime>,
    key_manager: Arc<KeyManager>,
}

impl WasmRouter {
    pub fn new(runtime: WasmRuntime, key_manager: KeyManager) -> Self {
        Self {
            runtime: Arc::new(runtime),
            key_manager: Arc::new(key_manager),
        }
    }

    /// Store WASM code
    pub async fn handle_store_code<S: StateRead + StateWrite>(
        &self,
        state: &mut S,
        request: StoreCodeRequest,
    ) -> HoResult<StoreCodeResponse> {
        self.runtime
            .store_code(
                state,
                request.wasm_byte_code,
                request.sender,
                request.source,
                request.builder,
            )
            .await
    }

    /// Instantiate contract
    pub async fn handle_instantiate<S: StateRead + StateWrite>(
        &self,
        state: &mut S,
        request: InstantiateContractRequest,
    ) -> HoResult<InstantiateContractResponse> {
        self.runtime.instantiate_contract(state, request).await
    }

    /// Execute contract
    pub async fn handle_execute<S: StateRead + StateWrite>(
        &self,
        state: &mut S,
        request: ExecuteContractRequest,
    ) -> HoResult<ExecuteContractResponse> {
        self.runtime.execute_contract(state, request).await
    }

    /// Query contract
    pub async fn handle_query<S: StateRead>(
        &self,
        state: &S,
        request: QueryContractRequest,
    ) -> HoResult<QueryContractResponse> {
        self.runtime.query_contract(state, request).await
    }

    /// Encrypt and store API key
    pub async fn handle_encrypt_key<S: StateWrite>(
        &self,
        state: &mut S,
        request: EncryptApiKeyRequest,
    ) -> HoResult<EncryptApiKeyResponse> {
        let encrypted = self
            .key_manager
            .encrypt_api_key(&request.provider_name, &request.api_key)?;

        state.put_encrypted_api_key(&encrypted);

        info!("Encrypted and stored API key for provider: {}", request.provider_name);

        Ok(EncryptApiKeyResponse {
            success: true,
            error: None,
        })
    }

    /// Decrypt API key
    pub async fn handle_decrypt_key<S: StateRead>(
        &self,
        state: &S,
        request: DecryptApiKeyRequest,
    ) -> HoResult<DecryptApiKeyResponse> {
        let encrypted = state
            .get_encrypted_api_key(&request.provider_name)
            .await?
            .ok_or_else(|| {
                HoError::Storage(format!("No encrypted key found for provider: {}", request.provider_name))
            })?;

        let api_key = self.key_manager.decrypt_api_key(&encrypted)?;

        Ok(DecryptApiKeyResponse { api_key })
    }

    /// List encrypted keys
    pub async fn handle_list_keys<S: StateRead>(
        &self,
        state: &S,
        _request: ListEncryptedKeysRequest,
    ) -> HoResult<ListEncryptedKeysResponse> {
        let provider_names = state.list_encrypted_key_providers().await?;

        Ok(ListEncryptedKeysResponse { provider_names })
    }
}
```

## 7. Integration with Existing LLM Router

Modify `packages/ho-std/src/llm/router.rs` to support encrypted key loading:

```rust
// Add to LlmRouter implementation
impl LlmRouter {
    /// Load API key for provider from encrypted storage
    pub async fn load_provider_key<S: StateRead>(
        &self,
        state: &S,
        provider_name: &str,
        key_manager: &KeyManager,
    ) -> HoResult<Option<String>> {
        use crate::wasm::state_ext::WasmStateReadExt;

        if let Some(encrypted) = state.get_encrypted_api_key(provider_name).await? {
            let key = key_manager.decrypt_api_key(&encrypted)?;
            return Ok(Some(key));
        }

        Ok(None)
    }

    /// Enhanced request handler with automatic key injection
    pub async fn handle_request_with_auth<S: StateRead>(
        &self,
        state: &S,
        request: &PromptRequest,
        model: &str,
        key_manager: &KeyManager,
    ) -> HoResult<PromptResponse> {
        // Find provider
        let provider = self.find_provider_for_model(model).ok_or_else(|| {
            HoError::Llm(format!("No provider found for model: {}", model))
        })?;

        // Load decrypted API key from storage
        let api_key = self.load_provider_key(state, provider.name(), key_manager).await?;

        // Call provider with decrypted key
        // Modify the client to inject Authorization header
        let mut headers = reqwest::header::HeaderMap::new();
        if let Some(key) = api_key {
            headers.insert(
                reqwest::header::AUTHORIZATION,
                format!("Bearer {}", key).parse().unwrap(),
            );
        }

        provider.call(&self.client, request).await
    }
}
```

## 8. Server Routes Addition

Add WASM routes to your Axum server in `packages/ergors/src/server.rs` (or equivalent):

```rust
use axum::{
    extract::State,
    routing::{get, post},
    Json, Router,
};
use ho_std::wasm::router::WasmRouter;
use ho_std::types::ergors::wasm::v1::*;

pub fn wasm_routes(wasm_router: Arc<WasmRouter>) -> Router {
    Router::new()
        .route("/wasm/store_code", post(store_code))
        .route("/wasm/instantiate", post(instantiate))
        .route("/wasm/execute", post(execute))
        .route("/wasm/query", post(query))
        .route("/keys/encrypt", post(encrypt_key))
        .route("/keys/decrypt", post(decrypt_key))
        .route("/keys/list", get(list_keys))
        .with_state(wasm_router)
}

async fn store_code(
    State(router): State<Arc<WasmRouter>>,
    State(mut state): State<AppState>,
    Json(request): Json<StoreCodeRequest>,
) -> Result<Json<StoreCodeResponse>, AppError> {
    let response = router.handle_store_code(&mut state, request).await?;
    Ok(Json(response))
}

async fn instantiate(
    State(router): State<Arc<WasmRouter>>,
    State(mut state): State<AppState>,
    Json(request): Json<InstantiateContractRequest>,
) -> Result<Json<InstantiateContractResponse>, AppError> {
    let response = router.handle_instantiate(&mut state, request).await?;
    Ok(Json(response))
}

async fn execute(
    State(router): State<Arc<WasmRouter>>,
    State(mut state): State<AppState>,
    Json(request): Json<ExecuteContractRequest>,
) -> Result<Json<ExecuteContractResponse>, AppError> {
    let response = router.handle_execute(&mut state, request).await?;
    Ok(Json(response))
}

async fn query(
    State(router): State<Arc<WasmRouter>>,
    State(state): State<AppState>,
    Json(request): Json<QueryContractRequest>,
) -> Result<Json<QueryContractResponse>, AppError> {
    let response = router.handle_query(&state, request).await?;
    Ok(Json(response))
}

async fn encrypt_key(
    State(router): State<Arc<WasmRouter>>,
    State(mut state): State<AppState>,
    Json(request): Json<EncryptApiKeyRequest>,
) -> Result<Json<EncryptApiKeyResponse>, AppError> {
    let response = router.handle_encrypt_key(&mut state, request).await?;
    Ok(Json(response))
}

async fn decrypt_key(
    State(router): State<Arc<WasmRouter>>,
    State(state): State<AppState>,
    Json(request): Json<DecryptApiKeyRequest>,
) -> Result<Json<DecryptApiKeyResponse>, AppError> {
    let response = router.handle_decrypt_key(&state, request).await?;
    Ok(Json(response))
}

async fn list_keys(
    State(router): State<Arc<WasmRouter>>,
    State(state): State<AppState>,
) -> Result<Json<ListEncryptedKeysResponse>, AppError> {
    let request = ListEncryptedKeysRequest {};
    let response = router.handle_list_keys(&state, request).await?;
    Ok(Json(response))
}
```

## 9. Initialization & Startup

Create initialization logic in `packages/ho-std/src/wasm/init.rs`:

```rust
use crate::crypto::KeyManager;
use crate::error::HoResult;
use crate::traits::StateWrite;
use crate::types::ergors::wasm::v1::*;
use crate::wasm::runtime::WasmRuntime;
use crate::wasm::state_ext::{WasmStateReadExt, WasmStateWriteExt};
use cnidarium::StateRead;
use ed25519_consensus::SigningKey;
use std::path::PathBuf;
use tracing::info;

/// Initialize WASM module on server startup
pub async fn initialize_wasm_module<S: StateRead + StateWrite>(
    state: &mut S,
    data_dir: PathBuf,
    node_key: SigningKey,
) -> HoResult<(WasmRuntime, KeyManager)> {
    // Create default config if not exists
    if state.get_wasm_config().await?.is_none() {
        let default_config = WasmConfig {
            max_wasm_code_size: 800 * 1024, // 800KB
            max_contract_state_size: 10 * 1024 * 1024, // 10MB
            instantiate_default_gas: 100_000_000,
            execute_default_gas: 50_000_000,
            query_default_gas: 10_000_000,
            memory_limit: 32 * 1024 * 1024, // 32MB
        };
        state.put_wasm_config(&default_config);
        info!("Initialized default WASM config");
    }

    let config = state.get_wasm_config().await?.unwrap();

    // Initialize WASM cache directory
    let cache_dir = data_dir.join("wasm_cache");
    std::fs::create_dir_all(&cache_dir)?;

    // Create WASM runtime
    let runtime = WasmRuntime::new(config, cache_dir)?;

    // Create key manager
    let key_manager = KeyManager::new(node_key);

    info!("WASM module initialized successfully");

    Ok((runtime, key_manager))
}

/// Migrate API keys from plain JSON to encrypted storage
pub async fn migrate_api_keys<S: StateWrite>(
    state: &mut S,
    api_keys_file: &PathBuf,
    key_manager: &KeyManager,
) -> HoResult<()> {
    use crate::config::api_keys::ApiKeysJson;

    if !api_keys_file.exists() {
        info!("No API keys file to migrate");
        return Ok(());
    }

    let api_keys = ApiKeysJson::load(&api_keys_file.try_into().unwrap())?;

    for (provider_name, provider_config) in api_keys.providers {
        if let Some(api_key) = provider_config.api_key {
            // Skip environment variable references
            if api_key.starts_with("${") && api_key.ends_with("}") {
                continue;
            }

            // Encrypt and store
            let encrypted = key_manager.encrypt_api_key(&provider_name, &api_key)?;
            state.put_encrypted_api_key(&encrypted);

            info!("Migrated API key for provider: {}", provider_name);
        }
    }

    info!("API key migration complete");

    Ok(())
}
```

## 10. Error Handling

Add to `packages/ho-std/src/error.rs`:

```rust
#[derive(Debug, thiserror::Error)]
pub enum HoError {
    // ... existing variants ...

    #[error("WASM error: {0}")]
    Wasm(String),

    #[error("Cryptography error: {0}")]
    Crypto(String),
}
```

## 11. Module Organization

Create `packages/ho-std/src/wasm/mod.rs`:

```rust
//! WASM VM integration module
//!
//! Provides CosmWasm smart contract execution capabilities with:
//! - Contract upload, instantiation, execution, and queries
//! - Encrypted API key storage using node keys
//! - Integration with cnidarium verifiable storage

pub mod init;
pub mod router;
pub mod runtime;
pub mod state_ext;

pub use router::WasmRouter;
pub use runtime::WasmRuntime;
```

Create `packages/ho-std/src/crypto/mod.rs`:

```rust
//! Cryptography module for key management and encryption

mod encryption;

pub use encryption::KeyManager;
```

Update `packages/ho-std/src/lib.rs`:

```rust
pub mod crypto;
pub mod wasm;

// ... existing modules ...
```

## 12. Testing Strategy

### 12.1 Unit Tests

Create `packages/ho-std/src/wasm/runtime_tests.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_store_and_retrieve_code() {
        let temp_dir = TempDir::new().unwrap();
        let config = WasmConfig::default();
        let runtime = WasmRuntime::new(config, temp_dir.path()).unwrap();

        // Create minimal WASM module
        let wasm_code = include_bytes!("../../../test_data/sample_contract.wasm").to_vec();
        let sender = vec![1, 2, 3, 4];

        let mut state = TestState::new();
        let response = runtime
            .store_code(&mut state, wasm_code, sender, None, None)
            .await
            .unwrap();

        assert_eq!(response.code_id, 1);
    }

    #[tokio::test]
    async fn test_encrypt_decrypt_api_key() {
        let signing_key = SigningKey::new(OsRng);
        let key_manager = KeyManager::new(signing_key);

        let encrypted = key_manager
            .encrypt_api_key("openai", "sk-test123")
            .unwrap();

        let decrypted = key_manager.decrypt_api_key(&encrypted).unwrap();

        assert_eq!(decrypted, "sk-test123");
    }
}
```

### 12.2 Integration Tests

Create `packages/ho-std/tests/wasm_integration_test.rs`:

```rust
use ho_std::wasm::*;
use tempfile::TempDir;

#[tokio::test]
async fn test_full_contract_lifecycle() {
    // 1. Initialize runtime
    // 2. Store code
    // 3. Instantiate contract
    // 4. Execute contract
    // 5. Query contract
    // 6. Verify state changes
}

#[tokio::test]
async fn test_api_key_workflow() {
    // 1. Initialize key manager
    // 2. Load API keys from file
    // 3. Encrypt and store
    // 4. Decrypt and use in LLM request
    // 5. Verify successful authorization
}
```

## 13. Security Considerations

### 13.1 Key Rotation

Implement key rotation support:

```rust
impl KeyManager {
    pub fn rotate_api_key(&self, old_encrypted: &EncryptedApiKey, new_api_key: &str) -> HoResult<EncryptedApiKey> {
        let mut new_encrypted = self.encrypt_api_key(&old_encrypted.provider_name, new_api_key)?;
        new_encrypted.version = old_encrypted.version + 1;
        Ok(new_encrypted)
    }
}
```

### 13.2 Access Control

Add permission checks in WASM router:

```rust
pub struct WasmPermissions {
    allowed_uploaders: Vec<Vec<u8>>,
    admin_addresses: Vec<Vec<u8>>,
}

impl WasmRouter {
    fn check_upload_permission(&self, sender: &[u8]) -> HoResult<()> {
        // Verify sender is authorized to upload code
        Ok(())
    }
}
```

### 13.3 Gas Metering

Implement proper gas metering for WASM execution to prevent DoS:

```rust
pub struct GasConfig {
    pub storage_read_cost: u64,
    pub storage_write_cost: u64,
    pub compute_cost_multiplier: f64,
}
```

## 14. Deployment & Operations

### 14.1 Server Startup Sequence

In your main server initialization (e.g., `packages/ergors/src/main.rs`):

```rust
#[tokio::main]
async fn main() -> Result<()> {
    // Load config
    let config = load_config()?;

    // Initialize storage
    let storage = cnidarium::Storage::load(config.data_dir.clone()).await?;
    let mut state = storage.state().await?;

    // Load or generate node key
    let node_key = load_or_generate_node_key(&config.key_file)?;

    // Initialize WASM module
    let (wasm_runtime, key_manager) =
        initialize_wasm_module(&mut state, config.data_dir.clone(), node_key).await?;

    // Migrate API keys to encrypted storage (one-time operation)
    migrate_api_keys(&mut state, &config.api_keys_file, &key_manager).await?;

    // Create routers
    let llm_router = LlmRouter::new(&state, &config.llm).await?;
    let wasm_router = WasmRouter::new(wasm_runtime, key_manager);

    // Build Axum app
    let app = Router::new()
        .merge(llm_routes(llm_router))
        .merge(wasm_routes(wasm_router))
        .merge(network_routes());

    // Start server
    axum::Server::bind(&config.listen_addr)
        .serve(app.into_make_service())
        .await?;

    Ok(())
}
```

### 14.2 Configuration

Add to your TOML config:

```toml
[wasm]
max_code_size = 800_000  # 800KB
cache_dir = "data/wasm_cache"
instantiate_gas = 100_000_000
execute_gas = 50_000_000
query_gas = 10_000_000

[encryption]
key_file = "data/node_key.json"
auto_migrate_keys = true
```

## 15. Potential Challenges & Solutions

### Challenge 1: WASM Sandboxing & Safety

**Issue**: Ensuring contracts can't break out of sandbox or DOS the system

**Solutions**:
- Use CosmWasm's proven VM with gas metering
- Implement strict memory limits
- Use `wasmtime` with proper security settings
- Regular security audits of uploaded contracts

### Challenge 2: Key Management in Distributed Mesh

**Issue**: Each node has different keys - how to share contracts across nodes?

**Solutions**:
- Use threshold encryption for shared secrets
- Implement key escrow for critical operations
- Use IBC for cross-node contract calls
- Implement proper key derivation hierarchies

### Challenge 3: State Synchronization

**Issue**: WASM contract state needs to stay consistent across distributed storage

**Solutions**:
- Leverage `cnidarium`'s JMT for verifiable state roots
- Implement state snapshots and migrations
- Use deterministic execution (CosmWasm guarantees this)
- Add state pruning and archival

### Challenge 4: Performance

**Issue**: WASM execution overhead may slow down routing

**Solutions**:
- Cache compiled WASM modules
- Use lazy loading for contract code
- Implement contract state caching
- Consider using `wasmer` for JIT compilation

### Challenge 5: Upgradeability

**Issue**: Smart contracts may need updates

**Solutions**:
- Implement migration endpoints
- Use admin-only upgrade permissions
- Version all contracts with semantic versioning
- Implement feature flags in contracts

## 16. Future Enhancements

1. **IBC Integration**: Connect WASM contracts to IBC for cross-chain operations
2. **DAO Governance**: Use your existing `dao-contracts` submodule for contract governance
3. **Oracle Support**: Allow contracts to fetch external data via LLM providers
4. **Event Streaming**: Emit contract events through your network layer
5. **Query Optimization**: Add indexed contract state queries
6. **Multi-sig Admin**: Require multiple signatures for critical operations

## 17. Complete File Structure

```
packages/ho-std/src/
├── crypto/
│   ├── mod.rs
│   └── encryption.rs
├── wasm/
│   ├── mod.rs
│   ├── init.rs
│   ├── runtime.rs
│   ├── router.rs
│   ├── state_ext.rs
│   └── state_keys.rs
├── error.rs (updated)
└── lib.rs (updated)

proto/ergors/wasm/v1/
├── wasm.proto
└── state.proto

packages/ergors/src/
└── server.rs (updated with WASM routes)
```

## 18. Summary

This implementation plan provides:

✅ **WASM-VM Runtime**: Full CosmWasm contract support with upload, instantiate, execute, query
✅ **Encrypted Storage**: ChaCha20-Poly1305 encryption for API keys using node Ed25519 keys
✅ **Mesh Integration**: Seamless routing between LLM providers and WASM contracts
✅ **JMT Storage**: Dedicated `cnidarium` branches for keys and contract state
✅ **Security**: Proper encryption, sandboxing, gas metering, and access control
✅ **Extensibility**: Proto3-based types for versioning, IBC compatibility
✅ **Production Ready**: Comprehensive error handling, logging, and testing

The design follows your existing patterns:
- Trait-based architecture (`StateReadExt`, `StateWriteExt`)
- Proto3 type definitions for all structures
- `cnidarium` for verifiable storage
- Axum for HTTP routing
- Modular, testable code organization

---

## Next Steps

1. **Phase 1 - Foundation** (Week 1-2)
   - Add dependencies to Cargo.toml
   - Create proto definitions
   - Generate Rust types from protos
   - Implement storage key structure

2. **Phase 2 - Encryption** (Week 2-3)
   - Implement KeyManager
   - Add encryption/decryption logic
   - Test key rotation
   - Add migration utilities

3. **Phase 3 - WASM Runtime** (Week 3-5)
   - Implement WasmRuntime
   - Add contract lifecycle management
   - Integrate CosmWasm VM
   - Add gas metering

4. **Phase 4 - Integration** (Week 5-6)
   - Create WasmRouter
   - Add server routes
   - Integrate with LlmRouter
   - Add initialization logic

5. **Phase 5 - Testing & Security** (Week 6-8)
   - Write unit tests
   - Add integration tests
   - Security audit
   - Performance optimization

6. **Phase 6 - Documentation & Deployment** (Week 8-9)
   - API documentation
   - Deployment guides
   - Monitoring setup
   - Production rollout
