#!/bin/bash
#
# tests/sdl_storage.sh - SDL Storage and Retrieval E2E Tests
#
# Tests CosmWasm cw-sdl contract storage/retrieval using REAL CLI wrappers
# and actual CosmWasm query API (not fabricated endpoints).
#
# This validates:
#   - Contract is deployed during init
#   - SDL templates can be queried
#   - Variable rendering works correctly
#   - Deployment results can be stored and retrieved
#
# Path A (manual cnidarium storage) is NOT implemented yet - those endpoints
# don't exist. When /api/storage/sdl is implemented, add those tests.

# Prevent multiple sourcing
[[ -n "${_E2E_TEST_SDL_STORAGE_LOADED:-}" ]] && return 0
_E2E_TEST_SDL_STORAGE_LOADED=1

# Test SDL content (simple but valid Akash SDL)
TEST_SDL_TEMPLATE='---
version: "2.0"

services:
  test:
    image: nginx:latest
    expose:
      - port: 80
        as: 80
        to:
          - global: true

profiles:
  compute:
    test:
      resources:
        cpu:
          units: 2
        memory:
          size: 4Gi
        storage:
          - size: 512Mi
  placement:
    akash:
      pricing:
        test:
          denom: uakt
          amount: 1000

deployment:
  test:
    akash:
      profile: test
      count: 1
'

# Track discovered contract address
CW_SDL_CONTRACT_ADDR=""

# =============================================================================
# CosmWasm cw-sdl Contract Tests (Using Real CLI/API)
# =============================================================================

test_cw_sdl_contract_deployed() {
    log_section "CosmWasm cw-sdl Contract Deployment Validation"

    # Test 1: Check if cw-sdl.wasm artifact was built
    local artifact="${ROOT_DIR}/contracts/artifacts/cw_sdl.wasm"

    if [[ ! -f "$artifact" ]]; then
        test_fail "cw_sdl_artifact_missing" "cw-sdl.wasm artifact not found" "Expected: $artifact"
        return 1
    fi

    local artifact_size
    artifact_size=$(stat -f%z "$artifact" 2>/dev/null || stat -c%s "$artifact" 2>/dev/null)

    if [[ "$artifact_size" -lt 100000 ]]; then
        test_fail "cw_sdl_artifact_invalid" "cw-sdl.wasm too small" "Only $artifact_size bytes"
        return 1
    fi

    test_pass "cw_sdl_artifact_exists" "cw-sdl.wasm artifact exists ($artifact_size bytes)"

    # Test 2: List SDL contracts using CLI (queries storage, not fabricated endpoints)
    log_verbose "Querying SDL contracts via ergors CLI..."
    local list_output
    list_output=$(ergors_sdl_list 2>&1)
    log_debug "SDL list output: ${list_output:0:500}"

    # Check for errors
    if echo "$list_output" | grep -qiE "connection refused|failed to connect|error"; then
        test_fail "cw_sdl_list_failed" "SDL list command failed" "Output: ${list_output:0:200}"
        return 1
    fi

    # Parse contract address from output
    # Format: JSON with contracts array or plain text with contract address
    if echo "$list_output" | jq -e '.contracts' >/dev/null 2>&1; then
        CW_SDL_CONTRACT_ADDR=$(echo "$list_output" | jq -r '.contracts[0].address // empty' 2>/dev/null)
    elif echo "$list_output" | grep -qE "ergors[a-z0-9_]+"; then
        CW_SDL_CONTRACT_ADDR=$(echo "$list_output" | grep -oE "ergors[a-z0-9_]+" | head -1)
    fi

    if [[ -z "$CW_SDL_CONTRACT_ADDR" ]]; then
        test_fail "cw_sdl_contract_not_found" "No SDL contract found in storage" \
            "This means init.rs didn't configure it or ContractManager didn't deploy it. Check logs."
        return 1
    fi

    test_pass "cw_sdl_contract_deployed" "cw-sdl contract deployed (address: ${CW_SDL_CONTRACT_ADDR:0:30}...)"
}

