# Ragamuffin: Repository RAG with ERGORS

A step-by-step tutorial for creating a RAG knowledge base from any Git repository using ERGORS and Akash-deployed embeddings.

**What you'll learn:**

1. Import a funded mnemonic for Akash deployments
2. Use githem to convert a repository into a single LLM-ready file
3. Deploy an embedding model to Akash
4. Create a RAG knowledge base from the githem output
5. Query your repository with semantic search

---

## Prerequisites

### 1. Install githem

Githem transforms Git repositories into LLM-ready text files.

```bash
curl -sL https://get.githem.com | bash
```

Verify installation:

```bash
githem --version
```

### 2. ERGORS Engine

Ensure you have the ERGORS engine built:

```bash
cd /path/to/CW-AGENT
cargo build --release -p ergors
```

### 3. Funded Akash Wallet

You need a BIP-39 mnemonic seed phrase for an Akash wallet funded with AKT tokens.

**Getting AKT:**

- Purchase AKT on exchanges (Osmosis, Kraken, etc.)
- Transfer to your Akash address

---

## Step 1: Import Your Mnemonic

Before deploying to Akash, import your funded wallet's mnemonic seed phrase:

```bash
# Import your 24-word mnemonic (will prompt for encryption password)
ergors keys import-mnemonic \
  --phrase "your twenty four word mnemonic seed phrase goes here ..." \
  --label "My Akash Wallet" \
  --key-name default \
  --chain-id akashnet-2 \
  --address-prefix akash \
  --make-default
```

**Security notes:**

- You'll be prompted to create an encryption password
- The mnemonic is encrypted at rest using Argon2id + ChaCha20Poly1305
- Never share your mnemonic or store it in plain text

Verify the import:

```bash
# List all keys
ergors keys list

# Should show:
# Key Name: default
# Label: My Akash Wallet
# Address: akash1...
# Default: true
```

---

## Step 2: Convert Repository with Githem

Use githem to create a single file containing the entire repository content in an optimized format.

### Basic Usage

```bash
# Current directory
githem . -o repo.md

# GitHub repository
githem anthropics/claude-code -o claude-code.md

# With code-only preset (excludes docs, configs)
githem . --preset code-only -o repo-code.md

# Specific branch
githem owner/repo --branch develop -o develop.md

# Include only Rust files
githem . --include "*.rs,*.toml" -o rust-only.md
```

### Example: Process CW-AGENT Repository

```bash
# Full repository with standard filtering
githem /Users/returniflost/CW-AGENT -o cw-agent.md --preset standard

# Check the output
wc -l cw-agent.md  # See line count
head -100 cw-agent.md  # Preview content
```

### Githem Presets

| Preset | Description | Best For |
|--------|-------------|----------|
| `raw` | No filtering | Complete backup |
| `standard` | Smart filtering (default) | LLM analysis |
| `code-only` | Source code only | Code review |
| `minimal` | Basic filtering | Quick scan |

**Output:** A single `.md` file ready for RAG ingestion.

---

## Step 3: Deploy Embedding Model to Akash

Deploy a Qwen3-VL-Embedding model to Akash for generating embeddings.

### SDL Configuration

The embedding deployment SDL is located at:

```
sdls/embeddings/qwen.yml
```

**Model:** `Qwen/Qwen3-VL-Embedding-8B`
**Endpoint:** OpenAI-compatible `/v1/embeddings`
**Requirements:** 2x H100 or A100 GPUs

### Deploy

```bash
# Deploy the embedding service
ergors deploy create \
  --sdl sdls/embeddings/qwen.yml \
  --key-name default \
  --auto
```

### Monitor Deployment

```bash
# Get the session ID from the deploy output, then:
ergors deploy get <session-id>

# Wait for status: "running"
# Note the endpoint URL (e.g., http://provider.akash.network:8000)
```

### Verify Endpoint

Once deployed, test the endpoint:

```bash
curl -X POST http://<provider-endpoint>:8000/v1/embeddings \
  -H "Content-Type: application/json" \
  -d '{
    "input": ["Hello, world!"],
    "model": "Qwen/Qwen3-VL-Embedding-8B"
  }'
```

Expected response:

```json
{
  "data": [
    {
      "embedding": [0.123, -0.456, ...],
      "index": 0
    }
  ],
  "model": "Qwen/Qwen3-VL-Embedding-8B"
}
```

**Save the endpoint URL** - you'll need it for the next steps.

---

## Step 4: Create RAG Knowledge Base

Now create a RAG instance and ingest the githem output.

### Option A: Using Rust Code

Create a file `ingest.rs`:

```rust
use ergors::rag;
use ergors::storage::ErgorsStorage;
use std::sync::Arc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Configuration
    let embedding_endpoint = "http://<your-akash-provider>:8000";
    let model = "Qwen/Qwen3-VL-Embedding-8B";
    let dimension = 3584;  // Qwen embedding dimension
    let githem_file = "./cw-agent.md";

    // Initialize storage with RAG prefixes
    let prefixes = vec![
        "rag_chunks".to_string(),
        "rag_source_index".to_string(),
    ];
    let storage = ErgorsStorage::new("./rag-data", prefixes).await?;

    // Create RAG with remote embedder pointing to Akash
    let rag = rag::new_remote(&storage, embedding_endpoint, model, dimension)?;

    // Read githem output
    let content = std::fs::read_to_string(githem_file)?;
    println!("Loaded {} bytes from {}", content.len(), githem_file);

    // Create document
    let doc = rag::Document {
        content,
        uri: githem_file.to_string(),
        doc_type: "markdown".to_string(),
        tags: vec!["repository".to_string(), "code".to_string()],
    };

    // Ingest (this calls the Akash embedding endpoint)
    println!("Ingesting document (this may take a while for large files)...");
    let chunk_ids = rag.ingest(doc, Some("githem".to_string())).await?;

    println!("Ingested {} chunks", chunk_ids.len());
    println!("RAG index size: {}", rag.size());
    println!("Storage: ./rag-data");

    Ok(())
}
```

### Option B: Using the Embedding API Directly

For more control, use the standalone embedding API:

```rust
use ho_std::llm::embed;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let endpoint = "http://<your-akash-provider>:8000";
    let model = "Qwen/Qwen3-VL-Embedding-8B";

    // Test embedding
    let texts = &["Hello world", "Rust programming"];
    let embeddings = embed::generate(endpoint, texts, model, None).await?;

    println!("Generated {} embeddings", embeddings.len());
    println!("Dimension: {}", embeddings[0].len());

    Ok(())
}
```

---

## Step 5: Query the Knowledge Base

Once ingestion is complete, query your repository.

### Basic Query

```rust
use ergors::rag;
use ergors::storage::ErgorsStorage;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Connect to existing RAG storage
    let prefixes = vec!["rag_chunks".into(), "rag_source_index".into()];
    let storage = ErgorsStorage::new("./rag-data", prefixes).await?;

    let rag = rag::new_remote(
        &storage,
        "http://<your-akash-provider>:8000",
        "Qwen/Qwen3-VL-Embedding-8B",
        3584,
    )?;

    // Query
    let query = "How does the RAG system handle embeddings?";
    let results = rag.query(query, 5, rag::QueryOptions::default()).await?;

    match results {
        rag::QueryResult::Standard(chunks) => {
            println!("Query: {}\n", query);
            for (i, chunk) in chunks.iter().enumerate() {
                println!("[{}] Score: {:.3}", i + 1, chunk.similarity);
                println!("    Preview: {}", chunk.metadata.preview);
                println!();
            }
        }
        _ => {}
    }

    Ok(())
}
```

### Verified Query (with Provenance)

```rust
let options = rag::QueryOptions {
    verify: true,
    include_proof: false,
    filters: Default::default(),
};

let results = rag.query("error handling", 5, options).await?;

match results {
    rag::QueryResult::Verified(chunks) => {
        for chunk in chunks {
            println!("Source: {}", chunk.provenance.source_uri);
            println!("Ingested: {:?}", chunk.provenance.ingested_at);
            println!("Content: {}", chunk.content);
            println!("Hash valid: {}", chunk.hash_valid);
        }
    }
    _ => {}
}
```

### Query with Filters

```rust
use ergors_rag::MetadataFilters;

let options = rag::QueryOptions {
    verify: false,
    include_proof: false,
    filters: MetadataFilters {
        tags: vec!["code".to_string()],
        source_uri_prefix: Some("src/".to_string()),
        ..Default::default()
    },
};

let results = rag.query("authentication", 10, options).await?;
```

---

## Complete Example Script

Here's a complete script that does everything:

```rust
//! Repository RAG with Githem and Akash
//!
//! Usage:
//!   1. Run githem to create repo file: githem . -o repo.md
//!   2. Deploy embedding service: ergors deploy create --sdl sdls/embeddings/qwen.yml
//!   3. Run this script with the endpoint URL

use ergors::rag;
use ergors::storage::ErgorsStorage;
use std::io::{self, BufRead, Write};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Parse args
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("Usage: {} <githem-file> <akash-endpoint>", args[0]);
        eprintln!("Example: {} cw-agent.md http://provider:8000", args[0]);
        std::process::exit(1);
    }

    let githem_file = &args[1];
    let endpoint = &args[2];
    let model = "Qwen/Qwen3-VL-Embedding-8B";
    let dimension = 3584;

    // Initialize storage
    println!("Initializing RAG storage...");
    let prefixes = vec!["rag_chunks".into(), "rag_source_index".into()];
    let storage = ErgorsStorage::new("./rag-data", prefixes).await?;

    // Create RAG
    let rag = rag::new_remote(&storage, endpoint, model, dimension)?;

    // Check if we need to ingest
    if rag.size() == 0 {
        println!("Loading {}...", githem_file);
        let content = std::fs::read_to_string(githem_file)?;
        println!("Loaded {} bytes", content.len());

        let doc = rag::Document {
            content,
            uri: githem_file.to_string(),
            doc_type: "markdown".to_string(),
            tags: vec!["repository".to_string()],
        };

        println!("Ingesting (calling Akash embedding service)...");
        let chunks = rag.ingest(doc, Some("githem".to_string())).await?;
        println!("Created {} chunks\n", chunks.len());
    } else {
        println!("Using existing index ({} chunks)\n", rag.size());
    }

    // Interactive query loop
    println!("=== Repository RAG ===");
    println!("Enter queries (Ctrl-D to exit):\n");

    let stdin = io::stdin();
    print!("> ");
    io::stdout().flush()?;

    for line in stdin.lock().lines() {
        let query = line?;
        if query.is_empty() {
            print!("> ");
            io::stdout().flush()?;
            continue;
        }

        let results = rag.query(&query, 5, rag::QueryOptions::default()).await?;

        match results {
            rag::QueryResult::Standard(chunks) => {
                println!("\n--- Results ---");
                for (i, chunk) in chunks.iter().enumerate() {
                    println!("\n[{}] Score: {:.3}", i + 1, chunk.similarity);
                    println!("{}", chunk.metadata.preview);
                }
            }
            _ => {}
        }

        print!("\n> ");
        io::stdout().flush()?;
    }

    println!("\nGoodbye!");
    Ok(())
}
```

