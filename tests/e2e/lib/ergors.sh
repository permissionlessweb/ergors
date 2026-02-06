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
TEST_CUSTODY_PASSWORD="${TEST_CUSTODY_PASSWORD:-e2e-test-password-12345}"

# Network config
BASE_PORT="${BASE_PORT:-50100}"
COORDINATOR_GRPC=""
COORDINATOR_API=""
EXECUTOR_GRPC=""
EXECUTOR_API=""

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

_ergors_configure_local_chain() {
    local home_dir="$1"
    local rpc_endpoint="${2:-http://127.0.0.1:26657}"
    local grpc_endpoint="${3:-http://127.0.0.1:9090}"

    log_verbose "Configuring local chain: rpc=$rpc_endpoint, grpc=$grpc_endpoint"

    ERGORS_CUSTODY_PASSWORD="${TEST_CUSTODY_PASSWORD}" \
        "$ERGORS_BIN" --home "$home_dir" config set-chain local \
        --name "Akash Local" \
        --prefix "akash" \
        --denom "uakt" \
        --rpc "$rpc_endpoint" \
        --grpc "$grpc_endpoint" \
        --gas-prices "0.025uakt" \
        --gas-adjustment "1.5" \
        --keyring-backend "test" \
        --default-key "default" 2>&1 || return 1
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
    # Both ERGORS_CUSTODY_PASSWORD and ERGORS_MNEMONIC env vars enable non-interactive import
    local import_output
    import_output=$(ERGORS_CUSTODY_PASSWORD="${TEST_CUSTODY_PASSWORD}" \
        ERGORS_MNEMONIC="$mnemonic" \
        "$ERGORS_BIN" --home "$home_dir" keys import-mnemonic \
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
    COORDINATOR_API="127.0.0.1:${coord_http}"
    port=$((port + 10))

    _ergors_init_node "$coord_home" "coordinator"
    _ergors_generate_config "coordinator" "coordinator" "$coord_http" "$coord_p2p" "$coord_home"
    _ergors_import_keys_to_node "$coord_home" "coordinator"

    # Configure LLM entities BEFORE starting coordinator (if API keys are available)
    if [[ -n "${OPENAI_API_KEY:-}" ]] && [[ -n "${ANTHROPIC_API_KEY:-}" ]] && [[ -n "${OLLAMA_API_KEY:-}" ]]; then
        log "Configuring LLM entities for coordinator..."
        ergors_configure_llm_entities "$coord_home" "$OPENAI_API_KEY" "$ANTHROPIC_API_KEY" "$OLLAMA_API_KEY"
    fi

    log "Starting coordinator..."

    # Start coordinator with live logs (use tee to both display and save)
    ERGORS_CUSTODY_PASSWORD="${TEST_CUSTODY_PASSWORD}" \
    NODE_DATA_PATH="${coord_home}" \
    OPENAI_API_KEY="${OPENAI_API_KEY:-}" \
    ANTHROPIC_API_KEY="${ANTHROPIC_API_KEY:-}" \
    OLLAMA_API_KEY="${OLLAMA_API_KEY:-}" \
    MOCK_PROVIDER_URL="${MOCK_PROVIDER_URL:-}" \
    RUST_LOG="${RUST_LOG:-info}" \
    "$ERGORS_BIN" --home "$coord_home" start --grpc-port "$coord_grpc" 2>&1 | \
        tee "$coord_home/node.log" &
    ERGORS_NODE_PIDS+=($!)
    register_pid $!

    # === Executor ===
    local exec_home="$TEST_DIR/executor_0"
    local exec_http=$port
    local exec_grpc=$((port + 1))
    local exec_p2p=$((port + 2))
    EXECUTOR_GRPC="127.0.0.1:${exec_grpc}"
    EXECUTOR_API="127.0.0.1:${exec_http}"
    port=$((port + 10))

    _ergors_init_node "$exec_home" "executor_0"
    _ergors_generate_config "executor_0" "executor" "$exec_http" "$exec_p2p" "$exec_home"
    _ergors_import_keys_to_node "$exec_home" "executor"

    # Configure LLM entities BEFORE starting executor (if API keys are available)
    if [[ -n "${OPENAI_API_KEY:-}" ]] && [[ -n "${ANTHROPIC_API_KEY:-}" ]] && [[ -n "${OLLAMA_API_KEY:-}" ]]; then
        log "Configuring LLM entities for executor..."
        ergors_configure_llm_entities "$exec_home" "$OPENAI_API_KEY" "$ANTHROPIC_API_KEY" "$OLLAMA_API_KEY"
    fi

    log "Starting executor..."

    # Start executor with live logs (use tee to both display and save)
    ERGORS_CUSTODY_PASSWORD="${TEST_CUSTODY_PASSWORD}" \
    NODE_DATA_PATH="${exec_home}" \
    OPENAI_API_KEY="${OPENAI_API_KEY:-}" \
    ANTHROPIC_API_KEY="${ANTHROPIC_API_KEY:-}" \
    OLLAMA_API_KEY="${OLLAMA_API_KEY:-}" \
    MOCK_PROVIDER_URL="${MOCK_PROVIDER_URL:-}" \
    RUST_LOG="${RUST_LOG:-info}" \
    "$ERGORS_BIN" --home "$exec_home" start --grpc-port "$exec_grpc" 2>&1 | \
        tee "$exec_home/node.log" &
    ERGORS_NODE_PIDS+=($!)
    register_pid $!

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

    # Configure local Akash chain in cnidarium
    log "Configuring local Akash chain..."
    if ! _ergors_configure_local_chain "$coord_home" "http://127.0.0.1:26657" "http://127.0.0.1:9090"; then
        log_warn "Failed to configure local chain (will use defaults)"
    else
        log_success "Local chain configured"
    fi

    # Export for child scripts
    export COORDINATOR_GRPC COORDINATOR_API EXECUTOR_GRPC EXECUTOR_API

    log_success "ERGORS network started"
    log "  Coordinator gRPC: $COORDINATOR_GRPC"
    log "  Coordinator API:  $COORDINATOR_API"
    log "  Executor gRPC:    $EXECUTOR_GRPC"
    log "  Executor API:     $EXECUTOR_API"

    # Verbose: show config, process info, and initial logs
    if [[ "${VERBOSE:-false}" == "true" ]]; then
        log_verbose "Coordinator config:"
        log_debug "$(head -30 "$coord_home/config.toml" 2>/dev/null || echo '  (config not found)')"
        log_verbose "Node PIDs: ${ERGORS_NODE_PIDS[*]}"
        log_verbose "Test directory: $TEST_DIR"

        # Show startup logs
        log_verbose ""
        log_verbose "=== Initial Engine Logs ==="
        if [[ -f "$coord_home/node.log" ]]; then
            log_verbose "--- Coordinator startup log ---"
            head -50 "$coord_home/node.log" 2>/dev/null || true
        fi
        if [[ -f "$exec_home/node.log" ]]; then
            log_verbose "--- Executor startup log ---"
            head -50 "$exec_home/node.log" 2>/dev/null || true
        fi
        log_verbose "=== End Initial Logs ==="
        log_verbose ""
    fi
}

ergors_stop_network() {
    log "Stopping ERGORS network..."

    # Kill all tracked ERGORS node processes with timeout and SIGKILL fallback
    for pid in "${ERGORS_NODE_PIDS[@]}"; do
        if [[ -n "$pid" ]]; then
            log_verbose "Stopping ERGORS node PID: $pid"
            kill_with_timeout "$pid" 5
        fi
    done
    ERGORS_NODE_PIDS=()

    # Kill any orphaned ergors processes related to our test directory
    if [[ -n "${TEST_DIR:-}" ]]; then
        kill_by_pattern "ergors.*--home.*${TEST_DIR}" 2>/dev/null || true
    fi

    # Clean up ERGORS ports (in case processes were orphaned)
    local ergors_ports=(50100 50101 50102 50110 50111 50112)
    for port in "${ergors_ports[@]}"; do
        kill_port "$port" 2>/dev/null || true
    done

    log_success "ERGORS network stopped"
}

# =============================================================================
# Node Health Checks (no log grepping!)
# =============================================================================

# Check if coordinator is healthy via gRPC port
ergors_coordinator_healthy() {
    [[ -n "$COORDINATOR_GRPC" ]] && nc -z 127.0.0.1 "${COORDINATOR_GRPC##*:}" 2>/dev/null
}

# Check if executor is healthy via gRPC port
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

# Display engine logs for debugging (called on failures)
# Shows last N lines of coordinator and executor logs
display_engine_logs() {
    local lines="${1:-50}"
    local coord_log="$TEST_DIR/coordinator/node.log"
    local exec_log="$TEST_DIR/executor_0/node.log"

    echo ""
    log_error "=== ENGINE LOGS (last $lines lines) ==="

    if [[ -f "$coord_log" ]]; then
        echo ""
        echo -e "${RED}--- Coordinator Log ($coord_log) ---${NC}"
        tail -"$lines" "$coord_log" 2>/dev/null || echo "  (could not read log)"
    else
        echo -e "${YELLOW}  Coordinator log not found${NC}"
    fi

    if [[ -f "$exec_log" ]]; then
        echo ""
        echo -e "${RED}--- Executor Log ($exec_log) ---${NC}"
        tail -"$lines" "$exec_log" 2>/dev/null || echo "  (could not read log)"
    fi

    echo ""
    log_error "=== END ENGINE LOGS ==="
    echo ""
}

# Check engine process status and report
check_engine_status() {
    local coord_pid="${ERGORS_NODE_PIDS[0]:-}"
    local exec_pid="${ERGORS_NODE_PIDS[1]:-}"

    echo ""
    log "Engine Process Status:"

    if [[ -n "$coord_pid" ]]; then
        if kill -0 "$coord_pid" 2>/dev/null; then
            log_success "  Coordinator (PID $coord_pid): RUNNING"
        else
            log_error "  Coordinator (PID $coord_pid): DEAD"
        fi
    else
        log_warn "  Coordinator: No PID tracked"
    fi

    if [[ -n "$exec_pid" ]]; then
        if kill -0 "$exec_pid" 2>/dev/null; then
            log_success "  Executor (PID $exec_pid): RUNNING"
        else
            log_error "  Executor (PID $exec_pid): DEAD"
        fi
    else
        log_warn "  Executor: No PID tracked"
    fi

    # Check ports
    if ergors_coordinator_healthy; then
        log_success "  Coordinator gRPC: LISTENING"
    else
        log_error "  Coordinator gRPC: NOT RESPONDING"
    fi

    if ergors_executor_healthy; then
        log_success "  Executor gRPC: LISTENING"
    else
        log_error "  Executor gRPC: NOT RESPONDING"
    fi
    echo ""
}

# =============================================================================
# CLI Wrappers
# =============================================================================

# Run ergors command against coordinator
# Routes to either ergors binary or HTTP API based on subcommand
# CLI commands that need gRPC use COORDINATOR_GRPC address
ergors_cli() {
    local coord_home="${TEST_DIR}/coordinator"
    local subcommand="${1:-}"

    # Build gRPC address for CLI commands
    local grpc_addr="http://${COORDINATOR_GRPC}"

    case "$subcommand" in
        node)
            # Node commands use HTTP API (topology endpoint)
            shift
            _ergors_node_api "coordinator" "$@"
            ;;
        deploy|sdl|bootstrap)
            # Deploy, SDL, and Bootstrap commands use CLI binary (connects to gRPC server)
            ERGORS_CUSTODY_PASSWORD="${TEST_CUSTODY_PASSWORD}" \
                "$ERGORS_BIN" --home "$coord_home" --grpc-addr "$grpc_addr" "$@" 2>&1
            ;;
        keys)
            # Keys commands use the ergors binary (local, no gRPC needed)
            ERGORS_CUSTODY_PASSWORD="${TEST_CUSTODY_PASSWORD}" \
                "$ERGORS_BIN" --home "$coord_home" keys "$@" 2>&1
            ;;
        *)
            # Default: try as ergors binary subcommand with gRPC
            ERGORS_CUSTODY_PASSWORD="${TEST_CUSTODY_PASSWORD}" \
                "$ERGORS_BIN" --home "$coord_home" --grpc-addr "$grpc_addr" "$@" 2>&1
            ;;
    esac
}

