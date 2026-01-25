#!/bin/bash
#
# tests/deployment.sh - Deployment workflow tests
#
# Tests: SDL templates, deployment creation, bid queries, lease creation

# Prevent multiple sourcing
[[ -n "${_E2E_TEST_DEPLOYMENT_LOADED:-}" ]] && return 0
_E2E_TEST_DEPLOYMENT_LOADED=1

# Track deployment session across tests
DEPLOY_SESSION_ID=""

# =============================================================================
# SDL Template Tests
# =============================================================================

test_sdl_templates() {
    log_section "SDL Template Tests"

    # Test: List SDL templates via API
    log "Querying SDL template contracts..."
    local list_output
    list_output=$(ergors_sdl_list 2>&1) || true

    local template_count
    template_count=$(json_get "$list_output" '.templates | length')

    if [[ -n "$template_count" ]] && [[ "$template_count" -gt 0 ]]; then
        test_pass "sdl_templates_found" "Found $template_count SDL template contract(s)"

        # Get first contract for further tests
        export SDL_TEMPLATE_CONTRACT
        SDL_TEMPLATE_CONTRACT=$(json_get "$list_output" '.templates[0].contract_address')
        log "  Using contract: ${SDL_TEMPLATE_CONTRACT:0:20}..."
    else
        test_fail "sdl_templates_found" "No SDL templates found" "API response had no templates"
        return 1
    fi

    # Test: Get SDL template from contract
    if [[ -n "$SDL_TEMPLATE_CONTRACT" ]]; then
        log "Getting SDL template from contract..."
        local template_output
        template_output=$(ergors_sdl_get_template "$SDL_TEMPLATE_CONTRACT" 2>&1) || true

        local sdl_template
        sdl_template=$(json_get "$template_output" '.sdl_template')

        if [[ -n "$sdl_template" ]]; then
            test_pass "sdl_template_get" "SDL template retrieved (${#sdl_template} bytes)"
        else
            test_fail "sdl_template_get" "Failed to retrieve SDL template"
        fi
    fi

    # Test: Render SDL template with variables
    if [[ -n "$SDL_TEMPLATE_CONTRACT" ]]; then
        log "Rendering SDL template with custom variables..."
        local render_output
        render_output=$(ergors_sdl_render "$SDL_TEMPLATE_CONTRACT" \
            --var CPU=4 \
            --var MEMORY=8Gi \
            --var GPU_COUNT=1 \
            2>&1) || true

        local rendered_sdl
        rendered_sdl=$(json_get "$render_output" '.rendered_sdl')

        if [[ -n "$rendered_sdl" ]]; then
            test_pass "sdl_render" "SDL template rendered successfully"

            # Verify variable substitution
            if echo "$rendered_sdl" | grep -q "cpu:.*4"; then
                test_pass "sdl_var_substitution" "Variable substitution verified"
            else
                test_fail "sdl_var_substitution" "Variable substitution failed"
            fi
        else
            test_fail "sdl_render" "Failed to render SDL template"
        fi
    fi
}

# =============================================================================
# Deployment Creation Tests
# =============================================================================

test_deployment_create() {
    log_section "Deployment Creation Tests"

    local deploy_sdl="${ROOT_DIR}/docker/mock-inference-provider/deploy.local.sdl.yaml"

    if [[ ! -f "$deploy_sdl" ]]; then
        test_skip "deployment_create" "SDL file not found: $deploy_sdl"
        return 1
    fi

    # Test: Create deployment workflow
    log "Creating deployment via engine..."
    log_verbose "SDL file: $deploy_sdl"
    local create_output
    create_output=$(ergors_deploy_create "$deploy_sdl" "default") || true
    log_verbose "Create deployment response:"
    log_debug "$create_output"

    local session_id
    session_id=$(json_get "$create_output" '.session_id')

    if [[ -n "$session_id" ]]; then
        DEPLOY_SESSION_ID="$session_id"
        test_pass "deployment_create" "Deployment workflow created (session: ${session_id:0:12}...)"
    else
        test_fail "deployment_create" "Failed to create deployment" "Response: ${create_output:0:100}"
        return 1
    fi

    # Test: Deployment appears in list
    log "Listing deployments..."
    local list_output
    list_output=$(ergors_deploy_list 2>&1) || true

    local total_count
    total_count=$(json_get "$list_output" '.total_count')

    if [[ -n "$total_count" ]] && [[ "$total_count" -gt 0 ]]; then
        test_pass "deployment_list" "Deployment list shows $total_count workflow(s)"
    else
        test_fail "deployment_list" "Deployment list empty"
    fi
}

