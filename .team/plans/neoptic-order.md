# Nepotic Order: Dynamic Consensus Membership via ABCI

## Context

The current consensus system runs in "supreme leader" mode - a single node proposes, self-notarizes, and finalizes blocks. Other nodes are passive observers. If the supreme leader goes down, consensus halts entirely. We need the supreme leader to invite other nodes into Simplex consensus, with liveness parameters for when invited nodes go down, and the ability to rotate the leader role.

The existing ABCI pipeline (CheckTx → PrepareProposal → ProcessProposal → BeginBlock → DeliverTx → EndBlock → Commit) and the `ValidatorUpdate` type already exist but are unused. The invite/accept two-step flow maps naturally to consensus transactions validated through `check_tx`.

## Architecture Decision: Governance Data Transmission

**Problem:** `NodeCommitment` stores only `transition_digest = SHA256(data)` for privacy. Governance actions (invites, accepts, transfers) need their actual data visible to all validators for state machine execution.

**Solution:** Add a `governance_cache` to the consensus app + a new gossip message variant. When the supreme leader creates an invite, the raw governance proto bytes are gossiped alongside the commitment. Receiving nodes verify `SHA256(governance_data) == transition_digest` before caching. This avoids changing the 201-byte wire format.

---

## Implementation Phases

### Phase 1: Proto3 Governance Types

**New file:** `proto/ergors/consensus/v1/consensus.proto`

```protobuf
message InviteValidator {
  bytes invitee_pubkey = 1;    // Ed25519 32-byte pubkey
  uint64 voting_power = 2;    // Power when accepted
}

message AcceptInvite {
  bytes inviter_pubkey = 1;    // Supreme leader who invited
  uint64 invite_height = 2;   // Height of the InviteValidator commitment
}

message TransferLeadership {
  bytes new_leader_pubkey = 1; // Must be an active validator
}

message LivenessConfig {
  uint64 max_missed_blocks = 1;  // Blocks before power reduction
  uint64 jail_blocks = 2;        // Duration of power=0
}
```

Regenerate with `cargo run` in `proto/` directory.

### Phase 2: Extend CommitmentKind

**File:** `packages/ergors/src/consensus/types.rs`

Add three new variants to the existing enum:

- `ValidatorInvite = 6`
- `InviteAccept = 7`
- `LeaderTransfer = 8`

Update `from_u8()`, `Display`, and tests. Wire format is compatible (u8 byte at offset 128).

### Phase 3: Governance State in ConsensusApp

**File:** `packages/ergors/src/consensus/app.rs`

New fields on `ErgorsConsensusApp`:

- `invited: BTreeMap<[u8; 32], (u64, u64)>` - invitee pubkey -> (invited_height, power)
- `supreme_leader: [u8; 32]` - current leader's pubkey bytes
- `governance_cache: HashMap<[u8; 32], Vec<u8>>` - transition_digest -> raw proto bytes
- `last_seen: BTreeMap<[u8; 32], u64>` - validator -> last_committed_height
- `liveness_config: Option<LivenessConfig>` - liveness parameters
- `current_height: u64` - tracked during block pipeline
- `pending_accepted: Vec<([u8; 32], u64)>` - newly accepted invites this block

**Modify `check_tx`:**

- `ValidatorInvite` - only supreme leader can submit; governance_cache must have matching data
- `InviteAccept` - only nodes in `invited` set can submit; governance_cache required
- `LeaderTransfer` - only supreme leader can submit
- All other kinds - existing validator set check (unchanged)

**Modify `deliver_tx`:**

- `ValidatorInvite` - decode cached governance data, add to `invited` set, store in Cnidarium (`meta_ledger/governance/invited/{hex}`)
- `InviteAccept` - move from `invited` to `pending_accepted`, add to validators, store in Cnidarium (`meta_ledger/governance/validators/{hex}`)
- `LeaderTransfer` - store pending transfer in Cnidarium (`meta_ledger/governance/pending_transfer`)
- All other kinds - update `last_seen[pk] = current_height` for liveness tracking

**Modify `end_block`:**

- Check liveness: for each validator, if `current_height - last_seen > max_missed_blocks`, emit `ValidatorUpdate { power: 0 }`
- Process pending_accepted: emit `ValidatorUpdate` for each newly accepted validator
- Process leadership transfer if effective this epoch

**Cnidarium storage prefixes:**

```
meta_ledger/governance/supreme_leader     -> 32 bytes (pubkey)
meta_ledger/governance/validators/{hex}   -> 8 bytes (power u64 LE)
meta_ledger/governance/invited/{hex}      -> 16 bytes (height u64 LE + power u64 LE)
meta_ledger/governance/liveness/{hex}     -> 8 bytes (last_seen u64 LE)
meta_ledger/governance/liveness_config    -> serialized LivenessConfig proto
meta_ledger/governance/pending_transfer   -> 32+8 bytes (pubkey + epoch)
```

