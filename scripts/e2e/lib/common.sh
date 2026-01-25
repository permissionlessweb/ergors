#!/bin/bash
#
# common.sh - Shared utilities for E2E tests
#
# Provides: colors, logging, test tracking, wait helpers

# Prevent multiple sourcing
[[ -n "${_E2E_COMMON_LOADED:-}" ]] && return 0
_E2E_COMMON_LOADED=1

# =============================================================================
# Colors
# =============================================================================
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
MAGENTA='\033[0;35m'
BOLD='\033[1m'
NC='\033[0m'

# =============================================================================
# Logging
# =============================================================================
log() { echo -e "${BLUE}[$(date +%H:%M:%S)]${NC} $1"; }
log_success() { echo -e "${GREEN}[$(date +%H:%M:%S)] ✓${NC} $1"; }
log_warn() { echo -e "${YELLOW}[$(date +%H:%M:%S)] ⚠${NC} $1"; }
log_error() { echo -e "${RED}[$(date +%H:%M:%S)] ✗${NC} $1"; }

# Verbose logging - only prints when VERBOSE=true
log_verbose() {
    [[ "${VERBOSE:-false}" == "true" ]] && echo -e "${MAGENTA}[$(date +%H:%M:%S)] [V]${NC} $1" >&2
    return 0
}

# Debug - prints variable values, JSON responses, etc when verbose
log_debug() {
    [[ "${VERBOSE:-false}" == "true" ]] && echo -e "${CYAN}$1${NC}" >&2
    return 0
}

# Run command with verbose output (tee) or quiet (redirect to log)
# Usage: run_cmd <log_file> <command...>
run_cmd() {
    local log_file="$1"
    shift
    if [[ "${VERBOSE:-false}" == "true" ]]; then
        "$@" 2>&1 | tee "$log_file"
        return "${PIPESTATUS[0]}"
    else
        "$@" > "$log_file" 2>&1
    fi
}

# Run command showing only tail when not verbose
# Usage: run_cmd_tail <lines> <command...>
run_cmd_tail() {
    local lines="$1"
    shift
    if [[ "${VERBOSE:-false}" == "true" ]]; then
        "$@" 2>&1
    else
        "$@" 2>&1 | tail -"$lines"
    fi
}

log_step() {
    echo -e "\n${CYAN}${BOLD}═══════════════════════════════════════════════════════════${NC}"
    echo -e "${CYAN}${BOLD}  $1${NC}"
    echo -e "${CYAN}${BOLD}═══════════════════════════════════════════════════════════${NC}\n"
}

log_section() {
    echo -e "\n${YELLOW}${BOLD}▶ $1${NC}"
}

# =============================================================================
# Test Tracking
# =============================================================================
TESTS_PASSED=0
TESTS_FAILED=0
declare -a TEST_RESULTS=()

test_pass() {
    local test_name="$1"
    local description="${2:-$test_name}"

    TESTS_PASSED=$((TESTS_PASSED + 1))
    TEST_RESULTS+=("PASS:$test_name:$description")
    echo -e "  ${GREEN}✓${NC} $description"

    # Log to file if TEST_DIR is set
    if [[ -n "${TEST_DIR:-}" ]]; then
        echo "[$(date +%H:%M:%S)] PASS: $test_name - $description" >> "${TEST_DIR}/test-results.log"
    fi
}

test_fail() {
    local test_name="$1"
    local description="${2:-$test_name}"
    local details="${3:-}"

    TESTS_FAILED=$((TESTS_FAILED + 1))
    TEST_RESULTS+=("FAIL:$test_name:$description:$details")
    echo -e "  ${RED}✗${NC} $description"
    if [[ -n "$details" ]]; then
        echo -e "    ${RED}└─ $details${NC}"
    fi

    if [[ -n "${TEST_DIR:-}" ]]; then
        echo "[$(date +%H:%M:%S)] FAIL: $test_name - $description" >> "${TEST_DIR}/test-results.log"
        [[ -n "$details" ]] && echo "    Details: $details" >> "${TEST_DIR}/test-results.log"
    fi
}

test_skip() {
    local test_name="$1"
    local reason="${2:-}"

    TEST_RESULTS+=("SKIP:$test_name:$reason")
    echo -e "  ${YELLOW}○${NC} $test_name (skipped${reason:+: $reason})"
}

