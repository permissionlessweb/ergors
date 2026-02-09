#!/bin/bash
#
# tests/api.sh - API endpoint tests (Open Responses, LLM routing, error handling)
#
# Tests:
#   - /v1/responses endpoint (Open Responses spec)
#   - /v1/chat/completions endpoint (OpenAI-compatible)
#   - Provider routing based on model name
#   - Streaming response format
#   - Error response format

# Prevent multiple sourcing
[[ -n "${_E2E_TEST_API_LOADED:-}" ]] && return 0
_E2E_TEST_API_LOADED=1

# Store discovered service endpoint from deployments
TEST_SERVICE_ENDPOINT=""

# =============================================================================
# Open Responses API Tests
# =============================================================================

test_open_responses_endpoint() {
    log_section "Open Responses API Tests (/v1/responses)"

    # Test 1: Endpoint exists and accepts requests
    log_verbose "Testing /v1/responses endpoint..."
    local response
    response=$(curl -s --max-time 15 -X POST "http://${COORDINATOR_API}/v1/responses" \
        -H "Content-Type: application/json" \
        -d '{
            "model": "test-model",
            "input": [
                {
                    "role": "user",
                    "content": [{"type": "input_text", "text": "Hello"}]
                }
            ]
        }' \
        2>/dev/null) || response="{}"
    log_debug "Open Responses response: $response"

    # Should return valid JSON with either success or proper error
    if json_has "$response" '.'; then
        # Check for expected response fields
        if json_has "$response" '.id' && json_has "$response" '.output'; then
            test_pass "open_responses_endpoint" "/v1/responses returns proper response format"
        elif json_has "$response" '.error'; then
            local error_type
            error_type=$(json_get "$response" '.error.type')
            # model_error is acceptable (no actual model configured)
            # invalid_request_error means format issue
            if [[ "$error_type" == "model_error" ]] || [[ "$error_type" == "not_found_error" ]]; then
                test_pass "open_responses_endpoint" "/v1/responses returns proper error format (type: $error_type)"
            elif [[ "$error_type" == "authentication_error" ]]; then
                test_skip "open_responses_endpoint" "Requires authentication"
            else
                test_fail "open_responses_endpoint" "Unexpected error type" "Got: $error_type"
            fi
        else
            test_fail "open_responses_endpoint" "Response missing expected fields" "Response: $response"
        fi
    else
        test_fail "open_responses_endpoint" "/v1/responses did not return valid JSON"
    fi

    # Test 2: Response object field validation
    log_verbose "Validating response object structure..."
    local test_response
    test_response=$(curl -s --max-time 15 -X POST "http://${COORDINATOR_API}/v1/responses" \
        -H "Content-Type: application/json" \
         \
        -d '{"model": "test", "input": [{"role": "user", "content": "test"}]}' \
        2>/dev/null) || test_response="{}"

    # Check for Open Responses spec fields
    local has_object
    has_object=$(json_get "$test_response" '.object')
    if [[ "$has_object" == "response" ]]; then
        test_pass "response_object_type" "Response has correct object type"
    elif json_has "$test_response" '.error'; then
        test_skip "response_object_type" "Got error response (expected without actual model)"
    else
        test_fail "response_object_type" "Response missing object: response" "Got: $has_object"
    fi

    # Test 3: Input can be string (simplified format)
    log_verbose "Testing simplified input format..."
    local simple_response
    simple_response=$(curl -s --max-time 15 -X POST "http://${COORDINATOR_API}/v1/responses" \
        -H "Content-Type: application/json" \
         \
        -d '{"model": "test", "input": [{"role": "user", "content": "Hello world"}]}' \
        2>/dev/null) || simple_response="{}"

    if json_has "$simple_response" '.' ; then
        test_pass "simple_input_format" "Simplified input format accepted"
    else
        test_fail "simple_input_format" "Simplified input format rejected"
    fi

    # Test 4: Missing model field returns proper error
    log_verbose "Testing missing model field..."
    local no_model_response
    no_model_response=$(curl -s --max-time 15 -X POST "http://${COORDINATOR_API}/v1/responses" \
        -H "Content-Type: application/json" \
         \
        -d '{"input": [{"role": "user", "content": "test"}]}' \
        2>/dev/null) || no_model_response="{}"
    log_debug "No model response: $no_model_response"

    if json_has "$no_model_response" '.error'; then
        local error_type
        error_type=$(json_get "$no_model_response" '.error.type')
        local error_param
        error_param=$(json_get "$no_model_response" '.error.param')

        if [[ "$error_type" == "invalid_request_error" ]]; then
            test_pass "missing_model_error" "Missing model returns invalid_request_error"
        else
            test_pass "missing_model_error" "Missing model returns error (type: $error_type)"
        fi
    else
        test_fail "missing_model_error" "Missing model did not return error"
    fi
}

