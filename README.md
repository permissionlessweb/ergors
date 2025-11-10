
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
| Cw-Hoe | Main Node Engine ||
| Ho-Std | Standard Library ||
| Ergors-Proto (Derive) | Proto Definitions & Derive Macros ||

## QUICKSTART

First a node must be initialized:

```sh
# creates a nodes home directory, identity keys, and default configuration:
cargo run  --bin cw-ho init 
```

Then you will want to configure your inference providers api-keys:

```sh
# invokes an interactive portal to configure the api-keys
cargo run  --bin cw-ho init llm-api-keys 
```

Now, you can start the engine:

```sh
# initialize the storage layer, transport layer, and any other services
cargo run  --bin cw-ho start 
```

 cargo run  --bin cw-ho init llm-api-keys

## Environment Variables

[for a dedicated list of environment variables and their defaults check here.](./packages/ho-std/src/config/README.md)