# Run ergors command against executor
ergors_cli_executor() {
    local exec_home="${TEST_DIR}/executor_0"
    local subcommand="${1:-}"

    # Build gRPC address for CLI commands
    local grpc_addr="http://${EXECUTOR_GRPC}"

    case "$subcommand" in
        node)
            shift
            _ergors_node_api "executor" "$@"
            ;;
        deploy|sdl|bootstrap)
            # CLI commands that need gRPC
            ERGORS_CUSTODY_PASSWORD="${TEST_CUSTODY_PASSWORD}" \
                "$ERGORS_BIN" --home "$exec_home" --grpc-addr "$grpc_addr" "$@" 2>&1
            ;;
        keys)
            # Keys commands are local (no gRPC needed)
            ERGORS_CUSTODY_PASSWORD="${TEST_CUSTODY_PASSWORD}" \
                "$ERGORS_BIN" --home "$exec_home" keys "$@" 2>&1
            ;;
        *)
            ERGORS_CUSTODY_PASSWORD="${TEST_CUSTODY_PASSWORD}" \
                "$ERGORS_BIN" --home "$exec_home" --grpc-addr "$grpc_addr" "$@" 2>&1
            ;;
    esac
}

# Internal: Node API calls via HTTP
# Uses /network/topology which returns node_identity info
_ergors_node_api() {
    local node_type="$1"
    local action="$2"
    shift 2

    local api_host
    if [[ "$node_type" == "coordinator" ]]; then
        api_host="$COORDINATOR_API"
    else
        api_host="$EXECUTOR_API"
    fi

    case "$action" in
        info)
            # /network/topology returns node_identity with node_type, node_id, etc.
            local response
            response=$(curl -s --max-time 10 -X GET "http://${api_host}/network/topology" \
                -H "Content-Type: application/json" 2>/dev/null) || echo '{"error":"request failed"}'
            # Extract node_identity from topology response
            if echo "$response" | jq -e '.node_identity' >/dev/null 2>&1; then
                echo "$response" | jq '.node_identity'
            else
                echo "$response"
            fi
            ;;
        address)
            # Parse --prefix argument (default to ergors, not akash)
            local prefix="ergors"
            while [[ $# -gt 0 ]]; do
                case "$1" in
                    --prefix) prefix="$2"; shift 2 ;;
                    *) shift ;;
                esac
            done
            # Get address from topology response
            local response
            response=$(curl -s --max-time 10 -X GET "http://${api_host}/network/topology" \
                -H "Content-Type: application/json" 2>/dev/null) || echo '{"error":"request failed"}'
            # Extract node_id and format as address response
            local node_id
            node_id=$(echo "$response" | jq -r '.node_identity.node_id // empty' 2>/dev/null)
            if [[ -n "$node_id" ]]; then
                # Node ID is the hex pubkey - for now just return it
                # TODO: actual bech32 encoding with prefix
                echo "{\"address\": \"${prefix}1${node_id:0:38}\"}"
            else
                echo '{"error":"could not extract node address"}'
            fi
            ;;
        *)
            echo "{\"error\": \"Unknown node action: $action\"}"
            ;;
    esac
}