**Add `load_governance_state` method** (called on startup alongside `load_sequences`).

### Phase 4: Gossip Extension for Governance Data

**File:** `packages/ergors/src/consensus/gossip.rs`

New gossip message variant:

- `TAG_GOVERNANCE_DATA = 0x02`
- Wire: `[1 tag][32 digest][4 data_len][N data_bytes]`
- On receive: verify `SHA256(data) == digest`, insert into governance_cache

### Phase 5: Multi-Participant Engine & Epoch Restart

**File:** `packages/ergors/src/consensus/engine.rs`

Modify `ConsensusConfig`:

- Add `participants: Vec<ed25519::PublicKey>` (default: empty = supreme leader only)

Modify `start_consensus`:

- Use `participants` if non-empty: `Ordered: config.participants.into()`
- Otherwise fall back to `vec![config.supreme_leader].into()`

Add `restart_consensus()` function:

1. Load updated validator set from Cnidarium governance state
2. Load current supreme leader from Cnidarium
3. Stop existing engine (drop handle)
4. Call `start_consensus()` with new set and `epoch + 1`

### Phase 6: Management RPCs + CLI

**File:** `proto/ergors/management/v1/management.proto`

New RPCs:

- `InviteValidator(InviteValidatorRequest) returns (OperationResult)`
- `AcceptInvite(AcceptInviteRequest) returns (OperationResult)`
- `TransferLeadership(TransferLeadershipRequest) returns (OperationResult)`
- `GetValidatorSet(Empty) returns (ValidatorSetResponse)`

**File:** `packages/ergors/src/commands/mod.rs` - New `ConsensusCmd` subcommand

**File:** `packages/ergors/CLI_REFERENCE.md` - Document new commands

---

## Testing Strategy (TDD - write tests BEFORE implementation)

### Phase 2 Tests (types.rs)

- New CommitmentKind variants roundtrip through `from_u8`/`as_u8`
- Wire format roundtrip with kinds 6, 7, 8

### Phase 3 Tests (app.rs) - Most Critical

- `check_tx_accepts_invite_from_leader` - supreme leader can submit ValidatorInvite
- `check_tx_rejects_invite_from_non_leader` - non-leader cannot invite
- `check_tx_accepts_accept_from_invited` - invited node can submit InviteAccept
- `check_tx_rejects_accept_from_uninvited` - uninvited node rejected
- `check_tx_rejects_transfer_from_non_leader` - only leader can transfer
- `full_invite_accept_lifecycle` - invite -> accept -> validator set changes in end_block
- `liveness_reduces_power_after_threshold` - missed blocks trigger ValidatorUpdate power=0
- `transfer_leadership_effective_at_epoch` - leader designation changes

### Phase 4 Tests (gossip.rs)

- `wire_roundtrip_governance_data` - new message encodes/decodes
- `governance_data_rejected_on_digest_mismatch` - tampered data rejected

### Phase 5 Tests (engine.rs)

- `consensus_config_with_participants` - Ordered set built from participants list
- `start_consensus_multi_participant` - engine accepts multiple validators

---

## Verification

After each phase:

1. `cargo chec` - clean compilation
2. `cargo tes` - all tests pass (existing + new)

After all phases:
3. E2E test: start 3-node network, invite node 2, accept, verify validator set changes
4. E2E test: supreme leader transfers to node 2, verify new leader produces blocks
5. E2E test: node goes offline, verify liveness triggers power reduction after threshold

---

## Files Modified (Summary)

| File | Change |
|------|--------|
| `proto/ergors/consensus/v1/consensus.proto` | **NEW** - governance types |
| `packages/ergors/src/consensus/types.rs` | Add CommitmentKind variants 6-8 |
| `packages/ergors/src/consensus/app.rs` | Core governance: invited set, check_tx gating, deliver_tx processing, end_block validator updates, liveness, governance_cache |
| `packages/ergors/src/consensus/engine.rs` | Multi-participant support, epoch restart |
| `packages/ergors/src/consensus/gossip.rs` | GovernanceData message variant |
| `proto/ergors/management/v1/management.proto` | Management RPCs for CLI |
| `packages/ergors/src/client/grpc.rs` | gRPC handler implementations |
| `packages/ergors/src/commands/mod.rs` | ConsensusCmd CLI subcommand |
| `packages/ergors/CLI_REFERENCE.md` | Document new commands |
