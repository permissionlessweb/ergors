# Embedding Inference Provider
# Serves SentenceTransformer models via OpenAI-compatible /v1/embeddings API
# Designed for Akash Network deployment

FROM python:3.11-slim-bookworm AS base

WORKDIR /app

# Install system dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Install Python dependencies
COPY requirements.txt .
RUN pip install --no-cache-dir -r requirements.txt

# Pre-download default model during build for faster cold starts
RUN python -c "from sentence_transformers import SentenceTransformer; SentenceTransformer('all-MiniLM-L6-v2')"

# Copy server code
COPY server.py .

# Create non-root user
RUN useradd -m -u 1000 embedding && chown -R embedding:embedding /app
USER embedding

# Configuration via environment
ENV PORT=8080
ENV HOST=0.0.0.0
ENV EMBEDDING_MODEL=all-MiniLM-L6-v2
ENV MAX_BATCH_SIZE=256

EXPOSE 8080

HEALTHCHECK --interval=30s --timeout=10s --start-period=30s --retries=3 \
    CMD curl -f http://localhost:${PORT}/health || exit 1

ENTRYPOINT ["python", "server.py"]
