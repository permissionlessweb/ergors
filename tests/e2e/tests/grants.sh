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

    test_account_funding
    test_grant_request
    test_grant_approval
    test_grant_verification
    test_cross_account_deployment
    test_grant_revocation
}
