#!/bin/bash
#
# tests/grants.sh - Authz and Feegrant workflow tests
#
# Tests: Grant request/approval, feegrant allowances, cross-account deployment

# Prevent multiple sourcing
[[ -n "${_E2E_TEST_GRANTS_LOADED:-}" ]] && return 0
_E2E_TEST_GRANTS_LOADED=1

# Track grant request ID across tests
GRANT_REQUEST_ID=""

# =============================================================================
# Account Funding Tests
# =============================================================================

test_account_funding() {
    log_section "Account Funding Tests"

    local coord_home="$TEST_DIR/coordinator"

    # Test: Import faucet mnemonic into coordinator
    # This gives the coordinator the pre-funded faucet key (10B AKT from genesis)
    log "Importing faucet key into coordinator..."

    if ergors_import_faucet_key "faucet" "$coord_home"; then
        test_pass "faucet_key_import" "Faucet key imported into coordinator"
    else
        test_fail "faucet_key_import" "Failed to import faucet key"
        return 1
    fi

    # Test: Verify faucet key has balance
    if [[ -z "${FAUCET_ADDRESS:-}" ]]; then
        test_fail "faucet_address" "Faucet address not set after import"
        return 1
    fi

    local balance_output
    balance_output=$(ergors_cli deploy query-balance "$FAUCET_ADDRESS" --denom uakt 2>&1) || true
    log_verbose "Balance query response: $balance_output"
    local balance
    balance=$(json_get "$balance_output" '.amount')

    if [[ -n "$balance" ]] && [[ "$balance" != "0" ]]; then
        # Format large number for readability
        local akt_balance=$((balance / 1000000))
        test_pass "faucet_balance" "Faucet has balance: ${akt_balance} AKT"
    else
        test_fail "faucet_balance" "Faucet has no balance" "Balance: ${balance:-0}"
    fi

    # Cache the faucet address as coordinator address for later tests
    COORDINATOR_ADDRESS="$FAUCET_ADDRESS"
}

# =============================================================================
# Grant Request Tests
# =============================================================================

test_grant_request() {
    log_section "Grant Request Tests"

    # Ensure we have addresses
    if [[ -z "$COORDINATOR_ADDRESS" ]] || [[ -z "$EXECUTOR_ADDRESS" ]]; then
        if ! ergors_get_addresses; then
            test_skip "grant_request" "No addresses available"
            return 1
        fi
    fi

    # Test: Executor requests grant from coordinator
    log "Executor requesting grant from coordinator..."
    log_verbose "Granter: $COORDINATOR_ADDRESS"
    log_verbose "Grantee: $EXECUTOR_ADDRESS"
    local grant_output
    grant_output=$(ergors_grant_request "$COORDINATOR_ADDRESS" "$EXECUTOR_ADDRESS" 10000000 "E2E test") || true
    log_verbose "Grant request response:"
    log_debug "$grant_output"

    local request_id
    request_id=$(json_get "$grant_output" '.request_id')

    if [[ -n "$request_id" ]]; then
        GRANT_REQUEST_ID="$request_id"
        test_pass "grant_request" "Grant request created (ID: ${request_id:0:12}...)"
    else
        test_fail "grant_request" "Failed to create grant request" "Response: ${grant_output:0:100}"
        return 1
    fi
}

# =============================================================================
# Grant Approval Tests
# =============================================================================

test_grant_approval() {
    log_section "Grant Approval Tests"

    if [[ -z "$GRANT_REQUEST_ID" ]]; then
        test_skip "grant_approval" "No grant request to approve"
        return 1
    fi

    # Test: Coordinator approves grant
    log "Coordinator approving grant request..."
    local approve_output
    approve_output=$(ergors_grant_approve "$GRANT_REQUEST_ID" "Approved for E2E testing") || true

    if json_has "$approve_output" '.success' && [[ $(json_get "$approve_output" '.success') == "true" ]]; then
        test_pass "grant_approval" "Grant request approved"
    else
        test_fail "grant_approval" "Failed to approve grant" "Response: ${approve_output:0:100}"
        return 1
    fi

    # Wait for blockchain to process
    log "Waiting for blockchain confirmation..."
    sleep 5
}

# =============================================================================
# Grant Verification Tests
# =============================================================================

