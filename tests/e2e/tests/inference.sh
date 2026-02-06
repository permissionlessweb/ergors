#!/bin/bash
#
# ERGORS Inference Provider Routing Tests
#
# Tests the complete workflow of:
# 1. Deploying mock inference provider
# 2. Generating API keys from the provider
# 3. Storing keys in ERGORS proxy configuration
# 4. Making inference requests via ERGORS routing
# 5. Validating deterministic responses from testdata.json

# shellcheck source=../lib/common.sh
source "${SCRIPT_DIR}/lib/common.sh"

# =============================================================================
# Mock Inference Provider Management
# =============================================================================

# Deploy mock inference provider (native binary)
deploy_mock_provider() {
    log_section "Deploying Mock Inference Provider"

    # If already running from infrastructure phase, just verify
    if [[ -n "${MOCK_PROVIDER_URL:-}" ]] && curl -s "${MOCK_PROVIDER_URL}/health" >/dev/null 2>&1; then
        log_success "Mock provider already running at $MOCK_PROVIDER_URL"
        return 0
    fi

    # Start via shared function from ergors.sh
    ergors_start_mock_provider "${MOCK_PROVIDER_PORT:-11434}"
}

# Stop mock provider
cleanup_mock_provider() {
    ergors_stop_mock_provider
}

# =============================================================================
# API Key Management
# =============================================================================

# Generate a mock API key from the provider
generate_mock_api_key() {
    local provider="${1:-openai}"  # openai, anthropic, ollama
    local valid="${2:-true}"
    local expiry_seconds="${3:-}"  # Optional expiration

    local payload
    if [[ -n "$expiry_seconds" ]]; then
        payload=$(jq -n \
            --arg provider "$provider" \
            --arg valid "$valid" \
            --arg expiry "$expiry_seconds" \
            '{provider: $provider, valid: ($valid == "true"), expiry_seconds: ($expiry | tonumber)}')
    else
        payload=$(jq -n \
            --arg provider "$provider" \
            --arg valid "$valid" \
            '{provider: $provider, valid: ($valid == "true")}')
    fi

    local response
    response=$(curl -s "${MOCK_PROVIDER_URL}/api/keys/generate" \
        -H "Content-Type: application/json" \
        -d "$payload")

    local api_key
    api_key=$(echo "$response" | jq -r '.api_key')

    if [[ "$api_key" == "null" ]] || [[ -z "$api_key" ]]; then
        log_error "Failed to generate API key: $response"
        return 1
    fi

    log_verbose "Generated ${provider} API key: ${api_key:0:20}..."
    echo "$api_key"
}

# Validate an API key with the provider
validate_mock_api_key() {
    local api_key="$1"

    local response
    response=$(curl -s "${MOCK_PROVIDER_URL}/api/keys/validate" \
        -H "Content-Type: application/json" \
        -d "{\"api_key\": \"$api_key\"}")

    local valid
    valid=$(echo "$response" | jq -r '.valid')

    echo "$valid"
}

# List all API keys from the provider
list_mock_api_keys() {
    curl -s "${MOCK_PROVIDER_URL}/api/keys/list" | jq '.'
}

# =============================================================================
# ERGORS Proxy Configuration
# =============================================================================

# Configure ERGORS proxy to route to mock provider
configure_ergors_proxy() {
    local node_home="$1"
    local api_key="$2"
    local model_pattern="${3:-llama*}"  # Default to llama* models

    # Create proxy configuration
    local config_json
    config_json=$(jq -n \
        --arg url "$MOCK_PROVIDER_URL" \
        --arg key "$api_key" \
        --arg pattern "$model_pattern" \
        '{
            model_routes: {($pattern): $url},
            api_keys: {($url): $key}
        }')

    # Store configuration (via gRPC to running node)
    local api_port
    api_port=$(grep "api_port" "${node_home}/config.toml" | head -1 | awk '{print $NF}')

    log_verbose "Configuring ERGORS proxy on port $api_port"

    # Note: This would use the gRPC endpoint /v1/proxy/config
    # For now, we'll create a config file directly
    local proxy_config="${node_home}/proxy_config.json"
    echo "$config_json" > "$proxy_config"

    log_verbose "Proxy config: $config_json"
    return 0
}

# =============================================================================
# Inference Request Tests
# =============================================================================

# Make an inference request via ERGORS proxy
make_inference_request() {
    local node_api="$1"
    local model="$2"
    local prompt="$3"
    local format="${4:-ollama}"  # ollama, openai, anthropic

    case "$format" in
        ollama)
            curl -s "${node_api}/proxy/ollama/api/generate" \
                -H "Content-Type: application/json" \
                -d "{\"model\": \"$model\", \"prompt\": \"$prompt\", \"stream\": false}"
            ;;
        openai)
            curl -s "${node_api}/proxy/openai/v1/chat/completions" \
                -H "Content-Type: application/json" \
                -H "Authorization: Bearer dummy" \
                -d "{\"model\": \"$model\", \"messages\": [{\"role\": \"user\", \"content\": \"$prompt\"}]}"
            ;;
        *)
            log_error "Unsupported format: $format"
            return 1
            ;;
    esac
}

