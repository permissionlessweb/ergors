#!/bin/bash
#
# ERGORS End-to-End Integration Test Runner
#
# Usage:
#   ./scripts/e2e/main.sh [options]
#
# Options:
#   --skip-build       Skip building ergors binary
#   --skip-contracts   Skip building CosmWasm contracts
#   --skip-network     Skip ERGORS network setup (use existing)
#   --skip-akash       Skip Akash/Kind setup (use existing)
#   --skip-cleanup     Keep everything running after tests
#   --verbose          Enable verbose output
#   --test SUITE       Run only specific test suite (network|grants|deployment|all)
#   --akash-home PATH  Set Akash repo location
#   --help             Show this help message
#

set -eu

# =============================================================================
# Script Setup
# =============================================================================
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/../.." && pwd)"
TEST_DIR="${TMPDIR:-/tmp}/ergors-e2e-test"

# Export for child scripts
export ROOT_DIR TEST_DIR

# =============================================================================
# Source Libraries
# =============================================================================
# shellcheck source=lib/common.sh
source "${SCRIPT_DIR}/lib/common.sh"
# shellcheck source=lib/akash.sh
source "${SCRIPT_DIR}/lib/akash.sh"
# shellcheck source=lib/ergors.sh
source "${SCRIPT_DIR}/lib/ergors.sh"

# Source test suites
# shellcheck source=tests/network.sh
source "${SCRIPT_DIR}/tests/network.sh"
# shellcheck source=tests/grants.sh
source "${SCRIPT_DIR}/tests/grants.sh"
# shellcheck source=tests/deployment.sh
source "${SCRIPT_DIR}/tests/deployment.sh"

# =============================================================================
# Configuration
# =============================================================================
SKIP_BUILD=false
SKIP_CONTRACTS=false
SKIP_NETWORK=false
SKIP_AKASH=false
SKIP_CLEANUP=false
VERBOSE=false
TEST_SUITE="all"

START_TIME=$(date +%s)

# =============================================================================
# Argument Parsing
# =============================================================================
print_help() {
    head -n 20 "$0" | tail -n 18 | sed 's/^#//'
}

while [[ $# -gt 0 ]]; do
    case $1 in
        --skip-build) SKIP_BUILD=true; shift ;;
        --skip-contracts) SKIP_CONTRACTS=true; shift ;;
        --skip-network) SKIP_NETWORK=true; shift ;;
        --skip-akash) SKIP_AKASH=true; shift ;;
        --skip-cleanup) SKIP_CLEANUP=true; shift ;;
        --verbose) VERBOSE=true; shift ;;
        --test) TEST_SUITE="$2"; shift 2 ;;
        --akash-home) AKASH_HOME="$2"; shift 2 ;;
        --help|-h) print_help; exit 0 ;;
        *) log_error "Unknown option: $1"; exit 1 ;;
    esac
done

export VERBOSE

