# Logging & Error Tracing Configuration

## Overview

CW-HO uses Rust's `tracing` ecosystem for structured logging with configurable verbosity levels. All API operations are automatically traced and errors include full error chains and stack traces based on environment configuration.

## Environment Variables

### RUST_LOG

Controls the logging level for the entire application. This is the standard Rust tracing environment variable.

**Levels** (from least to most verbose):
- `error` - Only errors
- `warn` - Warnings and errors
- `info` - Informational messages, warnings, and errors (default)
- `debug` - Debug information plus all above
- `trace` - Trace-level debugging plus all above

**Examples**:

```bash
# Basic levels
export RUST_LOG=info          # Default - general operational logs
export RUST_LOG=debug          # Detailed debugging information
export RUST_LOG=trace          # Very verbose trace-level logging

# Module-specific levels
export RUST_LOG=cw_ho=debug,tower_http=info    # Debug for cw_ho, info for tower_http
export RUST_LOG=cw_ho::server=trace            # Trace only server module

# Target specific components
export RUST_LOG=cw_ho::middleware=debug        # Debug middleware operations
export RUST_LOG=cw_ho::storage=trace           # Trace storage operations
```

### RUST_LOG_DETAIL

Controls whether detailed error traces (error chains and backtraces) are included in:
1. API error responses (JSON)
2. Storage operation error records
3. Log output

**Values**:
- `true` - Include full error chains and backtraces
- `false` - Basic error messages only (default)

**Note**: Automatically enabled when `RUST_LOG=debug` or `RUST_LOG=trace`.

**Examples**:

```bash
# Enable detailed error traces
export RUST_LOG_DETAIL=true

# Combined with log level
export RUST_LOG=info RUST_LOG_DETAIL=true

# Debug/trace automatically enable detailed traces
export RUST_LOG=debug  # RUST_LOG_DETAIL automatically becomes true
```

### RUST_BACKTRACE

Controls whether Rust generates backtraces for errors (separate from tracing).

**Values**:
- `0` - No backtraces (default)
- `1` - Full backtraces
- `full` - Full backtraces with all frames

**Example**:

```bash
export RUST_BACKTRACE=1
```

## Log Output Formats

### Standard Operation Logs (info level)

```
2024-01-15T10:30:45.123456Z  INFO cw_ho::middleware: 🚀 Request received
    operation_id="550e8400-e29b-41d4-a716-446655440000"
    method="POST"
    endpoint="/api/prompt"
    operation_type="prompt"

2024-01-15T10:30:45.678901Z  INFO cw_ho::middleware: ✅ Request completed successfully
    operation_id="550e8400-e29b-41d4-a716-446655440000"
    status="200 OK"
```

### Error Logs (error level)

**Basic (RUST_LOG_DETAIL=false)**:

```
2024-01-15T10:30:45.123456Z ERROR cw_ho::server: ❌ LLM processing failed
    error_type="LLM_ERROR"
    error="LLM provider error: Connection timeout"
```

**Detailed (RUST_LOG_DETAIL=true or RUST_LOG=debug)**:

```
2024-01-15T10:30:45.123456Z ERROR cw_ho::server: ❌ LLM processing failed
    error_type="LLM_ERROR"
    error="LLM provider error: Connection timeout"
    error_chain=["LLM provider error: Connection timeout", "HTTP client error: connection timeout", "IO error: timeout"]
    root_cause="IO error: timeout"
```

### Middleware Operation Traces

The middleware automatically adds tracing spans with structured fields:

```
2024-01-15T10:30:45.123456Z  INFO record_operation{operation_id="..." operation_type="prompt" endpoint="/api/prompt"}: cw_ho::middleware: 🚀 Request received
```

## API Error Responses

### Basic Error Response (RUST_LOG_DETAIL=false)

```json
{
  "error": "LLM processing failed: Connection timeout",
  "code": "LLM_ERROR",
  "timestamp": "2024-01-15T10:30:45.123456Z"
}
```

### Detailed Error Response (RUST_LOG_DETAIL=true)

```json
{
  "error": "LLM provider error: Connection timeout",
  "code": "LLM_ERROR",
  "timestamp": "2024-01-15T10:30:45.123456Z",
  "error_chain": [
    "LLM provider error: Connection timeout",
    "HTTP client error: connection timeout",
    "IO error: timeout"
  ],
  "backtrace": "...",
  "details": {
    "primary_error": "LLM provider error: Connection timeout",
    "root_cause": "IO error: timeout",
    "chain_length": 3
  }
}
```

## Storage Operation Error Records

Errors are automatically stored in the database with traces when `RUST_LOG_DETAIL=true`:

