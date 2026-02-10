# ════════════════════════════════════════════════════════════════════════════
# ERGORS Justfile - Task runner for building, testing, and installing
# Install just: cargo install just
# ════════════════════════════════════════════════════════════════════════════

# Default install location (typically in PATH)
install_dir := env_var_or_default("CARGO_HOME", env_var("HOME") + "/.cargo") + "/bin"

# Package names
engine := "ergors"
lib := "ho-std"
proto := "ergors-proto"

# ════════════════════════════════════════════════════════════════════════════
# Installation - build and install binaries to PATH
# ════════════════════════════════════════════════════════════════════════════

# Install both engine and CLI to ~/.cargo/bin (default)
install: build-release
    @echo "📦 Installing binaries to {{install_dir}}"
    @cp target/release/{{engine}} {{install_dir}}/{{engine}}
    @echo "✅ Installed: {{engine}}"
    @echo "   Run 'ergors --help'"

# Install only the engine
install-engine: (build-pkg engine "release")
    @cp target/release/{{engine}} {{install_dir}}/{{engine}}
    @echo "✅ Installed: {{engine}}"

# Install only the CLI
# install-cli: (build-pkg cli "release")
#    @cp target/release/{{cli}} {{install_dir}}/{{cli}}
#     @echo "✅ Installed: {{cli}}"

# Uninstall binaries
uninstall:
    @rm -f {{install_dir}}/{{engine}}
    @echo "🗑️  Uninstalled: {{engine}}"

# ════════════════════════════════════════════════════════════════════════════
# Building
# ════════════════════════════════════════════════════════════════════════════

# Build all packages in release mode
build-release:
    @echo "🔨 Building release binaries..."
    cargo build --release -p {{engine}} 

# Build all packages in debug mode
build:
    cargo build -p {{engine}}
# Build specific package
build-pkg pkg mode="debug":
    @if [ "{{mode}}" = "release" ]; then \
        cargo build --release -p {{pkg}}; \
    else \
        cargo build -p {{pkg}}; \
    fi

# Build with all features
build-all-features:
    cargo build --release --all-features -p {{engine}}

# ════════════════════════════════════════════════════════════════════════════
# Development
# ════════════════════════════════════════════════════════════════════════════

# Run engine in development mode
dev *args:
    RUST_BACKTRACE=1 cargo run -p {{engine}} -- {{args}}

# # Run CLI in development mode
# cli *args:
#     RUST_BACKTRACE=1 cargo run -p {{cli}} -- {{args}}

# Initialize a new node (dev mode)
init:
    @just dev init

# Initialize LLM providers
init-llms:
    @just dev init llms

# Start the engine
start:
    @just dev start

# Watch and rebuild on changes (requires cargo-watch)
watch:
    cargo watch -x "build -p {{engine}}"

# Sync .team/agents/ to .claude/skills/ structure
sync-agents:
    @./scripts/sync-claude-agents.sh

# ════════════════════════════════════════════════════════════════════════════
# Proto generation
# ════════════════════════════════════════════════════════════════════════════

# Vendor external proto dependencies (k8s, etc.) using Go modules
modvendor:
    #!/bin/bash
    set -e
    echo "📦 Vendoring external proto dependencies..."

    # Use Go modules to vendor k8s.io/apimachinery and other dependencies
    cd proto
    go mod tidy
    go mod vendor

    echo "✅ Vendored dependencies installed to proto/vendor/"
    echo "   (This directory is in .gitignore and will not be committed)"

# Regenerate proto types (vendors dependencies first)
proto: modvendor
    @echo "🔄 Regenerating proto types..."
    cargo run -p {{proto}}
    @echo "✅ Proto types regenerated"

# ════════════════════════════════════════════════════════════════════════════
# Testing & Quality
# ════════════════════════════════════════════════════════════════════════════

# Run all tests
test:
    cargo test --workspace

# Run tests for specific package
test-pkg pkg:
    cargo test -p {{pkg}}

# Run tests with output
test-verbose:
    cargo test --workspace -- --nocapture

# Run provider compatibility test (Rust JWT + manifest → Go provider verification)
test-jwt:
    @cd tests/scripts/jwt-verify && just test

