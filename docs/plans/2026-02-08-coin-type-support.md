# Cosmos Address Prefix Support Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add `--prefix` flag to `ergors keys import-mnemonic` to derive addresses for different Cosmos chains (Akash uses akash1, Cosmos uses cosmos1, Ergors uses ergo1)

**Architecture:** Extend existing `KeysCmd::ImportMnemonic` to accept optional bech32 prefix parameter, pass through to `EncryptedCosmosKeyManager`, update E2E tests to use `--prefix akash` for Akash faucet.

**Tech Stack:** Rust, clap CLI, Cosmos SDK bech32 encoding, E2E bash tests

---

## Background

**Problem:** Ergors currently hardcodes prefix "ergo" when importing mnemonics. The Akash faucet key needs prefix "akash" to produce valid Akash addresses, resulting in address mismatches in E2E tests.

**Current Behavior:**
- Importing Akash faucet mnemonic produces `ergo1...` addresses (coin type 118, prefix "ergo")
- Akash network expects `akash1...` addresses (coin type 118, prefix "akash")
- E2E grant tests fail due to prefix mismatch
- Storage has public key, can derive addresses with any prefix

**Key Insight:** Akash uses coin type 118 (same as Cosmos Hub). The difference is the bech32 prefix:
- Cosmos Hub: cosmos1
- Akash Network: akash1
- Ergors: ergo1

**Solution:** Add `--prefix` CLI flag (default "ergo") and update E2E tests to use `--prefix akash` for Akash faucet keys. Coin type stays 118 for all chains.

## Task 1: Update Encrypted Cosmos Key Manager Documentation

**Files:**
- Modify: `packages/ho-std/src/keys/encrypted_cosmos.rs:305-311`

**Step 1: Update coin type comment to clarify Akash**

Current comment says "118: Cosmos/Akash" which is technically correct but unclear. Clarify that both use 118 but different prefixes:

```rust
/// Get a keypair from an encrypted mnemonic with custom coin type
///
/// This allows deriving addresses for different cosmos chains:
/// - 118: Cosmos Hub, Akash Network, and most Cosmos chains (default)
/// - 330: Terra
/// - 60: Ethereum (for EVM chains)
/// - 529: Secret Network
///
/// Note: Coin type determines the BIP-44 derivation path. Many chains share
/// coin type 118 but use different bech32 prefixes (cosmos1, akash1, ergo1, etc.)
pub fn get_keypair_with_coin_type(
```

**Step 2: Run cargo check**

Run: `cargo chec -p ho-std`
Expected: No new errors

**Step 3: Commit**

```bash
git add packages/ho-std/src/keys/encrypted_cosmos.rs
git commit -m "docs: clarify coin type 118 is shared by Cosmos/Akash/etc"
```

## Task 2: Add Prefix Flag to CLI

**Files:**
- Modify: `packages/ergors/src/keys/mod.rs:44-59`

**Step 1: Add prefix field to ImportMnemonic variant**

```rust
#[derive(Debug, Clone, clap::Subcommand)]
pub enum KeysSubCmd {
    /// Import a BIP-39 mnemonic seed phrase as a funding key
    ///
    /// The mnemonic is entered interactively (hidden input) for security.
    /// It is never stored in shell history or visible in process listings.
    /// Keys are stored with public key + default address, can derive
    /// chain-specific addresses at usage time.
    #[clap(display_order = 100)]
    ImportMnemonic {
        /// Human-readable label for this key (used as identifier)
        #[arg(long)]
        label: String,

        /// Mark this key as the default for deployments
        #[arg(long)]
        default: bool,

        /// Bech32 address prefix (ergo=Ergors, akash=Akash, cosmos=Cosmos Hub, etc.)
        #[arg(long, default_value = "ergo")]
        prefix: String,
    },
```

**Step 2: Run cargo check**

Run: `cargo chec -p ergors`
Expected: Compilation errors in exec_via_grpc and exec_via_storage (missing prefix parameter)

**Step 3: Commit structure change**

```bash
git add packages/ergors/src/keys/mod.rs
git commit -m "feat: add --prefix flag to keys import-mnemonic"
```

## Task 3: Update gRPC Import Path

**Files:**
- Modify: `packages/ergors/src/keys/mod.rs:118-141`