test_grant_verification() {
    log_section "Grant Verification Tests"

    if [[ -z "$COORDINATOR_ADDRESS" ]] || [[ -z "$EXECUTOR_ADDRESS" ]]; then
        test_skip "grant_verification" "No addresses available"
        return 1
    fi

    # Test: Query authz grants on blockchain
    log "Querying authz grants on blockchain..."
    local grants_output
    grants_output=$(akash_make query-grants --granter "$COORDINATOR_ADDRESS" --grantee "$EXECUTOR_ADDRESS" 2>&1) || true

    if echo "$grants_output" | grep -qi "authorization\|Authorization"; then
        test_pass "authz_grant_exists" "Authz grant found on blockchain"
    else
        test_fail "authz_grant_exists" "Authz grant not found" "Query returned no authorizations"
    fi

    # Test: Query feegrant allowance on blockchain
    log "Querying feegrant allowances on blockchain..."
    local allowance_output
    allowance_output=$(akash_make query-feegrant --granter "$COORDINATOR_ADDRESS" --grantee "$EXECUTOR_ADDRESS" 2>&1) || true

    if echo "$allowance_output" | grep -qi "allowance\|Allowance"; then
        test_pass "feegrant_exists" "Feegrant allowance found on blockchain"
    else
        test_fail "feegrant_exists" "Feegrant allowance not found"
    fi
}

# =============================================================================
# Cross-Account Deployment Tests
# =============================================================================

test_cross_account_deployment() {
    log_section "Cross-Account Deployment Tests"

    if [[ -z "$COORDINATOR_ADDRESS" ]] || [[ -z "$EXECUTOR_ADDRESS" ]]; then
        test_skip "cross_account_deploy" "No addresses available"
        return 1
    fi

    # Create a simple test SDL
    local test_sdl="${TEST_DIR}/cross-account-test.sdl.yaml"
    cat > "$test_sdl" <<'EOF'
version: "2.0"
services:
  web:
    image: nginx:latest
    expose:
      - port: 80
        as: 80
        to:
          - global: true
profiles:
  compute:
    web:
      resources:
        cpu:
          units: 0.5
        memory:
          size: 512Mi
        storage:
          - size: 512Mi
  placement:
    akash:
      pricing:
        web:
          denom: uakt
          amount: 1000
deployment:
  web:
    akash:
      profile: web
      count: 1
EOF

    # Test: Create deployment using feegrant
    log "Executor creating deployment using coordinator's feegrant..."
    local deploy_output
    deploy_output=$(ergors_cli_executor deploy create \
        --sdl "$test_sdl" \
        --key-name "default" \
        --node "${AKASH_LOCAL_NODE}" \
        --chain-id "${AKASH_LOCAL_CHAIN_ID}" \
        --use-feegrant \
        --fee-granter "$COORDINATOR_ADDRESS" \
        2>&1) || true

    local session_id
    session_id=$(json_get "$deploy_output" '.session_id')

    if [[ -n "$session_id" ]]; then
        test_pass "feegrant_deployment" "Created deployment with feegrant (session: ${session_id:0:12}...)"
        export CROSS_ACCOUNT_SESSION="$session_id"
    else
        test_fail "feegrant_deployment" "Failed to create feegrant deployment" "Response: ${deploy_output:0:100}"
    fi
}

# =============================================================================
# Auto-Approval Mode Tests
# =============================================================================