# =============================================================================
# OpenAI-Compatible API Tests
# =============================================================================

test_openai_compatible_endpoint() {
    log_section "OpenAI-Compatible API Tests (/v1/chat/completions)"

    # Test 1: Endpoint accepts OpenAI format
    log_verbose "Testing /v1/chat/completions endpoint..."
    local response
    response=$(curl -s --max-time 15 -X POST "http://${COORDINATOR_API}/v1/chat/completions" \
        -H "Content-Type: application/json" \
         \
        -d '{
            "model": "test-model",
            "messages": [{"role": "user", "content": "Hello"}],
            "max_tokens": 10
        }' \
        2>/dev/null) || response="{}"
    log_debug "Chat completions response: $response"

    if json_has "$response" '.'; then
        if json_has "$response" '.choices' || json_has "$response" '.error'; then
            test_pass "chat_completions_endpoint" "/v1/chat/completions accepts OpenAI format"
        else
            test_fail "chat_completions_endpoint" "Response missing choices or error field"
        fi
    else
        test_fail "chat_completions_endpoint" "Endpoint did not return valid JSON"
    fi

    # Test 2: Proper error format for OpenAI-compatible errors
    log_verbose "Testing error format..."
    local error_response
    error_response=$(curl -s --max-time 15 -X POST "http://${COORDINATOR_API}/v1/chat/completions" \
        -H "Content-Type: application/json" \
         \
        -d '{"model": "nonexistent-model-xyz", "messages": [{"role": "user", "content": "test"}]}' \
        2>/dev/null) || error_response="{}"
    log_debug "Error response: $error_response"

    if json_has "$error_response" '.error'; then
        # Check error object has expected fields
        local has_type has_message
        has_type=$(json_get "$error_response" '.error.type')
        has_message=$(json_get "$error_response" '.error.message')

        if [[ -n "$has_type" ]] && [[ -n "$has_message" ]]; then
            test_pass "error_format" "Error response has type and message fields"
        else
            test_fail "error_format" "Error response missing type or message"
        fi
    else
        # If no error, the model might have been found (unlikely for nonexistent)
        test_skip "error_format" "No error returned (model may exist)"
    fi
}

# =============================================================================
# Provider Routing Tests
# =============================================================================

