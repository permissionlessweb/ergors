#!/bin/bash
#
# ergors.sh - ERGORS network management for E2E tests
#
# Provides: node setup, config generation, CLI wrappers, identity management

# Prevent multiple sourcing
[[ -n "${_E2E_ERGORS_LOADED:-}" ]] && return 0
_E2E_ERGORS_LOADED=1

# =============================================================================
# Configuration
# =============================================================================
ERGORS_BIN="${ERGORS_BIN:-${ROOT_DIR}/target/release/ergors}"
# ERGORS_CLI="${ERGORS_CLI:-${ROOT_DIR}/target/release/ergors-cli}"
TEST_CUSTODY_PASSWORD="${TEST_CUSTODY_PASSWORD:-e2e-test-password-12345}"

# Network config
BASE_PORT="${BASE_PORT:-50100}"
COORDINATOR_GRPC=""
EXECUTOR_GRPC=""

# Process tracking
declare -a ERGORS_NODE_PIDS=()

# Identity cache
COORDINATOR_ADDRESS=""
EXECUTOR_ADDRESS=""

# =============================================================================
# Build
# =============================================================================

ergors_build() {
    log_step "Building ERGORS"

    cd "$ROOT_DIR" || return 1

    log "Building ergors binaries..."
    log_verbose "ERGORS_BIN=$ERGORS_BIN"
    # log_verbose "ERGORS_CLI=$ERGORS_CLI"

    if ! run_cmd_tail 10 cargo build --release -p ergors; then
        log_error "Build failed"
        return 1
    fi

    if [[ ! -f "$ERGORS_BIN" ]]; then
        log_error "Binaries not found after build"
        return 1
    fi

    log_verbose "Binary sizes: $(ls -lh "$ERGORS_BIN" 2>/dev/null | awk '{print $5, $9}' | tr '\n' ' ')"
    log_success "ERGORS binaries built"
}

# =============================================================================
# Contract Building
# =============================================================================

ergors_build_contracts() {
    log_step "Building CosmWasm Contracts"

    local artifact="${ROOT_DIR}/contracts/artifacts/cw_sdl.wasm"

    if [[ -f "$artifact" ]]; then
        log_success "Contract artifacts already exist"
        return 0
    fi

    cd "$ROOT_DIR/contracts" || return 1

    local arch
    arch=$(uname -m)
    log "Building for architecture: $arch"

    local optimizer_image
    if [[ "$arch" == "arm64" ]] || [[ "$arch" == "aarch64" ]]; then
        optimizer_image="cosmwasm/optimizer-arm64:0.17.0"
    else
        optimizer_image="cosmwasm/optimizer:0.17.0"
    fi

    log_verbose "Using optimizer image: $optimizer_image"

    if [[ "${VERBOSE:-false}" == "true" ]]; then
        docker run --rm -v "$(pwd)":/code \
            --mount type=volume,source="contracts_cache",target=/target \
            --mount type=volume,source=registry_cache,target=/usr/local/cargo/registry \
            "$optimizer_image" 2>&1
    else
        docker run --rm -v "$(pwd)":/code \
            --mount type=volume,source="contracts_cache",target=/target \
            --mount type=volume,source=registry_cache,target=/usr/local/cargo/registry \
            "$optimizer_image" 2>&1 | tail -10
    fi

    if [[ ! -f "$artifact" ]]; then
        log_error "Contract build failed"
        return 1
    fi

    log_success "Contracts built"
    cd "$ROOT_DIR"
}

# =============================================================================
# Node Configuration
# =============================================================================

_ergors_generate_config() {
    local node_id="$1"
    local node_type="$2"
    local http_port="$3"
    local p2p_port="$4"
    local home_dir="$5"

    mkdir -p "$home_dir/data" "$home_dir/wasm_cache"

    local sdl_args=""
    if [[ "$node_type" == "coordinator" ]]; then
        local wasm_src="${ROOT_DIR}/contracts/artifacts/cw_sdl.wasm"
        local wasm_dst="${home_dir}/cw_sdl.wasm"
        if [[ -f "$wasm_src" ]]; then
            cp "$wasm_src" "$wasm_dst"
            sdl_args="--with-sdl-contract --sdl-wasm-path ${wasm_dst}"
        fi
    fi

    # Generate config
    # shellcheck disable=SC2086
    "$ERGORS_BIN" --home "$home_dir" config init \
        --node-type "$node_type" \
        --api-port "$http_port" \
        --p2p-port "$p2p_port" \
        $sdl_args 2>&1 || return 1

    # Set additional config
    "$ERGORS_BIN" --home "$home_dir" config set identity.host "127.0.0.1" 2>&1 || true
    "$ERGORS_BIN" --home "$home_dir" config set identity.user "e2e-test" 2>&1 || true
    "$ERGORS_BIN" --home "$home_dir" config set storage.data_dir "${home_dir}/data" 2>&1 || true

    # Create .env file
    cat > "$home_dir/.env" <<EOF
NODE_DATA_PATH=${home_dir}
ERGORS_CUSTODY_PASSWORD=${TEST_CUSTODY_PASSWORD}
EOF
}

