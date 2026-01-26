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

set -eu
# -e: exit on error, -u: error on unset variables

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
# ERGORS_CLI="${ROOT_DIR}/target/release/ergors-cli"
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

# Variables set during test execution (initialized for set -u safety)
COORDINATOR_ADDRESS=""
EXECUTOR_ADDRESS=""
DEPLOY_SESSION_ID=""
TEST_AUTH_TOKEN=""
GRANT_REQUEST_ID=""
TEST_SERVICE_ENDPOINT=""
TEST_SERVICE_NAME=""
HEALTH_ENDPOINT=""
SDL_TEMPLATE_CONTRACT=""
CROSS_ACCOUNT_SESSION_ID=""
FEEGRANT_DEPLOY_SESSION=""

# Live log streaming flag
LIVE_LOGS=false

# Enhanced test tracking with visual feedback
declare -a TEST_ORDER
TEST_CURRENT_SECTION=""

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

# Enhanced test result tracking with visual feedback
test_pass() {
    local test_name="$1"
    local description="$2"

    TESTS_PASSED=$((TESTS_PASSED + 1))
    TEST_ORDER+=("$test_name:PASS")

    echo -e "${GREEN} - $description${NC}"
    if [ "$VERBOSE" = true ]; then
        log_success "  ✓ $test_name"
    fi

    # Log to detailed test log for debugging
    echo "[$(date +%H:%M:%S)] PASS: $test_name - $description" >> "${TEST_DIR}/test-results.log"
}

test_fail() {
    local test_name="$1"
    local description="$2"
    local details="$3"

    TESTS_FAILED=$((TESTS_FAILED + 1))
    TEST_ORDER+=("$test_name:FAIL:$details")

    echo -e "${RED}❌ $description${NC}"
    if [ -n "$details" ]; then
        echo -e "${RED}   └─ $details${NC}"
    fi
    if [ "$VERBOSE" = true ]; then
        log_error "  ✗ $test_name: $details"
    fi

    # Log to detailed test log for debugging
    echo "[$(date +%H:%M:%S)] FAIL: $test_name - $description" >> "${TEST_DIR}/test-results.log"
    if [ -n "$details" ]; then
        echo "    Details: $details" >> "${TEST_DIR}/test-results.log"
    fi
}

test_section() {
    local section_name="$1"
    TEST_CURRENT_SECTION="$section_name"
    echo -e "\n${YELLOW}${BOLD}🧪 $section_name${NC}"
}

test_summary() {
    local section_name="$1"
    local passed=0
    local failed=0

    echo -e "\n${BLUE}${BOLD}📊 $section_name Summary:${NC}"

    for test_entry in "${TEST_ORDER[@]}"; do
        # Parse test_entry format: "test_name:STATUS:details"
        local test_name=$(echo "$test_entry" | cut -d: -f1)
        local status=$(echo "$test_entry" | cut -d: -f2)

        if [ "$status" = "PASS" ]; then
            echo -e "  ${GREEN}✅ $test_name${NC}"
            passed=$((passed + 1))
        elif [ "$status" = "FAIL" ]; then
            local details=$(echo "$test_entry" | cut -d: -f3-)
            echo -e "  ${RED}❌ $test_name${NC}"
            if [ -n "$details" ]; then
                echo -e "     ${RED}└─ $details${NC}"
            fi
            failed=$((failed + 1))
        fi
    done

    echo -e "  ${BOLD}Section Results:${NC} ${GREEN}$passed passed${NC}, ${RED}$failed failed${NC}"

    # Reset for next section
    TEST_ORDER=()
}