# Internal: Deploy API calls via HTTP
_ergors_deploy_api() {
    local node_type="$1"
    local action="$2"
    shift 2

    local api_host
    if [[ "$node_type" == "coordinator" ]]; then
        api_host="$COORDINATOR_API"
    else
        api_host="$EXECUTOR_API"
    fi

    case "$action" in
        register-token)
            # Parse --label argument
            local label=""
            while [[ $# -gt 0 ]]; do
                case "$1" in
                    --label) label="$2"; shift 2 ;;
                    *) shift ;;
                esac
            done
            curl -s --max-time 15 -X POST "http://${api_host}/api/tokens" \
                -H "Content-Type: application/json" \
                -d "{\"label\": \"${label}\"}" 2>/dev/null || echo '{"error":"request failed"}'
            ;;
        list-tokens)
            curl -s --max-time 15 -X GET "http://${api_host}/api/tokens" \
                -H "Content-Type: application/json" 2>/dev/null || echo '{"error":"request failed"}'
            ;;
        revoke-token)
            local token_id="${1:-}"
            curl -s --max-time 15 -X DELETE "http://${api_host}/api/tokens/${token_id}" \
                -H "Content-Type: application/json" 2>/dev/null || echo '{"error":"request failed"}'
            ;;
        request-grant)
            # Forward as JSON body
            local body="{}"
            while [[ $# -gt 0 ]]; do
                case "$1" in
                    --granter) body=$(echo "$body" | jq --arg v "$2" '. + {granter: $v}'); shift 2 ;;
                    --grantee) body=$(echo "$body" | jq --arg v "$2" '. + {grantee: $v}'); shift 2 ;;
                    --allowance) body=$(echo "$body" | jq --arg v "$2" '. + {allowance: $v}'); shift 2 ;;
                    --reason) body=$(echo "$body" | jq --arg v "$2" '. + {reason: $v}'); shift 2 ;;
                    *) shift ;;
                esac
            done
            curl -s --max-time 15 -X POST "http://${api_host}/api/grants/request" \
                -H "Content-Type: application/json" \
                -d "$body" 2>/dev/null || echo '{"error":"request failed"}'
            ;;
        *)
            echo "{\"error\": \"Unknown deploy action: $action\"}"
            ;;
    esac
}