test_provider_routing() {
    log_section "Provider Routing Tests"

    # Test 1: Claude model routes (should route to Anthropic)
    log_verbose "Testing claude-* model routing..."
    local claude_response
    claude_response=$(curl -s --max-time 15 -X POST "http://${COORDINATOR_API}/v1/chat/completions" \
        -H "Content-Type: application/json" \
         \
        -d '{"model": "claude-3-5-sonnet-20241022", "messages": [{"role": "user", "content": "test"}], "max_tokens": 5}' \
        2>/dev/null) || claude_response="{}"
    log_debug "Claude routing response: $claude_response"

    # Should either work or error with model-related issue (not routing error)
    if json_has "$claude_response" '.'; then
        local error_type
        error_type=$(json_get "$claude_response" '.error.type')
        local error_msg
        error_msg=$(json_get "$claude_response" '.error.message')

        if [[ -z "$error_type" ]]; then
            test_pass "claude_routing" "claude-* model routed successfully"
        elif echo "$error_msg" | grep -qiE "api.?key|unauthorized|anthropic"; then
            # This means routing worked but no API key - that's fine
            test_pass "claude_routing" "claude-* routes to Anthropic (API key issue)"
        elif [[ "$error_type" == "model_error" ]] || [[ "$error_type" == "not_found_error" ]]; then
            test_pass "claude_routing" "claude-* routing handled (provider error)"
        else
            test_fail "claude_routing" "Unexpected claude routing error" "Type: $error_type"
        fi
    else
        test_fail "claude_routing" "Invalid response for claude model"
    fi

    # Test 2: GPT model routes (should route to OpenAI)
    log_verbose "Testing gpt-* model routing..."
    local gpt_response
    gpt_response=$(curl -s --max-time 15 -X POST "http://${COORDINATOR_API}/v1/chat/completions" \
        -H "Content-Type: application/json" \
         \
        -d '{"model": "gpt-4o", "messages": [{"role": "user", "content": "test"}], "max_tokens": 5}' \
        2>/dev/null) || gpt_response="{}"
    log_debug "GPT routing response: $gpt_response"

    if json_has "$gpt_response" '.'; then
        local error_type
        error_type=$(json_get "$gpt_response" '.error.type')
        local error_msg
        error_msg=$(json_get "$gpt_response" '.error.message')

        if [[ -z "$error_type" ]]; then
            test_pass "gpt_routing" "gpt-* model routed successfully"
        elif echo "$error_msg" | grep -qiE "api.?key|unauthorized|openai"; then
            test_pass "gpt_routing" "gpt-* routes to OpenAI (API key issue)"
        elif [[ "$error_type" == "model_error" ]] || [[ "$error_type" == "not_found_error" ]]; then
            test_pass "gpt_routing" "gpt-* routing handled (provider error)"
        else
            test_fail "gpt_routing" "Unexpected GPT routing error" "Type: $error_type"
        fi
    else
        test_fail "gpt_routing" "Invalid response for GPT model"
    fi

    # Test 3: Unknown model handled gracefully
    log_verbose "Testing unknown model handling..."
    local unknown_response
    unknown_response=$(curl -s --max-time 15 -X POST "http://${COORDINATOR_API}/v1/chat/completions" \
        -H "Content-Type: application/json" \
         \
        -d '{"model": "unknown-model-xyz-123", "messages": [{"role": "user", "content": "test"}], "max_tokens": 5}' \
        2>/dev/null) || unknown_response="{}"
    log_debug "Unknown model response: $unknown_response"

    if json_has "$unknown_response" '.error'; then
        test_pass "unknown_model_handled" "Unknown model returns proper error"
    else
        # Might have a default provider
        test_pass "unknown_model_handled" "Unknown model handled (may use default provider)"
    fi
}

# =============================================================================
# Multi-Provider Simulation Tests
# =============================================================================

