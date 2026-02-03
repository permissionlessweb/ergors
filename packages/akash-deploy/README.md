# Akash-Deploy

## TODO

- wire in chain-sdk-rs types in replacement of granular struct object types for msgs

## Migration Path

1. **Create `packages/akash-deploy`** with above structure
2. **Implement `AkashBackend`** in `ergors`:
   - `CosmosClient` → queries
   - `broadcast_akash_msg` → transactions
   - `ErgorsStorage` → state persistence
   - `EncryptedCosmosKeyManager` → signing
3. **Delete** workflow logic from `ergors/src/deploy/workflow.rs`
4. **Keep** `ergors/src/deploy/` for backend implementation only

---

## Design Principles

1. **One trait** — `AkashBackend` is the only interface
2. **State machine is dumb** — just transitions and saves
3. **No storage coupling** — consumer implements persistence
4. **No signing coupling** — consumer provides signer type
5. **No transport coupling** — consumer implements HTTP/gRPC
6. **Minimal types** — only what the workflow needs
7. **Errors are explicit** — no `anyhow` leakage in public API
8. **Certificate key persistence** — encrypted keys stored separately from workflow state, keyed by owner address
9. **Provider info caching** — optional caching layer for human-readable bid display, consumer implements cache policy
