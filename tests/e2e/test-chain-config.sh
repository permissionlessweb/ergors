#!/bin/bash
#
# Manual test for chain config storage round-trip
#
# Tests:
# 1. Set chain config
# 2. Get chain config (verify fields)
# 3. List chains (verify entry exists)
# 4. Delete chain config
# 5. Get after delete (verify returns not found)

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/lib/common.sh"
source "$SCRIPT_DIR/lib/ergors.sh"

TEST_CHAIN_ID="test-chain-$(date +%s)"
TEST_DIR="${TEST_DIR:-/tmp/ergors-chain-config-test}"

log_step "Chain Config Storage Round-Trip Test"

# Clean and prepare
rm -rf "$TEST_DIR"
mkdir -p "$TEST_DIR"

# Build if needed
if [[ ! -f "$ERGORS_BIN" ]]; then
    log "Building ergors..."
    cd "$ROOT_DIR"
    cargo build --release -p ergors
fi

# Start test network
log "Starting ERGORS test network..."
ergors_start_network

# Test 1: Set chain config
log_step "Test 1: Set Chain Config"
ERGORS_CUSTODY_PASSWORD="${TEST_CUSTODY_PASSWORD}" \
    "$ERGORS_BIN" --home "${TEST_DIR}/coordinator" config set-chain "$TEST_CHAIN_ID" \
    --name "Test Chain" \
    --prefix "test" \
    --denom "utest" \
    --rpc "http://rpc.test.com:26657,http://rpc2.test.com:26657" \
    --grpc "http://grpc.test.com:9090" \
    --rest "http://rest.test.com:1317" \
    --gas-prices "0.05utest" \
    --gas-adjustment "2.0" \
    --keyring-backend "test" \
    --default-key "testkey"

if [[ $? -eq 0 ]]; then
    log_success "✓ Set chain config succeeded"
else
    log_error "✗ Set chain config failed"
    exit 1
fi

# Test 2: Get chain config and verify fields
log_step "Test 2: Get Chain Config"
OUTPUT=$(ERGORS_CUSTODY_PASSWORD="${TEST_CUSTODY_PASSWORD}" \
    "$ERGORS_BIN" --home "${TEST_DIR}/coordinator" config get-chain "$TEST_CHAIN_ID" 2>&1)

echo "$OUTPUT"

# Verify expected fields
if echo "$OUTPUT" | grep -q "Test Chain"; then
    log_success "✓ Chain name matches"
else
    log_error "✗ Chain name not found"
    exit 1
fi

if echo "$OUTPUT" | grep -q "Prefix:.*test"; then
    log_success "✓ Prefix matches"
else
    log_error "✗ Prefix not found"
    exit 1
fi

if echo "$OUTPUT" | grep -q "Denom:.*utest"; then
    log_success "✓ Denom matches"
else
    log_error "✗ Denom not found"
    exit 1
fi

if echo "$OUTPUT" | grep -q "http://rpc.test.com:26657"; then
    log_success "✓ RPC endpoint matches"
else
    log_error "✗ RPC endpoint not found"
    exit 1
fi

# Test 3: List chains
log_step "Test 3: List Chains"
LIST_OUTPUT=$(ERGORS_CUSTODY_PASSWORD="${TEST_CUSTODY_PASSWORD}" \
    "$ERGORS_BIN" --home "${TEST_DIR}/coordinator" config list-chains 2>&1)

echo "$LIST_OUTPUT"

if echo "$LIST_OUTPUT" | grep -q "$TEST_CHAIN_ID"; then
    log_success "✓ Chain appears in list"
else
    log_error "✗ Chain not found in list"
    exit 1
fi

# Should also have the local chain from startup
if echo "$LIST_OUTPUT" | grep -q "local"; then
    log_success "✓ Local chain from E2E setup also present"
else
    log_warn "⚠ Local chain not found (E2E setup may have failed)"
fi

# Test 4: Delete chain config
log_step "Test 4: Delete Chain Config"
ERGORS_CUSTODY_PASSWORD="${TEST_CUSTODY_PASSWORD}" \
    "$ERGORS_BIN" --home "${TEST_DIR}/coordinator" config delete-chain "$TEST_CHAIN_ID"

if [[ $? -eq 0 ]]; then
    log_success "✓ Delete chain config succeeded"
else
    log_error "✗ Delete chain config failed"
    exit 1
fi

# Test 5: Get after delete (should fail/return not found)
log_step "Test 5: Get After Delete (should fail)"
if ERGORS_CUSTODY_PASSWORD="${TEST_CUSTODY_PASSWORD}" \
    "$ERGORS_BIN" --home "${TEST_DIR}/coordinator" config get-chain "$TEST_CHAIN_ID" 2>&1 | grep -q "not found"; then
    log_success "✓ Get after delete correctly returns not found"
else
    log_error "✗ Get after delete did not return expected error"
    exit 1
fi

# Test 6: Delete non-existent chain (should fail)
log_step "Test 6: Delete Non-Existent Chain (should fail)"
if ERGORS_CUSTODY_PASSWORD="${TEST_CUSTODY_PASSWORD}" \
    "$ERGORS_BIN" --home "${TEST_DIR}/coordinator" config delete-chain "nonexistent-chain" 2>&1 | grep -q "not found"; then
    log_success "✓ Delete non-existent chain correctly fails"
else
    log_error "✗ Delete non-existent chain did not return expected error"
    exit 1
fi

# Cleanup
ergors_stop_network

log_success "All chain config tests passed!"