# =============================================================================
# Bid Reception Tests
# =============================================================================

test_bid_reception() {
    log_section "Bid Reception Tests"

    if [[ -z "$DEPLOY_SESSION_ID" ]]; then
        test_skip "bid_query" "No deployment session to query bids for"
        return 1
    fi

    # Test: Query bids with polling
    log "Waiting for bids..."
    local max_wait=30
    local waited=0
    local total_bids=0

    while [[ $waited -lt $max_wait ]]; do
        local bids_output
        bids_output=$(ergors_deploy_bids "$DEPLOY_SESSION_ID" 2>&1) || true

        total_bids=$(json_get "$bids_output" '.total')
        if [[ -n "$total_bids" ]] && [[ "$total_bids" -gt 0 ]]; then
            break
        fi

        sleep 3
        waited=$((waited + 3))
        log "  Waiting for bids... (${waited}s/${max_wait}s)"
    done

    if [[ "$total_bids" -gt 0 ]]; then
        test_pass "bid_reception" "Received $total_bids bid(s)"
        log_verbose "Bids response:"
        log_debug "$bids_output"
    else
        # Bids not received is acceptable in local dev environment
        test_fail "bid_reception" "No bids received" "Local provider may lack resources (timeout: ${max_wait}s)"
        log_verbose "Last bids query response:"
        log_debug "$bids_output"
    fi
}

# =============================================================================
# Lease Creation Tests
# =============================================================================

test_lease_creation() {
    log_section "Lease Creation Tests"

    if [[ -z "$DEPLOY_SESSION_ID" ]]; then
        test_skip "lease_creation" "No deployment session"
        return 1
    fi

    # Get first bid's provider
    local bids_output
    bids_output=$(ergors_deploy_bids "$DEPLOY_SESSION_ID" 2>&1) || true

    local provider_addr
    provider_addr=$(json_get "$bids_output" '.bids[0].provider')
    local bid_price
    bid_price=$(json_get "$bids_output" '.bids[0].price_uakt')

    if [[ -z "$provider_addr" ]]; then
        # Use fallback for local testing
        provider_addr="akash1localprovider"
        bid_price="100"
        log "No bids available, using test provider"
    fi

    # Test: Select provider
    log "Selecting provider: $provider_addr..."
    local select_output
    select_output=$(ergors_deploy_select "$DEPLOY_SESSION_ID" "$provider_addr" "$bid_price") || true

    if json_has "$select_output" '.success' && [[ $(json_get "$select_output" '.success') == "true" ]]; then
        test_pass "provider_selection" "Provider selected successfully"
    else
        test_fail "provider_selection" "Provider selection failed" "Response: ${select_output:0:100}"
        return 1
    fi

    # Wait for tx to finalize
    sleep 3

    # Test: Verify workflow state
    local get_output
    get_output=$(ergors_deploy_get "$DEPLOY_SESSION_ID" 2>&1) || true

    local current_step
    current_step=$(json_get "$get_output" '.current_step')

    if [[ -n "$current_step" ]]; then
        test_pass "workflow_state" "Workflow at step: $current_step"
    else
        test_fail "workflow_state" "Could not determine workflow state"
    fi
}

# =============================================================================
# Workflow Advancement Tests
# =============================================================================

test_workflow_advance() {
    log_section "Workflow Advancement Tests"

    if [[ -z "$DEPLOY_SESSION_ID" ]]; then
        test_skip "workflow_advance" "No deployment session"
        return 1
    fi

    # Test: Advance workflow (sends manifest)
    log "Advancing workflow..."
    local advance_output
    advance_output=$(ergors_deploy_advance "$DEPLOY_SESSION_ID") || true

    if json_has "$advance_output" '.success' && [[ $(json_get "$advance_output" '.success') == "true" ]]; then
        test_pass "workflow_advance" "Workflow advanced successfully"
    else
        test_fail "workflow_advance" "Workflow advancement failed" "Response: ${advance_output:0:100}"
    fi

    # Wait for deployment to be ready
    sleep 10

    # Test: Check final status
    local status_output
    status_output=$(ergors_deploy_get "$DEPLOY_SESSION_ID" 2>&1) || true

    local status
    status=$(json_get "$status_output" '.status')

    if [[ -n "$status" ]]; then
        test_pass "deployment_status" "Deployment status: $status"
    else
        test_fail "deployment_status" "Could not determine deployment status"
    fi
}

