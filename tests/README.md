# Test Library

## Integration Tests

### JWT Verification (`scripts/jwt-verify/`)

Verifies that Rust JWT generation is compatible with Akash provider verification logic.

**Quick test:**

```bash
cd tests/scripts/jwt-verify && ./test.sh
```

This runs:

1. Generates JWT using your `CosmosKeyPair::sign_jwt_es256k()`
2. Verifies using actual provider verification code (from `pkg.akt.dev/go/util/jwt`)
3. Pass/fail in < 5 seconds

See `scripts/jwt-verify/README.md` for details.

## Reproducible Testing Suite

implement standard testing suite for simulating/orchestrating multi-node test suites. We can do this via localization, emulation, another method.