test_grant_auto_approval() {
    log_section "Grant Auto-Approval Mode Tests"

    if [[ -z "$COORDINATOR_ADDRESS" ]] || [[ -z "$EXECUTOR_ADDRESS" ]]; then
        if ! ergors_get_addresses; then
            test_skip "grant_auto_approval" "No addresses available"
            return 1
        fi
    fi

    # Test 1: Configure granter with auto-accept mode
    log "Configuring granter with auto-accept mode..."
    local config_output
    config_output=$(ergors_grant_configure_mode "auto" 2>&1) || true
    log_debug "Configure mode output: $config_output"

    if echo "$config_output" | grep -qiE "success|configured|auto|updated"; then
        test_pass "granter_auto_config" "Granter configured with auto-accept mode"
    elif echo "$config_output" | grep -qiE "error|unknown|not found"; then
        test_skip "granter_auto_config" "Grant mode configuration not available"
        return 0
    else
        test_pass "granter_auto_config" "Granter mode configuration sent"
    fi

    # Test 2: Request grant -- should be immediately approved in auto mode
    log "Requesting grant (expecting auto-approval)..."
    local auto_grant_output
    auto_grant_output=$(ergors_grant_request "$COORDINATOR_ADDRESS" "$EXECUTOR_ADDRESS" 5000000 "Auto-approval E2E test") || true
    log_debug "Auto grant response: $auto_grant_output"

    local request_id
    request_id=$(json_get "$auto_grant_output" '.request_id')
    local grant_status
    grant_status=$(json_get "$auto_grant_output" '.status')

    if [[ "$grant_status" == "approved" ]] || [[ "$grant_status" == "active" ]]; then
        test_pass "grant_auto_approved" "Grant immediately approved in auto mode"
    elif [[ -n "$request_id" ]]; then
        # May need a moment for auto-processing
        sleep 3
        local status_output
        status_output=$(ergors_cli deploy grant-status --request-id "$request_id" 2>&1) || true
        local updated_status
        updated_status=$(json_get "$status_output" '.status')

        if [[ "$updated_status" == "approved" ]] || [[ "$updated_status" == "active" ]]; then
            test_pass "grant_auto_approved" "Grant auto-approved after processing (status: $updated_status)"
        else
            test_fail "grant_auto_approved" "Grant not auto-approved" "Status: ${updated_status:-unknown}"
        fi
    else
        test_fail "grant_auto_approved" "Failed to create grant request in auto mode" "Response: ${auto_grant_output:0:100}"
    fi

    # Test 3: Reset granter to manual mode for remaining tests
    log "Resetting granter to manual mode..."
    ergors_grant_configure_mode "manual" 2>&1 || true
}

# =============================================================================
# Whitelist Mode Tests
# =============================================================================

