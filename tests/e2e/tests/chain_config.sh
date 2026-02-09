#!/bin/bash
#
# tests/chain_config.sh - Cosmos chain config CRUD E2E tests
#
# Tests the full stack: CLI -> gRPC -> cnidarium storage -> retrieval
#   - Set chain config (store in cnidarium)
#   - Get chain config (verify field round-trip)
#   - List chains (verify entry present)
#   - Delete chain config
#   - Get after delete (verify not found)
#   - Delete non-existent chain (verify error)

# Prevent multiple sourcing
[[ -n "${_E2E_TEST_CHAIN_CONFIG_LOADED:-}" ]] && return 0
_E2E_TEST_CHAIN_CONFIG_LOADED=1

# =============================================================================
# Chain Config CRUD Tests
# =============================================================================

test_chain_config_set() {
    log_section "Chain Config: Set"

    local chain_id="e2e-test-chain-$$"
    local coord_home="${TEST_DIR}/coordinator"

    # Store chain_id for subsequent tests
    export E2E_TEST_CHAIN_ID="$chain_id"

    local output
    output=$(ERGORS_CUSTODY_PASSWORD="${TEST_CUSTODY_PASSWORD}" \
        "$ERGORS_BIN" --home "$coord_home" config set-chain "$chain_id" \
        --name "E2E Test Chain" \
        --prefix "test" \
        --denom "utest" \
        --rpc "http://rpc.test.com:26657,http://rpc2.test.com:26657" \
        --grpc "http://grpc.test.com:9090" \
        --rest "http://rest.test.com:1317" \
        --gas-prices "0.05utest" \
        --gas-adjustment "2.0" \
        --keyring-backend "test" \
        --default-key "testkey" 2>&1) || true

    if echo "$output" | grep -qi "stored\|success"; then
        test_pass "chain_config_set" "Set chain config for $chain_id"
    else
        test_fail "chain_config_set" "Failed to set chain config" "$output"
    fi
}

test_chain_config_get() {
    log_section "Chain Config: Get + Field Verification"

    local chain_id="${E2E_TEST_CHAIN_ID:-}"
    if [[ -z "$chain_id" ]]; then
        test_fail "chain_config_get" "No chain_id from prior set test"
        return
    fi

    local coord_home="${TEST_DIR}/coordinator"
    local output
    output=$(ERGORS_CUSTODY_PASSWORD="${TEST_CUSTODY_PASSWORD}" \
        "$ERGORS_BIN" --home "$coord_home" config get-chain "$chain_id" 2>&1) || true

    log_verbose "Get output: $output"

    # Verify chain name
    if echo "$output" | grep -q "E2E Test Chain"; then
        test_pass "chain_config_get_name" "Chain name round-trips correctly"
    else
        test_fail "chain_config_get_name" "Chain name not found in output" "$output"
    fi

    # Verify prefix
    if echo "$output" | grep -q "Prefix:.*test"; then
        test_pass "chain_config_get_prefix" "Bech32 prefix round-trips correctly"
    else
        test_fail "chain_config_get_prefix" "Prefix not found" "$output"
    fi

    # Verify denom
    if echo "$output" | grep -q "Denom:.*utest"; then
        test_pass "chain_config_get_denom" "Denom round-trips correctly"
    else
        test_fail "chain_config_get_denom" "Denom not found" "$output"
    fi

    # Verify RPC endpoint (with scheme)
    if echo "$output" | grep -q "http://rpc.test.com:26657"; then
        test_pass "chain_config_get_rpc" "RPC endpoint preserved with scheme"
    else
        test_fail "chain_config_get_rpc" "RPC endpoint not found" "$output"
    fi
}

test_chain_config_list() {
    log_section "Chain Config: List"

    local chain_id="${E2E_TEST_CHAIN_ID:-}"
    local coord_home="${TEST_DIR}/coordinator"

    local output
    output=$(ERGORS_CUSTODY_PASSWORD="${TEST_CUSTODY_PASSWORD}" \
        "$ERGORS_BIN" --home "$coord_home" config list-chains 2>&1) || true

    log_verbose "List output: $output"

    # Test chain should appear
    if [[ -n "$chain_id" ]] && echo "$output" | grep -q "$chain_id"; then
        test_pass "chain_config_list_test" "Test chain appears in list"
    else
        test_fail "chain_config_list_test" "Test chain not in list" "$output"
    fi

    # Local chain from ergors_start_network should also be present
    if echo "$output" | grep -q "local"; then
        test_pass "chain_config_list_local" "Local chain from network startup present"
    else
        test_fail "chain_config_list_local" "Local chain not in list (startup config may have failed)" "$output"
    fi
}

test_chain_config_delete() {
    log_section "Chain Config: Delete"

    local chain_id="${E2E_TEST_CHAIN_ID:-}"
    if [[ -z "$chain_id" ]]; then
        test_fail "chain_config_delete" "No chain_id from prior set test"
        return
    fi

    local coord_home="${TEST_DIR}/coordinator"

    # Delete should succeed
    local output
    output=$(ERGORS_CUSTODY_PASSWORD="${TEST_CUSTODY_PASSWORD}" \
        "$ERGORS_BIN" --home "$coord_home" config delete-chain "$chain_id" 2>&1) || true

    if echo "$output" | grep -qi "deleted\|success"; then
        test_pass "chain_config_delete" "Delete chain config succeeded"
    else
        test_fail "chain_config_delete" "Delete returned unexpected output" "$output"
    fi

    # Get after delete should return not found
    local get_output
    get_output=$(ERGORS_CUSTODY_PASSWORD="${TEST_CUSTODY_PASSWORD}" \
        "$ERGORS_BIN" --home "$coord_home" config get-chain "$chain_id" 2>&1) || true

    if echo "$get_output" | grep -qi "not found"; then
        test_pass "chain_config_get_after_delete" "Get after delete returns not found"
    else
        test_fail "chain_config_get_after_delete" "Expected 'not found' after delete" "$get_output"
    fi
}

test_chain_config_delete_nonexistent() {
    log_section "Chain Config: Delete Non-Existent"

    local coord_home="${TEST_DIR}/coordinator"

    local output
    output=$(ERGORS_CUSTODY_PASSWORD="${TEST_CUSTODY_PASSWORD}" \
        "$ERGORS_BIN" --home "$coord_home" config delete-chain "nonexistent-chain-$$" 2>&1) || true

    if echo "$output" | grep -qi "not found"; then
        test_pass "chain_config_delete_nonexistent" "Delete non-existent chain returns error"
    else
        test_fail "chain_config_delete_nonexistent" "Expected 'not found' for non-existent chain" "$output"
    fi
}

# =============================================================================
# Combined Chain Config Test Suite
# =============================================================================

run_chain_config_tests() {
    log_step "Running Chain Config Tests"

    test_chain_config_set
    test_chain_config_get
    test_chain_config_list
    test_chain_config_delete
    test_chain_config_delete_nonexistent
}
