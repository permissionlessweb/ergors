"""
RAG Embedding Generator

Processes code repositories or githem-formatted files into embeddings
using either a local SentenceTransformer or a remote Akash-deployed
embedding provider.

Usage:
    # Remote provider (Akash deployment)
    python rag-embedding.py --input formatted_repo.txt --provider-url http://akash-endpoint:8080

    # Local model
    python rag-embedding.py --repo ~/projects/my-repo --local

    # Custom output directory
    python rag-embedding.py --input repo.txt --provider-url http://host:8080 --output ./embeddings
"""

import os
import sys
import glob
import json
import argparse
import time
from pathlib import Path
from typing import Optional

import numpy as np
import requests
import faiss
import pickle


# --- Configuration ---

DEFAULT_CHUNK_SIZE = 500
DEFAULT_BATCH_SIZE = 64
DEFAULT_MODEL = "all-MiniLM-L6-v2"
CODE_EXTENSIONS = [
    ".go", ".js", ".py", ".ts", ".java", ".c", ".cpp", ".h", ".hpp",
    ".php", ".sql", ".rs", ".rb", ".swift", ".kt", ".scala", ".zig",
    ".md", ".toml", ".yaml", ".yml", ".json",
]


# --- Text Processing ---

def load_code_files(repo_path: str, extensions: list[str] = CODE_EXTENSIONS) -> list[tuple[str, str]]:
    """Load code files from a repository path. Returns list of (filepath, content) tuples."""
    files = []
    for ext in extensions:
        for filepath in glob.glob(f"{repo_path}/**/*{ext}", recursive=True):
            if os.path.isfile(filepath):
                try:
                    with open(filepath, "r", errors="ignore") as f:
                        content = f.read()
                    if content.strip():
                        files.append((filepath, content))
                except (IOError, PermissionError):
                    continue
    return files


def load_formatted_input(input_path: str) -> list[tuple[str, str]]:
    """Load a githem-formatted or plain text file. Returns list of (source, content) tuples."""
    with open(input_path, "r", errors="ignore") as f:
        content = f.read()

    # Try to detect githem format (file boundaries marked with headers)
    sections = []
    current_file = input_path
    current_lines = []

    for line in content.splitlines():
        # githem typically marks files with --- or === separators
        if line.startswith("## File: ") or line.startswith("--- "):
            if current_lines:
                sections.append((current_file, "\n".join(current_lines)))
                current_lines = []
            current_file = line.replace("## File: ", "").replace("--- ", "").strip()
        else:
            current_lines.append(line)

    if current_lines:
        sections.append((current_file, "\n".join(current_lines)))

    # If no sections detected, treat entire file as single source
    if len(sections) <= 1:
        return [(input_path, content)]

    return sections


def chunk_text(text: str, chunk_size: int = DEFAULT_CHUNK_SIZE) -> list[str]:
    """Split text into chunks by character count, preserving line boundaries."""
    lines = text.splitlines()
    chunks = []
    current = []
    count = 0

    for line in lines:
        current.append(line)
        count += len(line) + 1  # +1 for newline
        if count >= chunk_size:
            chunk = "\n".join(current)
            if chunk.strip():
                chunks.append(chunk)
            current = []
            count = 0

    if current:
        chunk = "\n".join(current)
        if chunk.strip():
            chunks.append(chunk)

    return chunks


# --- Embedding Generation ---

def embed_remote(texts: list[str], provider_url: str, batch_size: int = DEFAULT_BATCH_SIZE) -> np.ndarray:
    """Generate embeddings using a remote provider (OpenAI-compatible API)."""
    all_vectors = []
    url = f"{provider_url.rstrip('/')}/v1/embeddings"

    for i in range(0, len(texts), batch_size):
        batch = texts[i:i + batch_size]
        payload = {"input": batch, "model": DEFAULT_MODEL}

        try:
            resp = requests.post(url, json=payload, timeout=120)
            resp.raise_for_status()
            data = resp.json()

            # Sort by index to maintain order
            embeddings = sorted(data["data"], key=lambda x: x["index"])
            vectors = [e["embedding"] for e in embeddings]
            all_vectors.extend(vectors)

        except requests.exceptions.RequestException as e:
            print(f"Error calling provider at batch {i//batch_size}: {e}", file=sys.stderr)
            sys.exit(1)

        # Progress
        done = min(i + batch_size, len(texts))
        print(f"  Embedded {done}/{len(texts)} chunks")

    return np.array(all_vectors, dtype=np.float32)


