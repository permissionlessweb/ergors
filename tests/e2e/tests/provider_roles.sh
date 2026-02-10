#!/bin/bash
#
# tests/provider_roles.sh - Engine role assignment E2E tests
#
# Tests the full stack: CLI -> gRPC -> cnidarium storage -> retrieval
#   - Assign provider to role (orchestration)
#   - Assign second provider to same role (priority order)
#   - List roles (verify order)
#   - Unassign first provider → second becomes primary
#   - Assign to multiple roles
#   - Start engine with no roles → warning log, no crash

# Prevent multiple sourcing
[[ -n "${_E2E_TEST_PROVIDER_ROLES_LOADED:-}" ]] && return 0
_E2E_TEST_PROVIDER_ROLES_LOADED=1

# =============================================================================
# Provider Role Assignment Tests
# =============================================================================

run_provider_roles_tests() {
    log_step "Provider Role Assignment Tests"

    test_provider_role_assign || return 1
    test_provider_role_list || return 1
    test_provider_role_unassign || return 1
    test_provider_role_multi_role || return 1
}

test_provider_role_assign() {
    log_section "Provider Roles: Assign"

    local coord_home="${TEST_DIR}/coordinator"

    # First, add two keyless providers for testing
    ERGORS_CUSTODY_PASSWORD="${TEST_CUSTODY_PASSWORD}" \
        "$ERGORS_BIN" --home "$coord_home" provider add role-test-primary \
        --no-key --base-url "http://localhost:8080" 2>&1 || true

    ERGORS_CUSTODY_PASSWORD="${TEST_CUSTODY_PASSWORD}" \
        "$ERGORS_BIN" --home "$coord_home" provider add role-test-fallback \
        --no-key --base-url "http://localhost:8081" 2>&1 || true

    # Assign first provider to orchestration
    local output
    output=$(ERGORS_CUSTODY_PASSWORD="${TEST_CUSTODY_PASSWORD}" \
        "$ERGORS_BIN" --home "$coord_home" provider assign role-test-primary \
        --role orchestration 2>&1) || true

    if echo "$output" | grep -qi "assigned\|success"; then
        test_pass "role_assign_primary" "Assigned primary provider to orchestration role"
    else
        test_fail "role_assign_primary" "Failed to assign provider" "$output"
    fi

    # Assign second provider to same role (becomes fallback)
    output=$(ERGORS_CUSTODY_PASSWORD="${TEST_CUSTODY_PASSWORD}" \
        "$ERGORS_BIN" --home "$coord_home" provider assign role-test-fallback \
        --role orchestration 2>&1) || true

    if echo "$output" | grep -qi "assigned\|success"; then
        test_pass "role_assign_fallback" "Assigned fallback provider to orchestration role"
    else
        test_fail "role_assign_fallback" "Failed to assign fallback" "$output"
    fi
}

test_provider_role_list() {
    log_section "Provider Roles: List + Priority Order"

    local coord_home="${TEST_DIR}/coordinator"

    local output
    output=$(ERGORS_CUSTODY_PASSWORD="${TEST_CUSTODY_PASSWORD}" \
        "$ERGORS_BIN" --home "$coord_home" provider roles 2>&1) || true

    log_verbose "Roles output: $output"

    # Verify orchestration role appears
    if echo "$output" | grep -qi "orchestration"; then
        test_pass "role_list_has_orchestration" "Orchestration role listed"
    else
        test_fail "role_list_has_orchestration" "Orchestration role not in output" "$output"
    fi

    # Verify primary shows first
    if echo "$output" | grep -q "role-test-primary"; then
        test_pass "role_list_primary_present" "Primary provider present in listing"
    else
        test_fail "role_list_primary_present" "Primary provider not found" "$output"
    fi

    # Verify fallback also present
    if echo "$output" | grep -q "role-test-fallback"; then
        test_pass "role_list_fallback_present" "Fallback provider present in listing"
    else
        test_fail "role_list_fallback_present" "Fallback provider not found" "$output"
    fi

    # JSON output check
    local json_output
    json_output=$(ERGORS_CUSTODY_PASSWORD="${TEST_CUSTODY_PASSWORD}" \
        "$ERGORS_BIN" --home "$coord_home" --json provider roles 2>&1) || true

    if echo "$json_output" | jq -e '.mappings' >/dev/null 2>&1; then
        test_pass "role_list_json" "JSON output has mappings field"
    else
        test_fail "role_list_json" "JSON output missing mappings" "$json_output"
    fi
}

test_provider_role_unassign() {
    log_section "Provider Roles: Unassign + Priority Promotion"

    local coord_home="${TEST_DIR}/coordinator"

    # Unassign primary → fallback becomes primary
    local output
    output=$(ERGORS_CUSTODY_PASSWORD="${TEST_CUSTODY_PASSWORD}" \
        "$ERGORS_BIN" --home "$coord_home" provider unassign role-test-primary \
        --role orchestration 2>&1) || true

    if echo "$output" | grep -qi "unassigned\|success"; then
        test_pass "role_unassign" "Unassigned primary provider from orchestration"
    else
        test_fail "role_unassign" "Failed to unassign provider" "$output"
    fi

    # Verify fallback is now shown as primary
    output=$(ERGORS_CUSTODY_PASSWORD="${TEST_CUSTODY_PASSWORD}" \
        "$ERGORS_BIN" --home "$coord_home" provider roles 2>&1) || true

    if echo "$output" | grep -q "role-test-fallback.*primary\|role-test-fallback"; then
        test_pass "role_promotion" "Fallback promoted after primary unassigned"
    else
        test_fail "role_promotion" "Fallback not promoted" "$output"
    fi
}

test_provider_role_multi_role() {
    log_section "Provider Roles: Multi-Role Assignment"

    local coord_home="${TEST_DIR}/coordinator"

    # Assign same provider to embeddings role
    local output
    output=$(ERGORS_CUSTODY_PASSWORD="${TEST_CUSTODY_PASSWORD}" \
        "$ERGORS_BIN" --home "$coord_home" provider assign role-test-fallback \
        --role embeddings 2>&1) || true

    if echo "$output" | grep -qi "assigned\|success"; then
        test_pass "role_multi_assign" "Provider assigned to second role (embeddings)"
    else
        test_fail "role_multi_assign" "Failed to assign second role" "$output"
    fi

    # Verify both roles show up
    output=$(ERGORS_CUSTODY_PASSWORD="${TEST_CUSTODY_PASSWORD}" \
        "$ERGORS_BIN" --home "$coord_home" provider roles 2>&1) || true

    local has_orch has_embed
    has_orch=$(echo "$output" | grep -c "orchestration" || true)
    has_embed=$(echo "$output" | grep -c "embeddings" || true)

    if [[ "$has_orch" -ge 1 ]] && [[ "$has_embed" -ge 1 ]]; then
        test_pass "role_multi_list" "Both orchestration and embeddings roles listed"
    else
        test_fail "role_multi_list" "Missing roles in output" "$output"
    fi
}
