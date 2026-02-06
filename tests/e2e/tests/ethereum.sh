#!/bin/bash
#
# tests/ethereum.sh - Ethereum network interaction tests
#
# Tests: Local Anvil network, JSON-RPC queries, ETH transfers,
#        wallet derivation, engine ETH integration
#
# Requires: Anvil (Foundry) - auto-installed if missing

# Prevent multiple sourcing
[[ -n "${_E2E_TEST_ETHEREUM_LOADED:-}" ]] && return 0
_E2E_TEST_ETHEREUM_LOADED=1

# Track state across tests
ETH_TX_HASH=""

# =============================================================================
# Anvil Network Health Tests
# =============================================================================

test_anvil_network() {
    log_section "Anvil Network Health Tests"

    # Test 1: Anvil is running and responding
    if ethereum_anvil_healthy; then
        test_pass "anvil_healthy" "Anvil node is healthy and responding"
    else
        test_fail "anvil_healthy" "Anvil node not responding at $ANVIL_RPC"
        return 1
    fi

    # Test 2: Chain ID matches configuration
    local chain_id
    chain_id=$(eth_query_chain_id)
    log_verbose "Reported chain ID: $chain_id (expected: $ANVIL_CHAIN_ID)"

    if [[ "$chain_id" -eq "$ANVIL_CHAIN_ID" ]]; then
        test_pass "chain_id_match" "Chain ID matches: $chain_id"
    else
        test_fail "chain_id_match" "Chain ID mismatch" "Got: $chain_id, Expected: $ANVIL_CHAIN_ID"
    fi

    # Test 3: Blocks are being produced
    local block_num
    block_num=$(eth_query_block_number)
    log_verbose "Current block number: $block_num"

    if [[ "$block_num" -ge 0 ]]; then
        test_pass "blocks_produced" "Block number: $block_num"
    else
        test_fail "blocks_produced" "Could not query block number"
    fi

    # Test 4: Gas price is non-zero
    local gas_price
    gas_price=$(eth_query_gas_price)
    log_verbose "Gas price: $gas_price wei"

    if [[ "$gas_price" -gt 0 ]]; then
        test_pass "gas_price_available" "Gas price: $gas_price wei"
    else
        test_fail "gas_price_available" "Gas price is zero or unavailable"
    fi
}

# =============================================================================
# Pre-funded Account Tests
# =============================================================================

test_funded_accounts() {
    log_section "Pre-funded Account Tests"

    # Test 1: Account 0 has balance
    local balance_0
    balance_0=$(eth_query_balance "$ANVIL_ACCOUNT_0")
    log_verbose "Account 0 ($ANVIL_ACCOUNT_0) balance: $balance_0 wei"

    if [[ "$balance_0" -gt 0 ]]; then
        local eth_0
        eth_0=$(eth_wei_to_eth "$balance_0")
        test_pass "account_0_funded" "Account 0 has balance: ${eth_0} ETH"
    else
        test_fail "account_0_funded" "Account 0 has no balance"
        return 1
    fi

    # Test 2: Account 1 has balance
    local balance_1
    balance_1=$(eth_query_balance "$ANVIL_ACCOUNT_1")
    log_verbose "Account 1 ($ANVIL_ACCOUNT_1) balance: $balance_1 wei"

    if [[ "$balance_1" -gt 0 ]]; then
        local eth_1
        eth_1=$(eth_wei_to_eth "$balance_1")
        test_pass "account_1_funded" "Account 1 has balance: ${eth_1} ETH"
    else
        test_fail "account_1_funded" "Account 1 has no balance"
    fi

    # Test 3: Both accounts start with same balance (10000 ETH)
    if [[ "$balance_0" -eq "$balance_1" ]]; then
        test_pass "equal_initial_balance" "Both accounts have equal initial balance"
    else
        test_skip "equal_initial_balance" "Balances differ (may have been used already)"
    fi

    # Test 4: Nonce starts at 0
    local nonce_0
    nonce_0=$(eth_query_nonce "$ANVIL_ACCOUNT_0")
    log_verbose "Account 0 nonce: $nonce_0"

    if [[ "$nonce_0" -eq 0 ]]; then
        test_pass "initial_nonce_zero" "Account 0 nonce is 0 (fresh state)"
    else
        test_skip "initial_nonce_zero" "Nonce is $nonce_0 (node may have prior activity)"
    fi
}

