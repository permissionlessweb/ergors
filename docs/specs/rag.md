# RAG Integration Specification

## Status: Implemented

The ERGORS engine provides native RAG (Retrieval-Augmented Generation) support through the `ergors-rag` package. This document describes the implemented architecture and API.

---

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                     ERGORS Engine (cw-ho)                   │
├─────────────────────────────────────────────────────────────┤
│  rag.rs                                                     │
│  ├── new_remote(storage, endpoint, model, dim) → HybridRAG  │
│  └── new_dummy(storage, dim) → HybridRAG (testing)          │
│                              │                              │
│                              ▼                              │
│  ┌───────────────────────────────────────────────────────┐  │
│  │                    ergors-rag                         │  │
│  │  ├── HybridRAG<E: Embedder>                          │  │
│  │  ├── Embedders:                                      │  │
│  │  │   ├── RemoteEmbedder (Akash/OpenAI-compatible)   │  │
│  │  │   ├── CandleEmbedder (local inference)           │  │
│  │  │   ├── OpenAIEmbedder (OpenAI API)                │  │
│  │  │   └── DummyEmbedder (testing)                    │  │
│  │  ├── HNSW vector index (hnsw_rs)                    │  │
│  │  └── Cnidarium storage (verifiable provenance)      │  │
│  └───────────────────────────────────────────────────────┘  │
├─────────────────────────────────────────────────────────────┤
│  ho-std/src/llm/embed.rs                                    │
│  └── generate(endpoint, texts, model, api_key) → Vec<Vec>   │
└─────────────────────────────────────────────────────────────┘
```

### Query Flow

```
Query Text
    ↓
[1] Embedder → query vector (remote API or local inference)
    ↓
[2] HNSW Index → top-k chunk_ids + similarity scores (20-100ms)
    ↓
[3] Cnidarium (optional) → verify hashes, get provenance (+10-50ms per chunk)
    ↓
[4] Verified Context → LLM with cryptographic guarantees
```

---

## Packages

### ergors-rag

Standalone RAG library with no circular dependencies.

**Location:** `packages/ergors-rag/`

**Dependencies:**
- `cnidarium` - Verifiable storage
- `hnsw_rs` - HNSW vector index
- `reqwest` (optional) - Remote embedder HTTP client
- `candle-*` (optional) - Local inference

**Features:**
- `openai` - Enables RemoteEmbedder and OpenAIEmbedder
- `candle` - Enables CandleEmbedder for local inference

### cw-ho (Engine Integration)

Thin wrapper in `src/rag.rs` that bridges engine storage to ergors-rag.

**Exports:**
- `ergors::rag::new_remote()` - Create RAG with remote embedder
- `ergors::rag::new_dummy()` - Create RAG with dummy embedder (testing)
- Re-exports: `Document`, `QueryOptions`, `QueryResult`, `HybridRAG`

### ho-std (Embedding API)

Simple embedding function in `src/llm/embed.rs`.

**Functions:**
- `embed::generate(endpoint, texts, model, api_key)` - Batch embedding
- `embed::generate_one(endpoint, text, model, api_key)` - Single embedding

---

## API Reference

### Engine API (cw-ho)

```rust
use ergors::rag;
use ergors::storage::ErgorsStorage;

// Initialize storage with RAG prefixes
let storage = ErgorsStorage::new("./data", vec![
    "rag_chunks".into(),
    "rag_source_index".into(),
]).await?;

// Create RAG with remote embedder (e.g., Akash deployment)
let rag = rag::new_remote(
    &storage,
    "http://provider.akash.network:8000",  // embedding endpoint
    "Qwen/Qwen3-VL-Embedding-8B",          // model name
    3584,                                   // dimension
)?;

// Ingest document
let doc = rag::Document {
    content: "Your document content...".into(),
    uri: "docs/example.md".into(),
    doc_type: "markdown".into(),
    tags: vec!["example".into()],
};
let chunk_ids = rag.ingest(doc, Some("uploader_id".into())).await?;