# Internal: CosmWasm query via HTTP (generic)
_ergors_cw_query_api() {
    local node_type="$1"
    local contract="$2"
    local query="$3"

    local api_host
    if [[ "$node_type" == "coordinator" ]]; then
        api_host="$COORDINATOR_API"
    else
        api_host="$EXECUTOR_API"
    fi

    curl -s --max-time 15 -X POST "http://${api_host}/api/cosmwasm/query" \
        -H "Content-Type: application/json" \
        -d "{\"contract\": \"$contract\", \"query\": $query}" 2>/dev/null || echo '{"error":"request failed"}'
}

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
# Uses: ergors keys import-mnemonic
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
        ERGORS_MNEMONIC="$mnemonic" \
        "$ERGORS_BIN" --home "$home_dir" keys import-mnemonic \
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
    ergors_cli deploy run "$session_id" 2>&1
}

ergors_deploy_status() {
    local session_id="$1"
    ergors_cli deploy status "$session_id" 2>&1
}

ergors_deploy_close() {
    local session_id="$1"
    ergors_cli deploy close-lease "$session_id" 2>&1
}

ergors_deploy_cancel() {
    local session_id="$1"
    ergors_cli deploy cancel "$session_id" 2>&1
}

ergors_deploy_query_balance() {
    local address="$1"
    local denom="${2:-uakt}"
    ergors_cli deploy query-balance "$address" --denom "$denom" 2>&1
}

# =============================================================================
# Provider Management Commands
# =============================================================================

ergors_trusted_providers() {
    ergors_cli deploy trusted-providers 2>&1
}

ergors_add_provider() {
    local address="$1"
    local label="${2:-}"
    if [[ -n "$label" ]]; then
        ergors_cli deploy add-provider "$address" --label "$label" 2>&1
    else
        ergors_cli deploy add-provider "$address" 2>&1
    fi
}

