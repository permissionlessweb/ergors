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
cw := "contracts-optimize"
ct := "contracts-test"