# =============================================================================
# Endpoint Discovery Tests
# =============================================================================

test_endpoint_discovery() {
    log_section "Endpoint Discovery Tests"

    if [[ -z "$DEPLOY_SESSION_ID" ]]; then
        test_skip "endpoint_discovery" "No deployment session"
        return 1
    fi

    # Test: Query deployment endpoints
    log "Querying deployment endpoints..."
    local get_output
    get_output=$(ergors_deploy_get "$DEPLOY_SESSION_ID" 2>&1) || true

    local endpoints
    endpoints=$(json_get "$get_output" '.endpoints')

    if [[ -n "$endpoints" ]] && [[ "$endpoints" != "null" ]] && [[ "$endpoints" != "{}" ]]; then
        local endpoint_count
        endpoint_count=$(echo "$get_output" | jq -r '.endpoints | length' 2>/dev/null || echo 0)
        test_pass "endpoint_discovery" "Discovered $endpoint_count service endpoint(s)"

        # Extract first endpoint for connectivity test
        export TEST_SERVICE_ENDPOINT
        TEST_SERVICE_ENDPOINT=$(echo "$get_output" | jq -r '.endpoints | to_entries[0].value // empty' 2>/dev/null)

        if [[ -n "$TEST_SERVICE_ENDPOINT" ]]; then
            log "  Endpoint: $TEST_SERVICE_ENDPOINT"
        fi
    else
        test_fail "endpoint_discovery" "No endpoints discovered" "Lease may not be active yet"
    fi
}

# =============================================================================
# Endpoint Connectivity Tests
# =============================================================================

test_endpoint_connectivity() {
    log_section "Endpoint Connectivity Tests"

    if [[ -z "${TEST_SERVICE_ENDPOINT:-}" ]]; then
        test_skip "endpoint_connectivity" "No service endpoint available"
        return 1
    fi

    # Test: HTTP connectivity
    log "Testing HTTP connectivity to $TEST_SERVICE_ENDPOINT..."
    local http_status
    http_status=$(curl -s -o /dev/null -w "%{http_code}" --max-time 10 "$TEST_SERVICE_ENDPOINT" 2>/dev/null || echo "000")

    if [[ "$http_status" != "000" ]]; then
        test_pass "http_connectivity" "HTTP connectivity confirmed (status: $http_status)"
    else
        test_fail "http_connectivity" "HTTP connectivity failed" "Connection timeout or refused"
        return 1
    fi

    # Test: Response time
    local start_time end_time response_time
    start_time=$(date +%s%3N)
    curl -s -o /dev/null --max-time 10 "$TEST_SERVICE_ENDPOINT" 2>/dev/null || true
    end_time=$(date +%s%3N)
    response_time=$((end_time - start_time))

    if [[ $response_time -lt 5000 ]]; then
        test_pass "response_time" "Response time acceptable: ${response_time}ms"
    else
        test_fail "response_time" "Response time high: ${response_time}ms"
    fi

    # Test: Check for health endpoint
    for health_path in "/health" "/healthz" "/ready" "/v1/models"; do
        local health_url="${TEST_SERVICE_ENDPOINT}${health_path}"
        local health_status
        health_status=$(curl -s -o /dev/null -w "%{http_code}" --max-time 5 "$health_url" 2>/dev/null || echo "000")

        if [[ "$health_status" =~ ^2[0-9][0-9]$ ]]; then
            test_pass "health_endpoint" "Health endpoint found: $health_path (status: $health_status)"
            return 0
        fi
    done

    test_fail "health_endpoint" "No standard health endpoint found"
}

# =============================================================================
# Combined Deployment Test Suite
# =============================================================================

run_deployment_tests() {
    log_step "Running Deployment Tests"

    test_sdl_templates
    test_deployment_create
    test_bid_reception
    test_lease_creation
    test_workflow_advance
    test_endpoint_discovery
    test_endpoint_connectivity
}
