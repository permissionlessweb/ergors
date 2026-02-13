#!/bin/bash
#
# tests/rlm.sh - RLM (Recursive Language Model) agentic loop E2E tests
#
# Tests the full RLM stack: mock provider → gRPC → RlmService → Python REPL
# → sub-LLM calls → FINAL convergence
#
# Requires: mock provider (RLM_MODE=true), ERGORS network

# Prevent multiple sourcing
[[ -n "${_E2E_TEST_RLM_LOADED:-}" ]] && return 0
_E2E_TEST_RLM_LOADED=1

# =============================================================================
# RLM Test Suite
# =============================================================================

run_rlm_tests() {
    log_step "RLM Agentic Loop Tests"

    test_rlm_register_provider
    test_rlm_assign_roles
    test_rlm_configure
    test_rlm_ingest_document
    test_rlm_ingest_github_repo
    test_rlm_query_basic
    test_rlm_query_json_output
    test_rlm_query_convergence
}

# =============================================================================
# Test: Register mock provider for RLM
# =============================================================================
test_rlm_register_provider() {
    log_section "RLM: Register Mock Provider"

    local coord_home="${TEST_DIR}/coordinator"

    local output
    output=$(ERGORS_CUSTODY_PASSWORD="${TEST_CUSTODY_PASSWORD}" \
        "$ERGORS_BIN" --home "$coord_home" provider add rlm-mock \
        --no-key --base-url "${MOCK_PROVIDER_URL}" 2>&1) || true

    if echo "$output" | grep -qi "added\|success\|registered\|already\|configured"; then
        test_pass "rlm_register_provider" "Registered rlm-mock provider"
    else
        test_fail "rlm_register_provider" "Failed to register rlm-mock provider" "$output"
    fi
}

# =============================================================================
# Test: Assign RLM roles to provider
# =============================================================================
test_rlm_assign_roles() {
    log_section "RLM: Assign Provider Roles"

    local coord_home="${TEST_DIR}/coordinator"

    # Assign rlm-primary role
    local output
    output=$(ERGORS_CUSTODY_PASSWORD="${TEST_CUSTODY_PASSWORD}" \
        "$ERGORS_BIN" --home "$coord_home" provider assign rlm-mock \
        --role rlm-primary 2>&1) || true

    if echo "$output" | grep -qi "assigned\|success"; then
        test_pass "rlm_assign_primary" "Assigned rlm-primary role to rlm-mock"
    else
        test_fail "rlm_assign_primary" "Failed to assign rlm-primary role" "$output"
    fi

    # Assign rlm-secondary role
    output=$(ERGORS_CUSTODY_PASSWORD="${TEST_CUSTODY_PASSWORD}" \
        "$ERGORS_BIN" --home "$coord_home" provider assign rlm-mock \
        --role rlm-secondary 2>&1) || true

    if echo "$output" | grep -qi "assigned\|success"; then
        test_pass "rlm_assign_secondary" "Assigned rlm-secondary role to rlm-mock"
    else
        test_fail "rlm_assign_secondary" "Failed to assign rlm-secondary role" "$output"
    fi
}

# =============================================================================
# Test: Configure RLM service
# =============================================================================
test_rlm_configure() {
    log_section "RLM: Configure Service"

    local coord_home="${TEST_DIR}/coordinator"

    local output
    output=$(ERGORS_CUSTODY_PASSWORD="${TEST_CUSTODY_PASSWORD}" \
        "$ERGORS_BIN" --home "$coord_home" ask rlm configure \
        --primary rlm-mock --max-iterations 5 --max-sub-calls 20 2>&1) || true

    if echo "$output" | grep -qi "configured\|success\|saved"; then
        test_pass "rlm_configure" "RLM configured with primary=rlm-mock, max_iterations=5"
    else
        test_fail "rlm_configure" "Failed to configure RLM" "$output"
    fi
}

# =============================================================================
# Test: Ingest test document for RLM document callbacks
# =============================================================================
test_rlm_ingest_document() {
    log_section "RLM: Ingest Test Document"

    local coord_home="${TEST_DIR}/coordinator"
    local test_doc="${TEST_DIR}/rlm-test-doc.md"

    # Create test document with searchable keywords
    cat > "$test_doc" << 'DOCEOF'
# ERGORS System Architecture

The ERGORS engine is a modular distributed system designed for orchestrating
inference workflows across multiple LLM providers.

## Core Components

- **Orchestration Layer**: Manages workflow execution and task distribution
- **Inference Router**: Routes requests to appropriate LLM providers based on
  model patterns, role assignments, and provider capabilities
- **Document Storage**: RAG-capable document management using cnidarium for
  persistent storage with RocksDB snapshot isolation
- **gRPC Gateway**: Inter-service communication protocol supporting both
  streaming and unary RPC patterns

## Architecture Principles

The system uses a dual-registry architecture for provider management:
ProxyRouter for persistent HTTP proxy routing and LlmRouter for in-memory
engine-internal calls. This separation ensures restart resilience while
maintaining low-latency inference routing.
DOCEOF

    local output
    output=$(ERGORS_CUSTODY_PASSWORD="${TEST_CUSTODY_PASSWORD}" \
        "$ERGORS_BIN" --home "$coord_home" ask ingest-file "$test_doc" 2>&1) || true

    if echo "$output" | grep -qi "ingested\|success\|stored\|document"; then
        test_pass "rlm_ingest_document" "Ingested test document for RLM"
    else
        test_fail "rlm_ingest_document" "Failed to ingest test document" "$output"
    fi
}

