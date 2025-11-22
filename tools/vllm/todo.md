# TODO: VLLM tooling integration

### vLLM Tooling Beyond Embeddings for Agent Frameworks

vLLM's OpenAI-compatible API provides a robust set of endpoints that go far beyond embeddings, making it ideal for powering inference in distributed networks like yours on Akash. Since you're building an integrated agent framework with local (e.g., on-device or self-hosted) and remote (e.g., Akash-deployed) vLLM instances, the canonical access point allows seamless routing—use environment variables or config files in your agents to switch between endpoints (e.g., `base_url="http://local-vllm:8000/v1"` vs. `https://remote-lease.akash.network/v1"`). This enables hybrid setups: local for low-latency tasks, remote for heavy compute.

Focus on **non-embedding endpoints** that align with agentic workflows (e.g., reasoning, retrieval, multimodal processing). I'll prioritize those most useful for agents: chat completions for tool-calling loops, classification/scoring for decision-making, and reranking for RAG. All support streaming and batching for efficiency in multi-agent systems. Integrate via the OpenAI Python client (point `base_url` to your vLLM URI) or frameworks like LangChain/LlamaIndex for agent orchestration.

#### Key Recommended Endpoints & Agent Tooling
Here's a curated list of endpoints (excluding `/v1/embeddings`), with use cases tailored to your framework. Deploy separate vLLM instances per endpoint/model via YAML updates (e.g., swap the `vllm serve` model arg, like `--served-model-name gpt-like-model` for chat).

| Endpoint | Description & Agent Use Case | Example Query (cURL) | Framework Integration Tips |
|----------|------------------------------|----------------------|----------------------------|
| **/v1/chat/completions**<br>(Chat API) | Handles multi-turn conversations with roles (user/system/assistant). Supports tool calling, function calls, and structured JSON outputs if your model (e.g., Llama-3.1) enables it. Ideal for **core agent reasoning loops**: plan-act-observe cycles, where agents invoke tools (e.g., search, calc) and parse responses. Multimodal: Add images/audio via `content` for vision-language agents. | ```bash:disable-run
| **/v1/completions**<br>(Completions API) | Generates free-form text from prompts. Supports `response_format` (e.g., JSON schema) for constrained outputs. Great for **non-conversational generation in agents**: code gen, summarization, or creative ideation without chat history. Extra params like `top_k` tune for diversity in exploratory agents. | ```bash<br>curl https://<your-vllm-uri>/v1/completions \<br>  -H "Content-Type: application/json" \<br>  -d '{<br>    "model": "your-model",<br>    "prompt": "Write Python code to query Akash leases:",<br>    "max_tokens": 512,<br>    "response_format": {"type": "json_object"}<br>  }'<br>```<br>Response: `choices` with formatted text. | Direct OpenAI client: `client.completions.create(...)`. In Haystack or LlamaIndex, use for prompt chaining in retrieval agents. Local for quick prototyping, remote for large models. |
| **/classify**<br>(Classification API) | Runs sequence classification (e.g., sentiment, toxicity, reward modeling) on batches of text. Outputs class probabilities. Perfect for **agent evaluation/guardrails**: Score responses for safety, relevance, or intent before forwarding in multi-agent comms. Supports `softmax` for calibrated probs. | ```bash<br>curl https://<your-vllm-uri>/classify \<br>  -H "Content-Type: application/json" \<br>  -d '{<br>    "model": "your-classifier-model",<br>    "inputs": ["Agent response: 'Proceed with deployment.'", "Risky query: 'Hack the network.'"]<br>  }'<br>```<br>Response: Array of `logits` or probs per input. | Integrate with Guardrails AI or NeMo Guardrails for agent moderation. Batch local/remote for fleet-wide logging. Model: Use fine-tuned BERT-like via `--runner classification`. |
| **/score**<br>(Score API) | Computes similarity (e.g., cosine) between query-document pairs or multimodal inputs. Batch-friendly for **retrieval scoring in RAG agents**: Rank candidates post-vector search (pair with your embeddings endpoint). Handles images for visual agents. | ```bash<br>curl https://<your-vllm-uri>/score \<br>  -H "Content-Type: application/json" \<br>  -d '{<br>    "model": "your-scorer-model",<br>    "query": "Akash deployment YAML",<br>    "documents": ["YAML for vLLM", "Unrelated doc"]<br>  }'<br>```<br>Response: Scores per pair. | LlamaIndex's `SentenceTransformerRerank` with custom endpoint. Route to remote for GPU-heavy cross-encoders; local for quick filters. |
| **/rerank**<br>(Re-rank API; also /v1/rerank, /v2/rerank) | Reranks document lists by relevance to a query using cross-encoders. Compatible with Cohere/Jina formats. Essential for **advanced RAG in agents**: Refine top-k results from embeddings for precise context injection. Outputs sorted indices + scores. | ```bash<br>curl https://<your-vllm-uri>/rerank \<br>  -H "Content-Type: application/json" \<br>  -d '{<br>    "model": "your-reranker-model",<br>    "query": "vLLM agent tooling",<br>    "documents": ["Doc1", "Doc2"],<br>    "top_n": 3<br>  }'<br>```<br>Response: `results` with `index`, `relevance_score`. | LangChain's `CohereRerank` adapter (swap base_url). Multimodal variant for image-text agents. Deploy with `--runner rerank` for efficiency. |
| **/v1/audio/transcriptions**<br>(Transcriptions API) | Transcribes audio to text (Whisper models). Supports prompts/languages. For **multimodal agents**: Ingest voice commands in audio-based workflows (e.g., voice-to-agent pipelines). | ```bash<br>curl -X POST https://<your-vllm-uri>/v1/audio/transcriptions \<br>  -H "Authorization: Bearer token" \<br>  -F file=@audio.mp3 \<br>  -F model="whisper-large"<br>```<br>Response: `text` transcript. | Use with AssemblyAI or Whisper libs; feed to chat endpoint for voice agents. Remote for heavy audio processing. |