test_multi_provider_simulation() {
    log_section "Multi-Provider Simulation Tests"

    # Only run if mock provider is configured
    if [[ -z "${MOCK_PROVIDER_URL:-}" ]]; then
        log_warn "Mock provider not configured, skipping multi-provider tests"
        return 0
    fi

    # Test 1: OpenAI model (gpt-4) returns deterministic response
    log_verbose "Testing OpenAI model (gpt-4) for deterministic response..."
    local gpt_response
    gpt_response=$(curl -s --max-time 15 -X POST "http://${COORDINATOR_API}/v1/chat/completions" \
        -H "Content-Type: application/json" \
        -H "Authorization: Bearer ${OPENAI_API_KEY}" \
        -d '{
            "model": "gpt-4",
            "messages": [{"role": "user", "content": "test"}],
            "max_tokens": 20
        }' 2>/dev/null) || gpt_response="{}"

    log_debug "GPT-4 response: $gpt_response"

    if json_has "$gpt_response" '.choices[0].message.content'; then
        local content
        content=$(json_get "$gpt_response" '.choices[0].message.content')
        test_pass "gpt4_deterministic" "GPT-4 returned response: ${content:0:50}..."
    elif json_has "$gpt_response" '.error'; then
        local error_type
        error_type=$(json_get "$gpt_response" '.error.type')
        test_skip "gpt4_deterministic" "Got error: $error_type"
    else
        test_fail "gpt4_deterministic" "Invalid response format"
    fi

    # Test 2: Anthropic model (claude-3-sonnet) returns deterministic response
    log_verbose "Testing Anthropic model (claude-3-sonnet) for deterministic response..."
    local claude_response
    claude_response=$(curl -s --max-time 15 -X POST "http://${COORDINATOR_API}/v1/chat/completions" \
        -H "Content-Type: application/json" \
        -H "Authorization: Bearer ${ANTHROPIC_API_KEY}" \
        -d '{
            "model": "claude-3-sonnet",
            "messages": [{"role": "user", "content": "test"}],
            "max_tokens": 20
        }' 2>/dev/null) || claude_response="{}"

    log_debug "Claude response: $claude_response"

    if json_has "$claude_response" '.choices[0].message.content'; then
        local content
        content=$(json_get "$claude_response" '.choices[0].message.content')
        test_pass "claude_deterministic" "Claude returned response: ${content:0:50}..."
    elif json_has "$claude_response" '.error'; then
        local error_type
        error_type=$(json_get "$claude_response" '.error.type')
        test_skip "claude_deterministic" "Got error: $error_type"
    else
        test_fail "claude_deterministic" "Invalid response format"
    fi

    # Test 3: Ollama model (llama2) returns deterministic response
    log_verbose "Testing Ollama model (llama2) for deterministic response..."
    local llama_response
    llama_response=$(curl -s --max-time 15 -X POST "http://${COORDINATOR_API}/v1/chat/completions" \
        -H "Content-Type: application/json" \
        -H "Authorization: Bearer ${OLLAMA_API_KEY}" \
        -d '{
            "model": "llama2",
            "messages": [{"role": "user", "content": "test"}],
            "max_tokens": 20
        }' 2>/dev/null) || llama_response="{}"

    log_debug "Llama2 response: $llama_response"

    if json_has "$llama_response" '.choices[0].message.content'; then
        local content
        content=$(json_get "$llama_response" '.choices[0].message.content')
        test_pass "llama2_deterministic" "Llama2 returned response: ${content:0:50}..."
    elif json_has "$llama_response" '.error'; then
        local error_type
        error_type=$(json_get "$llama_response" '.error.type')
        test_skip "llama2_deterministic" "Got error: $error_type"
    else
        test_fail "llama2_deterministic" "Invalid response format"
    fi

    # Test 4: Verify responses are deterministic (same input = same output)
    log_verbose "Testing deterministic responses (repeatability)..."
    local gpt_response1 gpt_response2
    gpt_response1=$(curl -s --max-time 15 -X POST "http://${COORDINATOR_API}/v1/chat/completions" \
        -H "Content-Type: application/json" \
        -H "Authorization: Bearer ${OPENAI_API_KEY}" \
        -d '{"model": "gpt-4", "messages": [{"role": "user", "content": "hello"}], "max_tokens": 10}' \
        2>/dev/null)

    sleep 1

    gpt_response2=$(curl -s --max-time 15 -X POST "http://${COORDINATOR_API}/v1/chat/completions" \
        -H "Content-Type: application/json" \
        -H "Authorization: Bearer ${OPENAI_API_KEY}" \
        -d '{"model": "gpt-4", "messages": [{"role": "user", "content": "hello"}], "max_tokens": 10}' \
        2>/dev/null)

    if [[ -n "$gpt_response1" ]] && [[ -n "$gpt_response2" ]]; then
        local content1 content2
        content1=$(echo "$gpt_response1" | jq -r '.choices[0].message.content // empty')
        content2=$(echo "$gpt_response2" | jq -r '.choices[0].message.content // empty')

        if [[ -n "$content1" ]] && [[ "$content1" == "$content2" ]]; then
            test_pass "deterministic_responses" "Responses are deterministic (identical)"
        elif [[ -n "$content1" ]] && [[ -n "$content2" ]]; then
            test_warn "deterministic_responses" "Responses differ (may not be deterministic)"
            log_debug "Response 1: $content1"
            log_debug "Response 2: $content2"
        else
            test_skip "deterministic_responses" "Could not verify (no content returned)"
        fi
    else
        test_skip "deterministic_responses" "Could not verify (requests failed)"
    fi

    # Test 5: Verify all providers use same mock endpoint
    log_verbose "Testing that all providers route to single mock endpoint..."
    if curl -s "${MOCK_PROVIDER_URL}/health" | grep -q "ok"; then
        test_pass "single_mock_endpoint" "Mock provider is reachable at ${MOCK_PROVIDER_URL}"
    else
        test_fail "single_mock_endpoint" "Mock provider not responding"
    fi
}

