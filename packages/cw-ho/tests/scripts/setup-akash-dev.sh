#!/bin/bash
#
# Akash Development Environment Setup Script
#
# This script sets up Akash's Kind-based development environment for
# integration testing of the ERGORS deployment workflow.
#
# Usage:
#   ./setup-akash-dev.sh [options]
#
# Options:
#   --cluster-name NAME    Kind cluster name (default: akash-dev)
#   --with-gpu             Setup GPU support
#   --skip-provider        Skip provider setup
#   --cleanup              Delete existing environment first
#   --help                 Show this help message

set -e

# Configuration
CLUSTER_NAME="${CLUSTER_NAME:-akash-dev}"
KUBE_ROLLOUT_TIMEOUT="${KUBE_ROLLOUT_TIMEOUT:-300}"
SKIP_PROVIDER="${SKIP_PROVIDER:-false}"
WITH_GPU="${WITH_GPU:-false}"
CLEANUP="${CLEANUP:-false}"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Logging functions
log_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

log_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Show help
show_help() {
    head -n 20 "$0" | tail -n 17 | sed 's/^#//'
    exit 0
}

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --cluster-name)
            CLUSTER_NAME="$2"
            shift 2
            ;;
        --with-gpu)
            WITH_GPU="true"
            shift
            ;;
        --skip-provider)
            SKIP_PROVIDER="true"
            shift
            ;;
        --cleanup)
            CLEANUP="true"
            shift
            ;;
        --help|-h)
            show_help
            ;;
        *)
            log_error "Unknown option: $1"
            show_help
            ;;
    esac
done

