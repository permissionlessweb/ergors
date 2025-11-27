
# Ergors
<!-- https://en.wikipedia.org/wiki/Ergodicity -->
___
___
___
___
___
___
___
___
___
___
___
___
___
___
___
___
___

<div align="center">

+∑≠-∑+∑≠-∑+∑≠-∑+∑≠-∑+∑≠-∑+∑≠-∑+∑≠-∑+∑≠-∑+∑≠-∑+∑≠-∑+∑≠-∑+∑≠-∑+∑≠-∑+∑≠-∑+∑≠-∑+∑≠-∑+∑≠-∑+∑≠-∑+∑≠-∑+∑≠-∑+∑≠-∑+∑≠-∑\
**REDUCE CREATIVE FRICTION THROUGH INTELLIGENT AUTOMATION FOR PUBLIC GOODS**.\
*GOAL: Amplify human creativity through removing obstacles between alignment of intention and manifestation.*\
+∑≠-∑+∑≠-∑+∑≠-∑+∑≠-∑+∑≠-∑+∑≠-∑+∑≠-∑+∑≠-∑+∑≠-∑+∑≠-∑+∑≠-∑+∑≠-∑+∑≠-∑+∑≠-∑+∑≠-∑+∑≠-∑+∑≠-∑+∑≠-∑+∑≠-∑+∑≠-∑+∑≠-∑+∑≠-∑
</div>

___
___
___
___
___
___
___
___
___
___
___
___
___
___
___
___
___
<div align="center">

This is a **Rust binary** LLM orchestration engine. A recursive tool for building tools. It interfaces with the AI agents; deterministically store requests, acknowledgements, process logs, and responses for actions during an agentic llm for:

 reflection\
 reuse\
 context refinement

Engines create engines, strands become webs, task become distribution, accountably.
</div>

By leveraging multiple LLMs with **Prompt Request Loops**, **Data Ingestion Sandloops**, **Testing Edge‑Case Loops**, and **Random Audit/Snapshot Loops**, we create a pipeline that maximizes each agent’s strengths while maintaining deterministic, auditable workflows across the entire network.  

___

## Network‑Enhanced Creation Tools  

The first feature of this rust binary is a an network node for our agent development workspace:

* Nodes use cryptographic identities (**Call other nodes** for complex task coordination)
* Nodes are self replicating: *(**Manage & request distributed state** across the network for comprehensive context)*
* Nodes are self optimizing: *(**Execute prompt sandloops** for continuous quality improvement  )*
* We implement the P2P network configuration & cryptographic identities with the **Commonware** library.  
* We provide deterministic state management inside each node binary, via **Cnidarium**
* Nodes ingest state back to the core orchestrator in **snapshot** form, reducing storage on all nodes during long‑running agent workflows and fallbacks.

## Agentic Alignment tech

1. Requirements Mgmt: Define verifiable goals to align autonomy & prevent drift.
2. Risk Mgmt: Mitigate uncertainties like bias for reliable decisions.
3. Verification & Validation: Test AI in real scenarios for safety & efficacy.
4. Lifecycle Mgmt: Guide iterative dev from concept to ops.
5. Design Definition: Optimize architectures via trades for goal-oriented behavior.

## Packages

| Package | Default | Description |
|----------|---------|-------------|
| Ergo-Rs | Main Node Engine ||
| Ho-Std | Standard Library ||
| Ergors-Proto (Derive) | Proto Definitions & Derive Macros ||

## QUICKSTART

First a node must be initialized:

```sh
# creates a nodes home directory, identity keys, and default configuration:
cargo run  --bin ergors init 
```

Then you will want to configure your inference providers api-keys:

```sh
# invokes an interactive portal to configure the api-keys
cargo run  --bin ergors init llm-api-keys 
```

Now, you can start the engine:

```sh
# initialize the storage layer, transport layer, and any other services
cargo run  --bin ergors start 
```

 cargo run  --bin ergors init llm-api-keys

## Environment Variables

### RUST_LOG

> [for a dedicated list of environment variables and their defaults check here.](./packages/ho-std/src/constants.rs)

Controls the logging level for the entire application. This is the standard Rust tracing environment variable.

**Levels** (from least to most verbose):

* `error` - Only errors
* `warn` - Warnings and errors
* `info` - Informational messages, warnings, and errors (default)
* `debug` - Debug information plus all above
* `trace` - Trace-level debugging plus all above

**Examples**:

```bash
# Basic levels
export RUST_LOG=info          # Default - general operational logs
export RUST_LOG=debug          # Detailed debugging information
export RUST_LOG=trace          # Very verbose trace-level logging

# Module-specific levels
export RUST_LOG=ergors=debug,tower_http=info    # Debug for ergors, info for tower_http
export RUST_LOG=ergors::server=trace            # Trace only server module

# Target specific components
export RUST_LOG=ergors::middleware=debug        # Debug middleware operations
export RUST_LOG=ergors::storage=trace           # Trace storage operations
```

## Testing Library

Tests using orchestration servers are essentially scripts that can be used to verify integrity of logic, including its deployments, upgrades and migrations. we have a dedicated library and tooling specifically for this purpose.

### Mock Server

### Mock Inference Provider

* static responses from prompt requests
  * completions
  * prompts
  * toolcalling
  * api calls
  * mpc servers
  * embeddings

### Authentication Testing

* key gen siging libary
* custody middleware integration tests (reference penumbra testing library)
* integration test library
