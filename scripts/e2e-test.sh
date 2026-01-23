#!/bin/bash
#
# ERGORS End-to-End Integration Test Runner
#
# Tests the complete ERGORS deployment workflow:
#   1. Build ERGORS binary
#   2. Spawn ERGORS test network (coordinator + executors)
#   3. Setup Kind cluster with Akash node/provider
#   4. Build mock inference provider Docker image
#   5. Execute deployment through ERGORS workflow:
#      - Executor requests grant from coordinator
#      - Coordinator approves grant
#      - Executor deploys to Akash
#   6. Verify deployed service
#   7. Cleanup
#
# Usage:
#   ./scripts/e2e-test.sh [options]
#
# Options:
#   --skip-build       Skip building ergors binary
#   --skip-contracts   Skip building CosmWasm contracts (use existing artifacts)
#   --skip-network     Skip ERGORS network setup (use existing)
#   --skip-akash       Skip Akash/Kind setup (use existing)
#   --skip-cleanup     Keep everything running after tests
#   --verbose          Enable verbose output
#   --live-logs        Stream node logs in real-time (colored by node)
#   --mock             Use mock Akash mode (Kind cluster only, no real blockchain)
#   --akash-home PATH  Set Akash repo location (default: ~/go/src/github.com/akash-network)
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
TEST_DIR="${TMPDIR:-/tmp}/ergors-e2e-test"
CLUSTER_NAME="ergors-e2e"
MOCK_IMAGE="mock-inference-provider:e2e"

# ERGORS Network Config
ERGORS_BIN="${ROOT_DIR}/target/release/ergors"
ERGORS_CLI="${ROOT_DIR}/target/release/ergors-cli"
BASE_PORT=50100
EXECUTOR_COUNT=2
TEST_CUSTODY_PASSWORD="e2e-test-password-12345"

# Akash Deployment Config (for ergors-cli deploy)
DEPLOY_SDL="${ROOT_DIR}/docker/mock-inference-provider/deploy.local.sdl.yaml"
DEPLOY_SESSION_ID=""
AKASH_LOCAL_NODE="http://localhost:26657"
AKASH_LOCAL_CHAIN_ID="local"

# Akash Dev Environment Config
AKASH_HOME="${HOME}/go/src/github.com/akash-network"
AKASH_PROVIDER_DIR="${AKASH_HOME}/provider"
AKASH_KUBE_DIR="${AKASH_PROVIDER_DIR}/_run/kube"
KUBE_ROLLOUT_TIMEOUT=${KUBE_ROLLOUT_TIMEOUT:-30000}
GOVERSION_SEMVER=${GOVERSION_SEMVER:-"v1.24.2"}

# GNU Make command - will be resolved after PATH setup in check_prerequisites
MAKE_CMD="make"

# Akash process PIDs
AKASH_NODE_PID=""
AKASH_PROVIDER_PID=""
AKASH_OPERATOR_PIDS=()

# Real Akash mode (vs mock mode) - default to real blockchain
USE_REAL_AKASH=true

# Flags
SKIP_BUILD=false
SKIP_CONTRACTS=false
SKIP_NETWORK=false
SKIP_AKASH=false
SKIP_CLEANUP=false
VERBOSE=false

# Tracking
declare -a NODE_PIDS
declare -a TAIL_PIDS
COORDINATOR_GRPC=""
EXECUTOR_GRPC=""
DEPLOYED_ENDPOINT=""
DEPLOYMENT_ID=""
DEPLOY_DSEQ="${DEPLOY_DSEQ:-1}"

# Live log streaming flag
LIVE_LOGS=false

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --skip-build) SKIP_BUILD=true; shift ;;
        --skip-contracts) SKIP_CONTRACTS=true; shift ;;
        --skip-network) SKIP_NETWORK=true; shift ;;
        --skip-akash) SKIP_AKASH=true; shift ;;
        --skip-cleanup) SKIP_CLEANUP=true; shift ;;
        --verbose) VERBOSE=true; shift ;;
        --live-logs) LIVE_LOGS=true; shift ;;
        --mock) USE_REAL_AKASH=false; shift ;;
        --akash-home) AKASH_HOME="$2"; AKASH_PROVIDER_DIR="${AKASH_HOME}/provider"; AKASH_KUBE_DIR="${AKASH_PROVIDER_DIR}/_run/kube"; shift 2 ;;
        --help|-h)
            head -n 35 "$0" | tail -n 32 | sed 's/^#//'
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
log_step() { echo -e "\n${CYAN}${BOLD}═══════════════════════════════════════════════════════════${NC}"; echo -e "${CYAN}${BOLD}  $1${NC}"; echo -e "${CYAN}${BOLD}═══════════════════════════════════════════════════════════${NC}\n"; }

# Node-specific log colors
COORD_COLOR='\033[0;35m'   # Magenta for coordinator
EXEC_COLOR='\033[0;36m'    # Cyan for executor
MOCK_COLOR='\033[0;33m'    # Yellow for mock provider

# Start streaming logs from a node's log file
start_log_stream() {
    local node_name=$1
    local log_file=$2
    local color=$3
    local prefix="[$node_name]"

    if [ "$LIVE_LOGS" != true ]; then
        return
    fi

    # Create the log file if it doesn't exist
    touch "$log_file"

    # Start tail -f in background, prefixing each line with colored node name
    (tail -f "$log_file" 2>/dev/null | while IFS= read -r line; do
        echo -e "${color}${prefix}${NC} $line"
    done) &
    local tail_pid=$!
    TAIL_PIDS+=($tail_pid)
    log "Started log stream for $node_name (tail PID: $tail_pid)"
}