```rust
OperationRecord {
    id: "550e8400-e29b-41d4-a716-446655440000",
    operation_type: "prompt",
    error: Some(ErrorResponse {
        error: "LLM processing failed",
        code: "500 Internal Server Error",
        timestamp: ...,
        stack_trace: Some("Error Chain:\n  [0] LLM provider error: Connection timeout\n  [1] HTTP client error: connection timeout\n  [2] IO error: timeout\n\nBacktrace:\n...")
    }),
    ...
}
```

## Tracing Spans & Fields

### Middleware Span

Every request gets a tracing span with these fields:
- `operation_id` - Unique UUID for the operation
- `operation_type` - Classified operation type (prompt, bootstrap, etc.)
- `endpoint` - API endpoint path

### Additional Contextual Fields

Logs automatically include:
- `error_type` - Category of error (CONFIG_ERROR, STORAGE_ERROR, etc.)
- `error_chain` - Full error cause chain (when detailed)
- `root_cause` - Root cause of error (when detailed)
- `status` - HTTP status code

## Usage Examples

### Development - Full Debugging

```bash
export RUST_LOG=debug
export RUST_BACKTRACE=1
cargo run start
```

### Production - Info with Detailed Errors

```bash
export RUST_LOG=info
export RUST_LOG_DETAIL=true
./cw-ho start
```

### Production - Minimal Logging

```bash
export RUST_LOG=warn
export RUST_LOG_DETAIL=false
./cw-ho start
```

### Debugging Specific Component

```bash
export RUST_LOG=info,cw_ho::middleware=trace
./cw-ho start
```

### Error Investigation

```bash
export RUST_LOG=debug
export RUST_LOG_DETAIL=true
export RUST_BACKTRACE=full
./cw-ho start
```

## Querying Logged Errors

### From Storage API

```bash
# Get all failed operations with traces
curl -H "Authorization: Bearer <key>" \
  "http://localhost:8080/api/operations?limit=100" \
  | jq '.operations[] | select(.error != null)'

# View error details
curl -H "Authorization: Bearer <key>" \
  "http://localhost:8080/api/operations" \
  | jq '.operations[] | select(.error != null) | {id, error: .error.error, stack_trace: .error.stack_trace}'
```

### From Logs (when using JSON output)

Add JSON formatting to tracing:

```bash
export RUST_LOG=debug
export RUST_LOG_FORMAT=json  # If implemented
./cw-ho start 2>&1 | jq 'select(.fields.error_type != null)'
```

## Best Practices

### Development
- Use `RUST_LOG=debug` or `RUST_LOG=trace` for full visibility
- Enable `RUST_LOG_DETAIL=true` to capture error chains
- Enable `RUST_BACKTRACE=1` for Rust panic stack traces

### Staging
- Use `RUST_LOG=info` with `RUST_LOG_DETAIL=true`
- Captures all operations with detailed errors for debugging
- Manageable log volume with good error diagnostics

### Production
- Use `RUST_LOG=info` with `RUST_LOG_DETAIL=false` for normal operations
- Switch to `RUST_LOG=debug RUST_LOG_DETAIL=true` when investigating issues
- Use module-specific levels to focus on problem areas
- Consider log aggregation tools (e.g., Grafana Loki, ELK stack)

### Performance Considerations
- `RUST_LOG=trace` with `RUST_LOG_DETAIL=true` has the highest overhead
- Each log level adds minimal overhead (~nanoseconds per log call)
- Detailed error traces add ~100-500 microseconds per error
- Storage of stack traces increases database size proportionally

## Integration with Monitoring

### Prometheus Metrics (Future)
- Export error counts by type
- Track operation durations
- Monitor storage operation latency

### Structured Log Aggregation
- Use JSON formatter for structured log ingestion
- Filter by `operation_id` to trace request lifecycle
- Aggregate by `error_type` for error pattern analysis
- Alert on `error_chain` patterns for proactive debugging

## Troubleshooting

### No Logs Appearing
- Check `RUST_LOG` is set
- Default is `info` - try `debug` for more output
- Verify logs aren't filtered by external tool

### Missing Error Details
- Ensure `RUST_LOG_DETAIL=true` or `RUST_LOG=debug`
- Check error responses include `error_chain` field
- Verify storage records have `stack_trace` populated

### Too Many Logs
- Reduce level: `info` → `warn` → `error`
- Use module-specific filters: `RUST_LOG=cw_ho=info,tower_http=warn`
- Disable detailed traces: `RUST_LOG_DETAIL=false`

### Performance Issues
- Reduce to `info` or `warn` level
- Disable detailed tracing: `RUST_LOG_DETAIL=false`
- Use module-specific levels for targeted debugging
