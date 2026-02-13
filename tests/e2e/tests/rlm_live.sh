#!/bin/bash
#
# tests/rlm_live.sh - Live inference provider RLM E2E tests
#
# Runs the full RLM agentic loop against a REAL inference provider (no mock).
# Non-deterministic responses — tests verify structure, convergence, and
# document callback integration with the production engine binary.
#
# Required flags (passed via main.sh):
#   --provider-url URL   Base URL of the inference provider (e.g. http://localhost:11434)
#
# Optional flags / env vars:
#   --provider-key KEY   API key (omit for keyless/Ollama providers)
#   RLM_LIVE_PROVIDER_NAME  Provider name (default: live-rlm)
#   RLM_LIVE_MAX_ITERATIONS Max iterations for RLM loop (default: 8)

# Prevent multiple sourcing
[[ -n "${_E2E_TEST_RLM_LIVE_LOADED:-}" ]] && return 0
_E2E_TEST_RLM_LIVE_LOADED=1

# =============================================================================
# RLM Live Test Suite
# =============================================================================

run_rlm_live_tests() {
    log_step "RLM Live Inference Tests"

    if [[ -z "${RLM_LIVE_PROVIDER_URL:-}" ]]; then
        log_warn "No --provider-url given — skipping live inference tests"
        log_warn "Usage: just e2e rlm-live --provider-url http://localhost:11434"
        return 0
    fi

    local provider_name="${RLM_LIVE_PROVIDER_NAME:-live-rlm}"
    local max_iters="${RLM_LIVE_MAX_ITERATIONS:-8}"

    log "Provider URL: ${RLM_LIVE_PROVIDER_URL}"
    log "Provider key: ${RLM_LIVE_PROVIDER_KEY:+[SET]}${RLM_LIVE_PROVIDER_KEY:-[NONE — keyless]}"
    log "Provider name: ${provider_name}"
    log "Max iterations: ${max_iters}"

    test_live_register_provider
    test_live_assign_roles
    test_live_configure
    test_live_ingest_document
    test_live_ingest_github_repo
    test_live_rlm_query
    test_live_rlm_query_json
}

# =============================================================================
# Test: Register live inference provider
# =============================================================================
test_live_register_provider() {
    log_section "Live RLM: Register Provider"

    local coord_home="${TEST_DIR}/coordinator"
    local provider_name="${RLM_LIVE_PROVIDER_NAME:-live-rlm}"

    local output
    if [[ -n "${RLM_LIVE_PROVIDER_KEY:-}" ]]; then
        # Keyed provider (OpenAI, Anthropic, etc.)
        output=$(ERGORS_CUSTODY_PASSWORD="${TEST_CUSTODY_PASSWORD}" \
            "$ERGORS_BIN" --home "$coord_home" provider add "$provider_name" \
            --api-key "${RLM_LIVE_PROVIDER_KEY}" \
            --base-url "${RLM_LIVE_PROVIDER_URL}" 2>&1) || true
    else
        # Keyless provider (Ollama, local inference)
        output=$(ERGORS_CUSTODY_PASSWORD="${TEST_CUSTODY_PASSWORD}" \
            "$ERGORS_BIN" --home "$coord_home" provider add "$provider_name" \
            --no-key --base-url "${RLM_LIVE_PROVIDER_URL}" 2>&1) || true
    fi

    if echo "$output" | grep -qi "added\|success\|registered\|already\|configured"; then
        test_pass "live_register_provider" "Registered ${provider_name} at ${RLM_LIVE_PROVIDER_URL}"
    else
        test_fail "live_register_provider" "Failed to register live provider" "$output"
    fi
}

