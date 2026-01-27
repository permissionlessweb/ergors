#!/bin/bash
#
# akash.sh - Akash infrastructure management for E2E tests
#
# Provides: Akash dev environment setup, node, provider, faucet, Kind cluster

# Prevent multiple sourcing
[[ -n "${_E2E_AKASH_LOADED:-}" ]] && return 0
_E2E_AKASH_LOADED=1

# =============================================================================
# Configuration
# =============================================================================
AKASH_HOME="${AKASH_HOME:-${HOME}/go/src/github.com/akash-network}"
AKASH_PROVIDER_DIR="${AKASH_HOME}/provider"
AKASH_KUBE_DIR="${AKASH_PROVIDER_DIR}/_run/kube"
AKASH_LOCAL_NODE="${AKASH_LOCAL_NODE:-http://localhost:26657}"
AKASH_LOCAL_CHAIN_ID="${AKASH_LOCAL_CHAIN_ID:-local}"

# Timeouts
KUBE_ROLLOUT_TIMEOUT="${KUBE_ROLLOUT_TIMEOUT:-30000}"
NODE_READY_TIMEOUT="${NODE_READY_TIMEOUT:-60}"
PROVIDER_READY_TIMEOUT="${PROVIDER_READY_TIMEOUT:-60}"

# Process tracking
AKASH_NODE_PID=""
AKASH_PROVIDER_PID=""

# GNU Make command (set by akash_init)
MAKE_CMD="make"

# goreleaser-cross image - v1.22 has goreleaser 1.x which supports --id flag
# (latest has goreleaser 2.x which removed --id, breaking Akash's Makefile)
GORELEASER_IMAGE="${GORELEASER_IMAGE:-ghcr.io/goreleaser/goreleaser-cross:v1.22}"

# =============================================================================
# Initialization
# =============================================================================

# Initialize Akash environment (call once at start)
akash_init() {
    # On macOS, ensure GNU Make is in PATH for recursive make calls
    # Akash Makefiles internally call `make`, so PATH must resolve to GNU make 4+
    if [[ "$(uname)" == "Darwin" ]]; then
        # Homebrew on Apple Silicon
        if [[ -d "/opt/homebrew/opt/make/libexec/gnubin" ]]; then
            export PATH="/opt/homebrew/opt/make/libexec/gnubin:$PATH"
            MAKE_CMD="make"  # Now resolves to GNU make via PATH
        # Homebrew on Intel Mac
        elif [[ -d "/usr/local/opt/make/libexec/gnubin" ]]; then
            export PATH="/usr/local/opt/make/libexec/gnubin:$PATH"
            MAKE_CMD="make"
        # Fallback to gmake command if gnubin not available
        elif command -v gmake &>/dev/null; then
            MAKE_CMD="gmake"
            log_warn "Using gmake but recursive make calls may still fail - install GNU make via 'brew install make'"
        else
            log_error "GNU Make 4+ required. Install with: brew install make"
            return 1
        fi
    fi

    # Load environment
    _load_akash_env
}

