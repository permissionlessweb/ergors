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

# Deploy mock inference provider (locally with Docker for testing)
deploy_mock_provider() {
    log_section "Deploying Mock Inference Provider"

    local provider_port="${MOCK_PROVIDER_PORT:-11434}"
    local provider_host="${MOCK_PROVIDER_HOST:-127.0.0.1}"
    local image="ghcr.io/permissionlessweb/mock-inference-provider:latest"
    local container_name="ergors-e2e-mock-provider"

    # Stop existing container if running
    if docker ps -a --format '{{.Names}}' | grep -q "^${container_name}$"; then
        log "Stopping existing mock provider container..."
        docker stop "$container_name" >/dev/null 2>&1 || true
        docker rm "$container_name" >/dev/null 2>&1 || true
    fi

    # Pull latest image
    log "Pulling mock provider image: $image"
    if ! docker pull "$image" >/dev/null 2>&1; then
        log_warn "Failed to pull image, will use local build if available"
    fi

    # Start mock provider with testdata mode enabled
    log "Starting mock provider on port $provider_port..."
    docker run -d \
        --name "$container_name" \
        -p "${provider_port}:11434" \
        -e TESTDATA_MODE=true \
        -e MIN_LATENCY_MS=0 \
        -e MAX_LATENCY_MS=50 \
        -e PORT=11434 \
        -e RUST_LOG=info \
        "$image" >/dev/null 2>&1 || {
        log_error "Failed to start mock provider container"
        return 1
    }

    # Wait for provider to be ready
    local max_wait=30
    local wait_count=0
    log "Waiting for mock provider to be ready..."
    while ! curl -s "http://${provider_host}:${provider_port}/health" >/dev/null 2>&1; do
        sleep 1
        wait_count=$((wait_count + 1))
        if [[ $wait_count -ge $max_wait ]]; then
            log_error "Mock provider failed to start within ${max_wait}s"
            docker logs "$container_name" 2>&1 | tail -20
            return 1
        fi
    done

    log_success "Mock provider ready at http://${provider_host}:${provider_port}"

    # Export for use in tests
    export MOCK_PROVIDER_URL="http://${provider_host}:${provider_port}"
    export MOCK_PROVIDER_CONTAINER="$container_name"

    # Verify health
    local health_response
    health_response=$(curl -s "${MOCK_PROVIDER_URL}/health")
    if echo "$health_response" | jq -e '.status == "ok"' >/dev/null 2>&1; then
        log_success "Mock provider health check passed"
    else
        log_warn "Health check returned unexpected response: $health_response"
    fi

    return 0
}

# Stop and remove mock provider
cleanup_mock_provider() {
    if [[ -n "${MOCK_PROVIDER_CONTAINER:-}" ]]; then
        log "Stopping mock provider container..."
        docker stop "$MOCK_PROVIDER_CONTAINER" >/dev/null 2>&1 || true
        docker rm "$MOCK_PROVIDER_CONTAINER" >/dev/null 2>&1 || true
    fi
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

    # Test: Deterministic response - "Hello world"
    log "Testing deterministic Ollama generate endpoint..."
    local response
    response=$(curl -s "${MOCK_PROVIDER_URL}/api/generate" \
        -H "Content-Type: application/json" \
        -d '{"model": "llama2", "prompt": "Hello world", "stream": false}')

    local response_text
    response_text=$(echo "$response" | jq -r '.response')
    if echo "$response_text" | grep -q "mock inference provider"; then
        test_pass "deterministic_hello_world" "Received expected deterministic response"
    else
        test_fail "deterministic_hello_world" "Unexpected response" "Got: $response_text"
    fi

    # Test: Deterministic response - "What is 2+2?"
    log "Testing deterministic math response..."
    response=$(curl -s "${MOCK_PROVIDER_URL}/api/generate" \
        -H "Content-Type: application/json" \
        -d '{"model": "mistral", "prompt": "What is 2+2?", "stream": false, "options": {"temperature": 0.0, "num_predict": 50}}')

    response_text=$(echo "$response" | jq -r '.response')
    if echo "$response_text" | grep -q "2 + 2 = 4"; then
        test_pass "deterministic_math" "Received expected math response"
    else
        test_fail "deterministic_math" "Unexpected response" "Got: $response_text"
    fi

    # Test: Chat completion with deterministic response
    log "Testing deterministic chat completion..."
    response=$(curl -s "${MOCK_PROVIDER_URL}/api/chat" \
        -H "Content-Type: application/json" \
        -d '{"model": "llama2", "messages": [{"role": "user", "content": "Hello, how are you?"}], "stream": false}')

    response_text=$(echo "$response" | jq -r '.message.content')
    if echo "$response_text" | grep -q "doing well"; then
        test_pass "deterministic_chat" "Received expected chat response"
    else
        test_fail "deterministic_chat" "Unexpected response" "Got: $response_text"
    fi

    # Test: OpenAI chat completions endpoint
    log "Testing OpenAI-compatible endpoint..."
    response=$(curl -s "${MOCK_PROVIDER_URL}/v1/chat/completions" \
        -H "Content-Type: application/json" \
        -d '{"model": "gpt-3.5-turbo", "messages": [{"role": "user", "content": "Hello, how are you?"}], "max_tokens": 100, "temperature": 0.7}')

    response_text=$(echo "$response" | jq -r '.choices[0].message.content')
    if echo "$response_text" | grep -q "mock inference provider"; then
        test_pass "openai_chat" "OpenAI endpoint works correctly"
    else
        test_fail "openai_chat" "Unexpected response" "Got: $response_text"
    fi

    # Test: Tool calling (agentic)
    log "Testing agentic tool calling..."
    response=$(curl -s "${MOCK_PROVIDER_URL}/api/agentic/execute" \
        -H "Content-Type: application/json" \
        -d '{"model": "llama2", "prompt": "Search for information about Rust programming language", "tools": [{"name": "web_search", "description": "Search the web", "parameters": {"type": "object", "properties": {"query": {"type": "string"}}, "required": ["query"]}}], "max_iterations": 2}')

    local tool_call_name
    tool_call_name=$(echo "$response" | jq -r '.tool_calls[0].function.name')
    if [[ "$tool_call_name" == "web_search" ]]; then
        test_pass "agentic_tool_call" "Tool calling works correctly"
    else
        test_fail "agentic_tool_call" "Tool call failed" "Expected 'web_search', got: $tool_call_name"
    fi

    # Test: Invalid model error
    log "Testing invalid model error handling..."
    response=$(curl -s "${MOCK_PROVIDER_URL}/api/generate" \
        -H "Content-Type: application/json" \
        -d '{"model": "invalid-model", "prompt": "test", "stream": false}')

    local error_msg
    error_msg=$(echo "$response" | jq -r '.error')
    if echo "$error_msg" | grep -q "not found"; then
        test_pass "invalid_model_error" "Error handling works correctly"
    else
        test_fail "invalid_model_error" "Expected error message" "Got: $error_msg"
    fi

    # TODO: Test ERGORS proxy routing once gRPC endpoints are available
    # This would involve:
    # 1. configure_ergors_proxy "$COORDINATOR_HOME" "$api_key" "llama*"
    # 2. make_inference_request "$COORDINATOR_API" "llama2" "Hello world" "ollama"
    # 3. Verify response is routed through ERGORS and matches expected output

    log_success "All inference provider tests completed"
}

# Cleanup function
cleanup_inference_tests() {
    cleanup_mock_provider
}
