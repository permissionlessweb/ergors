"""
RAG Retriever Client

Queries stored embeddings for semantic similarity search.
Supports both local model and remote Akash-deployed provider for query embedding.

Usage:
    # Interactive query mode
    python rag-retriever.py --store ./rag-store --provider-url http://akash-endpoint:8080

    # Single query
    python rag-retriever.py --store ./rag-store --provider-url http://host:8080 --query "how does auth work"

    # JSON output for piping
    python rag-retriever.py --store ./rag-store --local --query "error handling" --json

    # Adjust results count
    python rag-retriever.py --store ./rag-store --local --query "database" --top-k 10
"""

import os
import sys
import json
import argparse
from typing import Optional

import numpy as np
import requests
import faiss
import pickle


DEFAULT_MODEL = "all-MiniLM-L6-v2"
DEFAULT_TOP_K = 5


def load_store(store_dir: str) -> tuple:
    """Load FAISS index and chunk metadata from store directory."""
    index_path = os.path.join(store_dir, "index.faiss")
    chunks_path = os.path.join(store_dir, "chunks.pkl")

    if not os.path.exists(index_path):
        print(f"Error: index.faiss not found in {store_dir}", file=sys.stderr)
        sys.exit(1)
    if not os.path.exists(chunks_path):
        print(f"Error: chunks.pkl not found in {store_dir}", file=sys.stderr)
        sys.exit(1)

    index = faiss.read_index(index_path)

    with open(chunks_path, "rb") as f:
        store_data = pickle.load(f)

    chunks = store_data["chunks"]
    metadata = store_data["metadata"]
    dimension = store_data["dimension"]

    return index, chunks, metadata, dimension


def embed_query_remote(query: str, provider_url: str) -> np.ndarray:
    """Embed a single query using the remote provider."""
    url = f"{provider_url.rstrip('/')}/v1/embeddings"
    payload = {"input": [query], "model": DEFAULT_MODEL}

    try:
        resp = requests.post(url, json=payload, timeout=30)
        resp.raise_for_status()
        data = resp.json()
        vector = data["data"][0]["embedding"]
        return np.array([vector], dtype=np.float32)
    except requests.exceptions.RequestException as e:
        print(f"Error embedding query: {e}", file=sys.stderr)
        sys.exit(1)


def embed_query_local(query: str, model_name: str = DEFAULT_MODEL) -> np.ndarray:
    """Embed a single query using a local SentenceTransformer."""
    from sentence_transformers import SentenceTransformer

    model = SentenceTransformer(model_name)
    vector = model.encode([query], normalize_embeddings=True)
    return np.array(vector, dtype=np.float32)


def search(
    index: faiss.Index,
    query_vector: np.ndarray,
    chunks: list[str],
    metadata: list[dict],
    top_k: int = DEFAULT_TOP_K,
) -> list[dict]:
    """Search the FAISS index for similar chunks."""
    scores, indices = index.search(query_vector, top_k)

    results = []
    for i, (score, idx) in enumerate(zip(scores[0], indices[0])):
        if idx == -1:  # FAISS returns -1 for empty slots
            continue
        results.append({
            "rank": i + 1,
            "score": float(score),
            "chunk": chunks[idx],
            "source": metadata[idx].get("source", "unknown"),
            "chunk_index": int(idx),
        })

    return results


def format_result(result: dict, verbose: bool = False) -> str:
    """Format a single search result for display."""
    lines = [
        f"[{result['rank']}] Score: {result['score']:.4f} | Source: {result['source']}",
    ]
    if verbose:
        lines.append(f"    Chunk #{result['chunk_index']}")
    # Show first 3 lines of chunk
    chunk_preview = result["chunk"].strip().splitlines()[:3]
    for line in chunk_preview:
        lines.append(f"    {line[:120]}")
    if len(result["chunk"].strip().splitlines()) > 3:
        lines.append(f"    ...")
    return "\n".join(lines)


def interactive_mode(
    index: faiss.Index,
    chunks: list[str],
    metadata: list[dict],
    provider_url: Optional[str],
    local: bool,
    model_name: str,
    top_k: int,
):
    """Run interactive query loop."""
    print(f"\nRAG Retriever (interactive mode)")
    print(f"Store: {len(chunks)} chunks, {index.d}d vectors")
    print(f"Type a query and press Enter. Type 'quit' to exit.\n")

    # Pre-load local model if needed
    local_model = None
    if local:
        from sentence_transformers import SentenceTransformer
        print(f"Loading model: {model_name}")
        local_model = SentenceTransformer(model_name)

    while True:
        try:
            query = input("query> ").strip()
        except (EOFError, KeyboardInterrupt):
            print("\nExiting.")
            break

        if not query or query.lower() in ("quit", "exit", "q"):
            break

        # Embed query
        if local and local_model:
            vector = local_model.encode([query], normalize_embeddings=True)
            query_vector = np.array(vector, dtype=np.float32)
        else:
            query_vector = embed_query_remote(query, provider_url)

        # Search
        results = search(index, query_vector, chunks, metadata, top_k)

        print(f"\n--- Top {len(results)} results ---")
        for r in results:
            print(format_result(r))
            print()


def main():
    parser = argparse.ArgumentParser(
        description="Query RAG embeddings for semantic similarity search"
    )
    parser.add_argument("--store", "-s", required=True, help="Path to embedding store directory")

    # Provider
    provider_group = parser.add_mutually_exclusive_group(required=True)
    provider_group.add_argument("--provider-url", "-p", help="Remote embedding provider URL")
    provider_group.add_argument("--local", action="store_true", help="Use local SentenceTransformer")

    # Query
    parser.add_argument("--query", "-q", help="Single query (omit for interactive mode)")
    parser.add_argument("--top-k", "-k", type=int, default=DEFAULT_TOP_K, help=f"Number of results (default: {DEFAULT_TOP_K})")
    parser.add_argument("--model", "-m", default=DEFAULT_MODEL, help=f"Model name (default: {DEFAULT_MODEL})")
    parser.add_argument("--json", action="store_true", help="Output results as JSON")

    args = parser.parse_args()

    # Load store
    index, chunks, metadata, dimension = load_store(args.store)

    if args.query:
        # Single query mode
        if args.local:
            query_vector = embed_query_local(args.query, args.model)
        else:
            query_vector = embed_query_remote(args.query, args.provider_url)

        results = search(index, query_vector, chunks, metadata, args.top_k)

        if args.json:
            print(json.dumps(results, indent=2))
        else:
            for r in results:
                print(format_result(r))
                print()
    else:
        # Interactive mode
        interactive_mode(
            index, chunks, metadata,
            args.provider_url, args.local, args.model, args.top_k,
        )


if __name__ == "__main__":
    main()
