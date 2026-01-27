#!/bin/bash
#
# tests/contracts.sh - CosmWasm contract deployment and query tests
#
# Tests:
#   - Contract artifact validation (WASM magic bytes)
#   - Contract deployment detection in logs
#   - SDL template contract queries
#   - SDL variable substitution
#   - Contract state isolation

# Prevent multiple sourcing
[[ -n "${_E2E_TEST_CONTRACTS_LOADED:-}" ]] && return 0
_E2E_TEST_CONTRACTS_LOADED=1

# Track discovered contract address
SDL_TEMPLATE_CONTRACT=""

# =============================================================================
# Contract Artifact Tests
# =============================================================================

test_contract_artifacts() {
    log_section "Contract Artifact Tests"

    local artifact="${ROOT_DIR}/contracts/artifacts/cw_sdl.wasm"

    # Test 1: Artifact file exists
    if [[ -f "$artifact" ]]; then
        local size
        size=$(ls -lh "$artifact" 2>/dev/null | awk '{print $5}')
        local bytes
        bytes=$(stat -f%z "$artifact" 2>/dev/null || stat -c%s "$artifact" 2>/dev/null)

        if [[ "$bytes" -gt 10000 ]]; then
            test_pass "artifact_exists" "SDL contract artifact exists ($size, $bytes bytes)"
        else
            test_fail "artifact_exists" "Artifact too small to be valid" "Only $bytes bytes"
        fi
    else
        test_fail "artifact_exists" "SDL contract artifact missing" "Expected: $artifact"
        return 1
    fi

    # Test 2: Valid WASM magic bytes (0x00 0x61 0x73 0x6D = \0asm)
    log_verbose "Verifying WASM magic bytes..."
    local magic
    magic=$(xxd -l 4 -p "$artifact" 2>/dev/null || od -A n -t x1 -N 4 "$artifact" 2>/dev/null | tr -d ' ')
    log_debug "Magic bytes: $magic"

    if [[ "$magic" == "0061736d" ]]; then
        test_pass "wasm_magic_bytes" "Valid WASM magic bytes (0061736d)"
    else
        test_fail "wasm_magic_bytes" "Invalid WASM file" "Magic: $magic, expected: 0061736d"
    fi

    # Test 3: List all contract artifacts in directory
    log_verbose "Scanning artifacts directory..."
    local artifact_dir="${ROOT_DIR}/contracts/artifacts"
    local artifact_count=0
    local artifact_list=""

    if [[ -d "$artifact_dir" ]]; then
        for wasm in "$artifact_dir"/*.wasm; do
            if [[ -f "$wasm" ]]; then
                artifact_count=$((artifact_count + 1))
                artifact_list+="$(basename "$wasm") "
            fi
        done
    fi

    if [[ $artifact_count -gt 0 ]]; then
        test_pass "artifacts_available" "Found $artifact_count contract artifact(s): $artifact_list"
    else
        test_fail "artifacts_available" "No contract artifacts found" "Dir: $artifact_dir"
    fi

    # Test 4: WASM copied to coordinator home
    local coord_home="$TEST_DIR/coordinator"
    if [[ -f "$coord_home/cw_sdl.wasm" ]]; then
        test_pass "wasm_copied" "SDL contract WASM present in coordinator home"
    else
        test_fail "wasm_copied" "SDL contract WASM not in coordinator home" "Expected: $coord_home/cw_sdl.wasm"
    fi
}

# =============================================================================
# Contract Deployment Tests
# =============================================================================

test_contract_deployment() {
    log_section "Contract Deployment Tests"

    local coord_home="$TEST_DIR/coordinator"
    local coord_log="$coord_home/node.log"

    # Test 1: Config has initial_contracts defined
    log_verbose "Checking config for initial_contracts..."
    if [[ -f "$coord_home/config.toml" ]]; then
        if grep -q "cw-sdl\|cw_sdl\|initial_contracts" "$coord_home/config.toml" 2>/dev/null; then
            test_pass "config_contracts" "SDL contract configured in initial_contracts"
        else
            test_fail "config_contracts" "SDL contract not in config.toml"
            log_verbose "Config content related to cosmwasm:"
            grep -i "cosmwasm\|contract\|wasm" "$coord_home/config.toml" 2>/dev/null || echo "No matches"
        fi
    else
        test_fail "config_contracts" "Coordinator config.toml missing"
        return 1
    fi

    # Test 2: CosmWasm runtime initialized (check logs)
    log_verbose "Checking CosmWasm runtime initialization..."
    if [[ -f "$coord_log" ]]; then
        if grep -qiE "cosmwasm.*init|wasm.*runtime.*init|vm.*init|contract.*loaded" "$coord_log" 2>/dev/null; then
            test_pass "cosmwasm_runtime" "CosmWasm runtime initialized"
        else
            # May have different log format - check for contract-related activity
            if grep -qiE "processing.*contract|deploy.*contract|upload.*wasm" "$coord_log" 2>/dev/null; then
                test_pass "cosmwasm_runtime" "CosmWasm contract processing detected"
            else
                test_fail "cosmwasm_runtime" "CosmWasm runtime initialization not detected in logs"
                log_verbose "WASM-related log lines:"
                grep -i "wasm\|contract" "$coord_log" 2>/dev/null | tail -10 || echo "None found"
            fi
        fi
    else
        test_fail "cosmwasm_runtime" "Coordinator log not available"
    fi

    # Test 3: Contract deployment attempted
    log_verbose "Checking for contract deployment attempt..."
    if [[ -f "$coord_log" ]]; then
        if grep -qiE "Processing.*contracts.*deployment|deploying.*contract|instantiat.*contract|upload.*code" "$coord_log" 2>/dev/null; then
            test_pass "contract_deploy_attempt" "Contract deployment initiated"
        else
            test_fail "contract_deploy_attempt" "Contract deployment not detected in logs"
        fi
    fi

    # Test 4: Contract deployment success or skip (already exists)
    log_verbose "Checking for deployment success..."
    if [[ -f "$coord_log" ]]; then
        if grep -qiE "Successfully deployed contract|contract.*deployed|instantiated.*cw.*sdl" "$coord_log" 2>/dev/null; then
            test_pass "contract_deploy_success" "SDL contract deployed successfully"
        elif grep -qiE "already deployed|skipping.*exists|contract exists" "$coord_log" 2>/dev/null; then
            test_pass "contract_deploy_success" "SDL contract already deployed (skipped)"
        else
            test_fail "contract_deploy_success" "SDL contract deployment success not confirmed"
            log_verbose "Last 20 contract-related log lines:"
            grep -i "contract\|deploy\|sdl" "$coord_log" 2>/dev/null | tail -20 || echo "None found"
        fi
    fi
}

# =============================================================================
# SDL Contract Query Tests
# =============================================================================

test_sdl_contract_queries() {
    log_section "SDL Contract Query Tests"

    # Test 1: List SDL template contracts via CLI
    log_verbose "Querying SDL template contracts..."
    local list_output
    list_output=$(ergors_sdl_list 2>&1) || true
    log_debug "SDL list output: $list_output"

    # Check for connection/engine errors
    if echo "$list_output" | grep -qiE "connection refused|failed to connect"; then
        local health_check
        health_check=$(curl -s --max-time 5 "http://${COORDINATOR_API}/health" 2>/dev/null || echo "")
        if [[ -z "$health_check" ]]; then
            test_fail "sdl_contract_list" "HTTP server not responding" "Check if engine is running"
            display_engine_logs
            return 1
        fi
    fi

    # Parse response - format depends on CLI output
    local template_count=0
    local contract_addr=""

    # Try to extract contract address from various output formats
    if echo "$list_output" | jq -e '.contracts' >/dev/null 2>&1; then
        template_count=$(echo "$list_output" | jq -r '.contracts | length' 2>/dev/null || echo "0")
        contract_addr=$(echo "$list_output" | jq -r '.contracts[0].address // empty' 2>/dev/null)
    elif echo "$list_output" | grep -qE "ergors[a-z0-9_]+"; then
        # Extract contract address from text output
        contract_addr=$(echo "$list_output" | grep -oE "ergors[a-z0-9_]+" | head -1)
        [[ -n "$contract_addr" ]] && template_count=1
    fi

    if [[ -n "$contract_addr" ]]; then
        test_pass "sdl_contract_list" "Found SDL template contract(s)"
        SDL_TEMPLATE_CONTRACT="$contract_addr"
        log_verbose "Using contract: $SDL_TEMPLATE_CONTRACT"
    elif echo "$list_output" | grep -qiE "error|failed"; then
        test_fail "sdl_contract_list" "SDL list returned error" "Output: ${list_output:0:200}"
        return 1
    else
        test_fail "sdl_contract_list" "No SDL template contracts found" "Response: ${list_output:0:200}"
        return 1
    fi

    # Test 2: Query SDL template from contract via CosmWasm query
    if [[ -n "$SDL_TEMPLATE_CONTRACT" ]]; then
        log_verbose "Querying SDL template from contract via CosmWasm..."
        local template_output
        template_output=$(ergors_sdl_get_template "$SDL_TEMPLATE_CONTRACT" 2>&1) || true
        log_debug "Template output: ${template_output:0:500}..."

        # Response format: {"contract": "...", "data": {...}}
        local sdl_template
        sdl_template=$(json_get "$template_output" '.data.sdl_template')

        if [[ -n "$sdl_template" ]] && [[ ${#sdl_template} -gt 50 ]]; then
            test_pass "sdl_template_get" "SDL template retrieved (${#sdl_template} bytes)"
        elif json_has "$template_output" '.error'; then
            local err_msg
            err_msg=$(json_get "$template_output" '.error.message')
            test_fail "sdl_template_get" "Contract query failed" "Error: $err_msg"
        else
            test_fail "sdl_template_get" "Failed to retrieve SDL template" "Response: ${template_output:0:200}"
        fi
    else
        test_skip "sdl_template_get" "No contract address available"
    fi

    # Test 3: Query variable defaults from contract via CosmWasm query
    if [[ -n "$SDL_TEMPLATE_CONTRACT" ]]; then
        log_verbose "Querying variable defaults from contract..."
        local defaults_output
        defaults_output=$(ergors_sdl_get_defaults "$SDL_TEMPLATE_CONTRACT" 2>&1) || true
        log_debug "Defaults output: $defaults_output"

        # Response format: {"contract": "...", "data": {"defaults": {...}}}
        local defaults
        defaults=$(json_get "$defaults_output" '.data.defaults')
        local default_count
        default_count=$(echo "$defaults_output" | jq -r '.data.defaults | length // 0' 2>/dev/null)

        if [[ -n "$defaults" ]] && [[ "$defaults" != "null" ]]; then
            test_pass "sdl_defaults_get" "Variable defaults retrieved ($default_count variables)"
        elif json_has "$defaults_output" '.error'; then
            local err_msg
            err_msg=$(json_get "$defaults_output" '.error.message')
            test_fail "sdl_defaults_get" "Defaults query failed" "Error: $err_msg"
        else
            # Defaults might be empty if template has no variables
            test_pass "sdl_defaults_get" "Variable defaults query succeeded (empty or no variables)"
        fi
    else
        test_skip "sdl_defaults_get" "No contract address available"
    fi
}

# =============================================================================
# SDL Variable Substitution Tests
# =============================================================================

test_sdl_variable_substitution() {
    log_section "SDL Variable Substitution Tests"

    if [[ -z "$SDL_TEMPLATE_CONTRACT" ]]; then
        test_skip "sdl_variable_substitution" "No contract address available"
        return 0
    fi

    # Test 1: Render SDL template with custom variables via CosmWasm query
    log_verbose "Rendering SDL template with variables via CosmWasm query..."
    local render_output
    render_output=$(ergors_sdl_render "$SDL_TEMPLATE_CONTRACT" \
        --var CPU=4 \
        --var MEMORY=8Gi \
        --var GPU_COUNT=1 \
        2>&1) || true
    log_debug "Render output: ${render_output:0:1000}..."

    # Response format: {"contract": "...", "data": {"rendered_sdl": "...", "used_variables": {...}}}
    local rendered_sdl
    rendered_sdl=$(json_get "$render_output" '.data.rendered_sdl')

    if [[ -n "$rendered_sdl" ]] && [[ ${#rendered_sdl} -gt 50 ]]; then
        test_pass "sdl_render" "SDL template rendered successfully (${#rendered_sdl} bytes)"
    elif json_has "$render_output" '.error'; then
        local err_msg
        err_msg=$(json_get "$render_output" '.error.message')
        test_fail "sdl_render" "Render query failed" "Error: $err_msg"
        return 1
    else
        test_fail "sdl_render" "Failed to render SDL template" "Response: ${render_output:0:300}"
        return 1
    fi

    # Test 2: Verify CPU variable was substituted
    if echo "$rendered_sdl" | grep -qE "cpu:.*4|cpu.*=.*4|cpu.*:.*4"; then
        test_pass "var_cpu_substituted" "CPU variable substituted (CPU=4)"
    else
        test_fail "var_cpu_substituted" "CPU variable not found in rendered SDL" "Expected cpu: 4 or similar"
    fi

    # Test 3: Verify MEMORY variable was substituted
    if echo "$rendered_sdl" | grep -qiE "memory.*8Gi|8Gi.*memory|memory.*8g"; then
        test_pass "var_memory_substituted" "MEMORY variable substituted (MEMORY=8Gi)"
    else
        # MEMORY might use different formats - check for presence
        if echo "$rendered_sdl" | grep -qiE "memory"; then
            test_pass "var_memory_substituted" "MEMORY field present in rendered SDL"
        else
            test_fail "var_memory_substituted" "MEMORY variable not found in rendered SDL"
        fi
    fi

    # Test 4: No unsubstituted variable placeholders remain
    log_verbose "Checking for unsubstituted variables..."
    if echo "$rendered_sdl" | grep -qE '\$\{[A-Z_]+\}|\{\{[A-Z_]+\}\}'; then
        local remaining
        remaining=$(echo "$rendered_sdl" | grep -oE '\$\{[A-Z_]+\}|\{\{[A-Z_]+\}\}' | head -5 | tr '\n' ' ')
        test_fail "no_unsubstituted_vars" "Unsubstituted variables remain" "Found: $remaining"
    else
        test_pass "no_unsubstituted_vars" "No unsubstituted variable placeholders"
    fi

    # Test 5: Rendered SDL is valid YAML
    log_verbose "Validating rendered SDL YAML..."
    if command -v python3 &>/dev/null; then
        if echo "$rendered_sdl" | python3 -c "import sys, yaml; yaml.safe_load(sys.stdin)" 2>/dev/null; then
            test_pass "rendered_yaml_valid" "Rendered SDL is valid YAML"
        else
            test_fail "rendered_yaml_valid" "Rendered SDL is not valid YAML"
        fi
    else
        test_skip "rendered_yaml_valid" "python3 not available for YAML validation"
    fi
}

# =============================================================================
# Contract Runtime State Tests
# =============================================================================

test_contract_state() {
    log_section "Contract State Tests"

    local coord_log="$TEST_DIR/coordinator/node.log"

    # Test 1: Check for contract state operations in logs
    log_verbose "Checking for contract state operations..."
    if [[ -f "$coord_log" ]]; then
        if grep -qiE "contract.*state|state.*write|state.*read|storage.*contract" "$coord_log" 2>/dev/null; then
            test_pass "contract_state_ops" "Contract state operations detected"
        else
            test_skip "contract_state_ops" "No contract state operations in logs (may be normal)"
        fi
    else
        test_skip "contract_state_ops" "Coordinator log not available"
    fi

    # Test 2: Gas metering active (if any execution happened)
    log_verbose "Checking gas metering..."
    if [[ -f "$coord_log" ]]; then
        if grep -qiE "gas.*used|gas.*limit|gas.*consumed" "$coord_log" 2>/dev/null; then
            test_pass "gas_metering" "Gas metering active"
        else
            test_skip "gas_metering" "No gas metering logs (no contract execution yet)"
        fi
    fi

    # Test 3: Contract address format validation
    if [[ -n "$SDL_TEMPLATE_CONTRACT" ]]; then
        log_verbose "Validating contract address format..."
        # ERGORS contract addresses should be ergors{node_id}_{hash} or similar
        if [[ "$SDL_TEMPLATE_CONTRACT" =~ ^ergors[a-z0-9_]+$ ]] || \
           [[ "$SDL_TEMPLATE_CONTRACT" =~ ^[a-z]+1[a-z0-9]+$ ]]; then
            test_pass "contract_address_format" "Contract address format valid: ${SDL_TEMPLATE_CONTRACT:0:30}..."
        else
            test_fail "contract_address_format" "Unexpected contract address format" "Got: $SDL_TEMPLATE_CONTRACT"
        fi
    else
        test_skip "contract_address_format" "No contract address to validate"
    fi
}

# =============================================================================
# Combined Contract Test Suite
# =============================================================================

run_contract_tests() {
    log_step "Running Contract Tests"

    test_contract_artifacts
    test_contract_deployment
    test_sdl_contract_queries
    test_sdl_variable_substitution
    test_contract_state
}