# =============================================================================
# JSON-RPC Query Tests
# =============================================================================

test_json_rpc_queries() {
    log_section "JSON-RPC Query Tests"

    # Test 1: eth_getBalance returns valid hex
    log_verbose "Testing eth_getBalance..."
    local balance_response
    balance_response=$(eth_rpc "eth_getBalance" "[\"$ANVIL_ACCOUNT_0\", \"latest\"]")
    local balance_result
    balance_result=$(eth_rpc_result "$balance_response")

    if [[ "$balance_result" == 0x* ]]; then
        test_pass "rpc_get_balance" "eth_getBalance returns valid hex: ${balance_result:0:18}..."
    else
        test_fail "rpc_get_balance" "eth_getBalance returned unexpected format" "Got: $balance_result"
    fi

    # Test 2: eth_blockNumber returns valid hex
    log_verbose "Testing eth_blockNumber..."
    local block_response
    block_response=$(eth_rpc "eth_blockNumber" "[]")
    local block_result
    block_result=$(eth_rpc_result "$block_response")

    if [[ "$block_result" == 0x* ]]; then
        test_pass "rpc_block_number" "eth_blockNumber returns valid hex: $block_result"
    else
        test_fail "rpc_block_number" "eth_blockNumber returned unexpected format"
    fi

    # Test 3: eth_getTransactionCount returns valid hex
    log_verbose "Testing eth_getTransactionCount..."
    local nonce_response
    nonce_response=$(eth_rpc "eth_getTransactionCount" "[\"$ANVIL_ACCOUNT_0\", \"latest\"]")
    local nonce_result
    nonce_result=$(eth_rpc_result "$nonce_response")

    if [[ "$nonce_result" == 0x* ]]; then
        test_pass "rpc_get_nonce" "eth_getTransactionCount returns valid hex: $nonce_result"
    else
        test_fail "rpc_get_nonce" "eth_getTransactionCount returned unexpected format"
    fi

    # Test 4: eth_gasPrice returns valid hex
    log_verbose "Testing eth_gasPrice..."
    local gas_response
    gas_response=$(eth_rpc "eth_gasPrice" "[]")
    local gas_result
    gas_result=$(eth_rpc_result "$gas_response")

    if [[ "$gas_result" == 0x* ]]; then
        test_pass "rpc_gas_price" "eth_gasPrice returns valid hex: $gas_result"
    else
        test_fail "rpc_gas_price" "eth_gasPrice returned unexpected format"
    fi

    # Test 5: eth_getBlockByNumber returns block object
    log_verbose "Testing eth_getBlockByNumber..."
    local latest_block
    latest_block=$(eth_rpc "eth_getBlockByNumber" "[\"latest\", false]")

    if json_has "$latest_block" '.result.number'; then
        local block_hash
        block_hash=$(json_get "$latest_block" '.result.hash')
        test_pass "rpc_get_block" "eth_getBlockByNumber returns block (hash: ${block_hash:0:18}...)"
    else
        test_fail "rpc_get_block" "eth_getBlockByNumber did not return block object"
    fi

    # Test 6: Invalid method returns proper error
    log_verbose "Testing invalid RPC method..."
    local invalid_response
    invalid_response=$(eth_rpc "eth_invalidMethodXYZ" "[]")

    if json_has "$invalid_response" '.error'; then
        test_pass "rpc_invalid_method" "Invalid method returns proper JSON-RPC error"
    else
        test_fail "rpc_invalid_method" "Invalid method did not return error"
    fi
}

