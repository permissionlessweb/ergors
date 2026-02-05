#!/bin/bash
#
# Spawn ERGORS Test Network
#
# Creates and starts a local ERGORS network for testing with:
#   - 1 Coordinator node
#   - N Executor nodes (default: 2)
#   - Optional Referee node
#
# Usage:
#   ./scripts/spawn-test-network.sh [options]
#
# Options:
#   --executors N      Number of executor nodes (default: 2)
#   --with-referee     Include a referee node
#   --base-port N      Starting port (default: 50100)
#   --keep-running     Don't stop on script exit
#   --help             Show this help message

set -e

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
BOLD='\033[1m'
NC='\033[0m'

# Configuration
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
TEST_DIR="${TMPDIR:-/tmp}/ergors-test-network"
ERGORS_BIN="${ROOT_DIR}/target/release/ergors"
EXECUTOR_COUNT=2
INCLUDE_REFEREE=false
BASE_PORT=50100
KEEP_RUNNING=false

# Node tracking
declare -a NODE_PIDS
declare -A NODE_HOMES

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --executors) EXECUTOR_COUNT=$2; shift 2 ;;
        --with-referee) INCLUDE_REFEREE=true; shift ;;
        --base-port) BASE_PORT=$2; shift 2 ;;
        --keep-running) KEEP_RUNNING=true; shift ;;
        --help|-h)
            head -n 20 "$0" | tail -n 17 | sed 's/^#//'
            exit 0
            ;;
        *) echo -e "${RED}Unknown option: $1${NC}"; exit 1 ;;
    esac
done

# Logging
log() { echo -e "${BLUE}[$(date +%H:%M:%S)]${NC} $1"; }
log_success() { echo -e "${GREEN}[$(date +%H:%M:%S)] ✓${NC} $1"; }
log_warn() { echo -e "${YELLOW}[$(date +%H:%M:%S)] ⚠${NC} $1"; }
log_error() { echo -e "${RED}[$(date +%H:%M:%S)] ✗${NC} $1"; }

# Cleanup function
cleanup() {
    if [ "$KEEP_RUNNING" = true ]; then
        log_warn "Keeping network running (--keep-running)"
        log "Node homes: ${!NODE_HOMES[@]}"
        log "PIDs: ${NODE_PIDS[*]}"
        return
    fi

    log "Stopping all nodes..."
    for pid in "${NODE_PIDS[@]}"; do
        if kill -0 "$pid" 2>/dev/null; then
            kill "$pid" 2>/dev/null || true
        fi
    done

    log "Cleaning up test directories..."
    rm -rf "$TEST_DIR"

    log_success "Cleanup complete"
}

trap cleanup EXIT

# Check prerequisites
check_prerequisites() {
    log "Checking prerequisites..."

    if [ ! -f "$ERGORS_BIN" ]; then
        log "Building ergors binary..."
        cd "$ROOT_DIR"
        cargo build --release -p ergors 2>&1 | tail -5
    fi

    if [ ! -f "$ERGORS_BIN" ]; then
        log_error "Failed to build ergors binary"
        exit 1
    fi

    log_success "Prerequisites satisfied"
}

# Generate node config
generate_config() {
    local node_id=$1
    local node_type=$2
    local grpc_port=$3
    local http_port=$4
    local p2p_port=$5
    local home_dir=$6
    local seeds=$7

    mkdir -p "$home_dir/data"

    cat > "$home_dir/config.toml" <<EOF
# ERGORS Test Node Configuration
# Node: $node_id ($node_type)

[network]
listen_addr = "0.0.0.0:${p2p_port}"
seeds = [${seeds}]
node_type = "${node_type}"

[identity]
node_id = "${node_id}"
public_key = ""
node_type = "NODE_TYPE_${node_type^^}"

[storage]
data_dir = "${home_dir}/data"

[llm]
# No LLM providers for testing

[grant]
acceptance_mode = "accept_all"
EOF

    # Create minimal .env
    cat > "$home_dir/.env" <<EOF
# Test environment
NODE_DATA_PATH=${home_dir}
EOF

    log "Generated config for $node_id at $home_dir"
}

# Start a node
start_node() {
    local node_id=$1
    local home_dir=$2
    local grpc_port=$3

    log "Starting node '$node_id' (gRPC: $grpc_port)..."

    "$ERGORS_BIN" start \
        --home "$home_dir" \
        --grpc-port "$grpc_port" \
        > "$home_dir/node.log" 2>&1 &

    local pid=$!
    NODE_PIDS+=("$pid")
    NODE_HOMES[$node_id]="$home_dir"

    log_success "Node '$node_id' started (PID: $pid)"
}

# Wait for node to be healthy
wait_for_node() {
    local node_id=$1
    local grpc_port=$2
    local timeout=${3:-30}

    log "Waiting for node '$node_id' to be healthy..."

    local start_time=$(date +%s)
    while true; do
        local elapsed=$(($(date +%s) - start_time))
        if [ $elapsed -gt $timeout ]; then
            log_error "Timeout waiting for node '$node_id'"
            return 1
        fi

        if nc -z 127.0.0.1 "$grpc_port" 2>/dev/null; then
            log_success "Node '$node_id' is healthy"
            return 0
        fi

        sleep 1
    done
}

