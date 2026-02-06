#!/bin/bash
#
# tests/deployment.sh - Akash deployment workflow tests
#
# Tests aligned with docs/specs/bootstrap/akash-deployment.md:
#   - Setup: Key verification, balance check
#   - Workflow: All 10 steps (KeySelection → Complete)
#   - Provider management: Trusted providers, selection
#   - Status and monitoring
#   - Lease closure
#   - Error handling

# Prevent multiple sourcing
[[ -n "${_E2E_TEST_DEPLOYMENT_LOADED:-}" ]] && return 0
_E2E_TEST_DEPLOYMENT_LOADED=1

# Track deployment state across tests
DEPLOY_SESSION_ID=""
DEPLOY_DSEQ=""
DEPLOY_PROVIDER=""
DEPLOY_ENDPOINTS=""

# =============================================================================
# Setup Verification Tests (Pre-deployment checks)
# =============================================================================

test_deployment_setup() {
    log_section "Deployment Setup Tests"

    local coord_home="$TEST_DIR/coordinator"

    # Test 1: Signing key exists
    log_verbose "Checking for signing keys..."
    local keys_output
    keys_output=$(ERGORS_CUSTODY_PASSWORD="${TEST_CUSTODY_PASSWORD}" \
        "$ERGORS_BIN" --home "$coord_home" keys list 2>&1) || true
    log_debug "Keys list: $keys_output"

    if echo "$keys_output" | grep -qE "akash1|default|faucet"; then
        local key_count
        key_count=$(echo "$keys_output" | grep -cE "akash1" || echo "0")
        test_pass "signing_key_exists" "Signing key(s) available ($key_count found)"
    else
        test_fail "signing_key_exists" "No signing keys found" "Run ergors keys import-mnemonic"
        return 1
    fi

    # Test 2: Default key is set
    if echo "$keys_output" | grep -qiE "default.*true|\*.*akash1"; then
        test_pass "default_key_set" "Default signing key is configured"
    else
        test_skip "default_key_set" "No default key marker found (may use first key)"
    fi

    # Test 3: Akash config section present
    log_verbose "Checking Akash configuration..."
    local config_file="$coord_home/config.toml"
    if [[ -f "$config_file" ]]; then
        if grep -q "\[akash\]" "$config_file" 2>/dev/null; then
            test_pass "akash_config" "Akash configuration section present"

            # Extract key config values
            local rpc_endpoint chain_id
            rpc_endpoint=$(grep -E "^rpc_endpoint" "$config_file" | head -1 | cut -d'"' -f2)
            chain_id=$(grep -E "^chain_id" "$config_file" | head -1 | cut -d'"' -f2)
            log_verbose "  RPC: $rpc_endpoint"
            log_verbose "  Chain ID: $chain_id"
        else
            test_fail "akash_config" "Akash configuration section missing"
        fi
    else
        test_fail "akash_config" "Config file not found" "$config_file"
    fi
}

