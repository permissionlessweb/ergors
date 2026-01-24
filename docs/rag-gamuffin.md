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
