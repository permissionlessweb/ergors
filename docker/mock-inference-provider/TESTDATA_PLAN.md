# Testdata JSON Creation Plan for Mock Inference Provider

## Objective

Create a comprehensive testdata.json file containing predefined request-response pairs to enable deterministic testing of network connectivity to the mock inference provider, reducing complexity by eliminating dynamic response generation.

## File Structure

The testdata.json will be located at: `docker/mock-inference-provider/testdata.json`

## JSON Structure

```json
{
  "ollama": {
    "generate": [...],
    "chat": [...],
    "tags": [...],
    "pull": [...],
    "show": [...],
    "embeddings": [...]
  },
  "openai": {
    "completions": [...],
    "chat_completions": [...],
    "models": [...],
    "embeddings": [...]
  },
  "tgi": {
    "generate": [...],
    "generate_stream": [...],
    "info": [...]
  },
  "agentic": {
    "execute": [...],
    "tool_calls": [...]
  },
  "api_keys": {
    "generate": [...],
    "validate": [...],
    "list": [...],
    "revoke": [...]
  },
  "system": {
    "health": [...],
    "metrics": [...],
    "root": [...]
  }
}
```

## Each Test Case Format

```json
{
  "request": {
    "method": "POST",
    "path": "/api/generate",
    "headers": {"Content-Type": "application/json"},
    "body": {...}
  },
  "response": {
    "status": 200,
    "headers": {"Content-Type": "application/json"},
    "body": {...}
  },
  "description": "Basic generate request with simple prompt",
  "tags": ["connectivity", "basic"]
}
```

## Coverage Requirements

### 1. Connectivity Testing (Primary Goal)

- Basic hello world requests for each API
- Minimal payload requests
- Standard success responses
- Network latency simulation data

### 2. Parameter Variations

- Different models (llama2, mistral, codellama, etc.)
- Various prompt lengths (short, medium, long)
- Temperature settings (0.0, 0.7, 1.0)
- Token limits (16, 256, 1024)
- Streaming vs non-streaming

### 3. Error Scenarios

- Invalid JSON payloads
- Missing required fields
- Unsupported models
- Server errors (500 status)
- Rate limiting (429 status)
- Authentication failures (401/403)

### 4. Edge Cases

- Empty prompts
- Very long prompts
- Special characters in prompts
- Unicode content
- Large request payloads

### 5. API-Specific Features

- Ollama: context handling, streaming chunks
- OpenAI: usage statistics, finish reasons
- TGI: token details, seed values
- Agentic: tool call simulation, iteration limits
- API Keys: key validation, expiration

## Implementation Steps

### Phase 1: Core Test Data Creation

1. Create basic connectivity tests for each endpoint
2. Add parameter variation tests
3. Include error response scenarios
4. Validate JSON structure and syntax

### Phase 2: Mock Provider Integration

1. Modify main.rs to load testdata.json on startup
2. Add request matching logic to find predefined responses
3. Implement fallback to dynamic generation if no match found
4. Add configuration flag to enable/disable testdata mode

### Phase 3: Testing and Validation

1. Test each endpoint with curl commands
2. Verify responses match expected JSON exactly
3. Performance testing with known response sizes
4. Integration testing with ERGORS workflow

### Phase 4: Documentation and Maintenance

1. Update README.md with testdata usage examples
2. Add scripts for validating testdata integrity
3. Document how to add new test cases
4. Create examples for different testing scenarios

## Success Criteria

- All major API endpoints have at least 3 test cases each
- Network connectivity can be tested with predictable responses
- Error scenarios are properly covered
- Integration with existing mock provider is seamless
- Test data is maintainable and extensible

## File Size Estimation

- Target: 50-100 test cases total
- JSON file size: ~100-200KB
- Focus on quality over quantity - each test case should serve a specific testing purpose

## Dependencies

- No external dependencies required
- Uses existing serde_json for parsing
- Compatible with current Rust version and dependencies

## Risk Mitigation

- Maintain backward compatibility - fallback to dynamic generation
- Validate JSON syntax before deployment
- Include comprehensive error handling for malformed test data
- Version control testdata.json for change tracking