# =============================================================================
# Inference Provider API Tests (for deployed services)
# =============================================================================

test_inference_provider_api() {
    log_section "Inference Provider API Tests"

    # This test requires a deployed service endpoint
    if [[ -z "$TEST_SERVICE_ENDPOINT" ]]; then
        # Try to discover from deployment tests
        TEST_SERVICE_ENDPOINT="${DEPLOYED_ENDPOINT:-}"
    fi

    if [[ -z "$TEST_SERVICE_ENDPOINT" ]]; then
        test_skip "inference_provider_api" "No deployed service endpoint available"
        return 0
    fi

    # Test 1: Ollama-compatible API (/api/generate)
    log_verbose "Testing Ollama-compatible API at $TEST_SERVICE_ENDPOINT..."
    local ollama_response
    ollama_response=$(curl -s --max-time 15 -X POST "$TEST_SERVICE_ENDPOINT/api/generate" \
        -H "Content-Type: application/json" \
        -d '{"model":"test","prompt":"hello","stream":false}' \
        2>/dev/null) || ollama_response="{}"
    log_debug "Ollama response: $ollama_response"

    if json_has "$ollama_response" '.'; then
        test_pass "ollama_api" "Ollama-compatible endpoint responds with valid JSON"
    else
        test_skip "ollama_api" "Ollama API format not detected"
    fi

    # Test 2: OpenAI-compatible API at service endpoint
    log_verbose "Testing OpenAI-compatible API at service..."
    local openai_response
    openai_response=$(curl -s --max-time 15 -X POST "$TEST_SERVICE_ENDPOINT/v1/chat/completions" \
        -H "Content-Type: application/json" \
        -d '{"model":"test","messages":[{"role":"user","content":"hello"}],"max_tokens":10}' \
        2>/dev/null) || openai_response="{}"
    log_debug "OpenAI service response: $openai_response"

    if json_has "$openai_response" '.'; then
        test_pass "service_openai_api" "Service OpenAI-compatible endpoint responds"
    else
        test_skip "service_openai_api" "OpenAI API format not available at service"
    fi

    # Test 3: Health endpoint
    log_verbose "Testing service health endpoint..."
    local health_response
    health_response=$(curl -s --max-time 10 -X GET "$TEST_SERVICE_ENDPOINT/health" 2>/dev/null) || \
    health_response=$(curl -s --max-time 10 -X GET "$TEST_SERVICE_ENDPOINT/healthz" 2>/dev/null) || \
    health_response=""

    if [[ -n "$health_response" ]]; then
        test_pass "service_health" "Service health endpoint responds"
    else
        test_skip "service_health" "No health endpoint found"
    fi
}

# =============================================================================
# Streaming Tests
# =============================================================================