# =============================================================================
# Prerequisites Check
# =============================================================================
check_prerequisites() {
    log_step "Checking Prerequisites"

    local missing=()

    command -v docker &>/dev/null || missing+=("docker")
    command -v kind &>/dev/null || missing+=("kind")
    command -v kubectl &>/dev/null || missing+=("kubectl")
    command -v cargo &>/dev/null || missing+=("cargo")
    command -v go &>/dev/null || missing+=("go")
    command -v jq &>/dev/null || missing+=("jq")
    command -v direnv &>/dev/null || missing+=("direnv")

    # Check GNU Make on macOS
    if [[ "$(uname)" == "Darwin" ]]; then
        if ! command -v gmake &>/dev/null; then
            if ! command -v make &>/dev/null || ! make --version 2>/dev/null | grep -q "GNU Make [4-9]"; then
                missing+=("gmake (GNU Make 4+)")
            fi
        fi
    fi

    if [[ ${#missing[@]} -gt 0 ]]; then
        log_error "Missing required tools: ${missing[*]}"
        exit 1
    fi

    if ! docker info &>/dev/null; then
        log_error "Docker is not running"
        exit 1
    fi

    log_success "All prerequisites satisfied"
}

# =============================================================================
# Cleanup
# =============================================================================
cleanup() {
    local exit_code=$?

    if [[ "$SKIP_CLEANUP" == true ]]; then
        log_warn "Skipping cleanup (--skip-cleanup)"
        log_warn "Test dir: ${TEST_DIR}"
        log_warn "ERGORS PIDs: ${ERGORS_NODE_PIDS[*]:-none}"
        log_warn "Akash Node PID: ${AKASH_NODE_PID:-none}"
        log_warn "Akash Provider PID: ${AKASH_PROVIDER_PID:-none}"
        return
    fi

    log_step "Cleanup"

    # Stop ERGORS network first (specific cleanup)
    ergors_stop_network

    # Stop Akash infrastructure (specific cleanup)
    akash_cleanup

    # Comprehensive cleanup of any remaining processes
    cleanup_all_processes

    # Remove test directory
    if [[ -d "$TEST_DIR" ]]; then
        log "Removing test directory..."
        rm -rf "$TEST_DIR"
    fi

    # Final verification - check if any known test ports are still in use
    local leftover_ports=()
    for port in 50100 50101 50110 50111 26657 9090 8443; do
        if lsof -ti ":$port" &>/dev/null; then
            leftover_ports+=("$port")
        fi
    done

    if [[ ${#leftover_ports[@]} -gt 0 ]]; then
        log_warn "Warning: Some ports still in use after cleanup: ${leftover_ports[*]}"
        log_warn "Attempting forceful cleanup..."
        for port in "${leftover_ports[@]}"; do
            kill_port "$port"
        done
    fi

    log_success "Cleanup complete"

    # Preserve original exit code
    return $exit_code
}

# Trap both EXIT and common error signals for comprehensive cleanup
trap cleanup EXIT
trap 'cleanup; exit 130' INT
trap 'cleanup; exit 143' TERM

# =============================================================================
# Build Phase
# =============================================================================
run_build_phase() {
    log_step "Build Phase"

    # Build contracts
    if [[ "$SKIP_BUILD" == true ]] || [[ "$SKIP_CONTRACTS" == true ]]; then
        log_warn "Skipping contract build"
    else
        ergors_build_contracts || {
            log_error "Contract build failed"
            exit 1
        }
    fi

    # Build ERGORS
    if [[ "$SKIP_BUILD" == true ]]; then
        log_warn "Skipping ERGORS build"
    else
        ergors_build || {
            log_error "ERGORS build failed"
            exit 1
        }
    fi

    log_success "Build phase complete"
}

# =============================================================================
# Infrastructure Setup Phase
# =============================================================================
run_infrastructure_phase() {
    log_step "Infrastructure Setup Phase"

    # Start ERGORS network
    if [[ "$SKIP_NETWORK" == true ]]; then
        log_warn "Skipping ERGORS network setup"
    else
        ergors_start_network || {
            log_error "ERGORS network setup failed"
            exit 1
        }
    fi

    # Setup Akash infrastructure
    if [[ "$SKIP_AKASH" == true ]]; then
        log_warn "Skipping Akash setup"
    else
        akash_setup_infrastructure || {
            log_error "Akash infrastructure setup failed"
            exit 1
        }
    fi

    log_success "Infrastructure ready"
}

# =============================================================================
# Test Execution
# =============================================================================
run_tests() {
    log_step "Running Tests"

    case "$TEST_SUITE" in
        network)
            run_network_tests
            ;;
        grants)
            run_network_tests  # Always run basic connectivity first
            run_grant_tests
            ;;
        deployment)
            run_network_tests
            akash_setup_provider || log_warn "Provider setup had issues"
            run_deployment_tests
            ;;
        all)
            # Phase 1: Network tests
            run_network_tests

            # Phase 2: Grant tests (before provider)
            run_grant_tests

            # Phase 3: Provider setup + deployment tests
            akash_setup_provider || log_warn "Provider setup had issues"
            run_deployment_tests
            ;;
        *)
            log_error "Unknown test suite: $TEST_SUITE"
            log_error "Valid options: network, grants, deployment, all"
            exit 1
            ;;
    esac
}

# =============================================================================
# Main
# =============================================================================
main() {
    echo ""
    echo -e "${CYAN}${BOLD}╔═══════════════════════════════════════════════════════════════╗${NC}"
    echo -e "${CYAN}${BOLD}║                                                               ║${NC}"
    echo -e "${CYAN}${BOLD}║          ERGORS E2E Integration Test Suite                    ║${NC}"
    echo -e "${CYAN}${BOLD}║                                                               ║${NC}"
    echo -e "${CYAN}${BOLD}╚═══════════════════════════════════════════════════════════════╝${NC}"
    echo ""

    echo -e "  ${BOLD}Test Suite:${NC}  $TEST_SUITE"
    echo -e "  ${BOLD}Test Dir:${NC}    $TEST_DIR"
    echo ""

    # Check prerequisites
    check_prerequisites

    # Build phase
    run_build_phase

    # Infrastructure setup
    run_infrastructure_phase

    # Run tests
    run_tests

    # Print summary
    local end_time duration
    end_time=$(date +%s)
    duration=$((end_time - START_TIME))

    print_test_summary "$duration"
}

main "$@"
