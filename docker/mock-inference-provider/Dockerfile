# Build stage
FROM rust:1.82-slim-bookworm AS builder

WORKDIR /app

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Copy Cargo files first for dependency caching
COPY Cargo.toml ./

# Create a dummy main.rs to build dependencies
RUN mkdir -p src && echo "fn main() {}" > src/main.rs

# Build dependencies only (this layer will be cached)
RUN cargo build --release && rm -rf src target/release/mock-inference-provider*

# Copy actual source code
COPY src ./src

# Build the actual application
RUN cargo build --release

# Runtime stage
FROM debian:bookworm-slim

WORKDIR /app

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Copy the binary from builder
COPY --from=builder /app/target/release/mock-inference-provider /usr/local/bin/

# Create non-root user
RUN useradd -m -u 1000 inference && chown -R inference:inference /app
USER inference

# Default environment variables
ENV PORT=11434
ENV HOST=0.0.0.0
ENV MIN_LATENCY_MS=50
ENV MAX_LATENCY_MS=200
ENV ERROR_RATE=0.0
ENV MODEL_NAME=llama2
ENV RUST_LOG=info

# Expose default port (Ollama compatible)
EXPOSE 11434

# Health check
HEALTHCHECK --interval=30s --timeout=10s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:${PORT}/health || exit 1

# Run the server
ENTRYPOINT ["mock-inference-provider"]
CMD ["--port", "11434"]
