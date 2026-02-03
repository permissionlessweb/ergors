#!/bin/sh
# Example: Chained SDL deployment workflow for Terp Network O-Line
# This demonstrates the contract workflow pattern, NOT actual CLI commands
# (CLI integration is handled by packages/cw-ho)

set -e

# Step 1: Kickoff - Deploy Snapshot Node (NODE_A)
# ================================================

# Instantiate cw-sdl contract with snapshot node SDL
# The SDL template contains variables like ${CPU}, ${MEMORY}, ${PERSISTENT_PEERS}, etc.
echo "1. Instantiate cw-sdl-snapshot..."
# cw-ho contract instantiate cw-sdl \
#   --sdl-template snapshot-sdl.json \
#   --variable-defaults '{"CPU":"4","MEMORY":"8Gi","PERSISTENT_PEERS":""}' \
#   --label "snapshot-node" \
#   --admin $ADMIN_ADDR

# Deploy the rendered SDL to Akash
echo "2. Deploy snapshot node to Akash..."
# cw-ho deploy akash \
#   --contract $CW_SDL_SNAPSHOT \
#   --provider $PROVIDER

# CLI retrieves deployment results from Akash (peer ID, endpoints, etc.)
# Then records them to the contract state
echo "3. Record snapshot deployment results..."
# cw-ho contract execute $CW_SDL_SNAPSHOT record-deployment-results \
#   --results '{"SNAPSHOT_PEER_ID":"abc123@1.2.3.4:26656","SNAPSHOT_RPC":"https://snapshot-rpc.example.com:26657"}'

# Step 2: Deploy Seed Node (NODE_B) with Snapshot Peer
# ======================================================

echo "4. Instantiate cw-sdl-seed via factory from snapshot contract..."
# Query snapshot's deployment results
# SNAPSHOT_RESULTS=$(cw-ho contract query $CW_SDL_SNAPSHOT list-deployment-results)

# Factory-instantiate seed contract with snapshot results injected
# cw-ho contract execute $CW_SDL_SNAPSHOT instantiate-new \
#   --sdl-template seed-sdl.json \
#   --variable-defaults '{"CPU":"2","MEMORY":"4Gi"}' \
#   --parent-results "$SNAPSHOT_RESULTS" \
#   --label "seed-node"

# The seed SDL template has ${SNAPSHOT_PEER_ID} which gets populated from parent_results

echo "5. Deploy seed node to Akash..."
# cw-ho deploy akash --contract $CW_SDL_SEED --provider $PROVIDER

echo "6. Record seed deployment results..."
# cw-ho contract execute $CW_SDL_SEED record-deployment-results \
#   --results '{"SEED_PEER_ID":"def456@5.6.7.8:26656","SEED_RPC":"https://seed-rpc.example.com:26657"}'

# Step 3: Deploy Left Tackle (Sentry) with Snapshot + Seed Peers
# ================================================================

echo "7. Instantiate cw-sdl-left-tackle..."
# Need both snapshot AND seed results for tackle node
# COMBINED_RESULTS=$(jq -s '.[0] * .[1]' <(echo $SNAPSHOT_RESULTS) <(echo $SEED_RESULTS))

# cw-ho contract execute $CW_SDL_SNAPSHOT instantiate-new \
#   --sdl-template left-tackle-sdl.json \
#   --variable-defaults '{"CPU":"2","MEMORY":"4Gi"}' \
#   --parent-results "$COMBINED_RESULTS" \
#   --label "left-tackle"

# Tackle SDL has ${SNAPSHOT_PEER_ID}, ${SEED_PEER_ID}, ${RIGHT_TACKLE_PEER_ID} (empty for now)

echo "8. Deploy left tackle to Akash..."
# cw-ho deploy akash --contract $CW_SDL_LEFT_TACKLE --provider $PROVIDER

echo "9. Record left tackle deployment results..."
# cw-ho contract execute $CW_SDL_LEFT_TACKLE record-deployment-results \
#   --results '{"LEFT_TACKLE_PEER_ID":"ghi789@9.10.11.12:26656"}'

# Step 4: Deploy Right Tackle with Left Tackle Peer
# ===================================================

echo "10. Instantiate cw-sdl-right-tackle..."
# Merge snapshot + seed + left tackle results
# cw-ho contract execute $CW_SDL_SNAPSHOT instantiate-new \
#   --sdl-template right-tackle-sdl.json \
#   --parent-results "$ALL_RESULTS" \
#   --label "right-tackle"

# Step 5: Deploy Forward Nodes (Public)
# ======================================

echo "11. Deploy left forward (public node) with all upstream peers..."
# cw-ho contract execute $CW_SDL_SNAPSHOT instantiate-new \
#   --sdl-template left-forward-sdl.json \
#   --parent-results "$ALL_RESULTS" \
#   --label "left-forward"

# Left forward SDL uses:
# - ${SNAPSHOT_PEER_ID} for state sync
# - ${SEED_PEER_ID} for peer discovery
# - ${LEFT_TACKLE_PEER_ID} for unconditional peering

echo "12. Deploy right forward..."
# Same pattern, uses right tackle peer

# Summary
# =======
# This workflow demonstrates:
# 1. Sequential deployment where each node depends on results from previous ones
# 2. Factory pattern creates child contracts with parent results auto-injected
# 3. Deployment results (peer IDs, endpoints) stored in contract state
# 4. Child contract registry allows querying the entire deployment family
# 5. No manual variable passing - contract handles the plumbing

# Query the full deployment tree:
# cw-ho contract query $CW_SDL_SNAPSHOT list-child-contracts
# Returns: {"seed-node": "akash1...", "left-tackle": "akash1...", ...}

# Get any deployment result:
# cw-ho contract query $CW_SDL_SNAPSHOT get-deployment-result --key SNAPSHOT_PEER_ID
# Returns: {"key": "SNAPSHOT_PEER_ID", "value": "abc123@1.2.3.4:26656"}