**Step 1: Pass prefix to import_cosmos_key**

```rust
async fn exec_via_grpc(&self, client: &mut ManagementClient, json: bool) -> Result<()> {
    match &self.subcmd {
        KeysSubCmd::ImportMnemonic {
            label,
            default,
            prefix,
        } => {
            // Prompt for mnemonic (hidden input - never stored in history)
            let phrase = get_mnemonic()?;

            // When daemon is running, it uses its custody password for key encryption.
            // We pass empty string and the daemon handles it.
            let password = String::new();

            // Import with user-specified prefix (default "ergo")
            let resp = client
                .import_cosmos_key(
                    &phrase,
                    label,
                    label, // use label as key_name
                    "",    // chain-agnostic
                    prefix, // user-specified prefix
                    *default,
                    &password,
                )
                .await?;

            if resp.success {
                if let Some(key) = resp.key {
                    if json {
                        let resp = KeyImportResponse {
                            label: key.label.clone(),
                            address: key.address.clone(),
                            is_default: key.is_default,
                        };
                        println!("{}", serde_json::to_string_pretty(&resp)?);
                    } else {
                        println!("Key imported successfully:");
                        println!("  Label:   {}", key.label);
                        println!("  Address: {}", key.address);
                        println!("  Default: {}", if key.is_default { "yes" } else { "no" });
                    }
                }
            } else {
                return Err(anyhow!("Import failed: {}", resp.error_message));
            }
        }
```

**Step 2: Run cargo check**

Run: `cargo chec -p ergors`
Expected: Still has errors in exec_via_storage path

**Step 3: Commit**

```bash
git add packages/ergors/src/keys/mod.rs
git commit -m "feat: pass prefix to gRPC import_cosmos_key"
```

## Task 4: Update Direct Storage Import Path

**Files:**
- Modify: `packages/ergors/src/keys/mod.rs:226-248`
- Modify: `packages/ergors/src/keys/mod.rs:250-328`

**Step 1: Pass prefix to import_mnemonic_direct**

```rust
async fn exec_via_storage(&self, storage: &ErgorsStorage, json: bool) -> Result<()> {
    match &self.subcmd {
        KeysSubCmd::ImportMnemonic {
            label,
            default,
            prefix,
        } => {
            // Prompt for mnemonic (hidden input - never stored in history)
            let phrase = get_mnemonic()?;

            self.import_mnemonic_direct(
                storage,
                &phrase,
                label,
                *default,
                prefix,
                json,
            )
            .await
        }
        KeysSubCmd::List {} => self.list_keys_direct(storage, json).await,
        KeysSubCmd::Delete { label } => self.delete_key_direct(storage, label).await,
        KeysSubCmd::SetDefault { label } => self.set_default_direct(storage, label).await,
    }
}
```

**Step 2: Update import_mnemonic_direct signature and implementation**

```rust
async fn import_mnemonic_direct(
    &self,
    storage: &ErgorsStorage,
    phrase: &str,
    label: &str,
    make_default: bool,
    prefix: &str,
    json: bool,
) -> Result<()> {
    let password = get_password(true)?;

    // Load or create key store
    let mut store = match storage.get_cosmos_key_store().await {
        Ok(Some(s)) => s,
        Ok(None) => EncryptedCosmosKeyManager::create_empty_store(),
        Err(e) => return Err(anyhow!("Failed to load key store: {}", e)),
    };

    // Create key manager
    let mut manager = if store.keys.is_empty() {
        EncryptedCosmosKeyManager::new()
    } else {
        EncryptedCosmosKeyManager::from_store(&store)
    };

    // Unlock with password
    manager.unlock(&password)?;

    // Use label as key_name
    let key_name = label;

    // Check for duplicate key name
    if store.keys.iter().any(|k| k.key_name == key_name) {
        return Err(anyhow!(
            "Key with label '{}' already exists. Use a different --label.",
            key_name
        ));
    }

    // Import and encrypt the mnemonic with custom prefix
    let (encrypted, account_info) = manager.import_mnemonic_with_label(
        key_name,
        phrase,
        "",  // chain-agnostic
        prefix, // user-specified prefix
        label,
        make_default,
    )?;

    // Check for duplicate address
    if EncryptedCosmosKeyManager::address_exists(&store, &account_info.address) {
        return Err(anyhow!(
            "Address {} already exists in the key store (duplicate mnemonic?)",
            account_info.address
        ));
    }

    // Add to store and persist
    manager.add_key_to_store(&mut store, encrypted, account_info.clone());
    storage
        .put_cosmos_key_store(&store)
        .await
        .map_err(|e| anyhow!("Failed to save key store: {}", e))?;

    if json {
        let resp = KeyImportResponse {
            label: label.to_string(),
            address: account_info.address.clone(),
            is_default: make_default,
        };
        println!("{}", serde_json::to_string_pretty(&resp)?);
    } else {
        println!("Key imported successfully:");
        println!("  Label:   {}", label);
        println!("  Address: {}", account_info.address);
        println!("  Default: {}", if make_default { "yes" } else { "no" });
    }

    Ok(())
}
```