def embed_local(texts: list[str], model_name: str = DEFAULT_MODEL, batch_size: int = DEFAULT_BATCH_SIZE) -> np.ndarray:
    """Generate embeddings using a local SentenceTransformer model."""
    from sentence_transformers import SentenceTransformer

    print(f"  Loading local model: {model_name}")
    model = SentenceTransformer(model_name)

    all_vectors = []
    for i in range(0, len(texts), batch_size):
        batch = texts[i:i + batch_size]
        vectors = model.encode(batch, normalize_embeddings=True)
        all_vectors.extend(vectors)

        done = min(i + batch_size, len(texts))
        print(f"  Embedded {done}/{len(texts)} chunks")

    return np.array(all_vectors, dtype=np.float32)


# --- Storage ---

def store_embeddings(
    vectors: np.ndarray,
    chunks: list[str],
    metadata: list[dict],
    output_dir: str,
):
    """Store embeddings as FAISS index + metadata pickle."""
    os.makedirs(output_dir, exist_ok=True)

    # Build FAISS index
    dimension = vectors.shape[1]
    index = faiss.IndexFlatIP(dimension)  # Inner product for normalized vectors
    index.add(vectors)

    # Save index
    index_path = os.path.join(output_dir, "index.faiss")
    faiss.write_index(index, index_path)

    # Save chunks and metadata
    store_data = {
        "chunks": chunks,
        "metadata": metadata,
        "dimension": dimension,
        "count": len(chunks),
        "created_at": time.time(),
    }
    meta_path = os.path.join(output_dir, "chunks.pkl")
    with open(meta_path, "wb") as f:
        pickle.dump(store_data, f)

    # Also save a JSON manifest for tooling
    manifest = {
        "dimension": dimension,
        "count": len(chunks),
        "index_file": "index.faiss",
        "chunks_file": "chunks.pkl",
        "created_at": time.time(),
    }
    manifest_path = os.path.join(output_dir, "manifest.json")
    with open(manifest_path, "w") as f:
        json.dump(manifest, f, indent=2)

    print(f"  Stored {len(chunks)} embeddings ({dimension}d) to {output_dir}")
    return index_path, meta_path


# --- Main ---

def main():
    parser = argparse.ArgumentParser(
        description="Generate RAG embeddings from code repositories or formatted text"
    )
    # Input sources (mutually exclusive)
    input_group = parser.add_mutually_exclusive_group(required=True)
    input_group.add_argument("--input", "-i", help="Path to githem-formatted or plain text file")
    input_group.add_argument("--repo", "-r", help="Path to code repository")

    # Provider
    provider_group = parser.add_mutually_exclusive_group(required=True)
    provider_group.add_argument("--provider-url", "-p", help="Remote embedding provider URL (Akash endpoint)")
    provider_group.add_argument("--local", action="store_true", help="Use local SentenceTransformer model")

    # Options
    parser.add_argument("--output", "-o", default="./rag-store", help="Output directory for embeddings")
    parser.add_argument("--model", "-m", default=DEFAULT_MODEL, help=f"Model name (default: {DEFAULT_MODEL})")
    parser.add_argument("--chunk-size", type=int, default=DEFAULT_CHUNK_SIZE, help="Chunk size in characters")
    parser.add_argument("--batch-size", type=int, default=DEFAULT_BATCH_SIZE, help="Batch size for API calls")

    args = parser.parse_args()

    print(f"RAG Embedding Generator")
    print(f"=======================")

    # Load source text
    if args.input:
        print(f"Loading formatted input: {args.input}")
        sources = load_formatted_input(args.input)
    else:
        print(f"Loading repository: {args.repo}")
        sources = load_code_files(args.repo)

    if not sources:
        print("No source files found.", file=sys.stderr)
        sys.exit(1)

    print(f"  Found {len(sources)} source files")

    # Chunk all sources
    all_chunks = []
    all_metadata = []
    for source_path, content in sources:
        chunks = chunk_text(content, args.chunk_size)
        for chunk in chunks:
            all_chunks.append(chunk)
            all_metadata.append({"source": source_path, "chunk_index": len(all_metadata)})

    print(f"  Generated {len(all_chunks)} chunks (chunk_size={args.chunk_size})")

    if not all_chunks:
        print("No text chunks generated.", file=sys.stderr)
        sys.exit(1)

    # Generate embeddings
    print(f"Generating embeddings...")
    if args.local:
        vectors = embed_local(all_chunks, model_name=args.model, batch_size=args.batch_size)
    else:
        print(f"  Provider: {args.provider_url}")
        vectors = embed_remote(all_chunks, args.provider_url, batch_size=args.batch_size)

    # Store
    print(f"Storing embeddings...")
    store_embeddings(vectors, all_chunks, all_metadata, args.output)

    print(f"\nDone. Embeddings stored in: {args.output}")
    print(f"  - index.faiss ({vectors.shape[1]}d, {vectors.shape[0]} vectors)")
    print(f"  - chunks.pkl (metadata + text)")
    print(f"  - manifest.json (index info)")


if __name__ == "__main__":
    main()