_ergors_init_node() {
    local home_dir="$1"
    local node_id="$2"

    export ERGORS_CUSTODY_PASSWORD="${TEST_CUSTODY_PASSWORD}"
    export NODE_DATA_PATH="${home_dir}"

    cd "$ROOT_DIR" || return 1

    # Initialize with empty API key prompts
    echo -e "\n\n\n\n\n" | "$ERGORS_BIN" --home "$home_dir" init new 2>&1 || {
        "$ERGORS_BIN" --home "$home_dir" init unsafe-wipe 2>&1 || true
        echo -e "\n\n\n\n\n" | "$ERGORS_BIN" --home "$home_dir" init new 2>&1 || true
    }

    if [[ -f "$home_dir/node_identity.enc" ]]; then
        log_success "Node '$node_id' initialized"
    else
        log_warn "Node '$node_id' using plaintext mode"
    fi
}

# Import faucet key into a node (internal helper)
_ergors_import_keys_to_node() {
    local home_dir="$1"
    local node_name="$2"

    # Get faucet mnemonic
    local mnemonic
    mnemonic=$(akash_get_faucet_mnemonic 2>/dev/null) || {
        log_warn "Could not get faucet mnemonic for $node_name, skipping key import"
        return 0
    }

    # Import mnemonic using ergors engine binary
    # ERGORS_CUSTODY_PASSWORD env var is read by the Rust code for non-interactive import
    local import_output
    import_output=$(ERGORS_CUSTODY_PASSWORD="${TEST_CUSTODY_PASSWORD}" \
        "$ERGORS_BIN" --home "$home_dir" keys import-mnemonic \
        --phrase "$mnemonic" \
        --label "E2E Faucet Key" \
        --key-name "faucet" \
        --chain-id "${AKASH_LOCAL_CHAIN_ID:-local}" \
        --address-prefix "akash" \
        --make-default 2>&1) || true

    # Check if import succeeded
    if echo "$import_output" | grep -q "akash1"; then
        local addr
        addr=$(echo "$import_output" | grep -o "akash1[a-z0-9]*" | head -1)
        log_verbose "$node_name key imported: $addr"
    else
        log_verbose "$node_name key import output: $import_output"
    fi
}

# =============================================================================
# Network Start/Stop
# =============================================================================

ergors_start_network() {
    log_step "Starting ERGORS Test Network"

    rm -rf "$TEST_DIR"
    mkdir -p "$TEST_DIR"

    local port=$BASE_PORT

    # === Coordinator ===
    local coord_home="$TEST_DIR/coordinator"
    local coord_http=$port
    local coord_grpc=$((port + 1))
    local coord_p2p=$((port + 2))
    COORDINATOR_GRPC="127.0.0.1:${coord_grpc}"
    port=$((port + 10))

    _ergors_init_node "$coord_home" "coordinator"
    _ergors_generate_config "coordinator" "coordinator" "$coord_http" "$coord_p2p" "$coord_home"
    _ergors_import_keys_to_node "$coord_home" "coordinator"

    log "Starting coordinator..."
    ERGORS_CUSTODY_PASSWORD="${TEST_CUSTODY_PASSWORD}" \
    NODE_DATA_PATH="${coord_home}" \
    "$ERGORS_BIN" --home "$coord_home" start --grpc-port "$coord_grpc" \
        > "$coord_home/node.log" 2>&1 &
    ERGORS_NODE_PIDS+=($!)

    # === Executor ===
    local exec_home="$TEST_DIR/executor_0"
    local exec_http=$port
    local exec_grpc=$((port + 1))
    local exec_p2p=$((port + 2))
    EXECUTOR_GRPC="127.0.0.1:${exec_grpc}"
    port=$((port + 10))

    _ergors_init_node "$exec_home" "executor_0"
    _ergors_generate_config "executor_0" "executor" "$exec_http" "$exec_p2p" "$exec_home"
    _ergors_import_keys_to_node "$exec_home" "executor"

    log "Starting executor..."
    ERGORS_CUSTODY_PASSWORD="${TEST_CUSTODY_PASSWORD}" \
    NODE_DATA_PATH="${exec_home}" \
    "$ERGORS_BIN" --home "$exec_home" start --grpc-port "$exec_grpc" \
        > "$exec_home/node.log" 2>&1 &
    ERGORS_NODE_PIDS+=($!)

    # Wait for nodes to be ready (check gRPC ports)
    sleep 2  # Brief startup delay

    if ! wait_for_port "127.0.0.1" "$coord_grpc" 30; then
        log_error "Coordinator failed to start"
        return 1
    fi

    if ! wait_for_port "127.0.0.1" "$exec_grpc" 30; then
        log_error "Executor failed to start"
        return 1
    fi

    log_success "ERGORS network started"
    log "  Coordinator gRPC: $COORDINATOR_GRPC"
    log "  Executor gRPC:    $EXECUTOR_GRPC"

    # Verbose: show config and process info
    if [[ "${VERBOSE:-false}" == "true" ]]; then
        log_verbose "Coordinator config:"
        log_debug "$(head -30 "$coord_home/config.toml" 2>/dev/null || echo '  (config not found)')"
        log_verbose "Node PIDs: ${ERGORS_NODE_PIDS[*]}"
        log_verbose "Test directory: $TEST_DIR"
    fi
}