**Step 3: Run cargo check**

Run: `cargo chec -p ergors`
Expected: No errors (import_mnemonic_with_label already exists and accepts prefix)

**Step 4: Commit**

```bash
git add packages/ergors/src/keys/mod.rs
git commit -m "feat: pass prefix through direct storage path"
```

## Task 5: Update E2E Test Helper Functions

**Files:**
- Modify: `tests/e2e/lib/ergors.sh:193-226` (_ergors_import_keys_to_node)
- Modify: `tests/e2e/lib/ergors.sh:718-779` (ergors_import_faucet_key)

**Step 1: Add prefix parameter to _ergors_import_keys_to_node**

Update the function signature and add `--prefix akash` flag:

```bash
# Import faucet key into a node (internal helper)
_ergors_import_keys_to_node() {
    local home_dir="$1"
    local node_name="$2"
    local prefix="${3:-ergo}"  # Default to ergo, pass akash for Akash

    # Get faucet mnemonic
    local mnemonic
    mnemonic=$(akash_get_faucet_mnemonic 2>/dev/null) || {
        log_warn "Could not get faucet mnemonic for $node_name, skipping key import"
        return 0
    }

    # Import mnemonic using ergors engine binary
    # Both ERGORS_CUSTODY_PASSWORD and ERGORS_MNEMONIC env vars enable non-interactive import
    local import_output
    import_output=$(ERGORS_CUSTODY_PASSWORD="${TEST_CUSTODY_PASSWORD}" \
        ERGORS_MNEMONIC="$mnemonic" \
        "$ERGORS_BIN" --home "$home_dir" keys import-mnemonic \
        --label "E2E Faucet Key" \
        --prefix "$prefix" \
        --default 2>&1) || true

    # Check if import succeeded
    if echo "$import_output" | grep -Eq "${prefix}1[a-z0-9]+"; then
        local addr
        addr=$(echo "$import_output" | grep -oE "${prefix}1[a-z0-9]*" | head -1)
        log_verbose "$node_name: Imported faucet key: $addr"
        return 0
    elif echo "$import_output" | grep -qi "already exists"; then
        log_verbose "$node_name: Faucet key already imported (skipping)"
        return 0
    else
        log_warn "$node_name: Key import may have issues"
        log_debug "$import_output"
        return 0  # Non-fatal
    fi
}
```

**Step 2: Update ergors_import_faucet_key to use prefix "akash"**

