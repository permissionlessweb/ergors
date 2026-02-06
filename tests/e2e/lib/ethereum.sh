#!/bin/bash
#
# ethereum.sh - Local Ethereum test network management for E2E tests
#
# Provides: Anvil (Foundry) setup, start/stop, funded accounts, JSON-RPC helpers

# Prevent multiple sourcing
[[ -n "${_E2E_ETHEREUM_LOADED:-}" ]] && return 0
_E2E_ETHEREUM_LOADED=1

# =============================================================================
# Configuration
# =============================================================================
ANVIL_PORT="${ANVIL_PORT:-8545}"
ANVIL_CHAIN_ID="${ANVIL_CHAIN_ID:-31337}"
ANVIL_RPC="http://127.0.0.1:${ANVIL_PORT}"
ANVIL_PID=""

# Anvil pre-funded accounts (deterministic from default mnemonic)
# Mnemonic: "test test test test test test test test test test test junk"
ANVIL_MNEMONIC="test test test test test test test test test test test junk"
# Account 0: 0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266 (10000 ETH)
ANVIL_ACCOUNT_0="0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266"
ANVIL_PRIVATE_KEY_0="0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80"
# Account 1: 0x70997970C51812dc3A010C7d01b50e0d17dc79C8 (10000 ETH)
ANVIL_ACCOUNT_1="0x70997970C51812dc3A010C7d01b50e0d17dc79C8"
ANVIL_PRIVATE_KEY_1="0x59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d"

# =============================================================================
# Installation Check
# =============================================================================

# Check if anvil is installed
ethereum_check_anvil() {
    if command -v anvil &>/dev/null; then
        return 0
    fi
    return 1
}

# Install Foundry (anvil + cast + forge) if not present
ethereum_install_foundry() {
    if ethereum_check_anvil; then
        log_verbose "Anvil already installed: $(anvil --version 2>&1 | head -1)"
        return 0
    fi

    log "Installing Foundry (anvil)..."
    if curl -L https://foundry.paradigm.xyz | bash 2>/dev/null; then
        # Source the updated PATH
        export PATH="${HOME}/.foundry/bin:${PATH}"
        # Run foundryup to install binaries
        if command -v foundryup &>/dev/null; then
            foundryup 2>&1 | tail -5
        fi
    fi

    if ethereum_check_anvil; then
        log_success "Foundry installed: $(anvil --version 2>&1 | head -1)"
        return 0
    else
        log_error "Failed to install Foundry"
        return 1
    fi
}

# =============================================================================
# Anvil Lifecycle
# =============================================================================

# Start a local Anvil Ethereum node
ethereum_start_anvil() {
    log_step "Starting Local Ethereum Network (Anvil)"

    # Ensure anvil is available
    if ! ethereum_check_anvil; then
        ethereum_install_foundry || return 1
    fi

    # Kill any existing process on the port
    kill_port "$ANVIL_PORT" 2>/dev/null || true
    sleep 1

    local anvil_log="${TEST_DIR:-/tmp}/anvil.log"

    log "Starting Anvil on port $ANVIL_PORT (chain ID: $ANVIL_CHAIN_ID)..."
    anvil \
        --port "$ANVIL_PORT" \
        --chain-id "$ANVIL_CHAIN_ID" \
        --mnemonic "$ANVIL_MNEMONIC" \
        --accounts 10 \
        --balance 10000 \
        --block-time 1 \
        --silent \
        > "$anvil_log" 2>&1 &
    ANVIL_PID=$!
    register_pid "$ANVIL_PID"

    # Wait for Anvil to be ready
    if wait_for_port "127.0.0.1" "$ANVIL_PORT" 15; then
        log_success "Anvil running (PID: $ANVIL_PID, RPC: $ANVIL_RPC)"
        export ANVIL_RPC ANVIL_PORT ANVIL_CHAIN_ID ANVIL_PID
        return 0
    else
        log_error "Anvil failed to start"
        [[ -f "$anvil_log" ]] && tail -20 "$anvil_log"
        return 1
    fi
}

# Stop the Anvil node
ethereum_stop_anvil() {
    if [[ -n "$ANVIL_PID" ]]; then
        log "Stopping Anvil (PID: $ANVIL_PID)..."
        kill_with_timeout "$ANVIL_PID" 3
        ANVIL_PID=""
    fi
    kill_port "$ANVIL_PORT" 2>/dev/null || true
    log_verbose "Anvil stopped"
}

# Check if Anvil is healthy via JSON-RPC
ethereum_anvil_healthy() {
    local result
    result=$(curl -s --max-time 2 -X POST "$ANVIL_RPC" \
        -H "Content-Type: application/json" \
        -d '{"jsonrpc":"2.0","method":"eth_chainId","params":[],"id":1}' 2>/dev/null) || return 1

    json_has "$result" '.result'
}

# =============================================================================
# JSON-RPC Helpers
# =============================================================================

# Make a raw JSON-RPC call to Anvil
# Usage: eth_rpc <method> <params_json>
eth_rpc() {
    local method="$1"
    local params="${2:-[]}"

    curl -s --max-time 15 -X POST "$ANVIL_RPC" \
        -H "Content-Type: application/json" \
        -d "{\"jsonrpc\":\"2.0\",\"method\":\"$method\",\"params\":$params,\"id\":1}" \
        2>/dev/null || echo '{"error":"rpc request failed"}'
}