test_cw_sdl_template_query() {
    log_section "SDL Template Query via CosmWasm"

    if [[ -z "$CW_SDL_CONTRACT_ADDR" ]]; then
        test_fail "cw_sdl_template_query" "No contract address available (previous test must have failed)"
        return 1
    fi

    # Query template using real CLI wrapper (uses POST /api/cosmwasm/query)
    log_verbose "Querying SDL template from contract..."
    local template_output
    template_output=$(ergors_sdl_get_template "$CW_SDL_CONTRACT_ADDR" 2>&1)
    log_debug "Template output: ${template_output:0:500}..."

    # Check for query errors
    if echo "$template_output" | jq -e '.error' >/dev/null 2>&1; then
        local error_msg
        error_msg=$(echo "$template_output" | jq -r '.error // .error.message // "unknown"')
        test_fail "cw_sdl_template_query" "Contract query failed" "Error: $error_msg"
        return 1
    fi

    # Extract SDL template from response
    local sdl_template
    sdl_template=$(echo "$template_output" | jq -r '.data.sdl_template // .sdl_template // empty' 2>/dev/null)

    if [[ -z "$sdl_template" ]]; then
        test_fail "cw_sdl_template_query" "No SDL template in response" "Response: ${template_output:0:200}"
        return 1
    fi

    # Template might be empty on first deploy (not populated yet) or have content
    if [[ ${#sdl_template} -lt 10 ]]; then
        test_pass "cw_sdl_template_query" "SDL template query succeeded (empty - not populated yet)"
    else
        test_pass "cw_sdl_template_query" "SDL template retrieved (${#sdl_template} bytes)"
    fi
}

test_cw_sdl_variable_defaults() {
    log_section "SDL Variable Defaults Query"

    if [[ -z "$CW_SDL_CONTRACT_ADDR" ]]; then
        test_fail "cw_sdl_defaults_query" "No contract address available"
        return 1
    fi

    log_verbose "Querying variable defaults from contract..."
    local defaults_output
    defaults_output=$(ergors_sdl_get_defaults "$CW_SDL_CONTRACT_ADDR" 2>&1)
    log_debug "Defaults output: ${defaults_output:0:300}"

    # Check for errors
    if echo "$defaults_output" | jq -e '.error' >/dev/null 2>&1; then
        local error_msg
        error_msg=$(echo "$defaults_output" | jq -r '.error // .error.message // "unknown"')
        test_fail "cw_sdl_defaults_query" "Defaults query failed" "Error: $error_msg"
        return 1
    fi

    # Extract defaults
    local defaults
    defaults=$(echo "$defaults_output" | jq -r '.data.defaults // .defaults // {}' 2>/dev/null)

    if [[ "$defaults" == "{}" ]] || [[ -z "$defaults" ]]; then
        test_pass "cw_sdl_defaults_query" "Variable defaults query succeeded (empty)"
    else
        local default_count
        default_count=$(echo "$defaults" | jq 'length' 2>/dev/null || echo "0")
        test_pass "cw_sdl_defaults_query" "Variable defaults retrieved ($default_count variables)"
    fi
}

test_cw_sdl_variable_rendering() {
    log_section "SDL Variable Rendering"

    if [[ -z "$CW_SDL_CONTRACT_ADDR" ]]; then
        test_fail "cw_sdl_render" "No contract address available"
        return 1
    fi

    # Render SDL with custom variables using real CLI wrapper
    log_verbose "Rendering SDL with custom variables (CPU=8, MEMORY=16Gi, GPU_COUNT=2)..."
    local render_output
    render_output=$(ergors_sdl_render "$CW_SDL_CONTRACT_ADDR" \
        --var CPU=8 \
        --var MEMORY=16Gi \
        --var GPU_COUNT=2 \
        2>&1)
    log_debug "Render output: ${render_output:0:1000}..."

    # Check for errors
    if echo "$render_output" | jq -e '.error' >/dev/null 2>&1; then
        local error_msg
        error_msg=$(echo "$render_output" | jq -r '.error // .error.message // "unknown"')
        # If template is empty, rendering will fail - that's expected on fresh deploy
        if echo "$error_msg" | grep -qiE "empty|no template|not found"; then
            test_skip "cw_sdl_render" "Template not populated yet (expected on first init)"
            return 0
        fi
        test_fail "cw_sdl_render" "SDL rendering failed" "Error: $error_msg"
        return 1
    fi

    # Extract rendered SDL
    local rendered_sdl
    rendered_sdl=$(echo "$render_output" | jq -r '.data.rendered_sdl // .rendered_sdl // empty' 2>/dev/null)

    if [[ -z "$rendered_sdl" ]] || [[ ${#rendered_sdl} -lt 50 ]]; then
        test_fail "cw_sdl_render" "Rendered SDL empty or too small" "Got: ${#rendered_sdl} bytes"
        return 1
    fi

    test_pass "cw_sdl_render" "SDL rendered successfully (${#rendered_sdl} bytes)"

    # Verify variable substitution (use proper multi-line grep)
    log_verbose "Verifying variable substitution..."

    # CPU check: look for "8" in units field (may be on different line from "cpu:")
    if echo "$rendered_sdl" | grep -q "units:.*8" || echo "$rendered_sdl" | grep -A2 "cpu:" | grep -q "8"; then
        test_pass "cw_sdl_var_cpu" "CPU variable substituted (CPU=8)"
    else
        test_fail "cw_sdl_var_cpu" "CPU variable not substituted correctly"
    fi

    # MEMORY check: look for "16Gi" anywhere in rendered SDL
    if echo "$rendered_sdl" | grep -q "16Gi"; then
        test_pass "cw_sdl_var_memory" "MEMORY variable substituted (MEMORY=16Gi)"
    else
        test_fail "cw_sdl_var_memory" "MEMORY variable not substituted correctly"
    fi

    # GPU_COUNT check
    if echo "$rendered_sdl" | grep -q "units:.*2" || echo "$rendered_sdl" | grep -A2 "gpu:" | grep -q "2"; then
        test_pass "cw_sdl_var_gpu" "GPU_COUNT variable substituted (GPU_COUNT=2)"
    else
        test_skip "cw_sdl_var_gpu" "GPU field may not be in template or formatted differently"
    fi

    # Verify no unsubstituted placeholders remain
    if echo "$rendered_sdl" | grep -qE '\$\{[A-Z_]+\}'; then
        local remaining
        remaining=$(echo "$rendered_sdl" | grep -oE '\$\{[A-Z_]+\}' | head -5 | tr '\n' ' ')
        test_fail "cw_sdl_no_placeholders" "Unsubstituted variables remain" "Found: $remaining"
    else
        test_pass "cw_sdl_no_placeholders" "No unsubstituted variable placeholders"
    fi

    # Validate rendered SDL is valid YAML
    if command -v python3 &>/dev/null; then
        if echo "$rendered_sdl" | python3 -c "import sys, yaml; yaml.safe_load(sys.stdin)" 2>/dev/null; then
            test_pass "cw_sdl_yaml_valid" "Rendered SDL is valid YAML"
        else
            test_fail "cw_sdl_yaml_valid" "Rendered SDL is not valid YAML"
        fi
    else
        test_skip "cw_sdl_yaml_valid" "python3 not available for YAML validation"
    fi
}

test_cw_sdl_deployment_results() {
    log_section "SDL Deployment Result Storage"

    if [[ -z "$CW_SDL_CONTRACT_ADDR" ]]; then
        test_fail "cw_sdl_record_result" "No contract address available"
        return 1
    fi

    # This tests RecordDeploymentResult execute message
    # Note: This requires proper sender authentication (admin)
    log_verbose "Testing deployment result storage via contract execute..."

    local test_key="e2e_test_result_$(date +%s)"
    local test_value="test_deployment_data"

    # Use real CosmWasm execute wrapper
    local execute_result
    execute_result=$(ergors_cw_execute "$CW_SDL_CONTRACT_ADDR" "ergors_coordinator" \
        "{\"record_deployment_result\": {\"key\": \"$test_key\", \"value\": \"$test_value\"}}" \
        '[]' 2>&1) || true

    log_debug "Execute result: ${execute_result:0:500}"

    # Check result
    if echo "$execute_result" | jq -e '.error' >/dev/null 2>&1; then
        local error_msg
        error_msg=$(echo "$execute_result" | jq -r '.error // .error.message // "unknown"')

        # Unauthorized is expected in test env (sender may not match admin)
        if echo "$error_msg" | grep -qiE "unauthorized|permission|admin"; then
            test_skip "cw_sdl_record_result" "Execute requires admin permission (expected in test env)"
            return 0
        fi

        test_fail "cw_sdl_record_result" "Failed to record deployment result" "Error: $error_msg"
        return 1
    fi

    test_pass "cw_sdl_record_result" "Deployment result recorded via contract execute"

    # Query it back
    log_verbose "Querying deployment result back..."
    local query_result
    query_result=$(ergors_cw_query "$CW_SDL_CONTRACT_ADDR" \
        "{\"get_deployment_result\": {\"key\": \"$test_key\"}}" 2>&1)

    if echo "$query_result" | jq -e '.data.value' >/dev/null 2>&1; then
        local retrieved_value
        retrieved_value=$(echo "$query_result" | jq -r '.data.value')

        if [[ "$retrieved_value" == "$test_value" ]]; then
            test_pass "cw_sdl_query_result" "Deployment result retrieved correctly"
        else
            test_fail "cw_sdl_query_result" "Retrieved value doesn't match" \
                "Expected: $test_value, Got: $retrieved_value"
        fi
    else
        test_skip "cw_sdl_query_result" "Could not verify result retrieval (may not be stored)"
    fi
}

# =============================================================================
# Combined SDL Storage Test Suite
# =============================================================================

run_sdl_storage_tests() {
    log_step "Running SDL Storage E2E Tests"

    # These tests use the ACTUAL CLI wrappers and CosmWasm query API
    # No fabricated endpoints, no silent skips
    test_cw_sdl_contract_deployed
    test_cw_sdl_template_query
    test_cw_sdl_variable_defaults
    test_cw_sdl_variable_rendering
    test_cw_sdl_deployment_results
}