// Query (fast, no verification)
let results = rag.query("search query", 5, rag::QueryOptions::default()).await?;

// Query (verified, with provenance)
let options = rag::QueryOptions { verify: true, ..Default::default() };
let verified = rag.query("search query", 5, options).await?;
```

### Standalone Embedding API (ho-std)

```rust
use ho_std::llm::embed;

// Generate embeddings directly (without full RAG)
let embeddings = embed::generate(
    "http://provider.akash.network:8000",
    &["hello world", "foo bar"],
    "Qwen/Qwen3-VL-Embedding-8B",
    None,  // api_key (optional)
).await?;
```

### ergors-rag Direct Usage

```rust
use ergors_rag::{HybridRAG, Document, QueryOptions};
use cnidarium::Storage;
use std::sync::Arc;

// Initialize cnidarium storage directly
let prefixes = vec!["rag_chunks".into(), "rag_source_index".into()];
std::fs::create_dir_all("./storage")?;
let storage = Storage::load("./storage".into(), prefixes).await?;

// Create RAG with remote embedder
let rag = HybridRAG::with_remote(
    Arc::new(storage),
    "http://provider:8000",
    "model-name",
    384,
)?;

// Or with local inference (requires "candle" feature)
// let rag = HybridRAG::with_candle(Arc::new(storage)).await?;

// Or with OpenAI API (requires "openai" feature + OPENAI_API_KEY env)
// let rag = HybridRAG::with_openai(Arc::new(storage))?;
```

---

## Data Structures

### Document

```rust
pub struct Document {
    pub content: String,     // Full document text
    pub uri: String,         // Source identifier (path, URL)
    pub doc_type: String,    // Type: "rust", "markdown", "text", etc.
    pub tags: Vec<String>,   // User-defined tags for filtering
}
```

### QueryOptions

```rust
pub struct QueryOptions {
    pub verify: bool,              // Enable hash verification (default: false)
    pub include_proof: bool,       // Generate JMT proofs (default: false)
    pub filters: MetadataFilters,  // Filter results by metadata
}

pub struct MetadataFilters {
    pub source_type: Option<String>,     // Filter by doc_type
    pub tags: Vec<String>,               // Must have ALL tags
    pub min_ingested_at: Option<i64>,    // Timestamp range
    pub max_ingested_at: Option<i64>,
    pub source_uri_prefix: Option<String>,  // URI prefix match
}
```

### QueryResult

```rust
pub enum QueryResult {
    Standard(Vec<SearchResult>),   // Fast, unverified
    Verified(Vec<VerifiedChunk>),  // With provenance
}

pub struct SearchResult {
    pub chunk_id: Uuid,
    pub similarity: f32,
    pub metadata: ChunkMetadata,
}

pub struct VerifiedChunk {
    pub chunk_id: Uuid,
    pub similarity: f32,
    pub content: String,
    pub provenance: ChunkProvenance,
    pub hash_valid: bool,
}

pub struct ChunkProvenance {
    pub source_uri: String,
    pub uploader_id: Option<String>,
    pub ingested_at: Timestamp,
    pub version: u64,
}
```

### VerifiableChunk (Storage)

```rust
pub struct VerifiableChunk {
    pub chunk_id: Uuid,
    pub content: String,
    pub content_hash: [u8; 32],     // BLAKE3(content)
    pub embedding_hash: [u8; 32],   // BLAKE3(embedding bytes)
    pub version: u64,
    pub ingested_at: Timestamp,
    pub source_uri: String,
    pub uploader_id: Option<String>,
    pub access_policy: Option<Vec<u8>>,
    pub commit_ref: Option<String>,
    pub previous_version: Option<Uuid>,
}
```

---

## Embedders

### RemoteEmbedder

Calls OpenAI-compatible `/v1/embeddings` endpoint. Works with Akash deployments, vLLM, SGLang, etc.

```rust
// Via engine
let rag = rag::new_remote(&storage, "http://endpoint:8000", "model", 384)?;