# =============================================================================
# Test: Ingest GitHub repo via githem for RLM document callbacks
# =============================================================================
test_rlm_ingest_github_repo() {
    log_section "RLM: Ingest GitHub Repo (githem)"

    local coord_home="${TEST_DIR}/coordinator"
    local repo_url="https://github.com/permissionlessweb/akash-deploy-rs"

    # Ingest via githem — clones repo, filters files, stores consolidated
    # document to DocumentStorage (content-addressed, single entry)
    local output
    output=$("$ERGORS_BIN" --home "$coord_home" document ingest \
        --github "$repo_url" 2>&1) || true

    log_verbose "GitHub ingest output: $output"

    # Extract DocumentId (64-char hex blake3 hash)
    local repo_doc_id
    repo_doc_id=$(echo "$output" | sed -n 's/.*Document ingested: \([a-f0-9]*\).*/\1/p')

    if [[ -n "$repo_doc_id" ]]; then
        test_pass "rlm_ingest_github" "Ingested akash-deploy-rs: ${repo_doc_id:0:16}..."
    else
        test_fail "rlm_ingest_github" "Failed to ingest GitHub repo" "$output"
        return
    fi

    # Verify document appears in document list
    local list_output
    list_output=$("$ERGORS_BIN" --home "$coord_home" document list 2>&1) || true

    if echo "$list_output" | grep -q "$repo_doc_id"; then
        test_pass "rlm_github_listed" "GitHub document visible in document list"
    else
        test_fail "rlm_github_listed" "Document not found in list" "$list_output"
        return
    fi

    # Verify content contains expected repo artifacts (githem curated output)
    local content_output
    content_output=$("$ERGORS_BIN" --home "$coord_home" document get \
        "$repo_doc_id" 2>&1 | head -200) || true

    if echo "$content_output" | grep -qi "akash\|deploy\|Cargo.toml\|SDL\|manifest"; then
        test_pass "rlm_github_content" "GitHub document contains expected repo content"
    else
        test_fail "rlm_github_content" "Content missing expected keywords" \
            "$(echo "$content_output" | head -20)"
    fi
}

# =============================================================================
# Test: Basic RLM query (full agentic loop)
# =============================================================================
test_rlm_query_basic() {
    log_section "RLM: Basic Query (Full Agentic Loop)"

    local coord_home="${TEST_DIR}/coordinator"

    # Reset mock provider state
    curl -s "${MOCK_PROVIDER_URL}/debug/rlm-reset" -X POST >/dev/null 2>&1 || true

    # Execute RLM query
    local output
    output=$(ERGORS_CUSTODY_PASSWORD="${TEST_CUSTODY_PASSWORD}" \
        "$ERGORS_BIN" --home "$coord_home" ask rlm query \
        "What is the system architecture?" --source-prefix "file://" 2>&1) || true

    log_verbose "RLM query output: $output"

    # Assert: answer contains architecture keywords
    if echo "$output" | grep -qi "architecture\|ERGORS\|modular\|distributed"; then
        test_pass "rlm_query_answer" "RLM answer contains architecture keywords"
    else
        test_fail "rlm_query_answer" "RLM answer missing expected keywords" "$output"
    fi

    # Assert: iterations >= 2 (discover docs, search, then FINAL)
    local iterations
    iterations=$(echo "$output" | grep -oi "iterations:[ ]*[0-9]*" | grep -o '[0-9]*' | head -1)
    if [[ -n "$iterations" ]] && [[ "$iterations" -ge 2 ]] && [[ "$iterations" -le 5 ]]; then
        test_pass "rlm_query_iterations" "RLM completed in $iterations iterations (expected 2-5)"
    else
        test_fail "rlm_query_iterations" "Unexpected iteration count: ${iterations:-none}" "$output"
    fi

    # Assert: sub-LLM calls field present (>= 0; document callbacks don't count as sub-LLM calls,
    # only sandbox llm_query() calls increment this counter)
    local sub_calls
    sub_calls=$(echo "$output" | grep -oi "sub.*call[s]*:[ ]*[0-9]*" | grep -o '[0-9]*' | head -1)
    if [[ -n "$sub_calls" ]]; then
        test_pass "rlm_query_sub_calls" "RLM reported $sub_calls sub-LLM calls"
    else
        test_fail "rlm_query_sub_calls" "Sub-LLM calls metric not found in output" "$output"
    fi

    # Assert: output shows document discovery happened (from mock iteration 1 output)
    if echo "$output" | grep -qi "document\|found\|ingested"; then
        test_pass "rlm_query_doc_access" "RLM exercised document access callbacks"
    else
        test_fail "rlm_query_doc_access" "No evidence of document access in output" "$output"
    fi
}

