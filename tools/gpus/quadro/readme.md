```py
# Create a clean venv (optional but recommended)
python3 -m venv llm_homelab
source llm_homelab/bin/activate

# Core libs – pinned to versions that still compile for CC3.5
pip install --upgrade pip
pip install "torch==2.0.1+cu118" --extra-index-url https://download.pytorch.org/whl/cu118
pip install transformers sentence-transformers faiss-gpu==1.7.4 nltk tqdm
```

```py
import os
import json
import torch
import numpy as np
import faiss
from tqdm.auto import tqdm
from transformers import AutoTokenizer, AutoModelForTokenClassification, pipeline
from sentence_transformers import SentenceTransformer

# -------------------------------------------------
# Detect GPU (K620) – fall back to CPU if not there
DEVICE = torch.device("cuda:0" if torch.cuda.is_available() else "cpu")
print(f"Using device: {DEVICE}")

# -------------------------------------------------
# 1️⃣  Embedding model (tiny but high‑quality)
EMBED_MODEL_NAME = "sentence-transformers/all-MiniLM-L6-v2"
embedder = SentenceTransformer(EMBED_MODEL_NAME, device=DEVICE)

# -------------------------------------------------
# 2️⃣  NER pipeline (distilled model)
NER_MODEL_NAME = "dslim/bert-base-NER"   # ~110 MB, works on K620
tokenizer = AutoTokenizer.from_pretrained(NER_MODEL_NAME)
ner_model = AutoModelForTokenClassification.from_pretrained(NER_MODEL_NAME).to(DEVICE)
ner_pipe = pipeline("ner", model=ner_model, tokenizer=tokenizer, device=0 if DEVICE.type == "cuda" else -1)

# -------------------------------------------------
# 3️⃣  Simple profanity / PII filter (keyword list)
BAD_WORDS = {"badword1", "badword2", "ssn", "credit", "passport"}  # extend as needed
def contains_prohibited(text: str) -> bool:
    lowered = text.lower()
    return any(bad in lowered for bad in BAD_WORDS)

```

## PAISS GPU index 
```py 
DIM = embedder.get_sentence_embedding_dimension()   # 384 for MiniLM‑L6
INDEX = faiss.IndexFlatIP(DIM)                     # inner‑product (cosine after norm)
# Wrap in GPU resources – this will copy the index to the K620
res = faiss.StandardGpuResources()
gpu_index = faiss.index_cpu_to_gpu(res, 0, INDEX)

def add_documents(docs: list[str]) -> None:
    """Encode a list of raw strings and push them into the GPU index."""
    # Batch encode to keep VRAM footprint low
    batch_size = 64
    for i in tqdm(range(0, len(docs), batch_size), desc="Embedding docs"):
        batch = docs[i:i+batch_size]
        embeddings = embedder.encode(batch, normalize_embeddings=True, batch_size=len(batch))
        gpu_index.add(np.array(embeddings, dtype=np.float32))

def query(text: str, k: int = 5) -> list[tuple[int, float]]:
    """Return the top‑k internal IDs + scores for a query string."""
    emb = embedder.encode([text], normalize_embeddings=True)
    D, I = gpu_index.search(np.array(emb, dtype=np.float32), k)
    return [(int(idx), float(score)) for idx, score in zip(I[0], D[0])]
```
<!-- 
<think>
We need to respond with Python code that utilizes a Quadro K620 GPU for a homelab LLM workflow: embedding retrieval, person/entity identity, filtering. Provide code that uses PyTorch with support, maybe leveraging HuggingFace Transformers for embeddings (sentence transformers), using FAISS for vector store with GPU, implementing simple pipelines: embed text, store/retrieve, identity extraction via NER (spaCy or HuggingFace), filtering (e.g., content moderation). Need to ensure code works on low-end GPU. Mention that K620 is compute capability 3.5, may need older CUDA (maybe up to 10.2). Suggest using torch==1.12, or appropriate version. Provide install instructions, requirements. Provide sample usage.

Write concise, but thorough. Provide sections: setup, embedding model, FAISS index, NER model, filtering function, main workflow. Also note GPU memory constraints. Use huggingface "sentence-transformers/all-MiniLM-L6-v2" small model, about 100MB. Use "distilbert-base-cased" for NER. Use "torch.cuda.is_available()" and fallback.

We'll comply with policy, no disallowed content. Provide code.
</think>

Below is a **self‑contained, ready‑to‑run** Python notebook‑style script that wires a modest Quadro K620 (CUDA 3.5, ~2 GB VRAM) into a tiny “homelab LLM” pipeline.  

It covers three typical tasks you asked for:

| Task | Why it fits a K620 | What we use |
|------|-------------------|-------------|
| **Embedding → vector store** | Small sentence‑transformer (≈ 110 MB) fits in VRAM; FAISS can run on the GPU for fast similarity search. | `sentence‑transformers/all‑MiniLM‑L6‑v2` + FAISS‑GPU |
| **Person/Entity Identity (NER)** | A distilled BERT‑style NER model (~30 MB) runs comfortably on the GPU. | `huggingface/transformers` with `distilbert-base‑cased‑finetuned‑conll03‑english` |
| **Content filtering** | Simple rule‑based + cosine‑similarity filter that can also be accelerated on‑GPU. | Custom Python code + PyTorch tensors |

---

## 1️⃣  Prerequisites (one‑time install)

> **NOTE** – The K620 only supports up to CUDA 11 (Compute 3.5).  
> Use a PyTorch build that matches the driver on your machine (e.g. `torch==2.0.1+cu118` works for most recent drivers, but you can drop to `torch==1.12.1+cu113` if needed).

```bash
# Create a clean venv (optional but recommended)
python3 -m venv llm_homelab
source llm_homelab/bin/activate

# Core libs – pinned to versions that still compile for CC3.5
pip install --upgrade pip
pip install "torch==2.0.1+cu118" --extra-index-url https://download.pytorch.org/whl/cu118
pip install transformers sentence-transformers faiss-gpu==1.7.4 nltk tqdm
```

*If you hit a “No matching distribution” error, downgrade to `torch==1.12.1+cu113` and install the corresponding `faiss-gpu==1.7.1`.*

---

## 2️⃣  Helper utilities

```python
import os
import json
import torch
import numpy as np
import faiss
from tqdm.auto import tqdm
from transformers import AutoTokenizer, AutoModelForTokenClassification, pipeline
from sentence_transformers import SentenceTransformer

# -------------------------------------------------
# Detect GPU (K620) – fall back to CPU if not there
DEVICE = torch.device("cuda:0" if torch.cuda.is_available() else "cpu")
print(f"Using device: {DEVICE}")

# -------------------------------------------------
# 1️⃣  Embedding model (tiny but high‑quality)
EMBED_MODEL_NAME = "sentence-transformers/all-MiniLM-L6-v2"
embedder = SentenceTransformer(EMBED_MODEL_NAME, device=DEVICE)

# -------------------------------------------------
# 2️⃣  NER pipeline (distilled model)
NER_MODEL_NAME = "dslim/bert-base-NER"   # ~110 MB, works on K620
tokenizer = AutoTokenizer.from_pretrained(NER_MODEL_NAME)
ner_model = AutoModelForTokenClassification.from_pretrained(NER_MODEL_NAME).to(DEVICE)
ner_pipe = pipeline("ner", model=ner_model, tokenizer=tokenizer, device=0 if DEVICE.type == "cuda" else -1)

# -------------------------------------------------
# 3️⃣  Simple profanity / PII filter (keyword list)
BAD_WORDS = {"badword1", "badword2", "ssn", "credit", "passport"}  # extend as needed
def contains_prohibited(text: str) -> bool:
    lowered = text.lower()
    return any(bad in lowered for bad in BAD_WORDS)
```

---

## 3️⃣  FAISS‑GPU index (≈ 2 GB VRAM usage)

```python
DIM = embedder.get_sentence_embedding_dimension()   # 384 for MiniLM‑L6
INDEX = faiss.IndexFlatIP(DIM)                     # inner‑product (cosine after norm)
# Wrap in GPU resources – this will copy the index to the K620
res = faiss.StandardGpuResources()
gpu_index = faiss.index_cpu_to_gpu(res, 0, INDEX)

def add_documents(docs: list[str]) -> None:
    """Encode a list of raw strings and push them into the GPU index."""
    # Batch encode to keep VRAM footprint low
    batch_size = 64
    for i in tqdm(range(0, len(docs), batch_size), desc="Embedding docs"):
        batch = docs[i:i+batch_size]
        embeddings = embedder.encode(batch, normalize_embeddings=True, batch_size=len(batch))
        gpu_index.add(np.array(embeddings, dtype=np.float32))

def query(text: str, k: int = 5) -> list[tuple[int, float]]:
    """Return the top‑k internal IDs + scores for a query string."""
    emb = embedder.encode([text], normalize_embeddings=True)
    D, I = gpu_index.search(np.array(emb, dtype=np.float32), k)
    return [(int(idx), float(score)) for idx, score in zip(I[0], D[0])]
```

> **Tip:** Keep a *side‑car* mapping `doc_id → original text` in a simple JSON or SQLite DB so you can retrieve the real document after the FAISS lookup.

```python
# Example mapping file
DOC_MAP_PATH = "doc_id_map.json"
if os.path.exists(DOC_MAP_PATH):
    with open(DOC_MAP_PATH) as f:
        DOC_MAP = json.load(f)
else:
    DOC_MAP = {}

def register_docs(docs: list[str]) -> None:
    """Add docs to the index and maintain the ID → text map."""
    start_id = len(DOC_MAP)
    for i, doc in enumerate(docs):
        DOC_MAP[str(start_id + i)] = doc
    add_documents(docs)
    # Persist
    with open(DOC_MAP_PATH, "w") as f:
        json.dump(DOC_MAP, f, indent=2)
```