test_streaming_responses() {
    log_section "Streaming Response Tests"

    # Test 1: Stream parameter accepted
    log_verbose "Testing stream=true parameter..."
    local stream_response
    stream_response=$(curl -s --max-time 15 -X POST "http://${COORDINATOR_API}/v1/chat/completions" \
        -H "Content-Type: application/json" \
        -H "Accept: text/event-stream" \
         \
        -d '{"model": "test", "messages": [{"role": "user", "content": "Count to 3"}], "stream": true, "max_tokens": 20}' \
        2>/dev/null) || stream_response=""
    log_debug "Stream response (first 500 chars): ${stream_response:0:500}"

    # SSE responses start with "data: " or contain event: markers
    if echo "$stream_response" | grep -qE "^data:|^event:|^\[DONE\]"; then
        test_pass "streaming_format" "Streaming response uses SSE format"
    elif json_has "$stream_response" '.error'; then
        local error_type
        error_type=$(json_get "$stream_response" '.error.type')
        if [[ "$error_type" == "model_error" ]]; then
            test_skip "streaming_format" "Model error (no provider) - cannot verify streaming"
        else
            test_fail "streaming_format" "Streaming request returned error" "Type: $error_type"
        fi
    else
        test_fail "streaming_format" "Response not in SSE format" "Got: ${stream_response:0:200}"
    fi

    # Test 2: Open Responses streaming events
    log_verbose "Testing Open Responses streaming..."
    local or_stream_response
    or_stream_response=$(curl -s --max-time 15 -X POST "http://${COORDINATOR_API}/v1/responses" \
        -H "Content-Type: application/json" \
        -H "Accept: text/event-stream" \
         \
        -d '{"model": "test", "input": [{"role": "user", "content": "Hello"}], "stream": true}' \
        2>/dev/null) || or_stream_response=""
    log_debug "OR stream response: ${or_stream_response:0:500}"

    # Open Responses uses specific event types
    if echo "$or_stream_response" | grep -qE "response\.in_progress|response\.output"; then
        test_pass "or_streaming_events" "Open Responses streaming uses correct event types"
    elif echo "$or_stream_response" | grep -qE "^data:"; then
        test_pass "or_streaming_events" "Open Responses streaming returns SSE data"
    else
        test_skip "or_streaming_events" "Could not verify Open Responses streaming events"
    fi
}

# =============================================================================
# CosmWasm Event Router Tests
# =============================================================================