#### Deployment & Framework Tooling Tips
- **YAML Updates for Variety**: Clone your embedding YAML, swap `vllm serve <model>` (e.g., `meta-llama/Llama-3.1-8B-Instruct` for chat/tools), and adjust resources (e.g., 1 GPU suffices for most). Use `--enforce-eager` for local CPU fallbacks. Access via Akash forwarding URIs as before—add auth with `--api-key` for secure agent routing.
- **Agent Framework Routing**: 
  - **LangChain**: Define `ChatOpenAI(base_url=<uri>, model="served-name")` per instance. Use `Tool` wrappers to call endpoints dynamically (e.g., a "classify" tool).
  - **CrewAI/AutoGen**: Multi-agent setups with endpoint selectors for load balancing (local for cheap tasks, remote for complex).
  - **Observability**: Leverage `request_id` for tracing agent calls across instances.
- **Scaling in Your Network**: For Akash remotes, bid higher for GPU tenants. Local: Dockerize vLLM for Kubernetes. Test hybrid: Ping endpoints with `health` (vLLM adds `/health`).
- **Next Steps**: Start with chat completions for a basic ReAct agent prototype. If multimodal (audio/vision), prioritize Whisper/VLM models.

This setup turns vLLM into a unified inference backbone for your agents—efficient, canonical, and network-agnostic.
```


```yaml
---
version: "2.0"
services:
  vllm-embeddings:
    image: vllm/vllm-openai:v0.7.3
    expose:
      - port: 8000
        as: 8000
        to:
          - global: true
    command:
      - bash
      - "-c"
    args:
      - |
        vllm serve BAAI/bge-base-en-v1.5 --trust-remote-code --host 0.0.0.0 --port 8000 --served-model-name bge-base-en-v1.5
    params:
      storage:
        shm:
          mount: /dev/shm
        data:
          mount: /root/.cache # Mount the data storage to the cache directory for persistent storage of model files
          readOnly: false
profiles:
  compute:
    vllm-embeddings:
      resources:
        cpu:
          units: 8  # Reduced for embedding model efficiency
        memory:
          size: 32Gi  # Reduced for embedding model efficiency
        storage:
          - size: 50Gi
          - name: data
            size: 50Gi
            attributes:
              persistent: true
              class: beta3
          - name: shm
            size: 8Gi
            attributes:
              class: ram
              persistent: false
        gpu:
          units: 1
          attributes:
            vendor:
              nvidia:
  placement:
    dcloud:
      pricing:
          vllm-embeddings:
            denom: uakt
            amount: 1000000