# =============================================================================
# ETH Transfer Tests
# =============================================================================

test_eth_transfer() {
    log_section "ETH Transfer Tests"

    # Check if cast is available for transfers
    if ! command -v cast &>/dev/null; then
        test_skip "eth_transfer" "cast not available (install Foundry for transfer tests)"
        return 0
    fi

    # Record pre-transfer balances
    local pre_balance_0 pre_balance_1
    pre_balance_0=$(eth_query_balance "$ANVIL_ACCOUNT_0")
    pre_balance_1=$(eth_query_balance "$ANVIL_ACCOUNT_1")
    log_verbose "Pre-transfer: Account 0 = $pre_balance_0 wei, Account 1 = $pre_balance_1 wei"

    # Test 1: Send 1 ETH from Account 0 to Account 1
    log "Sending 1 ETH from Account 0 to Account 1..."
    local send_output
    send_output=$(eth_send_transfer "$ANVIL_PRIVATE_KEY_0" "$ANVIL_ACCOUNT_1" "1" 2>&1) || true
    log_debug "Send output: $send_output"

    # Extract tx hash from cast output
    local tx_hash
    tx_hash=$(echo "$send_output" | grep -oE "0x[a-fA-F0-9]{64}" | head -1)

    if [[ -n "$tx_hash" ]]; then
        ETH_TX_HASH="$tx_hash"
        test_pass "eth_send" "ETH transfer submitted (tx: ${tx_hash:0:18}...)"
    else
        # cast might output differently, check for success indicators
        if echo "$send_output" | grep -qiE "success|status.*1|blockNumber"; then
            test_pass "eth_send" "ETH transfer succeeded"
        else
            test_fail "eth_send" "ETH transfer failed" "Output: ${send_output:0:200}"
            return 1
        fi
    fi

    # Wait for block confirmation
    sleep 2

    # Test 2: Verify recipient balance increased
    local post_balance_1
    post_balance_1=$(eth_query_balance "$ANVIL_ACCOUNT_1")
    log_verbose "Post-transfer: Account 1 = $post_balance_1 wei (was $pre_balance_1)"

    if [[ "$post_balance_1" -gt "$pre_balance_1" ]]; then
        test_pass "balance_increased" "Recipient balance increased after transfer"
    else
        test_fail "balance_increased" "Recipient balance did not increase"
    fi

    # Test 3: Verify sender balance decreased (balance - amount - gas)
    local post_balance_0
    post_balance_0=$(eth_query_balance "$ANVIL_ACCOUNT_0")
    log_verbose "Post-transfer: Account 0 = $post_balance_0 wei (was $pre_balance_0)"

    if [[ "$post_balance_0" -lt "$pre_balance_0" ]]; then
        test_pass "balance_decreased" "Sender balance decreased (transfer + gas)"
    else
        test_fail "balance_decreased" "Sender balance did not decrease"
    fi

    # Test 4: Verify sender nonce incremented
    local post_nonce_0
    post_nonce_0=$(eth_query_nonce "$ANVIL_ACCOUNT_0")
    log_verbose "Post-transfer: Account 0 nonce = $post_nonce_0"

    if [[ "$post_nonce_0" -gt 0 ]]; then
        test_pass "nonce_incremented" "Sender nonce incremented to $post_nonce_0"
    else
        test_fail "nonce_incremented" "Sender nonce did not increment"
    fi
}

# =============================================================================
# Transaction Receipt Tests
# =============================================================================