# ════════════════════════════════════════════════════════════════════════════
# E2E Testing - Unified Interface
# ════════════════════════════════════════════════════════════════════════════
#
# Usage:
#   just e2e                          # Run all test suites
#   just e2e <suite>                  # Run specific suite
#   just e2e <suite> --verbose        # Run with verbose output
#   just e2e <suite> --skip-build     # Skip building ergors
#   just e2e list                     # List available test suites
#   just e2e help                     # Show detailed help
#
# Available Suites:
#   all              - Run all test suites (default)
#   network          - Network setup and connectivity tests
#   grants           - Grant management and validation tests
#   deployment       - Akash deployment workflow tests
#   security         - Security and permissions tests
#   contracts        - CosmWasm contract integration tests
#   api              - gRPC/REST API endpoint tests
#   bootstrap        - Node bootstrap and P2P file transfer tests
#   ethereum         - Ethereum integration tests
#   inference        - LLM inference proxy routing tests
#   sdl-storage      - SDL storage and retrieval tests
#   chain-config     - Chain configuration tests
#   sentinel         - Sentinel mode tests (standalone, no infra)
#   provider-roles   - Engine role assignment tests
#
# Common Options:
#   --skip-build        Skip building ergors binary
#   --skip-contracts    Skip building CosmWasm contracts
#   --skip-network      Skip ERGORS network setup (use existing)
#   --skip-akash        Skip Akash/Kind setup (use existing)
#   --skip-cleanup      Keep everything running after tests
#   --skip-ethereum     Skip Ethereum/Anvil setup
#   --skip-inference    Skip mock inference provider
#   --verbose           Enable verbose output
#
# Examples:
#   just e2e inference                    # Test inference routing
#   just e2e inference --verbose          # With verbose output
#   just e2e deployment --skip-build      # Skip build step
#   just e2e sentinel --skip-cleanup      # Keep processes running
#   just e2e all --verbose --skip-build   # All tests, verbose, no build
#
# ════════════════════════════════════════════════════════════════════════════

# Run E2E tests (default: all suites)
[private]
e2e-run suite="all" *args="":
    @bash tests/e2e/main.sh --test {{suite}} {{args}}