# Get the hex result field from an RPC response
# Usage: eth_rpc_result <rpc_response_json>
eth_rpc_result() {
    local response="$1"
    json_get "$response" '.result'
}

# Convert hex string to decimal
# Usage: eth_hex_to_dec <hex_string>
eth_hex_to_dec() {
    local hex="$1"
    hex="${hex#0x}"
    printf "%d" "0x${hex}" 2>/dev/null || echo "0"
}

# Convert decimal wei to ETH string
# Usage: eth_wei_to_eth <wei_decimal>
eth_wei_to_eth() {
    local wei="$1"
    echo "scale=6; $wei / 1000000000000000000" | bc 2>/dev/null || echo "?"
}

# =============================================================================
# Account Helpers
# =============================================================================

# Query ETH balance of an address (returns decimal wei)
# Usage: eth_query_balance <address>
eth_query_balance() {
    local address="$1"
    local response
    response=$(eth_rpc "eth_getBalance" "[\"$address\", \"latest\"]")
    local hex_balance
    hex_balance=$(eth_rpc_result "$response")
    if [[ -n "$hex_balance" ]] && [[ "$hex_balance" != "null" ]]; then
        eth_hex_to_dec "$hex_balance"
    else
        echo "0"
    fi
}

# Query nonce for an address (returns decimal)
# Usage: eth_query_nonce <address>
eth_query_nonce() {
    local address="$1"
    local response
    response=$(eth_rpc "eth_getTransactionCount" "[\"$address\", \"latest\"]")
    local hex_nonce
    hex_nonce=$(eth_rpc_result "$response")
    if [[ -n "$hex_nonce" ]] && [[ "$hex_nonce" != "null" ]]; then
        eth_hex_to_dec "$hex_nonce"
    else
        echo "0"
    fi
}

# Query current block number (returns decimal)
eth_query_block_number() {
    local response
    response=$(eth_rpc "eth_blockNumber" "[]")
    local hex_block
    hex_block=$(eth_rpc_result "$response")
    if [[ -n "$hex_block" ]] && [[ "$hex_block" != "null" ]]; then
        eth_hex_to_dec "$hex_block"
    else
        echo "0"
    fi
}

# Query gas price (returns decimal wei)
eth_query_gas_price() {
    local response
    response=$(eth_rpc "eth_gasPrice" "[]")
    local hex_price
    hex_price=$(eth_rpc_result "$response")
    if [[ -n "$hex_price" ]] && [[ "$hex_price" != "null" ]]; then
        eth_hex_to_dec "$hex_price"
    else
        echo "0"
    fi
}

# Query chain ID (returns decimal)
eth_query_chain_id() {
    local response
    response=$(eth_rpc "eth_chainId" "[]")
    local hex_chain
    hex_chain=$(eth_rpc_result "$response")
    if [[ -n "$hex_chain" ]] && [[ "$hex_chain" != "null" ]]; then
        eth_hex_to_dec "$hex_chain"
    else
        echo "0"
    fi
}

# Send a simple ETH transfer using cast (if available)
# Usage: eth_send_transfer <from_private_key> <to_address> <amount_eth>
eth_send_transfer() {
    local private_key="$1"
    local to="$2"
    local amount="$3"

    if ! command -v cast &>/dev/null; then
        log_error "cast not available (install Foundry)"
        return 1
    fi

    cast send --rpc-url "$ANVIL_RPC" \
        --private-key "$private_key" \
        "$to" \
        --value "${amount}ether" \
        2>&1
}

# Get transaction receipt
# Usage: eth_get_receipt <tx_hash>
eth_get_receipt() {
    local tx_hash="$1"
    eth_rpc "eth_getTransactionReceipt" "[\"$tx_hash\"]"
}

# =============================================================================
# ERGORS Ethereum CLI Wrappers
# =============================================================================

# Derive ETH address through ergors CLI (when available)
ergors_eth_address() {
    local key_name="${1:-default}"
    local account_index="${2:-0}"

    ergors_cli deploy eth-address \
        --key-name "$key_name" \
        --account-index "$account_index" \
        2>&1
}

# Query ETH balance through ergors CLI (when available)
ergors_eth_balance() {
    local address="$1"
    local rpc_url="${2:-$ANVIL_RPC}"

    ergors_cli deploy eth-balance \
        --address "$address" \
        --rpc-url "$rpc_url" \
        2>&1
}

# Send ETH through ergors CLI (when available)
ergors_eth_send() {
    local to="$1"
    local amount="$2"
    local key_name="${3:-default}"
    local rpc_url="${4:-$ANVIL_RPC}"

    ergors_cli deploy eth-send \
        --to "$to" \
        --amount "$amount" \
        --key-name "$key_name" \
        --rpc-url "$rpc_url" \
        --chain-id "$ANVIL_CHAIN_ID" \
        2>&1
}

# =============================================================================
# Full Setup/Teardown
# =============================================================================

ethereum_setup() {
    ethereum_start_anvil
}

ethereum_cleanup() {
    ethereum_stop_anvil
}