deployment:
  vllm-embeddings:
    dcloud:
      profile: vllm-embeddings
      count: 1
```

### Deployment Overview

This updated SDL (Service Deployment Language) configuration deploys a vLLM OpenAI-compatible server optimized for serving the `BAAI/bge-base-en-v1.5` embedding model (a compact, high-performance English text embedding model from Hugging Face, ~0.3B parameters). Key changes from the original:

- **Service name**: Renamed to `vllm-embeddings` for clarity.
- **Model**: Switched to `BAAI/bge-base-en-v1.5`, an embedding-specific model. The `--served-model-name` flag sets the API model name to `bge-base-en-v1.5` for client compatibility.
- **Command**: Removed `--tensor-parallel-size 2` as it's unnecessary for this smaller model (single GPU suffices). Added `--trust-remote-code` for model loading.
- **Resources**: Scaled down CPU (8 units), memory (32Gi), and storage (50Gi each for general/data) to match the lighter embedding workload, while retaining 1 NVIDIA GPU for acceleration. SHM reduced to 8Gi.
- **Pricing/Placement**: Updated profile references to match the new service name; pricing remains illustrative (1 AKT max bid).

Deploy this via the Akash CLI:

1. Save as `embedding-deployment.yaml`.
2. Run `akash tx deploy embedding-deployment.yaml -y --from <key-name> --fees 5000uakt --node <akash-node>`.
3. Bid on the manifest: `akash tx market bid <deployment-id> --owner <deployer> -y --from <key-name> --fees 5000uakt`.
4. Once active (`akash query market lease list --owner <deployer> --state active`), note the **forwarding URI** from the lease (e.g., `https://<lease-id>-80.<provider>.akash.network`—Akash auto-provisions an HTTPS load balancer on port 80/443, proxying to your service's port 8000).

### Accessing the Deployed URI

The deployment exposes an OpenAI-compatible API at `/v1/embeddings` for generating embeddings. Use the forwarding URI as the base URL (e.g., `https://<lease-id>-80.<provider>.akash.network`). Akash handles TLS termination, so clients connect via HTTPS.

#### Using cURL (Query Example)

Generate embeddings for a single text input:

```bash
curl https://<your-forwarding-uri>/v1/embeddings \
  -H "Content-Type: application/json" \
  -d '{
    "model": "bge-base-en-v1.5",
    "input": "Your query text here, e.g., 'What is Akash Network?'"
  }'
```

- **Response**: JSON with `data` array containing `embedding` (vector of floats, dim=768 for this model) and `usage` stats.
- **Batch Input**: Pass an array of strings to `"input"` for multiple embeddings.
- **Our Network Integration**: If using Akash's default public network, the URI is globally accessible. For private "our network" (e.g., custom Akash provider chain or VPN), resolve the URI via internal DNS (e.g., `<lease-id>.<internal-domain>`) or tunnel (e.g., `kubectl port-forward` if on Kubernetes backend). Query the provider's lease details via `akash query market lease get <lease-id> --node <provider-node>` to confirm IP/port mapping.

#### Using Python (OpenAI Client)

Install `openai` library and query:

```python
from openai import OpenAI

client = OpenAI(
    base_url="https://<your-forwarding-uri>/v1",
    api_key="not-needed"  # vLLM doesn't require auth by default
)

response = client.embeddings.create(
    model="bge-base-en-v1.5",
    input="Your query text here"
)

print(response.data[0].embedding)  # List of floats
```

- **Error Handling**: Check for 4xx/5xx (e.g., model not loaded: 503). Monitor logs via `akash provider logs <deployment-id>`.
- **Scaling/Updates**: For production, add auth (`--api-key` in vLLM command) or scale `count: >1`. Redeploy with `akash tx deployment update <deployment-id> <new-yaml>`. Model downloads to `/root/.cache` on first run (persisted via storage).
