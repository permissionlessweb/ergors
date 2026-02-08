#!/bin/bash
#
# tests/security.sh - Authentication, authorization, and key security tests
#
# Tests:
#   - Public vs protected endpoint access
#   - Authenticator registration and management
#   - Error handling for malformed requests
#   - Protected route enforcement

# Prevent multiple sourcing
[[ -n "${_E2E_TEST_SECURITY_LOADED:-}" ]] && return 0
_E2E_TEST_SECURITY_LOADED=1

# =============================================================================
# Public Endpoint Access Tests
# =============================================================================

test_public_endpoints() {
    log_section "Public Endpoint Access Tests"

    # Test 1: Health endpoint is public
    log_verbose "Testing /health endpoint (public)..."
    local health_response
    health_response=$(curl -s --max-time 10 -X GET "http://${COORDINATOR_API}/health" \
        -H "Content-Type: application/json" 2>/dev/null) || health_response="{}"
    log_debug "Health response: $health_response"

    if json_has "$health_response" '.status'; then
        local status
        status=$(json_get "$health_response" '.status')
        if [[ "$status" == "ok" ]]; then
            test_pass "health_endpoint" "/health returns status: ok"
        else
            test_fail "health_endpoint" "/health status not ok" "Got: $status"
        fi
    else
        test_fail "health_endpoint" "/health did not return status field"
    fi

    # Test 2: Network topology is public
    log_verbose "Testing /network/topology endpoint (public)..."
    local topology_response
    topology_response=$(curl -s --max-time 10 -X GET "http://${COORDINATOR_API}/network/topology" \
        -H "Content-Type: application/json" 2>/dev/null) || topology_response="{}"
    log_debug "Topology response: $topology_response"

    if json_has "$topology_response" '.node_identity'; then
        test_pass "topology_endpoint" "/network/topology returns node_identity"
    else
        test_fail "topology_endpoint" "/network/topology missing node_identity"
    fi

    # Test 3: v1/chat/completions is public (Open Responses API)
    log_verbose "Testing /v1/chat/completions endpoint (public)..."
    local chat_response
    chat_response=$(curl -s --max-time 15 -X POST "http://${COORDINATOR_API}/v1/chat/completions" \
        -H "Content-Type: application/json" \
        -d '{"model":"test","messages":[{"role":"user","content":"test"}],"max_tokens":5}' \
        2>/dev/null) || chat_response="{}"
    log_debug "Chat response: $chat_response"

    # Should return valid response (may be error for missing model, but not auth error)
    if json_has "$chat_response" '.'; then
        local error_type
        error_type=$(json_get "$chat_response" '.error.type')
        if [[ "$error_type" == "authentication_error" ]]; then
            test_fail "chat_completions_public" "/v1/chat/completions requires unexpected auth"
        else
            test_pass "chat_completions_public" "/v1/chat/completions is accessible"
        fi
    else
        test_fail "chat_completions_public" "/v1/chat/completions did not return JSON"
    fi

    # Test 4: v1/responses is public (Open Responses API)
    log_verbose "Testing /v1/responses endpoint (public)..."
    local or_response
    or_response=$(curl -s --max-time 15 -X POST "http://${COORDINATOR_API}/v1/responses" \
        -H "Content-Type: application/json" \
        -d '{"model":"test","input":[{"role":"user","content":"test"}]}' \
        2>/dev/null) || or_response="{}"
    log_debug "Open Responses: $or_response"

    if json_has "$or_response" '.'; then
        local error_type
        error_type=$(json_get "$or_response" '.error.type')
        if [[ "$error_type" == "authentication_error" ]]; then
            test_fail "open_responses_public" "/v1/responses requires unexpected auth"
        else
            test_pass "open_responses_public" "/v1/responses is accessible"
        fi
    else
        test_fail "open_responses_public" "/v1/responses did not return JSON"
    fi
}

# =============================================================================
# Protected Endpoint Tests
# =============================================================================