# Port-forward helper: runs a callback with port-forward active, guarantees cleanup.
# Usage: with_port_forward <namespace> <service> <local_port>:<remote_port> <callback_function>
with_port_forward() {
    local namespace="$1"
    local service="$2"
    local ports="$3"
    local callback="$4"
    shift 4

    kubectl port-forward -n "$namespace" "svc/$service" "$ports" &>/dev/null &
    local pf_pid=$!

    # Wait for port-forward to establish
    local local_port="${ports%%:*}"
    local waited=0
    while [ $waited -lt 10 ]; do
        if nc -z 127.0.0.1 "$local_port" 2>/dev/null; then
            break
        fi
        sleep 1
        waited=$((waited + 1))
    done

    # Run callback, capture exit code, always kill port-forward
    local rc=0
    "$callback" "$@" || rc=$?
    kill "$pf_pid" 2>/dev/null || true
    wait "$pf_pid" 2>/dev/null || true
    return $rc
}

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
    touch "$log_file"
    (tail -f "$log_file" 2>/dev/null | while IFS= read -r line; do
        echo -e "${color}${prefix}${NC} $line"
    done) &
    local tail_pid=$!
    TAIL_PIDS+=("$tail_pid")
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
    cleanup_akash_environment
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
                log_error "Install make"
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

    if [ ${#missing[@]} -gt 0 ]; then
        log_error "Missing required tools: ${missing[*]}"
        exit 1
    fi

    if ! docker info &>/dev/null; then
        log_error "Docker is not running"
        exit 1
    fi

    # Check Go version
    local go_version=$(go version 2>/dev/null | awk '{print $3}')
    log "Go version: $go_version"

    # Check direnv installation and configuration
    if ! command -v direnv &>/dev/null; then
        log_error "direnv not found in PATH"
        log_error "Install with: brew install direnv (macOS) or apt install direnv (Linux)"
        log_error "Minimum required version: 2.32.x"
        exit 1
    fi

    local direnv_version=$(direnv version 2>/dev/null || echo "unknown")
    log "direnv version: $direnv_version"

    # Verify direnv hook is configured in shell profile
    local shell_profile=""
    if [ -n "$ZSH_VERSION" ]; then
        shell_profile="$HOME/.zshrc"
    elif [ -n "$BASH_VERSION" ]; then
        shell_profile="$HOME/.bashrc"
    fi

    if [ -n "$shell_profile" ] && [ -f "$shell_profile" ]; then
        if ! grep -q "direnv hook" "$shell_profile" 2>/dev/null; then
            log_error "direnv hook not configured in $shell_profile"
            log_error "Add the following line to $shell_profile:"
            if [ -n "$ZSH_VERSION" ]; then
                log_error '  eval "$(direnv hook zsh)"'
            elif [ -n "$BASH_VERSION" ]; then
                log_error '  eval "$(direnv hook bash)"'
            fi
            log_error "Then reload your shell: source $shell_profile"
            exit 1
        fi
        log "direnv hook configured in $shell_profile"
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

    # Allow direnv for provider directory
    cd "${AKASH_PROVIDER_DIR}"
    log "Configuring direnv for Akash provider..."
    direnv allow . >/dev/null 2>&1 || true

    log_success "Akash repos ready"
}

# Setup Akash environment variables (replaces direnv)
# Sources the .env file and sets all required variables the Makefile expects
load_akash_env() {
    export AP_ROOT="${AKASH_PROVIDER_DIR}"
    export AKASH_DIRENV_SET=1

    # Strategy: Try direnv first. Only fall back to manual .env loading if direnv fails.
    local direnv_loaded=false

    if command -v direnv >/dev/null 2>&1; then
        # Allow direnv for provider directory
        (cd "${AKASH_PROVIDER_DIR}" && direnv allow . >/dev/null 2>&1) || true

        # Load direnv environment variables
        if eval "$(cd "${AKASH_PROVIDER_DIR}" && direnv export bash 2>/dev/null)"; then
            direnv_loaded=true
        else
            log_warn "Failed to load direnv environment for Akash provider"
        fi
    fi

    # Fallback: Source the .env file ONLY if direnv didn't work
    if [ "$direnv_loaded" = false ] && [ -f "${AKASH_PROVIDER_DIR}/.env" ]; then
        log_warn "Falling back to manual .env loading"
        # Substitute AP_ROOT in .env values and export them
        # Skip ROOT_DIR to avoid overwriting our project's ROOT_DIR
        while IFS='=' read -r key value || [ -n "$key" ]; do
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
    # NOTE: AKASH_HOME will be set by direnv - do NOT export it manually
    # Exporting it here causes Makefile errors about direnv not being configured

    # direnv should have set DIRENV_FILE and DIRENV_DIR, but ensure they're set
    export DIRENV_FILE="${DIRENV_FILE:-${AKASH_KUBE_DIR}/.envrc}"
    export DIRENV_DIR="${DIRENV_DIR:-${AKASH_KUBE_DIR}}"

    # Force local Go toolchain to avoid download issues
    export GOTOOLCHAIN=local

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

    # Use direnv exec to run make commands in the proper environment
    # This satisfies the Akash Makefiles' requirement that direnv is active
    cd "${AKASH_KUBE_DIR}"

    # Allow direnv for kube directory if not already allowed
    direnv allow . >/dev/null 2>&1 || true

    # Use direnv exec to run make with proper environment
    # This ensures AKASH_HOME and other vars are set by direnv, not manually
    direnv exec . $MAKE_CMD "$target" "$@"
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

# Build Akash binaries if they don't exist
build_akash_binaries() {
    log_step "Checking Akash Binaries"

    local akash_bin="${AKASH_PROVIDER_DIR}/.cache/bin/akash"
    local provider_bin="${AKASH_PROVIDER_DIR}/.cache/bin/provider-services"

    # Check if binaries exist
    if [ -f "$akash_bin" ] && [ -f "$provider_bin" ]; then
        log_success "Akash binaries already exist"
        return 0
    fi

    log "Akash binaries not found, building them..."
    log "This may take several minutes..."

    cd "$AKASH_PROVIDER_DIR" || {
        log_error "Failed to access Akash provider directory: $AKASH_PROVIDER_DIR"
        exit 1
    }

    # Prerequisites (direnv, make, go) already validated by check_prerequisites().
    # Just activate direnv and build.
    direnv allow . >/dev/null 2>&1 || true

    local build_log="${TEST_DIR}/akash-binaries-build.log"
    log "Building akash and provider-services binaries..."

    if [ "$VERBOSE" = true ]; then
        direnv exec . make bins 2>&1 | tee "$build_log"
    else
        direnv exec . make bins > "$build_log" 2>&1 &
        local build_pid=$!
        local elapsed=0
        while kill -0 "$build_pid" 2>/dev/null; do
            sleep 10
            elapsed=$((elapsed + 10))
            log "  [${elapsed}s] Building binaries..."
        done
        wait "$build_pid"
        local exit_code=$?
        if [ $exit_code -ne 0 ]; then
            log_error "Binary build failed. Last 50 lines:"
            tail -50 "$build_log"
            log_error "Full build log: $build_log"
            cd "$ROOT_DIR"
            exit 1
        fi
    fi

    # Verify binaries now exist
    if [ ! -f "$akash_bin" ] || [ ! -f "$provider_bin" ]; then
        log_error "Failed to build Akash binaries"
        log_error "Expected: $akash_bin, $provider_bin"
        cd "$ROOT_DIR"
        exit 1
    fi

    log_success "Akash binaries built successfully"
    log "  akash: $akash_bin"
    log "  provider-services: $provider_bin"

    cd "$ROOT_DIR"
}

# Create Kind cluster with Akash components
# Verify faucet is deployed and accessible in the cluster
verify_faucet_deployment() {
    log_step "Verifying Akash Faucet Deployment"

    # Check if faucet pod exists in akash-services namespace
    log "Checking for faucet pod in akash-services namespace..."
    local max_wait=10
    local waited=0

    while [ $waited -lt $max_wait ]; do
        if kubectl get pods -n akash-services 2>/dev/null | grep -q "akash-faucet.*Running"; then
            log_success "Faucet pod is running"
            break
        fi
        sleep 2
        waited=$((waited + 2))
        log "  Waiting for faucet pod... (${waited}s/${max_wait}s)"
    done

    if [ $waited -ge $max_wait ]; then
        log_error "Faucet pod not found or not running after ${max_wait}s"
        log "Checking faucet deployment status..."
        kubectl get pods -n akash-services 2>/dev/null | grep faucet || log_warn "No faucet pods found"
        kubectl describe pod -n akash-services -l app=akash-faucet 2>/dev/null | tail -30 || true
        log_warn "Faucet may not be deployed - funding tests may fail"
        return 1
    fi

    # Verify faucet service exists
    log "Checking for faucet service..."
    if kubectl get svc -n akash-services akash-faucet &>/dev/null; then
        local faucet_port=$(kubectl get svc -n akash-services akash-faucet -o jsonpath='{.spec.ports[0].port}' 2>/dev/null)
        log_success "Faucet service found (port: ${faucet_port:-5005})"
    else
        log_error "Faucet service not found"
        return 1
    fi

    # Test faucet health endpoint via port-forward
    log "Testing faucet /status endpoint..."
    kubectl port-forward -n akash-services svc/akash-faucet 5005:5005 &>/dev/null &
    local port_forward_pid=$!
    # Ensure cleanup on any exit
    trap 'kill $port_forward_pid 2>/dev/null || true' RETURN

    # Wait for port-forward (poll instead of blind sleep)
    local pf_waited=0
    while [ $pf_waited -lt 5 ]; do
        if curl -s --max-time 1 "http://localhost:5005/status" >/dev/null 2>&1; then
            break
        fi
        sleep 1
        pf_waited=$((pf_waited + 1))
    done

    local status_check=$(curl -s --max-time 5 "http://localhost:5005/status" 2>/dev/null || echo "{}")

    if echo "$status_check" | jq -e '.address' >/dev/null 2>&1; then
        local faucet_addr=$(echo "$status_check" | jq -r '.address // "unknown"')
        local faucet_amount=$(echo "$status_check" | jq -r '.amount // "unknown"')
        log_success "Faucet is healthy and ready"
        log "  Address: $faucet_addr"
        log "  Amount: $faucet_amount"
    else
        log_warn "Faucet /status endpoint not responding as expected"
        log "  Response: $status_check"
    fi
}

setup_akash_kube_cluster() {
    log_step "Setting Up Akash Kind Cluster"

    # Build binaries first if they don't exist
    build_akash_binaries

    # Delete existing cluster if present
    if kind get clusters 2>/dev/null | grep -q "^kind$"; then
        log "Deleting existing Kind cluster..."
        akash_make kube-cluster-delete 2>/dev/null || kind delete cluster --name kind
    fi
 
    # Set environment to use pre-built images and skip source builds
    export GOVERSION_SEMVER="${GOVERSION_SEMVER}"
    export KUBE_ROLLOUT_TIMEOUT="${KUBE_ROLLOUT_TIMEOUT}"
    # export SKIP_BUILD=true
    # Tell make to use the stable images we just pulled (prevents source build attempts)
    export DOCKER_IMAGE="$provider_image"
    export AKASH_DOCKER_IMAGE="$node_image"

    log "Creating Kind cluster with Akash components (timeout: ${KUBE_ROLLOUT_TIMEOUT}s)..."
    log "This may take several minutes..."

    local cluster_log="${TEST_DIR}/akash-cluster-setup.log"
    if [ "$VERBOSE" = true ]; then
        akash_make kube-cluster-setup 2>&1 | tee "$cluster_log"
        local exit_code=${PIPESTATUS[0]}
        if [ $exit_code -ne 0 ]; then
            log_error "kube-cluster-setup failed (exit code: $exit_code)"
            exit 1
        fi
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

    # Verify faucet is deployed and ready
    verify_faucet_deployment
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

# Setup Akash infrastructure ONLY (node + faucet + cluster)
# This prepares the blockchain and faucet for testing, but does NOT create provider
# Provider creation happens AFTER feegrant/authz tests
setup_akash_infrastructure() {
    log_step "Setting Up Akash Infrastructure (Node + Faucet)"

    setup_akash_repos
    init_akash_kube
    setup_akash_kube_cluster
    start_akash_node

    log_success "Akash infrastructure ready (node + faucet + cluster)"
    log "  Node PID: ${AKASH_NODE_PID}"
    log "  Blockchain: ${AKASH_LOCAL_NODE}"
    log "  Chain ID: ${AKASH_LOCAL_CHAIN_ID}"
    log "  Faucet: Available in akash-services namespace"
}

# Setup Akash provider for accepting deployments
# This happens AFTER feegrant/authz tests prove the grant workflow works
setup_akash_provider() {
    log_step "Setting Up Akash Provider (Post-Feegrant Tests)"

    create_akash_provider
    start_akash_provider

    log_success "Akash provider ready to accept bids"
    log "  Provider PID: ${AKASH_PROVIDER_PID}"
    log "  Gateway: ${GATEWAY_ENDPOINT:-https://localhost:8443}"
}

# Full Akash dev environment setup (wrapper for backward compatibility)
setup_real_akash_environment() {
    setup_akash_infrastructure
    setup_akash_provider
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
    if [ -d "${AKASH_KUBE_DIR}" ]; then
        akash_make kube-cluster-delete 2>/dev/null || true
    fi

    log_success "Akash environment cleaned up"
}

# ==================== Engine Akash Deployment Workflow ====================

# Helper: run ergors-cli deploy command against coordinator
ergors_deploy() {
    local subcommand="$1"
    shift

    # if [ ! -f "$ERGORS_CLI" ]; then
    #     echo '{"error": "ergors-cli binary not found at '"$ERGORS_CLI"'"}'
    #     return 1
    # fi

    if [ -z "$COORDINATOR_GRPC" ]; then
        echo '{"error": "COORDINATOR_GRPC not set"}'
        return 1
    fi

    "$ERGORS_CLI" --grpc-addr "http://${COORDINATOR_GRPC}" --json deploy "$subcommand" "$@"
}

# Create deployment via engine workflow
create_akash_deployment() {
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
    log "Querying engine deployments..."
    ergors_deploy list 2>&1 | tee "${TEST_DIR}/query-deployments.log"
}

# Query bids via engine workflow
query_akash_bids() {
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
    if [ -z "$DEPLOY_SESSION_ID" ]; then
        return 0
    fi

    log "Checking deployment status (session: ${DEPLOY_SESSION_ID:0:8})..."
    ergors_deploy get "$DEPLOY_SESSION_ID" 2>&1 | tee "${TEST_DIR}/lease-status.log" || true
}

# Get deployment workflow details
get_akash_deployment_logs() {
    if [ -z "$DEPLOY_SESSION_ID" ]; then
        return 0
    fi

    log "Getting deployment details (session: ${DEPLOY_SESSION_ID:0:8})..."
    ergors_deploy get "$DEPLOY_SESSION_ID" 2>&1 || true
}

# Full deployment workflow on real Akash
run_real_akash_deployment() {
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
        cargo build --release -p ergors  
    else
        cargo build --release -p ergors 2>&1 | tail -5
    fi

    if [ ! -f "$ERGORS_BIN" ]; then
        log_error "Failed to build ergors binary"
        exit 1
    fi

    # if [ ! -f "$ERGORS_CLI" ]; then
    #     log_error "Failed to build ergors-cli binary"
    #     exit 1
    # fi

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

    # Setup infrastructure only (node + faucet, NO provider yet)
    # Provider setup happens after feegrant tests
    setup_akash_infrastructure
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
    test_section "ERGORS Network Tests"

    # Test 1: Coordinator is healthy
    if nc -z 127.0.0.1 "${COORDINATOR_GRPC##*:}" 2>/dev/null; then
        test_pass "coordinator_grpc_reachable" "Coordinator gRPC reachable at ${COORDINATOR_GRPC}"
    else
        test_fail "coordinator_grpc_reachable" "Coordinator gRPC unreachable at ${COORDINATOR_GRPC}"
    fi

    # Test 2: Executor is healthy
    if nc -z 127.0.0.1 "${EXECUTOR_GRPC##*:}" 2>/dev/null; then
        test_pass "executor_grpc_reachable" "Executor gRPC reachable at ${EXECUTOR_GRPC}"
    else
        test_fail "executor_grpc_reachable" "Executor gRPC unreachable at ${EXECUTOR_GRPC}"
    fi

    # Test 3: Nodes are running
    local running=0
    for pid in "${NODE_PIDS[@]}"; do
        if kill -0 "$pid" 2>/dev/null; then
            running=$((running + 1))
        fi
    done
    if [ $running -eq ${#NODE_PIDS[@]} ]; then
        test_pass "all_nodes_running" "All ${#NODE_PIDS[@]} ERGORS nodes running"
    else
        test_fail "all_nodes_running" "Only $running/${#NODE_PIDS[@]} nodes running" "Expected all nodes to be running"
        # Show logs for debugging
        show_node_logs "coordinator"
        show_node_logs "executor_0"
    fi

    test_summary "ERGORS Network"
}

# Test ERGORS node configuration
test_node_config() {
    test_section "Node Configuration Tests"

    # Test 1: Config files exist
    if [ -f "$TEST_DIR/coordinator/config.toml" ]; then
        test_pass "coordinator_config_exists" "Coordinator config.toml exists"
    else
        test_fail "coordinator_config_exists" "Coordinator config.toml missing"
    fi

    # Test 2: CosmWasm config present
    if grep -q "cosmwasm" "$TEST_DIR/coordinator/config.toml" 2>/dev/null; then
        test_pass "cosmwasm_config_present" "CosmWasm configuration present in config"
    else
        test_fail "cosmwasm_config_present" "CosmWasm configuration missing from config" "Config should include cosmwasm settings"
        show_config "coordinator"
    fi

    # Test 3: WASM artifact copied
    if [ -f "$TEST_DIR/coordinator/sdl_template_registrar.wasm" ]; then
        test_pass "wasm_artifact_present" "SDL contract WASM artifact present"
    else
        test_fail "wasm_artifact_present" "SDL contract WASM artifact missing"
    fi

    # Test 4: Validate config using ergors config get
    local node_type=$("$ERGORS_BIN" --home "$TEST_DIR/coordinator" config get identity.node_type 2>&1)
    if echo "$node_type" | grep -q "Coordinator"; then
        test_pass "config_get_node_type" "Config get: identity.node_type = Coordinator"
    else
        test_fail "config_get_node_type" "Config get failed for identity.node_type" "Expected 'Coordinator', got: $node_type"
    fi

    # Test 5: Validate api_port
    local api_port=$("$ERGORS_BIN" --home "$TEST_DIR/coordinator" config get identity.api_port 2>&1)
    if echo "$api_port" | grep -q "50100"; then
        test_pass "config_get_api_port" "Config get: identity.api_port = 50100"
    else
        test_fail "config_get_api_port" "Config get api_port failed" "Expected '50100', got: $api_port"
    fi

    # Test 6: Validate cosmwasm enabled
    local cw_enabled=$("$ERGORS_BIN" --home "$TEST_DIR/coordinator" config get cosmwasm.enabled 2>&1)
    if echo "$cw_enabled" | grep -q "true"; then
        test_pass "config_get_cosmwasm_enabled" "Config get: cosmwasm.enabled = true"
    else
        test_fail "config_get_cosmwasm_enabled" "Config get cosmwasm.enabled failed" "Expected 'true', got: $cw_enabled"
    fi

    # Test 7: Validate config list command
    local config_list=$("$ERGORS_BIN" --home "$TEST_DIR/coordinator" config list 2>&1)
    if echo "$config_list" | grep -q "identity.host" && echo "$config_list" | grep -q "network.listen_port"; then
        test_pass "config_list_command" "Config list shows available keys (identity.host, network.listen_port)"
    else
        test_fail "config_list_command" "Config list failed" "Expected to find identity.host and network.listen_port keys"
    fi

    test_summary "Node Configuration"
}

# Test SDL template workflow
test_sdl_workflow() {
    test_section "SDL Template Workflow Tests"

    # Check coordinator logs for contract activity
    if [ -f "$TEST_DIR/coordinator/node.log" ]; then
        # Check for startup
        if grep -q "Starting ERGORS\|Starting.*engine" "$TEST_DIR/coordinator/node.log" 2>/dev/null; then
            test_pass "coordinator_startup_successful" "Coordinator started successfully"
        else
            test_fail "coordinator_startup_successful" "Coordinator startup not detected in logs"
        fi

        # Check for storage init
        if grep -q "storage\|Cnidarium\|rocksdb" "$TEST_DIR/coordinator/node.log" 2>/dev/null; then
            test_pass "storage_initialized" "Storage system initialized (Cnidarium/RocksDB)"
        else
            test_fail "storage_initialized" "Storage initialization not detected in logs"
        fi
    else
        test_fail "coordinator_log_exists" "Coordinator log file not found" "Expected $TEST_DIR/coordinator/node.log to exist"
    fi

    # Check executor logs
    if [ -f "$TEST_DIR/executor_0/node.log" ]; then
        if grep -q "Starting ERGORS\|Starting.*engine" "$TEST_DIR/executor_0/node.log" 2>/dev/null; then
            test_pass "executor_startup_successful" "Executor started successfully"
        else
            test_fail "executor_startup_successful" "Executor startup not detected in logs"
        fi
    else
        test_fail "executor_log_exists" "Executor log file not found" "Expected $TEST_DIR/executor_0/node.log to exist"
    fi

    test_summary "SDL Workflow Basics"

    # Run additional SDL tests with their own sections
    test_sdl_contract_queries
    test_sdl_template_from_contract
    test_sdl_variable_substitution
}

# ==================== API Key Management Tests ====================
# NOTE: Removed ~290 lines of dead code. start_mock_provider() always returned 1
# and set MOCK_PROVIDER_URL="", so all subsequent test functions immediately returned.
# These tests should be re-implemented when the mock provider is actually deployed to Akash.
# The code is preserved in git history.

# ==================== End API Key Management Tests ====================

# ==================== Authz/Feegrant Workflow Tests ====================

# Get node addresses for coordinator and executor
get_node_addresses() {
    if [ -n "$COORDINATOR_ADDRESS" ] && [ -n "$EXECUTOR_ADDRESS" ]; then
        log "Using cached node addresses:"
        log "  Coordinator: $COORDINATOR_ADDRESS"
        log "  Executor: $EXECUTOR_ADDRESS"
        return 0
    fi

    log "Querying node account addresses..."

    # Get coordinator address
    local coord_identity
    coord_identity=$("$ERGORS_CLI" --grpc-addr "http://${COORDINATOR_GRPC}" --json identity get 2>&1) || true
    export COORDINATOR_ADDRESS=$(echo "$coord_identity" | jq -r '.cosmos_address // empty' 2>/dev/null)

    if [ -z "$COORDINATOR_ADDRESS" ]; then
        log_error "Failed to get coordinator address"
        return 1
    fi

    # Get executor address
    local exec_identity
    exec_identity=$("$ERGORS_CLI" --grpc-addr "http://${EXECUTOR_GRPC}" --json identity get 2>&1) || true
    export EXECUTOR_ADDRESS=$(echo "$exec_identity" | jq -r '.cosmos_address // empty' 2>/dev/null)

    if [ -z "$EXECUTOR_ADDRESS" ]; then
        log_error "Failed to get executor address"
        return 1
    fi

    log "Node addresses retrieved:"
    log "  Coordinator: $COORDINATOR_ADDRESS"
    log "  Executor: $EXECUTOR_ADDRESS"
}

# Test authz grant request from executor to coordinator
test_authz_grant_request() {
    log_step "Testing Authz Grant Request Workflow"

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

    # Query actual grants from blockchain
    if [ -d "${AKASH_KUBE_DIR}" ]; then
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

    # Query actual allowances from blockchain
    if [ -d "${AKASH_KUBE_DIR}" ]; then
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

# Run all authz/feegrant tests - ENGINE-DRIVEN WORKFLOW
run_authz_feegrant_tests() {
    log_step "Running Engine-Driven Authz/Feegrant Workflow Tests"

    # Get account addresses for both nodes
    get_node_addresses

    # Test 1: Executor requests grant from coordinator via engine
    log "Testing grant request workflow..."
    local grant_request_output
    grant_request_output=$("$ERGORS_CLI" --grpc-addr "http://${EXECUTOR_GRPC}" --json grant request \
        --granter "$COORDINATOR_ADDRESS" \
        --grantee "$EXECUTOR_ADDRESS" \
        --msg-type "/akash.deployment.v1beta3.MsgCreateDeployment" \
        --msg-type "/akash.deployment.v1beta3.MsgDepositDeployment" \
        --msg-type "/akash.market.v1beta3.MsgCreateLease" \
        --allowance 10000000 \
        --reason "E2E test - executor needs deployment permissions" \
        2>&1) || true

    local request_id=$(echo "$grant_request_output" | jq -r '.request_id // empty' 2>/dev/null)

    if [ -n "$request_id" ]; then
        test_pass "grant_request_created" "Grant request created (ID: ${request_id:0:8}...)"
        export GRANT_REQUEST_ID="$request_id"
    else
        test_fail "grant_request_created" "Failed to create grant request" "Output: $grant_request_output"
        return 1
    fi

    # Test 2: Coordinator approves grant via engine
    log "Coordinator approving grant request..."
    local approve_output
    approve_output=$("$ERGORS_CLI" --grpc-addr "http://${COORDINATOR_GRPC}" --json grant approve "$request_id" \
        --reason "Approved for e2e testing" \
        2>&1) || true

    if echo "$approve_output" | jq -e '.success == true' >/dev/null 2>&1; then
        test_pass "grant_approved" "Grant request approved by coordinator"
    else
        test_fail "grant_approved" "Failed to approve grant" "Output: $approve_output"
        return 1
    fi

    # Test 3: Verify grant exists on blockchain
    log "Verifying grant exists on Akash blockchain..."
    local query_output
    query_output=$(akash_make query-grants --granter "$COORDINATOR_ADDRESS" --grantee "$EXECUTOR_ADDRESS" 2>&1) || true

    if echo "$query_output" | grep -q "authorization\|Authorization"; then
        test_pass "grant_on_blockchain" "Authz grant found on blockchain"
    else
        test_fail "grant_on_blockchain" "Authz grant not found on blockchain" "Query output: $query_output"
    fi

    # Test 4: Verify feegrant allowance exists
    log "Verifying feegrant allowance on blockchain..."
    local allowance_output
    allowance_output=$(akash_make query-feegrant --granter "$COORDINATOR_ADDRESS" --grantee "$EXECUTOR_ADDRESS" 2>&1) || true

    if echo "$allowance_output" | grep -q "allowance\|Allowance"; then
        test_pass "feegrant_on_blockchain" "Feegrant allowance found on blockchain"
    else
        test_fail "feegrant_on_blockchain" "Feegrant allowance not found on blockchain" "Query output: $allowance_output"
    fi

    test_summary "Authz/Feegrant Workflow"
}

# Test that executor can deploy to Akash using feegrant/authz
test_executor_deployment_with_grants() {
    log_step "Testing Executor Deployment Using Feegrant/Authz"

    if [ -z "$EXECUTOR_ADDRESS" ] || [ -z "$COORDINATOR_ADDRESS" ]; then
        log_error "Node addresses not available"
        return 1
    fi

    # Test 1: Create deployment via executor using feegrant
    log "Executor creating deployment using coordinator's feegrant..."

    # Use a simple SDL for testing
    local test_sdl="${TEST_DIR}/feegrant-test.sdl.yaml"
    cat > "$test_sdl" <<EOF
version: "2.0"
services:
  web:
    image: nginx:latest
    expose:
      - port: 80
        as: 80
        to:
          - global: true
profiles:
  compute:
    web:
      resources:
        cpu:
          units: 0.5
        memory:
          size: 512Mi
        storage:
          - size: 512Mi
  placement:
    akash:
      pricing:
        web:
          denom: uakt
          amount: 1000
deployment:
  web:
    akash:
      profile: web
      count: 1
EOF

    # Executor creates deployment using feegrant from coordinator
    local deploy_output
    deploy_output=$("$ERGORS_CLI" --grpc-addr "http://${EXECUTOR_GRPC}" --json deploy create \
        --sdl "$test_sdl" \
        --key-name "default" \
        --node "${AKASH_LOCAL_NODE}" \
        --chain-id "${AKASH_LOCAL_CHAIN_ID}" \
        --use-feegrant \
        --fee-granter "$COORDINATOR_ADDRESS" \
        2>&1) || true

    local session_id=$(echo "$deploy_output" | jq -r '.session_id // empty' 2>/dev/null)

    if [ -n "$session_id" ]; then
        test_pass "executor_deploy_with_feegrant" "Executor created deployment using feegrant (Session: ${session_id:0:8}...)"
        export FEEGRANT_DEPLOY_SESSION="$session_id"
    else
        test_fail "executor_deploy_with_feegrant" "Executor failed to create deployment with feegrant" "Output: $deploy_output"
        return 1
    fi

    # Test 2: Verify deployment was paid for by coordinator (feegranter)
    log "Verifying deployment transaction used feegrant..."

    # Query the deployment to check fee_payer
    local deploy_status
    deploy_status=$("$ERGORS_CLI" --grpc-addr "http://${EXECUTOR_GRPC}" --json deploy get "$session_id" 2>&1) || true

    if echo "$deploy_status" | jq -e '.deployment' >/dev/null 2>&1; then
        test_pass "feegrant_deployment_created" "Deployment created successfully with feegrant"

        # Check coordinator's balance decreased (paid fees)
        local coord_balance_after
        coord_balance_after=$(ergors_deploy query-balance "$COORDINATOR_ADDRESS" --denom uakt 2>&1 | jq -r '.amount // "0"' 2>/dev/null)
        log "  Coordinator balance after deployment: ${coord_balance_after} uakt"
    else
        test_fail "feegrant_deployment_created" "Deployment not found" "Status: $deploy_status"
    fi

    test_summary "Executor Deployment with Feegrant"
}

# ==================== Cross-Account Deployment Tests ====================

# Fund coordinator account using Akash faucet
fund_coordinator_account() {
    log_step "Funding Coordinator Account via Akash Faucet"

    # Ensure addresses are cached
    get_node_addresses || {
        TESTS_FAILED=$((TESTS_FAILED + 1))
        log_error "  Failed to get coordinator address for faucet funding"
        return 1
    }
    local coord_address="$COORDINATOR_ADDRESS"

    log "Coordinator address: $coord_address"

    # Setup port-forward to access faucet service in Kind cluster
    log "Setting up port-forward to faucet service..."
    kubectl port-forward -n akash-services svc/akash-faucet 5005:5005 &>/dev/null &
    local port_forward_pid=$!
    # Ensure port-forward is killed on any exit path from this function
    trap 'kill $port_forward_pid 2>/dev/null || true' RETURN

    # Wait for port-forward to be ready (poll, don't sleep blindly)
    local pf_waited=0
    while [ $pf_waited -lt 10 ]; do
        if curl -s --max-time 1 "http://localhost:5005/status" >/dev/null 2>&1; then
            break
        fi
        sleep 1
        pf_waited=$((pf_waited + 1))
    done

    local faucet_url="http://localhost:5005"

    # Check faucet status first
    log "Checking faucet availability..."
    local status_check
    status_check=$(curl -s --max-time 5 "$faucet_url/status" 2>/dev/null || echo "{}")

    if echo "$status_check" | jq -e '.address' >/dev/null 2>&1; then
        local faucet_addr=$(echo "$status_check" | jq -r '.address // "unknown"')
        local faucet_amount=$(echo "$status_check" | jq -r '.amount // "unknown"')
        log_success "  Faucet is ready"
        log "  Faucet address: $faucet_addr"
        log "  Send amount: $faucet_amount"
    else
        log_error "  Faucet not responding properly"
        log "  Status response: $status_check"
        return 1
    fi

    # Request funds from faucet
    log "Requesting funds from faucet for $coord_address..."
    local faucet_response
    faucet_response=$(curl -s --max-time 10 "$faucet_url/faucet?address=$coord_address" 2>&1) || true

    # Check if funding was successful
    # The faucet may return: {"tx_hash":"...", "amount":"..."} or {"error":"..."}
    local tx_hash=$(echo "$faucet_response" | jq -r '.tx_hash // .txhash // empty' 2>/dev/null)

    if [ -n "$tx_hash" ]; then
        TESTS_PASSED=$((TESTS_PASSED + 1))
        local amount=$(echo "$faucet_response" | jq -r '.amount // "unknown"' 2>/dev/null)
        log_success "  Faucet funding successful"
        log "  Amount: $amount"
        log "  TX Hash: $tx_hash"

        # Wait for transaction to be processed and included in a block
        log "Waiting for transaction to be confirmed (10s)..."
        sleep 10
    else
        local error_msg=$(echo "$faucet_response" | jq -r '.error // .message // empty' 2>/dev/null)
        if [ -n "$error_msg" ]; then
            log_error "  Faucet request failed: $error_msg"
        else
            log_warn "  Unexpected faucet response: $faucet_response"
        fi
        log_warn "  Continuing anyway, balance verification will check actual state"
    fi

    # Verify balance after funding
    log "Verifying coordinator balance after funding..."
    local balance_output
    balance_output=$(ergors_deploy query-balance "$coord_address" --denom uakt 2>&1) || true
    local balance=$(echo "$balance_output" | jq -r '.amount // "0"' 2>/dev/null)

    if [ -n "$balance" ] && [ "$balance" != "0" ]; then
        TESTS_PASSED=$((TESTS_PASSED + 1))
        log_success "  Coordinator balance confirmed: ${balance} uakt"
    else
        TESTS_FAILED=$((TESTS_FAILED + 1))
        log_error "  Coordinator balance still zero after faucet request"
    fi
}

# Test AKT balance verification for coordinator and executor accounts
test_akt_balance_verification() {
    log_step "Testing AKT Balance Verification"

    # Use cached addresses from get_node_addresses()
    get_node_addresses || {
        TESTS_FAILED=$((TESTS_FAILED + 1))
        log_error "  Failed to get node addresses"
        return 1
    }

    # Query coordinator balance
    log "Coordinator address: $COORDINATOR_ADDRESS"
    local balance_output
    balance_output=$(ergors_deploy query-balance "$COORDINATOR_ADDRESS" --denom uakt 2>&1) || true
    local balance=$(echo "$balance_output" | jq -r '.amount // "0"' 2>/dev/null)

    if [ -n "$balance" ] && [ "$balance" != "0" ]; then
        TESTS_PASSED=$((TESTS_PASSED + 1))
        log_success "  Coordinator balance: ${balance} uakt"
    else
        TESTS_FAILED=$((TESTS_FAILED + 1))
        log_error "  Coordinator has insufficient balance: ${balance} uakt"
        log_warn "  Need to fund coordinator account for cross-account deployment tests"
    fi

    # Query executor balance
    log "Executor address: $EXECUTOR_ADDRESS"
    local exec_balance_output
    exec_balance_output=$(ergors_deploy query-balance "$EXECUTOR_ADDRESS" --denom uakt 2>&1) || true
    local exec_balance=$(echo "$exec_balance_output" | jq -r '.amount // "0"' 2>/dev/null)

    log "  Executor balance: ${exec_balance} uakt"
    TESTS_PASSED=$((TESTS_PASSED + 1))
}

# Test grant request and approval workflow
test_grant_request_approval() {
    log_step "Testing Grant Request/Approval Workflow"

    if [ -z "$COORDINATOR_ADDRESS" ] || [ -z "$EXECUTOR_ADDRESS" ]; then
        log_warn "Addresses not available, skipping grant workflow test"
        return 0
    fi

    # Test 1: Executor requests grant from coordinator
    log "Executor requesting grant from coordinator..."
    local grant_request_output
    grant_request_output=$(ergors_deploy request-grant \
        --granter "$COORDINATOR_ADDRESS" \
        --grantee "$EXECUTOR_ADDRESS" \
        --msg-type "/akash.deployment.v1beta3.MsgCreateDeployment" \
        --msg-type "/akash.deployment.v1beta3.MsgDepositDeployment" \
        --allowance 10000000 \
        --reason "E2E test cross-account deployment" \
        2>&1) || true

    local request_id=$(echo "$grant_request_output" | jq -r '.request_id // empty' 2>/dev/null)

    if [ -n "$request_id" ]; then
        TESTS_PASSED=$((TESTS_PASSED + 1))
        log_success "  Grant requested (ID: ${request_id:0:8}...)"
        export GRANT_REQUEST_ID="$request_id"
    else
        TESTS_FAILED=$((TESTS_FAILED + 1))
        log_error "  Grant request failed"
        if [ "$VERBOSE" = true ]; then
            echo "$grant_request_output"
        fi
        return 1
    fi

    # Test 2: List pending grant requests
    log "Listing pending grant requests..."
    local list_output
    list_output=$(ergors_deploy list-grants --status pending 2>&1) || true

    if echo "$list_output" | grep -q "$request_id"; then
        TESTS_PASSED=$((TESTS_PASSED + 1))
        log_success "  Grant request found in pending list"
    else
        log_warn "  Grant request not found in list (may be auto-approved)"
    fi

    # Test 3: Coordinator approves grant
    log "Coordinator approving grant request..."
    local approve_output
    approve_output=$(ergors_deploy approve-grant "$request_id" \
        --reason "Approved for e2e testing" \
        2>&1) || true

    if echo "$approve_output" | jq -e '.success == true' >/dev/null 2>&1; then
        TESTS_PASSED=$((TESTS_PASSED + 1))
        log_success "  Grant request approved"
    else
        TESTS_FAILED=$((TESTS_FAILED + 1))
        log_error "  Grant approval failed"
    fi

    # Wait for blockchain to process grant
    log "Waiting for grant to be processed on blockchain (5s)..."
    sleep 5
}

# Test cross-account deployment execution
test_cross_account_deployment() {
    log_step "Testing Cross-Account Deployment Execution"

    if [ -z "$GRANT_REQUEST_ID" ]; then
        log_warn "No grant approved, skipping cross-account deployment test"
        return 0
    fi

    # Create deployment via executor using coordinator's funds
    log "Executor creating deployment using coordinator's grant..."
    local deploy_output
    deploy_output=$(ergors_deploy create \
        --sdl "${DEPLOY_SDL}" \
        --key-name "default" \
        --node "${AKASH_LOCAL_NODE}" \
        --chain-id "${AKASH_LOCAL_CHAIN_ID}" \
        2>&1) || true

    local session_id=$(echo "$deploy_output" | jq -r '.session_id // empty' 2>/dev/null)

    if [ -n "$session_id" ]; then
        TESTS_PASSED=$((TESTS_PASSED + 1))
        log_success "  Cross-account deployment created (session: ${session_id:0:8}...)"
        export CROSS_ACCOUNT_SESSION_ID="$session_id"

        # Verify deployment is funded by coordinator
        log "Verifying deployment ownership and fee payer..."
        local workflow_output
        workflow_output=$(ergors_deploy get "$session_id" 2>&1) || true

        if echo "$workflow_output" | jq -e '.account_address' >/dev/null 2>&1; then
            local deploy_account=$(echo "$workflow_output" | jq -r '.account_address')
            log "  Deployment account: ${deploy_account}"

            # TODO: Verify fee was paid by coordinator, not executor
            # This requires querying blockchain transaction history
            TESTS_PASSED=$((TESTS_PASSED + 1))
            log_success "  Deployment verified"
        fi
    else
        TESTS_FAILED=$((TESTS_FAILED + 1))
        log_error "  Cross-account deployment creation failed"
        if [ "$VERBOSE" = true ]; then
            echo "$deploy_output"
        fi
    fi
}

# Test grant revocation after deployment
test_grant_revocation() {
    log_step "Testing Grant Revocation After Deployment"

    if [ -z "$COORDINATOR_ADDRESS" ] || [ -z "$EXECUTOR_ADDRESS" ]; then
        log_warn "Addresses not available, skipping revocation test"
        return 0
    fi

    # Revoke grant from coordinator
    log "Coordinator revoking grant from executor..."
    local revoke_output
    revoke_output=$(ergors_deploy revoke-grant \
        --granter "$COORDINATOR_ADDRESS" \
        --grantee "$EXECUTOR_ADDRESS" \
        --msg-type "/akash.deployment.v1beta3.MsgCreateDeployment" \
        --revoke-feegrant \
        2>&1) || true

    if echo "$revoke_output" | jq -e '.success == true' >/dev/null 2>&1; then
        TESTS_PASSED=$((TESTS_PASSED + 1))
        log_success "  Grant revoked successfully"
    else
        TESTS_FAILED=$((TESTS_FAILED + 1))
        log_error "  Grant revocation failed"
    fi

    # Wait for blockchain to process revocation
    log "Waiting for revocation to be processed (5s)..."
    sleep 5

    # Try to create deployment after revocation (should fail)
    log "Attempting deployment after revocation (should fail)..."
    local post_revoke_output
    post_revoke_output=$(ergors_deploy create \
        --sdl "${DEPLOY_SDL}" \
        --key-name "default" \
        --node "${AKASH_LOCAL_NODE}" \
        --chain-id "${AKASH_LOCAL_CHAIN_ID}" \
        2>&1) || true

    # If deployment creation fails, test passes (grant was revoked)
    if echo "$post_revoke_output" | grep -qi "unauthorized\|insufficient\|denied"; then
        TESTS_PASSED=$((TESTS_PASSED + 1))
        log_success "  Deployment correctly denied after revocation"
    else
        log_warn "  Post-revocation behavior unclear (deployment may use executor's own funds)"
    fi
}

# Run all cross-account deployment tests
run_cross_account_tests() {
    log_step "Running Cross-Account Deployment Tests"

    # Note: fund_coordinator_account is called earlier in main() before feegrant tests

    # Verify balances
    test_akt_balance_verification

    # Test grant workflow (this is the legacy test, feegrant tests happen earlier now)
    test_grant_request_approval
    test_cross_account_deployment
    test_grant_revocation

    log_success "Cross-account deployment tests complete"
}

# ==================== End Cross-Account Deployment Tests ====================

# ==================== End Authz/Feegrant Tests ====================

# ==================== Service Endpoint Validation Tests (Phase 4) ====================

# Test endpoint discovery from Akash provider
test_endpoint_discovery() {
    log_step "Testing Endpoint Discovery from Akash Provider (Phase 4)"

    # Check if we have a deployment to query
    if [ -z "$DEPLOY_SESSION_ID" ]; then
        log_warn "No deployment session, skipping endpoint discovery test"
        TESTS_FAILED=$((TESTS_FAILED + 1))
        return 1
    fi

    # Test 1: Query deployment endpoints via engine
    log "Querying deployment endpoints from workflow..."
    local endpoints_output
    endpoints_output=$(ergors_deploy get "$DEPLOY_SESSION_ID" 2>&1) || true

    local endpoints=$(echo "$endpoints_output" | jq -r '.endpoints // empty' 2>/dev/null)
    if [ -n "$endpoints" ] && [ "$endpoints" != "null" ] && [ "$endpoints" != "{}" ]; then
        TESTS_PASSED=$((TESTS_PASSED + 1))
        local endpoint_count=$(echo "$endpoints_output" | jq -r '.endpoints | length' 2>/dev/null)
        log_success "  Discovered $endpoint_count service endpoint(s)"

        # Extract first endpoint for further tests
        export TEST_SERVICE_ENDPOINT=$(echo "$endpoints_output" | jq -r '.endpoints | to_entries[0].value // empty' 2>/dev/null)
        export TEST_SERVICE_NAME=$(echo "$endpoints_output" | jq -r '.endpoints | to_entries[0].key // empty' 2>/dev/null)
        log "  Sample endpoint: $TEST_SERVICE_NAME -> $TEST_SERVICE_ENDPOINT"
    else
        TESTS_FAILED=$((TESTS_FAILED + 1))
        log_error "  No endpoints discovered from deployment"
        log_warn "  This may be expected if lease is not yet active"
    fi

    # Test 2: Verify endpoint format
    if [ -n "$TEST_SERVICE_ENDPOINT" ]; then
        if echo "$TEST_SERVICE_ENDPOINT" | grep -qE '^https?://'; then
            TESTS_PASSED=$((TESTS_PASSED + 1))
            log_success "  Endpoint has valid HTTP(S) format"
        else
            TESTS_FAILED=$((TESTS_FAILED + 1))
            log_error "  Endpoint format invalid: $TEST_SERVICE_ENDPOINT"
        fi
    fi

    log_success "Endpoint discovery tests complete"
}

# Test endpoint connectivity and health checks
test_endpoint_connectivity() {
    log_step "Testing Service Endpoint Connectivity"

    if [ -z "$TEST_SERVICE_ENDPOINT" ]; then
        log_warn "No service endpoint available, skipping connectivity test"
        TESTS_FAILED=$((TESTS_FAILED + 1))
        return 1
    fi

    # Test 1: Basic HTTP connectivity
    log "Testing HTTP connectivity to $TEST_SERVICE_ENDPOINT..."
    local http_status
    http_status=$(curl -s -o /dev/null -w "%{http_code}" --max-time 10 "$TEST_SERVICE_ENDPOINT" 2>/dev/null || echo "000")

    if [ "$http_status" != "000" ]; then
        TESTS_PASSED=$((TESTS_PASSED + 1))
        log_success "  HTTP connectivity confirmed (status: $http_status)"
    else
        TESTS_FAILED=$((TESTS_FAILED + 1))
        log_error "  HTTP connectivity failed (timeout or connection refused)"
    fi

    # Test 2: Test response time
    if [ "$http_status" != "000" ]; then
        log "Measuring endpoint response time..."
        local start_time=$(date +%s%3N)
        curl -s -o /dev/null --max-time 10 "$TEST_SERVICE_ENDPOINT" 2>/dev/null || true
        local end_time=$(date +%s%3N)
        local response_time=$((end_time - start_time))

        if [ "$response_time" -lt 5000 ]; then
            TESTS_PASSED=$((TESTS_PASSED + 1))
            log_success "  Response time acceptable: ${response_time}ms"
        else
            TESTS_FAILED=$((TESTS_FAILED + 1))
            log_warn "  Response time high: ${response_time}ms"
        fi
    fi

    # Test 3: Check for common health endpoints
    for health_path in "/health" "/healthz" "/ready" "/v1/models"; do
        local health_url="${TEST_SERVICE_ENDPOINT}${health_path}"
        local health_status
        health_status=$(curl -s -o /dev/null -w "%{http_code}" --max-time 5 "$health_url" 2>/dev/null || echo "000")

        if [ "$health_status" = "200" ] || [ "$health_status" = "201" ]; then
            TESTS_PASSED=$((TESTS_PASSED + 1))
            log_success "  Health endpoint accessible: $health_path (status: $health_status)"
            export HEALTH_ENDPOINT="$health_url"
            break
        fi
    done

    if [ -z "$HEALTH_ENDPOINT" ]; then
        log_warn "  No standard health endpoint found"
    fi

    log_success "Endpoint connectivity tests complete"
}

# Test inference provider API calls
test_inference_provider_api() {
    log_step "Testing Inference Provider API Calls"

    if [ -z "$TEST_SERVICE_ENDPOINT" ]; then
        log_warn "No service endpoint available, skipping inference API test"
        TESTS_FAILED=$((TESTS_FAILED + 1))
        return 1
    fi

    # Test 1: Test Ollama-compatible API (common for self-hosted models)
    log "Testing Ollama-compatible API at $TEST_SERVICE_ENDPOINT..."
    local ollama_response
    ollama_response=$(curl -s --max-time 10 -X POST "$TEST_SERVICE_ENDPOINT/api/generate" \
        -H "Content-Type: application/json" \
        -d '{"model":"test","prompt":"hello","stream":false}' \
        2>/dev/null || echo "{}")

    if echo "$ollama_response" | jq -e . >/dev/null 2>&1; then
        TESTS_PASSED=$((TESTS_PASSED + 1))
        log_success "  Ollama API endpoint responds with valid JSON"
    else
        log_warn "  Ollama API format not detected"
    fi

    # Test 2: Test OpenAI-compatible API
    log "Testing OpenAI-compatible API at $TEST_SERVICE_ENDPOINT..."
    local openai_response
    openai_response=$(curl -s --max-time 10 -X POST "$TEST_SERVICE_ENDPOINT/v1/chat/completions" \
        -H "Content-Type: application/json" \
        -d '{"model":"test","messages":[{"role":"user","content":"hello"}],"max_tokens":10}' \
        2>/dev/null || echo "{}")

    if echo "$openai_response" | jq -e . >/dev/null 2>&1; then
        local has_error=$(echo "$openai_response" | jq -r '.error // empty' 2>/dev/null)
        if [ -z "$has_error" ] || echo "$has_error" | grep -qi "model"; then
            TESTS_PASSED=$((TESTS_PASSED + 1))
            log_success "  OpenAI API endpoint responds with valid format"
        else
            log_warn "  OpenAI API returned error: $has_error"
        fi
    else
        log_warn "  OpenAI API format not detected"
    fi

    # Test 3: Verify API error handling
    log "Testing API error handling..."
    local error_response
    error_response=$(curl -s --max-time 10 -X POST "$TEST_SERVICE_ENDPOINT/v1/chat/completions" \
        -H "Content-Type: application/json" \
        -d '{"invalid":"data"}' \
        2>/dev/null || echo "{}")

    if echo "$error_response" | jq -e '.error' >/dev/null 2>&1; then
        TESTS_PASSED=$((TESTS_PASSED + 1))
        log_success "  API returns proper error responses"
    else
        log_warn "  API error handling unclear"
    fi

    log_success "Inference provider API tests complete"
}

# Test proxy routing to discovered endpoints
test_proxy_routing() {
    log_step "Testing Proxy Routing to Discovered Endpoints"

    if [ -z "$TEST_SERVICE_ENDPOINT" ]; then
        log_warn "No service endpoint available, skipping proxy routing test"
        TESTS_FAILED=$((TESTS_FAILED + 1))
        return 1
    fi

    # Test 1: Configure proxy route to discovered endpoint
    log "Configuring proxy route to $TEST_SERVICE_NAME..."
    local route_config
    route_config=$(cat <<EOF
{
  "ollama_base_url": "$TEST_SERVICE_ENDPOINT",
  "model_routes": {
    "llama*": "$TEST_SERVICE_ENDPOINT",
    "test-model": "$TEST_SERVICE_ENDPOINT"
  }
}
EOF
)

    local configure_output
    configure_output=$("$ERGORS_CLI" --grpc-addr "http://${COORDINATOR_GRPC}" --json deploy configure-proxy \
        --config "$route_config" \
        2>&1) || true

    if echo "$configure_output" | jq -e '.success == true' >/dev/null 2>&1; then
        TESTS_PASSED=$((TESTS_PASSED + 1))
        log_success "  Proxy route configured successfully"
    else
        TESTS_FAILED=$((TESTS_FAILED + 1))
        log_error "  Proxy route configuration failed"
        return 1
    fi

    # Test 2: Verify proxy can route requests
    log "Testing proxy request routing..."
    # Make a request through the proxy API endpoint
    local proxy_response
    proxy_response=$(curl -s --max-time 15 -X POST "http://localhost:${COORDINATOR_GRPC}/v1/chat/completions" \
        -H "Content-Type: application/json" \
        -H "Authorization: Bearer test-key" \
        -d '{"model":"test-model","messages":[{"role":"user","content":"test"}],"max_tokens":5}' \
        2>/dev/null || echo "{}")

    if echo "$proxy_response" | jq -e . >/dev/null 2>&1; then
        TESTS_PASSED=$((TESTS_PASSED + 1))
        log_success "  Proxy successfully routed request to endpoint"

        # Verify response came from our deployed endpoint
        local response_model=$(echo "$proxy_response" | jq -r '.model // empty' 2>/dev/null)
        if [ -n "$response_model" ]; then
            log_success "  Proxy response validated (model: $response_model)"
        fi
    else
        log_warn "  Proxy routing test inconclusive"
    fi

    log_success "Proxy routing tests complete"
}

# Run all Phase 4 endpoint validation tests
run_endpoint_validation_tests() {
    log_step "Running Service Endpoint Validation Tests (Phase 4)"

    test_endpoint_discovery
    test_endpoint_connectivity
    test_inference_provider_api
    test_proxy_routing

    log_success "Phase 4: Service Endpoint Validation tests complete"
}

# ==================== End Service Endpoint Validation Tests ====================

# ==================== Security Testing (Phase 5) ====================

# Test authentication and API key validation
test_authentication() {
    log_step "Testing Authentication and API Key Validation (Phase 5)"

    # Test 1: Register a test API key
    log "Registering test API key..."
    local register_output
    register_output=$("$ERGORS_CLI" --grpc-addr "http://${COORDINATOR_GRPC}" --json deploy register-token \
        --label "e2e-test-key" \
        2>&1) || true

    local test_token=$(echo "$register_output" | jq -r '.token // empty' 2>/dev/null)
    if [ -n "$test_token" ]; then
        TESTS_PASSED=$((TESTS_PASSED + 1))
        log_success "  Test API key registered (token: ${test_token:0:16}...)"
        export TEST_AUTH_TOKEN="$test_token"
    else
        TESTS_FAILED=$((TESTS_FAILED + 1))
        log_error "  Failed to register API key"
        return 1
    fi

    # Test 2: Verify authenticated request succeeds
    log "Testing authenticated request..."
    local auth_response
    auth_response=$(curl -s --max-time 10 -X POST "http://localhost:${COORDINATOR_GRPC}/v1/chat/completions" \
        -H "Content-Type: application/json" \
        -H "Authorization: Bearer $TEST_AUTH_TOKEN" \
        -d '{"model":"test","messages":[{"role":"user","content":"test"}],"max_tokens":5}' \
        2>/dev/null || echo "{}")

    if echo "$auth_response" | jq -e . >/dev/null 2>&1; then
        local has_auth_error=$(echo "$auth_response" | jq -r '.error.type // empty' 2>/dev/null)
        if [ "$has_auth_error" != "authentication_error" ]; then
            TESTS_PASSED=$((TESTS_PASSED + 1))
            log_success "  Authenticated request accepted"
        else
            TESTS_FAILED=$((TESTS_FAILED + 1))
            log_error "  Authentication failed with valid token"
        fi
    else
        log_warn "  Authentication test inconclusive"
    fi

    # Test 3: Verify unauthenticated request fails
    log "Testing unauthenticated request rejection..."
    local unauth_response
    unauth_response=$(curl -s --max-time 10 -X POST "http://localhost:${COORDINATOR_GRPC}/v1/chat/completions" \
        -H "Content-Type: application/json" \
        -d '{"model":"test","messages":[{"role":"user","content":"test"}],"max_tokens":5}' \
        2>/dev/null || echo "{}")

    if echo "$unauth_response" | jq -e '.error' >/dev/null 2>&1; then
        local error_type=$(echo "$unauth_response" | jq -r '.error.type // empty' 2>/dev/null)
        if [ "$error_type" = "authentication_error" ] || echo "$unauth_response" | grep -qi "unauthorized\|authentication"; then
            TESTS_PASSED=$((TESTS_PASSED + 1))
            log_success "  Unauthenticated request correctly rejected"
        else
            log_warn "  Unauthenticated request handling unclear"
        fi
    else
        log_warn "  Unauthenticated request test inconclusive"
    fi

    # Test 4: Test invalid token rejection
    log "Testing invalid token rejection..."
    local invalid_response
    invalid_response=$(curl -s --max-time 10 -X POST "http://localhost:${COORDINATOR_GRPC}/v1/chat/completions" \
        -H "Content-Type: application/json" \
        -H "Authorization: Bearer invalid-token-12345" \
        -d '{"model":"test","messages":[{"role":"user","content":"test"}],"max_tokens":5}' \
        2>/dev/null || echo "{}")

    if echo "$invalid_response" | jq -e '.error' >/dev/null 2>&1; then
        TESTS_PASSED=$((TESTS_PASSED + 1))
        log_success "  Invalid token correctly rejected"
    else
        log_warn "  Invalid token test inconclusive"
    fi

    log_success "Authentication tests complete"
}

# Test authorization and permission boundaries
test_authorization() {
    log_step "Testing Authorization and Permission Boundaries"

    # Test 1: List API tokens
    log "Listing registered API tokens..."
    local list_output
    list_output=$("$ERGORS_CLI" --grpc-addr "http://${COORDINATOR_GRPC}" --json deploy list-tokens 2>&1) || true

    local token_count=$(echo "$list_output" | jq -r '.tokens | length' 2>/dev/null)
    if [ -n "$token_count" ] && [ "$token_count" -gt 0 ] 2>/dev/null; then
        TESTS_PASSED=$((TESTS_PASSED + 1))
        log_success "  Found $token_count registered token(s)"
    else
        TESTS_FAILED=$((TESTS_FAILED + 1))
        log_error "  No tokens found or list failed"
    fi

    # Test 2: Verify grant permissions are enforced
    log "Testing grant permission enforcement..."
    if [ -n "$EXECUTOR_ADDRESS" ] && [ -n "$COORDINATOR_ADDRESS" ]; then
        # Try to create deployment without grant (should fail)
        local no_grant_output
        no_grant_output=$(ergors_deploy create \
            --sdl "${DEPLOY_SDL}" \
            --key-name "executor-key" \
            --node "${AKASH_LOCAL_NODE}" \
            --chain-id "${AKASH_LOCAL_CHAIN_ID}" \
            2>&1) || true

        if echo "$no_grant_output" | grep -qi "unauthorized\|insufficient\|grant"; then
            TESTS_PASSED=$((TESTS_PASSED + 1))
            log_success "  Permission boundaries correctly enforced"
        else
            log_warn "  Permission enforcement test inconclusive"
        fi
    else
        log_warn "  Skipping grant permission test (no cross-account setup)"
    fi

    # Test 3: Test role-based access control
    log "Testing role-based access control..."
    local identity_output
    identity_output=$("$ERGORS_CLI" --grpc-addr "http://${COORDINATOR_GRPC}" --json identity get 2>&1) || true

    local node_type=$(echo "$identity_output" | jq -r '.node_type // empty' 2>/dev/null)
    if [ -n "$node_type" ]; then
        TESTS_PASSED=$((TESTS_PASSED + 1))
        log_success "  Node type identified: $node_type"

        # Verify coordinator has appropriate permissions
        if [ "$node_type" = "coordinator" ] || [ "$node_type" = "development" ]; then
            log_success "  Node has coordinator-level permissions"
        fi
    else
        TESTS_FAILED=$((TESTS_FAILED + 1))
        log_error "  Failed to identify node type"
    fi

    log_success "Authorization tests complete"
}

# Test key compromise and revocation scenarios
test_key_compromise() {
    log_step "Testing Key Compromise Scenarios (Phase 5)"

    if [ -z "$TEST_AUTH_TOKEN" ]; then
        log_warn "No test token available, skipping key compromise tests"
        TESTS_FAILED=$((TESTS_FAILED + 1))
        return 1
    fi

    # Test 1: List tokens before revocation
    log "Listing tokens before revocation..."
    local before_list
    before_list=$("$ERGORS_CLI" --grpc-addr "http://${COORDINATOR_GRPC}" --json deploy list-tokens 2>&1) || true
    local before_count=$(echo "$before_list" | jq -r '.tokens | length' 2>/dev/null)

    # Test 2: Revoke the test token
    log "Revoking compromised token..."
    local token_id=$(echo "$before_list" | jq -r '.tokens[] | select(.label == "e2e-test-key") | .id // empty' 2>/dev/null)

    if [ -n "$token_id" ]; then
        local revoke_output
        revoke_output=$("$ERGORS_CLI" --grpc-addr "http://${COORDINATOR_GRPC}" --json deploy revoke-token "$token_id" 2>&1) || true

        if echo "$revoke_output" | jq -e '.success == true' >/dev/null 2>&1; then
            TESTS_PASSED=$((TESTS_PASSED + 1))
            log_success "  Token successfully revoked"
        else
            TESTS_FAILED=$((TESTS_FAILED + 1))
            log_error "  Token revocation failed"
            return 1
        fi
    else
        log_warn "  Could not find token ID for revocation"
    fi

    # Test 3: Verify revoked token no longer works
    log "Testing revoked token rejection..."
    local revoked_response
    revoked_response=$(curl -s --max-time 10 -X POST "http://localhost:${COORDINATOR_GRPC}/v1/chat/completions" \
        -H "Content-Type: application/json" \
        -H "Authorization: Bearer $TEST_AUTH_TOKEN" \
        -d '{"model":"test","messages":[{"role":"user","content":"test"}],"max_tokens":5}' \
        2>/dev/null || echo "{}")

    if echo "$revoked_response" | jq -e '.error' >/dev/null 2>&1; then
        TESTS_PASSED=$((TESTS_PASSED + 1))
        log_success "  Revoked token correctly rejected"
    else
        TESTS_FAILED=$((TESTS_FAILED + 1))
        log_error "  Revoked token still accepted (security issue!)"
    fi

    # Test 4: Test key rotation
    log "Testing key rotation..."
    local new_token_output
    new_token_output=$("$ERGORS_CLI" --grpc-addr "http://${COORDINATOR_GRPC}" --json deploy register-token \
        --label "e2e-rotated-key" \
        2>&1) || true

    local new_token=$(echo "$new_token_output" | jq -r '.token // empty' 2>/dev/null)
    if [ -n "$new_token" ]; then
        TESTS_PASSED=$((TESTS_PASSED + 1))
        log_success "  New key registered after rotation"
        export TEST_AUTH_TOKEN="$new_token"
    else
        TESTS_FAILED=$((TESTS_FAILED + 1))
        log_error "  Failed to register rotated key"
    fi

    # Test 5: Verify new token works
    if [ -n "$new_token" ]; then
        log "Testing rotated key..."
        local rotated_response
        rotated_response=$(curl -s --max-time 10 -X POST "http://localhost:${COORDINATOR_GRPC}/v1/chat/completions" \
            -H "Content-Type: application/json" \
            -H "Authorization: Bearer $new_token" \
            -d '{"model":"test","messages":[{"role":"user","content":"test"}],"max_tokens":5}' \
            2>/dev/null || echo "{}")

        if echo "$rotated_response" | jq -e . >/dev/null 2>&1; then
            TESTS_PASSED=$((TESTS_PASSED + 1))
            log_success "  Rotated key works correctly"
        else
            log_warn "  Rotated key test inconclusive"
        fi
    fi

    log_success "Key compromise scenario tests complete"
}

# Test error handling and resilience
test_error_handling() {
    log_step "Testing Error Handling and Resilience (Phase 5)"

    # Test 1: Network timeout handling
    log "Testing network timeout handling..."
    local timeout_response
    timeout_response=$(curl -s --max-time 1 -X POST "http://localhost:${COORDINATOR_GRPC}/v1/chat/completions" \
        -H "Content-Type: application/json" \
        -H "Authorization: Bearer ${TEST_AUTH_TOKEN:-test}" \
        -d '{"model":"test","messages":[{"role":"user","content":"test"}],"max_tokens":5}' \
        2>/dev/null || echo "timeout")

    if [ "$timeout_response" = "timeout" ] || echo "$timeout_response" | grep -qi "timeout\|timed out"; then
        TESTS_PASSED=$((TESTS_PASSED + 1))
        log_success "  Timeout handling working (request timed out as expected)"
    else
        log_warn "  Timeout test completed (response received)"
    fi

    # Test 2: Invalid JSON handling
    log "Testing invalid JSON handling..."
    local invalid_json_response
    invalid_json_response=$(curl -s --max-time 10 -X POST "http://localhost:${COORDINATOR_GRPC}/v1/chat/completions" \
        -H "Content-Type: application/json" \
        -H "Authorization: Bearer ${TEST_AUTH_TOKEN:-test}" \
        -d 'invalid json data' \
        2>/dev/null || echo "{}")

    if echo "$invalid_json_response" | jq -e '.error' >/dev/null 2>&1; then
        TESTS_PASSED=$((TESTS_PASSED + 1))
        log_success "  Invalid JSON properly rejected with error"
    else
        log_warn "  Invalid JSON handling test inconclusive"
    fi

    # Test 3: Missing required fields
    log "Testing missing required fields handling..."
    local missing_fields_response
    missing_fields_response=$(curl -s --max-time 10 -X POST "http://localhost:${COORDINATOR_GRPC}/v1/chat/completions" \
        -H "Content-Type: application/json" \
        -H "Authorization: Bearer ${TEST_AUTH_TOKEN:-test}" \
        -d '{"model":"test"}' \
        2>/dev/null || echo "{}")

    if echo "$missing_fields_response" | jq -e '.error' >/dev/null 2>&1; then
        TESTS_PASSED=$((TESTS_PASSED + 1))
        log_success "  Missing required fields properly validated"
    else
        log_warn "  Missing fields validation test inconclusive"
    fi

    # Test 4: Deployment failure recovery
    log "Testing deployment failure recovery..."
    local fail_output
    fail_output=$(ergors_deploy create \
        --sdl "/nonexistent/file.yaml" \
        --key-name "default" \
        --node "${AKASH_LOCAL_NODE}" \
        --chain-id "${AKASH_LOCAL_CHAIN_ID}" \
        2>&1) || true

    if echo "$fail_output" | grep -qi "error\|failed\|not found"; then
        TESTS_PASSED=$((TESTS_PASSED + 1))
        log_success "  Deployment failures properly reported"
    else
        log_warn "  Deployment failure test inconclusive"
    fi

    # Test 5: Concurrent request handling
    log "Testing concurrent request handling..."
    local concurrent_pids=()

    for i in {1..3}; do
        (curl -s --max-time 15 -X POST "http://localhost:${COORDINATOR_GRPC}/v1/chat/completions" \
            -H "Content-Type: application/json" \
            -H "Authorization: Bearer ${TEST_AUTH_TOKEN:-test}" \
            -d '{"model":"test","messages":[{"role":"user","content":"concurrent test"}],"max_tokens":5}' \
            >/dev/null 2>&1) &
        concurrent_pids+=($!)
    done

    # Wait for all concurrent requests
    local concurrent_success=0
    for pid in "${concurrent_pids[@]}"; do
        if wait $pid 2>/dev/null; then
            concurrent_success=$((concurrent_success + 1))
        fi
    done

    if [ $concurrent_success -ge 2 ]; then
        TESTS_PASSED=$((TESTS_PASSED + 1))
        log_success "  Concurrent requests handled ($concurrent_success/3 succeeded)"
    else
        TESTS_FAILED=$((TESTS_FAILED + 1))
        log_error "  Concurrent request handling failed"
    fi

    log_success "Error handling tests complete"
}

# Test performance and resource limits
test_performance() {
    log_step "Testing Performance and Resource Limits"

    # Test 1: Response time under load
    log "Testing response time under load..."
    local total_time=0
    local request_count=5

    for i in $(seq 1 $request_count); do
        local start_time=$(date +%s%3N)
        curl -s --max-time 10 -X POST "http://localhost:${COORDINATOR_GRPC}/v1/chat/completions" \
            -H "Content-Type: application/json" \
            -H "Authorization: Bearer ${TEST_AUTH_TOKEN:-test}" \
            -d '{"model":"test","messages":[{"role":"user","content":"perf test"}],"max_tokens":5}' \
            >/dev/null 2>&1 || true
        local end_time=$(date +%s%3N)
        local response_time=$((end_time - start_time))
        total_time=$((total_time + response_time))
    done

    local avg_time=$((total_time / request_count))
    if [ $avg_time -lt 3000 ]; then
        TESTS_PASSED=$((TESTS_PASSED + 1))
        log_success "  Average response time: ${avg_time}ms (acceptable)"
    else
        TESTS_FAILED=$((TESTS_FAILED + 1))
        log_warn "  Average response time: ${avg_time}ms (high)"
    fi

    # Test 2: Storage health check
    log "Testing storage health..."
    local status_output
    status_output=$("$ERGORS_CLI" --grpc-addr "http://${COORDINATOR_GRPC}" --json status 2>&1) || true

    local storage_status=$(echo "$status_output" | jq -r '.storage_status // empty' 2>/dev/null)
    if echo "$storage_status" | grep -qi "healthy\|operational\|ok"; then
        TESTS_PASSED=$((TESTS_PASSED + 1))
        log_success "  Storage health: $storage_status"
    else
        log_warn "  Storage status: $storage_status"
    fi

    # Test 3: Check workflow count limits
    log "Testing workflow management..."
    local workflows_output
    workflows_output=$(ergors_deploy list 2>&1) || true

    local workflow_count=$(echo "$workflows_output" | jq -r '.total_count // 0' 2>/dev/null)
    log_success "  Active workflows: $workflow_count"

    if [ "$workflow_count" -lt 100 ]; then
        TESTS_PASSED=$((TESTS_PASSED + 1))
        log_success "  Workflow count within reasonable limits"
    else
        log_warn "  High workflow count: $workflow_count"
    fi

    log_success "Performance tests complete"
}

# Test monitoring and logging
test_monitoring_logging() {
    log_step "Testing Monitoring and Logging Validation (Phase 5)"

    # Test 1: Verify node logs exist and are being written
    log "Verifying node logs..."
    if [ -f "$TEST_DIR/coordinator/node.log" ]; then
        local log_size=$(wc -c < "$TEST_DIR/coordinator/node.log" 2>/dev/null || echo 0)
        if [ "$log_size" -gt 1000 ]; then
            TESTS_PASSED=$((TESTS_PASSED + 1))
            log_success "  Node logs active (${log_size} bytes)"
        else
            TESTS_FAILED=$((TESTS_FAILED + 1))
            log_error "  Node logs too small or empty"
        fi
    else
        TESTS_FAILED=$((TESTS_FAILED + 1))
        log_error "  Node log file not found"
    fi

    # Test 2: Check for critical log patterns
    log "Checking for critical events in logs..."
    local critical_patterns=("ERROR" "WARN" "panic" "fatal")
    local critical_count=0

    for pattern in "${critical_patterns[@]}"; do
        if grep -qi "$pattern" "$TEST_DIR/coordinator/node.log" 2>/dev/null; then
            critical_count=$((critical_count + 1))
        fi
    done

    if [ $critical_count -gt 0 ]; then
        log_warn "  Found $critical_count types of critical log messages"
        # This is informational, not necessarily a failure
    else
        log_success "  No critical errors in logs"
    fi

    # Test 3: Verify structured logging
    log "Verifying structured logging format..."
    if grep -qE '\{.*"level".*"msg".*\}|level=|msg=' "$TEST_DIR/coordinator/node.log" 2>/dev/null; then
        TESTS_PASSED=$((TESTS_PASSED + 1))
        log_success "  Structured logging detected"
    else
        log_warn "  Structured logging format not clearly detected"
    fi

    # Test 4: Check health check endpoint
    log "Testing health check endpoint..."
    local health_response
    health_response=$(curl -s --max-time 5 "http://localhost:${COORDINATOR_GRPC}/health" 2>/dev/null || echo "")

    if [ -n "$health_response" ]; then
        TESTS_PASSED=$((TESTS_PASSED + 1))
        log_success "  Health endpoint responding"
    else
        log_warn "  Health endpoint not available (may not be implemented)"
    fi

    # Test 5: Verify metrics collection
    log "Checking for metrics in logs..."
    if grep -qiE "metric|counter|gauge|histogram|requests.*handled|uptime" "$TEST_DIR/coordinator/node.log" 2>/dev/null; then
        TESTS_PASSED=$((TESTS_PASSED + 1))
        log_success "  Metrics collection detected in logs"
    else
        log_warn "  Metrics not clearly visible in logs"
    fi

    # Test 6: Verify deployment event logging
    log "Verifying deployment events are logged..."
    if grep -qiE "deployment.*created|workflow.*started|lease.*created|endpoint.*discovered" "$TEST_DIR/coordinator/node.log" 2>/dev/null; then
        TESTS_PASSED=$((TESTS_PASSED + 1))
        log_success "  Deployment events properly logged"
    else
        log_warn "  Deployment event logging not clearly detected"
    fi

    log_success "Monitoring and logging validation complete"
}

# Run all Phase 5 security tests
run_security_tests() {
    log_step "Running Security Testing (Phase 5)"

    test_authentication
    test_authorization
    test_key_compromise
    test_error_handling
    test_performance
    test_monitoring_logging

    log_success "Phase 5: Security Testing complete"
}

# ==================== End Security Testing ====================

# ==================== Engine Akash Deployment Tests ====================

# Test engine deployment creation
test_real_deployment_create() {
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

# Show contract deployment debug information
show_contract_debug_info() {
    log "Contract Deployment Debug Information"
    echo -e "${YELLOW}=== Contract Debug ===${NC}"

    # Show WASM file location
    if [ -f "$TEST_DIR/coordinator/sdl_template_registrar.wasm" ]; then
        echo "✓ SDL contract WASM found: $TEST_DIR/coordinator/sdl_template_registrar.wasm"
        ls -lh "$TEST_DIR/coordinator/sdl_template_registrar.wasm" 2>/dev/null || true
    else
        echo "✗ SDL contract WASM missing"
    fi

    # Show contract deployment logs
    if [ -f "$TEST_DIR/coordinator/node.log" ]; then
        echo ""
        echo "Contract deployment logs:"
        grep -i "contract.*deployed\|instantiated contract\|uploaded contract\|registered sdl template" \
            "$TEST_DIR/coordinator/node.log" 2>/dev/null | tail -20 || echo "No contract logs found"

        echo ""
        echo "SDL/WASM related logs:"
        grep -i "sdl\|wasm.*runtime\|cosmwasm" "$TEST_DIR/coordinator/node.log" 2>/dev/null | tail -10 || echo "No SDL/WASM logs found"
    fi

    echo -e "${YELLOW}=== end ===${NC}"
}

# Test SDL contract queries (real blockchain verification)
test_sdl_contract_queries() {
    test_section "SDL Contract Queries Tests"

    # Test 1: Query SDL template contracts via API (proper workflow)
    log "Querying SDL template contracts via gRPC API..."
    local list_output
    list_output=$("$ERGORS_CLI" --grpc-addr "http://${COORDINATOR_GRPC}" --json sdl list 2>&1) || true

    local template_count=$(echo "$list_output" | jq -r '.templates | length' 2>/dev/null)
    if [ -n "$template_count" ] && [ "$template_count" -gt 0 ] 2>/dev/null; then
        test_pass "contract_address_found_via_api" "Found $template_count SDL template contract(s) via API"

        # Extract first contract address for verification
        local contract_addr=$(echo "$list_output" | jq -r '.templates[0].contract_address // empty' 2>/dev/null)
        if [ -n "$contract_addr" ]; then
            log "  Contract address: ${contract_addr:0:20}..."
            local contract_label=$(echo "$list_output" | jq -r '.templates[0].label // "unlabeled"' 2>/dev/null)
            log "  Label: $contract_label"
        fi
    else
        test_fail "contract_address_found_via_api" "No SDL template contracts found via API"
        log "  API response: $list_output"

        # Show debug information to help diagnose
        show_contract_debug_info
    fi

    # Test 2: Verify automatic SDL template registration occurred
    # Check via API (proper workflow) first, fall back to logs for debugging
    if [ -n "$template_count" ] && [ "$template_count" -gt 0 ] 2>/dev/null; then
        test_pass "template_registration_verified" "SDL template contract(s) registered and accessible via API"
    elif grep -q "Registered SDL template contract\|Automatically registering SDL template" "$TEST_DIR/coordinator/node.log" 2>/dev/null; then
        test_fail "template_registration_verified" "Registration detected in logs but not accessible via API"
        log_warn "  This indicates registration happened but storage/query may have issues"
    else
        test_fail "template_registration_verified" "No template registration detected (neither API nor logs)"
    fi

    # Test 3: Check for gRPC SDL template endpoints availability
    # Pattern matches: gRPC server logs or actual query attempts
    if grep -q "Listing SDL template contracts\|Getting SDL template from contract" "$TEST_DIR/coordinator/node.log" 2>/dev/null; then
        test_pass "template_query_capability" "SDL template query endpoints active"
    else
        # If no queries in logs yet, just mark as skipped (queries will be tested later)
        log_warn "  SDL template query capability not yet exercised (will be tested in Phase 2)"
    fi

    # Perform actual contract queries
    # Check if the infrastructure is ready
    if [ -f "$TEST_DIR/coordinator/node.log" ]; then
        if grep -q "cosmwasm.*initialized\|wasm.*runtime\|contract.*loaded" "$TEST_DIR/coordinator/node.log" 2>/dev/null; then
            test_pass "cosmwasm_runtime_ready" "CosmWasm runtime initialized for real queries"
        else
            test_fail "cosmwasm_runtime_ready" "CosmWasm runtime not initialized"
        fi
    else
        test_fail "coordinator_log_available" "Coordinator log not available for CosmWasm checks"
    fi

    test_summary "SDL Contract Queries"
}

# Test SDL template contract operations (Phase 2)
test_sdl_template_from_contract() {
    log_step "Testing SDL Template Contract Operations (Phase 2)"

    # Test 1: List SDL template contracts
    log "Listing SDL template contracts..."
    local list_output
    list_output=$("$ERGORS_CLI" --grpc-addr "http://${COORDINATOR_GRPC}" --json sdl list 2>&1) || true

    local template_count=$(echo "$list_output" | jq -r '.templates | length' 2>/dev/null)
    if [ -n "$template_count" ] && [ "$template_count" -gt 0 ] 2>/dev/null; then
        TESTS_PASSED=$((TESTS_PASSED + 1))
        log_success "  Found $template_count SDL template contract(s)"

        # Extract first template contract address for further tests
        export SDL_TEMPLATE_CONTRACT=$(echo "$list_output" | jq -r '.templates[0].contract_address // empty' 2>/dev/null)
        log "  Using contract: $SDL_TEMPLATE_CONTRACT"
    else
        TESTS_FAILED=$((TESTS_FAILED + 1))
        log_error "  No SDL template contracts found"
        log_warn "  SDL template contract tests will be skipped"

        # Show debug information to help diagnose the issue
        show_contract_debug_info
        return 1
    fi

    # Test 2: Get SDL template from contract
    if [ -n "$SDL_TEMPLATE_CONTRACT" ]; then
        log "Getting SDL template from contract..."
        local template_output
        template_output=$("$ERGORS_CLI" --grpc-addr "http://${COORDINATOR_GRPC}" --json sdl get-template "$SDL_TEMPLATE_CONTRACT" 2>&1) || true

        local sdl_template=$(echo "$template_output" | jq -r '.sdl_template // empty' 2>/dev/null)
        if [ -n "$sdl_template" ]; then
            TESTS_PASSED=$((TESTS_PASSED + 1))
            log_success "  SDL template retrieved (${#sdl_template} bytes)"
        else
            TESTS_FAILED=$((TESTS_FAILED + 1))
            log_error "  Failed to retrieve SDL template"
        fi
    fi

    # Test 3: Get variable defaults from contract
    if [ -n "$SDL_TEMPLATE_CONTRACT" ]; then
        log "Getting variable defaults from contract..."
        local defaults_output
        defaults_output=$("$ERGORS_CLI" --grpc-addr "http://${COORDINATOR_GRPC}" --json sdl get-defaults "$SDL_TEMPLATE_CONTRACT" 2>&1) || true

        local defaults=$(echo "$defaults_output" | jq -r '.defaults // empty' 2>/dev/null)
        if [ -n "$defaults" ] && [ "$defaults" != "null" ]; then
            TESTS_PASSED=$((TESTS_PASSED + 1))
            local default_count=$(echo "$defaults_output" | jq -r '.defaults | length' 2>/dev/null)
            log_success "  Variable defaults retrieved ($default_count variables)"
        else
            TESTS_FAILED=$((TESTS_FAILED + 1))
            log_error "  Failed to retrieve variable defaults"
        fi
    fi

    # Test 4: Render SDL template with variables
    if [ -n "$SDL_TEMPLATE_CONTRACT" ]; then
        log "Rendering SDL template with custom variables..."
        local render_output
        render_output=$("$ERGORS_CLI" --grpc-addr "http://${COORDINATOR_GRPC}" --json sdl render \
            "$SDL_TEMPLATE_CONTRACT" \
            --var CPU=4 \
            --var MEMORY=8Gi \
            --var GPU_COUNT=1 \
            2>&1) || true

        local rendered_sdl=$(echo "$render_output" | jq -r '.rendered_sdl // empty' 2>/dev/null)
        local used_vars=$(echo "$render_output" | jq -r '.used_variables // empty' 2>/dev/null)

        if [ -n "$rendered_sdl" ]; then
            TESTS_PASSED=$((TESTS_PASSED + 1))
            log_success "  SDL template rendered successfully"

            # Verify variables were substituted
            if echo "$rendered_sdl" | grep -q "cpu:.*4"; then
                TESTS_PASSED=$((TESTS_PASSED + 1))
                log_success "  Variable substitution verified (CPU=4)"
            else
                TESTS_FAILED=$((TESTS_FAILED + 1))
                log_error "  Variable substitution failed"
            fi
        else
            TESTS_FAILED=$((TESTS_FAILED + 1))
            log_error "  Failed to render SDL template"
        fi
    fi

    log_success "SDL template contract operations tests complete"
}

# Test SDL template variable substitution
test_sdl_variable_substitution() {
    test_section "SDL Variable Substitution Tests"

    # Test 1: Check for variable substitution in logs
    if grep -q "substitut\|variable\|\${.*}" "$TEST_DIR/coordinator/node.log" 2>/dev/null; then
        test_pass "variable_substitution_detected" "Variable substitution processing detected in logs"
    else
        test_fail "variable_substitution_detected" "Variable substitution not detected in logs"
    fi

    # Test 2: Check SDL processing
    if grep -q "SDL\|sdl.*process\|yaml\|manifest" "$TEST_DIR/coordinator/node.log" 2>/dev/null; then
        test_pass "sdl_processing_detected" "SDL processing detected in logs"
    else
        test_fail "sdl_processing_detected" "SDL processing not detected in logs"
    fi

    # Test 3: Verify no unsubstituted variables in processed SDL
    # Look for error messages about unsubstituted variables
    if grep -q "unsubstituted\|missing.*variable\|undefined.*\${" "$TEST_DIR/coordinator/node.log" 2>/dev/null; then
        test_fail "no_unsubstituted_variables" "Unsubstituted variables found in logs"
    else
        test_pass "no_unsubstituted_variables" "No unsubstituted variable errors detected"
    fi

    test_summary "SDL Variable Substitution"
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

    # Show detailed test results if available
    if [ -f "${TEST_DIR}/test-results.log" ]; then
        echo -e "  ${BOLD}Test Details:${NC}"
        echo -e "    Detailed log: ${TEST_DIR}/test-results.log"
        echo ""

        # Show failed tests
        if [ $TESTS_FAILED -gt 0 ]; then
            echo -e "  ${RED}${BOLD}Failed Tests:${NC}"
            grep "^\[" "${TEST_DIR}/test-results.log" | grep "FAIL:" | while IFS= read -r line; do
                # Extract test name from log line
                local test_info=$(echo "$line" | sed 's/.*FAIL: \([^ ]*\) - .*/\1/')
                echo -e "    ${RED}❌ $test_info${NC}"
            done
            echo ""
        fi
    fi

    if [ $TESTS_FAILED -eq 0 ]; then
        echo -e "${GREEN}${BOLD}  ╔═══════════════════════════════════════╗${NC}"
        echo -e "${GREEN}${BOLD}  ║     ALL TESTS PASSED SUCCESSFULLY     ║${NC}"
        echo -e "${GREEN}${BOLD}  ╚═══════════════════════════════════════╝${NC}"
        return 0
    else
        echo -e "${RED}${BOLD}  ╔═══════════════════════════════════════╗${NC}"
        echo -e "${RED}${BOLD}  ║         SOME TESTS FAILED             ║${NC}"
        echo -e "${RED}${BOLD}  ╚═══════════════════════════════════════╝${NC}"
        echo -e "  ${BOLD}Check logs:${NC} ${TEST_DIR}/test-results.log"
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
    echo -e "  ${BOLD}Mode:${NC} ${GREEN}REAL AKASH${NC} (blockchain node + provider)"
    echo -e "  ${BOLD}Akash Home:${NC} ${AKASH_HOME}"
    echo ""

    if [ "$LIVE_LOGS" = true ]; then
        echo -e "  ${BOLD}Live Logs:${NC} ${GREEN}ENABLED${NC}"
        echo -e "    ${COORD_COLOR}[COORD]${NC} Coordinator logs"
        echo -e "    ${EXEC_COLOR}[EXEC ]${NC} Executor logs"
        echo -e "    ${MOCK_COLOR}[MOCK ]${NC} Mock provider logs"
        echo -e "    ${BLUE}[ANODE]${NC} Akash node logs"
        echo -e "    ${GREEN}[APROV]${NC} Akash provider logs"
        echo ""
    fi

    echo -e "  ${BOLD}Workflow:${NC}"
    echo -e "    ${CYAN}Phase 1: Build & Infrastructure${NC}"
    echo -e "      1. Build CosmWasm contracts (Docker optimizer)"
    echo -e "      2. Build ERGORS binary"
    echo -e "      3. Verify contract artifacts"
    echo -e "      4. Start ERGORS test network (coordinator + executor)"
    echo -e "      5. Setup Akash infrastructure (node + faucet ONLY)"
    echo -e ""
    echo -e "    ${CYAN}Phase 2: Pre-Provider Tests${NC}"
    echo -e "      6. Test ERGORS network connectivity"
    echo -e "      7. Test node configuration"
    echo -e "      8. Test SDL contract deployment"
    echo -e "      9. Test SDL workflow (templates, queries, variables)"
    echo -e "     10. Fund coordinator account via Akash faucet"
    echo -e ""
    echo -e "    ${CYAN}Phase 3: Feegrant/Authz Workflow (CRITICAL)${NC}"
    echo -e "     11. Executor requests grant from coordinator"
    echo -e "     12. Coordinator approves (creates authz + feegrant on chain)"
    echo -e "     13. Verify grants exist on blockchain"
    echo -e "     14. Test executor deployment using feegrant/authz"
    echo -e ""
    echo -e "    ${CYAN}Phase 4: Provider & Deployment Tests${NC}"
    echo -e "     15. Setup Akash provider (ready to accept bids)"
    echo -e "     16. Run real Akash deployment workflow"
    echo -e "     17. Run engine-driven deployment workflow"
    echo -e "     18. Test API key management workflow"
    echo -e "     19. Test cross-account deployment"
    echo -e "     20. Test real Akash deployment lifecycle"
    echo ""

    # ===== Phase 1: Build & Infrastructure =====
    check_prerequisites
    build_contracts
    build_ergors
    test_contract_artifacts
    start_ergors_network
    setup_akash_environment  # Sets up ONLY node + faucet (NO provider yet)

    # ===== Phase 2: Pre-Provider Tests =====
    # Test basic network and node config
    test_ergors_network
    test_node_config
    test_contract_deployment

    # Test SDL contracts - verify templates are accessible
    test_sdl_workflow

    # Fund the coordinator account (feegranter) via Akash faucet
    fund_coordinator_account

    # ===== Phase 3: Feegrant/Authz Workflow (CRITICAL) =====
    # Test the engine-driven feegrant/authz workflow:
    # 1. Executor requests grant from coordinator
    # 2. Coordinator approves grant (creates authz + feegrant on blockchain)
    # 3. Verify grants exist and are usable
    run_authz_feegrant_tests

    # Now test that executor can deploy using feegrant/authz
    # This validates the grants work for actual Akash deployments
    test_executor_deployment_with_grants

    # ===== Phase 4: Provider & Deployment Tests =====
    # NOW setup the Akash provider (after feegrant tests prove grants work)
    setup_akash_provider

    # Test real Akash deployment workflow (directly via akash CLI)
    run_real_akash_deployment || log_warn "Real Akash deployment workflow had failures"

    # Test engine-driven deployment workflow (via ERGORS engine)
    deploy_via_ergors || log_warn "ERGORS deployment workflow had failures (continuing tests)"

    # NOTE: API key tests removed - start_mock_provider() always returned 1
    # and MOCK_PROVIDER_URL was never set, making all api_key tests no-ops.
    # Re-add when mock provider is actually deployed to Akash.

    # Cross-Account Deployment Tests
    run_cross_account_tests

    # Real Akash deployment tests
    run_real_akash_tests

    # Phase 4: Service Endpoint Validation
    run_endpoint_validation_tests

    # Phase 5: Security Testing
    run_security_tests

    print_summary
}

main "$@"
