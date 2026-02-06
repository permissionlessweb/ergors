#!/bin/bash
#
# tests/network.sh - Network connectivity tests
#
# Tests: ERGORS node health, Akash node health, port connectivity

# Prevent multiple sourcing
[[ -n "${_E2E_TEST_NETWORK_LOADED:-}" ]] && return 0
_E2E_TEST_NETWORK_LOADED=1

# =============================================================================
# ERGORS Network Tests
# =============================================================================

test_ergors_network() {
    log_section "ERGORS Network Tests"

    # Test: Coordinator gRPC reachable
    if ergors_coordinator_healthy; then
        test_pass "coordinator_grpc" "Coordinator gRPC reachable at $COORDINATOR_GRPC"
    else
        test_fail "coordinator_grpc" "Coordinator gRPC unreachable" "Expected port ${COORDINATOR_GRPC##*:} to be listening"
    fi

    # Test: Executor gRPC reachable
    if ergors_executor_healthy; then
        test_pass "executor_grpc" "Executor gRPC reachable at $EXECUTOR_GRPC"
    else
        test_fail "executor_grpc" "Executor gRPC unreachable" "Expected port ${EXECUTOR_GRPC##*:} to be listening"
    fi

    # Test: All node processes running
    if ergors_all_nodes_running; then
        test_pass "nodes_running" "All ${#ERGORS_NODE_PIDS[@]} ERGORS nodes running"
    else
        local running=0
        for pid in "${ERGORS_NODE_PIDS[@]}"; do
            kill -0 "$pid" 2>/dev/null && running=$((running + 1))
        done
        test_fail "nodes_running" "Node processes not running" "Only $running/${#ERGORS_NODE_PIDS[@]} alive"
    fi

    # Test: Coordinator responds to node info query
    local coord_info
    coord_info=$(ergors_cli node info 2>&1) || true
    log_verbose "Coordinator node info response:"
    log_debug "$coord_info"
    if json_has "$coord_info" '.node_type'; then
        local node_type
        node_type=$(json_get "$coord_info" '.node_type')
        test_pass "coordinator_node_info" "Coordinator responding (type: $node_type)"
    else
        test_fail "coordinator_node_info" "Coordinator node info failed" "No node_type in response"
    fi

    # Test: Executor responds to node info query
    local exec_info
    exec_info=$(ergors_cli_executor node info 2>&1) || true
    log_verbose "Executor node info response:"
    log_debug "$exec_info"
    if json_has "$exec_info" '.node_type'; then
        local node_type
        node_type=$(json_get "$exec_info" '.node_type')
        test_pass "executor_node_info" "Executor responding (type: $node_type)"
    else
        test_fail "executor_node_info" "Executor node info failed" "No node_type in response"
    fi

    # Test: Coordinator node info includes bech32 address
    if json_has "$coord_info" '.bech32_address'; then
        local bech32_addr
        bech32_addr=$(json_get "$coord_info" '.bech32_address')
        if [[ "$bech32_addr" == ergors1* ]]; then
            test_pass "coordinator_bech32_in_info" "Coordinator node info includes bech32: $bech32_addr"
        else
            test_fail "coordinator_bech32_in_info" "Invalid bech32 format in node info" "Got: $bech32_addr"
        fi
    else
        test_fail "coordinator_bech32_in_info" "Node info missing bech32_address field"
    fi

    # Test: Executor node info includes bech32 address
    if json_has "$exec_info" '.bech32_address'; then
        local exec_bech32
        exec_bech32=$(json_get "$exec_info" '.bech32_address')
        if [[ "$exec_bech32" == ergors1* ]]; then
            test_pass "executor_bech32_in_info" "Executor node info includes bech32: $exec_bech32"
        else
            test_fail "executor_bech32_in_info" "Invalid bech32 format in executor info" "Got: $exec_bech32"
        fi
    else
        test_fail "executor_bech32_in_info" "Executor info missing bech32_address field"
    fi

    # # Test: Coordinator can derive cosmos address
    # local coord_addr
    # coord_addr=$(ergors_cli node address 2>&1) || true
    # log_verbose "Coordinator address response:"
    # log_debug "$coord_addr"
    # if json_has "$coord_addr" '.address'; then
    #     local address
    #     address=$(json_get "$coord_addr" '.address')
    #     if [[ "$address" == ergors1* ]]; then
    #         test_pass "coordinator_cosmos_address" "Coordinator cosmos address: $address"
    #     else
    #         test_fail "coordinator_cosmos_address" "Invalid address format" "Got: $address"
    #     fi
    # else
    #     test_fail "coordinator_cosmos_address" "Coordinator address query failed" "No address in response"
    # fi

    # Test: Executor can derive cosmos address
    local exec_addr
    exec_addr=$(ergors_cli_executor node address 2>&1) || true
    log_verbose "Executor address response:"
    log_debug "$exec_addr"
    if json_has "$exec_addr" '.address'; then
        local address
        address=$(json_get "$exec_addr" '.address')
        if [[ "$address" == ergors1* ]]; then
            test_pass "executor_cosmos_address" "Executor cosmos address: $address"
        else
            test_fail "executor_cosmos_address" "Invalid address format" "Got: $address"
        fi
    else
        test_fail "executor_cosmos_address" "Executor address query failed" "No address in response"
    fi

    # Test: Can derive address with different prefix (cosmos)
    local cosmos_addr
    cosmos_addr=$(ergors_cli node address --prefix cosmos 2>&1) || true
    log_verbose "Coordinator cosmos-prefixed address:"
    log_debug "$cosmos_addr"
    if json_has "$cosmos_addr" '.address'; then
        local address
        address=$(json_get "$cosmos_addr" '.address')
        if [[ "$address" == cosmos1* ]]; then
            test_pass "coordinator_cosmos_prefix" "Can derive cosmos-prefixed address: $address"
        else
            test_fail "coordinator_cosmos_prefix" "Invalid cosmos prefix" "Got: $address"
        fi
    else
        test_fail "coordinator_cosmos_prefix" "Cosmos prefix derivation failed" "No address in response"
    fi
}