# E2E test dispatcher with smart defaults
e2e suite="all" *args="":
    #!/usr/bin/env bash
    set -euo pipefail

    # Handle special commands
    case "{{suite}}" in
        list)
            echo "📋 Available E2E Test Suites:"
            echo ""
            echo "  all              Run all test suites (default)"
            echo "  network          Network setup and connectivity tests"
            echo "  grants           Grant management and validation tests"
            echo "  deployment       Akash deployment workflow tests"
            echo "  security         Security and permissions tests"
            echo "  contracts        CosmWasm contract integration tests"
            echo "  api              gRPC/REST API endpoint tests"
            echo "  bootstrap        Node bootstrap and P2P file transfer tests"
            echo "  ethereum         Ethereum integration tests"
            echo "  inference        LLM inference proxy routing tests"
            echo "  sdl-storage      SDL storage and retrieval tests"
            echo "  chain-config     Chain configuration tests"
            echo "  sentinel         Sentinel mode tests (standalone)"
            echo "  document         Document storage tests (ingest, retrieve, list, delete)"
            echo "  provider-roles   Engine role assignment tests (assign, unassign, list)"
            echo ""
            echo "Usage: just e2e <suite> [options]"
            echo "Run 'just e2e help' for detailed documentation"
            ;;

        help)
            echo "════════════════════════════════════════════════════════════════"
            echo "ERGORS E2E Testing - Unified Interface"
            echo "════════════════════════════════════════════════════════════════"
            echo ""
            echo "USAGE:"
            echo "  just e2e [suite] [options]"
            echo ""
            echo "SUITES:"
            echo "  all              Run all test suites (default)"
            echo "  network          Network setup and connectivity"
            echo "  grants           Grant management and validation"
            echo "  deployment       Akash deployment workflows"
            echo "  security         Security and permissions"
            echo "  contracts        CosmWasm contract integration"
            echo "  api              gRPC/REST API endpoints"
            echo "  bootstrap        Node bootstrap and P2P transfers"
            echo "  ethereum         Ethereum integration"
            echo "  inference        LLM inference proxy routing"
            echo "  sdl-storage      SDL storage and retrieval"
            echo "  chain-config     Chain configuration"
            echo "  sentinel         Sentinel mode (standalone)"
            echo "  provider-roles   Engine role assignments"
            echo ""
            echo "OPTIONS:"
            echo "  --skip-build        Skip building ergors binary"
            echo "  --skip-contracts    Skip building CosmWasm contracts"
            echo "  --skip-network      Skip ERGORS network setup"
            echo "  --skip-akash        Skip Akash/Kind setup"
            echo "  --skip-cleanup      Keep everything running after tests"
            echo "  --skip-ethereum     Skip Ethereum/Anvil setup"
            echo "  --skip-inference    Skip mock inference provider"
            echo "  --verbose           Enable verbose output"
            echo ""
            echo "EXAMPLES:"
            echo "  just e2e                          # Run all tests"
            echo "  just e2e inference                # Test inference only"
            echo "  just e2e inference --verbose      # With verbose output"
            echo "  just e2e deployment --skip-build  # Skip build step"
            echo "  just e2e all --skip-cleanup       # Keep running after"
            echo ""
            echo "INFRASTRUCTURE AUTO-SKIP:"
            echo "  Tests automatically skip unnecessary infrastructure:"
            echo "  • network, contracts, api → skip Akash, Ethereum, inference"
            echo "  • inference → skip Akash, Ethereum"
            echo "  • grants, bootstrap → skip Ethereum, inference"
            echo ""
            echo "For more details, see: tests/e2e/README.md"
            ;;

        *)
            # Validate suite name
            VALID_SUITES=(all network grants deployment security contracts api bootstrap ethereum inference sdl-storage chain-config sentinel document provider-roles)
            SUITE_VALID=false
            for valid in "${VALID_SUITES[@]}"; do
                if [[ "{{suite}}" == "$valid" ]]; then
                    SUITE_VALID=true
                    break
                fi
            done

            if [[ "$SUITE_VALID" == "false" ]]; then
                echo "❌ Unknown test suite: {{suite}}"
                echo ""
                echo "Available suites:"
                echo "  all, network, grants, deployment, security, contracts, api,"
                echo "  bootstrap, ethereum, inference, sdl-storage, chain-config, sentinel, document, provider-roles"
                echo ""
                echo "Run 'just e2e list' to see all suites"
                echo "Run 'just e2e help' for detailed help"
                exit 1
            fi

            # Run the test suite
            echo "🧪 Running E2E test suite: {{suite}}"
            bash tests/e2e/main.sh --test {{suite}} {{args}}
            ;;
    esac

# Generate code coverage report (format: html or json, default: html)
coverage format="html" *args="--workspace":
    #!/bin/bash
    set -e
    IGNORE_FLAGS="--ignore-filename-regex 'packages/ho-std/src/types/ergors/gen/.*' --ignore-filename-regex 'proto/.*'"
    if [ "{{format}}" = "json" ]; then
        echo "Generating JSON coverage report..."
        eval cargo llvm-cov {{args}} --json --output-path coverage.json $IGNORE_FLAGS
        echo "Coverage report: coverage.json"
    elif [ "{{format}}" = "html" ]; then
        echo "Generating HTML coverage report..."
        eval cargo llvm-cov {{args}} --html --output-dir coverage $IGNORE_FLAGS
        echo "Coverage report: coverage/html/index.html"
    else
        echo "Unknown format: {{format}}. Use 'html' or 'json'."
        exit 1
    fi

# Quick check (faster than full build)
check:
    cargo chec

# Full check with all features
check-all:
    cargo chec --workspace --all-features

# Clippy lints
clippy:
    cargo clippy --workspace -- -D warnings

# Format code
fmt:
    cargo fmt --all

# Format check (for CI)
fmt-check:
    cargo fmt --all -- --check

# ════════════════════════════════════════════════════════════════════════════
# CI Pipeline
# ════════════════════════════════════════════════════════════════════════════

# Full CI pipeline
ci: fmt-check clippy test build-release
    @echo "✅ CI pipeline passed"

# Quick CI (skip slow tests)
ci-quick: fmt-check check clippy
    @echo "✅ Quick CI passed"

# ════════════════════════════════════════════════════════════════════════════
# Documentation
# ════════════════════════════════════════════════════════════════════════════

