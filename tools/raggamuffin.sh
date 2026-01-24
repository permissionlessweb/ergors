#!/bin/bash
#
# raggamuffin.sh - RAG Embedding Workflow Orchestrator
#
# Orchestrates the full RAG embedding generation pipeline:
#   1. Deploy embedding inference provider on Akash via ergors
#   2. Format target repository using githem
#   3. Generate embeddings using the deployed provider
#   4. Verify retrieval client connectivity
#
# Usage:
#   ./raggamuffin.sh <repo-url-or-path> [options]
#
# Examples:
#   ./raggamuffin.sh https://github.com/user/repo
#   ./raggamuffin.sh ~/projects/my-repo --model bge-large-en-v1.5 --gpu
#   ./raggamuffin.sh https://github.com/user/repo --local  # skip Akash, use local model
#   ./raggamuffin.sh https://github.com/user/repo --provider-url http://existing:8080  # use existing provider
#
# Requirements:
#   - ergors CLI (for Akash deployment)
#   - githem (for repo formatting)
#   - python3 with: requests, numpy, faiss-cpu, sentence-transformers (if --local)
#

set -euo pipefail

# ============================================================
# Configuration
# ============================================================

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PYTHON_DIR="${SCRIPT_DIR}/python"
SDL_DIR="${SCRIPT_DIR}/../sdls/embeddings"

# Defaults
MODEL="${EMBEDDING_MODEL:-all-MiniLM-L6-v2}"
OUTPUT_DIR="./rag-store"
CHUNK_SIZE=500
BATCH_SIZE=64
USE_LOCAL=false
USE_GPU=false
PROVIDER_URL=""
DEPLOY_NAME="embedding-provider"
SKIP_DEPLOY=false
SKIP_FORMAT=false

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# ============================================================
# Helpers
# ============================================================

log_step() { echo -e "${BLUE}[step]${NC} $1"; }
log_ok()   { echo -e "${GREEN}[ok]${NC} $1"; }
log_warn() { echo -e "${YELLOW}[warn]${NC} $1"; }
log_err()  { echo -e "${RED}[error]${NC} $1" >&2; }

usage() {
    cat <<EOF
Usage: $(basename "$0") <repo-url-or-path> [options]

Arguments:
  repo-url-or-path    GitHub URL or local path to the target repository

Options:
  --model MODEL       Embedding model name (default: all-MiniLM-L6-v2)
  --output DIR        Output directory for embeddings (default: ./rag-store)
  --chunk-size N      Chunk size in characters (default: 500)
  --batch-size N      Batch size for embedding requests (default: 64)
  --gpu               Use GPU-accelerated SDL for deployment
  --local             Use local SentenceTransformer (skip Akash deployment)
  --provider-url URL  Use existing provider (skip deployment)
  --deploy-name NAME  Deployment name on Akash (default: embedding-provider)
  --skip-format       Skip githem formatting (use repo directly)
  --help              Show this help message

Environment Variables:
  EMBEDDING_MODEL     Override default model
  ERGORS_HOST         Ergors daemon address (default: localhost:50051)
  GITHEM_PRESET       Githem preset (default: standard)
EOF
    exit 0
}

check_command() {
    if ! command -v "$1" &>/dev/null; then
        log_err "Required command not found: $1"
        return 1
    fi
}

wait_for_provider() {
    local url="$1"
    local max_attempts=60
    local attempt=0

    log_step "Waiting for provider to become healthy..."
    while [ $attempt -lt $max_attempts ]; do
        if curl -sf "${url}/health" >/dev/null 2>&1; then
            log_ok "Provider is healthy"
            return 0
        fi
        attempt=$((attempt + 1))
        sleep 5
    done

    log_err "Provider failed to become healthy after ${max_attempts} attempts"
    return 1
}

# ============================================================
# Parse Arguments
# ============================================================

REPO_TARGET=""

while [[ $# -gt 0 ]]; do
    case $1 in
        --model)       MODEL="$2"; shift 2 ;;
        --output)      OUTPUT_DIR="$2"; shift 2 ;;
        --chunk-size)  CHUNK_SIZE="$2"; shift 2 ;;
        --batch-size)  BATCH_SIZE="$2"; shift 2 ;;
        --gpu)         USE_GPU=true; shift ;;
        --local)       USE_LOCAL=true; SKIP_DEPLOY=true; shift ;;
        --provider-url) PROVIDER_URL="$2"; SKIP_DEPLOY=true; shift 2 ;;
        --deploy-name) DEPLOY_NAME="$2"; shift 2 ;;
        --skip-format) SKIP_FORMAT=true; shift ;;
        --help|-h)     usage ;;
        -*)            log_err "Unknown option: $1"; usage ;;
        *)             REPO_TARGET="$1"; shift ;;
    esac