---

## Deployment SDL Reference

The Qwen embedding SDL (`sdls/embeddings/qwen.yml`):

```yaml
version: "2.0"
services:
  sglang:
    image: lmsysorg/sglang:dev-cu13
    expose:
      - port: 8000
        as: 8000
        to:
          - global: true
    command:
      - bash
      - "-c"
    args:
      - >-
        python3 -m sglang.launch_server
        --model-path Qwen/Qwen3-VL-Embedding-8B
        --tensor-parallel-size 2
        --host 0.0.0.0
        --port 8000
        --is-embedding
        --trust-remote-code
        --mem-fraction-static 0.87

profiles:
  compute:
    sglang:
      resources:
        cpu:
          units: 32
        memory:
          size: 64Gi
        storage:
          - size: 50Gi
          - name: data
            size: 300Gi
            attributes:
              persistent: true
              class: beta3
          - name: shm
            size: 10Gi
            attributes:
              class: ram
              persistent: false
        gpu:
          units: 2
          attributes:
            vendor:
              nvidia:
                - model: h100
                  ram: 80Gi
                - model: a100
                  ram: 40Gi
  placement:
    dcloud:
      pricing:
        sglang:
          denom: uakt
          amount: 1000000

deployment:
  sglang:
    dcloud:
      profile: sglang
      count: 1
```

### Alternative: Lighter Model (all-MiniLM-L6-v2)

For testing or lower resource requirements, use `sdls/embeddings/embedding-provider.yml`:

- Model: `all-MiniLM-L6-v2` (384 dimensions)
- Resources: 4 CPU, 4Gi RAM (no GPU required)

---

## Troubleshooting

### Deployment not starting

```bash
# Check deployment logs
ergors deploy get <session-id>

# Common issues:
# - Insufficient AKT balance
# - No providers with required GPU
# - SDL syntax errors
```

### Embedding request failing

```bash
# Test endpoint directly
curl http://<endpoint>:8000/health

# Check if model is loaded (may take a few minutes after deployment)
curl http://<endpoint>:8000/v1/models
```

### Large file ingestion slow

- The embedding service processes text in batches
- Large files (>1MB) may take several minutes
- Consider using `--preset code-only` with githem to reduce size

### Memory issues

- Qwen3-VL-Embedding-8B requires 2x GPUs with 40GB+ VRAM
- For smaller deployments, use `all-MiniLM-L6-v2` instead

---

## Architecture Summary

```
┌─────────────────────────────────────────────────────────────────┐
│                         Workflow                                │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  [1] Git Repository                                             │
│       │                                                         │
│       ▼ githem                                                  │
│  [2] Single .md File (LLM-ready format)                        │
│       │                                                         │
│       ▼ ergors ingest                                           │
│  [3] ERGORS Engine                                              │
│       │                                                         │
│       ├──► HNSW Vector Index (local, fast search)              │
│       │                                                         │
│       ├──► Cnidarium Storage (verifiable provenance)           │
│       │                                                         │
│       └──► Akash Embedding Service (GPU inference)             │
│             │                                                   │
│             ▼                                                   │
│       [4] Qwen3-VL-Embedding-8B                                │
│            - 3584 dimensions                                    │
│            - OpenAI-compatible API                              │
│            - /v1/embeddings endpoint                            │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

---

## Next Steps

- **Add more repositories**: Ingest multiple githem outputs with different URIs
- **Filter by source**: Use `source_uri_prefix` in queries
- **Production deployment**: Set up persistent storage and monitoring
- **Multi-tenant**: Use `uploader_id` for attribution

---

## References

- [RAG Specification](/docs/specs/rag.md)
- [Githem Documentation](https://githem.com)
- [Akash Deployment Guide](https://akash.network/docs)
- [Qwen Embedding Models](https://huggingface.co/Qwen)