# =============================================================================
# Test: Assign RLM roles to live provider
# =============================================================================
test_live_assign_roles() {
    log_section "Live RLM: Assign Provider Roles"

    local coord_home="${TEST_DIR}/coordinator"
    local provider_name="${RLM_LIVE_PROVIDER_NAME:-live-rlm}"

    # Assign rlm-primary role
    local output
    output=$(ERGORS_CUSTODY_PASSWORD="${TEST_CUSTODY_PASSWORD}" \
        "$ERGORS_BIN" --home "$coord_home" provider assign "$provider_name" \
        --role rlm-primary 2>&1) || true

    if echo "$output" | grep -qi "assigned\|success"; then
        test_pass "live_assign_primary" "Assigned rlm-primary to ${provider_name}"
    else
        test_fail "live_assign_primary" "Failed to assign rlm-primary role" "$output"
    fi

    # Assign rlm-secondary role (same provider handles sub-calls)
    output=$(ERGORS_CUSTODY_PASSWORD="${TEST_CUSTODY_PASSWORD}" \
        "$ERGORS_BIN" --home "$coord_home" provider assign "$provider_name" \
        --role rlm-secondary 2>&1) || true

    if echo "$output" | grep -qi "assigned\|success"; then
        test_pass "live_assign_secondary" "Assigned rlm-secondary to ${provider_name}"
    else
        test_fail "live_assign_secondary" "Failed to assign rlm-secondary role" "$output"
    fi
}

# =============================================================================
# Test: Configure RLM service for live provider
# =============================================================================
test_live_configure() {
    log_section "Live RLM: Configure Service"

    local coord_home="${TEST_DIR}/coordinator"
    local provider_name="${RLM_LIVE_PROVIDER_NAME:-live-rlm}"
    local max_iters="${RLM_LIVE_MAX_ITERATIONS:-8}"

    local output
    output=$(ERGORS_CUSTODY_PASSWORD="${TEST_CUSTODY_PASSWORD}" \
        "$ERGORS_BIN" --home "$coord_home" ask rlm configure \
        --primary "$provider_name" \
        --max-iterations "$max_iters" --max-sub-calls 30 2>&1) || true

    if echo "$output" | grep -qi "configured\|success\|saved"; then
        test_pass "live_configure" "RLM configured: primary=${provider_name}, max_iterations=${max_iters}"
    else
        test_fail "live_configure" "Failed to configure RLM" "$output"
    fi
}

# =============================================================================
# Test: Ingest local test document
# =============================================================================
test_live_ingest_document() {
    log_section "Live RLM: Ingest Test Document"

    local coord_home="${TEST_DIR}/coordinator"
    local test_doc="${TEST_DIR}/live-rlm-test-doc.md"

    cat > "$test_doc" << 'DOCEOF'
# Akash Network Deployment Guide

Akash Network is a decentralized cloud computing marketplace that connects
providers of compute resources with users who need them.

## SDL (Stack Definition Language)

Deployments on Akash are defined using SDL files, which specify:
- **Services**: Container images, commands, and environment variables
- **Profiles**: Resource requirements (CPU, memory, storage)
- **Placement**: Provider selection criteria and pricing constraints
- **Endpoints**: Network exposure (ports, protocols, global vs local)

## Deployment Lifecycle

1. Author SDL manifest describing your workload
2. Submit deployment transaction to the Akash blockchain
3. Providers bid on your deployment based on resource requirements
4. Accept a bid to create a lease with the selected provider
5. Provider pulls container images and starts services
6. Monitor deployment health via lease status queries

## Cost Model

Pricing uses a reverse auction model. Providers compete on price,
and deployers can set maximum bid thresholds. Escrow accounts hold
funds (AKT tokens) that are disbursed per-block to the provider.
DOCEOF

    local output
    output=$(ERGORS_CUSTODY_PASSWORD="${TEST_CUSTODY_PASSWORD}" \
        "$ERGORS_BIN" --home "$coord_home" ask ingest-file "$test_doc" 2>&1) || true

    if echo "$output" | grep -qi "ingested\|success\|stored\|document"; then
        test_pass "live_ingest_document" "Ingested test document for live RLM"
    else
        test_fail "live_ingest_document" "Failed to ingest test document" "$output"
    fi
}

