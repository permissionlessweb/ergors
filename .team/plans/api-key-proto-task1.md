# API Key Proto Types Task - Plan

## Current Status
Working on Task 1: Define API Key Proto Types

## Completed
1. Created proto file at `/Users/returniflost/CW-AGENT/e2e-improvements/proto/ergors/apikeys/v1/apikeys.proto` with exact content specified:
   - ApiKey message with 8 fields (id, name, secret_hash, created_at_unix, expires_at_unix, is_active, endpoint_label, metadata)
   - ApiKeyMetadata message with 3 fields (created_by_pubkey, created_by_nonce, tags)
   - CreateApiKeyRequest message
   - CreateApiKeyResponse message
   - ValidateApiKeyRequest message
   - ValidateApiKeyResponse message

## TODO - Remaining Steps
1. Create test file - TWO POSSIBLE LOCATIONS:
   - Option A: `packages/ergors/tests/apikey_types.rs` (as per task spec) - permission denied to create here
   - Option B: Add module to `tests/src/apikeys.rs` in the test workspace (standard pattern in repo)

2. Run proto generation: `cd proto && cargo run`
   - This should generate Rust types in `packages/ho-proto-rs/src/prelude.rs`

3. Run test: `cargo tes -p ergors apikey_types` (using cargo tes alias per CLAUDE.md)
   - This outputs compact JSON format errors

4. Commit with message: "proto: add API key management types"

## Context Notes
- Proto file created: /Users/returniflost/CW-AGENT/e2e-improvements/proto/ergors/apikeys/v1/apikeys.proto
- Test workspace located at: /Users/returniflost/CW-AGENT/e2e-improvements/tests/
- Standard test pattern: modules in tests/src/ with entries in tests/src/lib.rs
- Per CLAUDE.md: NEVER modify prost generated types manually - only regenerate via cargo run
- Per CLAUDE.md: Use cargo tes (compact error format) instead of cargo test
- Per CLAUDE.md: proto types accessed via ho_std::types::ergors::apikeys::v1::*

## Design Notes
- Test content uses hex::encode for secret_hash generation
- Test should check ApiKey serialization/deserialization via encode_to_vec() and decode()
- Proto package name: ergors.apikeys.v1
- All proto messages use proto3 syntax with optional fields for expires_at_unix, endpoint_label, metadata

## Blockers
- Cannot create files in packages/ergors/tests/ directly due to workspace permissions
- Need to clarify test location before proceeding with test file creation
