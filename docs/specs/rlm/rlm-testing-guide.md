# RLM Integration Testing Guide

## Prerequisites

### Python Environment

1. Python 3.8+ installed
2. pip available
3. RestrictedPython library (auto-installed by build script)

```bash
# Verify Python installation
python3 --version

# Verify pip
python3 -m pip --version
```

### Build the Project

```bash
# From project root
cargo build --release

# Or build just the RLM crate
cargo build -p ergors-rlm
```

The build script will automatically:

- Check for Python 3
- Install pip if missing
- Install RestrictedPython and dependencies
- Verify the installation

## Testing Levels

### 1. Unit Tests

Test individual components:

```bash
# Test RLM crate
cd packages/ergors-rlm
cargo test

# Test with verbose output
cargo test -- --nocapture
```

### 2. Integration Tests

Test Rust-Python communication:

```bash
# Run integration tests
cargo test -p ergors-rlm --test test_integration
```

### 3. Manual Testing with Discord

#### Step 1: Start ERGORS

```bash
ergors start
```

#### Step 2: Configure Discord Guild for RLM

In your Discord server, use the `/rlmconfig` command:

```
/rlmconfig mode:rlm max_iterations:10 max_sub_calls:50
```

Modes:

- `static`: Traditional RAG with embeddings (default)
- `rlm`: Pure RLM (agentic code execution)
- `hybrid`: Try RLM first, fallback to RAG

#### Step 3: Ingest Test Documents

```
/ingest url:https://example.com/document.html
```

Or use local documents:

```
/ingest url:file:///path/to/document.md
```

#### Step 4: Test Query

```
/prompt What is the main topic of the ingested documents?
```

#### Step 5: Verify RLM Execution

Check the logs for:

```
[INFO] RLM service initialized successfully
[DEBUG] RLM querying 5 documents
[DEBUG] RLM response: 3 iterations, 5 sub-LLM calls
```

### 4. Performance Testing

#### Measure Latency

Compare RAG vs RLM response times:

```bash
# Enable info logging
export RUST_LOG=info

# Test with static RAG
/rlmconfig mode:static
/prompt <your query>
# Note the response time

# Test with RLM
/rlmconfig mode:rlm
/prompt <same query>
# Compare response time (should be 10-50x slower)
```

#### Measure Cost

Check sub-LLM call count in logs:

```sh
[DEBUG] RLM response: 3 iterations, 5 sub-LLM calls
```

Estimated cost: ~$0.01-0.05 per query (vs $0.001 for static RAG)

### 5. Error Handling Tests

#### Test Python Errors

Try a query that might cause Python errors:

```sh
/prompt Calculate the square root of -1
```

Expected: Graceful error handling, fallback to RAG in hybrid mode

#### Test Iteration Limit

Set low iteration limit:

```sh
/rlmconfig max_iterations:2
/prompt <complex query requiring many iterations>
```

Expected: "Failed to converge after 2 iterations" message

#### Test Sub-LLM Limit

Set low sub-LLM call limit:

```sh
/rlmconfig max_sub_calls:3
/prompt <query requiring many LLM calls>
```

Expected: "Max sub-LLM calls exceeded" error

## Debugging

### Enable Debug Logging

```bash
export RUST_LOG=debug,ergors_rlm=trace
ergors start
```

### Check Python Worker Status

The RLM service spawns Python subprocess workers. Check if they're running:

```bash
ps aux | grep python3 | grep repl_worker
```

### Manual Python Testing

Test the Python scripts directly:

```bash
cd packages/ergors-rlm/python

# Test imports
python3 -c "import repl_worker; import repl_engine; print('OK')"

# Test RestrictedPython
python3 -c "from RestrictedPython import compile_restricted; print('OK')"
```

### Test REPL Execution Manually

Create a test script:

```python
# test_repl.py
import sys
import json

# Simulate a simple execute request
request = {
    "jsonrpc": "2.0",
    "method": "execute",
    "params": {
        "query": "What is the total length of all documents?",
        "documents": [
            {"source_uri": "test://doc1", "content": "Hello world", "doc_type": "text", "tags": [], "ingested_at": 0}
        ],
        "max_iterations": 5,
        "max_sub_calls": 10
    },
    "id": 1
}

print(json.dumps(request))
```

Run with worker:

```bash
python3 python/repl_worker.py < test_request.json
```

## Common Issues

### Python Not Found