print_test_summary() {
    local duration="${1:-0}"

    echo ""
    echo -e "${BOLD}═══════════════════════════════════════════════════════════${NC}"
    echo -e "${BOLD}  Test Summary${NC}"
    echo -e "${BOLD}═══════════════════════════════════════════════════════════${NC}"
    echo ""
    echo -e "  Duration:      ${duration}s"
    echo -e "  Tests Passed:  ${GREEN}${TESTS_PASSED}${NC}"
    echo -e "  Tests Failed:  ${RED}${TESTS_FAILED}${NC}"
    echo ""

    if [[ $TESTS_FAILED -gt 0 ]]; then
        echo -e "  ${RED}${BOLD}Failed Tests:${NC}"
        for result in "${TEST_RESULTS[@]}"; do
            if [[ "$result" == FAIL:* ]]; then
                local name=$(echo "$result" | cut -d: -f2)
                local desc=$(echo "$result" | cut -d: -f3)
                echo -e "    ${RED}✗${NC} $name: $desc"
            fi
        done
        echo ""
        return 1
    else
        echo -e "${GREEN}${BOLD}  All tests passed!${NC}"
        echo ""
        return 0
    fi
}

# =============================================================================
# Wait Helpers (polling, not sleeping)
# =============================================================================

# Wait for a TCP port to be listening
# Usage: wait_for_port <host> <port> [timeout_seconds]
wait_for_port() {
    local host="$1"
    local port="$2"
    local timeout="${3:-30}"
    local waited=0

    while [[ $waited -lt $timeout ]]; do
        if nc -z "$host" "$port" 2>/dev/null; then
            return 0
        fi
        sleep 1
        waited=$((waited + 1))
    done
    return 1
}

# Wait for an HTTP endpoint to return 2xx
# Usage: wait_for_http <url> [timeout_seconds]
wait_for_http() {
    local url="$1"
    local timeout="${2:-30}"
    local waited=0

    while [[ $waited -lt $timeout ]]; do
        local status
        status=$(curl -s -o /dev/null -w "%{http_code}" --max-time 2 "$url" 2>/dev/null || echo "000")
        if [[ "$status" =~ ^2[0-9][0-9]$ ]]; then
            return 0
        fi
        sleep 1
        waited=$((waited + 1))
    done
    return 1
}

# Wait for a process to be running
# Usage: wait_for_process <pid> [timeout_seconds]
wait_for_process() {
    local pid="$1"
    local timeout="${2:-10}"
    local waited=0

    while [[ $waited -lt $timeout ]]; do
        if kill -0 "$pid" 2>/dev/null; then
            return 0
        fi
        sleep 1
        waited=$((waited + 1))
    done
    return 1
}

# Wait for a condition function to return true
# Usage: wait_for_condition <function_name> [timeout_seconds] [interval_seconds]
wait_for_condition() {
    local condition_fn="$1"
    local timeout="${2:-30}"
    local interval="${3:-2}"
    local waited=0

    while [[ $waited -lt $timeout ]]; do
        if "$condition_fn"; then
            return 0
        fi
        sleep "$interval"
        waited=$((waited + interval))
    done
    return 1
}

# =============================================================================
# Port-Forward Helper
# =============================================================================

# Run a command with kubectl port-forward active, guarantees cleanup
# Usage: with_port_forward <namespace> <service> <local:remote> <callback> [args...]
with_port_forward() {
    local namespace="$1"
    local service="$2"
    local ports="$3"
    local callback="$4"
    shift 4

    local local_port="${ports%%:*}"
    local pf_log="${TEST_DIR:-/tmp}/port-forward-$service.log"

    log_verbose "Port-forward: kubectl port-forward -n $namespace svc/$service $ports"

    kubectl port-forward -n "$namespace" "svc/$service" "$ports" > "$pf_log" 2>&1 &
    local pf_pid=$!

    # Wait for port-forward to be ready
    if ! wait_for_port "127.0.0.1" "$local_port" 15; then
        log_verbose "Port-forward failed. Log: $(cat "$pf_log" 2>/dev/null || echo 'no log')"
        kill "$pf_pid" 2>/dev/null || true
        return 1
    fi

    log_verbose "Port-forward established (PID: $pf_pid)"

    # Run callback, capture result, always cleanup
    local rc=0
    "$callback" "$@" || rc=$?

    kill "$pf_pid" 2>/dev/null || true
    wait "$pf_pid" 2>/dev/null || true

    return $rc
}

# =============================================================================
# JSON Helpers
# =============================================================================

# Extract a field from JSON, returning empty string on error
# Usage: json_get <json_string> <jq_expression>
json_get() {
    local json="$1"
    local expr="$2"
    echo "$json" | jq -r "$expr // empty" 2>/dev/null || echo ""
}

# Check if JSON has a field
# Usage: json_has <json_string> <jq_expression>
json_has() {
    local json="$1"
    local expr="$2"
    echo "$json" | jq -e "$expr" >/dev/null 2>&1
}
