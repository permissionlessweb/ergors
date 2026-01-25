# Ragamuffin: RAG Embedding Pipeline

Ragamuffin generates semantic embeddings from code repositories and stores them in a FAISS index for retrieval-augmented generation. It deploys a SentenceTransformer-based inference provider on Akash Network via ergors, formats source repositories with githem, and produces a queryable vector store.

**Pipeline:** `Repository → githem (format) → Akash Provider (embed) → FAISS Store (retrieve)`

---

## Prerequisites

- ERGORS engine installed and running (`ergors start`)
- Python 3.10+ with pip
- [githem](https://github.com/githem/githem) (optional, for remote repo formatting)
- Akash account with funded wallet (for cloud deployment)

```bash
# Install Python dependencies
pip install -r tools/python/requirements.txt

# For local-only mode (no Akash needed)
pip install sentence-transformers torch
```

---

## Quick Start

### Local Mode (No Akash)

Embed a local repository using your machine's CPU:

```bash
./tools/raggamuffin.sh ~/projects/my-repo --local
```

Query results:

```bash
python3 tools/python/rag-retriever.py --store ./rag-store --local --query "how does authentication work"
```

### Akash Mode (Cloud Deployment)

Deploy an embedding provider to Akash and process a remote repo:

```bash
./tools/raggamuffin.sh https://github.com/user/repo
```

---

## Pipeline Walkthrough

### Step 1: Deploy Embedding Provider

Ragamuffin deploys a containerized SentenceTransformer model to Akash Network as an EXECUTOR node. The container exposes OpenAI-compatible `/v1/embeddings` and Ollama-compatible `/api/embeddings` endpoints.

```bash
# CPU deployment (all-MiniLM-L6-v2, 384d, ~80MB model)
ergors-cli deploy create \
  --sdl sdls/embeddings/embedding-provider.yml \
  --var EMBEDDING_MODEL=all-MiniLM-L6-v2

# GPU deployment (bge-large-en-v1.5, 1024d, larger models)
ergors-cli deploy create \
  --sdl sdls/embeddings/embedding-provider-gpu.yml \
  --var EMBEDDING_MODEL=BAAI/bge-large-en-v1.5
```

The deployment progresses through ergors' workflow steps:

```
key_selection → balance_check → grant_request → sdl_configuration →
certificate_setup → deployment_create → bid_wait → provider_selection →
lease_create → manifest_send → endpoint_retrieval → endpoint_testing → complete
```

Advance the workflow:

```bash
ergors deploy advance --name embedding-provider --wait
```

Retrieve the endpoint:

```bash
ergors deploy get --name embedding-provider
```

Verify health:

```bash
curl http://<akash-endpoint>:8080/health
# {"status":"healthy","model":"all-MiniLM-L6-v2","dimension":384,"max_batch_size":256}
```

### Step 2: Format Repository

Ragamuffin uses [githem](https://github.com/githem/githem) to consolidate a repository into a single LLM-ready document. This strips noise, respects `.gitignore`, and produces clean structured text.

```bash
# Remote repo via githem
githem https://github.com/user/repo --preset standard --output formatted_repo.txt

# Or use a local repo directly (raggamuffin handles this automatically)
./tools/raggamuffin.sh ~/projects/my-repo --skip-format
```

If githem is not installed, the pipeline falls back to cloning the repo and scanning files directly by extension.

**Supported file extensions:**
`.go` `.js` `.py` `.ts` `.java` `.c` `.cpp` `.h` `.hpp` `.php` `.sql` `.rs` `.rb` `.swift` `.kt` `.scala` `.zig` `.md` `.toml` `.yaml` `.yml` `.json`

### Step 3: Generate Embeddings

The embedding script chunks source text (default 500 chars, line-boundary aligned) and sends batches to the provider:

```bash
# Using remote Akash provider
python3 tools/python/rag-embedding.py \
  --input formatted_repo.txt \
  --provider-url http://<akash-endpoint>:8080 \
  --output ./rag-store

# Using local model
python3 tools/python/rag-embedding.py \
  --repo ~/projects/my-repo \
  --local \
  --output ./rag-store

# Custom chunk and batch sizes
python3 tools/python/rag-embedding.py \
  --input formatted_repo.txt \
  --provider-url http://host:8080 \
  --chunk-size 1000 \
  --batch-size 128 \
  --model BAAI/bge-large-en-v1.5
```

Output structure:

```
rag-store/
├── index.faiss       # FAISS inner-product index (normalized vectors)
├── chunks.pkl        # Chunk text + source metadata (pickle)
└── manifest.json     # Index info (dimension, count, timestamps)
```

### Step 4: Query Embeddings

The retriever client embeds your query using the same model/provider and searches the FAISS index for semantically similar chunks:

```bash
# Single query
python3 tools/python/rag-retriever.py \
  --store ./rag-store \
  --provider-url http://<akash-endpoint>:8080 \
  --query "error handling in the auth module"

# Interactive mode
python3 tools/python/rag-retriever.py \
  --store ./rag-store \
  --provider-url http://<akash-endpoint>:8080

# JSON output (for piping to other tools)
python3 tools/python/rag-retriever.py \
  --store ./rag-store \
  --local \
  --query "database connection" \
  --json \
  --top-k 10
```

Example output:

```
[1] Score: 0.8234 | Source: src/auth/handler.rs
    pub async fn verify_token(token: &str) -> Result<Claims> {
        let decoded = decode::<Claims>(token, &KEYS.decoding, &Validation::default())
            .map_err(|e| AuthError::InvalidToken(e.to_string()))?;
    ...

[2] Score: 0.7891 | Source: src/auth/middleware.rs
    pub async fn auth_middleware(req: Request, next: Next) -> Response {
        let token = req.headers().get("authorization")
    ...
```

---

## Orchestration Script

`tools/raggamuffin.sh` combines all steps into a single command:

```bash
./tools/raggamuffin.sh <repo-url-or-path> [options]
```

### Options

| Flag | Default | Description |
|------|---------|-------------|
| `--model MODEL` | `all-MiniLM-L6-v2` | Embedding model name |
| `--output DIR` | `./rag-store` | Output directory |
| `--chunk-size N` | `500` | Characters per chunk |
| `--batch-size N` | `64` | Texts per API request |
| `--gpu` | off | Use GPU-accelerated Akash SDL |
| `--local` | off | Skip Akash, use local SentenceTransformer |
| `--provider-url URL` | — | Use an existing provider endpoint |
| `--deploy-name NAME` | `embedding-provider` | Akash deployment name |
| `--skip-format` | off | Skip githem, scan repo files directly |

### Environment Variables

| Variable | Purpose |
|----------|---------|
| `EMBEDDING_MODEL` | Override default model |
| `ERGORS_HOST` | Ergors daemon address (default: `localhost:50051`) |
| `GITHEM_PRESET` | Githem formatting preset (default: `standard`) |

---

## Deployment Profiles

### CPU — Lightweight (default)

Best for: `all-MiniLM-L6-v2`, `bge-small-en-v1.5`, `EmbeddingGemma-300M`

| Resource | Allocation |
|----------|-----------|
| CPU | 4 units |
| Memory | 4 GB |
| Storage | 2 GB ephemeral + 10 GB persistent cache |
| GPU | None |
| Cost | ~100,000 uakt/block |

```bash
./tools/raggamuffin.sh https://github.com/user/repo --model all-MiniLM-L6-v2
```

### GPU — High Throughput

Best for: `bge-large-en-v1.5`, `jina-embeddings-v3`, `Qwen3-Embedding-0.6B`

| Resource | Allocation |
|----------|-----------|
| CPU | 8 units |
| Memory | 16 GB |
| Storage | 5 GB ephemeral + 50 GB persistent cache |
| GPU | 1x RTX 4090 or A100 (40GB) |
| Cost | ~500,000 uakt/block |

```bash
./tools/raggamuffin.sh https://github.com/user/repo --model BAAI/bge-large-en-v1.5 --gpu
```

---

## Provider API Reference

The deployed container exposes these endpoints:

### `POST /v1/embeddings` (OpenAI-compatible)

```bash
curl -X POST http://<endpoint>:8080/v1/embeddings \
  -H "Content-Type: application/json" \
  -d '{
    "input": ["function authenticate(user, pass) { ... }"],
    "model": "all-MiniLM-L6-v2"
  }'
```

Response:

```json
{
  "object": "list",
  "data": [
    {"object": "embedding", "embedding": [0.023, -0.114, ...], "index": 0}
  ],
  "model": "all-MiniLM-L6-v2",
  "usage": {"prompt_tokens": 12, "total_tokens": 12}
}
```

### `POST /api/embeddings` (Ollama-compatible)

```bash
curl -X POST http://<endpoint>:8080/api/embeddings \
  -H "Content-Type: application/json" \
  -d '{"model": "all-MiniLM-L6-v2", "prompt": "hello world"}'
```

### `GET /health`

```json
{"status": "healthy", "model": "all-MiniLM-L6-v2", "dimension": 384, "max_batch_size": 256}
```

### `GET /info`

```json
{"model": "all-MiniLM-L6-v2", "dimension": 384, "max_batch_size": 256, "max_tokens": 512, "normalize": true}
```

---

## Model Selection Guide

| Model | Dimensions | Size | Best For |
|-------|-----------|------|----------|
| `all-MiniLM-L6-v2` | 384 | ~80 MB | Fast, lightweight, good baseline |
| `bge-small-en-v1.5` | 384 | ~130 MB | Better quality, still small |
| `bge-large-en-v1.5` | 1024 | ~1.3 GB | High quality, needs GPU |
| `Qwen3-Embedding-0.6B` | 1024 | ~1.2 GB | Multilingual, long context |
| `jina-embeddings-v3` | 1024 | ~570 MB | 8K context, multilingual |

Use `--model` to specify. Ensure the deployed provider matches (models are downloaded on first use and cached persistently).

---

## Building the Container

To build and push the embedding provider image:

```bash
cd docker/embedding-provider

# Build for CPU
docker build -t ghcr.io/ergors/embedding-provider:latest .

# Build with GPU support (use a CUDA base image)
docker build -t ghcr.io/ergors/embedding-provider:latest-gpu \
  --build-arg BASE_IMAGE=pytorch/pytorch:2.1.0-cuda12.1-cudnn8-runtime .

# Push to registry
docker push ghcr.io/ergors/embedding-provider:latest
```

### Container Configuration

| Env Variable | Default | Description |
|-------------|---------|-------------|
| `EMBEDDING_MODEL` | `all-MiniLM-L6-v2` | Model to load at startup |
| `PORT` | `8080` | Server port |
| `HOST` | `0.0.0.0` | Bind address |
| `MAX_BATCH_SIZE` | `256` | Max texts per request |

---

## Integration with ERGORS Workflows

### Agentic Retrieval

Wire ragamuffin into ergors task workflows for context-aware code generation:

```bash
# 1. Generate embeddings for the workspace
./tools/raggamuffin.sh . --local --output .ergors/rag-store

# 2. Query during task execution
CONTEXT=$(python3 tools/python/rag-retriever.py \
  --store .ergors/rag-store \
  --local \
  --query "implement authentication middleware" \
  --json \
  --top-k 3)

# 3. Feed context to LLM via ergors prompt
ergors-cli prompt --context "$CONTEXT" --message "implement auth middleware based on existing patterns"
```

### Iterative Re-embedding

Re-run after code changes to keep the store current:

```bash
# Re-embed after significant changes
./tools/raggamuffin.sh . --local --output .ergors/rag-store

# Or target specific directories
python3 tools/python/rag-embedding.py --repo ./src --local --output .ergors/rag-store
```

### Multi-Repo Aggregation

Embed multiple repos into a single store:

```bash
# Format multiple repos
githem https://github.com/org/repo-a --output /tmp/repo-a.txt
githem https://github.com/org/repo-b --output /tmp/repo-b.txt
cat /tmp/repo-a.txt /tmp/repo-b.txt > /tmp/combined.txt

# Embed combined
python3 tools/python/rag-embedding.py \
  --input /tmp/combined.txt \
  --provider-url http://akash-endpoint:8080 \
  --output ./multi-rag-store
```

---

## File Reference

```
tools/
├── raggamuffin.sh                      # Orchestration script
└── python/
    ├── rag-embedding.py                # Embedding generator (local + remote)
    ├── rag-retriever.py                # Semantic search client
    └── requirements.txt                # Python dependencies

docker/
└── embedding-provider/
    ├── server.py                       # FastAPI embedding server
    ├── Dockerfile                      # Container build
    └── requirements.txt                # Server dependencies

sdls/embeddings/
├── embedding-provider.yml              # Akash SDL (CPU)
├── embedding-provider-gpu.yml          # Akash SDL (GPU)
└── qwen.yml                            # Large-scale Qwen SDL
```

---

## Troubleshooting

### Provider Not Responding

```bash
# Check deployment status
ergors deploy get --name embedding-provider

# Check provider health directly
curl http://<endpoint>:8080/health

# View deployment logs (if available)
ergors deploy logs --name embedding-provider
```

### Embedding Dimension Mismatch

The query model must match the embedding model. If you embedded with `bge-large-en-v1.5` (1024d), you must query with the same model:

```bash
python3 tools/python/rag-retriever.py \
  --store ./rag-store \
  --provider-url http://host:8080 \
  --model BAAI/bge-large-en-v1.5 \
  --query "your question"
```

### Out of Memory (Local Mode)

For large repos, reduce batch size:

```bash
./tools/raggamuffin.sh ~/large-repo --local --batch-size 16 --chunk-size 300
```

### githem Not Found

Install githem or use `--skip-format` to scan files directly:

```bash
# Skip githem, process repo files by extension
./tools/raggamuffin.sh https://github.com/user/repo --skip-format

# Or install githem
curl -sL https://get.githem.com | bash
```

### FAISS Import Error

```bash
# Install faiss-cpu (no GPU needed for indexing/search)
pip install faiss-cpu

# On Apple Silicon
pip install faiss-cpu --no-cache-dir
```

---

## Quick Reference

| Task | Command |
|------|---------|
| Embed local repo | `./tools/raggamuffin.sh ~/repo --local` |
| Embed via Akash | `./tools/raggamuffin.sh https://github.com/user/repo` |
| Embed with GPU | `./tools/raggamuffin.sh ~/repo --gpu --model bge-large-en-v1.5` |
| Use existing provider | `./tools/raggamuffin.sh ~/repo --provider-url http://host:8080` |
| Single query | `python3 tools/python/rag-retriever.py -s ./rag-store --local -q "query"` |
| Interactive search | `python3 tools/python/rag-retriever.py -s ./rag-store --local` |
| JSON results | `python3 tools/python/rag-retriever.py -s ./rag-store --local -q "query" --json` |
| Check provider | `curl http://<endpoint>:8080/health` |
| Deploy provider | `ergors deploy create --sdl sdls/embeddings/embedding-provider.yml` |
| Check deployment | `ergors deploy get --name embedding-provider` |


# raggamuffin: generate rag via embedding llm deployed from akash

1. deploy embedding inference provider on akash
2. format repository using <https://github.com/rotkonetworks/githem>
3. generate rag embeddings using: /Users/returniflost/CW-AGENT/tools/python/rag-embedding.py
4. wire in client to allow agentic retrieval of rags

Here is a comprehensive Markdown table summarizing many of the most notable **open-source embedding models** (as of early 2026). It draws from the MTEB leaderboard trends, popular usage, and recent benchmarks. I've focused on models that are openly available (e.g., Apache 2.0, MIT, or similar permissive licenses) with weights on Hugging Face.

| Model Name (Hugging Face path)              | Approx. Size | Dimensions | Max Tokens (approx.) | Multilingual? | Notable Features / Best For                  | Approx. MTEB Avg. Score (recent refs) | License     |
|---------------------------------------------|--------------|------------|----------------------|---------------|----------------------------------------------|---------------------------------------|-------------|
| Qwen/Qwen3-Embedding-8B                     | 8B          | 4096      | 8192+               | Yes          | Currently top open-source; instruction-aware, strong multilingual | ~70.5                                 | Apache 2.0 |
| nvidia/llama-embed-nemotron-8b              | 8B          | 4096      | 32768               | Yes          | Excellent retrieval; high Top-1 accuracy in benchmarks | ~69–70                                | Restrictive (non-commercial in base) |
| Qwen/Qwen3-Embedding-4B                     | 4B          | 4096?     | High                | Yes          | Balanced size/performance in Qwen3 family    | High (close to 8B)                    | Apache 2.0 |
| dunzhang/stella_en_1.5B_v5                  | 1.5B        | 1024      | Varies              | English-only | Very strong compact English model            | Top-tier in mid-size                  | Open       |
| google/embeddinggemma-300m (or -1b variants)| 300M–1B     | Varies    | Moderate            | Yes          | On-device / mobile friendly; efficient       | Strong for size                       | Open       |
| BAAI/bge-m3                                 | ~0.5–1B     | 1024      | 8192                | Yes (100+ lang) | Multi-functional (dense + sparse + ColBERT); long context | ~63–64                                | MIT        |
| Alibaba-NLP/gte-Qwen2-7B-instruct           | 7B          | 3584      | 32768               | Yes          | High performance, instruction-tuned          | Very high                             | Apache 2.0 |
| BAAI/bge-large-en-v1.5                      | ~335M       | 1024      | 512                 | English-only | Classic battle-tested English model          | ~64–65                                | MIT        |
| BAAI/bge-base-en-v1.5                       | ~110M       | 768       | 512                 | English-only | Great speed/quality balance                  | ~63–64                                | MIT        |
| BAAI/bge-small-en-v1.5                      | ~33M        | 384       | 512                 | English-only | Very fast, low-resource                      | Good for size                         | MIT        |
| intfloat/e5-large                           | ~560M       | 1024      | 512                 | Yes (multilingual variants) | Contrastive style, strong retrieval          | ~65+ (instruct variants higher)      | MIT        |
| intfloat/e5-base-v2                         | ~110M       | 768       | 512                 | Yes          | Reliable mid-size                            | Strong                                | MIT        |
| intfloat/multilingual-e5-large              | ~560M       | 1024      | 512                 | Yes          | Solid multilingual baseline                  | Good                                  | MIT        |
| nomic-ai/nomic-embed-text-v1.5              | ~137M       | 768       | 8192                | Yes          | Long context, good general performance       | Competitive                           | Apache 2.0 |
| nomic-ai/nomic-embed-text-v2                | Varies      | Varies    | High                | Yes          | Recent iteration, improved                   | Improved over v1                      | Apache 2.0 |
| jinaai/jina-embeddings-v3                   | Varies      | 1024      | 8192+               | Yes          | Very long context support                    | Strong                                | Open       |
| mixedbread-ai/mxbai-embed-large-v1          | ~335M       | 1024      | 512+                | Yes          | High quality, popular choice                 | Very competitive                      | Apache 2.0 |
| sentence-transformers/all-mpnet-base-v2     | ~110M       | 768       | 384                 | English-heavy| Extremely popular & fast baseline            | ~63–64                                | Apache 2.0 |
| sentence-transformers/all-MiniLM-L6-v2      | ~22M        | 384       | 256                 | English-heavy| Tiny & blazing fast                          | Good for size                         | Apache 2.0 |
| Alibaba-NLP/gte-large-en-v1.5               | ~335M       | 1024      | 512                 | English-only | Strong English general-purpose               | High                                  | Open       |

### Quick Notes (as of Jan 2026)

- **Top overall open-source performer** → Qwen3-Embedding-8B family (often leading or near-leading MTEB among permissively licensed models).
- **Best compact/efficient** → EmbeddingGemma-300M, Qwen3-Embedding-0.6B, bge-small-en-v1.5, nomic-embed-text.
- **Long context / multilingual** → bge-m3, Qwen3 family, jina-embeddings-v3, llama-embed-nemotron-8b.
- Scores fluctuate; check the live [MTEB Leaderboard](https://huggingface.co/spaces/mteb/leaderboard) (filter for open models) for the absolute latest rankings.
- Most are runnable via `sentence-transformers`, Hugging Face Transformers, or Ollama.

Let me know if you'd like columns added (e.g., specific retrieval score, license link, or domain specialization like code/biomedical), or if you want this filtered to English-only, small models (<1B), etc.!