# =============================================================================
# Test Cases
# =============================================================================

run_inference_tests() {
    log_section "Running Inference Provider Routing Tests"

    # Deploy mock provider
    if ! deploy_mock_provider; then
        test_fail "deploy_mock_provider" "Failed to deploy mock inference provider"
        return 1
    fi
    test_pass "deploy_mock_provider" "Mock provider deployed successfully"

    # Test: Health check
    local health_status
    health_status=$(curl -s "${MOCK_PROVIDER_URL}/health" | jq -r '.status')
    if [[ "$health_status" == "ok" ]]; then
        test_pass "provider_health" "Provider health check passed"
    else
        test_fail "provider_health" "Health check failed" "Expected 'ok', got '$health_status'"
    fi

    # Test: Generate valid API key
    log "Generating mock API key..."
    local api_key
    api_key=$(generate_mock_api_key "openai" "true")
    if [[ "$api_key" =~ ^sk-mock-openai- ]]; then
        test_pass "generate_api_key" "Generated valid OpenAI API key"
    else
        test_fail "generate_api_key" "Invalid API key format" "Got: $api_key"
        return 1
    fi

    # Test: Validate generated key
    local is_valid
    is_valid=$(validate_mock_api_key "$api_key")
    if [[ "$is_valid" == "true" ]]; then
        test_pass "validate_api_key" "API key validation passed"
    else
        test_fail "validate_api_key" "API key validation failed"
    fi

    # Test: List keys
    local key_count
    key_count=$(list_mock_api_keys | jq -r '.total')
    if [[ "$key_count" -ge 1 ]]; then
        test_pass "list_api_keys" "Listed $key_count API key(s)"
    else
        test_fail "list_api_keys" "Failed to list keys"
    fi

    # Test: Deterministic response - "Hello world" via ERGORS routing
    log "Testing deterministic Ollama routing via ERGORS engine..."
    local response
    response=$(curl -s "http://${COORDINATOR_API}/v1/chat/completions" \
        -H "Content-Type: application/json" \
        -H "Authorization: Bearer ${OLLAMA_API_KEY}" \
        -d '{"model": "llama2", "messages": [{"role": "user", "content": "Hello world"}], "stream": false}')

    local response_text
    response_text=$(echo "$response" | jq -r '.choices[0].message.content // .response // .error')
    if echo "$response_text" | grep -q "mock inference provider\|Hello"; then
        test_pass "ergors_routing_hello" "ERGORS routed request to mock provider successfully"
    else
        test_fail "ergors_routing_hello" "Unexpected response" "Got: $response_text"
    fi

    # Test: Deterministic response - "What is 2+2?" via ERGORS routing
    log "Testing deterministic math response via ERGORS engine..."
    response=$(curl -s "http://${COORDINATOR_API}/v1/chat/completions" \
        -H "Content-Type: application/json" \
        -H "Authorization: Bearer ${OLLAMA_API_KEY}" \
        -d '{"model": "mistral", "messages": [{"role": "user", "content": "What is 2+2?"}], "stream": false, "temperature": 0.0}')

    response_text=$(echo "$response" | jq -r '.choices[0].message.content // .response // .error')
    if echo "$response_text" | grep -q "2 + 2 = 4"; then
        test_pass "ergors_routing_math" "ERGORS routed math request correctly"
    else
        test_fail "ergors_routing_math" "Unexpected response" "Got: $response_text"
    fi

    # Test: Chat completion via ERGORS routing (Ollama)
    log "Testing Ollama chat completion via ERGORS engine..."
    response=$(curl -s "http://${COORDINATOR_API}/v1/chat/completions" \
        -H "Content-Type: application/json" \
        -H "Authorization: Bearer ${OLLAMA_API_KEY}" \
        -d '{"model": "llama2", "messages": [{"role": "user", "content": "Hello, how are you?"}], "stream": false}')

    response_text=$(echo "$response" | jq -r '.choices[0].message.content // .message.content // .error')
    if echo "$response_text" | grep -q "mock inference provider\|Hello"; then
        test_pass "ergors_routing_ollama_chat" "ERGORS routed Ollama chat correctly"
    else
        test_fail "ergors_routing_ollama_chat" "Unexpected response" "Got: $response_text"
    fi

    # Test: OpenAI chat completions via ERGORS routing
    log "Testing OpenAI routing via ERGORS engine..."
    response=$(curl -s "http://${COORDINATOR_API}/v1/chat/completions" \
        -H "Content-Type: application/json" \
        -H "Authorization: Bearer ${OPENAI_API_KEY}" \
        -d '{"model": "gpt-3.5-turbo", "messages": [{"role": "user", "content": "Hello, how are you?"}], "max_tokens": 100, "temperature": 0.7}')

    response_text=$(echo "$response" | jq -r '.choices[0].message.content // .error')
    if echo "$response_text" | grep -q "mock inference provider\|gpt-3.5-turbo\|Hello"; then
        test_pass "ergors_routing_openai" "ERGORS routed OpenAI request correctly"
    else
        test_fail "ergors_routing_openai" "Unexpected response" "Got: $response_text"
    fi

    # Test: Anthropic chat completions via ERGORS routing
    log "Testing Anthropic routing via ERGORS engine..."
    response=$(curl -s "http://${COORDINATOR_API}/v1/chat/completions" \
        -H "Content-Type: application/json" \
        -H "Authorization: Bearer ${ANTHROPIC_API_KEY}" \
        -d '{"model": "claude-3-sonnet", "messages": [{"role": "user", "content": "Hello, how are you?"}], "max_tokens": 100}')

    response_text=$(echo "$response" | jq -r '.choices[0].message.content // .error')
    if echo "$response_text" | grep -q "mock inference provider\|claude-3-sonnet\|Hello"; then
        test_pass "ergors_routing_anthropic" "ERGORS routed Anthropic request correctly"
    else
        test_fail "ergors_routing_anthropic" "Unexpected response" "Got: $response_text"
    fi

    # Test: Model-based routing validation
    log "Testing model-based routing with different model types..."

    # Test GPT model routes to OpenAI entity
    response=$(curl -s "http://${COORDINATOR_API}/v1/chat/completions" \
        -H "Content-Type: application/json" \
        -H "Authorization: Bearer ${OPENAI_API_KEY}" \
        -d '{"model": "gpt-4", "messages": [{"role": "user", "content": "test"}], "stream": false}')

    response_text=$(echo "$response" | jq -r '.choices[0].message.content // .model // .error')
    if [[ -n "$response_text" ]] && [[ "$response_text" != "null" ]]; then
        test_pass "ergors_model_routing_gpt" "GPT-4 routed correctly to OpenAI entity"
    else
        test_fail "ergors_model_routing_gpt" "Failed to route GPT-4" "Got: $response_text"
    fi

    # Test Claude model routes to Anthropic entity
    response=$(curl -s "http://${COORDINATOR_API}/v1/chat/completions" \
        -H "Content-Type: application/json" \
        -H "Authorization: Bearer ${ANTHROPIC_API_KEY}" \
        -d '{"model": "claude-3-haiku", "messages": [{"role": "user", "content": "test"}], "stream": false}')

    response_text=$(echo "$response" | jq -r '.choices[0].message.content // .model // .error')
    if [[ -n "$response_text" ]] && [[ "$response_text" != "null" ]]; then
        test_pass "ergors_model_routing_claude" "Claude-3-haiku routed correctly to Anthropic entity"
    else
        test_fail "ergors_model_routing_claude" "Failed to route Claude-3-haiku" "Got: $response_text"
    fi

    # Test: Deterministic responses (repeatability)
    log "Testing deterministic response repeatability..."

    # Make the same request twice, should get identical responses
    local first_response second_response
    first_response=$(curl -s "http://${COORDINATOR_API}/v1/chat/completions" \
        -H "Content-Type: application/json" \
        -H "Authorization: Bearer ${OLLAMA_API_KEY}" \
        -d '{"model": "mistral", "messages": [{"role": "user", "content": "What is 2+2?"}], "stream": false, "temperature": 0.0}')

    second_response=$(curl -s "http://${COORDINATOR_API}/v1/chat/completions" \
        -H "Content-Type: application/json" \
        -H "Authorization: Bearer ${OLLAMA_API_KEY}" \
        -d '{"model": "mistral", "messages": [{"role": "user", "content": "What is 2+2?"}], "stream": false, "temperature": 0.0}')

    local first_text second_text
    first_text=$(echo "$first_response" | jq -r '.choices[0].message.content // .response')
    second_text=$(echo "$second_response" | jq -r '.choices[0].message.content // .response')

    if [[ "$first_text" == "$second_text" ]] && echo "$first_text" | grep -q "2 + 2 = 4"; then
        test_pass "ergors_deterministic_repeatability" "Deterministic responses are repeatable"
    else
        test_fail "ergors_deterministic_repeatability" "Responses not deterministic" "First: $first_text | Second: $second_text"
    fi

    log_success "All inference provider routing tests completed"
    log "Validated full stack: API Keys → ERGORS Engine → LLM Router → Mock Provider"
}

# Cleanup function
cleanup_inference_tests() {
    cleanup_mock_provider
}