```bash
# Import the Akash faucet mnemonic into the coordinator node
# This gives the coordinator a pre-funded key (10B AKT from genesis)
# Uses: ergors keys import-mnemonic --prefix akash
# If a faucet key already exists (e.g., "E2E Faucet Key"), use that instead of importing again
ergors_import_faucet_key() {
    local key_name="${1:-faucet}"
    local home_dir="${2:-$TEST_DIR/coordinator}"

    # Check if a faucet key already exists (may have been imported during node startup)
    local keys_output
    keys_output=$(ERGORS_CUSTODY_PASSWORD="${TEST_CUSTODY_PASSWORD}" \
        "$ERGORS_BIN" --home "$home_dir" keys list 2>&1) || true

    log_verbose "Checking existing keys..."
    log_verbose "Keys list: $keys_output"

    # Check if the requested key already exists
    if echo "$keys_output" | grep -q "$key_name"; then
        local addr
        # Extract address using regex (akash1... or ergo1...)
        addr=$(echo "$keys_output" | grep "$key_name" | grep -oE "(akash1|ergo1)[a-z0-9]{38,}" | head -1)
        if [[ -n "$addr" ]]; then
            log_success "Faucet key already exists: $addr (label: $key_name)"
            export FAUCET_ADDRESS="$addr"
            return 0
        fi
    fi

    # Check if "E2E Faucet Key" exists (imported during node startup)
    if echo "$keys_output" | grep -q "E2E Faucet Key"; then
        local addr
        # Extract address using regex (akash1... or ergo1...)
        addr=$(echo "$keys_output" | grep "E2E Faucet Key" | grep -oE "(akash1|ergo1)[a-z0-9]{38,}" | head -1)
        if [[ -n "$addr" ]]; then
            log_success "Using existing E2E Faucet Key: $addr"
            export FAUCET_ADDRESS="$addr"
            return 0
        fi
    fi

    # No existing key found, import the mnemonic
    log "Importing faucet mnemonic into coordinator..."

    local mnemonic
    mnemonic=$(akash_get_faucet_mnemonic) || {
        log_error "Could not get faucet mnemonic"
        return 1
    }

    log_verbose "Mnemonic word count: $(echo "$mnemonic" | wc -w | tr -d ' ')"

    # Import mnemonic using ergors engine binary with Akash prefix
    # Requires ERGORS_CUSTODY_PASSWORD for key encryption
    local import_output
    import_output=$(ERGORS_CUSTODY_PASSWORD="${TEST_CUSTODY_PASSWORD}" \
        ERGORS_MNEMONIC="$mnemonic" \
        "$ERGORS_BIN" --home "$home_dir" keys import-mnemonic \
        --label "$key_name" \
        --prefix akash \
        --default 2>&1) || true

    log_verbose "Import output: $import_output"

    # Verify key was imported by listing keys again
    keys_output=$(ERGORS_CUSTODY_PASSWORD="${TEST_CUSTODY_PASSWORD}" \
        "$ERGORS_BIN" --home "$home_dir" keys list 2>&1) || true

    log_verbose "Keys list after import: $keys_output"

    # Extract faucet address from keys list output
    if echo "$keys_output" | grep -q "$key_name"; then
        # Extract address using regex (handles multi-word labels)
        local addr
        addr=$(echo "$keys_output" | grep "$key_name" | grep -oE "(akash1|ergo1)[a-z0-9]{38,}" | head -1)

        if [[ -n "$addr" ]]; then
            log_success "Faucet key imported: $addr"
            export FAUCET_ADDRESS="$addr"
            return 0
        fi
    fi

    # Fallback: check if import message contains address with either prefix
    if echo "$import_output" | grep -Eq "akash1|ergo1"; then
        local addr
        addr=$(echo "$import_output" | grep -oE "(akash1|ergo1)[a-z0-9]*" | head -1)
        if [[ -n "$addr" ]]; then
            log_success "Faucet key imported: $addr"
            export FAUCET_ADDRESS="$addr"
            return 0
        fi
    fi

    log_error "Failed to verify imported key"
    return 1
}
```

**Step 3: Update node startup calls to pass prefix "akash"**

Find calls to `_ergors_import_keys_to_node` (around lines 1504-1540) and add prefix parameter:

```bash
# In ergors_import_keys function:
log "Importing faucet key into coordinator..."
local coord_import
coord_import=$(ERGORS_CUSTODY_PASSWORD="${TEST_CUSTODY_PASSWORD}" \
    ERGORS_MNEMONIC="$mnemonic" \
    "$ERGORS_BIN" --home "$coord_home" keys import-mnemonic \
    --label "E2E Faucet Key" \
    --prefix akash \
    --default 2>&1) || true

# ... and for executor ...
log "Importing faucet key into executor..."
local exec_import
exec_import=$(ERGORS_CUSTODY_PASSWORD="${TEST_CUSTODY_PASSWORD}" \
    ERGORS_MNEMONIC="$mnemonic" \
    "$ERGORS_BIN" --home "$exec_home" keys import-mnemonic \
    --label "E2E Faucet Key" \
    --prefix akash \
    --default 2>&1) || true
```

**Step 4: Test manually**

Run: `just e2e grants --skip-build --verbose 2>&1 | head -80`
Expected: Should see "Using existing E2E Faucet Key: akash1..." instead of "ergo1..."