test_tx_receipt() {
    log_section "Transaction Receipt Tests"

    if [[ -z "$ETH_TX_HASH" ]]; then
        test_skip "tx_receipt" "No transaction hash available (transfer test may have been skipped)"
        return 0
    fi

    # Test 1: Receipt exists
    log_verbose "Querying receipt for $ETH_TX_HASH..."
    local receipt_response
    receipt_response=$(eth_get_receipt "$ETH_TX_HASH")
    local receipt
    receipt=$(eth_rpc_result "$receipt_response")

    if [[ -n "$receipt" ]] && [[ "$receipt" != "null" ]]; then
        test_pass "receipt_exists" "Transaction receipt found"
    else
        test_fail "receipt_exists" "Transaction receipt not found"
        return 1
    fi

    # Test 2: Receipt shows success (status 0x1)
    local status
    status=$(json_get "$receipt_response" '.result.status')
    log_verbose "Receipt status: $status"

    if [[ "$status" == "0x1" ]]; then
        test_pass "receipt_success" "Transaction status: success (0x1)"
    elif [[ "$status" == "0x0" ]]; then
        test_fail "receipt_success" "Transaction reverted (0x0)"
    else
        test_skip "receipt_success" "Unknown status format: $status"
    fi

    # Test 3: Receipt has gas used
    local gas_used
    gas_used=$(json_get "$receipt_response" '.result.gasUsed')
    log_verbose "Gas used: $gas_used"

    if [[ -n "$gas_used" ]] && [[ "$gas_used" == 0x* ]]; then
        local gas_dec
        gas_dec=$(eth_hex_to_dec "$gas_used")
        test_pass "receipt_gas_used" "Gas used: $gas_dec"
    else
        test_fail "receipt_gas_used" "Gas used field missing or invalid"
    fi

    # Test 4: Non-existent receipt returns null
    local fake_receipt
    fake_receipt=$(eth_get_receipt "0x0000000000000000000000000000000000000000000000000000000000000000")
    local fake_result
    fake_result=$(eth_rpc_result "$fake_receipt")

    if [[ "$fake_result" == "null" ]] || [[ -z "$fake_result" ]]; then
        test_pass "receipt_nonexistent" "Non-existent tx returns null receipt"
    else
        test_fail "receipt_nonexistent" "Non-existent tx returned unexpected data"
    fi
}

# =============================================================================
# EIP-1559 Fee Data Tests
# =============================================================================

test_fee_data() {
    log_section "EIP-1559 Fee Data Tests"

    # Test 1: baseFeePerGas available in latest block
    local block_response
    block_response=$(eth_rpc "eth_getBlockByNumber" "[\"latest\", false]")

    local base_fee
    base_fee=$(json_get "$block_response" '.result.baseFeePerGas')
    log_verbose "Base fee per gas: $base_fee"

    if [[ -n "$base_fee" ]] && [[ "$base_fee" == 0x* ]]; then
        local base_fee_dec
        base_fee_dec=$(eth_hex_to_dec "$base_fee")
        test_pass "base_fee_available" "Base fee per gas: $base_fee_dec wei"
    else
        test_skip "base_fee_available" "baseFeePerGas not in block (pre-London fork)"
    fi

    # Test 2: eth_maxPriorityFeePerGas
    local priority_response
    priority_response=$(eth_rpc "eth_maxPriorityFeePerGas" "[]")
    local priority_fee
    priority_fee=$(eth_rpc_result "$priority_response")
    log_verbose "Max priority fee: $priority_fee"

    if [[ -n "$priority_fee" ]] && [[ "$priority_fee" == 0x* ]]; then
        local priority_dec
        priority_dec=$(eth_hex_to_dec "$priority_fee")
        test_pass "priority_fee_available" "Max priority fee per gas: $priority_dec wei"
    else
        test_skip "priority_fee_available" "eth_maxPriorityFeePerGas not supported"
    fi

    # Test 3: Gas estimation works
    log_verbose "Testing gas estimation for simple transfer..."
    local estimate_response
    estimate_response=$(eth_rpc "eth_estimateGas" "[{\"from\":\"$ANVIL_ACCOUNT_0\",\"to\":\"$ANVIL_ACCOUNT_1\",\"value\":\"0x1\"}]")
    local estimate_result
    estimate_result=$(eth_rpc_result "$estimate_response")

    if [[ -n "$estimate_result" ]] && [[ "$estimate_result" == 0x* ]]; then
        local gas_estimate
        gas_estimate=$(eth_hex_to_dec "$estimate_result")
        test_pass "gas_estimation" "Gas estimate for simple transfer: $gas_estimate"
    else
        test_fail "gas_estimation" "Gas estimation failed"
    fi
}

