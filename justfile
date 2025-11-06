# ────────────────────────────────────────────────────────────────
# Just – a tiny task runner (https://github.com/casey/just)
# Install locally with:
#   cargo install just
# ────────────────────────────────────────────────────────────────

# ---- Global configuration -------------------------------------------------
# All tasks run in the crate root (`just` is executed there by default)
# Use the same tool‑chain that CI will use.
# set shell := ["bash", "-eu", "-o", "pipefail", "-c"]
# set -e

# ---- Global configuration -------------------------------------------------
package := "ho-core"

# ---------------------------------------------------------------------------
# CI entry point – runs the whole pipeline in the right order
# ---------------------------------------------------------------------------
ci:
    @echo "===== 📦 CI pipeline for {{package}} ====="
    @just fmt
    @just clippy
    @just build
    @just test
    @just doc

# ---------------------------------------------------------------------------
# Individual steps – each can also be called directly (e.g. `just test`)
# ---------------------------------------------------------------------------
fmt:
    cargo fmt 

clippy:
    # Fail on warnings so CI catches them early
    cargo clippy -p {{package}} -- -D warnings  

build:
    # Build the binary (`{{package}}`) in release mode – fast for CI caches
    cargo build -p {{package}} --release

test:
    # Run unit / integration tests (including `dev-dependencies`)
    cargo test -p {{package}} --all-targets --locked

doc:
    # Build documentation (no‑run, no‑deps = faster, also checks doctests)
    cargo doc -p {{package}} --no-deps --all-features

dev:
    cargo build
    RUST_BACKTRACE=1 cargo run -- init
    RUST_BACKTRACE=1 cargo run -- start

# ---------------------------------------------------------------------------
# Optional convenience helpers (not required for CI, but nice locally)
# ---------------------------------------------------------------------------
clean:
    cargo clean -p {{package}}

bench:
    cargo bench -p {{package}}