// Direct
use ergors_rag::embedder::remote::RemoteEmbedder;
let embedder = RemoteEmbedder::new("http://endpoint:8000", "model", 384)?;
```

### CandleEmbedder

Local inference using Candle. Supports any BERT-compatible HuggingFace model.

```rust
// Via HybridRAG (downloads model on first use)
let rag = HybridRAG::with_candle(storage).await?;  // BGE-small default
let rag = HybridRAG::with_candle_model(storage, "BAAI/bge-base-en-v1.5").await?;

// Supported models:
// - BAAI/bge-small-en-v1.5 (384 dims, ~134MB)
// - BAAI/bge-base-en-v1.5 (768 dims, ~438MB)
// - intfloat/multilingual-e5-small (384 dims, 100+ languages)
```

### OpenAIEmbedder

OpenAI API. Requires `OPENAI_API_KEY` environment variable.

```rust
std::env::set_var("OPENAI_API_KEY", "sk-...");
let rag = HybridRAG::with_openai(storage)?;  // text-embedding-3-small default
```

### DummyEmbedder

Deterministic hash-based embeddings for testing. **Do not use in production.**

```rust
let rag = HybridRAG::with_dummy(storage, 128)?;
// or via engine:
let rag = rag::new_dummy(&storage, 128)?;
```

---

## Storage Schema

Cnidarium prefixes:
- `rag_chunks/{chunk_id}` → `VerifiableChunk` (bincode)
- `rag_source_index/{source_uri}` → `Vec<Uuid>` (bincode)

```
storage/
├── <rocksdb-files>
└── substores/
    ├── rag_chunks/
    │   └── <uuid> → VerifiableChunk
    └── rag_source_index/
        └── <source_uri> → Vec<Uuid>
```

---

## Performance

### Query Latency

| Mode | 10k chunks | 100k chunks | 1M chunks |
|------|-----------|-------------|-----------|
| Standard | 20-50ms | 50-100ms | 100-200ms |
| Verified | 50-100ms | 100-150ms | 150-300ms |

### Storage Overhead

- HNSW: ~4KB per chunk (768-dim)
- Cnidarium: ~500 bytes per chunk
- Total: ~4.5KB per chunk

### Batch Ingestion

`put_chunks_batch()` commits all chunks in single atomic operation (10-100x faster than individual commits).

---

## Feature Flags

```toml
[dependencies]
ergors-rag = { path = "...", features = ["openai"] }  # Remote + OpenAI
ergors-rag = { path = "...", features = ["candle"] }  # Local inference
ergors-rag = { path = "...", features = ["openai", "candle"] }  # Both
```

---

## When to Use Verification

### Enable verification (`verify: true`)
- Legal/compliance RAG with audit requirements
- Multi-tenant SaaS with attribution
- Regulated industries (HIPAA, FDA)
- Content integrity guarantees needed

### Skip verification (default)
- Development and prototyping
- Internal tools without threat model
- Real-time systems with <50ms budget
- Personal knowledge bases

---

## Files

```
packages/ergors-rag/
├── Cargo.toml
└── src/
    ├── lib.rs           # HybridRAG, constructors
    ├── types.rs         # Document, QueryOptions, VerifiableChunk
    ├── embedder.rs      # Embedder trait + implementations
    ├── vector_index.rs  # HNSW wrapper
    ├── storage.rs       # Cnidarium integration
    ├── ingest.rs        # Chunking + embedding pipeline
    └── query.rs         # Retrieval flows

packages/cw-ho/src/
└── rag.rs              # Engine integration wrapper

packages/ho-std/src/llm/
└── embed.rs            # Standalone embedding API
```

---

## References

- [Ragamuffin Tutorial](/examples/ragamuffin.md) - Step-by-step usage guide
- [Cnidarium Docs](https://docs.rs/cnidarium/)
- [HNSW Algorithm](https://arxiv.org/abs/1603.09320)
- [MTEB Leaderboard](https://huggingface.co/spaces/mteb/leaderboard) - Embedding benchmarks