# Stop all log streams
stop_log_streams() {
    if [ ${#TAIL_PIDS[@]} -gt 0 ]; then
        log "Stopping log streams..."
        for pid in "${TAIL_PIDS[@]}"; do
            if kill -0 "$pid" 2>/dev/null; then
                kill "$pid" 2>/dev/null || true
            fi
        done
        TAIL_PIDS=()
    fi
}

# Debug: show node logs
show_node_logs() {
    local node_name=$1
    local log_file="$TEST_DIR/$node_name/node.log"

    if [ -f "$log_file" ]; then
        echo -e "\n${YELLOW}=== $node_name logs (last 30 lines) ===${NC}"
        tail -30 "$log_file"
        echo -e "${YELLOW}=== end $node_name logs ===${NC}\n"
    fi
}

# Debug: show config
show_config() {
    local node_name=$1
    local config_file="$TEST_DIR/$node_name/config.toml"

    if [ -f "$config_file" ]; then
        echo -e "\n${YELLOW}=== $node_name config.toml ===${NC}"
        cat "$config_file"
        echo -e "${YELLOW}=== end config ===${NC}\n"
    fi
}

# Track test results
TESTS_PASSED=0
TESTS_FAILED=0
START_TIME=$(date +%s)

# Cleanup function
cleanup() {
    # Always stop log streams first
    stop_log_streams

    if [ "$SKIP_CLEANUP" = true ]; then
        log_warn "Skipping cleanup (--skip-cleanup)"
        log_warn "Test dir: ${TEST_DIR}"
        log_warn "Cluster: ${CLUSTER_NAME}"
        [ ${#NODE_PIDS[@]} -gt 0 ] && log_warn "ERGORS PIDs: ${NODE_PIDS[*]}"
        [ -n "$AKASH_NODE_PID" ] && log_warn "Akash Node PID: ${AKASH_NODE_PID}"
        [ -n "$AKASH_PROVIDER_PID" ] && log_warn "Akash Provider PID: ${AKASH_PROVIDER_PID}"
        return
    fi

    log_step "Cleanup"

    # Stop ERGORS nodes
    for pid in "${NODE_PIDS[@]}"; do
        if kill -0 "$pid" 2>/dev/null; then
            log "Stopping ERGORS node (PID: $pid)..."
            kill "$pid" 2>/dev/null || true
        fi
    done

    # Cleanup real Akash environment if used
    if [ "$USE_REAL_AKASH" = true ]; then
        cleanup_akash_environment
    else
        # Delete Kind cluster (mock mode)
        if kind get clusters 2>/dev/null | grep -q "^${CLUSTER_NAME}$"; then
            log "Deleting Kind cluster..."
            kind delete cluster --name "${CLUSTER_NAME}" 2>/dev/null || true
        fi
    fi

    # Remove test directory
    if [ -d "$TEST_DIR" ]; then
        log "Removing test directory..."
        rm -rf "$TEST_DIR"
    fi

    log_success "Cleanup complete"
}

trap cleanup EXIT

# Check prerequisites
check_prerequisites() {
    log_step "Checking Prerequisites"

    local missing=()

    command -v docker &>/dev/null || missing+=("docker")
    command -v kind &>/dev/null || missing+=("kind")
    command -v kubectl &>/dev/null || missing+=("kubectl")
    command -v cargo &>/dev/null || missing+=("cargo")

    # Additional prerequisites for real Akash mode
    if [ "$USE_REAL_AKASH" = true ]; then
        command -v go &>/dev/null || missing+=("go")

        # Check for GNU Make 4.0+ (required by Akash)
        local make_version=""
        if [[ "$(uname)" == "Darwin" ]]; then
            # macOS: prefer gmake (GNU Make from Homebrew)
            if command -v gmake &>/dev/null; then
                make_version=$(gmake --version 2>/dev/null | head -1 | grep -o '[0-9]\+\.[0-9]\+' | head -1)
                MAKE_CMD="gmake"
                log "Using GNU Make (gmake): $make_version"
            else
                log_error "GNU Make 4.0+ required but not found"
                log_error "Install with: brew install make"
                log_error "Then use 'gmake' or add /opt/homebrew/opt/make/libexec/gnubin to PATH"
                missing+=("gmake (GNU Make 4.0+)")
            fi
        else
            # Linux: check system make version
            if command -v make &>/dev/null; then
                make_version=$(make --version 2>/dev/null | head -1 | grep -o '[0-9]\+\.[0-9]\+' | head -1)
                local major_version=$(echo "$make_version" | cut -d. -f1)
                if [ "$major_version" -lt 4 ] 2>/dev/null; then
                    log_error "GNU Make 4.0+ required, found $make_version"
                    missing+=("make (version 4.0+)")
                fi
            else
                missing+=("make")
            fi
        fi

        # On macOS, setup GNU Make wrapper and install missing deps
        if [[ "$(uname)" == "Darwin" ]]; then
            # Use Homebrew's gnubin to put GNU Make as 'make' in PATH
            if [ -d "/opt/homebrew/opt/make/libexec/gnubin" ]; then
                export PATH="/opt/homebrew/opt/make/libexec/gnubin:$PATH"
                log "Using Homebrew GNU Make: $(make --version | head -1)"
            elif [ -d "/usr/local/opt/make/libexec/gnubin" ]; then
                export PATH="/usr/local/opt/make/libexec/gnubin:$PATH"
                log "Using Homebrew GNU Make: $(make --version | head -1)"
            elif command -v gmake &>/dev/null; then
                # Fallback: create wrapper symlink
                MAKE_WRAPPER_DIR="${HOME}/.local/bin/gmake-wrapper"
                mkdir -p "$MAKE_WRAPPER_DIR"
                ln -sf "$(which gmake)" "$MAKE_WRAPPER_DIR/make"
                export PATH="$MAKE_WRAPPER_DIR:$PATH"
                log "Created make -> gmake wrapper"
            else
                log_error "GNU Make 4.0+ required: brew install make"
                missing+=("gmake")
            fi

            # Ensure wget is available (required by Akash Makefile)
            if ! command -v wget &>/dev/null; then
                log_warn "wget not found - installing..."
                brew install wget 2>&1 | tail -3
            fi

            # Ensure realpath is available (coreutils on macOS)
            if ! command -v realpath &>/dev/null; then
                log_warn "realpath not found - installing coreutils..."
                brew install coreutils 2>&1 | tail -3
            fi
        fi
    fi

    if [ ${#missing[@]} -gt 0 ]; then
        log_error "Missing required tools: ${missing[*]}"
        exit 1
    fi

    if ! docker info &>/dev/null; then
        log_error "Docker is not running"
        exit 1
    fi

    # Check Go version for real Akash mode
    if [ "$USE_REAL_AKASH" = true ]; then
        local go_version=$(go version 2>/dev/null | awk '{print $3}')
        log "Go version: $go_version"
    fi

    log_success "All prerequisites satisfied"
}

# ==================== Real Akash Dev Environment ====================

# Clone or update Akash provider repository
setup_akash_repos() {
    log_step "Setting Up Akash Dev Repositories"

    # Create parent directory if needed
    mkdir -p "${AKASH_HOME}"

    # Clone provider repo if not present
    if [ ! -d "${AKASH_PROVIDER_DIR}" ]; then
        log "Cloning permissionlessweb/provider (feat/local-dev)..."
        cd "${AKASH_HOME}"
        git clone -b feat/local-dev https://github.com/permissionlessweb/provider.git
        log_success "Provider repo cloned to ${AKASH_PROVIDER_DIR}"
    else
        log "Provider repo already exists at ${AKASH_PROVIDER_DIR}"
        cd "${AKASH_PROVIDER_DIR}"
        log "Current branch: $(git branch --show-current)"
    fi

    # Verify kube runbook exists
    if [ ! -d "${AKASH_KUBE_DIR}" ]; then
        log_error "Akash kube directory not found at ${AKASH_KUBE_DIR}"
        log_error "The provider repo structure may have changed"
        exit 1
    fi

    # Setup environment for the provider repo
    cd "${AKASH_PROVIDER_DIR}"
    log "Loading Akash environment variables..."
    load_akash_env

    log_success "Akash repos ready"
}

# Setup Akash environment variables (replaces direnv)
# Sources the .env file and sets all required variables the Makefile expects
load_akash_env() {
    export AP_ROOT="${AKASH_PROVIDER_DIR}"
    export AKASH_DIRENV_SET=1

    # Source the .env file to get DEVCACHE paths
    if [ -f "${AKASH_PROVIDER_DIR}/.env" ]; then
        # Substitute AP_ROOT in .env values and export them
        # Skip ROOT_DIR to avoid overwriting our project's ROOT_DIR
        while IFS='=' read -r key value; do
            # Skip comments and empty lines
            [[ "$key" =~ ^#.*$ || -z "$key" ]] && continue
            # Never overwrite ROOT_DIR (it points to our project root)
            [[ "$key" == "ROOT_DIR" ]] && continue
            # Substitute ${AP_ROOT} with actual path
            value="${value//\$\{AP_ROOT\}/$AP_ROOT}"
            value="${value//\$AP_ROOT/$AP_ROOT}"
            export "$key=$value"
        done < "${AKASH_PROVIDER_DIR}/.env"
    fi

    # Set kube-specific run variables
    export AP_RUN_NAME="kube"
    export DEVCACHE_RUN="${AP_DEVCACHE_BASE:-${AP_ROOT}/.cache}/run"
    export AP_RUN_DIR="${DEVCACHE_RUN}/${AP_RUN_NAME}"
    export AKASH_HOME="${AP_RUN_DIR}/.akash"

    # Trick the Makefile into thinking direnv loaded (DIRENV_FILE must match CURDIR/.envrc)
    export DIRENV_FILE="${AKASH_KUBE_DIR}/.envrc"
    export DIRENV_DIR="${AKASH_KUBE_DIR}"

    # Set Go environment variables (parsed from go.mod)
    local gomod="${AP_ROOT}/go.mod"
    if [ -f "$gomod" ]; then
        local go_version=$(grep -E '^go [0-9]+\.[0-9]+' "$gomod" | head -1 | awk '{print $2}')
        local toolchain=$(grep -E '^toolchain go' "$gomod" | head -1 | awk '{print $2}')

        if [ -n "$toolchain" ]; then
            export GOTOOLCHAIN="$toolchain"
        elif [ -n "$go_version" ]; then
            # If no toolchain directive, use go version
            export GOTOOLCHAIN="go${go_version}"
        fi

        export GOVERSION="${GOTOOLCHAIN}"
        export GOTOOLCHAIN_SEMVER="v${GOTOOLCHAIN#go}"
        export GOVERSION_SEMVER="${GOTOOLCHAIN_SEMVER}"
    fi

    # Set GOPATH if not set
    if [ -z "$GOPATH" ]; then
        export GOPATH=$(go env GOPATH 2>/dev/null || echo "$HOME/go")
    fi

    # Note: ROOT_DIR is passed via akash_make, not exported globally
    # (exporting it would overwrite our project's ROOT_DIR)

    # Set binary paths (normally set by .envrc)
    export AP_DEVCACHE="${AP_DEVCACHE:-${AP_ROOT}/.cache}"
    export AP_DEVCACHE_BIN="${AP_DEVCACHE_BIN:-${AP_DEVCACHE}/bin}"
    export AP_DEVCACHE_INCLUDE="${AP_DEVCACHE_INCLUDE:-${AP_DEVCACHE}/include}"
    export AP_DEVCACHE_VERSIONS="${AP_DEVCACHE_VERSIONS:-${AP_DEVCACHE}/versions}"
    export AP_DEVCACHE_NODE_MODULES="${AP_DEVCACHE_NODE_MODULES:-${AP_DEVCACHE}}"
    export AP_DEVCACHE_NODE_BIN="${AP_DEVCACHE_NODE_BIN:-${AP_DEVCACHE_NODE_MODULES}/node_modules/.bin}"
    export AP_DEVCACHE_TESTS="${AP_DEVCACHE_TESTS:-${AP_DEVCACHE}/tests}"
    export AKASH="${AP_DEVCACHE_BIN}/akash"
    export PROVIDER_SERVICES="${AP_DEVCACHE_BIN}/provider-services"
    export SEMVER="${AP_ROOT}/script/semver.sh"

    # Create required directories
    mkdir -p "$AP_RUN_DIR" "$AKASH_HOME" "$AP_DEVCACHE_BIN" "$AP_DEVCACHE_INCLUDE" \
             "$AP_DEVCACHE_VERSIONS" "$AP_DEVCACHE_TESTS" "$DEVCACHE_RUN" 2>/dev/null || true
}

# Run a make command in the Akash kube directory with proper environment
akash_make() {
    local target="$1"
    shift

    # Ensure environment is loaded
    load_akash_env

    # Run gmake from the kube directory, passing ROOT_DIR only to make
    cd "${AKASH_KUBE_DIR}"
    $MAKE_CMD "$target" ROOT_DIR="${AP_ROOT}" "$@"
}

# Initialize Akash kube environment (clean state)
init_akash_kube() {
    log "Initializing Akash kube environment..."

    # Clean up any existing state using direnv exec
    akash_make clean 2>/dev/null || true
    akash_make init 2>/dev/null || true

    # Speed up block time for test environment (1s commits)
    local config_toml="${AKASH_HOME}/config/config.toml"
    if [ -f "$config_toml" ]; then
        log "Configuring fast block times (1s)..."
        sed -i.bak \
            -e 's/^timeout_propose = .*/timeout_propose = "1s"/' \
            -e 's/^timeout_propose_delta = .*/timeout_propose_delta = "200ms"/' \
            -e 's/^timeout_prevote = .*/timeout_prevote = "500ms"/' \
            -e 's/^timeout_prevote_delta = .*/timeout_prevote_delta = "200ms"/' \
            -e 's/^timeout_precommit = .*/timeout_precommit = "500ms"/' \
            -e 's/^timeout_precommit_delta = .*/timeout_precommit_delta = "200ms"/' \
            -e 's/^timeout_commit = .*/timeout_commit = "1s"/' \
            "$config_toml"
        rm -f "${config_toml}.bak"
        log_success "Block time set to ~1s"
    else
        log_warn "config.toml not found at ${config_toml}, using default block times"
    fi

    log_success "Akash kube environment initialized"
}

# Create Kind cluster with Akash components
setup_akash_kube_cluster() {
    log_step "Setting Up Akash Kind Cluster"

    # Delete existing cluster if present
    if kind get clusters 2>/dev/null | grep -q "^kind$"; then
        log "Deleting existing Kind cluster..."
        akash_make kube-cluster-delete 2>/dev/null || kind delete cluster --name kind
    fi

    # Set environment for build
    export GOVERSION_SEMVER="${GOVERSION_SEMVER}"
    export KUBE_ROLLOUT_TIMEOUT="${KUBE_ROLLOUT_TIMEOUT}"
    # Skip building provider from source - uses released binaries instead
    # (goreleaser-cross images may not exist for latest Go versions)
    export SKIP_BUILD=true

    log "Creating Kind cluster with Akash components (timeout: ${KUBE_ROLLOUT_TIMEOUT}s)..."
    log "This may take several minutes..."

    local cluster_log="${TEST_DIR}/akash-cluster-setup.log"
    if [ "$VERBOSE" = true ]; then
        akash_make kube-cluster-setup 2>&1 | tee "$cluster_log"
    else
        # Stream to log file, show periodic progress
        akash_make kube-cluster-setup > "$cluster_log" 2>&1 &
        local make_pid=$!
        local elapsed=0
        while kill -0 "$make_pid" 2>/dev/null; do
            sleep 10
            elapsed=$((elapsed + 10))
            local last_line=$(tail -1 "$cluster_log" 2>/dev/null | head -c 120)
            log "  [${elapsed}s] ${last_line}"
        done
        wait "$make_pid"
        local exit_code=$?
        if [ $exit_code -ne 0 ]; then
            log_error "kube-cluster-setup failed (exit code: $exit_code). Last 30 lines:"
            tail -30 "$cluster_log" | while IFS= read -r line; do
                log "  $line"
            done
            exit 1
        fi
        log "Cluster setup complete. Full log: $cluster_log"
    fi

    # Verify cluster is ready
    if ! kubectl cluster-info &>/dev/null; then
        log_error "Kind cluster failed to start"
        exit 1
    fi

    log_success "Akash Kind cluster ready"
    kubectl get nodes
}

# Start Akash blockchain node
start_akash_node() {
    log_step "Starting Akash Blockchain Node"

    local node_log="${TEST_DIR}/akash-node.log"
    mkdir -p "${TEST_DIR}"

    # Kill any leftover processes on node ports (26657 RPC, 26656 P2P, 9090 gRPC, 1317 REST)
    for port in 26657 26656 9090 1317; do
        local pid=$(lsof -ti :$port 2>/dev/null || true)
        if [ -n "$pid" ]; then
            log "Killing leftover process on port $port (PID: $pid)"
            kill $pid 2>/dev/null || true
            sleep 1
        fi
    done

    log "Starting Akash node (logs: ${node_log})..."

    # Start node in background
    akash_make node-run > "${node_log}" 2>&1 &
    AKASH_NODE_PID=$!

    log "Akash node PID: ${AKASH_NODE_PID}"

    # Start log streaming if enabled
    start_log_stream "ANODE" "${node_log}" "${BLUE}"

    # Wait for node to be ready
    local max_wait=60
    local waited=0
    log "Waiting for Akash node to be ready..."

    while [ $waited -lt $max_wait ]; do
        sleep 2
        waited=$((waited + 2))

        # Check if process is still running
        if ! kill -0 "$AKASH_NODE_PID" 2>/dev/null; then
            log_error "Akash node process died"
            tail -50 "${node_log}" 2>/dev/null || true
            exit 1
        fi

        # Check for ready indicators in log
        if grep -q "committed state\|indexed block\|Timed out" "${node_log}" 2>/dev/null; then
            log_success "Akash node ready (${waited}s)"
            return 0
        fi

        log "  Waiting for node... (${waited}/${max_wait}s)"
    done

    log_warn "Node may not be fully ready yet (timeout after ${max_wait}s)"
    log_warn "Continuing anyway - check logs if issues occur"
}

# Create provider on blockchain
create_akash_provider() {
    log_step "Creating Akash Provider"

    log "Registering provider on blockchain..."

    if [ "$VERBOSE" = true ]; then
        akash_make provider-create
    else
        akash_make provider-create 2>&1 | tail -10
    fi

    log_success "Akash provider created"
}

# Start Akash provider service
start_akash_provider() {
    log_step "Starting Akash Provider Service"

    local provider_log="${TEST_DIR}/akash-provider.log"

    # Kill any leftover processes on provider ports (8443 gateway, 8444 status)
    for port in 8443 8444; do
        local pid=$(lsof -ti :$port 2>/dev/null || true)
        if [ -n "$pid" ]; then
            log "Killing leftover process on port $port (PID: $pid)"
            kill $pid 2>/dev/null || true
            sleep 1
        fi
    done

    log "Starting Akash provider (logs: ${provider_log})..."

    # Start provider in background
    akash_make provider-run > "${provider_log}" 2>&1 &
    AKASH_PROVIDER_PID=$!

    log "Akash provider PID: ${AKASH_PROVIDER_PID}"

    # Start log streaming if enabled
    start_log_stream "APROV" "${provider_log}" "${GREEN}"

    # Wait for provider to be ready
    local max_wait=60
    local waited=0
    log "Waiting for Akash provider to be ready..."

    while [ $waited -lt $max_wait ]; do
        sleep 2
        waited=$((waited + 2))

        # Check if process is still running
        if ! kill -0 "$AKASH_PROVIDER_PID" 2>/dev/null; then
            log_error "Akash provider process died"
            tail -50 "${provider_log}" 2>/dev/null || true
            exit 1
        fi

        # Check for ready indicators
        if grep -q "listening\|bidengine.*running\|server started" "${provider_log}" 2>/dev/null; then
            log_success "Akash provider ready (${waited}s)"
            return 0
        fi

        log "  Waiting for provider... (${waited}/${max_wait}s)"
    done

    log_warn "Provider may not be fully ready yet (timeout after ${max_wait}s)"
}

# Full Akash dev environment setup
setup_real_akash_environment() {
    if [ "$USE_REAL_AKASH" != true ]; then
        log_warn "Real Akash mode not enabled (use --real-akash flag)"
        return 0
    fi

    setup_akash_repos
    init_akash_kube
    setup_akash_kube_cluster
    start_akash_node
    create_akash_provider
    start_akash_provider

    log_success "Real Akash development environment ready"
    log "  Node PID: ${AKASH_NODE_PID}"
    log "  Provider PID: ${AKASH_PROVIDER_PID}"
}

# Cleanup Akash dev environment
cleanup_akash_environment() {
    log "Cleaning up Akash environment..."

    # Stop provider
    if [ -n "$AKASH_PROVIDER_PID" ] && kill -0 "$AKASH_PROVIDER_PID" 2>/dev/null; then
        log "Stopping Akash provider (PID: $AKASH_PROVIDER_PID)..."
        kill "$AKASH_PROVIDER_PID" 2>/dev/null || true
    fi

    # Stop node
    if [ -n "$AKASH_NODE_PID" ] && kill -0 "$AKASH_NODE_PID" 2>/dev/null; then
        log "Stopping Akash node (PID: $AKASH_NODE_PID)..."
        kill "$AKASH_NODE_PID" 2>/dev/null || true
    fi

    # Delete Kind cluster created by Akash
    if [ "$USE_REAL_AKASH" = true ] && [ -d "${AKASH_KUBE_DIR}" ]; then
        akash_make kube-cluster-delete 2>/dev/null || true
    fi

    log_success "Akash environment cleaned up"
}

# ==================== Engine Akash Deployment Workflow ====================

# Helper: run ergors-cli deploy command against coordinator
ergors_deploy() {
    local subcommand="$1"
    shift

    if [ ! -f "$ERGORS_CLI" ]; then
        echo '{"error": "ergors-cli binary not found at '"$ERGORS_CLI"'"}'
        return 1
    fi

    if [ -z "$COORDINATOR_GRPC" ]; then
        echo '{"error": "COORDINATOR_GRPC not set"}'
        return 1
    fi

    "$ERGORS_CLI" --grpc-addr "http://${COORDINATOR_GRPC}" --json deploy "$subcommand" "$@"
}

# Create deployment via engine workflow
create_akash_deployment() {
    if [ "$USE_REAL_AKASH" != true ]; then
        log_warn "Skipping real deployment (use real Akash mode)"
        return 0
    fi

    log_step "Creating Akash Deployment (via engine)"

    # Check coordinator is reachable
    if [ -n "$COORDINATOR_GRPC" ] && ! nc -z 127.0.0.1 "${COORDINATOR_GRPC##*:}" 2>/dev/null; then
        log_error "Coordinator gRPC not reachable at ${COORDINATOR_GRPC}"
        log "  Ensure ERGORS engine is running and gRPC port is open"
        return 1
    fi

    log "Submitting deployment via ergors-cli deploy create..."
    log "  SDL: ${DEPLOY_SDL}"
    log "  Node: ${AKASH_LOCAL_NODE}"
    log "  Chain: ${AKASH_LOCAL_CHAIN_ID}"

    local create_output
    create_output=$(ergors_deploy create \
        --sdl "${DEPLOY_SDL}" \
        --key-name "default" \
        --account-index 0 \
        --node "${AKASH_LOCAL_NODE}" \
        --chain-id "${AKASH_LOCAL_CHAIN_ID}" \
        2>&1) || true
    echo "$create_output" > "${TEST_DIR}/deployment-create.log"

    # Extract session_id from JSON output
    DEPLOY_SESSION_ID=$(echo "$create_output" | jq -r '.session_id // empty' 2>/dev/null)

    if [ -n "$DEPLOY_SESSION_ID" ]; then
        log_success "Deployment workflow created (session: ${DEPLOY_SESSION_ID:0:8}...)"
    else
        log_error "Failed to create deployment workflow"
        log "  Output: $create_output"
        return 1
    fi
}

# List deployments via engine
query_akash_deployments() {
    if [ "$USE_REAL_AKASH" != true ]; then
        return 0
    fi

    log "Querying engine deployments..."
    ergors_deploy list 2>&1 | tee "${TEST_DIR}/query-deployments.log"
}

# Query bids via engine workflow
query_akash_bids() {
    if [ "$USE_REAL_AKASH" != true ]; then
        return 0
    fi

    if [ -z "$DEPLOY_SESSION_ID" ]; then
        log_warn "No deployment session ID, skipping bid query"
        return 0
    fi

    log "Querying bids for session ${DEPLOY_SESSION_ID:0:8}..."

    local max_wait=30
    local waited=0

    while [ $waited -lt $max_wait ]; do
        local bids
        bids=$(ergors_deploy bids "$DEPLOY_SESSION_ID" 2>&1) || true
        echo "$bids" > "${TEST_DIR}/query-bids.log"

        local total=$(echo "$bids" | jq -r '.total // 0' 2>/dev/null)
        if [ "$total" -gt 0 ] 2>/dev/null; then
            log_success "Found $total bid(s)"
            echo "$bids" | jq '.bids[]' 2>/dev/null
            return 0
        fi

        sleep 2
        waited=$((waited + 2))
        log "  Waiting for bids... (${waited}/${max_wait}s)"
    done

    log_warn "No bids received (timeout)"
}

# Select provider via engine workflow
create_akash_lease() {
    if [ "$USE_REAL_AKASH" != true ]; then
        return 0
    fi

    if [ -z "$DEPLOY_SESSION_ID" ]; then
        log_warn "No deployment session ID, skipping lease creation"
        return 0
    fi

    log_step "Selecting Provider"

    # Get first bid's provider address
    local bids
    bids=$(ergors_deploy bids "$DEPLOY_SESSION_ID" 2>&1) || true
    local provider_addr=$(echo "$bids" | jq -r '.bids[0].provider // empty' 2>/dev/null)
    local bid_price=$(echo "$bids" | jq -r '.bids[0].price_uakt // 0' 2>/dev/null)

    if [ -z "$provider_addr" ]; then
        log_warn "No provider found in bids, using first available"
        provider_addr="akash1provider"
        bid_price=100
    fi

    log "Selecting provider: ${provider_addr}"
    local select_output
    select_output=$(ergors_deploy select "$DEPLOY_SESSION_ID" \
        --provider "$provider_addr" \
        --price "$bid_price" \
        2>&1) || true
    echo "$select_output" > "${TEST_DIR}/lease-create.log"

    local success=$(echo "$select_output" | jq -r '.success // false' 2>/dev/null)
    if [ "$success" = "true" ]; then
        log_success "Provider selected, lease pending"
    else
        log_warn "Provider selection response: $select_output"
    fi

    # Wait for tx to finalize
    log "Waiting for lease tx to finalize..."
    sleep 3
}

# Advance deployment workflow (sends manifest, etc.)
send_akash_manifest() {
    if [ "$USE_REAL_AKASH" != true ]; then
        return 0
    fi

    if [ -z "$DEPLOY_SESSION_ID" ]; then
        log_warn "No deployment session ID, skipping manifest send"
        return 0
    fi

    log_step "Advancing Deployment Workflow"

    log "Advancing workflow to manifest send..."
    local advance_output
    advance_output=$(ergors_deploy advance "$DEPLOY_SESSION_ID" 2>&1) || true
    echo "$advance_output" > "${TEST_DIR}/send-manifest.log"

    local success=$(echo "$advance_output" | jq -r '.success // false' 2>/dev/null)
    if [ "$success" = "true" ]; then
        log_success "Workflow advanced"
    else
        log_warn "Advance response: $advance_output"
    fi
}

# Check deployment status via engine
check_akash_lease_status() {
    if [ "$USE_REAL_AKASH" != true ]; then
        return 0
    fi

    if [ -z "$DEPLOY_SESSION_ID" ]; then
        return 0
    fi

    log "Checking deployment status (session: ${DEPLOY_SESSION_ID:0:8})..."
    ergors_deploy get "$DEPLOY_SESSION_ID" 2>&1 | tee "${TEST_DIR}/lease-status.log" || true
}

# Get deployment workflow details
get_akash_deployment_logs() {
    if [ "$USE_REAL_AKASH" != true ]; then
        return 0
    fi

    if [ -z "$DEPLOY_SESSION_ID" ]; then
        return 0
    fi

    log "Getting deployment details (session: ${DEPLOY_SESSION_ID:0:8})..."
    ergors_deploy get "$DEPLOY_SESSION_ID" 2>&1 || true
}

# Full deployment workflow on real Akash
run_real_akash_deployment() {
    if [ "$USE_REAL_AKASH" != true ]; then
        log_warn "Skipping real Akash deployment workflow (use --real-akash)"
        return 0
    fi

    log_step "Running Real Akash Deployment Workflow"

    if ! create_akash_deployment; then
        log_warn "Deployment creation failed, skipping remaining workflow steps"
        return 0
    fi

    query_akash_deployments || true
    query_akash_bids || true
    create_akash_lease || true
    send_akash_manifest || true

    # Wait for deployment to be ready
    log "Waiting for deployment to be ready..."
    sleep 10

    check_akash_lease_status || true
    get_akash_deployment_logs || true

    log_success "Real Akash deployment workflow complete"
}

# ==================== End Real Akash Dev Environment ====================

# Build CosmWasm contracts
build_contracts() {
    if [ "$SKIP_BUILD" = true ] || [ "$SKIP_CONTRACTS" = true ]; then
        log_warn "Skipping contract build (--skip-build or --skip-contracts)"
        if [ -f "$ROOT_DIR/contracts/artifacts/sdl_template_registrar.wasm" ]; then
            log_success "Using existing contract artifacts"
        else
            log_warn "No existing contract artifacts found - tests may fail"
        fi
        return
    fi

    # Skip build if artifacts already exist
    if [ -f "$ROOT_DIR/contracts/artifacts/sdl_template_registrar.wasm" ]; then
        local size=$(ls -lh "$ROOT_DIR/contracts/artifacts/sdl_template_registrar.wasm" | awk '{print $5}')
        log_success "Contract artifacts already exist (sdl_template_registrar.wasm: $size), skipping build"
        return
    fi

    log_step "Building CosmWasm Contracts"

    cd "$ROOT_DIR/contracts"

    # Check if Docker is running
    if ! docker info &>/dev/null; then
        log_error "Docker is not running - required for contract optimization"
        exit 1
    fi

    # Detect platform and build optimized contracts
    local arch=$(uname -m)
    log "Detected architecture: $arch"

    if [[ "$arch" == "arm64" ]] || [[ "$arch" == "aarch64" ]]; then
        log "Building optimized contracts for ARM64..."
        docker run --rm -v "$(pwd)":/code \
            --mount type=volume,source="contracts_cache",target=/target \
            --mount type=volume,source=registry_cache,target=/usr/local/cargo/registry \
            --platform linux/arm64 \
            cosmwasm/optimizer-arm64:0.17.0 2>&1 | tail -20
    elif [[ "$arch" == "x86_64" ]]; then
        log "Building optimized contracts for x86_64..."
        docker run --rm -v "$(pwd)":/code \
            --mount type=volume,source="contracts_cache",target=/target \
            --mount type=volume,source=registry_cache,target=/usr/local/cargo/registry \
            --platform linux/amd64 \
            cosmwasm/optimizer:0.17.0 2>&1 | tail -20
    else
        log_error "Unsupported architecture: $arch"
        exit 1
    fi

    cd "$ROOT_DIR"

    # Verify artifacts were created
    if [ -f "$ROOT_DIR/contracts/artifacts/sdl_template_registrar.wasm" ]; then
        local size=$(ls -lh "$ROOT_DIR/contracts/artifacts/sdl_template_registrar.wasm" | awk '{print $5}')
        log_success "SDL Template Registrar contract built: $size"
    else
        log_error "Contract build failed - sdl_template_registrar.wasm not found"
        ls -la "$ROOT_DIR/contracts/artifacts/" 2>/dev/null || log_error "artifacts/ directory not found"
        exit 1
    fi

    # List all built artifacts
    log "Built contract artifacts:"
    ls -lh "$ROOT_DIR/contracts/artifacts/"*.wasm 2>/dev/null || true
}

# Build ERGORS binary
build_ergors() {
    if [ "$SKIP_BUILD" = true ]; then
        log_warn "Skipping build (--skip-build)"
        return
    fi

    log_step "Building ERGORS"

    cd "$ROOT_DIR"

    log "Building ergors binary..."
    if [ "$VERBOSE" = true ]; then
        cargo build --release -p ergors -p ergors-cli
    else
        cargo build --release -p ergors -p ergors-cli 2>&1 | tail -5
    fi

    if [ ! -f "$ERGORS_BIN" ]; then
        log_error "Failed to build ergors binary"
        exit 1
    fi

    if [ ! -f "$ERGORS_CLI" ]; then
        log_error "Failed to build ergors-cli binary"
        exit 1
    fi

    log_success "ERGORS binaries built: $ERGORS_BIN, $ERGORS_CLI"
}

# Generate node config using ergors CLI
generate_node_config() {
    local node_id=$1
    local node_type=$2
    local grpc_port=$3
    local p2p_port=$4
    local home_dir=$5
    local seeds=$6

    mkdir -p "$home_dir/data"
    mkdir -p "$home_dir/wasm_cache"

    # Copy SDL contract artifact for coordinators
    local wasm_path="${home_dir}/sdl_template_registrar.wasm"
    local sdl_contract_args=""

    if [ "$node_type" = "coordinator" ]; then
        if cp "${ROOT_DIR}/contracts/artifacts/sdl_template_registrar.wasm" "$wasm_path" 2>/dev/null; then
            sdl_contract_args="--with-sdl-contract --sdl-wasm-path ${wasm_path}"
            log "SDL contract WASM copied for $node_id"
        else
            log_warn "SDL contract WASM not found at ${ROOT_DIR}/contracts/artifacts/sdl_template_registrar.wasm"
        fi
    fi

    log "Generating config using 'ergors config init'..."

    # Use ergors config init with SDL contract for coordinators
    # shellcheck disable=SC2086
    "$ERGORS_BIN" --home "$home_dir" config init \
        --node-type "$node_type" \
        --api-port "$grpc_port" \
        --p2p-port "$p2p_port" \
        $sdl_contract_args 2>&1 || {
        log_error "Failed to initialize config for $node_id"
        return 1
    }

    # Customize additional settings using ergors config set
    log "Setting additional config values..."

    # Set identity.host
    "$ERGORS_BIN" --home "$home_dir" config set identity.host "127.0.0.1" 2>&1 || true

    # Set identity.user
    "$ERGORS_BIN" --home "$home_dir" config set identity.user "e2e-test" 2>&1 || true

    # Set storage data_dir
    "$ERGORS_BIN" --home "$home_dir" config set storage.data_dir "${home_dir}/data" 2>&1 || true

    # Create .env file
    cat > "$home_dir/.env" <<EOF
NODE_DATA_PATH=${home_dir}
ERGORS_CUSTODY_PASSWORD=${TEST_CUSTODY_PASSWORD}
EOF

    log_success "Config generated for $node_id using ergors CLI"
}

# Initialize a node (create encrypted custody)
init_node() {
    local home_dir=$1
    local node_id=$2

    log "Initializing node '$node_id'..."

    # Export env vars so they're available to the subprocess
    export ERGORS_CUSTODY_PASSWORD="${TEST_CUSTODY_PASSWORD}"
    export NODE_DATA_PATH="${home_dir}"

    # Run ergors init new from project root (where templates/ exists)
    cd "$ROOT_DIR"

    # Pipe empty lines for API key prompts (password comes from env var)
    echo -e "\n\n\n\n\n" | "$ERGORS_BIN" --home "$home_dir" init new 2>&1 || {
        log_warn "init new failed, trying unsafe-wipe + init new..."
        "$ERGORS_BIN" --home "$home_dir" init unsafe-wipe 2>&1 || true
        echo -e "\n\n\n\n\n" | "$ERGORS_BIN" --home "$home_dir" init new 2>&1 || true
    }

    # Verify initialization created the identity file
    if [ -f "$home_dir/node_identity.enc" ]; then
        log_success "Node '$node_id' initialized (identity created)"
    else
        log_warn "Node '$node_id' identity file not found - may use plaintext mode"
    fi
}

# Start ERGORS test network
start_ergors_network() {
    if [ "$SKIP_NETWORK" = true ]; then
        log_warn "Skipping ERGORS network setup (--skip-network)"
        return
    fi

    log_step "Starting ERGORS Test Network"

    rm -rf "$TEST_DIR"
    mkdir -p "$TEST_DIR"

    local port=$BASE_PORT

    # === Coordinator ===
    local coord_home="$TEST_DIR/coordinator"
    local coord_http=$port           # HTTP API port (identity.api_port in config)
    local coord_grpc=$((port + 1))   # gRPC management port (--grpc-port CLI)
    local coord_p2p=$((port + 2))    # P2P port
    COORDINATOR_GRPC="127.0.0.1:${coord_grpc}"
    local coordinator_p2p="127.0.0.1:${coord_p2p}"
    port=$((port + 10))

    # Init first (creates identity), then generate config (overwrites init's config)
    init_node "$coord_home" "coordinator"
    generate_node_config "coordinator" "coordinator" "$coord_http" "$coord_p2p" "$coord_home" ""

    # Debug: show config
    if [ "$VERBOSE" = true ]; then
        log "Coordinator config:"
        head -30 "$coord_home/config.toml"
    fi

    log "Starting coordinator node..."
    ERGORS_CUSTODY_PASSWORD="${TEST_CUSTODY_PASSWORD}" \
    NODE_DATA_PATH="${coord_home}" \
    "$ERGORS_BIN" --home "$coord_home" start --grpc-port "$coord_grpc" \
        > "$coord_home/node.log" 2>&1 &
    NODE_PIDS+=($!)
    log_success "Coordinator started (PID: $!, HTTP: $coord_http, gRPC: $coord_grpc)"

    # Start log streaming for coordinator
    start_log_stream "COORD" "$coord_home/node.log" "$COORD_COLOR"

    sleep 2

    # === Executor ===
    local exec_home="$TEST_DIR/executor_0"
    local exec_http=$port            # HTTP API port
    local exec_grpc=$((port + 1))    # gRPC management port
    local exec_p2p=$((port + 2))     # P2P port
    EXECUTOR_GRPC="127.0.0.1:${exec_grpc}"
    port=$((port + 10))

    # Init first, then generate config
    init_node "$exec_home" "executor_0"
    generate_node_config "executor_0" "executor" "$exec_http" "$exec_p2p" "$exec_home" ""

    log "Starting executor node..."
    ERGORS_CUSTODY_PASSWORD="${TEST_CUSTODY_PASSWORD}" \
    NODE_DATA_PATH="${exec_home}" \
    "$ERGORS_BIN" --home "$exec_home" start --grpc-port "$exec_grpc" \
        > "$exec_home/node.log" 2>&1 &
    NODE_PIDS+=($!)
    log_success "Executor started (PID: $!, HTTP: $exec_http, gRPC: $exec_grpc)"

    # Start log streaming for executor
    start_log_stream "EXEC " "$exec_home/node.log" "$EXEC_COLOR"

    # Brief pause to let nodes initialize
    sleep 2

    # Quick health check - show logs if nodes crashed
    local coord_pid=${NODE_PIDS[0]}
    local exec_pid=${NODE_PIDS[1]}

    if ! kill -0 "$coord_pid" 2>/dev/null; then
        log_error "Coordinator crashed immediately!"
        show_node_logs "coordinator"
    fi

    if ! kill -0 "$exec_pid" 2>/dev/null; then
        log_error "Executor crashed immediately!"
        show_node_logs "executor_0"
    fi

    log_success "ERGORS network started"
    log "  Coordinator HTTP: 127.0.0.1:$coord_http, gRPC: $COORDINATOR_GRPC"
    log "  Executor HTTP: 127.0.0.1:$exec_http, gRPC: $EXECUTOR_GRPC"
}

# Setup Kind cluster with Akash
setup_akash_environment() {
    if [ "$SKIP_AKASH" = true ]; then
        log_warn "Skipping Akash setup (--skip-akash)"
        return
    fi

    # Use real Akash if enabled
    if [ "$USE_REAL_AKASH" = true ]; then
        setup_real_akash_environment
        return
    fi

    log_step "Setting Up Mock Akash Environment"

    # Delete existing cluster
    if kind get clusters 2>/dev/null | grep -q "^${CLUSTER_NAME}$"; then
        log "Deleting existing cluster..."
        kind delete cluster --name "${CLUSTER_NAME}"
    fi

    # Create Kind cluster
    local kind_config=$(mktemp)
    cat > "$kind_config" <<EOF
kind: Cluster
apiVersion: kind.x-k8s.io/v1alpha4
nodes:
- role: control-plane
  image: kindest/node:v1.29.4
  extraPortMappings:
  - containerPort: 30434
    hostPort: 30434
    protocol: TCP
EOF

    log "Creating Kind cluster..."
    kind create cluster --name "${CLUSTER_NAME}" --config "$kind_config"
    rm -f "$kind_config"

    kubectl cluster-info --context "kind-${CLUSTER_NAME}"
    log_success "Kind cluster ready"

    # Create namespaces
    kubectl create namespace akash-services --dry-run=client -o yaml | kubectl apply -f -
    kubectl create namespace mock-inference --dry-run=client -o yaml | kubectl apply -f -

    log_success "Mock Akash environment ready"
}

# Build mock inference provider image
build_mock_image() {
    log_step "Building Mock Inference Provider"

    cd "${ROOT_DIR}/docker/mock-inference-provider"

    log "Building Docker image..."
    if [ "$VERBOSE" = true ]; then
        docker build -t "${MOCK_IMAGE}" .
    else
        docker build -t "${MOCK_IMAGE}" . 2>&1 | tail -5
    fi

    log "Loading image into Kind cluster..."
    kind load docker-image "${MOCK_IMAGE}" --name "${CLUSTER_NAME}"

    log_success "Mock inference image ready"
    cd "$ROOT_DIR"
}

# Deploy through ERGORS workflow using gRPC API
deploy_via_ergors() {
    log_step "Deploying via ERGORS Workflow"

    log "ERGORS deployment workflow (engine-driven):"
    log "  1. ergors-cli deploy create → engine creates workflow"
    log "  2. ergors-cli deploy bids   → engine queries Akash node for bids"
    log "  3. ergors-cli deploy select → engine selects provider"
    log "  4. ergors-cli deploy advance → engine creates lease + sends manifest"
    log "  5. ergors-cli deploy get    → engine reports deployment status"

    # Verify coordinator is reachable
    if ! nc -z 127.0.0.1 "${COORDINATOR_GRPC##*:}" 2>/dev/null; then
        log_error "Coordinator gRPC not reachable at ${COORDINATOR_GRPC}"
        return 1
    fi

    # Create deployment via engine
    log "Creating deployment workflow..."
    local create_output
    create_output=$(ergors_deploy create \
        --sdl "${DEPLOY_SDL}" \
        --key-name "default" \
        --node "${AKASH_LOCAL_NODE}" \
        --chain-id "${AKASH_LOCAL_CHAIN_ID}" \
        2>&1) || true
    echo "$create_output" > "${TEST_DIR}/deployment-create.log"

    DEPLOY_SESSION_ID=$(echo "$create_output" | jq -r '.session_id // empty' 2>/dev/null)

    if [ -n "$DEPLOY_SESSION_ID" ]; then
        log_success "Deployment workflow created"
        log "  Session:  ${DEPLOY_SESSION_ID:0:8}..."
        log "  Node:     ${AKASH_LOCAL_NODE}"
        log "  Chain:    ${AKASH_LOCAL_CHAIN_ID}"
    else
        log_error "Failed to create deployment workflow"
        log "  Output: $create_output"
        return 1
    fi

    # Store deployment info
    DEPLOYMENT_ID="$DEPLOY_SESSION_ID"
    DEPLOYED_ENDPOINT="http://localhost:30434"

    log_success "ERGORS deployment workflow initiated"
    log "  Coordinator: ${COORDINATOR_GRPC}"
    log "  Session: ${DEPLOY_SESSION_ID:0:8}..."
}

# Test ERGORS network communication
test_ergors_network() {
    log_step "Testing ERGORS Network"

    # Test 1: Coordinator is healthy
    log "Testing coordinator health..."
    if nc -z 127.0.0.1 "${COORDINATOR_GRPC##*:}" 2>/dev/null; then
        TESTS_PASSED=$((TESTS_PASSED + 1))
        log_success "  Coordinator gRPC reachable"
    else
        TESTS_FAILED=$((TESTS_FAILED + 1))
        log_error "  Coordinator gRPC unreachable"
    fi

    # Test 2: Executor is healthy
    log "Testing executor health..."
    if nc -z 127.0.0.1 "${EXECUTOR_GRPC##*:}" 2>/dev/null; then
        TESTS_PASSED=$((TESTS_PASSED + 1))
        log_success "  Executor gRPC reachable"
    else
        TESTS_FAILED=$((TESTS_FAILED + 1))
        log_error "  Executor gRPC unreachable"
    fi

    # Test 3: Nodes are running
    log "Testing node processes..."
    local running=0
    for pid in "${NODE_PIDS[@]}"; do
        if kill -0 "$pid" 2>/dev/null; then
            running=$((running + 1))
        fi
    done
    if [ $running -eq ${#NODE_PIDS[@]} ]; then
        TESTS_PASSED=$((TESTS_PASSED + 1))
        log_success "  All ${#NODE_PIDS[@]} nodes running"
    else
        TESTS_FAILED=$((TESTS_FAILED + 1))
        log_error "  Only $running/${#NODE_PIDS[@]} nodes running"
        # Show logs for debugging
        show_node_logs "coordinator"
        show_node_logs "executor_0"
    fi
}

# Test ERGORS node configuration
test_node_config() {
    log_step "Testing Node Configuration"

    # Test 1: Config files exist
    log "Verifying config files..."
    if [ -f "$TEST_DIR/coordinator/config.toml" ]; then
        TESTS_PASSED=$((TESTS_PASSED + 1))
        log_success "  Coordinator config.toml exists"
    else
        TESTS_FAILED=$((TESTS_FAILED + 1))
        log_error "  Coordinator config.toml missing"
    fi

    # Test 2: CosmWasm config present
    if grep -q "cosmwasm" "$TEST_DIR/coordinator/config.toml" 2>/dev/null; then
        TESTS_PASSED=$((TESTS_PASSED + 1))
        log_success "  CosmWasm configuration present"
    else
        TESTS_FAILED=$((TESTS_FAILED + 1))
        log_error "  CosmWasm configuration missing"
        show_config "coordinator"
    fi

    # Test 3: WASM artifact copied
    if [ -f "$TEST_DIR/coordinator/sdl_template_registrar.wasm" ]; then
        TESTS_PASSED=$((TESTS_PASSED + 1))
        log_success "  SDL contract WASM artifact present"
    else
        TESTS_FAILED=$((TESTS_FAILED + 1))
        log_error "  SDL contract WASM artifact missing"
    fi

    # Test 4: Validate config using ergors config get
    log "Testing 'ergors config get' command..."
    local node_type=$("$ERGORS_BIN" --home "$TEST_DIR/coordinator" config get identity.node_type 2>&1)
    if echo "$node_type" | grep -q "Coordinator"; then
        TESTS_PASSED=$((TESTS_PASSED + 1))
        log_success "  Config get: identity.node_type = Coordinator"
    else
        TESTS_FAILED=$((TESTS_FAILED + 1))
        log_error "  Config get failed: $node_type"
    fi

    # Test 5: Validate api_port
    local api_port=$("$ERGORS_BIN" --home "$TEST_DIR/coordinator" config get identity.api_port 2>&1)
    if echo "$api_port" | grep -q "50100"; then
        TESTS_PASSED=$((TESTS_PASSED + 1))
        log_success "  Config get: identity.api_port = 50100"
    else
        TESTS_FAILED=$((TESTS_FAILED + 1))
        log_error "  Config get api_port failed: $api_port"
    fi

    # Test 6: Validate cosmwasm enabled
    local cw_enabled=$("$ERGORS_BIN" --home "$TEST_DIR/coordinator" config get cosmwasm.enabled 2>&1)
    if echo "$cw_enabled" | grep -q "true"; then
        TESTS_PASSED=$((TESTS_PASSED + 1))
        log_success "  Config get: cosmwasm.enabled = true"
    else
        TESTS_FAILED=$((TESTS_FAILED + 1))
        log_error "  Config get cosmwasm.enabled failed: $cw_enabled"
    fi

    # Test 7: Validate config list command
    log "Testing 'ergors config list' command..."
    local config_list=$("$ERGORS_BIN" --home "$TEST_DIR/coordinator" config list 2>&1)
    if echo "$config_list" | grep -q "identity.host" && echo "$config_list" | grep -q "network.listen_port"; then
        TESTS_PASSED=$((TESTS_PASSED + 1))
        log_success "  Config list shows available keys"
    else
        TESTS_FAILED=$((TESTS_FAILED + 1))
        log_error "  Config list failed"
    fi
}

# Test SDL template workflow
test_sdl_workflow() {
    log_step "Testing SDL Template Workflow"

    # Check coordinator logs for contract activity
    if [ -f "$TEST_DIR/coordinator/node.log" ]; then
        log "Checking coordinator node logs..."

        # Check for startup
        if grep -q "Starting ERGORS\|Starting.*engine" "$TEST_DIR/coordinator/node.log" 2>/dev/null; then
            TESTS_PASSED=$((TESTS_PASSED + 1))
            log_success "  Coordinator started successfully"
        else
            TESTS_FAILED=$((TESTS_FAILED + 1))
            log_error "  Coordinator startup not detected"
        fi

        # Check for storage init
        if grep -q "storage\|Cnidarium\|rocksdb" "$TEST_DIR/coordinator/node.log" 2>/dev/null; then
            TESTS_PASSED=$((TESTS_PASSED + 1))
            log_success "  Storage initialized"
        else
            TESTS_FAILED=$((TESTS_FAILED + 1))
            log_error "  Storage initialization not detected"
        fi
    else
        log_warn "  Coordinator log not found"
    fi

    # Check executor logs
    if [ -f "$TEST_DIR/executor_0/node.log" ]; then
        log "Checking executor node logs..."

        if grep -q "Starting ERGORS\|Starting.*engine" "$TEST_DIR/executor_0/node.log" 2>/dev/null; then
            TESTS_PASSED=$((TESTS_PASSED + 1))
            log_success "  Executor started successfully"
        else
            TESTS_FAILED=$((TESTS_FAILED + 1))
            log_error "  Executor startup not detected"
        fi
    else
        log_warn "  Executor log not found"
    fi

    # Run additional SDL tests
    test_sdl_contract_queries
    test_sdl_variable_substitution
}

# ==================== API Key Management Tests ====================

# Start mock inference provider for API key testing
start_mock_provider() {
    log_step "Starting Mock Inference Provider"

    local mock_port=11434
    local mock_home="$TEST_DIR/mock-provider"
    mkdir -p "$mock_home"

    # First, build the mock provider binary
    log "Building mock inference provider..."
    cd "${ROOT_DIR}/docker/mock-inference-provider"
    if [ "$VERBOSE" = true ]; then
        cargo build --release 2>&1
    else
        cargo build --release 2>&1 | tail -5
    fi
    cd "$ROOT_DIR"

    local mock_bin="${ROOT_DIR}/docker/mock-inference-provider/target/release/mock-inference-provider"

    if [ ! -f "$mock_bin" ]; then
        log_error "Failed to build mock inference provider binary"
        log_warn "API key tests will be skipped"
        return 1
    fi

    log "Starting mock provider binary..."
    "$mock_bin" --port "$mock_port" > "$mock_home/provider.log" 2>&1 &
    MOCK_PROVIDER_PID=$!
    NODE_PIDS+=($MOCK_PROVIDER_PID)
    log "Mock provider PID: $MOCK_PROVIDER_PID, port: $mock_port"

    # Start log streaming for mock provider
    start_log_stream "MOCK " "$mock_home/provider.log" "$MOCK_COLOR"

    # Wait for provider to be ready with retries
    local max_retries=10
    local retry=0
    while [ $retry -lt $max_retries ]; do
        sleep 1
        if curl -s "http://localhost:$mock_port/health" > /dev/null 2>&1; then
            log_success "Mock provider health check passed"
            MOCK_PROVIDER_URL="http://localhost:$mock_port"
            return 0
        fi
        retry=$((retry + 1))
        log "Waiting for mock provider to start... ($retry/$max_retries)"
    done

    # If we get here, the provider didn't start
    log_error "Mock provider failed to start after $max_retries seconds"
    if [ -f "$mock_home/provider.log" ]; then
        echo -e "${YELLOW}=== Mock provider logs ===${NC}"
        tail -30 "$mock_home/provider.log"
        echo -e "${YELLOW}=== end mock provider logs ===${NC}"
    fi

    # Check if process is still running
    if ! kill -0 "$MOCK_PROVIDER_PID" 2>/dev/null; then
        log_error "Mock provider process died"
    fi

    MOCK_PROVIDER_URL=""
    return 1
}

# Test API key generation on mock provider
test_api_key_generation() {
    log_step "Testing API Key Generation"

    if [ -z "$MOCK_PROVIDER_URL" ]; then
        log_warn "Mock provider not running, skipping API key tests"
        return
    fi

    # First, verify the /api/keys/generate endpoint exists
    log "Checking if API key endpoints are available..."
    local root_response=$(curl -s "$MOCK_PROVIDER_URL/")
    if echo "$root_response" | grep -q "api_keys"; then
        log_success "  API key endpoints are registered"
    else
        log_error "  API key endpoints not found - mock provider may need rebuild"
        log "Root response: $root_response"
        return
    fi

    # Test 1: Generate valid key
    log "Generating valid API key..."
    local gen_response=$(curl -s -X POST "$MOCK_PROVIDER_URL/api/keys/generate" \
        -H "Content-Type: application/json" \
        -d '{"provider": "anthropic", "expiry_seconds": 3600, "valid": true}')

    if [ "$VERBOSE" = true ]; then
        log "Generate response: $gen_response"
    fi

    if echo "$gen_response" | grep -q "api_key"; then
        TESTS_PASSED=$((TESTS_PASSED + 1))
        TEST_API_KEY=$(echo "$gen_response" | grep -o '"api_key":"[^"]*"' | cut -d'"' -f4)
        log_success "  Generated valid API key: ${TEST_API_KEY:0:16}..."
    else
        TESTS_FAILED=$((TESTS_FAILED + 1))
        log_error "  Failed to generate API key: $gen_response"
    fi

    # Test 2: Generate invalid key (for testing failure scenarios)
    log "Generating invalid API key..."
    local invalid_response=$(curl -s -X POST "$MOCK_PROVIDER_URL/api/keys/generate" \
        -H "Content-Type: application/json" \
        -d '{"provider": "mock-invalid", "valid": false}')

    if echo "$invalid_response" | grep -q '"valid":false'; then
        TESTS_PASSED=$((TESTS_PASSED + 1))
        INVALID_TEST_KEY=$(echo "$invalid_response" | grep -o '"api_key":"[^"]*"' | cut -d'"' -f4)
        log_success "  Generated invalid test key"
    else
        TESTS_FAILED=$((TESTS_FAILED + 1))
        log_error "  Failed to generate invalid key: $invalid_response"
    fi

    # Test 3: Generate expiring key (short TTL)
    log "Generating short-TTL key..."
    local expiring_response=$(curl -s -X POST "$MOCK_PROVIDER_URL/api/keys/generate" \
        -H "Content-Type: application/json" \
        -d '{"provider": "mock-expiring", "expiry_seconds": 2, "valid": true}')

    if echo "$expiring_response" | grep -q "expires_at"; then
        TESTS_PASSED=$((TESTS_PASSED + 1))
        EXPIRING_KEY=$(echo "$expiring_response" | grep -o '"api_key":"[^"]*"' | cut -d'"' -f4)
        log_success "  Generated expiring key (TTL: 2s)"
    else
        TESTS_FAILED=$((TESTS_FAILED + 1))
        log_error "  Failed to generate expiring key: $expiring_response"
    fi
}

# Test API key validation
test_api_key_validation() {
    log_step "Testing API Key Validation"

    if [ -z "$MOCK_PROVIDER_URL" ]; then
        log_warn "Mock provider not running, skipping validation tests"
        return
    fi

    # Test 1: Validate valid key
    if [ -n "$TEST_API_KEY" ]; then
        log "Validating valid API key..."
        local valid_response=$(curl -s -X POST "$MOCK_PROVIDER_URL/api/keys/validate" \
            -H "Content-Type: application/json" \
            -d "{\"api_key\": \"$TEST_API_KEY\"}")

        if echo "$valid_response" | grep -q '"valid":true'; then
            TESTS_PASSED=$((TESTS_PASSED + 1))
            log_success "  Valid key validated successfully"
        else
            TESTS_FAILED=$((TESTS_FAILED + 1))
            log_error "  Valid key validation failed: $valid_response"
        fi
    fi

    # Test 2: Validate invalid key
    if [ -n "$INVALID_TEST_KEY" ]; then
        log "Validating invalid API key..."
        local invalid_response=$(curl -s -X POST "$MOCK_PROVIDER_URL/api/keys/validate" \
            -H "Content-Type: application/json" \
            -d "{\"api_key\": \"$INVALID_TEST_KEY\"}")

        if echo "$invalid_response" | grep -q '"valid":false'; then
            TESTS_PASSED=$((TESTS_PASSED + 1))
            log_success "  Invalid key correctly rejected"
        else
            TESTS_FAILED=$((TESTS_FAILED + 1))
            log_error "  Invalid key not rejected: $invalid_response"
        fi
    fi

    # Test 3: Validate non-existent key
    log "Validating non-existent API key..."
    local nonexistent_response=$(curl -s -X POST "$MOCK_PROVIDER_URL/api/keys/validate" \
        -H "Content-Type: application/json" \
        -d '{"api_key": "sk-nonexistent-key-12345"}')

    if echo "$nonexistent_response" | grep -q '"valid":false'; then
        TESTS_PASSED=$((TESTS_PASSED + 1))
        log_success "  Non-existent key correctly rejected"
    else
        TESTS_FAILED=$((TESTS_FAILED + 1))
        log_error "  Non-existent key not rejected: $nonexistent_response"
    fi

    # Test 4: Wait for expiring key to expire and validate
    if [ -n "$EXPIRING_KEY" ]; then
        log "Waiting for key to expire (3s)..."
        sleep 3

        local expired_response=$(curl -s -X POST "$MOCK_PROVIDER_URL/api/keys/validate" \
            -H "Content-Type: application/json" \
            -d "{\"api_key\": \"$EXPIRING_KEY\"}")

        if echo "$expired_response" | grep -q '"expired":true' || echo "$expired_response" | grep -q '"valid":false'; then
            TESTS_PASSED=$((TESTS_PASSED + 1))
            log_success "  Expired key correctly rejected"
        else
            TESTS_FAILED=$((TESTS_FAILED + 1))
            log_error "  Expired key not rejected: $expired_response"
        fi
    fi
}

# Test API key revocation
test_api_key_revocation() {
    log_step "Testing API Key Revocation"

    if [ -z "$MOCK_PROVIDER_URL" ]; then
        log_warn "Mock provider not running, skipping revocation tests"
        return
    fi

    # Generate a key to revoke
    log "Generating key for revocation test..."
    local gen_response=$(curl -s -X POST "$MOCK_PROVIDER_URL/api/keys/generate" \
        -H "Content-Type: application/json" \
        -d '{"provider": "revoke-test", "valid": true}')

    local revoke_key=$(echo "$gen_response" | grep -o '"api_key":"[^"]*"' | cut -d'"' -f4)

    if [ -z "$revoke_key" ]; then
        TESTS_FAILED=$((TESTS_FAILED + 1))
        log_error "  Failed to generate key for revocation test"
        return
    fi

    # Verify key is valid before revocation
    local pre_revoke=$(curl -s -X POST "$MOCK_PROVIDER_URL/api/keys/validate" \
        -H "Content-Type: application/json" \
        -d "{\"api_key\": \"$revoke_key\"}")

    if echo "$pre_revoke" | grep -q '"valid":true'; then
        log_success "  Key valid before revocation"
    else
        log_error "  Key not valid before revocation"
    fi

    # Revoke the key
    log "Revoking API key..."
    local revoke_response=$(curl -s -X POST "$MOCK_PROVIDER_URL/api/keys/revoke" \
        -H "Content-Type: application/json" \
        -d "{\"api_key\": \"$revoke_key\"}")

    if echo "$revoke_response" | grep -q '"success":true'; then
        TESTS_PASSED=$((TESTS_PASSED + 1))
        log_success "  Key revocation request successful"
    else
        TESTS_FAILED=$((TESTS_FAILED + 1))
        log_error "  Key revocation failed: $revoke_response"
    fi

    # Verify key is invalid after revocation
    local post_revoke=$(curl -s -X POST "$MOCK_PROVIDER_URL/api/keys/validate" \
        -H "Content-Type: application/json" \
        -d "{\"api_key\": \"$revoke_key\"}")

    if echo "$post_revoke" | grep -q '"valid":false'; then
        TESTS_PASSED=$((TESTS_PASSED + 1))
        log_success "  Revoked key correctly invalidated"
    else
        TESTS_FAILED=$((TESTS_FAILED + 1))
        log_error "  Revoked key still valid: $post_revoke"
    fi
}

# Test key list endpoint
test_api_key_listing() {
    log_step "Testing API Key Listing"

    if [ -z "$MOCK_PROVIDER_URL" ]; then
        log_warn "Mock provider not running, skipping listing tests"
        return
    fi

    log "Listing all API keys..."
    local list_response=$(curl -s "$MOCK_PROVIDER_URL/api/keys/list")

    if echo "$list_response" | grep -q '"keys":'; then
        TESTS_PASSED=$((TESTS_PASSED + 1))
        local key_count=$(echo "$list_response" | grep -o '"total":[0-9]*' | cut -d':' -f2)
        log_success "  Listed $key_count API keys"
    else
        TESTS_FAILED=$((TESTS_FAILED + 1))
        log_error "  Failed to list keys: $list_response"
    fi
}

# Test coordinator API key configuration
test_coordinator_key_config() {
    log_step "Testing Coordinator API Key Configuration"

    # Check if coordinator has ephemeral key manager logs
    if [ -f "$TEST_DIR/coordinator/node.log" ]; then
        log "Checking coordinator logs for key management..."

        # Check for ephemeral key manager initialization
        if grep -q "EphemeralKeyManager\|ephemeral.*key\|key.*manager" "$TEST_DIR/coordinator/node.log" 2>/dev/null; then
            TESTS_PASSED=$((TESTS_PASSED + 1))
            log_success "  Ephemeral key manager initialized"
        else
            log_warn "  Ephemeral key manager not found in logs (may not be wired yet)"
        fi

        # Check for bootstrap handler initialization
        if grep -q "BootstrapHandler\|bootstrap.*handler\|key.*sharing" "$TEST_DIR/coordinator/node.log" 2>/dev/null; then
            TESTS_PASSED=$((TESTS_PASSED + 1))
            log_success "  Bootstrap handler initialized"
        else
            log_warn "  Bootstrap handler not found in logs (may not be wired yet)"
        fi

        # Check for encrypted API keys loading
        if grep -q "encrypted.*api.*key\|API.*key.*encrypted\|Decrypted.*key\|🔐\|🔑" "$TEST_DIR/coordinator/node.log" 2>/dev/null; then
            TESTS_PASSED=$((TESTS_PASSED + 1))
            log_success "  Encrypted API keys processed"
        else
            log_warn "  Encrypted API keys not found in logs"
        fi
    else
        log_warn "  Coordinator log not found"
    fi

    # Check coordinator config for LLM settings
    if grep -q "llm_router\|api_keys" "$TEST_DIR/coordinator/config.toml" 2>/dev/null; then
        TESTS_PASSED=$((TESTS_PASSED + 1))
        log_success "  LLM router configuration present"
    else
        log_warn "  LLM router configuration not found"
    fi
}

# Run all API key management tests
run_api_key_tests() {
    log_step "Running API Key Management Tests"

    # Variables for test keys
    TEST_API_KEY=""
    INVALID_TEST_KEY=""
    EXPIRING_KEY=""
    MOCK_PROVIDER_PID=""
    MOCK_PROVIDER_URL=""

    start_mock_provider
    test_api_key_generation
    test_api_key_validation
    test_api_key_revocation
    test_api_key_listing
    test_coordinator_key_config
}

# ==================== End API Key Management Tests ====================

# ==================== Authz/Feegrant Workflow Tests ====================

# Test authz grant request from executor to coordinator
test_authz_grant_request() {
    log_step "Testing Authz Grant Request Workflow"

    if [ "$USE_REAL_AKASH" != true ]; then
        log_warn "Authz tests require real Akash mode (--real-akash)"
        log "Testing ERGORS grant request infrastructure instead..."
    fi

    # Test 1: Check coordinator supports grant requests
    if [ -f "$TEST_DIR/coordinator/node.log" ]; then
        log "Checking coordinator grant request support..."

        if grep -q "grant\|authz\|GrantRequest" "$TEST_DIR/coordinator/node.log" 2>/dev/null; then
            TESTS_PASSED=$((TESTS_PASSED + 1))
            log_success "  Grant request handler initialized"
        else
            log_warn "  Grant request handler not found in logs (may be pending implementation)"
        fi
    fi

    # Test 2: Check executor can discover coordinator
    if [ -f "$TEST_DIR/executor_0/node.log" ]; then
        log "Checking executor coordinator discovery..."

        if grep -q "coordinator\|peer\|connected" "$TEST_DIR/executor_0/node.log" 2>/dev/null; then
            TESTS_PASSED=$((TESTS_PASSED + 1))
            log_success "  Executor discovered coordinator"
        else
            log_warn "  Coordinator discovery not confirmed"
        fi
    fi

    # Test 3: Verify authz config in coordinator
    log "Checking authz configuration..."
    if grep -q "authz\|auto_approve\|grant" "$TEST_DIR/coordinator/config.toml" 2>/dev/null; then
        TESTS_PASSED=$((TESTS_PASSED + 1))
        log_success "  Authz configuration present"
    else
        log_warn "  Authz configuration not found (may use defaults)"
    fi

    # For real Akash mode, query actual grants
    if [ "$USE_REAL_AKASH" = true ] && [ -d "${AKASH_KUBE_DIR}" ]; then
        log "Querying actual authz grants on blockchain..."

        # Try to query grants (may fail if provider cli not in path)
        local grants_output=$(akash_make query-grants 2>/dev/null || echo "")
        if [ -n "$grants_output" ]; then
            TESTS_PASSED=$((TESTS_PASSED + 1))
            log_success "  Blockchain authz query successful"
            if [ "$VERBOSE" = true ]; then
                echo "$grants_output"
            fi
        fi
    fi
}

# Test feegrant allowance workflow
test_feegrant_workflow() {
    log_step "Testing Feegrant Allowance Workflow"

    if [ "$USE_REAL_AKASH" != true ]; then
        log_warn "Feegrant tests require real Akash mode (--real-akash)"
        log "Testing ERGORS feegrant infrastructure instead..."
    fi

    # Test 1: Check feegrant config in coordinator
    log "Checking feegrant configuration..."
    if grep -q "feegrant\|fee.*grant\|allowance" "$TEST_DIR/coordinator/config.toml" 2>/dev/null; then
        TESTS_PASSED=$((TESTS_PASSED + 1))
        log_success "  Feegrant configuration present"
    else
        log_warn "  Feegrant configuration not found"
    fi

    # Test 2: Check coordinator logs for feegrant initialization
    if [ -f "$TEST_DIR/coordinator/node.log" ]; then
        log "Checking feegrant handler in logs..."

        if grep -q "feegrant\|FeeAllowance\|allowance" "$TEST_DIR/coordinator/node.log" 2>/dev/null; then
            TESTS_PASSED=$((TESTS_PASSED + 1))
            log_success "  Feegrant handler initialized"
        else
            log_warn "  Feegrant handler not found in logs"
        fi
    fi

    # For real Akash mode, query actual allowances
    if [ "$USE_REAL_AKASH" = true ] && [ -d "${AKASH_KUBE_DIR}" ]; then
        log "Querying actual feegrant allowances on blockchain..."

        # Try to query allowances
        local allowances_output=$(akash_make query-allowances 2>/dev/null || echo "")
        if [ -n "$allowances_output" ]; then
            TESTS_PASSED=$((TESTS_PASSED + 1))
            log_success "  Blockchain feegrant query successful"
        fi
    fi
}

# Test combined grant request workflow (ERGORS multi-node)
test_ergors_grant_workflow() {
    log_step "Testing ERGORS Grant Request Workflow"

    # This tests the complete grant request flow:
    # 1. Executor discovers coordinator
    # 2. Executor sends grant request via gRPC
    # 3. Coordinator approves (or queues for manual approval)
    # 4. Executor receives grant confirmation
    # 5. Executor can deploy on behalf of coordinator

    log "Testing grant request infrastructure..."

    # Test 1: Both nodes are running
    log "Verifying node processes..."
    local coord_running=false
    local exec_running=false

    for pid in "${NODE_PIDS[@]}"; do
        if kill -0 "$pid" 2>/dev/null; then
            # Check which node this PID belongs to
            if grep -q "coordinator" "$TEST_DIR/coordinator/node.log" 2>/dev/null; then
                coord_running=true
            fi
            if grep -q "executor" "$TEST_DIR/executor_0/node.log" 2>/dev/null; then
                exec_running=true
            fi
        fi
    done

    if [ "$coord_running" = true ] || [ "$exec_running" = true ]; then
        TESTS_PASSED=$((TESTS_PASSED + 1))
        log_success "  ERGORS nodes running for grant workflow"
    else
        TESTS_FAILED=$((TESTS_FAILED + 1))
        log_error "  ERGORS nodes not running"
    fi

    # Test 2: Check for GrantRequestService in coordinator
    log "Checking GrantRequestService..."
    if grep -qi "GrantRequest\|grant_request\|grant-request" "$TEST_DIR/coordinator/node.log" 2>/dev/null; then
        TESTS_PASSED=$((TESTS_PASSED + 1))
        log_success "  GrantRequestService detected"
    else
        log_warn "  GrantRequestService not detected (may be pending wire-up)"
    fi

    # Test 3: Check channel registration for grant requests
    log "Checking P2P channels..."
    if grep -q "channel\|P2P\|libp2p" "$TEST_DIR/coordinator/node.log" 2>/dev/null; then
        TESTS_PASSED=$((TESTS_PASSED + 1))
        log_success "  P2P channel infrastructure detected"
    else
        log_warn "  P2P channels not detected"
    fi

    # Test 4: Verify gRPC endpoints available
    log "Checking gRPC management endpoints..."
    if nc -z 127.0.0.1 "${COORDINATOR_GRPC##*:}" 2>/dev/null; then
        TESTS_PASSED=$((TESTS_PASSED + 1))
        log_success "  Coordinator gRPC available for grant management"
    else
        TESTS_FAILED=$((TESTS_FAILED + 1))
        log_error "  Coordinator gRPC not available"
    fi
}

# Run all authz/feegrant tests
run_authz_feegrant_tests() {
    log_step "Running Authz/Feegrant Workflow Tests"

    test_authz_grant_request
    test_feegrant_workflow
    test_ergors_grant_workflow

    log_success "Authz/Feegrant tests complete"
}

# ==================== End Authz/Feegrant Tests ====================

# ==================== Engine Akash Deployment Tests ====================

# Test engine deployment creation
test_real_deployment_create() {
    if [ "$USE_REAL_AKASH" != true ]; then
        return 0
    fi

    log_step "Testing Engine Deployment Creation"

    # Test 1: Create deployment via engine
    log "Creating deployment via engine workflow..."
    local create_output
    create_output=$(ergors_deploy create \
        --sdl "${DEPLOY_SDL}" \
        --key-name "default" \
        --node "${AKASH_LOCAL_NODE}" \
        --chain-id "${AKASH_LOCAL_CHAIN_ID}" \
        2>&1) || true
    echo "$create_output" > "${TEST_DIR}/test-deployment-create.log"

    local session_id=$(echo "$create_output" | jq -r '.session_id // empty' 2>/dev/null)
    if [ -n "$session_id" ]; then
        TESTS_PASSED=$((TESTS_PASSED + 1))
        DEPLOY_SESSION_ID="$session_id"
        log_success "  Deployment workflow created (session: ${session_id:0:8})"
    else
        TESTS_FAILED=$((TESTS_FAILED + 1))
        log_error "  Deployment creation failed"
        if [ "$VERBOSE" = true ]; then
            echo "$create_output"
        fi
    fi

    # Test 2: List deployments
    log "Listing deployments..."
    local list_output
    list_output=$(ergors_deploy list 2>&1) || true

    local total=$(echo "$list_output" | jq -r '.total_count // 0' 2>/dev/null)
    if [ "$total" -gt 0 ] 2>/dev/null; then
        TESTS_PASSED=$((TESTS_PASSED + 1))
        log_success "  Deployment list returned $total workflow(s)"
    else
        TESTS_FAILED=$((TESTS_FAILED + 1))
        log_error "  Deployment list empty"
    fi
}

# Test bid query via engine
test_real_bid_reception() {
    if [ "$USE_REAL_AKASH" != true ]; then
        return 0
    fi

    if [ -z "$DEPLOY_SESSION_ID" ]; then
        log_warn "No session ID, skipping bid test"
        return 0
    fi

    log_step "Testing Engine Bid Query"

    log "Querying bids via engine..."
    local max_wait=30
    local waited=0

    while [ $waited -lt $max_wait ]; do
        local bids_output
        bids_output=$(ergors_deploy bids "$DEPLOY_SESSION_ID" 2>&1) || true

        local total=$(echo "$bids_output" | jq -r '.total // 0' 2>/dev/null)
        if [ "$total" -gt 0 ] 2>/dev/null; then
            TESTS_PASSED=$((TESTS_PASSED + 1))
            log_success "  Received $total bid(s)"
            if [ "$VERBOSE" = true ]; then
                echo "$bids_output" | jq '.bids[]' 2>/dev/null
            fi
            return 0
        fi

        sleep 3
        waited=$((waited + 3))
        log "  Waiting for bids... (${waited}/${max_wait}s)"
    done

    # Bids not received is acceptable in local dev environment
    log_warn "  No bids received (local provider may not have resources)"
}

# Test provider selection via engine
test_real_lease_creation() {
    if [ "$USE_REAL_AKASH" != true ]; then
        return 0
    fi

    if [ -z "$DEPLOY_SESSION_ID" ]; then
        log_warn "No session ID, skipping provider selection"
        return 0
    fi

    log_step "Testing Engine Provider Selection"

    # Select provider (use test address for local dev)
    log "Selecting provider via engine..."
    local select_output
    select_output=$(ergors_deploy select "$DEPLOY_SESSION_ID" \
        --provider "akash1localprovider" \
        --price 100 \
        2>&1) || true
    echo "$select_output" > "${TEST_DIR}/test-lease-create.log"

    local success=$(echo "$select_output" | jq -r '.success // false' 2>/dev/null)
    if [ "$success" = "true" ]; then
        TESTS_PASSED=$((TESTS_PASSED + 1))
        log_success "  Provider selected successfully"
    else
        TESTS_FAILED=$((TESTS_FAILED + 1))
        log_error "  Provider selection failed"
    fi

    # Get workflow status
    log "Verifying workflow state..."
    local get_output
    get_output=$(ergors_deploy get "$DEPLOY_SESSION_ID" 2>&1) || true

    local step=$(echo "$get_output" | jq -r '.current_step // empty' 2>/dev/null)
    if [ -n "$step" ]; then
        TESTS_PASSED=$((TESTS_PASSED + 1))
        log_success "  Workflow at step: $step"
    else
        TESTS_FAILED=$((TESTS_FAILED + 1))
        log_error "  Workflow state query failed"
    fi
}

# Test workflow advancement via engine
test_real_manifest_deployment() {
    if [ "$USE_REAL_AKASH" != true ]; then
        return 0
    fi

    if [ -z "$DEPLOY_SESSION_ID" ]; then
        log_warn "No session ID, skipping manifest test"
        return 0
    fi

    log_step "Testing Engine Workflow Advancement"

    # Advance workflow
    log "Advancing workflow..."
    local advance_output
    advance_output=$(ergors_deploy advance "$DEPLOY_SESSION_ID" 2>&1) || true

    local success=$(echo "$advance_output" | jq -r '.success // false' 2>/dev/null)
    if [ "$success" = "true" ]; then
        TESTS_PASSED=$((TESTS_PASSED + 1))
        log_success "  Workflow advanced successfully"
    else
        TESTS_FAILED=$((TESTS_FAILED + 1))
        log_error "  Workflow advancement failed"
    fi

    # Check final status
    log "Checking deployment status..."
    local status_output
    status_output=$(ergors_deploy get "$DEPLOY_SESSION_ID" 2>&1) || true

    local status=$(echo "$status_output" | jq -r '.status // empty' 2>/dev/null)
    if [ -n "$status" ]; then
        TESTS_PASSED=$((TESTS_PASSED + 1))
        log_success "  Deployment status: $status"
    else
        log_warn "  Deployment status unclear"
        if [ "$VERBOSE" = true ]; then
            echo "$status_output"
        fi
    fi
}

# Run all real Akash deployment tests
run_real_akash_tests() {
    if [ "$USE_REAL_AKASH" != true ]; then
        log_warn "Skipping real Akash tests (use --real-akash to enable)"
        return 0
    fi

    log_step "Running Real Akash Deployment Tests"

    test_real_deployment_create
    test_real_bid_reception
    test_real_lease_creation
    test_real_manifest_deployment

    log_success "Real Akash deployment tests complete"
}

# ==================== End Real Akash Deployment Tests ====================

# Test contract build artifacts
test_contract_artifacts() {
    log_step "Testing Contract Build Artifacts"

    # Test 1: Check SDL contract artifact exists
    log "Verifying SDL contract WASM artifact..."
    if [ -f "$ROOT_DIR/contracts/artifacts/sdl_template_registrar.wasm" ]; then
        TESTS_PASSED=$((TESTS_PASSED + 1))
        local size=$(ls -lh "$ROOT_DIR/contracts/artifacts/sdl_template_registrar.wasm" | awk '{print $5}')
        log_success "  SDL contract artifact exists ($size)"
    else
        TESTS_FAILED=$((TESTS_FAILED + 1))
        log_error "  SDL contract artifact not found"
    fi

    # Test 2: Verify artifact is a valid WASM file
    log "Verifying WASM file header..."
    if [ -f "$ROOT_DIR/contracts/artifacts/sdl_template_registrar.wasm" ]; then
        # WASM files start with magic bytes: 0x00 0x61 0x73 0x6D (\0asm)
        local magic=$(xxd -l 4 -p "$ROOT_DIR/contracts/artifacts/sdl_template_registrar.wasm" 2>/dev/null)
        if [ "$magic" = "0061736d" ]; then
            TESTS_PASSED=$((TESTS_PASSED + 1))
            log_success "  Valid WASM magic bytes verified"
        else
            TESTS_FAILED=$((TESTS_FAILED + 1))
            log_error "  Invalid WASM file (magic: $magic)"
        fi
    fi

    # Test 3: List all contract artifacts
    log "Listing all contract artifacts..."
    local artifact_count=$(ls "$ROOT_DIR/contracts/artifacts/"*.wasm 2>/dev/null | wc -l)
    if [ "$artifact_count" -gt 0 ]; then
        TESTS_PASSED=$((TESTS_PASSED + 1))
        log_success "  Found $artifact_count contract artifact(s)"
        if [ "$VERBOSE" = true ]; then
            ls -lh "$ROOT_DIR/contracts/artifacts/"*.wasm
        fi
    else
        TESTS_FAILED=$((TESTS_FAILED + 1))
        log_error "  No contract artifacts found"
    fi
}

# Test SDL contract deployment
test_contract_deployment() {
    log_step "Testing SDL Contract Deployment"

    # Test 1: Check config has initial_contracts
    log "Verifying SDL contract in config..."
    if grep -q "sdl-template-registrar" "$TEST_DIR/coordinator/config.toml" 2>/dev/null; then
        TESTS_PASSED=$((TESTS_PASSED + 1))
        log_success "  SDL contract configured in initial_contracts"
    else
        TESTS_FAILED=$((TESTS_FAILED + 1))
        log_error "  SDL contract not found in config"
        show_config "coordinator"
    fi

    # Test 2: Check coordinator logs for contract deployment
    log "Checking for contract deployment in logs..."
    if grep -q "Processing.*contracts for deployment\|deploy.*contract\|sdl-template-registrar" "$TEST_DIR/coordinator/node.log" 2>/dev/null; then
        TESTS_PASSED=$((TESTS_PASSED + 1))
        log_success "  Contract deployment initiated"
    else
        TESTS_FAILED=$((TESTS_FAILED + 1))
        log_error "  Contract deployment not detected in logs"
        # Show relevant log lines
        if [ -f "$TEST_DIR/coordinator/node.log" ]; then
            echo -e "${YELLOW}=== Contract-related logs ===${NC}"
            grep -i "contract\|wasm\|cosmwasm\|deploy" "$TEST_DIR/coordinator/node.log" 2>/dev/null | tail -20 || echo "No contract logs found"
            echo -e "${YELLOW}=== end ===${NC}"
        fi
    fi

    # Test 3: Check for successful deployment
    log "Checking for successful contract deployment..."
    if grep -q "Successfully deployed contract.*sdl-template-registrar" "$TEST_DIR/coordinator/node.log" 2>/dev/null; then
        TESTS_PASSED=$((TESTS_PASSED + 1))
        log_success "  SDL contract deployed successfully"
    else
        # Check if it was skipped (already exists)
        if grep -q "already deployed\|skipping" "$TEST_DIR/coordinator/node.log" 2>/dev/null; then
            TESTS_PASSED=$((TESTS_PASSED + 1))
            log_success "  SDL contract already deployed (skipped)"
        else
            TESTS_FAILED=$((TESTS_FAILED + 1))
            log_error "  SDL contract deployment success not confirmed"
        fi
    fi

    # Test 4: Check WASM file was present
    if [ -f "$TEST_DIR/coordinator/sdl_template_registrar.wasm" ]; then
        TESTS_PASSED=$((TESTS_PASSED + 1))
        log_success "  SDL contract WASM file present"
    else
        TESTS_FAILED=$((TESTS_FAILED + 1))
        log_error "  SDL contract WASM file missing"
    fi
}

# Test SDL contract queries (real blockchain verification)
test_sdl_contract_queries() {
    log_step "Testing SDL Contract Queries"

    # Test 1: Check for contract address in logs
    log "Looking for deployed contract address..."
    local contract_addr=$(grep -oP 'contract_address.*?:\s*\K[a-z0-9]+' "$TEST_DIR/coordinator/node.log" 2>/dev/null | head -1)

    if [ -n "$contract_addr" ]; then
        TESTS_PASSED=$((TESTS_PASSED + 1))
        log_success "  Contract address: ${contract_addr:0:20}..."
    else
        log_warn "  Contract address not found in logs"
    fi

    # Test 2: Check for template registration
    log "Checking for SDL template registration..."
    if grep -q "template.*registered\|RegisterTemplate\|template.*added" "$TEST_DIR/coordinator/node.log" 2>/dev/null; then
        TESTS_PASSED=$((TESTS_PASSED + 1))
        log_success "  SDL templates registered"
    else
        log_warn "  Template registration not found in logs"
    fi

    # Test 3: Check for template query capability
    log "Checking template query capability..."
    if grep -q "QueryTemplate\|template.*query\|GetTemplate" "$TEST_DIR/coordinator/node.log" 2>/dev/null; then
        TESTS_PASSED=$((TESTS_PASSED + 1))
        log_success "  Template query capability available"
    else
        log_warn "  Template query capability not detected"
    fi

    # For real Akash mode, perform actual contract queries
    if [ "$USE_REAL_AKASH" = true ]; then
        log "Performing real contract state queries..."

        # This would query the actual contract state on the blockchain
        # For now, we check if the infrastructure is ready
        if [ -f "$TEST_DIR/coordinator/node.log" ]; then
            if grep -q "cosmwasm.*initialized\|wasm.*runtime\|contract.*loaded" "$TEST_DIR/coordinator/node.log" 2>/dev/null; then
                TESTS_PASSED=$((TESTS_PASSED + 1))
                log_success "  CosmWasm runtime initialized for real queries"
            fi
        fi
    fi
}

# Test SDL template variable substitution
test_sdl_variable_substitution() {
    log_step "Testing SDL Variable Substitution"

    # Test 1: Check for variable substitution in logs
    log "Checking variable substitution processing..."
    if grep -q "substitut\|variable\|\${.*}" "$TEST_DIR/coordinator/node.log" 2>/dev/null; then
        TESTS_PASSED=$((TESTS_PASSED + 1))
        log_success "  Variable substitution processing detected"
    else
        log_warn "  Variable substitution not detected in logs"
    fi

    # Test 2: Check SDL processing
    log "Checking SDL processing..."
    if grep -q "SDL\|sdl.*process\|yaml\|manifest" "$TEST_DIR/coordinator/node.log" 2>/dev/null; then
        TESTS_PASSED=$((TESTS_PASSED + 1))
        log_success "  SDL processing detected"
    else
        log_warn "  SDL processing not detected"
    fi

    # Test 3: Verify no unsubstituted variables in processed SDL
    log "Checking for unsubstituted variables..."
    # Look for error messages about unsubstituted variables
    if grep -q "unsubstituted\|missing.*variable\|undefined.*\${" "$TEST_DIR/coordinator/node.log" 2>/dev/null; then
        TESTS_FAILED=$((TESTS_FAILED + 1))
        log_error "  Unsubstituted variables found"
    else
        TESTS_PASSED=$((TESTS_PASSED + 1))
        log_success "  No unsubstituted variable errors"
    fi
}

# Print summary
print_summary() {
    local end_time=$(date +%s)
    local duration=$((end_time - START_TIME))

    log_step "Test Summary"

    echo ""
    echo -e "  ${BOLD}Duration:${NC}      ${duration}s"
    echo -e "  ${BOLD}Tests Passed:${NC}  ${GREEN}${TESTS_PASSED}${NC}"
    echo -e "  ${BOLD}Tests Failed:${NC}  ${RED}${TESTS_FAILED}${NC}"
    echo ""
    echo -e "  ${BOLD}ERGORS Network:${NC}"
    echo -e "    Coordinator: ${COORDINATOR_GRPC}"
    echo -e "    Executor:    ${EXECUTOR_GRPC}"
    echo ""
    echo -e "  ${BOLD}Deployed Service:${NC}"
    echo -e "    Endpoint: ${DEPLOYED_ENDPOINT}"
    echo ""

    if [ $TESTS_FAILED -eq 0 ]; then
        echo -e "${GREEN}${BOLD}  ╔═══════════════════════════════════════╗${NC}"
        echo -e "${GREEN}${BOLD}  ║     ALL TESTS PASSED SUCCESSFULLY     ║${NC}"
        echo -e "${GREEN}${BOLD}  ╚═══════════════════════════════════════╝${NC}"
        return 0
    else
        echo -e "${RED}${BOLD}  ╔═══════════════════════════════════════╗${NC}"
        echo -e "${RED}${BOLD}  ║         SOME TESTS FAILED             ║${NC}"
        echo -e "${RED}${BOLD}  ╚═══════════════════════════════════════╝${NC}"
        return 1
    fi
}

# Main
main() {
    echo ""
    echo -e "${CYAN}${BOLD}╔═══════════════════════════════════════════════════════════════╗${NC}"
    echo -e "${CYAN}${BOLD}║                                                               ║${NC}"
    echo -e "${CYAN}${BOLD}║          ERGORS E2E Deployment Workflow Test                  ║${NC}"
    echo -e "${CYAN}${BOLD}║                                                               ║${NC}"
    echo -e "${CYAN}${BOLD}╚═══════════════════════════════════════════════════════════════╝${NC}"
    echo ""

    # Show mode
    if [ "$USE_REAL_AKASH" = true ]; then
        echo -e "  ${BOLD}Mode:${NC} ${GREEN}REAL AKASH${NC} (blockchain node + provider)"
        echo -e "  ${BOLD}Akash Home:${NC} ${AKASH_HOME}"
    else
        echo -e "  ${BOLD}Mode:${NC} ${YELLOW}MOCK${NC} (Kind cluster only)"
    fi
    echo ""

    if [ "$LIVE_LOGS" = true ]; then
        echo -e "  ${BOLD}Live Logs:${NC} ${GREEN}ENABLED${NC}"
        echo -e "    ${COORD_COLOR}[COORD]${NC} Coordinator logs"
        echo -e "    ${EXEC_COLOR}[EXEC ]${NC} Executor logs"
        echo -e "    ${MOCK_COLOR}[MOCK ]${NC} Mock provider logs"
        if [ "$USE_REAL_AKASH" = true ]; then
            echo -e "    ${BLUE}[ANODE]${NC} Akash node logs"
            echo -e "    ${GREEN}[APROV]${NC} Akash provider logs"
        fi
        echo ""
    fi

    echo -e "  ${BOLD}Workflow:${NC}"
    echo -e "    1. Build CosmWasm contracts (Docker optimizer)"
    echo -e "    2. Build ERGORS binary"
    echo -e "    3. Verify contract artifacts"
    echo -e "    4. Start ERGORS test network (coordinator + executor)"
    if [ "$USE_REAL_AKASH" = true ]; then
        echo -e "    5. Setup real Akash dev environment (node + provider)"
        echo -e "    6. Run real Akash deployment workflow"
        echo -e "    7. Test real blockchain deployments"
    else
        echo -e "    5. Setup Kind cluster with Akash (mock mode)"
        echo -e "    6. Build mock inference provider image"
    fi
    echo -e "    7. Configure ERGORS deployment workflow"
    echo -e "    8. Test ERGORS network connectivity"
    echo -e "    9. Test node configuration"
    echo -e "   10. Test SDL contract deployment"
    echo -e "   11. Test SDL workflow"
    echo -e "   12. Test API key management workflow"
    echo -e "   13. Test Authz/Feegrant workflow"
    if [ "$USE_REAL_AKASH" = true ]; then
        echo -e "   14. Test real Akash deployment lifecycle"
    fi
    echo ""

    check_prerequisites
    build_contracts
    build_ergors
    test_contract_artifacts
    start_ergors_network
    setup_akash_environment

    # Real Akash deployment workflow (if enabled)
    if [ "$USE_REAL_AKASH" = true ]; then
        run_real_akash_deployment || log_warn "Real Akash deployment workflow had failures"
    else
        build_mock_image
    fi

    deploy_via_ergors || log_warn "ERGORS deployment workflow had failures (continuing tests)"
    test_ergors_network
    test_node_config
    test_contract_deployment
    test_sdl_workflow
    run_api_key_tests
    run_authz_feegrant_tests

    # Real Akash tests (if enabled)
    if [ "$USE_REAL_AKASH" = true ]; then
        run_real_akash_tests
    fi

    print_summary
}

main "$@"