---

## 4️⃣  Entity extraction (person / organization / location)

```python
def extract_entities(text: str) -> dict[str, list[str]]:
    """
    Returns a dict like {"PERSON": [...], "ORG": [...], "LOC": [...]}
    """
    ents = ner_pipe(text)
    out = {}
    for ent in ents:
        label = ent["entity_group"]
        word = ent["word"]
        out.setdefault(label, []).append(word)
    # Deduplicate
    return {k: list(set(v)) for k, v in out.items()}
```

---

## 5️⃣  End‑to‑end workflow

```python
def ingest_corpus(corpus: list[str]) -> None:
    """
    1️⃣  Filter out prohibited content
    2️⃣  Register the clean docs in FAISS + DB
    """
    clean = [doc for doc in corpus if not contains_prohibited(doc)]
    if not clean:
        print("⚠️  Nothing passed the filter!")
        return
    register_docs(clean)
    print(f"✅  Indexed {len(clean)} documents.")

def answer_query(query: str, top_k: int = 5) -> None:
    """
    1️⃣  Run a quick filter on the user query.
    2️⃣  Retrieve the most similar indexed passages.
    3️⃣  Show extracted entities for each hit.
    """
    if contains_prohibited(query):
        print("🚫  Query contains prohibited terms – abort.")
        return

    hits = query(query, k=top_k)
    for doc_id, score in hits:
        raw = DOC_MAP.get(str(doc_id), "<missing>")
        ents = extract_entities(raw)
        print("-" * 60)
        print(f"Doc ID: {doc_id}  |  Score: {score:.4f}")
        print(f"Snippet: {raw[:200]}{'…' if len(raw) > 200 else ''}")
        if ents:
            print("Entities found:", json.dumps(ents, ensure_ascii=False))
        else:
            print("No entities detected.")
```

### Quick demo

```python
# ------------------------------------------------------------------
# 1️⃣  Build a tiny demo corpus (in practice you would load many KB docs)
demo_corpus = [
    "Apple announced the new iPhone 15 at its Cupertino headquarters.",
    "Elon Musk's SpaceX successfully launched the Starlink batch 12.",
    "The 2024 Summer Olympics will be held in Paris, France.",
    "OpenAI released GPT‑4 Turbo, which is cheaper and faster.",
    "Data breach at Acme Corp exposed the SSNs of 10k employees."   # <- prohibited, will be filtered out
]

ingest_corpus(demo_corpus)   # → 4 docs actually indexed

# ------------------------------------------------------------------
# 2️⃣  Run a user query
answer_query("Who is leading the next SpaceX launch?")
```

Sample output (your actual scores will differ):

```
Using device: cuda:0
✅  Indexed 4 documents.
------------------------------------------------------------
Doc ID: 1  |  Score: 0.8765
Snippet: Elon Musk's SpaceX successfully launched the Starlink batch 12…
Entities found: {"PERSON": ["Elon Musk"], "ORG": ["SpaceX"], "LOC": []}
------------------------------------------------------------
Doc ID: 0  |  Score: 0.3124
Snippet: Apple announced the new iPhone 15 at its Cupertino headquarters....
Entities found: {"ORG": ["Apple"], "LOC": ["Cupertino"]}
...
```

---

## 6️⃣  Tips for a real homelab deployment

| Issue | Fix / Work‑around |
|-------|-------------------|
| **GPU memory exhaustion** (FAISS + model + batch) | Use `faiss.IndexIVFFlat` + coarse clustering to keep the active set < 200 k vectors, or switch the embedding model to `all-MiniLM-L12‑v2` (still ~120 MB). |
| **CUDA 3.5 support loss** (newer PyTorch) | Pin `torch==1.12.1+cu113` and install `faiss-gpu==1.7.1`. |
| **Persistence** | Store FAISS index to disk with `faiss.write_index(faiss.index_gpu_to_cpu(gpu_index), "my_index.faiss")` and reload with `faiss.read_index`. |
| **Batch ingestion** | Wrap `add_documents` in a `multiprocessing.Pool` with `torch.set_num_threads(1)` per worker – K620 has 2 SMs, so a single process is usually fastest. |
| **Security** | Keep the `BAD_WORDS` list in a separate config file and reload on‑the‑fly to avoid having to restart the service. |

---

### 🎉 What you now have

* **GPU‑accelerated embedding & similarity search** (tiny index, fast queries).  
* **Entity‑level insight** on every retrieved chunk (person, org, location).  
* **Lightweight content‑filtering** that runs before anything hits the GPU, protecting both the model and your data.

All of this fits comfortably on a **Quadro K620** (≈ 2 GB VRAM) and can be wrapped in a simple Flask/FastAPI micro‑service for HTTP‑based homelab usage.  

Happy hacking! 🚀 -->