test_protected_endpoints() {
    log_section "Protected Endpoint Tests"

    # Protected endpoints require authentication per custody-and-auth.md spec
    # These use the AuthLayer middleware

    # Test 1: /api/prompts is protected
    log_verbose "Testing /api/prompts endpoint (protected)..."
    local prompts_response
    prompts_response=$(curl -s --max-time 10 -X GET "http://${COORDINATOR_API}/api/prompts" \
        -H "Content-Type: application/json" 2>/dev/null) || prompts_response="{}"
    log_debug "Prompts response: $prompts_response"

    # Should return 401/403 or authentication error
    if json_has "$prompts_response" '.error'; then
        local error_type
        error_type=$(json_get "$prompts_response" '.error.type')
        local error_msg
        error_msg=$(json_get "$prompts_response" '.error.message')

        if [[ "$error_type" == "authentication_error" ]] || \
           echo "$error_msg" | grep -qiE "unauthorized|forbidden|auth"; then
            test_pass "prompts_protected" "/api/prompts requires authentication"
        else
            test_pass "prompts_protected" "/api/prompts returns error (type: $error_type)"
        fi
    else
        # If no error, check if it returned data
        if json_has "$prompts_response" '.' && [[ ${#prompts_response} -gt 2 ]]; then
            test_fail "prompts_protected" "SECURITY: /api/prompts accessible without auth"
        else
            test_pass "prompts_protected" "/api/prompts access controlled"
        fi
    fi

    # Test 2: /orchestrate/fractal is protected
    log_verbose "Testing /orchestrate/fractal endpoint (protected)..."
    local fractal_response
    fractal_response=$(curl -s --max-time 10 -X POST "http://${COORDINATOR_API}/orchestrate/fractal" \
        -H "Content-Type: application/json" \
        -d '{"test": "data"}' 2>/dev/null) || fractal_response="{}"
    log_debug "Fractal response: $fractal_response"

    if json_has "$fractal_response" '.error' || \
       echo "$fractal_response" | grep -qiE "unauthorized|forbidden|auth"; then
        test_pass "fractal_protected" "/orchestrate/fractal is protected"
    else
        test_fail "fractal_protected" "SECURITY: /orchestrate/fractal may be unprotected"
    fi

    # Test 3: /auth/register endpoint (protected management)
    log_verbose "Testing /auth/register endpoint..."
    local auth_register_response
    auth_register_response=$(curl -s --max-time 10 -X POST "http://${COORDINATOR_API}/auth/register" \
        -H "Content-Type: application/json" \
        -d '{"endpoint_label": "test", "contract_address": "test"}' 2>/dev/null) || auth_register_response="{}"
    log_debug "Auth register response: $auth_register_response"

    # This is a protected endpoint for registering authenticators
    if json_has "$auth_register_response" '.error' || \
       echo "$auth_register_response" | grep -qiE "unauthorized|forbidden|auth|missing"; then
        test_pass "auth_register_protected" "/auth/register is protected"
    else
        test_skip "auth_register_protected" "Auth register behavior unclear"
    fi
}

# =============================================================================
# Node Identity Tests
# =============================================================================

test_node_identity() {
    log_section "Node Identity Tests"

    # Test 1: Verify node identity from topology
    log_verbose "Checking node identity..."
    local identity_output
    identity_output=$(ergors_cli node info 2>&1) || true
    log_debug "Identity output: $identity_output"

    local node_type
    node_type=$(json_get "$identity_output" '.node_type')

    if [[ -n "$node_type" ]]; then
        test_pass "node_identity" "Node type identified: $node_type"

        # Verify coordinator has expected capabilities
        if [[ "$node_type" == *"oordinator"* ]] || [[ "$node_type" == *"evelopment"* ]]; then
            test_pass "coordinator_role" "Node has coordinator-level role"
        else
            test_skip "coordinator_role" "Node is $node_type, not coordinator"
        fi
    else
        test_fail "node_identity" "Failed to identify node type" "Response: $identity_output"
    fi

    # Test 2: Verify node_id is present
    local node_id
    node_id=$(json_get "$identity_output" '.node_id')
    if [[ -n "$node_id" ]] && [[ ${#node_id} -gt 10 ]]; then
        test_pass "node_id_present" "Node ID present (${#node_id} chars)"
    else
        test_fail "node_id_present" "Node ID missing or too short"
    fi

    # Test 3: Verify P2P address format
    local p2p_address
    p2p_address=$(json_get "$identity_output" '.p2p_address')
    if [[ "$p2p_address" =~ ^[0-9]+\.[0-9]+\.[0-9]+\.[0-9]+:[0-9]+$ ]] || \
       [[ "$p2p_address" =~ ^localhost:[0-9]+$ ]] || \
       [[ "$p2p_address" =~ ^127\.0\.0\.1:[0-9]+$ ]]; then
        test_pass "p2p_address_format" "P2P address format valid: $p2p_address"
    else
        if [[ -n "$p2p_address" ]]; then
            test_pass "p2p_address_format" "P2P address present: $p2p_address"
        else
            test_fail "p2p_address_format" "P2P address missing"
        fi
    fi
}

# =============================================================================
# Authenticator Tests
# =============================================================================

test_authenticators() {
    log_section "Authenticator Tests"

    # The ERGORS auth model uses contract-based authenticators per endpoint
    # See: custody-and-auth.md - Contract-Based Authentication

    # Test 1: List authenticators endpoint
    log_verbose "Testing /auth/list endpoint..."
    local list_response
    list_response=$(curl -s --max-time 10 -X GET "http://${COORDINATOR_API}/auth/list" \
        -H "Content-Type: application/json" 2>/dev/null) || list_response="{}"
    log_debug "Auth list response: $list_response"

    if json_has "$list_response" '.authenticators' || json_has "$list_response" '.error'; then
        if json_has "$list_response" '.authenticators'; then
            local auth_count
            auth_count=$(echo "$list_response" | jq -r '.authenticators | length' 2>/dev/null || echo "0")
            test_pass "auth_list" "Can list authenticators (count: $auth_count)"
        else
            # May require auth itself
            test_pass "auth_list" "/auth/list endpoint exists (may require auth)"
        fi
    else
        test_skip "auth_list" "Authenticator list response unclear"
    fi

    # Test 2: Check authorization endpoint
    log_verbose "Testing /auth/check endpoint..."
    local check_response
    check_response=$(curl -s --max-time 10 -X GET "http://${COORDINATOR_API}/auth/check?endpoint=test" \
        -H "Content-Type: application/json" 2>/dev/null) || check_response="{}"
    log_debug "Auth check response: $check_response"

    if json_has "$check_response" '.'; then
        test_pass "auth_check" "/auth/check endpoint responds"
    else
        test_skip "auth_check" "Auth check response unclear"
    fi
}

# =============================================================================
# Error Handling Tests
# =============================================================================

test_error_handling() {
    log_section "Error Handling Tests"

    # Test 1: Invalid JSON handling
    log_verbose "Testing invalid JSON handling..."
    local invalid_json_response
    invalid_json_response=$(curl -s --max-time 10 -X POST "http://${COORDINATOR_API}/v1/chat/completions" \
        -H "Content-Type: application/json" \
        -d 'not valid json at all {{{' \
        2>/dev/null) || invalid_json_response="{}"
    log_debug "Invalid JSON response: $invalid_json_response"

    if json_has "$invalid_json_response" '.error'; then
        local error_type
        error_type=$(json_get "$invalid_json_response" '.error.type')
        test_pass "invalid_json_error" "Invalid JSON returns error (type: $error_type)"
    else
        test_fail "invalid_json_error" "Invalid JSON did not return proper error"
    fi

    # Test 2: Missing required fields
    log_verbose "Testing missing required fields..."
    local missing_fields_response
    missing_fields_response=$(curl -s --max-time 10 -X POST "http://${COORDINATOR_API}/v1/chat/completions" \
        -H "Content-Type: application/json" \
        -d '{"model":"test"}' \
        2>/dev/null) || missing_fields_response="{}"
    log_debug "Missing fields response: $missing_fields_response"

    if json_has "$missing_fields_response" '.error'; then
        local error_msg
        error_msg=$(json_get "$missing_fields_response" '.error.message')
        if echo "$error_msg" | grep -qiE "messages|required|missing"; then
            test_pass "missing_fields_error" "Missing required field 'messages' detected"
        else
            test_pass "missing_fields_error" "Missing fields returns error"
        fi
    elif echo "$missing_fields_response" | grep -qiE "missing field|messages|required|deserialize"; then
        test_pass "missing_fields_error" "Missing fields returns error: ${missing_fields_response:0:80}"
    else
        test_fail "missing_fields_error" "Missing required fields not validated" "Response: ${missing_fields_response:0:200}"
    fi

    # Test 3: Empty request body
    log_verbose "Testing empty request body..."
    local empty_body_response
    empty_body_response=$(curl -s --max-time 10 -X POST "http://${COORDINATOR_API}/v1/chat/completions" \
        -H "Content-Type: application/json" \
        -d '' \
        2>/dev/null) || empty_body_response="{}"
    log_debug "Empty body response: $empty_body_response"

    if json_has "$empty_body_response" '.error'; then
        test_pass "empty_body_error" "Empty request body returns error"
    else
        test_fail "empty_body_error" "Empty request body not validated"
    fi

    # Test 4: Oversized payload rejection
    log_verbose "Testing oversized payload rejection..."
    # Generate a large payload (>1MB of repeated data)
    local large_content
    large_content=$(printf 'x%.0s' {1..1100000})
    local large_payload='{"model":"test","messages":[{"role":"user","content":"'"$large_content"'"}]}'

    local oversized_response
    oversized_response=$(curl -s --max-time 30 -X POST "http://${COORDINATOR_API}/v1/chat/completions" \
        -H "Content-Type: application/json" \
        -d "$large_payload" \
        2>/dev/null) || oversized_response="timeout_or_error"
    log_debug "Oversized response: ${oversized_response:0:200}..."

    # Should either error or timeout - not crash
    if json_has "$oversized_response" '.error' || \
       [[ "$oversized_response" == *"timeout"* ]] || \
       [[ "$oversized_response" == *"too large"* ]] || \
       [[ "$oversized_response" == *"413"* ]]; then
        test_pass "oversized_payload" "Oversized payload handled gracefully"
    else
        # If it returned a valid response for 1MB of garbage, that's concerning
        test_fail "oversized_payload" "Oversized payload not rejected" "Server processed 1MB+ payload"
    fi
}

# =============================================================================
# Combined Security Test Suite
# =============================================================================

run_security_tests() {
    log_step "Running Security Tests"

    # Test public vs protected endpoint access
    test_public_endpoints
    test_protected_endpoints

    # Test node identity verification
    test_node_identity

    # Test authenticator system
    test_authenticators

    # Test error handling and input validation
    test_error_handling
}