# =============================================================================
# Test: JSON output format
# =============================================================================
test_rlm_query_json_output() {
    log_section "RLM: JSON Output Format"

    local coord_home="${TEST_DIR}/coordinator"

    # Reset mock provider state
    curl -s "${MOCK_PROVIDER_URL}/debug/rlm-reset" -X POST >/dev/null 2>&1 || true

    # Execute with --json flag — separate stdout (JSON) from stderr (logs)
    local json_file="${TEST_DIR}/rlm_json_output.json"
    ERGORS_CUSTODY_PASSWORD="${TEST_CUSTODY_PASSWORD}" \
        "$ERGORS_BIN" --home "$coord_home" --json ask rlm query \
        "What is the system architecture?" --source-prefix "file://" \
        >"$json_file" 2>&1 || true

    local output
    output=$(cat "$json_file" 2>/dev/null)
    log_verbose "RLM JSON output: $output"

    # Verify JSON has required fields
    if echo "$output" | jq -e '.answer' >/dev/null 2>&1; then
        test_pass "rlm_json_has_answer" "JSON output has answer field"
    else
        test_fail "rlm_json_has_answer" "JSON output missing answer field" "$output"
    fi

    if echo "$output" | jq -e '.iterations' >/dev/null 2>&1; then
        test_pass "rlm_json_has_iterations" "JSON output has iterations field"
    else
        test_fail "rlm_json_has_iterations" "JSON output missing iterations field" "$output"
    fi

    if echo "$output" | jq -e '.sub_llm_calls' >/dev/null 2>&1; then
        test_pass "rlm_json_has_sub_calls" "JSON output has sub_llm_calls field"
    else
        test_fail "rlm_json_has_sub_calls" "JSON output missing sub_llm_calls field" "$output"
    fi
}

# =============================================================================
# Test: Convergence respects max_iterations limit
# =============================================================================
test_rlm_query_convergence() {
    log_section "RLM: Convergence (max_iterations limit)"

    local coord_home="${TEST_DIR}/coordinator"

    # Reconfigure with max_iterations=1 (mock FINALs on iteration 2, so this forces early stop)
    local config_output
    config_output=$(ERGORS_CUSTODY_PASSWORD="${TEST_CUSTODY_PASSWORD}" \
        "$ERGORS_BIN" --home "$coord_home" ask rlm configure \
        --primary rlm-mock --max-iterations 1 --max-sub-calls 20 2>&1) || true

    log_verbose "Reconfigure output: $config_output"

    # Reset mock provider state
    curl -s "${MOCK_PROVIDER_URL}/debug/rlm-reset" -X POST >/dev/null 2>&1 || true

    # Execute query — should complete within 1 iteration (no hang)
    # Separate stdout (JSON) from stderr (logs)
    local json_file="${TEST_DIR}/rlm_convergence.json"
    ERGORS_CUSTODY_PASSWORD="${TEST_CUSTODY_PASSWORD}" \
        "$ERGORS_BIN" --home "$coord_home" --json ask rlm query \
        "What is the system architecture?" --source-prefix "file://" \
        >"$json_file" 2>&1 || true

    local output
    output=$(cat "$json_file" 2>/dev/null)
    log_verbose "Convergence output: $output"

    # Verify query completed (got any response, didn't hang)
    if [[ -n "$output" ]]; then
        test_pass "rlm_convergence_completed" "RLM query completed with max_iterations=1"
    else
        test_fail "rlm_convergence_completed" "RLM query produced no output" ""
    fi

    # Verify iterations <= 1
    local iterations
    iterations=$(echo "$output" | jq -r '.iterations // empty' 2>/dev/null)
    if [[ -n "$iterations" ]] && [[ "$iterations" -le 1 ]]; then
        test_pass "rlm_convergence_limit" "Iteration limit respected: $iterations <= 1"
    else
        test_fail "rlm_convergence_limit" "Iteration limit not respected: ${iterations:-unknown}" "$output"
    fi

    # Restore original config for any subsequent tests
    ERGORS_CUSTODY_PASSWORD="${TEST_CUSTODY_PASSWORD}" \
        "$ERGORS_BIN" --home "$coord_home" ask rlm configure \
        --primary rlm-mock --max-iterations 5 --max-sub-calls 20 2>&1 >/dev/null || true
}