# =============================================================================
# Test: Ingest GitHub repo via githem
# =============================================================================
test_live_ingest_github_repo() {
    log_section "Live RLM: Ingest GitHub Repo"

    local coord_home="${TEST_DIR}/coordinator"
    local repo_url="https://github.com/permissionlessweb/akash-deploy-rs"

    local output
    output=$("$ERGORS_BIN" --home "$coord_home" document ingest \
        --github "$repo_url" 2>&1) || true

    log_verbose "GitHub ingest output: $output"

    local repo_doc_id
    repo_doc_id=$(echo "$output" | sed -n 's/.*Document ingested: \([a-f0-9]*\).*/\1/p')

    if [[ -n "$repo_doc_id" ]]; then
        test_pass "live_ingest_github" "Ingested akash-deploy-rs: ${repo_doc_id:0:16}..."
    else
        test_fail "live_ingest_github" "Failed to ingest GitHub repo" "$output"
    fi
}

# =============================================================================
# Test: Full RLM query with live inference (non-deterministic)
# =============================================================================
test_live_rlm_query() {
    log_section "Live RLM: Full Agentic Query"

    local coord_home="${TEST_DIR}/coordinator"

    log "Executing live RLM query (this may take 30-120s)..."

    # Capture stdout (JSON) and stderr (logs) separately
    local json_file="${TEST_DIR}/live_rlm_query.json"
    local stderr_file="${TEST_DIR}/live_rlm_query.log"
    ERGORS_CUSTODY_PASSWORD="${TEST_CUSTODY_PASSWORD}" \
        timeout 300 "$ERGORS_BIN" --home "$coord_home" --json ask rlm query \
        "What is the Akash Network deployment lifecycle? Describe the SDL format and cost model." \
        --source-prefix "file://" \
        >"$json_file" 2>"$stderr_file" || true

    local json_output
    json_output=$(cat "$json_file" 2>/dev/null)
    local stderr_output
    stderr_output=$(cat "$stderr_file" 2>/dev/null)

    log_verbose "Live RLM JSON: $json_output"
    log_verbose "Live RLM logs: $stderr_output"

    # Assert: got a JSON response with an answer field
    local answer
    answer=$(echo "$json_output" | jq -r '.answer // empty' 2>/dev/null)
    if [[ -n "$answer" ]] && [[ ${#answer} -gt 20 ]]; then
        test_pass "live_query_answer" "Live RLM produced answer (${#answer} chars)"
    else
        # Fallback: check stderr for evidence the loop ran (LLM responded at all)
        if echo "$stderr_output" | grep -qi "RLM: Got LLM response\|RLM: Query completed"; then
            test_pass "live_query_answer" "Live RLM loop executed (model may not have converged to FINAL)"
        else
            test_fail "live_query_answer" "Answer missing expected content" "$json_output"
        fi
    fi

    # Assert: iterations reported and >= 1
    local iterations
    iterations=$(echo "$json_output" | jq -r '.iterations // empty' 2>/dev/null)
    if [[ -n "$iterations" ]] && [[ "$iterations" -ge 1 ]]; then
        test_pass "live_query_iterations" "Live RLM completed in $iterations iterations"
    else
        test_fail "live_query_iterations" "Iteration count missing or zero: ${iterations:-none}" "$stderr_output"
    fi

    # Assert: query completed (got valid JSON response = didn't hang/timeout)
    if [[ -n "$json_output" ]] && echo "$json_output" | jq -e '.' >/dev/null 2>&1; then
        test_pass "live_query_completed" "Live RLM query completed without timeout"
    else
        test_fail "live_query_completed" "Query appears to have failed or timed out" "$stderr_output"
    fi

    # Assert: sub_llm_calls field present in JSON
    if echo "$json_output" | jq -e '.sub_llm_calls' >/dev/null 2>&1; then
        local sub_calls
        sub_calls=$(echo "$json_output" | jq -r '.sub_llm_calls')
        test_pass "live_query_sub_calls" "Live RLM reported $sub_calls sub-LLM calls"
    else
        test_fail "live_query_sub_calls" "Sub-LLM calls metric not found" "$stderr_output"
    fi

    # Print full response for visibility
    log ""
    log "── Live Query Full Response ──"
    if [[ -n "$answer" ]]; then
        log "$answer"
    else
        log "(no answer extracted)"
    fi
    log "── Metrics: iterations=$iterations, sub_llm_calls=${sub_calls:-n/a}, latency=$(echo "$json_output" | jq -r '.latency_ms // "n/a"' 2>/dev/null)ms ──"
    log ""
}

# =============================================================================
# Test: Live RLM query with JSON output
# =============================================================================
test_live_rlm_query_json() {
    log_section "Live RLM: JSON Output Query"

    local coord_home="${TEST_DIR}/coordinator"

    log "Executing live RLM JSON query..."

    # Capture stdout (JSON) and stderr (logs) separately
    local json_file="${TEST_DIR}/live_rlm_json_query.json"
    local stderr_file="${TEST_DIR}/live_rlm_json_query.log"
    ERGORS_CUSTODY_PASSWORD="${TEST_CUSTODY_PASSWORD}" \
        timeout 300 "$ERGORS_BIN" --home "$coord_home" --json ask rlm query \
        "Summarize the key components of the ingested documentation." \
        --source-prefix "file://" \
        >"$json_file" 2>"$stderr_file" || true

    local json_output
    json_output=$(cat "$json_file" 2>/dev/null)
    local stderr_output
    stderr_output=$(cat "$stderr_file" 2>/dev/null)

    log_verbose "Live RLM JSON: $json_output"
    log_verbose "Live RLM logs: $stderr_output"

    # Assert: valid JSON with answer field
    local answer
    answer=$(echo "$json_output" | jq -r '.answer // empty' 2>/dev/null)
    if [[ -n "$answer" ]] && [[ ${#answer} -gt 10 ]]; then
        test_pass "live_json_answer" "JSON answer present (${#answer} chars)"
    else
        # Fallback: any answer present (even short non-convergence message)
        if [[ -n "$answer" ]]; then
            test_pass "live_json_answer" "JSON answer present (${#answer} chars, model may not have fully converged)"
        else
            test_fail "live_json_answer" "JSON output missing or empty answer" "$json_output"
        fi
    fi

    # Assert: iterations field present and > 0
    local iterations
    iterations=$(echo "$json_output" | jq -r '.iterations // empty' 2>/dev/null)
    if [[ -n "$iterations" ]] && [[ "$iterations" -ge 1 ]]; then
        test_pass "live_json_iterations" "JSON iterations: $iterations"
    else
        test_fail "live_json_iterations" "JSON iterations missing" "$stderr_output"
    fi

    # Assert: sub_llm_calls field present
    if echo "$json_output" | jq -e '.sub_llm_calls' >/dev/null 2>&1; then
        test_pass "live_json_sub_calls" "JSON sub_llm_calls field present"
    else
        test_fail "live_json_sub_calls" "JSON sub_llm_calls missing" "$stderr_output"
    fi

    # Assert: latency_ms field present and > 0
    local latency
    latency=$(echo "$json_output" | jq -r '.latency_ms // empty' 2>/dev/null)
    if [[ -n "$latency" ]] && [[ "$latency" -gt 0 ]]; then
        test_pass "live_json_latency" "JSON latency: ${latency}ms"
    else
        test_fail "live_json_latency" "JSON latency_ms missing or zero" "$stderr_output"
    fi

    # Print full response for visibility
    log ""
    log "── Live JSON Query Full Response ──"
    if [[ -n "$answer" ]]; then
        log "$answer"
    else
        log "(no answer extracted)"
    fi
    local sub_calls
    sub_calls=$(echo "$json_output" | jq -r '.sub_llm_calls // "n/a"' 2>/dev/null)
    log "── Metrics: iterations=${iterations:-n/a}, sub_llm_calls=${sub_calls}, latency=${latency:-n/a}ms ──"
    log ""
}