# Main
main() {
    echo ""
    echo -e "${CYAN}${BOLD}╔═══════════════════════════════════════════════════════════════╗${NC}"
    echo -e "${CYAN}${BOLD}║                                                               ║${NC}"
    echo -e "${CYAN}${BOLD}║              ERGORS Test Network Spawner                      ║${NC}"
    echo -e "${CYAN}${BOLD}║                                                               ║${NC}"
    echo -e "${CYAN}${BOLD}╚═══════════════════════════════════════════════════════════════╝${NC}"
    echo ""
    echo -e "  ${BOLD}Configuration:${NC}"
    echo -e "    Executor nodes: ${EXECUTOR_COUNT}"
    echo -e "    Referee node:   ${INCLUDE_REFEREE}"
    echo -e "    Base port:      ${BASE_PORT}"
    echo -e "    Test directory: ${TEST_DIR}"
    echo ""

    check_prerequisites

    # Clean previous test directory
    rm -rf "$TEST_DIR"
    mkdir -p "$TEST_DIR"

    local port=$BASE_PORT
    local coordinator_p2p_addr=""

    # === Coordinator Node ===
    log "Setting up coordinator node..."
    local coord_home="$TEST_DIR/coordinator"
    local coord_grpc=$port
    local coord_http=$((port + 1))
    local coord_p2p=$((port + 2))
    coordinator_p2p_addr="127.0.0.1:${coord_p2p}"
    port=$((port + 10))

    generate_config "coordinator" "coordinator" "$coord_grpc" "$coord_http" "$coord_p2p" "$coord_home" ""
    start_node "coordinator" "$coord_home" "$coord_grpc"
    sleep 2  # Give coordinator time to initialize

    # === Executor Nodes ===
    for i in $(seq 0 $((EXECUTOR_COUNT - 1))); do
        local node_id="executor_${i}"
        local home="$TEST_DIR/$node_id"
        local grpc=$port
        local http=$((port + 1))
        local p2p=$((port + 2))
        port=$((port + 10))

        generate_config "$node_id" "executor" "$grpc" "$http" "$p2p" "$home" "\"${coordinator_p2p_addr}\""
        start_node "$node_id" "$home" "$grpc"
    done

    # === Referee Node (optional) ===
    if [ "$INCLUDE_REFEREE" = true ]; then
        local home="$TEST_DIR/referee"
        local grpc=$port
        local http=$((port + 1))
        local p2p=$((port + 2))

        generate_config "referee" "referee" "$grpc" "$http" "$p2p" "$home" "\"${coordinator_p2p_addr}\""
        start_node "referee" "$home" "$grpc"
    fi

    # Wait for all nodes to be healthy
    echo ""
    log "Waiting for network to be ready..."

    wait_for_node "coordinator" "$coord_grpc" || exit 1

    for i in $(seq 0 $((EXECUTOR_COUNT - 1))); do
        local node_id="executor_${i}"
        local grpc=$((BASE_PORT + 10 * (i + 1)))
        wait_for_node "$node_id" "$grpc" || exit 1
    done

    if [ "$INCLUDE_REFEREE" = true ]; then
        local grpc=$((BASE_PORT + 10 * (EXECUTOR_COUNT + 1)))
        wait_for_node "referee" "$grpc" || exit 1
    fi

    # Print summary
    echo ""
    echo -e "${GREEN}${BOLD}═══════════════════════════════════════════════════════════════${NC}"
    echo -e "${GREEN}${BOLD}  ERGORS Test Network Ready${NC}"
    echo -e "${GREEN}${BOLD}═══════════════════════════════════════════════════════════════${NC}"
    echo ""
    echo -e "  ${BOLD}Coordinator:${NC}"
    echo -e "    gRPC:  127.0.0.1:${coord_grpc}"
    echo -e "    P2P:   ${coordinator_p2p_addr}"
    echo -e "    Home:  ${coord_home}"
    echo ""
    echo -e "  ${BOLD}Executors:${NC}"
    for i in $(seq 0 $((EXECUTOR_COUNT - 1))); do
        local grpc=$((BASE_PORT + 10 * (i + 1)))
        echo -e "    executor_${i}: 127.0.0.1:${grpc}"
    done
    echo ""

    if [ "$KEEP_RUNNING" = true ]; then
        echo -e "  ${BOLD}Network is running. Press Ctrl+C to stop.${NC}"
        echo ""

        # Export for other scripts
        export ERGORS_COORDINATOR_GRPC="127.0.0.1:${coord_grpc}"
        export ERGORS_COORDINATOR_P2P="${coordinator_p2p_addr}"
        export ERGORS_TEST_DIR="$TEST_DIR"

        # Wait indefinitely
        while true; do
            sleep 60
        done
    fi
}

main "$@"