# =============================================================================
# Akash Network Tests
# =============================================================================

test_akash_network() {
    log_section "Akash Network Tests"

    # Test: Akash RPC reachable
    if wait_for_port "127.0.0.1" 26657 5; then
        test_pass "akash_rpc" "Akash RPC port 26657 reachable"
    else
        test_fail "akash_rpc" "Akash RPC unreachable" "Port 26657 not listening"
    fi

    # Test: Akash node healthy via status endpoint
    if akash_node_healthy; then
        test_pass "akash_node_healthy" "Akash node responding to /status"
    else
        test_fail "akash_node_healthy" "Akash node not healthy" "Status endpoint not responding"
    fi

    # Test: Akash gRPC reachable
    if wait_for_port "127.0.0.1" 9090 5; then
        test_pass "akash_grpc" "Akash gRPC port 9090 reachable"
    else
        test_fail "akash_grpc" "Akash gRPC unreachable" "Port 9090 not listening"
    fi

    # Test: Kubernetes cluster accessible
    if kubectl cluster-info &>/dev/null; then
        test_pass "kubernetes_cluster" "Kubernetes cluster accessible"
    else
        test_fail "kubernetes_cluster" "Kubernetes cluster not accessible"
    fi

    # Test: Faucet mnemonic available (pre-funded key for testing)
    local faucet_mnemonic
    faucet_mnemonic=$(akash_get_faucet_mnemonic 2>/dev/null) || true

    if [[ -n "$faucet_mnemonic" ]]; then
        local word_count
        word_count=$(echo "$faucet_mnemonic" | wc -w | tr -d ' ')
        test_pass "faucet_mnemonic" "Faucet mnemonic available ($word_count words)"
    else
        test_fail "faucet_mnemonic" "Faucet mnemonic not found" "Check $AP_RUN_DIR/key-secrets/faucet.txt"
    fi
}

# =============================================================================
# Config Validation Tests
# =============================================================================

test_node_config() {
    log_section "Node Configuration Tests"

    local coord_home="$TEST_DIR/coordinator"

    # Test: Config file exists
    if [[ -f "$coord_home/config.toml" ]]; then
        test_pass "config_exists" "Coordinator config.toml exists"
    else
        test_fail "config_exists" "Coordinator config.toml missing"
        return 1
    fi

    # Test: Config get command works
    local node_type
    node_type=$("$ERGORS_BIN" --home "$coord_home" config get identity.node_type 2>&1) || true
    if [[ "$node_type" == *"Coordinator"* ]] || [[ "$node_type" == *"coordinator"* ]]; then
        test_pass "config_node_type" "Config node_type = Coordinator"
    else
        test_fail "config_node_type" "Config node_type incorrect" "Got: $node_type"
    fi

    # Test: CosmWasm enabled for coordinator
    local cw_enabled
    cw_enabled=$("$ERGORS_BIN" --home "$coord_home" config get cosmwasm.enabled 2>&1) || true
    if [[ "$cw_enabled" == *"true"* ]]; then
        test_pass "cosmwasm_enabled" "CosmWasm enabled in config"
    else
        test_fail "cosmwasm_enabled" "CosmWasm not enabled" "Got: $cw_enabled"
    fi

    # Test: SDL contract WASM present for coordinator
    if [[ -f "$coord_home/cw_sdl.wasm" ]]; then
        test_pass "sdl_wasm_present" "SDL contract WASM file present"
    else
        test_fail "sdl_wasm_present" "SDL contract WASM file missing"
    fi

    # Test: Executor config exists
    local exec_home="$TEST_DIR/executor_0"
    if [[ -f "$exec_home/config.toml" ]]; then
        test_pass "executor_config" "Executor config.toml exists"
    else
        test_fail "executor_config" "Executor config.toml missing"
    fi
}

# =============================================================================
# Contract Artifact Tests
# =============================================================================

test_contract_artifacts() {
    log_section "Contract Artifact Tests"

    # local artifact="${ROOT_DIR}/contracts/artifacts/cw_sdl.wasm"

    # # Test: Artifact exists
    # if [[ -f "$artifact" ]]; then
    #     local size
    #     size=$(ls -lh "$artifact" | awk '{print $5}')
    #     test_pass "artifact_exists" "SDL contract artifact exists ($size)"
    # else
    #     test_fail "artifact_exists" "SDL contract artifact missing"
    #     return 1
    # fi

    # # Test: Valid WASM magic bytes
    # local magic
    # magic=$(xxd -l 4 -p "$artifact" 2>/dev/null || echo "")
    # if [[ "$magic" == "0061736d" ]]; then
    #     test_pass "wasm_magic" "Valid WASM magic bytes"
    # else
    #     test_fail "wasm_magic" "Invalid WASM file" "Magic bytes: $magic"
    # fi
}

# =============================================================================
# Combined Network Test Suite
# =============================================================================

run_network_tests() {
    log_step "Running Network Tests"

    test_ergors_network

    if [[ "${SKIP_AKASH:-false}" != true ]]; then
        test_akash_network
    else
        log_warn "Skipping Akash network tests (SKIP_AKASH=true)"
    fi

    test_node_config
    test_contract_artifacts
}