_load_akash_env() {
    export AP_ROOT="${AKASH_PROVIDER_DIR}"
    export AKASH_DIRENV_SET=1

    # Try direnv first, but preserve our ROOT_DIR
    local saved_root_dir="${ROOT_DIR:-}"
    local direnv_loaded=false
    if command -v direnv >/dev/null 2>&1; then
        (cd "${AKASH_PROVIDER_DIR}" && direnv allow . >/dev/null 2>&1) || true
        if eval "$(cd "${AKASH_PROVIDER_DIR}" && direnv export bash 2>/dev/null)"; then
            direnv_loaded=true
        fi
    fi
    # Restore ROOT_DIR if it was set (Akash's direnv may override it)
    if [[ -n "$saved_root_dir" ]]; then
        export ROOT_DIR="$saved_root_dir"
    fi

    # Fallback to manual .env only if direnv failed
    if [[ "$direnv_loaded" == false ]] && [[ -f "${AKASH_PROVIDER_DIR}/.env" ]]; then
        while IFS='=' read -r key value || [[ -n "$key" ]]; do
            [[ "$key" =~ ^#.*$ || -z "$key" ]] && continue
            [[ "$key" == "ROOT_DIR" ]] && continue
            value="${value//\$\{AP_ROOT\}/$AP_ROOT}"
            value="${value//\$AP_ROOT/$AP_ROOT}"
            export "$key=$value"
        done < "${AKASH_PROVIDER_DIR}/.env"
    fi

    # Set fallback paths
    export AP_RUN_NAME="kube"
    export AP_DEVCACHE="${AP_DEVCACHE:-${AP_ROOT}/.cache}"
    export AP_DEVCACHE_BIN="${AP_DEVCACHE_BIN:-${AP_DEVCACHE}/bin}"
    export DEVCACHE_RUN="${AP_DEVCACHE_BASE:-${AP_ROOT}/.cache}/run"
    export AP_RUN_DIR="${DEVCACHE_RUN}/${AP_RUN_NAME}"
    export AKASH="${AP_DEVCACHE_BIN}/akash"
    export PROVIDER_SERVICES="${AP_DEVCACHE_BIN}/provider-services"
    export GOTOOLCHAIN=local
    export GORELEASER_IMAGE

    # Create required directories
    mkdir -p "$AP_RUN_DIR" "$AP_DEVCACHE_BIN" "$DEVCACHE_RUN" 2>/dev/null || true
}

# =============================================================================
# Make Wrapper
# =============================================================================

# Run make command in Akash kube directory with proper environment
# Passes GORELEASER_IMAGE as command-line arg to override := assignment in Makefile
akash_make() {
    local target="$1"
    shift

    cd "${AKASH_KUBE_DIR}" || return 1
    direnv allow . >/dev/null 2>&1 || true
    direnv exec . $MAKE_CMD "$target" "GORELEASER_IMAGE=${GORELEASER_IMAGE}" "$@"
}

# =============================================================================
# Repository Setup
# =============================================================================

akash_setup_repos() {
    log_step "Setting Up Akash Repositories"

    mkdir -p "${AKASH_HOME}"

    if [[ ! -d "${AKASH_PROVIDER_DIR}" ]]; then
        log "Cloning provider repository..."
        cd "${AKASH_HOME}"
        git clone -b feat/local-dev https://github.com/permissionlessweb/provider.git
        log_success "Provider repo cloned"
    else
        log "Provider repo exists at ${AKASH_PROVIDER_DIR}"
    fi

    if [[ ! -d "${AKASH_KUBE_DIR}" ]]; then
        log_error "Kube directory not found: ${AKASH_KUBE_DIR}"
        return 1
    fi

    cd "${AKASH_PROVIDER_DIR}"
    direnv allow . >/dev/null 2>&1 || true
    log_success "Akash repos ready"
}

# =============================================================================
# Binary Building
# =============================================================================

akash_build_binaries() {
    log_step "Building Akash Binaries"

    local akash_bin="${AKASH_PROVIDER_DIR}/.cache/bin/akash"
    local provider_bin="${AKASH_PROVIDER_DIR}/.cache/bin/provider-services"

    if [[ -f "$akash_bin" ]] && [[ -f "$provider_bin" ]]; then
        log_success "Akash binaries already exist"
        return 0
    fi

    log "Building binaries (this may take several minutes)..."
    log_verbose "MAKE_CMD=$MAKE_CMD GORELEASER_IMAGE=$GORELEASER_IMAGE"

    cd "$AKASH_PROVIDER_DIR" || return 1
    direnv allow . >/dev/null 2>&1 || true

    local build_log="${TEST_DIR:-/tmp}/akash-build.log"
    if ! run_cmd "$build_log" direnv exec . $MAKE_CMD bins "GORELEASER_IMAGE=${GORELEASER_IMAGE}"; then
        log_error "Build failed. See: $build_log"
        tail -30 "$build_log"
        return 1
    fi
    log_verbose "Build log: $build_log"

    if [[ ! -f "$akash_bin" ]] || [[ ! -f "$provider_bin" ]]; then
        log_error "Binaries not created"
        return 1
    fi

    log_success "Akash binaries built"
}

# =============================================================================
# Docker Image
# =============================================================================

# akash_build_docker_image() {
#     log_step "Building Akash Docker Image"

#     log "Building local Docker image with provider + faucet..."
#     log_verbose "This builds provider-services and faucet binaries into the image"

#     local image_log="${TEST_DIR:-/tmp}/docker-image-build.log"

#     if [[ "${VERBOSE:-false}" == "true" ]]; then
#         if ! akash_make docker-image-local 2>&1 | tee "$image_log"; then
#             log_error "Docker image build failed. See: $image_log"
#             return 1
#         fi
#     else
#         if ! akash_make docker-image-local > "$image_log" 2>&1; then
#             log_error "Docker image build failed. See: $image_log"
#             tail -30 "$image_log"
#             return 1
#         fi
#     fi

#     log_success "Docker image built"
# }

# =============================================================================
# Kind Cluster
# =============================================================================

akash_setup_cluster() {
    log_step "Setting Up Kind Cluster"

    # Delete existing cluster if present
    if kind get clusters 2>/dev/null | grep -q "^kind$"; then
        log "Deleting existing cluster..."
        akash_make kube-cluster-delete 2>/dev/null || kind delete cluster --name kind
    fi

    # Clean before setup
    log "Running make clean..."
    akash_make clean 2>/dev/null || true

    export KUBE_ROLLOUT_TIMEOUT="${KUBE_ROLLOUT_TIMEOUT}"

    log "Creating Kind cluster with Akash components..."
    log_verbose "KUBE_ROLLOUT_TIMEOUT=$KUBE_ROLLOUT_TIMEOUT"
    local cluster_log="${TEST_DIR:-/tmp}/cluster-setup.log"

    if [[ "${VERBOSE:-false}" == "true" ]]; then
        if ! akash_make kube-cluster-setup 2>&1 | tee "$cluster_log"; then
            log_error "Cluster setup failed. See: $cluster_log"
            return 1
        fi
    else
        if ! akash_make kube-cluster-setup > "$cluster_log" 2>&1; then
            log_error "Cluster setup failed. See: $cluster_log"
            tail -30 "$cluster_log"
            return 1
        fi
    fi

    if ! kubectl cluster-info &>/dev/null; then
        log_error "Cluster not accessible"
        return 1
    fi

    log_success "Kind cluster ready"
    kubectl get nodes
}

# =============================================================================
# Blockchain Node
# =============================================================================

akash_start_node() {
    log_step "Starting Akash Node"

    local node_log="${TEST_DIR:-/tmp}/akash-node.log"

    # Kill any leftover processes on node ports
    for port in 26657 26656 9090 1317; do
        local pid
        pid=$(lsof -ti ":$port" 2>/dev/null || true)
        [[ -n "$pid" ]] && kill "$pid" 2>/dev/null || true
    done

    log "Starting node..."
    akash_make node-run > "$node_log" 2>&1 &
    AKASH_NODE_PID=$!
    register_pid $AKASH_NODE_PID

    # Wait for node to be ready (check RPC endpoint)
    if wait_for_port "127.0.0.1" 26657 "$NODE_READY_TIMEOUT"; then
        log_success "Akash node ready (PID: $AKASH_NODE_PID)"
        return 0
    else
        log_error "Node failed to start"
        [[ -f "$node_log" ]] && tail -30 "$node_log"
        return 1
    fi
}

akash_stop_node() {
    if [[ -n "$AKASH_NODE_PID" ]]; then
        log "Stopping Akash node (PID: $AKASH_NODE_PID)..."
        kill_with_timeout "$AKASH_NODE_PID" 5
        AKASH_NODE_PID=""
    fi

    # Clean up any orphaned akash node processes on standard ports
    local akash_node_ports=(26657 26656 9090 1317)
    for port in "${akash_node_ports[@]}"; do
        kill_port "$port" 2>/dev/null || true
    done
}

# Check if node is healthy via RPC
akash_node_healthy() {
    local status
    status=$(curl -s --max-time 2 "http://localhost:26657/status" 2>/dev/null || echo "{}")
    json_has "$status" '.result.sync_info'
}

# =============================================================================
# Provider
# =============================================================================

akash_create_provider() {
    log_step "Creating Akash Provider"

    log "Registering provider on blockchain..."
    if ! run_cmd_tail 10 akash_make provider-create; then
        log_error "Provider creation failed"
        return 1
    fi
    log_success "Provider created"
}

akash_start_provider() {
    log_step "Starting Akash Provider"

    local provider_log="${TEST_DIR:-/tmp}/akash-provider.log"

    # Kill any leftover processes
    for port in 8443 8444; do
        local pid
        pid=$(lsof -ti ":$port" 2>/dev/null || true)
        [[ -n "$pid" ]] && kill "$pid" 2>/dev/null || true
    done

    log "Starting provider..."
    akash_make provider-run > "$provider_log" 2>&1 &
    AKASH_PROVIDER_PID=$!
    register_pid $AKASH_PROVIDER_PID

    # Wait for provider to be ready (check gateway)
    if wait_for_port "127.0.0.1" 8443 "$PROVIDER_READY_TIMEOUT"; then
        log_success "Akash provider ready (PID: $AKASH_PROVIDER_PID)"
        return 0
    else
        log_error "Provider failed to start"
        [[ -f "$provider_log" ]] && tail -30 "$provider_log"
        return 1
    fi
}

akash_stop_provider() {
    if [[ -n "$AKASH_PROVIDER_PID" ]]; then
        log "Stopping Akash provider (PID: $AKASH_PROVIDER_PID)..."
        kill_with_timeout "$AKASH_PROVIDER_PID" 5
        AKASH_PROVIDER_PID=""
    fi

    # Clean up any orphaned provider processes on standard ports
    local provider_ports=(8443 8444)
    for port in "${provider_ports[@]}"; do
        kill_port "$port" 2>/dev/null || true
    done

    # Kill any provider-services processes that may have been spawned
    kill_by_pattern "provider-services" 2>/dev/null || true
}

# =============================================================================
# Faucet
# =============================================================================

# Get the faucet mnemonic from Akash dev environment
# This key is pre-funded with 10B AKT during genesis
# Key files are now JSON format in .akash/key-secrets/
akash_get_faucet_mnemonic() {
    local base_dir="${AP_RUN_DIR:-${AKASH_PROVIDER_DIR}/.cache/run/kube}"
    local key_dir="${base_dir}/.akash/key-secrets"
    local json_file="${key_dir}/faucet.json"

    log_verbose "Looking for faucet key at: $json_file"

    if [[ -f "$json_file" ]]; then
        local mnemonic
        mnemonic=$(jq -r '.mnemonic // .phrase // .seed // empty' "$json_file" 2>/dev/null)
        if [[ -n "$mnemonic" ]]; then
            echo "$mnemonic"
            return 0
        fi
        log_error "Could not parse mnemonic from JSON: $json_file"
        log_verbose "JSON content: $(cat "$json_file")"
        return 1
    fi

    log_error "Faucet key file not found: $json_file"
    return 1
}

# =============================================================================
# Full Setup Workflows
# =============================================================================

# Setup infrastructure only (node + faucet, no provider)
akash_setup_infrastructure() {
    log_step "Setting Up Akash Infrastructure"

    akash_init
    akash_setup_repos || return 1
    akash_build_binaries || return 1
    # akash_build_docker_image || return 1
    akash_setup_cluster || return 1
    akash_start_node || return 1

    log_success "Akash infrastructure ready"
}

# Setup provider (call after infrastructure)
akash_setup_provider() {
    log_step "Setting Up Akash Provider"

    akash_create_provider || return 1
    akash_start_provider || return 1

    log_success "Provider ready to accept bids"
}

# Full cleanup
akash_cleanup() {
    log "Cleaning up Akash environment..."

    # Stop provider and node with proper cleanup
    akash_stop_provider
    akash_stop_node

    # Kill any kubectl port-forward processes
    kill_by_pattern "kubectl.*port-forward" 2>/dev/null || true

    # Delete Kind cluster if it exists
    if [[ -d "${AKASH_KUBE_DIR}" ]]; then
        akash_make kube-cluster-delete 2>/dev/null || true
    fi

    # Fallback: delete kind cluster directly if make fails
    if kind get clusters 2>/dev/null | grep -q "^kind$"; then
        log "Deleting Kind cluster directly..."
        kind delete cluster --name kind 2>/dev/null || true
    fi

    log_success "Akash cleanup complete"
}