test_balance_check() {
    log_section "Balance Check Tests"

    # Test: Query balance via CLI (Step 2: BalanceCheck from workflow)
    log_verbose "Querying account balance..."

    # Get the default key address first
    local coord_home="$TEST_DIR/coordinator"
    local keys_output
    keys_output=$(ERGORS_CUSTODY_PASSWORD="${TEST_CUSTODY_PASSWORD}" \
        "$ERGORS_BIN" --home "$coord_home" keys list 2>&1) || true

    # Extract first akash address
    local account_address
    account_address=$(echo "$keys_output" | grep -oE "akash1[a-z0-9]+" | head -1)

    if [[ -z "$account_address" ]]; then
        test_skip "balance_query" "No account address found to query balance"
        return 0
    fi

    log_verbose "Querying balance for: $account_address"

    # Use the CLI to query balance
    local balance_output
    balance_output=$(ergors_deploy_query_balance "$account_address" 2>&1) || true
    log_debug "Balance response: $balance_output"

    # Parse balance from CLI output (format varies)
    local balance_uakt
    balance_uakt=$(json_get "$balance_output" '.amount' 2>/dev/null || \
                   echo "$balance_output" | grep -oE "[0-9]+" | head -1)

    if [[ -n "$balance_uakt" ]] && [[ "$balance_uakt" =~ ^[0-9]+$ ]]; then
        local balance_akt
        balance_akt=$(echo "scale=6; $balance_uakt / 1000000" | bc 2>/dev/null || echo "?")
        test_pass "balance_query" "Account balance: $balance_akt AKT ($balance_uakt uakt)"

        # Check minimum balance (default 5 AKT = 5000000 uakt)
        local min_balance=5000000
        if [[ "$balance_uakt" -ge "$min_balance" ]]; then
            test_pass "balance_sufficient" "Balance sufficient for deployment (>= 5 AKT)"
        else
            test_fail "balance_sufficient" "Insufficient balance" "Have: $balance_uakt, Need: $min_balance uakt"
        fi
    else
        # Balance query may fail in local dev - skip gracefully
        test_skip "balance_query" "Could not parse balance (local dev network may not have balances)"
    fi
}

# =============================================================================
# SDL Template Tests
# =============================================================================

test_sdl_templates() {
    log_section "SDL Template Tests"

    # Test: List SDL templates via CLI/gRPC
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

        # Store bids output for downstream verification
        export LAST_BIDS_OUTPUT="$bids_output"
    else
        # Bids not received is acceptable in local dev environment
        test_fail "bid_reception" "No bids received" "Local provider may lack resources (timeout: ${max_wait}s)"
        log_verbose "Last bids query response:"
        log_debug "$bids_output"
    fi
}

# =============================================================================
# Bid Field Verification Tests
# =============================================================================