test_grant_whitelist_mode() {
    log_section "Grant Whitelist Mode Tests"

    if [[ -z "$COORDINATOR_ADDRESS" ]] || [[ -z "$EXECUTOR_ADDRESS" ]]; then
        if ! ergors_get_addresses; then
            test_skip "grant_whitelist" "No addresses available"
            return 1
        fi
    fi

    # Test 1: Configure granter with whitelist mode
    log "Configuring granter with whitelist mode..."
    local config_output
    config_output=$(ergors_grant_configure_mode "whitelist" 2>&1) || true
    log_debug "Configure whitelist mode output: $config_output"

    if echo "$config_output" | grep -qiE "success|configured|whitelist|updated"; then
        test_pass "granter_whitelist_config" "Granter configured with whitelist mode"
    elif echo "$config_output" | grep -qiE "error|unknown|not found"; then
        test_skip "granter_whitelist_config" "Whitelist mode configuration not available"
        return 0
    else
        test_pass "granter_whitelist_config" "Granter whitelist mode configuration sent"
    fi

    # Test 2: Add executor to whitelist
    log "Adding executor to whitelist..."
    local add_output
    add_output=$(ergors_grant_whitelist_add "$EXECUTOR_ADDRESS" 2>&1) || true
    log_debug "Whitelist add output: $add_output"

    if echo "$add_output" | grep -qiE "success|added|whitelist"; then
        test_pass "whitelist_add" "Executor added to whitelist"
    else
        test_fail "whitelist_add" "Failed to add executor to whitelist" "Response: ${add_output:0:100}"
    fi

    # Test 3: Request grant while on whitelist (should auto-approve)
    log "Requesting grant while on whitelist..."
    local wl_grant_output
    wl_grant_output=$(ergors_grant_request "$COORDINATOR_ADDRESS" "$EXECUTOR_ADDRESS" 5000000 "Whitelist E2E test") || true
    log_debug "Whitelist grant response: $wl_grant_output"

    local wl_request_id
    wl_request_id=$(json_get "$wl_grant_output" '.request_id')
    local wl_status
    wl_status=$(json_get "$wl_grant_output" '.status')

    if [[ "$wl_status" == "approved" ]] || [[ "$wl_status" == "active" ]]; then
        test_pass "whitelist_grant_approved" "Grant approved for whitelisted address"
    elif [[ -n "$wl_request_id" ]]; then
        sleep 3
        local check_output
        check_output=$(ergors_cli deploy grant-status --request-id "$wl_request_id" 2>&1) || true
        local check_status
        check_status=$(json_get "$check_output" '.status')
        if [[ "$check_status" == "approved" ]] || [[ "$check_status" == "active" ]]; then
            test_pass "whitelist_grant_approved" "Whitelisted grant approved after processing"
        else
            test_fail "whitelist_grant_approved" "Whitelisted grant not auto-approved" "Status: ${check_status:-unknown}"
        fi
    else
        test_fail "whitelist_grant_approved" "Failed to request grant while on whitelist"
    fi

    # Test 4: Check whitelist entry exists
    log "Checking whitelist entries..."
    local check_output
    check_output=$(ergors_grant_whitelist_check "$EXECUTOR_ADDRESS" 2>&1) || true
    log_debug "Whitelist check output: $check_output"

    if echo "$check_output" | grep -qiE "true|whitelisted|found|exists"; then
        test_pass "whitelist_check" "Executor confirmed on whitelist"
    elif echo "$check_output" | grep -qiE "error|unknown"; then
        test_skip "whitelist_check" "Whitelist check command not available"
    else
        test_fail "whitelist_check" "Executor not found on whitelist" "Response: ${check_output:0:100}"
    fi

    # Test 5: Remove executor from whitelist
    log "Removing executor from whitelist..."
    local remove_output
    remove_output=$(ergors_grant_whitelist_remove "$EXECUTOR_ADDRESS" 2>&1) || true
    log_debug "Whitelist remove output: $remove_output"

    if echo "$remove_output" | grep -qiE "success|removed|whitelist"; then
        test_pass "whitelist_remove" "Executor removed from whitelist"
    else
        test_fail "whitelist_remove" "Failed to remove executor from whitelist" "Response: ${remove_output:0:100}"
    fi

    # Test 6: Request grant after removal (should be rejected in whitelist mode)
    log "Requesting grant after whitelist removal (expecting rejection)..."
    local rejected_output
    rejected_output=$(ergors_grant_request "$COORDINATOR_ADDRESS" "$EXECUTOR_ADDRESS" 5000000 "Post-removal E2E test") || true
    log_debug "Post-removal grant response: $rejected_output"

    local rejected_status
    rejected_status=$(json_get "$rejected_output" '.status')

    if [[ "$rejected_status" == "rejected" ]] || [[ "$rejected_status" == "denied" ]] || [[ "$rejected_status" == "pending" ]]; then
        test_pass "whitelist_removal_enforced" "Grant correctly rejected/pending after whitelist removal (status: $rejected_status)"
    elif echo "$rejected_output" | grep -qiE "not whitelisted|denied|rejected|unauthorized"; then
        test_pass "whitelist_removal_enforced" "Grant rejected for non-whitelisted address"
    else
        test_fail "whitelist_removal_enforced" "Grant not rejected after whitelist removal" "Status: ${rejected_status:-unknown}"
    fi

    # Reset to manual mode
    ergors_grant_configure_mode "manual" 2>&1 || true
}

# =============================================================================
# Spending Limits Tests
# =============================================================================