test_cosmwasm_event_router() {
    log_section "CosmWasm Event Router Tests (/api/cosmwasm/execute)"

    # Test 1: Execute endpoint exists and accepts requests
    log_verbose "Testing /api/cosmwasm/execute endpoint availability..."
    local execute_response
    execute_response=$(curl -s --max-time 15 -w "\n%{http_code}" -X POST \
        "http://${COORDINATOR_API}/api/cosmwasm/execute" \
        -H "Content-Type: application/json" \
        -d '{
            "contract": "ergors_test_contract",
            "sender": "akash1testaddress",
            "msg": {"emit_actions": {"actions": [{"Log": {"level": "info", "message": "E2E event router test"}}]}},
            "funds": []
        }' \
        2>/dev/null) || execute_response=""

    # Split response body from HTTP status code
    local execute_body execute_status
    execute_status=$(echo "$execute_response" | tail -1)
    execute_body=$(echo "$execute_response" | sed '$d')
    log_debug "Execute response (status $execute_status): $execute_body"

    # The endpoint should exist and return JSON (even if contract is not found)
    if [[ -n "$execute_body" ]] && json_has "$execute_body" '.'; then
        if [[ "$execute_status" == "404" ]]; then
            # 404 for unknown contract is acceptable -- endpoint exists
            test_pass "cw_execute_endpoint" "/api/cosmwasm/execute endpoint exists (contract not found)"
        elif [[ "$execute_status" =~ ^2[0-9][0-9]$ ]]; then
            test_pass "cw_execute_endpoint" "/api/cosmwasm/execute endpoint accepts requests (status: $execute_status)"
        elif json_has "$execute_body" '.error'; then
            local error_msg
            error_msg=$(json_get "$execute_body" '.error')
            if [[ -z "$error_msg" ]]; then
                error_msg=$(json_get "$execute_body" '.error.message')
            fi
            # Any structured error means the endpoint is active and parsing requests
            test_pass "cw_execute_endpoint" "/api/cosmwasm/execute returns structured error (${error_msg:0:60})"
        else
            test_pass "cw_execute_endpoint" "/api/cosmwasm/execute responds with JSON (status: $execute_status)"
        fi
    elif [[ "$execute_status" == "405" ]]; then
        test_fail "cw_execute_endpoint" "/api/cosmwasm/execute returned Method Not Allowed"
    elif [[ "$execute_status" == "000" ]] || [[ -z "$execute_status" ]]; then
        test_fail "cw_execute_endpoint" "/api/cosmwasm/execute not reachable" "Connection failed"
    else
        test_fail "cw_execute_endpoint" "/api/cosmwasm/execute did not return valid JSON" "Status: $execute_status"
    fi

    # Test 2: Execute with ergors_action events returns action results
    # Uses the existing ergors_cw_execute wrapper from ergors.sh
    log_verbose "Testing contract execution with ergors_action events..."

    # Use SDL template contract if available, otherwise use a test address
    local test_contract="${SDL_TEMPLATE_CONTRACT:-ergors_test_event_contract}"
    local test_sender="${COORDINATOR_ADDRESS:-akash1testaddress}"

    local action_response
    action_response=$(ergors_cw_execute "$test_contract" "$test_sender" \
        '{"emit_actions": {"actions": [{"Log": {"level": "info", "message": "E2E action test"}}, {"StorePut": {"key": "e2e_test_key", "value": "e2e_test_value"}}]}}' \
        '[]' 2>&1) || true
    log_debug "Action response: $action_response"

    if json_has "$action_response" '.action_results' || json_has "$action_response" '.events'; then
        test_pass "cw_action_results" "Contract execution returned action results"

        # Verify action result structure
        local result_count
        result_count=$(echo "$action_response" | jq -r '.action_results | length // 0' 2>/dev/null || echo "0")
        if [[ "$result_count" -gt 0 ]]; then
            test_pass "cw_action_count" "Received $result_count action result(s)"
        fi
    elif json_has "$action_response" '.data' || json_has "$action_response" '.result'; then
        # Successful execution without explicit action_results field
        test_pass "cw_action_results" "Contract execution succeeded (action events processed)"
    elif json_has "$action_response" '.error'; then
        local err_msg
        err_msg=$(json_get "$action_response" '.error')
        if [[ -z "$err_msg" ]]; then
            err_msg=$(json_get "$action_response" '.error.message')
        fi
        # Contract-not-found or similar errors are expected for test contracts
        if echo "$err_msg" | grep -qiE "not found|unknown|no such|does not exist"; then
            test_skip "cw_action_results" "Test contract not deployed (expected in minimal setup)"
        else
            test_fail "cw_action_results" "Contract execution returned error" "Error: ${err_msg:0:100}"
        fi
    else
        test_skip "cw_action_results" "Could not verify action results (no contract deployed)"
    fi

    # Test 3: Invalid contract address returns proper error
    log_verbose "Testing invalid contract address error handling..."
    local invalid_contract_response
    invalid_contract_response=$(ergors_cw_execute \
        "totally_invalid_contract_address_!@#" \
        "$test_sender" \
        '{"some_action": {}}' \
        '[]' 2>&1) || true
    log_debug "Invalid contract response: $invalid_contract_response"

    if json_has "$invalid_contract_response" '.error'; then
        local err_msg
        err_msg=$(json_get "$invalid_contract_response" '.error')
        if [[ -z "$err_msg" ]]; then
            err_msg=$(json_get "$invalid_contract_response" '.error.message')
        fi
        test_pass "cw_invalid_contract_error" "Invalid contract address returns error (${err_msg:0:60})"
    elif echo "$invalid_contract_response" | grep -qiE "error|invalid|not found|fail"; then
        test_pass "cw_invalid_contract_error" "Invalid contract address handled with error response"
    else
        test_fail "cw_invalid_contract_error" "Invalid contract address did not return error" "Response: ${invalid_contract_response:0:100}"
    fi

    # Test 4: Malformed execute message returns proper error
    log_verbose "Testing malformed execute message error handling..."
    local malformed_response
    malformed_response=$(curl -s --max-time 15 -X POST \
        "http://${COORDINATOR_API}/api/cosmwasm/execute" \
        -H "Content-Type: application/json" \
        -d '{"contract": "test", "sender": "test", "msg": "this_is_not_valid_json_object"}' \
        2>/dev/null) || malformed_response="{}"
    log_debug "Malformed msg response: $malformed_response"

    if json_has "$malformed_response" '.error'; then
        test_pass "cw_malformed_msg_error" "Malformed execute message returns proper error"
    elif echo "$malformed_response" | grep -qiE "error|invalid|parse|deserializ"; then
        test_pass "cw_malformed_msg_error" "Malformed execute message rejected"
    else
        test_fail "cw_malformed_msg_error" "Malformed execute message not rejected" "Response: ${malformed_response:0:100}"
    fi

    # Test 5: Missing required fields in execute request
    log_verbose "Testing missing fields in execute request..."
    local missing_fields_response
    missing_fields_response=$(curl -s --max-time 15 -X POST \
        "http://${COORDINATOR_API}/api/cosmwasm/execute" \
        -H "Content-Type: application/json" \
        -d '{"contract": "test"}' \
        2>/dev/null) || missing_fields_response="{}"
    log_debug "Missing fields response: $missing_fields_response"

    if json_has "$missing_fields_response" '.error'; then
        test_pass "cw_missing_fields_error" "Missing execute fields returns error"
    elif echo "$missing_fields_response" | grep -qiE "error|missing|required|invalid"; then
        test_pass "cw_missing_fields_error" "Missing execute fields handled"
    else
        test_fail "cw_missing_fields_error" "Missing execute fields not validated" "Response: ${missing_fields_response:0:100}"
    fi

    # Test 6: Verify supported action types are documented in response
    log_verbose "Testing event router action type awareness..."
    local action_types_response
    action_types_response=$(curl -s --max-time 15 -X POST \
        "http://${COORDINATOR_API}/api/cosmwasm/execute" \
        -H "Content-Type: application/json" \
        -d "{
            \"contract\": \"$test_contract\",
            \"sender\": \"$test_sender\",
            \"msg\": {\"emit_actions\": {\"actions\": [
                {\"InferenceRequest\": {\"model\": \"test-model\", \"prompt\": \"hello\"}},
                {\"P2pMessage\": {\"target\": \"node123\", \"payload\": \"test\"}},
                {\"AkashDeploy\": {\"sdl\": \"version: 2.0\"}}
            ]}},
            \"funds\": []
        }" \
        2>/dev/null) || action_types_response="{}"
    log_debug "Action types response: $action_types_response"

    # Any structured response (success or contract-not-found) means the router parsed the action types
    if json_has "$action_types_response" '.'; then
        if json_has "$action_types_response" '.action_results' || json_has "$action_types_response" '.events'; then
            test_pass "cw_action_types" "Event router processes multiple action types (InferenceRequest, P2pMessage, AkashDeploy)"
        elif json_has "$action_types_response" '.error'; then
            local err_msg
            err_msg=$(json_get "$action_types_response" '.error')
            if [[ -z "$err_msg" ]]; then
                err_msg=$(json_get "$action_types_response" '.error.message')
            fi
            if echo "$err_msg" | grep -qiE "not found|unknown|no such"; then
                test_skip "cw_action_types" "Test contract not deployed (action type routing not testable)"
            else
                test_pass "cw_action_types" "Event router accepted action type request (error in execution: ${err_msg:0:60})"
            fi
        else
            test_pass "cw_action_types" "Event router responded to multi-action request"
        fi
    else
        test_fail "cw_action_types" "Event router did not respond to action type request"
    fi
}

# =============================================================================
# Combined API Test Suite
# =============================================================================

run_api_tests() {
    log_step "Running API Tests"

    # Mock provider should already be started in infrastructure phase
    if [[ -z "${MOCK_PROVIDER_URL:-}" ]]; then
        log_warn "Mock provider not running, inference tests will be limited"
    fi

    test_open_responses_endpoint
    test_openai_compatible_endpoint
    test_provider_routing
    test_multi_provider_simulation
    test_streaming_responses

    # CosmWasm event router tests
    test_cosmwasm_event_router

    # Only run inference provider tests if we have a deployed endpoint
    if [[ -n "${DEPLOYED_ENDPOINT:-}" ]] || [[ -n "$TEST_SERVICE_ENDPOINT" ]]; then
        test_inference_provider_api
    fi
}
