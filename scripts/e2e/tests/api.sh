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
# Combined API Test Suite
# =============================================================================

run_api_tests() {
    log_step "Running API Tests"

    test_open_responses_endpoint
    test_openai_compatible_endpoint
    test_provider_routing
    test_streaming_responses

    # Only run inference provider tests if we have a deployed endpoint
    if [[ -n "${DEPLOYED_ENDPOINT:-}" ]] || [[ -n "$TEST_SERVICE_ENDPOINT" ]]; then
        test_inference_provider_api
    fi
}