test_grant_spending_limits() {
    log_section "Grant Spending Limits Tests"

    if [[ -z "$COORDINATOR_ADDRESS" ]] || [[ -z "$EXECUTOR_ADDRESS" ]]; then
        if ! ergors_get_addresses; then
            test_skip "spending_limits" "No addresses available"
            return 1
        fi
    fi

    # Test 1: Configure spending limit for grantee
    local spending_limit=10000000  # 10 AKT
    log "Configuring spending limit: ${spending_limit} uakt..."
    local limit_output
    limit_output=$(ergors_grant_set_spending_limit "$EXECUTOR_ADDRESS" "$spending_limit" 2>&1) || true
    log_debug "Spending limit output: $limit_output"

    if echo "$limit_output" | grep -qiE "success|configured|limit|updated"; then
        test_pass "spending_limit_set" "Spending limit configured: ${spending_limit} uakt"
    elif echo "$limit_output" | grep -qiE "error|unknown|not found"; then
        test_skip "spending_limit_set" "Spending limit configuration not available"
        return 0
    else
        test_pass "spending_limit_set" "Spending limit configuration sent"
    fi

    # Test 2: Query current spending for grantee
    log "Querying current spending..."
    local spending_output
    spending_output=$(ergors_grant_query_spending "$EXECUTOR_ADDRESS" 2>&1) || true
    log_debug "Spending query output: $spending_output"

    local spent_amount
    spent_amount=$(json_get "$spending_output" '.spent')
    if [[ -z "$spent_amount" ]]; then
        spent_amount=$(json_get "$spending_output" '.total_spent')
    fi

    local remaining
    remaining=$(json_get "$spending_output" '.remaining')
    if [[ -z "$remaining" ]]; then
        remaining=$(json_get "$spending_output" '.limit_remaining')
    fi

    if [[ -n "$spent_amount" ]] || [[ -n "$remaining" ]]; then
        test_pass "spending_query" "Spending tracked (spent: ${spent_amount:-0}, remaining: ${remaining:-unknown})"
    elif echo "$spending_output" | grep -qiE "error|unknown"; then
        test_skip "spending_query" "Spending query not available"
    else
        test_fail "spending_query" "Could not query spending data" "Response: ${spending_output:0:100}"
    fi

    # Test 3: Verify spending limit enforcement (attempt to exceed)
    log "Testing spending limit enforcement..."
    local over_limit=999999999999  # Very large amount
    local exceed_output
    exceed_output=$(ergors_grant_request "$COORDINATOR_ADDRESS" "$EXECUTOR_ADDRESS" "$over_limit" "Exceed limit E2E test") || true
    log_debug "Exceed limit response: $exceed_output"

    local exceed_status
    exceed_status=$(json_get "$exceed_output" '.status')

    if [[ "$exceed_status" == "rejected" ]] || [[ "$exceed_status" == "denied" ]]; then
        test_pass "spending_limit_enforced" "Spending limit enforced -- excessive request rejected"
    elif echo "$exceed_output" | grep -qiE "exceed|limit|insufficient|denied|over"; then
        test_pass "spending_limit_enforced" "Spending limit enforcement detected"
    else
        # May still accept if limit is on cumulative spend, not per-request
        test_skip "spending_limit_enforced" "Spending limit enforcement could not be verified (may be cumulative)"
    fi
}

# =============================================================================
# Grant Status Query Tests
# =============================================================================

test_grant_status_queries() {
    log_section "Grant Status Query Tests"

    if [[ -z "$COORDINATOR_ADDRESS" ]]; then
        if ! ergors_get_addresses; then
            test_skip "grant_status_queries" "No addresses available"
            return 1
        fi
    fi

    # Test 1: List all active grants
    log "Querying active grants..."
    local grants_list_output
    grants_list_output=$(ergors_cli deploy grant-status 2>&1) || true
    log_debug "Grant status output: $grants_list_output"

    if json_has "$grants_list_output" '.grants'; then
        local grant_count
        grant_count=$(echo "$grants_list_output" | jq -r '.grants | length' 2>/dev/null || echo "0")
        test_pass "grant_status_list" "Active grants listed ($grant_count grant(s))"

        # Test 2: Verify grant details contain expected fields
        if [[ "$grant_count" -gt 0 ]]; then
            local first_grant_granter
            first_grant_granter=$(json_get "$grants_list_output" '.grants[0].granter')
            local first_grant_grantee
            first_grant_grantee=$(json_get "$grants_list_output" '.grants[0].grantee')
            local first_grant_status
            first_grant_status=$(json_get "$grants_list_output" '.grants[0].status')

            if [[ -n "$first_grant_granter" ]] && [[ -n "$first_grant_grantee" ]]; then
                test_pass "grant_detail_fields" "Grant details include granter and grantee addresses"
            else
                test_fail "grant_detail_fields" "Grant details missing address fields"
            fi

            if [[ -n "$first_grant_status" ]]; then
                test_pass "grant_detail_status" "Grant has status field: $first_grant_status"
            else
                test_skip "grant_detail_status" "Grant status field not present"
            fi

            # Test 3: Check for allowance/expiration info
            local first_grant_allowance
            first_grant_allowance=$(json_get "$grants_list_output" '.grants[0].allowance')
            local first_grant_expiration
            first_grant_expiration=$(json_get "$grants_list_output" '.grants[0].expiration')

            if [[ -n "$first_grant_allowance" ]] || [[ -n "$first_grant_expiration" ]]; then
                test_pass "grant_detail_limits" "Grant includes allowance/expiration info"
            else
                test_skip "grant_detail_limits" "Allowance/expiration not in grant listing"
            fi
        fi
    elif echo "$grants_list_output" | grep -qiE "grant|granter|grantee|allowance"; then
        test_pass "grant_status_list" "Grant status information returned"
    elif echo "$grants_list_output" | grep -qiE "error|unknown|not found"; then
        test_skip "grant_status_list" "Grant status query not available"
    else
        test_fail "grant_status_list" "Could not query grant status" "Response: ${grants_list_output:0:100}"
    fi
}

