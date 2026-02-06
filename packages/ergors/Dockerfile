# Multi-stage build for minimal production image
FROM rust:1.87.0-slim-bookworm AS builder

# Install build dependencies
RUN apt-get update && apt-get install -y \
    build-essential \
    pkg-config \
    libssl-dev \
    ca-certificates \
    clang \
    libclang-dev \
    cmake \
    make \
    perl \
    python3 \
    python3-venv \
    python3-pip \
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
    python3 \
    python3-venv \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user for security
RUN useradd -m -u 1000 ergors

# Copy binary from builder
COPY --from=builder /build/target/release/ergors /usr/local/bin/ergors

# Set ownership
RUN chown ergors:ergors /usr/local/bin/ergors

# Setup RLM Python venv
RUN python3 -m venv /opt/rlm-venv
COPY packages/ergors-rlm/python/requirements.txt /tmp/rlm-requirements.txt
RUN /opt/rlm-venv/bin/pip install --no-cache-dir -r /tmp/rlm-requirements.txt && rm /tmp/rlm-requirements.txt
COPY packages/ergors-rlm/python/ /opt/rlm/
RUN chown -R ergors:ergors /opt/rlm-venv /opt/rlm
ENV RLM_PYTHON_PATH=/opt/rlm-venv/bin/python3
ENV RLM_SCRIPT_DIR=/opt/rlm

# Switch to non-root user
USER ergors

# Create data directory
RUN mkdir -p /home/ergors/.ergors

WORKDIR /home/ergors

# Expose ports
# P2P port (commonware)
EXPOSE 26969
# HTTP API port
EXPOSE 8080

# Default command: start node
ENTRYPOINT ["ergors"]
CMD ["start"]