ergors_stop_network() {
    log "Stopping ERGORS network..."

    for pid in "${ERGORS_NODE_PIDS[@]}"; do
        if kill -0 "$pid" 2>/dev/null; then
            kill "$pid" 2>/dev/null || true
        fi
    done
    ERGORS_NODE_PIDS=()

    log_success "ERGORS network stopped"
}

# =============================================================================
# Node Health Checks (no log grepping!)
# =============================================================================

# Check if coordinator is healthy via gRPC
ergors_coordinator_healthy() {
    [[ -n "$COORDINATOR_GRPC" ]] && nc -z 127.0.0.1 "${COORDINATOR_GRPC##*:}" 2>/dev/null
}

# Check if executor is healthy via gRPC
ergors_executor_healthy() {
    [[ -n "$EXECUTOR_GRPC" ]] && nc -z 127.0.0.1 "${EXECUTOR_GRPC##*:}" 2>/dev/null
}

# Check if all nodes are running
ergors_all_nodes_running() {
    local running=0
    for pid in "${ERGORS_NODE_PIDS[@]}"; do
        if kill -0 "$pid" 2>/dev/null; then
            running=$((running + 1))
        fi
    done
    [[ $running -eq ${#ERGORS_NODE_PIDS[@]} ]] && [[ $running -gt 0 ]]
}

# =============================================================================
# CLI Wrappers
# =============================================================================

# Run ergors-cli command against coordinator
# ergors_cli() {
#     if [[ ! -f "$ERGORS_CLI" ]]; then
#         echo '{"error":"ergors-cli not found"}'
#         return 1
#     fi

#     "$ERGORS_CLI" --grpc-addr "http://${COORDINATOR_GRPC}" --json "$@"
# }

# Run ergors-cli command against executor
# ergors_cli_executor() {
#     if [[ ! -f "$ERGORS_CLI" ]]; then
#         echo '{"error":"ergors-cli not found"}'
#         return 1
#     fi

#     "$ERGORS_CLI" --grpc-addr "http://${EXECUTOR_GRPC}" --json "$@"
# }

# =============================================================================
# Identity Management
# =============================================================================

# Get and cache node addresses
ergors_get_addresses() {
    if [[ -n "$COORDINATOR_ADDRESS" ]] && [[ -n "$EXECUTOR_ADDRESS" ]]; then
        return 0  # Already cached
    fi

    log "Querying node addresses..."

    local coord_info
    coord_info=$(ergors_cli node info 2>&1) || true
    log_verbose "Coordinator node info: $coord_info"
    COORDINATOR_ADDRESS=$(json_get "$coord_info" '.cosmos_address')

    if [[ -z "$COORDINATOR_ADDRESS" ]]; then
        log_error "Failed to get coordinator address"
        return 1
    fi

    local exec_info
    exec_info=$(ergors_cli_executor node info 2>&1) || true
    log_verbose "Executor node info: $exec_info"
    EXECUTOR_ADDRESS=$(json_get "$exec_info" '.cosmos_address')

    if [[ -z "$EXECUTOR_ADDRESS" ]]; then
        log_error "Failed to get executor address"
        return 1
    fi

    log "  Coordinator: $COORDINATOR_ADDRESS"
    log "  Executor:    $EXECUTOR_ADDRESS"
    return 0
}

# Import the Akash faucet mnemonic into the coordinator node
# This gives the coordinator a pre-funded key (10B AKT from genesis)
# Uses: ergors keys import-mnemonic (the engine binary, not ergors-cli)
ergors_import_faucet_key() {
    local key_name="${1:-faucet}"
    local home_dir="${2:-$TEST_DIR/coordinator}"

    log "Importing faucet mnemonic into coordinator..."

    local mnemonic
    mnemonic=$(akash_get_faucet_mnemonic) || {
        log_error "Could not get faucet mnemonic"
        return 1
    }

    log_verbose "Mnemonic word count: $(echo "$mnemonic" | wc -w | tr -d ' ')"

    # Import mnemonic using ergors engine binary
    # Requires ERGORS_CUSTODY_PASSWORD for key encryption
    local import_output
    import_output=$(ERGORS_CUSTODY_PASSWORD="${TEST_CUSTODY_PASSWORD}" \
        "$ERGORS_BIN" --home "$home_dir" keys import-mnemonic \
        --phrase "$mnemonic" \
        --label "E2E Faucet Key" \
        --key-name "$key_name" \
        --chain-id "${AKASH_LOCAL_CHAIN_ID:-local}" \
        --address-prefix "akash" \
        --make-default 2>&1) || true

    log_verbose "Import output: $import_output"

    # Verify key was imported by listing keys
    local keys_output
    keys_output=$(ERGORS_CUSTODY_PASSWORD="${TEST_CUSTODY_PASSWORD}" \
        "$ERGORS_BIN" --home "$home_dir" keys list 2>&1) || true

    log_verbose "Keys list: $keys_output"

    # Extract faucet address from keys list output
    if echo "$keys_output" | grep -q "$key_name"; then
        # Parse address from keys list (format: NAME  LABEL  ADDRESS  CHAIN  DEFAULT)
        local addr
        addr=$(echo "$keys_output" | grep "$key_name" | awk '{print $3}')

        if [[ -n "$addr" ]] && [[ "$addr" == akash* ]]; then
            log_success "Faucet key imported: $addr"
            export FAUCET_ADDRESS="$addr"
            return 0
        fi
    fi

    # Fallback: check if import message contains address
    if echo "$import_output" | grep -q "akash1"; then
        local addr
        addr=$(echo "$import_output" | grep -o "akash1[a-z0-9]*" | head -1)
        if [[ -n "$addr" ]]; then
            log_success "Faucet key imported: $addr"
            export FAUCET_ADDRESS="$addr"
            return 0
        fi
    fi

    log_error "Failed to verify imported key"
    return 1
}

# =============================================================================
# Deployment Commands
# =============================================================================

ergors_deploy_create() {
    local sdl="$1"
    local key_name="${2:-default}"

    ergors_cli deploy create \
        --sdl "$sdl" \
        --key-name "$key_name" \
        --node "${AKASH_LOCAL_NODE}" \
        --chain-id "${AKASH_LOCAL_CHAIN_ID}" \
        2>&1
}

ergors_deploy_list() {
    ergors_cli deploy list 2>&1
}

ergors_deploy_get() {
    local session_id="$1"
    ergors_cli deploy get "$session_id" 2>&1
}

ergors_deploy_bids() {
    local session_id="$1"
    ergors_cli deploy bids "$session_id" 2>&1
}

ergors_deploy_select() {
    local session_id="$1"
    local provider="$2"
    local price="$3"

    ergors_cli deploy select "$session_id" \
        --provider "$provider" \
        --price "$price" \
        2>&1
}

ergors_deploy_advance() {
    local session_id="$1"
    ergors_cli deploy advance "$session_id" 2>&1
}

# =============================================================================
# Grant Commands (via deploy subcommand)
# =============================================================================

ergors_grant_request() {
    local granter="$1"
    local grantee="$2"
    local allowance="${3:-10000000}"
    local reason="${4:-E2E test}"

    ergors_cli_executor deploy request-grant \
        --granter "$granter" \
        --grantee "$grantee" \
        --msg-type "/akash.deployment.v1beta3.MsgCreateDeployment" \
        --msg-type "/akash.deployment.v1beta3.MsgDepositDeployment" \
        --msg-type "/akash.market.v1beta3.MsgCreateLease" \
        --allowance "$allowance" \
        --reason "$reason" \
        2>&1
}

ergors_grant_approve() {
    local request_id="$1"
    local reason="${2:-Approved for testing}"

    ergors_cli deploy approve-grant "$request_id" --reason "$reason" 2>&1
}

# =============================================================================
# SDL Template Commands
# =============================================================================

ergors_sdl_list() {
    ergors_cli sdl list 2>&1
}

ergors_sdl_get_template() {
    local contract="$1"
    ergors_cli sdl get-template "$contract" 2>&1
}

ergors_sdl_render() {
    local contract="$1"
    shift
    ergors_cli sdl render "$contract" "$@" 2>&1
}