# Build documentation
doc:
    cargo doc --workspace --no-deps

# Build and open documentation
doc-open:
    cargo doc --workspace --no-deps --open

# ════════════════════════════════════════════════════════════════════════════
# Cleanup
# ════════════════════════════════════════════════════════════════════════════

# Clean build artifacts
clean:
    cargo clean

# Clean and rebuild
rebuild: clean build-release

# ════════════════════════════════════════════════════════════════════════════
# Release
# ════════════════════════════════════════════════════════════════════════════

# Create release build with optimizations
release: proto build-release
    @echo "📦 Release build complete"
    @ls -lh target/release/{{engine}}

# Build for distribution (uses dist profile)
dist:
    cargo build --profile dist -p {{engine}}

# ════════════════════════════════════════════════════════════════════════════
# CosmWasm Contracts
# ════════════════════════════════════════════════════════════════════════════

# Build optimized WASM contracts for all contracts in workspace
contracts-optimize:
    #!/bin/bash
    cd contracts
    if [[ $(uname -m) == 'arm64' ]] || [[ $(uname -m) == 'aarch64' ]]; then \
        echo "🔨 Building optimized contracts for ARM64..."; \
        docker run --rm -v "$(pwd)":/code \
            --mount type=volume,source="$(basename "$(pwd)")_cache",target=/target \
            --mount type=volume,source=registry_cache,target=/usr/local/cargo/registry \
            --platform linux/arm64 \
            cosmwasm/optimizer-arm64:0.17.0; \
    elif [[ $(uname -m) == 'x86_64' ]]; then \
        echo "🔨 Building optimized contracts for x86_64..."; \
        docker run --rm -v "$(pwd)":/code \
            --mount type=volume,source="$(basename "$(pwd)")_cache",target=/target \
            --mount type=volume,source=registry_cache,target=/usr/local/cargo/registry \
            --platform linux/amd64 \
            cosmwasm/optimizer:0.17.0; \
    else \
        echo "❌ Unsupported architecture: $(uname -m)"; \
        exit 1; \
    fi
    echo "✅ Optimized contracts available in contracts/artifacts/"

# Test all contracts
contracts-test:
    cd contracts && cargo test --workspace

# Check all contracts
contracts-check:
    cd contracts && cargo chec

# Generate contract schemas
contracts-schema:
    #!/bin/bash
    cd contracts
    for contract in */; do \
        if [ -f "$contract/Cargo.toml" ]; then \
            echo "📄 Generating schema for $contract"; \
            cd "$contract" && cargo schema 2>/dev/null || true; \
            cd ..; \
        fi \
    done
    echo "✅ Schemas generated"

# Build contracts in debug mode
contracts-build:
    cd contracts && cargo build --workspace

# Clean contract artifacts
contracts-clean:
    cd contracts && cargo clean
    rm -rf contracts/artifacts

# ════════════════════════════════════════════════════════════════════════════
# Utilities
# ════════════════════════════════════════════════════════════════════════════

# Show binary versions
version:
    @echo "Engine version:"
    @cargo run -p {{engine}} -q -- --version 2>/dev/null || echo "  (not built)"

# Show installed binary locations
which:
    @echo "Installed binaries:"
    @which {{engine}} 2>/dev/null || echo "  {{engine}}: not found in PATH"

# Print environment info
env:
    @echo "Install directory: {{install_dir}}"
    @echo "Packages: {{engine}}, {{lib}}"
    @echo "Rust version: $(rustc --version)"
    @echo "Cargo version: $(cargo --version)"

# List available recipes
help:
    @just --list

# ════════════════════════════════════════════════════════════════════════════
# Shortcuts
# ════════════════════════════════════════════════════════════════════════════

# Aliases for common operations
b := "build"
r := "build-release"
t := "test"
c := "check"
chec := "check"
i := "install"
cov := "coverage"
cw := "contracts-optimize"
ct := "contracts-test"

# E2E test suite shortcuts
e2e-inf := "e2e inference"
e2e-net := "e2e network"
e2e-api := "e2e api"
e2e-dep := "e2e deployment"
e2e-sec := "e2e security"
e2e-sen := "e2e sentinel"
e2e-doc := "e2e document"
e2e-roles := "e2e provider-roles"