**Step 5: Commit**

```bash
git add tests/e2e/lib/ergors.sh
git commit -m "fix: use prefix akash for Akash faucet key in E2E tests"
```

## Task 6: Run Full E2E Grant Tests

**Files:**
- Test: `tests/e2e/tests/grants.sh`

**Step 1: Run grant tests**

Run: `just e2e grants --verbose`
Expected:
- ✓ Faucet key already exists or imports successfully with akash1... address
- ✓ Faucet has balance (10B AKT)
- ✓ Grant request tests pass
- No "invalid bech32 string" errors

**Step 2: Verify address format**

Check test output for:
```
[timestamp] Using existing E2E Faucet Key: akash1s260305ypnythlpjgxlee9fh90fddlhd43rf5l
```

Address should start with `akash1` (not `ergo1`)

**Step 3: If tests pass, commit success marker**

```bash
git add tests/e2e/tests/grants.sh  # No changes, just marker commit
git commit -m "test: verify prefix akash works for Akash in E2E grants"
```

## Task 7: Update CLI Documentation

**Files:**
- Modify: `packages/ergors/CLI_REFERENCE.md` (keys section)

**Step 1: Add prefix flag to keys import-mnemonic documentation**

Find the keys section and update:

```markdown
### `ergors keys import-mnemonic`

Import a BIP-39 mnemonic seed phrase as a cosmos funding key.

**Usage:**
```bash
ergors keys import-mnemonic --label <LABEL> [--default] [--prefix <PREFIX>]
```

**Flags:**
- `--label <LABEL>` - Human-readable label for this key (required)
- `--default` - Mark this key as default for deployments
- `--prefix <PREFIX>` - Bech32 address prefix (default: "ergo")
  - ergo: Ergors (default)
  - akash: Akash Network
  - cosmos: Cosmos Hub
  - osmo: Osmosis
  - juno: Juno
  - (any valid bech32 prefix)

**Interactive:**
The command prompts for:
1. Mnemonic phrase (24 words, hidden input)
2. Encryption password (hidden, confirmed)

**Non-interactive (scripting):**
```bash
export ERGORS_MNEMONIC="word1 word2 ... word24"
export ERGORS_CUSTODY_PASSWORD="your-password"
ergors keys import-mnemonic --label "my-key" --prefix akash
```

**Examples:**
```bash
# Import Ergors key (default prefix "ergo")
ergors keys import-mnemonic --label "ergors-wallet" --default

# Import Akash key (prefix "akash")
ergors keys import-mnemonic --label "akash-faucet" --prefix akash --default

# Import Cosmos Hub key (prefix "cosmos")
ergors keys import-mnemonic --label "cosmos-wallet" --prefix cosmos
```

**Note:**
Most Cosmos SDK chains use coin type 118 with different prefixes. The prefix determines
the address format (akash1..., cosmos1..., ergo1...), but the underlying key derivation
path remains the same (m/44'/118'/0'/0/0).

**Security:**
- Mnemonics are never stored in plaintext
- Encrypted using Argon2id + ChaCha20Poly1305
- Input is hidden (not visible in terminal or process list)
```

**Step 2: Run documentation check**

Run: `just lint-docs` (if available) or manually verify markdown formatting

**Step 3: Commit**

```bash
git add packages/ergors/CLI_REFERENCE.md
git commit -m "docs: add --prefix flag to keys import-mnemonic reference"
```

## Task 8: Add Unit Test for Prefix Support

**Files:**
- Modify existing test file in `packages/ho-std/src/keys/encrypted_cosmos.rs` (add to test module)