ergors_remove_provider() {
    local address="$1"
    ergors_cli deploy remove-provider "$address" 2>&1
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
# CosmWasm Commands (via HTTP API)
# =============================================================================

# Generic CosmWasm contract query
# Usage: ergors_cw_query <contract_address> <query_json>
ergors_cw_query() {
    local contract="$1"
    local query="$2"

    curl -s --max-time 15 -X POST "http://${COORDINATOR_API}/api/cosmwasm/query" \
        -H "Content-Type: application/json" \
        -d "{\"contract\": \"$contract\", \"query\": $query}" 2>/dev/null || echo '{"error":"request failed"}'
}

# Generic CosmWasm contract execute
# Usage: ergors_cw_execute <contract_address> <sender> <msg_json> [funds_json]
# funds_json format: [{"denom":"uakt","amount":"1000000"}]
ergors_cw_execute() {
    local contract="$1"
    local sender="$2"
    local msg="$3"
    local funds="${4:-[]}"

    curl -s --max-time 30 -X POST "http://${COORDINATOR_API}/api/cosmwasm/execute" \
        -H "Content-Type: application/json" \
        -d "{\"contract\": \"$contract\", \"sender\": \"$sender\", \"msg\": $msg, \"funds\": $funds}" \
        2>/dev/null || echo '{"error":"request failed"}'
}

# Generic CosmWasm code store (upload)
# Usage: ergors_cw_store <sender> <wasm_base64>
ergors_cw_store() {
    local sender="$1"
    local wasm_base64="$2"

    curl -s --max-time 60 -X POST "http://${COORDINATOR_API}/api/cosmwasm/store" \
        -H "Content-Type: application/json" \
        -d "{\"sender\": \"$sender\", \"wasm_byte_code\": \"$wasm_base64\"}" \
        2>/dev/null || echo '{"error":"request failed"}'
}

# Generic CosmWasm contract instantiate
# Usage: ergors_cw_instantiate <code_id> <sender> <label> <msg_json> [admin] [funds_json]
ergors_cw_instantiate() {
    local code_id="$1"
    local sender="$2"
    local label="$3"
    local msg="$4"
    local admin="${5:-null}"
    local funds="${6:-[]}"

    # Handle admin field (null or string)
    local admin_json
    if [[ "$admin" == "null" ]] || [[ -z "$admin" ]]; then
        admin_json="null"
    else
        admin_json="\"$admin\""
    fi

    curl -s --max-time 30 -X POST "http://${COORDINATOR_API}/api/cosmwasm/instantiate" \
        -H "Content-Type: application/json" \
        -d "{\"code_id\": $code_id, \"sender\": \"$sender\", \"admin\": $admin_json, \"label\": \"$label\", \"msg\": $msg, \"funds\": $funds}" \
        2>/dev/null || echo '{"error":"request failed"}'
}

# Generic CosmWasm contract instantiate2 (predictable address)
# Usage: ergors_cw_instantiate2 <code_id> <sender> <label> <msg_json> <salt_base64> [admin] [funds_json]
ergors_cw_instantiate2() {
    local code_id="$1"
    local sender="$2"
    local label="$3"
    local msg="$4"
    local salt="$5"
    local admin="${6:-null}"
    local funds="${7:-[]}"

    # Handle admin field (null or string)
    local admin_json
    if [[ "$admin" == "null" ]] || [[ -z "$admin" ]]; then
        admin_json="null"
    else
        admin_json="\"$admin\""
    fi

    curl -s --max-time 30 -X POST "http://${COORDINATOR_API}/api/cosmwasm/instantiate2" \
        -H "Content-Type: application/json" \
        -d "{\"code_id\": $code_id, \"sender\": \"$sender\", \"admin\": $admin_json, \"label\": \"$label\", \"msg\": $msg, \"salt\": \"$salt\", \"funds\": $funds}" \
        2>/dev/null || echo '{"error":"request failed"}'
}

# =============================================================================
# Grant Configuration Commands (GranterService / GrantRequesterService)
# =============================================================================

# Configure granter acceptance mode (auto, manual, whitelist)
# Usage: ergors_grant_configure_mode <mode>
ergors_grant_configure_mode() {
    local mode="$1"
    ergors_cli deploy grant-config --acceptance-mode "$mode" 2>&1
}

# Add address to granter whitelist
# Usage: ergors_grant_whitelist_add <address>
ergors_grant_whitelist_add() {
    local address="$1"
    ergors_cli deploy grant-whitelist add "$address" 2>&1
}

# Remove address from granter whitelist
# Usage: ergors_grant_whitelist_remove <address>
ergors_grant_whitelist_remove() {
    local address="$1"
    ergors_cli deploy grant-whitelist remove "$address" 2>&1
}

# Check if address is on granter whitelist
# Usage: ergors_grant_whitelist_check <address>
ergors_grant_whitelist_check() {
    local address="$1"
    ergors_cli deploy grant-whitelist check "$address" 2>&1
}

# Set spending limit for a grantee
# Usage: ergors_grant_set_spending_limit <grantee_address> <limit_uakt>
ergors_grant_set_spending_limit() {
    local grantee="$1"
    local limit="$2"
    ergors_cli deploy grant-spending-limit \
        --grantee "$grantee" \
        --limit "$limit" \
        2>&1
}

# Query spending for a grantee
# Usage: ergors_grant_query_spending <grantee_address>
ergors_grant_query_spending() {
    local grantee="$1"
    ergors_cli deploy grant-spending \
        --grantee "$grantee" \
        2>&1
}

# =============================================================================
# Bootstrap Commands
# =============================================================================

# Generate bootstrap configuration
# Usage: ergors_bootstrap_config_generate [--node-type TYPE] [--target-name NAME] [--image-tag TAG]
ergors_bootstrap_config_generate() {
    ergors_cli bootstrap config-generate "$@" 2>&1
}

# Generate SDL from bootstrap configuration
# Usage: ergors_bootstrap_sdl_generate [--node-type TYPE] [--target-name NAME] [--image-tag TAG] [--cpu N] [--memory SIZE] [--storage SIZE]
ergors_bootstrap_sdl_generate() {
    ergors_cli bootstrap sdl-generate "$@" 2>&1
}

# Initiate a bootstrap workflow
# Usage: ergors_bootstrap_initiate [--node-type TYPE] [--target-name NAME] [--image-tag TAG]
ergors_bootstrap_initiate() {
    ergors_cli bootstrap initiate "$@" 2>&1
}

# Query bootstrap workflow status
# Usage: ergors_bootstrap_status <workflow_id>
ergors_bootstrap_status() {
    local workflow_id="$1"
    ergors_cli bootstrap status "$workflow_id" 2>&1
}

# List all bootstrap workflows
# Usage: ergors_bootstrap_list
ergors_bootstrap_list() {
    ergors_cli bootstrap list 2>&1
}

# Cancel a bootstrap workflow
# Usage: ergors_bootstrap_cancel <workflow_id>
ergors_bootstrap_cancel() {
    local workflow_id="$1"
    ergors_cli bootstrap cancel "$workflow_id" 2>&1
}

# =============================================================================
# SDL Template Commands (built on generic CosmWasm query)
# =============================================================================

# List SDL template contracts (queries storage via gRPC)
ergors_sdl_list() {
    # This queries the engine via gRPC for registered SDL contracts
    # Returns contracts with addresses, labels, and code_ids
    local coord_home="${TEST_DIR}/coordinator"
    local grpc_addr="http://${COORDINATOR_GRPC}"

    # Use the ergors binary to list contracts from engine (requires gRPC connection)
    ERGORS_CUSTODY_PASSWORD="${TEST_CUSTODY_PASSWORD}" \
        "$ERGORS_BIN" --home "$coord_home" --grpc-addr "$grpc_addr" sdl list 2>&1
}

# =============================================================================
# Chain Config Functions
# =============================================================================

ergors_config_get_chain() {
    local chain_id="$1"
    local coord_home="${TEST_DIR}/coordinator"

    ERGORS_CUSTODY_PASSWORD="${TEST_CUSTODY_PASSWORD}" \
        "$ERGORS_BIN" --home "$coord_home" config get-chain "$chain_id" 2>&1
}

ergors_config_list_chains() {
    local coord_home="${TEST_DIR}/coordinator"

    ERGORS_CUSTODY_PASSWORD="${TEST_CUSTODY_PASSWORD}" \
        "$ERGORS_BIN" --home "$coord_home" config list-chains 2>&1
}

# =============================================================================
# Mock Inference Provider
# =============================================================================

MOCK_PROVIDER_PID=""
MOCK_PROVIDER_URL=""
MOCK_PROVIDER_BIN="${ROOT_DIR}/docker/mock-inference-provider/target/release/mock-inference-provider"
MOCK_PROVIDER_SRC="${ROOT_DIR}/docker/mock-inference-provider"

ergors_start_mock_provider() {
    local provider_port="${1:-11434}"
    local provider_host="127.0.0.1"

    log "Starting mock inference provider..."

    # Kill any existing mock provider on this port
    kill_port "$provider_port" 2>/dev/null || true

    # Always build fresh to pick up any code changes
    log "Building mock inference provider..."
    if ! cargo build --release --manifest-path "${MOCK_PROVIDER_SRC}/Cargo.toml" 2>&1 | tail -5; then
        log_error "Failed to build mock inference provider"
        return 1
    fi

    if [[ ! -f "$MOCK_PROVIDER_BIN" ]]; then
        log_error "Mock provider binary not found after build: $MOCK_PROVIDER_BIN"
        return 1
    fi

    # Ensure log directory exists
    mkdir -p "${TEST_DIR}"

    # Start as background process
    log_verbose "Starting mock provider on port $provider_port..."
    TESTDATA_MODE=true \
    MIN_LATENCY_MS=0 \
    MAX_LATENCY_MS=50 \
    PORT="$provider_port" \
    RUST_LOG=info \
        "$MOCK_PROVIDER_BIN" > "${TEST_DIR}/mock-provider.log" 2>&1 &
    MOCK_PROVIDER_PID=$!
    register_pid $MOCK_PROVIDER_PID

    # Wait for readiness
    local max_wait=10
    local wait_count=0
    while ! curl -s "http://${provider_host}:${provider_port}/health" >/dev/null 2>&1; do
        sleep 1
        wait_count=$((wait_count + 1))
        if [[ $wait_count -ge $max_wait ]]; then
            log_error "Mock provider failed to start within ${max_wait}s"
            if [[ -f "${TEST_DIR}/mock-provider.log" ]]; then
                tail -20 "${TEST_DIR}/mock-provider.log"
            fi
            return 1
        fi
    done

    MOCK_PROVIDER_URL="http://${provider_host}:${provider_port}"
    export MOCK_PROVIDER_URL

    log_success "Mock provider ready at $MOCK_PROVIDER_URL (PID $MOCK_PROVIDER_PID)"
    return 0
}

ergors_stop_mock_provider() {
    if [[ -n "${MOCK_PROVIDER_PID:-}" ]]; then
        log_verbose "Stopping mock provider (PID $MOCK_PROVIDER_PID)..."
        kill "$MOCK_PROVIDER_PID" 2>/dev/null || true
        wait "$MOCK_PROVIDER_PID" 2>/dev/null || true
        MOCK_PROVIDER_PID=""
    fi
}

ergors_generate_mock_api_key() {
    local provider="${1:-openai}"

    local payload
    payload=$(printf '{"provider":"%s","valid":true}' "$provider")

    local response
    response=$(curl -s "${MOCK_PROVIDER_URL}/api/keys/generate" \
        -H "Content-Type: application/json" \
        -d "$payload")

    local api_key
    api_key=$(echo "$response" | jq -r '.api_key')

    if [[ "$api_key" == "null" ]] || [[ -z "$api_key" ]]; then
        log_error "Failed to generate API key: $response"
        return 1
    fi

    log_verbose "Generated ${provider} API key: ${api_key:0:20}..."
    echo "$api_key"
}

ergors_configure_api_keys() {
    local coord_home="${TEST_DIR}/coordinator"

    log "Configuring API keys for inference..."

    # Generate API keys for each provider
    local openai_key anthropic_key ollama_key
    openai_key=$(ergors_generate_mock_api_key "openai") || return 1
    anthropic_key=$(ergors_generate_mock_api_key "anthropic") || return 1
    ollama_key=$(ergors_generate_mock_api_key "ollama") || return 1

    log_verbose "Generated keys:"
    log_verbose "  OpenAI: ${openai_key:0:20}..."
    log_verbose "  Anthropic: ${anthropic_key:0:20}..."
    log_verbose "  Ollama: ${ollama_key:0:20}..."

    # Export as environment variables for LLM router (fallback method)
    export OPENAI_API_KEY="$openai_key"
    export ANTHROPIC_API_KEY="$anthropic_key"
    export OLLAMA_API_KEY="$ollama_key"
    export MOCK_PROVIDER_URL

    # Use CLI to add providers with encrypted storage
    # This tests the production path of encrypted API key storage
    log "Adding providers via CLI (encrypted storage)..."

    # Add OpenAI provider
    echo "$openai_key" | ERGORS_CUSTODY_PASSWORD="${TEST_CUSTODY_PASSWORD}" \
        "$ERGORS_BIN" --home "$coord_home" provider add openai \
        --api-key "$openai_key" --default 2>&1 | grep -v "password" || true

    # Add Anthropic provider
    echo "$anthropic_key" | ERGORS_CUSTODY_PASSWORD="${TEST_CUSTODY_PASSWORD}" \
        "$ERGORS_BIN" --home "$coord_home" provider add anthropic \
        --api-key "$anthropic_key" 2>&1 | grep -v "password" || true

    # Add Ollama provider
    echo "$ollama_key" | ERGORS_CUSTODY_PASSWORD="${TEST_CUSTODY_PASSWORD}" \
        "$ERGORS_BIN" --home "$coord_home" provider add ollama \
        --api-key "$ollama_key" 2>&1 | grep -v "password" || true

    log_success "API keys configured via encrypted storage"

    # Configure LLM entities for model-based routing
    ergors_configure_llm_entities "$coord_home" "$openai_key" "$anthropic_key" "$ollama_key"

    return 0
}

ergors_configure_llm_entities() {
    local home_dir="$1"
    local openai_key="$2"
    local anthropic_key="$3"
    local ollama_key="$4"
    local config_file="${home_dir}/config.toml"

    log_verbose "Configuring LLM entities for model-based routing..."
    log_verbose "  Home: $home_dir"
    log_verbose "  OpenAI key: ${openai_key:0:20}..."
    log_verbose "  Anthropic key: ${anthropic_key:0:20}..."
    log_verbose "  Ollama key: ${ollama_key:0:20}..."

    # Create api_keys.json file with all provider keys
    local api_keys_file="${home_dir}/api_keys.json"
    cat > "$api_keys_file" <<EOF
{
  "openai": "$openai_key",
  "anthropic": "$anthropic_key",
  "ollama": "$ollama_key"
}
EOF

    log_verbose "Created api_keys.json: $api_keys_file"

    # Check if config.toml exists
    if [[ ! -f "$config_file" ]]; then
        log_error "Config file not found: $config_file"
        return 1
    fi

    # Remove existing [llm] section to avoid duplicate key error
    # Strips from [llm] line to next [section] or EOF
    if grep -q '^\[llm\]' "$config_file"; then
        log_verbose "Removing existing [llm] section from config..."
        sed -i.bak '/^\[llm\]/,/^\[/{/^\[llm\]/d;/^\[/!d;}' "$config_file"
        rm -f "${config_file}.bak"
    fi

    # Append LLM configuration to config.toml
    # This configures all three providers to route to the same mock provider
    cat >> "$config_file" <<'EOF'

# LLM Router Configuration
[llm]
api_keys_file = "api_keys.json"
default_strategy = 0
timeout_seconds = 30
max_retries = 3
default_entity = 0

# OpenAI Entity (gpt-*, chatgpt-*)
[[llm.entities]]
name = "openai"
base_url = "http://127.0.0.1:11434/v1"
models = ["gpt-4", "gpt-4-turbo", "gpt-3.5-turbo", "test-model"]
default_model = "gpt-3.5-turbo"
priority = 1
enabled = true
default_strategy = 0
timeout_seconds = 30
max_retries = 3

# Anthropic Entity (claude-*)
[[llm.entities]]
name = "anthropic"
base_url = "http://127.0.0.1:11434/v1"
models = ["claude-3-opus", "claude-3-sonnet", "claude-3-haiku", "claude-2"]
default_model = "claude-3-sonnet"
priority = 2
enabled = true
default_strategy = 0
timeout_seconds = 30
max_retries = 3

# Ollama Entity (llama*, mistral*)
[[llm.entities]]
name = "ollama"
base_url = "http://127.0.0.1:11434"
models = ["llama2", "llama3", "mistral", "codellama"]
default_model = "llama2"
priority = 3
enabled = true
default_strategy = 0
timeout_seconds = 30
max_retries = 3
EOF

    log_verbose "LLM entities configured with model-based routing"
    log_verbose "  OpenAI: gpt-*, chatgpt-* → ${MOCK_PROVIDER_URL:-http://127.0.0.1:11434}/v1"
    log_verbose "  Anthropic: claude-* → ${MOCK_PROVIDER_URL:-http://127.0.0.1:11434}/v1"
    log_verbose "  Ollama: llama*, mistral* → ${MOCK_PROVIDER_URL:-http://127.0.0.1:11434}"
    log_verbose "Config file updated: $config_file"

    return 0
}

# =============================================================================
# SDL Query Functions
# =============================================================================

# Get SDL template from contract
ergors_sdl_get_template() {
    local contract="$1"
    ergors_cw_query "$contract" '{"get_template": {}}'
}

# Get variable defaults from contract
ergors_sdl_get_defaults() {
    local contract="$1"
    ergors_cw_query "$contract" '{"get_defaults": {}}'
}

# Render SDL template with variables
ergors_sdl_render() {
    local contract="$1"
    shift

    # Build variables JSON from --var arguments
    local vars="{}"
    while [[ $# -gt 0 ]]; do
        case "$1" in
            --var)
                local kv="$2"
                local key="${kv%%=*}"
                local val="${kv#*=}"
                vars=$(echo "$vars" | jq --arg k "$key" --arg v "$val" '. + {($k): $v}')
                shift 2
                ;;
            *) shift ;;
        esac
    done

    ergors_cw_query "$contract" "{\"render_sdl\": {\"variables\": $vars}}"
}

# Import keys into running nodes (post-startup)
ergors_import_keys_post_startup() {
    log_section "Importing Keys into Running Nodes"
    
    local coord_home="$TEST_DIR/coordinator"
    local exec_home="$TEST_DIR/executor_0"
    
    # Get faucet mnemonic
    local mnemonic
    mnemonic=$(akash_get_faucet_mnemonic 2>/dev/null) || {
        log_warn "Could not get faucet mnemonic, skipping key import"
        return 0
    }
    
    log "Importing faucet key into coordinator..."
    local coord_import
    coord_import=$(ERGORS_CUSTODY_PASSWORD="${TEST_CUSTODY_PASSWORD}" \
        ERGORS_MNEMONIC="$mnemonic" \
        "$ERGORS_BIN" --home "$coord_home" keys import-mnemonic \
        --label "E2E Faucet Key" \
        --key-name "faucet" \
        --chain-id "${AKASH_LOCAL_CHAIN_ID:-local}" \
        --address-prefix "akash" \
        --make-default 2>&1) || true
    
    if echo "$coord_import" | grep -q "akash1"; then
        local addr
        addr=$(echo "$coord_import" | grep -o "akash1[a-z0-9]*" | head -1)
        log_success "Coordinator key imported: $addr"
        export COORDINATOR_ADDRESS="$addr"
    else
        log_warn "Coordinator key import may have failed"
        log_debug "$coord_import"
    fi
    
    log "Importing faucet key into executor..."
    local exec_import
    exec_import=$(ERGORS_CUSTODY_PASSWORD="${TEST_CUSTODY_PASSWORD}" \
        ERGORS_MNEMONIC="$mnemonic" \
        "$ERGORS_BIN" --home "$exec_home" keys import-mnemonic \
        --label "E2E Faucet Key" \
        --key-name "faucet" \
        --chain-id "${AKASH_LOCAL_CHAIN_ID:-local}" \
        --address-prefix "akash" \
        --make-default 2>&1) || true
    
    if echo "$exec_import" | grep -q "akash1"; then
        local addr
        addr=$(echo "$exec_import" | grep -o "akash1[a-z0-9]*" | head -1)
        log_success "Executor key imported: $addr"
        export EXECUTOR_ADDRESS="$addr"
    else
        log_warn "Executor key import may have failed"
        log_debug "$exec_import"
    fi
}
