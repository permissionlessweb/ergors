# Multi-stage build for minimal production image
FROM rust:1.76-slim as builder

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build

# Copy workspace files first (for layer caching)
COPY Cargo.toml Cargo.lock ./
COPY packages ./packages
COPY proto ./proto
COPY tests ./tests

# Build release binary
# Use --locked to ensure reproducible builds
RUN cargo build --release --locked --bin ergors

# Production image - minimal
FROM debian:bookworm-slim

# Install runtime dependencies only
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user for security
RUN useradd -m -u 1000 ergors

# Copy binary from builder
COPY --from=builder /build/target/release/ergors /usr/local/bin/ergors

# Set ownership
RUN chown ergors:ergors /usr/local/bin/ergors

# Switch to non-root user
USER ergors

# Data directory — must match ERGORS_HOME
ENV ERGORS_HOME=/home/ergors/.ergors
RUN mkdir -p "$ERGORS_HOME"

WORKDIR /home/ergors

# Sentinel mode: set ERGORS_ADMIN_PUBKEY to activate
# Full list of env vars:
#   ERGORS_ADMIN_PUBKEY  - Ed25519 public key hex (triggers sentinel mode)
#   ERGORS_HOME          - data directory (default: /home/ergors/.ergors)
#   ERGORS_API_PORT      - HTTP API port (default: 8080)
#   ERGORS_GRPC_PORT     - gRPC management port (default: 50051)
#   NOTE: Mnemonic for identity recovery is provided via signed /sentinel/init
#         API call only — never as an env var (visible to infrastructure providers)

EXPOSE 8080 26969 50051

# Health check — works during both sentinel phase and full server
HEALTHCHECK --interval=10s --timeout=3s --start-period=15s --retries=3 \
    CMD curl -sf http://localhost:${ERGORS_API_PORT:-8080}/sentinel/health || \
        curl -sf http://localhost:${ERGORS_API_PORT:-8080}/health || exit 1

ENTRYPOINT ["ergors"]
CMD ["start"]