**Step 1: Write test for different prefixes**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_import_with_akash_prefix() {
        // Known test mnemonic
        let phrase = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
        let mut manager = EncryptedCosmosKeyManager::new();
        manager.unlock("test-password").unwrap();

        // Import with Akash prefix (coin type 118 is default)
        let (encrypted, account_info) = manager
            .import_mnemonic_with_label(
                "test-akash",
                phrase,
                "",
                "akash", // prefix
                "test-akash",
                false,
            )
            .unwrap();

        // Verify HD path uses coin type 118
        assert!(account_info.hd_path.contains("44'/118'"));

        // Verify address starts with akash1
        assert!(account_info.address.starts_with("akash1"));

        // Verify standard bech32 length
        assert_eq!(account_info.address.len(), 45);
    }

    #[test]
    fn test_import_with_cosmos_prefix() {
        let phrase = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
        let mut manager = EncryptedCosmosKeyManager::new();
        manager.unlock("test-password").unwrap();

        // Import with Cosmos prefix
        let (encrypted, account_info) = manager
            .import_mnemonic_with_label(
                "test-cosmos",
                phrase,
                "",
                "cosmos", // prefix
                "test-cosmos",
                false,
            )
            .unwrap();

        // Verify HD path uses coin type 118
        assert!(account_info.hd_path.contains("44'/118'"));

        // Verify address starts with cosmos1
        assert!(account_info.address.starts_with("cosmos1"));
    }

    #[test]
    fn test_different_prefixes_same_public_key() {
        let phrase = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
        let mut manager = EncryptedCosmosKeyManager::new();
        manager.unlock("test-password").unwrap();

        let (_, akash_info) = manager
            .import_mnemonic_with_label("akash", phrase, "", "akash", "akash", false)
            .unwrap();

        let (_, cosmos_info) = manager
            .import_mnemonic_with_label("cosmos", phrase, "", "cosmos", "cosmos", false)
            .unwrap();

        // Same mnemonic, same coin type, different prefixes = same public key
        assert_eq!(
            akash_info.public_key, cosmos_info.public_key,
            "Same coin type with different prefixes should produce same public key"
        );

        // But different addresses
        assert_ne!(
            akash_info.address, cosmos_info.address,
            "Different prefixes should produce different addresses"
        );
    }
}
```

**Step 2: Run tests**

Run: `cargo tes -p ho-std -- encrypted_cosmos`
Expected: All 3 new tests pass

**Step 3: Commit**

```bash
git add packages/ho-std/src/keys/encrypted_cosmos.rs
git commit -m "test: add unit tests for bech32 prefix support"
```

## Task 9: Final Integration Test

**Files:**
- Test: `tests/e2e/tests/grants.sh`

**Step 1: Run full E2E test suite**

Run: `just e2e all --verbose`
Expected: All tests pass, no regressions

**Step 2: Specifically verify grant tests**

Run: `just e2e grants --verbose 2>&1 | tee /tmp/grants-test.log`

Look for:
- ✓ "Using existing E2E Faucet Key: akash1..."
- ✓ "Faucet has balance: 10000000 AKT"
- ✓ Grant request/approval tests pass
- ✓ No "invalid bech32" errors

**Step 3: Create summary commit**

```bash
git add .
git commit -m "feat: complete bech32 prefix support for Cosmos keys

- Add --prefix flag to ergors keys import-mnemonic
- Update E2E tests to use prefix akash for Akash
- Clarify that Akash uses coin type 118 (same as Cosmos)
- Add unit tests for bech32 prefix derivation
- Update CLI documentation

Fixes grant E2E test failures due to address prefix mismatch."
```

---

## Testing Checklist

**Unit Tests:**
- [ ] `cargo tes -p ho-std` - All pass
- [ ] `cargo tes -p ergors` - All pass
- [ ] Bech32 prefix derivation tests pass

**E2E Tests:**
- [ ] `just e2e grants` - Passes without "invalid bech32" errors
- [ ] `just e2e network` - No regressions
- [ ] `just e2e all` - Full suite passes

**Manual Testing:**
- [ ] `ergors keys import-mnemonic --label test --prefix akash` - Works
- [ ] `ergors keys import-mnemonic --label test2` - Defaults to "ergo"
- [ ] `ergors keys list` - Shows correct addresses with prefixes

## Success Criteria

1. ✅ CLI accepts `--prefix` flag
2. ✅ Default prefix is "ergo" (Ergors standard)
3. ✅ Akash faucet keys use prefix "akash" (coin type 118)
4. ✅ E2E grant tests pass with correct addresses
5. ✅ Documentation updated and clarifies coin type 118 is shared
6. ✅ Unit tests cover bech32 prefix derivation
7. ✅ No regressions in other tests

---

**Implementation Complete!** This plan adds full bech32 prefix support to ergors key management, fixing the Akash faucet address mismatch in E2E tests.