**Error:** `Python 3 not found. RLM service will not work.`

**Solution:**

```bash
# Install Python 3.8+
# macOS
brew install python3

# Ubuntu/Debian
sudo apt install python3 python3-pip

# Verify
python3 --version
```

### RestrictedPython Not Installed

**Error:** `RestrictedPython not available. RLM service will not work.`

**Solution:**

```bash
python3 -m pip install RestrictedPython --user
```

### Worker Process Crashes

**Error:** `Worker process closed stdin`

**Check:**

1. Python syntax errors in worker scripts
2. Missing dependencies
3. Python version compatibility (need 3.8+)

**Debug:**

```bash
# Run worker directly to see errors
python3 packages/ergors-rlm/python/repl_worker.py
# (will wait for stdin)
```

### RLM Service Not Initialized

**Error:** `RLM mode requested but feature is not enabled`

**Solution:**

```bash
# Make sure rlm feature is enabled (it's in default features)
cargo build --release --features rlm
```

### No Documents Found

**Error:** `No documents found for RLM query`

**Solution:**

1. Ingest documents first with `/ingest`
2. Check document ingestion with `/ragsources`
3. Verify guild ID matches

## Performance Benchmarks

Expected performance characteristics:

| Metric | Static RAG | RLM |
|--------|-----------|-----|
| Latency | 100-200ms | 2-10s |
| Iterations | 1 | 2-10 |
| Sub-LLM calls | 0 | 2-10 |
| Cost/query | $0.001 | $0.01-0.05 |
| Accuracy | Good | Better |

## Safety & Security

### Sandbox Verification

The RLM service uses RestrictedPython to sandbox code execution. Verify it works:

```python
# test_sandbox.py
from RestrictedPython import compile_restricted

# This should compile fine
safe_code = "result = len(context)"
compile_restricted(safe_code, '<string>', 'exec')

# This should fail or be restricted
dangerous_code = "import os; os.system('ls')"
try:
    compile_restricted(dangerous_code, '<string>', 'exec')
    print("WARNING: Dangerous code was not restricted!")
except:
    print("OK: Dangerous code blocked")
```

### Resource Limits

RLM queries are limited by:

- `max_iterations`: Prevents infinite loops (default: 10)
- `max_sub_calls`: Prevents excessive LLM usage (default: 50)
- Timeout: Each sub-LLM call has timeout (configured in LlmRouter)

### Cost Control

Monitor costs in logs:

```
[INFO] RLM response: cost=$0.023, iterations=4, sub_llm_calls=7
```

Set up per-guild daily limits in future versions.

## Success Criteria

✅ **Phase 3 Complete** when:

- [ ] Build succeeds with `cargo build --release`
- [ ] Python dependencies install automatically
- [ ] Unit tests pass
- [ ] Integration tests pass (if Python available)

✅ **Phase 4 Complete** when:

- [ ] Discord gateway starts without errors
- [ ] `/rlmconfig` command works
- [ ] RLM service initializes successfully
- [ ] Mode switching works (static/rlm/hybrid)

✅ **Full Integration** when:

- [ ] Query with mode=rlm returns answer with sources
- [ ] Logs show RLM iterations and sub-LLM calls
- [ ] Hybrid mode falls back to RAG on error
- [ ] Performance is within expected range

## Next Steps After Testing

1. **Optimize Python Worker Pool**
   - Tune pool size based on load
   - Add worker health monitoring
   - Implement automatic worker restart

2. **Improve System Prompt**
   - Refine based on query quality
   - Add domain-specific instructions
   - Optimize for fewer iterations

3. **Add Metrics**
   - Track query latency
   - Monitor cost per guild
   - Log error rates

4. **Production Rollout**
   - Enable for test guild
   - Monitor for 1 week
   - Gradually roll out to more guilds
   - Keep static RAG as default during rollout

## Troubleshooting Commands

```bash
# Check if RLM feature is enabled
cargo tree -p ergors | grep ergors-rlm

# Rebuild with clean
cargo clean && cargo build --release

# Test Python directly
python3 packages/ergors-rlm/python/repl_worker.py

# Check logs
tail -f /path/to/ergors/logs/ergors.log | grep RLM

# Test a simple query
curl -X POST http://localhost:8080/api/prompt \
  -H "Content-Type: application/json" \
  -d '{"messages":[{"role":"user","content":"test"}]}'
```