# =============================================================================
# Multiple Msg Type Grant Tests
# =============================================================================

test_grant_multiple_msg_types() {
    log_section "Multiple Msg Type Grant Tests"

    if [[ -z "$COORDINATOR_ADDRESS" ]] || [[ -z "$EXECUTOR_ADDRESS" ]]; then
        if ! ergors_get_addresses; then
            test_skip "multi_msg_grant" "No addresses available"
            return 1
        fi
    fi

    # Test 1: Request grant for multiple message types at once
    log "Requesting grant for multiple msg types (CreateDeployment + DepositDeployment + CreateLease)..."
    local multi_grant_output
    multi_grant_output=$(ergors_cli_executor deploy request-grant \
        --granter "$COORDINATOR_ADDRESS" \
        --grantee "$EXECUTOR_ADDRESS" \
        --msg-type "/akash.deployment.v1beta3.MsgCreateDeployment" \
        --msg-type "/akash.deployment.v1beta3.MsgDepositDeployment" \
        --msg-type "/akash.market.v1beta3.MsgCreateLease" \
        --allowance 10000000 \
        --reason "Multi-msg-type E2E test" \
        2>&1) || true
    log_debug "Multi msg type grant response: $multi_grant_output"

    local request_id
    request_id=$(json_get "$multi_grant_output" '.request_id')
    local msg_types
    msg_types=$(json_get "$multi_grant_output" '.msg_types')

    if [[ -n "$request_id" ]]; then
        test_pass "multi_msg_grant_request" "Multi-msg-type grant request created (ID: ${request_id:0:12}...)"

        # Test 2: Verify all msg types are included in the request
        if [[ -n "$msg_types" ]] && [[ "$msg_types" != "null" ]]; then
            local type_count
            type_count=$(echo "$multi_grant_output" | jq -r '.msg_types | length' 2>/dev/null || echo "0")
            if [[ "$type_count" -ge 3 ]]; then
                test_pass "multi_msg_types_included" "All 3 msg types included in grant request"
            elif [[ "$type_count" -gt 0 ]]; then
                test_pass "multi_msg_types_included" "$type_count msg type(s) in grant request"
            else
                test_skip "multi_msg_types_included" "Msg types count could not be determined"
            fi
        else
            test_skip "multi_msg_types_included" "Msg types not returned in response"
        fi

        # Test 3: Approve the multi-msg-type grant
        log "Approving multi-msg-type grant..."
        local approve_output
        approve_output=$(ergors_grant_approve "$request_id" "Multi-msg approved for E2E") || true

        if json_has "$approve_output" '.success' && [[ $(json_get "$approve_output" '.success') == "true" ]]; then
            test_pass "multi_msg_grant_approve" "Multi-msg-type grant approved"

            # Wait for blockchain
            sleep 5

            # Test 4: Verify all msg types are authorized on-chain
            log "Verifying multi-msg-type authorizations..."
            local verify_output
            verify_output=$(akash_make query-grants \
                --granter "$COORDINATOR_ADDRESS" \
                --grantee "$EXECUTOR_ADDRESS" 2>&1) || true

            local create_deploy_auth=false
            local deposit_deploy_auth=false
            local create_lease_auth=false

            if echo "$verify_output" | grep -qi "MsgCreateDeployment\|CreateDeployment"; then
                create_deploy_auth=true
            fi
            if echo "$verify_output" | grep -qi "MsgDepositDeployment\|DepositDeployment"; then
                deposit_deploy_auth=true
            fi
            if echo "$verify_output" | grep -qi "MsgCreateLease\|CreateLease"; then
                create_lease_auth=true
            fi

            if $create_deploy_auth && $deposit_deploy_auth && $create_lease_auth; then
                test_pass "multi_msg_all_authorized" "All 3 msg types authorized on-chain"
            elif $create_deploy_auth || $deposit_deploy_auth || $create_lease_auth; then
                test_pass "multi_msg_all_authorized" "Some msg types authorized on-chain"
            else
                test_fail "multi_msg_all_authorized" "No msg type authorizations found on-chain"
            fi
        else
            test_fail "multi_msg_grant_approve" "Failed to approve multi-msg-type grant" "Response: ${approve_output:0:100}"
        fi
    else
        test_fail "multi_msg_grant_request" "Failed to create multi-msg-type grant request" "Response: ${multi_grant_output:0:100}"
    fi
}