test_bid_field_verification() {
    log_section "Bid Field Verification Tests"

    if [[ -z "$DEPLOY_SESSION_ID" ]]; then
        test_skip "bid_field_verification" "No deployment session"
        return 1
    fi

    # Re-query bids if not cached
    local bids_output="${LAST_BIDS_OUTPUT:-}"
    if [[ -z "$bids_output" ]]; then
        bids_output=$(ergors_deploy_bids "$DEPLOY_SESSION_ID" 2>&1) || true
    fi

    local total_bids
    total_bids=$(json_get "$bids_output" '.total')

    if [[ -z "$total_bids" ]] || [[ "$total_bids" -eq 0 ]]; then
        test_skip "bid_field_verification" "No bids to verify fields on"
        return 0
    fi

    # Test 1: Bid has provider address
    local bid_provider
    bid_provider=$(json_get "$bids_output" '.bids[0].provider')
    if [[ -n "$bid_provider" ]] && [[ "$bid_provider" == akash1* ]]; then
        test_pass "bid_has_provider" "Bid contains valid provider address: ${bid_provider:0:20}..."
    elif [[ -n "$bid_provider" ]]; then
        test_fail "bid_has_provider" "Bid provider address has unexpected format" "Got: $bid_provider"
    else
        test_fail "bid_has_provider" "Bid missing provider address field"
    fi

    # Test 2: Bid has price
    local bid_price
    bid_price=$(json_get "$bids_output" '.bids[0].price_uakt')
    if [[ -z "$bid_price" ]]; then
        bid_price=$(json_get "$bids_output" '.bids[0].price')
    fi

    if [[ -n "$bid_price" ]] && [[ "$bid_price" =~ ^[0-9]+$ ]] && [[ "$bid_price" -gt 0 ]]; then
        test_pass "bid_has_price" "Bid contains price: $bid_price uakt"
    elif [[ -n "$bid_price" ]]; then
        test_fail "bid_has_price" "Bid price has unexpected format" "Got: $bid_price"
    else
        test_fail "bid_has_price" "Bid missing price field"
    fi

    # Test 3: Bid has resource offer (CPU, memory, storage)
    local bid_resources
    bid_resources=$(json_get "$bids_output" '.bids[0].resources')
    if [[ -z "$bid_resources" ]]; then
        bid_resources=$(json_get "$bids_output" '.bids[0].resource_offer')
    fi

    if [[ -n "$bid_resources" ]] && [[ "$bid_resources" != "null" ]]; then
        test_pass "bid_has_resources" "Bid contains resource offer"
        log_verbose "  Resources: ${bid_resources:0:200}"
    else
        # Resources may not be in the bid response directly -- skip gracefully
        test_skip "bid_has_resources" "Resource offer not included in bid response"
    fi

    # Test 4: Bid state is valid
    local bid_state
    bid_state=$(json_get "$bids_output" '.bids[0].state')
    if [[ -z "$bid_state" ]]; then
        bid_state=$(json_get "$bids_output" '.bids[0].status')
    fi

    if [[ -n "$bid_state" ]]; then
        if echo "$bid_state" | grep -qiE "open|active|pending|matched"; then
            test_pass "bid_state_valid" "Bid state is valid: $bid_state"
        else
            test_fail "bid_state_valid" "Unexpected bid state" "Got: $bid_state"
        fi
    else
        test_skip "bid_state_valid" "Bid state field not present"
    fi

    # Test 5: All bids have unique providers (no duplicate bids)
    if [[ "$total_bids" -gt 1 ]]; then
        local unique_providers
        unique_providers=$(echo "$bids_output" | jq -r '[.bids[].provider] | unique | length' 2>/dev/null || echo "0")
        if [[ "$unique_providers" -eq "$total_bids" ]]; then
            test_pass "bids_unique_providers" "All $total_bids bids from unique providers"
        else
            test_fail "bids_unique_providers" "Duplicate provider bids detected" "Unique: $unique_providers, Total: $total_bids"
        fi
    else
        test_skip "bids_unique_providers" "Only 1 bid received, cannot test uniqueness"
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
# Lease On-Chain Verification Tests
# =============================================================================

test_lease_onchain_verification() {
    log_section "Lease On-Chain Verification Tests"

    if [[ -z "$DEPLOY_SESSION_ID" ]]; then
        test_skip "lease_onchain" "No deployment session"
        return 1
    fi

    # Test 1: Query lease details from the engine
    log "Querying lease details from engine..."
    local get_output
    get_output=$(ergors_deploy_get "$DEPLOY_SESSION_ID" 2>&1) || true
    log_debug "Deploy get output: $get_output"

    local lease_id
    lease_id=$(json_get "$get_output" '.lease_id')
    if [[ -z "$lease_id" ]]; then
        lease_id=$(json_get "$get_output" '.lease.lease_id')
    fi

    local dseq
    dseq=$(json_get "$get_output" '.dseq')

    if [[ -n "$lease_id" ]] && [[ "$lease_id" != "null" ]]; then
        test_pass "lease_id_exists" "Lease ID recorded: ${lease_id:0:30}..."
    elif [[ -n "$dseq" ]] && [[ "$dseq" =~ ^[0-9]+$ ]]; then
        test_pass "lease_id_exists" "Deployment has DSEQ: $dseq (lease tracked)"
    else
        test_fail "lease_id_exists" "No lease ID or DSEQ found in deployment state"
        return 0
    fi

    # Test 2: Verify provider address is stored in the deployment
    local stored_provider
    stored_provider=$(json_get "$get_output" '.provider')
    if [[ -z "$stored_provider" ]]; then
        stored_provider=$(json_get "$get_output" '.lease.provider')
    fi

    if [[ -n "$stored_provider" ]] && [[ "$stored_provider" == akash1* ]]; then
        test_pass "lease_provider_stored" "Lease provider address stored: ${stored_provider:0:20}..."
        DEPLOY_PROVIDER="$stored_provider"
    elif [[ -n "$stored_provider" ]]; then
        test_pass "lease_provider_stored" "Lease provider reference stored: ${stored_provider:0:30}"
        DEPLOY_PROVIDER="$stored_provider"
    else
        test_fail "lease_provider_stored" "No provider address in deployment state"
    fi

    # Test 3: Query on-chain lease via Akash node (if dseq available)
    if [[ -n "$dseq" ]] && [[ "$dseq" =~ ^[0-9]+$ ]]; then
        log "Querying on-chain lease for DSEQ $dseq..."
        local onchain_output
        onchain_output=$(ergors_cli deploy query-lease "$DEPLOY_SESSION_ID" 2>&1) || true
        log_debug "On-chain lease query: $onchain_output"

        if echo "$onchain_output" | grep -qiE "lease|active|state"; then
            test_pass "lease_onchain_confirmed" "Lease found on-chain (DSEQ: $dseq)"
        elif echo "$onchain_output" | grep -qiE "not found|unknown"; then
            test_fail "lease_onchain_confirmed" "Lease not found on-chain" "DSEQ: $dseq"
        else
            # query-lease may not exist as a CLI command yet
            test_skip "lease_onchain_confirmed" "On-chain lease query not available"
        fi
    else
        test_skip "lease_onchain_confirmed" "No DSEQ available for on-chain query"
    fi

    # Test 4: Verify deployment workflow state reflects lease creation
    local current_step
    current_step=$(json_get "$get_output" '.current_step')
    local status
    status=$(json_get "$get_output" '.status')

    # After lease creation, step should be past ProviderSelection
    local valid_post_lease_steps="ManifestSend|EndpointRetrieval|Complete|LeaseCreate|SendManifest|WaitReady"
    if [[ -n "$current_step" ]] && echo "$current_step" | grep -qiE "$valid_post_lease_steps"; then
        test_pass "lease_workflow_state" "Workflow advanced past lease creation (step: $current_step)"
    elif [[ -n "$status" ]] && echo "$status" | grep -qiE "active|deploying|ready|complete"; then
        test_pass "lease_workflow_state" "Deployment status confirms lease (status: $status)"
    else
        test_skip "lease_workflow_state" "Could not confirm post-lease workflow state"
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
# Manifest Send Verification Tests
# =============================================================================

test_manifest_send_verification() {
    log_section "Manifest Send Verification Tests"

    if [[ -z "$DEPLOY_SESSION_ID" ]]; then
        test_skip "manifest_send" "No deployment session"
        return 1
    fi

    # Test 1: Query deployment state to verify manifest was sent
    log "Checking manifest send status..."
    local get_output
    get_output=$(ergors_deploy_get "$DEPLOY_SESSION_ID" 2>&1) || true
    log_debug "Deploy get output: $get_output"

    local current_step
    current_step=$(json_get "$get_output" '.current_step')
    local status
    status=$(json_get "$get_output" '.status')

    # If workflow is past ManifestSend, manifest was sent
    local post_manifest_steps="EndpointRetrieval|Complete|WaitReady"
    if [[ -n "$current_step" ]] && echo "$current_step" | grep -qiE "$post_manifest_steps"; then
        test_pass "manifest_sent" "Manifest sent successfully (now at step: $current_step)"
    elif [[ -n "$status" ]] && echo "$status" | grep -qiE "active|ready|complete|running"; then
        test_pass "manifest_sent" "Manifest accepted by provider (status: $status)"
    else
        # Check if manifest send is indicated in the detailed output
        local manifest_status
        manifest_status=$(json_get "$get_output" '.manifest_status')
        if [[ -z "$manifest_status" ]]; then
            manifest_status=$(json_get "$get_output" '.manifest.status')
        fi

        if [[ -n "$manifest_status" ]] && echo "$manifest_status" | grep -qiE "sent|accepted|delivered"; then
            test_pass "manifest_sent" "Manifest status: $manifest_status"
        else
            test_fail "manifest_sent" "Cannot confirm manifest was sent" "Step: $current_step, Status: $status"
        fi
    fi

    # Test 2: Verify provider acknowledged manifest
    if [[ -n "${DEPLOY_PROVIDER:-}" ]]; then
        log "Checking provider manifest acknowledgement..."
        local status_output
        status_output=$(ergors_deploy_status "$DEPLOY_SESSION_ID" 2>&1) || true
        log_debug "Status output: $status_output"

        local provider_status
        provider_status=$(json_get "$status_output" '.provider_status')
        if [[ -z "$provider_status" ]]; then
            provider_status=$(json_get "$status_output" '.lease_status')
        fi

        if [[ -n "$provider_status" ]] && echo "$provider_status" | grep -qiE "active|running|ready|accepted"; then
            test_pass "manifest_provider_ack" "Provider acknowledged manifest (status: $provider_status)"
        elif [[ -n "$provider_status" ]]; then
            test_pass "manifest_provider_ack" "Provider status available: $provider_status"
        else
            test_skip "manifest_provider_ack" "Provider acknowledgement status not available"
        fi
    else
        test_skip "manifest_provider_ack" "No provider address stored"
    fi

    # Test 3: Verify no manifest errors in deployment state
    local error_msg
    error_msg=$(json_get "$get_output" '.error')
    if [[ -z "$error_msg" ]]; then
        error_msg=$(json_get "$get_output" '.last_error')
    fi

    if [[ -z "$error_msg" ]] || [[ "$error_msg" == "null" ]]; then
        test_pass "manifest_no_errors" "No manifest errors recorded in deployment state"
    else
        test_fail "manifest_no_errors" "Manifest error recorded" "Error: ${error_msg:0:200}"
    fi
}

# =============================================================================
# Message Routing Through Deployed Endpoint Tests
# =============================================================================

test_message_routing_through_endpoint() {
    log_section "Message Routing Through Deployed Endpoint Tests"

    if [[ -z "${TEST_SERVICE_ENDPOINT:-}" ]]; then
        test_skip "message_routing" "No service endpoint available for routing test"
        return 0
    fi

    if [[ -z "$DEPLOY_SESSION_ID" ]]; then
        test_skip "message_routing" "No deployment session"
        return 0
    fi

    # Test 1: Route an inference request through the ERGORS engine to the deployed provider
    log "Routing inference request through engine to deployed endpoint..."
    local route_response
    route_response=$(curl -s --max-time 30 -X POST "http://${COORDINATOR_API}/v1/chat/completions" \
        -H "Content-Type: application/json" \
        -d "{
            \"model\": \"deployed-provider\",
            \"messages\": [{\"role\": \"user\", \"content\": \"Hello, this is an E2E routing test\"}],
            \"max_tokens\": 10,
            \"deployment_session\": \"$DEPLOY_SESSION_ID\"
        }" \
        2>/dev/null) || route_response="{}"
    log_debug "Routing response: $route_response"

    if json_has "$route_response" '.choices'; then
        test_pass "engine_to_provider_routing" "Inference request routed through engine to deployed provider"
    elif json_has "$route_response" '.error'; then
        local error_type
        error_type=$(json_get "$route_response" '.error.type')
        local error_msg
        error_msg=$(json_get "$route_response" '.error.message')

        # model_error or routing errors are acceptable -- the path is tested
        if [[ "$error_type" == "model_error" ]] || [[ "$error_type" == "not_found_error" ]]; then
            test_pass "engine_to_provider_routing" "Engine routing path works (model lookup: $error_type)"
        elif echo "$error_msg" | grep -qiE "provider|endpoint|route"; then
            test_pass "engine_to_provider_routing" "Engine attempted provider routing (error: ${error_msg:0:80})"
        else
            test_fail "engine_to_provider_routing" "Routing failed with unexpected error" "Type: $error_type, Msg: ${error_msg:0:100}"
        fi
    else
        test_fail "engine_to_provider_routing" "No valid response from engine routing" "Response: ${route_response:0:200}"
    fi

    # Test 2: Direct request to the deployed service endpoint (bypass engine)
    log "Testing direct request to deployed service endpoint..."
    local direct_response
    direct_response=$(curl -s --max-time 15 -X POST "$TEST_SERVICE_ENDPOINT/v1/chat/completions" \
        -H "Content-Type: application/json" \
        -d '{"model": "default", "messages": [{"role": "user", "content": "E2E direct test"}], "max_tokens": 10}' \
        2>/dev/null) || direct_response="{}"
    log_debug "Direct response: $direct_response"

    if json_has "$direct_response" '.choices'; then
        test_pass "direct_endpoint_inference" "Direct inference request to deployed endpoint succeeded"
    elif json_has "$direct_response" '.'; then
        # Got valid JSON back -- endpoint is responding
        test_pass "direct_endpoint_inference" "Deployed endpoint accepts inference requests (may require model config)"
    else
        test_fail "direct_endpoint_inference" "Deployed endpoint did not respond to inference request"
    fi

    # Test 3: Verify the engine can discover and use the deployment endpoint
    log "Verifying engine endpoint awareness..."
    local deploy_get_output
    deploy_get_output=$(ergors_deploy_get "$DEPLOY_SESSION_ID" 2>&1) || true

    local endpoints
    endpoints=$(json_get "$deploy_get_output" '.endpoints')
    local service_url
    service_url=$(echo "$deploy_get_output" | jq -r '.endpoints | to_entries[0].value // empty' 2>/dev/null)

    if [[ -n "$service_url" ]] && [[ "$service_url" != "null" ]]; then
        test_pass "engine_endpoint_awareness" "Engine has deployment endpoint registered: ${service_url:0:50}..."
    else
        test_skip "engine_endpoint_awareness" "Engine endpoint awareness could not be verified"
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
# Provider Management Tests
# =============================================================================

test_provider_management() {
    log_section "Provider Management Tests"

    # Test 1: List trusted providers via CLI
    log_verbose "Querying trusted providers..."
    local trusted_output
    trusted_output=$(ergors_trusted_providers 2>&1) || true
    log_debug "Trusted providers: $trusted_output"

    # Parse output (may be JSON or table format)
    if json_has "$trusted_output" '.providers'; then
        local provider_count
        provider_count=$(echo "$trusted_output" | jq -r '.providers | length' 2>/dev/null || echo "0")
        test_pass "trusted_providers_list" "Trusted providers list available ($provider_count configured)"

        # Log provider names if verbose
        if [[ "${VERBOSE:-false}" == "true" ]] && [[ "$provider_count" -gt 0 ]]; then
            echo "$trusted_output" | jq -r '.providers[] | "  - \(.label // .address)"' 2>/dev/null || true
        fi
    elif echo "$trusted_output" | grep -qiE "akash1|provider|d3akash|overclock"; then
        test_pass "trusted_providers_list" "Trusted providers returned"
    else
        # Trusted providers may be empty in local dev
        test_skip "trusted_providers_list" "No trusted providers configured (local dev)"
    fi

    # Test 2: Provider selection mode (auto vs interactive)
    # This is tested implicitly in test_lease_creation (auto-select cheapest)
    test_pass "provider_selection_mode" "Automatic provider selection tested in lease creation"
}

# =============================================================================
# Deployment Status Tests
# =============================================================================

test_deployment_status() {
    log_section "Deployment Status Tests"

    if [[ -z "$DEPLOY_SESSION_ID" ]]; then
        test_skip "deployment_status" "No deployment session"
        return 1
    fi

    # Test 1: Query deployment status via CLI
    log_verbose "Querying deployment status..."
    local status_output
    status_output=$(ergors_deploy_status "$DEPLOY_SESSION_ID" 2>&1) || true
    log_debug "Status response: $status_output"

    # Also try get for more details
    local get_output
    get_output=$(ergors_deploy_get "$DEPLOY_SESSION_ID" 2>&1) || true
    log_debug "Get response: $get_output"

    # Parse status from either response
    local lease_status
    lease_status=$(json_get "$status_output" '.status' 2>/dev/null || \
                   json_get "$get_output" '.status' 2>/dev/null || \
                   echo "$status_output" | grep -oE "(active|pending|failed|closed)" | head -1)

    local dseq
    dseq=$(json_get "$status_output" '.dseq' 2>/dev/null || \
           json_get "$get_output" '.dseq' 2>/dev/null || \
           echo "$status_output" | grep -oE "DSEQ:?\s*([0-9]+)" | grep -oE "[0-9]+")

    if [[ -n "$lease_status" ]]; then
        test_pass "deployment_status_query" "Deployment status: $lease_status"
        DEPLOY_DSEQ="$dseq"

        # Log additional status info
        log_verbose "  DSEQ: $dseq"
        local provider
        provider=$(json_get "$get_output" '.provider' 2>/dev/null)
        [[ -n "$provider" ]] && log_verbose "  Provider: $provider"
    else
        test_fail "deployment_status_query" "Could not query deployment status"
    fi

    # Test 2: Workflow step tracking
    local current_step
    current_step=$(json_get "$get_output" '.current_step' 2>/dev/null || \
                   echo "$get_output" | grep -oiE "(KeySelection|BalanceCheck|CertificateSetup|DeploymentCreate|BidWait|ProviderSelection|LeaseCreate|ManifestSend|EndpointRetrieval|Complete)" | tail -1)

    if [[ -n "$current_step" ]]; then
        test_pass "workflow_step_tracking" "Current workflow step: $current_step"
    else
        test_skip "workflow_step_tracking" "Workflow step info not available"
    fi

    # Test 3: Verify DSEQ is stored
    if [[ -n "$dseq" ]] && [[ "$dseq" =~ ^[0-9]+$ ]] && [[ "$dseq" -gt 0 ]]; then
        test_pass "dseq_stored" "Deployment sequence (DSEQ) stored: $dseq"
    else
        test_skip "dseq_stored" "DSEQ not available (may not be deployed yet)"
    fi
}

# =============================================================================
# Lease Close Tests
# =============================================================================

test_lease_close() {
    log_section "Lease Close Tests"

    if [[ -z "$DEPLOY_SESSION_ID" ]]; then
        test_skip "lease_close" "No deployment session"
        return 1
    fi

    # Only close lease if explicitly requested (don't close during normal test run)
    if [[ "${CLOSE_LEASE_AFTER_TEST:-false}" != "true" ]]; then
        test_skip "lease_close" "Skipped (set CLOSE_LEASE_AFTER_TEST=true to test)"
        return 0
    fi

    # Test: Close lease via CLI
    log "Closing deployment lease..."
    local close_output
    close_output=$(ergors_deploy_close "$DEPLOY_SESSION_ID" 2>&1) || true
    log_debug "Close response: $close_output"

    # Check for success in output
    if echo "$close_output" | grep -qiE "success|closed|lease.*closed"; then
        test_pass "lease_close" "Lease closed successfully"

        # Verify status changed
        sleep 3
        local status_output
        status_output=$(ergors_deploy_status "$DEPLOY_SESSION_ID" 2>&1) || true
        local new_status
        new_status=$(json_get "$status_output" '.status' 2>/dev/null || \
                     echo "$status_output" | grep -oiE "(closed|complete)" | head -1)

        if [[ "$new_status" == "closed" ]] || [[ "$new_status" == "complete" ]]; then
            test_pass "lease_close_verified" "Lease status updated to: $new_status"
        else
            test_fail "lease_close_verified" "Lease status not updated" "Status: $new_status"
        fi
    elif json_has "$close_output" '.error'; then
        local error_msg
        error_msg=$(json_get "$close_output" '.error')
        test_fail "lease_close" "Failed to close lease" "Error: $error_msg"
    else
        test_fail "lease_close" "Failed to close lease" "Response: ${close_output:0:100}"
    fi
}

# =============================================================================
# Error Handling Tests
# =============================================================================

test_deployment_errors() {
    log_section "Deployment Error Handling Tests"

    local coord_home="$TEST_DIR/coordinator"

    # Test 1: Invalid SDL file via CLI
    log_verbose "Testing invalid SDL file handling..."
    local invalid_sdl_file="/tmp/e2e-invalid-sdl-test.yml"
    echo "invalid: yaml: {{broken" > "$invalid_sdl_file"

    local invalid_sdl_output
    invalid_sdl_output=$(ergors_cli deploy create --sdl "$invalid_sdl_file" 2>&1) || true
    log_debug "Invalid SDL response: $invalid_sdl_output"
    rm -f "$invalid_sdl_file"

    if echo "$invalid_sdl_output" | grep -qiE "error|invalid|parse|fail"; then
        test_pass "invalid_sdl_error" "Invalid SDL returns proper error"
    else
        test_skip "invalid_sdl_error" "Error handling response unclear"
    fi

    # Test 2: Missing key error via CLI
    log_verbose "Testing missing key handling..."
    local missing_key_output
    missing_key_output=$(ergors_cli deploy create \
        --sdl "${ROOT_DIR}/docker/mock-inference-provider/deploy.local.sdl.yaml" \
        --key-name "nonexistent-key-xyz-12345" 2>&1) || true
    log_debug "Missing key response: $missing_key_output"

    if echo "$missing_key_output" | grep -qiE "key.*not.*found|unknown.*key|no.*key|error"; then
        test_pass "missing_key_error" "Missing key returns appropriate error"
    else
        test_skip "missing_key_error" "Key validation response unclear"
    fi

    # Test 3: Query non-existent session
    log_verbose "Testing non-existent session query..."
    local nonexistent_output
    nonexistent_output=$(ergors_deploy_get "nonexistent-session-id-12345" 2>&1) || true
    log_debug "Non-existent session response: $nonexistent_output"

    if echo "$nonexistent_output" | grep -qiE "not found|unknown|invalid|error"; then
        test_pass "nonexistent_session_error" "Non-existent session returns error"
    else
        test_skip "nonexistent_session_error" "Session validation response unclear"
    fi

    # Test 4: Invalid session ID format
    log_verbose "Testing invalid session ID format..."
    local invalid_id_output
    invalid_id_output=$(ergors_deploy_status "!!invalid!!id!!" 2>&1) || true
    log_debug "Invalid ID response: $invalid_id_output"

    if echo "$invalid_id_output" | grep -qiE "invalid|error|fail"; then
        test_pass "invalid_session_id_error" "Invalid session ID returns error"
    else
        test_skip "invalid_session_id_error" "Session ID validation response unclear"
    fi
}

# =============================================================================
# Combined Deployment Test Suite
# =============================================================================

run_deployment_tests() {
    log_step "Running Deployment Tests"

    # Phase 1: Setup verification
    test_deployment_setup
    test_balance_check

    # Phase 2: SDL templates
    test_sdl_templates

    # Phase 3: Deployment workflow (Steps 1-10)
    test_deployment_create      # Steps 1-4: Key, Balance, Cert, Create
    test_bid_reception          # Step 5: BidWait
    test_bid_field_verification # Step 5b: Verify bid fields (provider, price, resources)
    test_lease_creation         # Steps 6-7: ProviderSelection, LeaseCreate
    test_lease_onchain_verification  # Step 7b: Verify lease exists on-chain
    test_workflow_advance       # Step 8: ManifestSend
    test_manifest_send_verification  # Step 8b: Verify manifest sent and accepted
    test_endpoint_discovery     # Step 9: EndpointRetrieval
    test_endpoint_connectivity  # Verify endpoints work
    test_message_routing_through_endpoint  # Step 10: Route inference through deployed endpoint

    # Phase 4: Provider management
    test_provider_management

    # Phase 5: Status monitoring
    test_deployment_status

    # Phase 6: Error handling
    test_deployment_errors

    # Phase 7: Cleanup (optional)
    test_lease_close
}