done

if [ -z "$REPO_TARGET" ]; then
    log_err "Repository URL or path is required"
    usage
fi

# ============================================================
# Preflight Checks
# ============================================================

echo ""
echo "========================================="
echo "  raggamuffin - RAG Embedding Pipeline"
echo "========================================="
echo ""
echo "  Target:  ${REPO_TARGET}"
echo "  Model:   ${MODEL}"
echo "  Output:  ${OUTPUT_DIR}"
echo "  Mode:    $([ "$USE_LOCAL" = true ] && echo "local" || echo "akash")"
echo ""

# Check required tools
check_command python3
if [ "$SKIP_DEPLOY" = false ]; then
    check_command ergors
fi
if [ "$SKIP_FORMAT" = false ] && echo "$REPO_TARGET" | grep -q "^http"; then
    check_command githem || log_warn "githem not found - will attempt to use repo directly"
fi

# ============================================================
# Step 1: Deploy Embedding Provider on Akash
# ============================================================

if [ "$SKIP_DEPLOY" = false ]; then
    log_step "Step 1: Deploying embedding provider on Akash..."

    # Select SDL based on GPU flag
    if [ "$USE_GPU" = true ]; then
        SDL_FILE="${SDL_DIR}/embedding-provider-gpu.yml"
    else
        SDL_FILE="${SDL_DIR}/embedding-provider.yml"
    fi

    if [ ! -f "$SDL_FILE" ]; then
        log_err "SDL file not found: ${SDL_FILE}"
        exit 1
    fi

    # Deploy via ergors
    DEPLOY_OUTPUT=$(ergors deploy create \
        --sdl "$SDL_FILE" \
        --var "EMBEDDING_MODEL=${MODEL}" \
        --var "MAX_BATCH_SIZE=${BATCH_SIZE}" \
        --name "$DEPLOY_NAME" \
        2>&1) || {
        log_err "Deployment failed: ${DEPLOY_OUTPUT}"
        exit 1
    }

    log_ok "Deployment initiated: ${DEPLOY_NAME}"

    # Wait for deployment to complete and get endpoint
    log_step "Advancing deployment workflow..."
    ergors deploy advance --name "$DEPLOY_NAME" --wait || {
        log_err "Deployment workflow failed"
        exit 1
    }

    # Get the provider endpoint
    PROVIDER_URL=$(ergors deploy get --name "$DEPLOY_NAME" --format json | python3 -c "
import sys, json
data = json.load(sys.stdin)
endpoints = data.get('endpoints', [])
for ep in endpoints:
    if ep.get('port') == 8080:
        print(f\"http://{ep['host']}:{ep['port']}\")
        break
else:
    if endpoints:
        ep = endpoints[0]
        print(f\"http://{ep['host']}:{ep.get('port', 8080)}\")
")

    if [ -z "$PROVIDER_URL" ]; then
        log_err "Could not extract provider endpoint from deployment"
        exit 1
    fi

    log_ok "Provider endpoint: ${PROVIDER_URL}"

    # Wait for provider health
    wait_for_provider "$PROVIDER_URL"
else
    if [ "$USE_LOCAL" = true ]; then
        log_step "Step 1: Skipped (using local model)"
    else
        log_step "Step 1: Skipped (using existing provider: ${PROVIDER_URL})"
        wait_for_provider "$PROVIDER_URL"
    fi
fi

# ============================================================
# Step 2: Format Repository
# ============================================================

FORMATTED_FILE=""
INPUT_ARG=""

if [ "$SKIP_FORMAT" = true ]; then
    log_step "Step 2: Skipped formatting (using repo directly)"
    INPUT_ARG="--repo ${REPO_TARGET}"
else
    log_step "Step 2: Formatting repository..."

    # Determine if URL or local path
    if echo "$REPO_TARGET" | grep -q "^http"; then
        # Use githem for remote repos
        if command -v githem &>/dev/null; then
            FORMATTED_FILE="${OUTPUT_DIR}/formatted_repo.txt"
            mkdir -p "$OUTPUT_DIR"

            GITHEM_PRESET="${GITHEM_PRESET:-standard}"
            githem "$REPO_TARGET" --preset "$GITHEM_PRESET" --output "$FORMATTED_FILE" || {
                log_warn "githem failed, cloning repo instead..."
                CLONE_DIR=$(mktemp -d)
                git clone --depth 1 "$REPO_TARGET" "$CLONE_DIR" 2>/dev/null
                INPUT_ARG="--repo ${CLONE_DIR}"
                FORMATTED_FILE=""
            }

            if [ -n "$FORMATTED_FILE" ] && [ -f "$FORMATTED_FILE" ]; then
                INPUT_ARG="--input ${FORMATTED_FILE}"
                log_ok "Formatted: ${FORMATTED_FILE} ($(wc -c < "$FORMATTED_FILE" | tr -d ' ') bytes)"
            fi
        else
            # Fallback: clone and use directly
            log_warn "githem not available, cloning repo..."
            CLONE_DIR=$(mktemp -d)
            git clone --depth 1 "$REPO_TARGET" "$CLONE_DIR" 2>/dev/null
            INPUT_ARG="--repo ${CLONE_DIR}"
            log_ok "Cloned to: ${CLONE_DIR}"
        fi
    else
        # Local path - use directly
        if [ -d "$REPO_TARGET" ]; then
            INPUT_ARG="--repo ${REPO_TARGET}"
            log_ok "Using local repo: ${REPO_TARGET}"
        elif [ -f "$REPO_TARGET" ]; then
            INPUT_ARG="--input ${REPO_TARGET}"
            log_ok "Using input file: ${REPO_TARGET}"
        else
            log_err "Target not found: ${REPO_TARGET}"
            exit 1
        fi
    fi
fi

# ============================================================
# Step 3: Generate Embeddings
# ============================================================

log_step "Step 3: Generating embeddings..."

PROVIDER_ARG=""
if [ "$USE_LOCAL" = true ]; then
    PROVIDER_ARG="--local"
else
    PROVIDER_ARG="--provider-url ${PROVIDER_URL}"
fi

python3 "${PYTHON_DIR}/rag-embedding.py" \
    ${INPUT_ARG} \
    ${PROVIDER_ARG} \
    --output "$OUTPUT_DIR" \
    --model "$MODEL" \
    --chunk-size "$CHUNK_SIZE" \
    --batch-size "$BATCH_SIZE" || {
    log_err "Embedding generation failed"
    exit 1
}

log_ok "Embeddings generated in: ${OUTPUT_DIR}"

# ============================================================
# Step 4: Verify Retrieval Client
# ============================================================

log_step "Step 4: Verifying retrieval client..."

# Check that store files exist
if [ ! -f "${OUTPUT_DIR}/index.faiss" ] || [ ! -f "${OUTPUT_DIR}/chunks.pkl" ]; then
    log_err "Store files missing from ${OUTPUT_DIR}"
    exit 1
fi

# Display manifest
if [ -f "${OUTPUT_DIR}/manifest.json" ]; then
    echo ""
    echo "  Store Manifest:"
    python3 -c "
import json
with open('${OUTPUT_DIR}/manifest.json') as f:
    m = json.load(f)
print(f\"    Vectors:   {m['count']}\")
print(f\"    Dimension: {m['dimension']}\")
print(f\"    Index:     {m['index_file']}\")
print(f\"    Chunks:    {m['chunks_file']}\")
"
fi

log_ok "Retrieval client ready"

# ============================================================
# Summary
# ============================================================

echo ""
echo "========================================="
echo "  Pipeline Complete"
echo "========================================="
echo ""
echo "  Store:    ${OUTPUT_DIR}"
if [ "$USE_LOCAL" = false ] && [ -n "$PROVIDER_URL" ]; then
echo "  Provider: ${PROVIDER_URL}"
fi
echo ""
echo "  To query embeddings:"
if [ "$USE_LOCAL" = true ]; then
echo "    python3 ${PYTHON_DIR}/rag-retriever.py --store ${OUTPUT_DIR} --local --query \"your question\""
else
echo "    python3 ${PYTHON_DIR}/rag-retriever.py --store ${OUTPUT_DIR} --provider-url ${PROVIDER_URL} --query \"your question\""
fi
echo ""
echo "  Interactive mode:"
if [ "$USE_LOCAL" = true ]; then
echo "    python3 ${PYTHON_DIR}/rag-retriever.py --store ${OUTPUT_DIR} --local"
else
echo "    python3 ${PYTHON_DIR}/rag-retriever.py --store ${OUTPUT_DIR} --provider-url ${PROVIDER_URL}"
fi
echo ""