# Check prerequisites
check_prerequisites() {
    log_info "Checking prerequisites..."

    local missing=()

    # Check Docker
    if ! command -v docker &> /dev/null; then
        missing+=("docker")
    elif ! docker info &> /dev/null; then
        log_error "Docker is not running. Please start Docker Desktop or the Docker daemon."
        exit 1
    fi

    # Check Kind
    if ! command -v kind &> /dev/null; then
        missing+=("kind")
    fi

    # Check kubectl
    if ! command -v kubectl &> /dev/null; then
        missing+=("kubectl")
    fi

    # Check jq
    if ! command -v jq &> /dev/null; then
        missing+=("jq")
    fi

    if [ ${#missing[@]} -gt 0 ]; then
        log_error "Missing required tools: ${missing[*]}"
        log_info "Install with:"
        log_info "  brew install ${missing[*]} (macOS)"
        log_info "  apt-get install ${missing[*]} (Debian/Ubuntu)"
        exit 1
    fi

    log_success "All prerequisites satisfied"
}

# Cleanup existing environment
cleanup_environment() {
    log_info "Cleaning up existing environment..."

    # Delete Kind cluster if exists
    if kind get clusters 2>/dev/null | grep -q "^${CLUSTER_NAME}$"; then
        log_info "Deleting Kind cluster '${CLUSTER_NAME}'..."
        kind delete cluster --name "${CLUSTER_NAME}" || true
    fi

    log_success "Cleanup complete"
}

# Create Kind cluster configuration
create_kind_config() {
    local config_file="$1"

    cat > "${config_file}" <<EOF
kind: Cluster
apiVersion: kind.x-k8s.io/v1alpha4
nodes:
- role: control-plane
  image: kindest/node:v1.29.4
  kubeadmConfigPatches:
  - |
    kind: InitConfiguration
    nodeRegistration:
      kubeletExtraArgs:
        node-labels: "ingress-ready=true"
  extraPortMappings:
  # Ingress HTTP/HTTPS
  - containerPort: 80
    hostPort: 80
    protocol: TCP
  - containerPort: 443
    hostPort: 443
    protocol: TCP
  # Akash Node RPC
  - containerPort: 26657
    hostPort: 26657
    protocol: TCP
  # Akash Node REST
  - containerPort: 1317
    hostPort: 1317
    protocol: TCP
  # Akash Node gRPC
  - containerPort: 9090
    hostPort: 9090
    protocol: TCP
  # Akash Provider API
  - containerPort: 8443
    hostPort: 8443
    protocol: TCP
  # Mock inference provider
  - containerPort: 11434
    hostPort: 11434
    protocol: TCP
EOF
}

# Setup Kind cluster
setup_cluster() {
    log_info "Setting up Kind cluster '${CLUSTER_NAME}'..."

    # Check if cluster already exists
    if kind get clusters 2>/dev/null | grep -q "^${CLUSTER_NAME}$"; then
        log_warn "Kind cluster '${CLUSTER_NAME}' already exists"
        kubectl cluster-info --context "kind-${CLUSTER_NAME}" &>/dev/null || {
            log_error "Cluster exists but is not accessible. Run with --cleanup to recreate."
            exit 1
        }
        return 0
    fi

    # Create cluster config
    local config_file=$(mktemp)
    create_kind_config "${config_file}"

    # Create cluster
    log_info "Creating Kind cluster..."
    kind create cluster --name "${CLUSTER_NAME}" --config "${config_file}"
    rm -f "${config_file}"

    # Set kubectl context
    kubectl cluster-info --context "kind-${CLUSTER_NAME}"

    log_success "Kind cluster created"
}

# Install ingress controller
install_ingress() {
    log_info "Installing nginx ingress controller..."

    kubectl apply -f https://raw.githubusercontent.com/kubernetes/ingress-nginx/main/deploy/static/provider/kind/deploy.yaml

    log_info "Waiting for ingress controller to be ready..."
    kubectl wait --namespace ingress-nginx \
        --for=condition=ready pod \
        --selector=app.kubernetes.io/component=controller \
        --timeout="${KUBE_ROLLOUT_TIMEOUT}s" || {
        log_warn "Ingress controller timeout - continuing anyway"
    }

    log_success "Ingress controller installed"
}

# Create Akash namespace
setup_namespace() {
    log_info "Setting up Akash namespace..."

    kubectl create namespace akash-services --dry-run=client -o yaml | kubectl apply -f -

    log_success "Namespace created"
}

# Deploy Akash node
deploy_node() {
    log_info "Deploying Akash node..."

    cat <<EOF | kubectl apply -f -
apiVersion: apps/v1
kind: Deployment
metadata:
  name: akash-node
  namespace: akash-services
spec:
  replicas: 1
  selector:
    matchLabels:
      app: akash-node
  template:
    metadata:
      labels:
        app: akash-node
    spec:
      containers:
      - name: akash-node
        image: ghcr.io/akash-network/node:latest
        ports:
        - containerPort: 26657
          name: rpc
        - containerPort: 1317
          name: rest
        - containerPort: 9090
          name: grpc
        env:
        - name: AKASH_HOME
          value: /root/.akash
        - name: AKASH_CHAIN_ID
          value: localakash
        - name: AKASH_KEYRING_BACKEND
          value: test
        command: ["/bin/sh", "-c"]
        args:
        - |
          set -e
          if [ ! -f /root/.akash/config/genesis.json ]; then
            akash init test-node --chain-id localakash
            akash keys add validator --keyring-backend test
            akash keys add faucet --keyring-backend test
            akash keys add deployer --keyring-backend test
            akash keys add granter --keyring-backend test
            akash keys add grantee --keyring-backend test
            akash add-genesis-account \$(akash keys show validator -a --keyring-backend test) 100000000000000uakt
            akash add-genesis-account \$(akash keys show faucet -a --keyring-backend test) 100000000000000uakt
            akash add-genesis-account \$(akash keys show deployer -a --keyring-backend test) 100000000000000uakt
            akash add-genesis-account \$(akash keys show granter -a --keyring-backend test) 100000000000000uakt
            akash add-genesis-account \$(akash keys show grantee -a --keyring-backend test) 1000000uakt
            akash gentx validator 10000000000uakt --chain-id localakash --keyring-backend test
            akash collect-gentxs
          fi
          akash start --rpc.laddr tcp://0.0.0.0:26657 --api.enable --api.address tcp://0.0.0.0:1317 --grpc.address 0.0.0.0:9090
        volumeMounts:
        - name: akash-data
          mountPath: /root/.akash
        resources:
          requests:
            memory: "512Mi"
            cpu: "500m"
          limits:
            memory: "2Gi"
            cpu: "2"
      volumes:
      - name: akash-data
        emptyDir: {}
---
apiVersion: v1
kind: Service
metadata:
  name: akash-node
  namespace: akash-services
spec:
  type: NodePort
  selector:
    app: akash-node
  ports:
  - name: rpc
    port: 26657
    targetPort: 26657
    nodePort: 30657
  - name: rest
    port: 1317
    targetPort: 1317
    nodePort: 31317
  - name: grpc
    port: 9090
    targetPort: 9090
    nodePort: 30090
EOF

    log_info "Waiting for Akash node to be ready..."
    kubectl wait --namespace akash-services \
        --for=condition=available deployment/akash-node \
        --timeout="${KUBE_ROLLOUT_TIMEOUT}s" || {
        log_error "Akash node deployment failed"
        kubectl logs -n akash-services -l app=akash-node --tail=50
        exit 1
    }

    # Wait for node to be synced
    log_info "Waiting for node to sync..."
    for i in {1..60}; do
        if curl -s http://localhost:26657/status 2>/dev/null | jq -e '.result.sync_info.catching_up == false' &>/dev/null; then
            log_success "Akash node is ready and synced"
            return 0
        fi
        sleep 2
    done

    log_warn "Node sync check timed out - continuing anyway"
    log_success "Akash node deployed"
}

# Deploy Akash provider
deploy_provider() {
    if [ "$SKIP_PROVIDER" = "true" ]; then
        log_info "Skipping provider deployment (--skip-provider)"
        return 0
    fi

    log_info "Deploying Akash provider..."

    cat <<EOF | kubectl apply -f -
apiVersion: apps/v1
kind: Deployment
metadata:
  name: akash-provider
  namespace: akash-services
spec:
  replicas: 1
  selector:
    matchLabels:
      app: akash-provider
  template:
    metadata:
      labels:
        app: akash-provider
    spec:
      containers:
      - name: akash-provider
        image: ghcr.io/akash-network/provider:latest
        ports:
        - containerPort: 8443
          name: api
        env:
        - name: AKASH_NODE
          value: http://akash-node:26657
        - name: AKASH_CHAIN_ID
          value: localakash
        - name: AKASH_KEYRING_BACKEND
          value: test
        - name: AKASH_FROM
          value: provider
        - name: AKASH_HOME
          value: /root/.akash
        volumeMounts:
        - name: provider-data
          mountPath: /root/.akash
        resources:
          requests:
            memory: "256Mi"
            cpu: "250m"
          limits:
            memory: "1Gi"
            cpu: "1"
      volumes:
      - name: provider-data
        emptyDir: {}
---
apiVersion: v1
kind: Service
metadata:
  name: akash-provider
  namespace: akash-services
spec:
  type: NodePort
  selector:
    app: akash-provider
  ports:
  - name: api
    port: 8443
    targetPort: 8443
    nodePort: 30443
EOF

    log_info "Waiting for Akash provider to be ready..."
    kubectl wait --namespace akash-services \
        --for=condition=available deployment/akash-provider \
        --timeout="${KUBE_ROLLOUT_TIMEOUT}s" || {
        log_warn "Provider deployment timeout - may need more time"
    }

    log_success "Akash provider deployed"
}

# Print environment info
print_info() {
    echo ""
    log_success "============================================"
    log_success "  Akash Development Environment Ready!"
    log_success "============================================"
    echo ""
    echo "Endpoints:"
    echo "  Node RPC:      http://localhost:26657"
    echo "  Node REST:     http://localhost:1317"
    echo "  Node gRPC:     localhost:9090"
    echo "  Provider API:  https://localhost:8443"
    echo ""
    echo "Kubernetes:"
    echo "  Cluster:       ${CLUSTER_NAME}"
    echo "  Context:       kind-${CLUSTER_NAME}"
    echo "  Namespace:     akash-services"
    echo ""
    echo "Test Accounts:"
    echo "  validator, faucet, deployer, granter, grantee"
    echo ""
    echo "Commands:"
    echo "  kubectl get pods -n akash-services"
    echo "  kubectl logs -n akash-services -l app=akash-node"
    echo "  kind delete cluster --name ${CLUSTER_NAME}"
    echo ""
    echo "Run integration tests:"
    echo "  cargo test -p ergors --features testing -- --nocapture"
    echo ""
}

# Main execution
main() {
    echo ""
    log_info "============================================"
    log_info "  Akash Development Environment Setup"
    log_info "============================================"
    echo ""

    check_prerequisites

    if [ "$CLEANUP" = "true" ]; then
        cleanup_environment
    fi

    setup_cluster
    install_ingress
    setup_namespace
    deploy_node
    deploy_provider
    print_info
}

# Run main
main "$@"