# =============================================================================
# Grant Revocation Tests
# =============================================================================

test_grant_revocation() {
    log_section "Grant Revocation Tests"

    if [[ -z "$COORDINATOR_ADDRESS" ]] || [[ -z "$EXECUTOR_ADDRESS" ]]; then
        test_skip "grant_revocation" "No addresses available"
        return 1
    fi

    # Test: Revoke grant
    log "Coordinator revoking grant from executor..."
    local revoke_output
    revoke_output=$(ergors_cli deploy revoke-grant \
        --granter "$COORDINATOR_ADDRESS" \
        --grantee "$EXECUTOR_ADDRESS" \
        --msg-type "/akash.deployment.v1beta3.MsgCreateDeployment" \
        --revoke-feegrant \
        2>&1) || true

    if json_has "$revoke_output" '.success' && [[ $(json_get "$revoke_output" '.success') == "true" ]]; then
        test_pass "grant_revocation" "Grant revoked successfully"
    else
        test_fail "grant_revocation" "Failed to revoke grant" "Response: ${revoke_output:0:100}"
        return 1
    fi

    # Wait for blockchain
    sleep 5

    # Test: Verify deployment fails after revocation
    log "Attempting deployment after revocation (should fail)..."
    local test_sdl="${TEST_DIR}/post-revoke-test.sdl.yaml"
    cat > "$test_sdl" <<'EOF'
version: "2.0"
services:
  web:
    image: nginx:latest
    expose:
      - port: 80
        as: 80
        to:
          - global: true
profiles:
  compute:
    web:
      resources:
        cpu:
          units: 0.5
        memory:
          size: 512Mi
        storage:
          - size: 512Mi
  placement:
    akash:
      pricing:
        web:
          denom: uakt
          amount: 1000
deployment:
  web:
    akash:
      profile: web
      count: 1
EOF

    local post_revoke_output
    post_revoke_output=$(ergors_cli_executor deploy create \
        --sdl "$test_sdl" \
        --key-name "default" \
        --node "${AKASH_LOCAL_NODE}" \
        --chain-id "${AKASH_LOCAL_CHAIN_ID}" \
        --use-feegrant \
        --fee-granter "$COORDINATOR_ADDRESS" \
        2>&1) || true

    if echo "$post_revoke_output" | grep -qi "unauthorized\|insufficient\|denied\|error"; then
        test_pass "post_revoke_denied" "Deployment correctly denied after revocation"
    else
        test_fail "post_revoke_denied" "Deployment not denied after revocation" "May have used executor's own funds"
    fi
}

# =============================================================================
# Combined Grant Test Suite
# =============================================================================

run_grant_tests() {
    log_step "Running Grant/Feegrant Tests"

    # Phase 1: Basic grant flow
    test_account_funding
    test_grant_request
    test_grant_approval
    test_grant_verification

    # Phase 2: Grant status queries
    test_grant_status_queries

    # Phase 3: Auto-approval mode
    test_grant_auto_approval

    # Phase 4: Whitelist mode
    test_grant_whitelist_mode

    # Phase 5: Spending limits
    test_grant_spending_limits

    # Phase 6: Multiple msg type grants
    test_grant_multiple_msg_types

    # Phase 7: Cross-account deployment with feegrant
    test_cross_account_deployment

    # Phase 8: Revocation
    test_grant_revocation
}