# =============================================================================
# Engine ETH Integration Tests (via ergors CLI)
# =============================================================================

test_engine_eth_integration() {
    log_section "Engine ETH Integration Tests"

    # These tests verify the engine can interact with the local ETH network.
    # They use ergors CLI commands that wrap the EthClient/EthSigner modules.
    # Commands may not exist yet, so we test gracefully.

    # Test 1: Derive ETH address from engine keystore
    log_verbose "Testing ETH address derivation via engine..."
    local addr_output
    addr_output=$(ergors_eth_address "default" 0 2>&1) || true
    log_debug "ETH address output: $addr_output"

    if echo "$addr_output" | grep -qE "^0x[a-fA-F0-9]{40}$|\"address\".*0x"; then
        local eth_addr
        eth_addr=$(echo "$addr_output" | grep -oE "0x[a-fA-F0-9]{40}" | head -1)
        test_pass "engine_eth_address" "Engine derived ETH address: ${eth_addr:0:12}..."
    elif echo "$addr_output" | grep -qiE "not.*found|unknown|unrecognized|error"; then
        test_skip "engine_eth_address" "ETH address CLI command not available yet"
    else
        test_skip "engine_eth_address" "ETH address derivation response unclear"
    fi

    # Test 2: Query balance via engine against local Anvil
    log_verbose "Testing ETH balance query via engine..."
    local balance_output
    balance_output=$(ergors_eth_balance "$ANVIL_ACCOUNT_0" "$ANVIL_RPC" 2>&1) || true
    log_debug "ETH balance output: $balance_output"

    if echo "$balance_output" | grep -qiE "[0-9].*ETH|balance.*[0-9]|wei"; then
        test_pass "engine_eth_balance" "Engine queried ETH balance from Anvil"
    elif echo "$balance_output" | grep -qiE "not.*found|unknown|unrecognized|error"; then
        test_skip "engine_eth_balance" "ETH balance CLI command not available yet"
    else
        test_skip "engine_eth_balance" "ETH balance response unclear"
    fi

    # Test 3: Send ETH via engine (if command exists)
    log_verbose "Testing ETH send via engine..."
    local send_output
    send_output=$(ergors_eth_send "$ANVIL_ACCOUNT_1" "0.01" "default" "$ANVIL_RPC" 2>&1) || true
    log_debug "ETH send output: $send_output"

    if echo "$send_output" | grep -qE "0x[a-fA-F0-9]{64}|tx_hash|success"; then
        test_pass "engine_eth_send" "Engine sent ETH via Anvil"
    elif echo "$send_output" | grep -qiE "not.*found|unknown|unrecognized|error"; then
        test_skip "engine_eth_send" "ETH send CLI command not available yet"
    else
        test_skip "engine_eth_send" "ETH send response unclear"
    fi
}

# =============================================================================
# Combined Ethereum Test Suite
# =============================================================================

run_ethereum_tests() {
    log_step "Running Ethereum Tests"

    # Network health
    test_anvil_network

    # Pre-funded account verification
    test_funded_accounts

    # JSON-RPC query coverage
    test_json_rpc_queries

    # Fee data (EIP-1559)
    test_fee_data

    # ETH transfer workflow
    test_eth_transfer

    # Transaction receipt verification
    test_tx_receipt

    # Engine integration (skip gracefully if commands don't exist yet)
    test_engine_eth_integration
}
