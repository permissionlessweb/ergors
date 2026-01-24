"""
Embedding Inference Provider Server

A FastAPI service that wraps SentenceTransformer models and exposes
OpenAI-compatible /v1/embeddings endpoint for deployment on Akash Network.

Supports:
- OpenAI /v1/embeddings API format
- Ollama /api/embeddings API format
- Batch embedding requests
- Model info/health endpoints
"""

import os
import time
import hashlib
from typing import Union

import numpy as np
import uvicorn
from fastapi import FastAPI, HTTPException
from pydantic import BaseModel, Field
from sentence_transformers import SentenceTransformer

# Configuration from environment
MODEL_NAME = os.environ.get("EMBEDDING_MODEL", "all-MiniLM-L6-v2")
PORT = int(os.environ.get("PORT", "8080"))
HOST = os.environ.get("HOST", "0.0.0.0")
MAX_BATCH_SIZE = int(os.environ.get("MAX_BATCH_SIZE", "256"))

app = FastAPI(title="Embedding Provider", version="1.0.0")

# Global model instance
model: SentenceTransformer = None
model_dimension: int = 0


class OpenAIEmbeddingRequest(BaseModel):
    input: Union[str, list[str]]
    model: str = MODEL_NAME
    encoding_format: str = "float"


class OllamaEmbeddingRequest(BaseModel):
    model: str = MODEL_NAME
    prompt: Union[str, list[str]] = ""


class EmbeddingObject(BaseModel):
    object: str = "embedding"
    embedding: list[float]
    index: int


class UsageInfo(BaseModel):
    prompt_tokens: int
    total_tokens: int


class OpenAIEmbeddingResponse(BaseModel):
    object: str = "list"
    data: list[EmbeddingObject]
    model: str
    usage: UsageInfo


class OllamaEmbeddingResponse(BaseModel):
    model: str
    embeddings: list[list[float]]


class ModelInfo(BaseModel):
    id: str
    object: str = "model"
    owned_by: str = "ergors"
    dimension: int
    max_tokens: int = 512


@app.on_event("startup")
async def load_model():
    global model, model_dimension
    print(f"Loading embedding model: {MODEL_NAME}")
    model = SentenceTransformer(MODEL_NAME)
    # Get dimension from a test encoding
    test_vec = model.encode(["test"])
    model_dimension = test_vec.shape[1]
    print(f"Model loaded. Dimension: {model_dimension}")


@app.get("/health")
async def health():
    return {
        "status": "healthy",
        "model": MODEL_NAME,
        "dimension": model_dimension,
        "max_batch_size": MAX_BATCH_SIZE,
    }


@app.get("/v1/models")
async def list_models():
    return {
        "object": "list",
        "data": [
            {
                "id": MODEL_NAME,
                "object": "model",
                "owned_by": "ergors",
                "dimension": model_dimension,
            }
        ],
    }


@app.post("/v1/embeddings", response_model=OpenAIEmbeddingResponse)
async def openai_embeddings(request: OpenAIEmbeddingRequest):
    """OpenAI-compatible embeddings endpoint."""
    texts = request.input if isinstance(request.input, list) else [request.input]

    if len(texts) > MAX_BATCH_SIZE:
        raise HTTPException(
            status_code=400,
            detail=f"Batch size {len(texts)} exceeds max {MAX_BATCH_SIZE}",
        )

    if not texts or all(t.strip() == "" for t in texts):
        raise HTTPException(status_code=400, detail="Input cannot be empty")

    vectors = model.encode(texts, normalize_embeddings=True)

    data = [
        EmbeddingObject(
            embedding=vec.tolist(),
            index=i,
        )
        for i, vec in enumerate(vectors)
    ]

    # Approximate token count
    total_tokens = sum(len(t.split()) for t in texts)

    return OpenAIEmbeddingResponse(
        data=data,
        model=MODEL_NAME,
        usage=UsageInfo(prompt_tokens=total_tokens, total_tokens=total_tokens),
    )


@app.post("/api/embeddings", response_model=OllamaEmbeddingResponse)
async def ollama_embeddings(request: OllamaEmbeddingRequest):
    """Ollama-compatible embeddings endpoint."""
    prompts = request.prompt if isinstance(request.prompt, list) else [request.prompt]

    if len(prompts) > MAX_BATCH_SIZE:
        raise HTTPException(
            status_code=400,
            detail=f"Batch size {len(prompts)} exceeds max {MAX_BATCH_SIZE}",
        )

    vectors = model.encode(prompts, normalize_embeddings=True)

    return OllamaEmbeddingResponse(
        model=request.model,
        embeddings=[vec.tolist() for vec in vectors],
    )


@app.get("/info")
async def model_info():
    """Model information endpoint."""
    return {
        "model": MODEL_NAME,
        "dimension": model_dimension,
        "max_batch_size": MAX_BATCH_SIZE,
        "max_tokens": 512,
        "normalize": True,
    }


if __name__ == "__main__":
    uvicorn.run(app, host=HOST, port=PORT)
