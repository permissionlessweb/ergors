# PRIORITIZE SOVEREIGN MODEL USE

- SESSION SNAPSHOT AND STORAGE WORKFLOW TESTS

- RECIPIES: EMBEDDING STORAGE: Ability to store embeddings for reuse of known/familiar/common code for agentic workflows. correct storage mapping and data routing.
  - create new session with details about the task we are performing. Saves session information in organized storage structures. Used for creating accurate traiding data for fine tuning.
  - inlcude metadata in data structure with prompts, provide them within context if desired feature that will allow use for guided classification of data during these sessions (prepare,install,start,use,debug,export).
  - SOCRATIC LOOP CUSTOMIZATION: 3-point refactor to make this function -like, where there is a generic template to define for each prompt, with data to include for each granular interfaces: get action to perform, run socratic script with defined actors, even incrementing steps temperature.
  - Agentic Field Focus: reference various agentic building frameworks that exists in the wild, cherrypick features related to agents and ai models we are not focusing on (<https://docs.letta.com/>)
  - MCP server support: Deployment configuration templates
  - AKASHIC RECORDS: storage layer for active and historical sessions data. Makes use of recursive sublayer structure for accurate data location mappping within 4 dimensions for retroactive fine tuning. Each layer has a dimension to map for its local node data and **a reference (active hash (is this the storage prefix since we make use of pnardium? can we make prefix deterministic))**.
- CONFIG PATH SANITY TEST: ENSURE OUR CURRENT CONFIGURATION PATHS ARE WIRED IN PROPERLY WITH OUR NEW INTERFACES WITH ACCURATE LOGIC FOR CW-HOE
- CONSTANT-IZE REPO: ensure we parse through codebased and make constant values for classification into proto for easy iterations and mitigation of changes coming from errors.
- NETOWRK COMPRESSION TEST:  ensure we can compress network storage upstream to orchestration node via e2e tests
  - create snapshot & export data into fodler for future classification
- protobuf & swagger api definitions:
- HIFI PROMPT DEFINITION TEMPLATES:
  - Dense.high frequency, prompt generation with recursive refinement: Prompt templates for each prefix/postfix of agent action. reusable, includes context & schema for specific agent workflow.
- FIRST PRINCIPLE INITIALIZATION SCRIPTS:
  - ensure reusable, multi-environment compatiblility/ guided deployment steps
- MULTI-AGEBNT: Multi‑LLM routing system with prompt‑refinement loops
- FUN: Web interface & API with referee service integration (chat interface, network visualizer, infrence visualizer)
  - data vis
  - egui
  - <https://textual.textualize.io/>
  - n8n
  - [langfuse trace data-model observability compatibility](https://langfuse.com/docs/observability)
  - [langfuse metric dashboard](https://langfuse.com/docs/metrics/overview)
- DOCKER DEPLOYMENT STRATEGY:
  - AKASH workflow (terraform/custom bash scripts)
- benchmarking via criterion

## SECURITY

- enhance identity key security: use soft-kms for storage and access to node key
- ephemeral users: create dedicated user profile for containerized instance to granularize access to agents in workflow
- [api-key storage](https://crates.io/crates/keyring):

## COMMUNICATION

- ELASTIC SEARCH + KIBANA SUPPORT: search through db with relevant historical context
- OBSERVABILITIY:
- RERUN: <https://github.com/rerun-io/rerun>
- DOCUMENTATION: elastidocs
- INTERNAL LLMROUTER: Sharpen accuracy, and iterability for our llm router by using constant defined variables ,(so were not hard coding api endpoints & into the logic)

## DEPLOYMENTS

- 1 Click deployments: ssh, installation, & configuration of nodes into quickstart
- default docker containers
- API,GRPC,REST

### STORAGE LAYER ARCHITECTURE

#### TODOS

- document storage layer structure assigments accurately via proto files
- document minimal current trait requirements for the storage layer
- document visualation demo

### NODE CONFIG

- Capabilities: Description of the resources for the local environment this nodes engine is running on.
- Networking Consensus: snapshots of health status for nodes, net sure.
- Node Identity & Registry: node communication info circut layer.

## ORCHESTRATOR SERVICE

- ENSURE ORCHESTRATOR SERVICE RUNS TO PROCESS AS API
- Sandloops: Extend sandloop execution so outputs from one node become inputs for another node, maintaining Möbius continuity over the network. This requires state synchronization and inter-node messaging.

## AI

### AI TOOLS

## AGENT WORKSPACES

- PROJECT ALICE
- PHIDATA: <https://docs.phidata.com/>
- <https://www.gradio.app/>

### GROK TOOLS

### OPENAI TOOLS

### CLAUDE TOOLS

- JSON EXTRACTOR,CALCULATOR,
- TOKEN EFFECIENT TOOL USE
- [BASH TOOL](https://docs.anthropic.com/en/docs/agents-and-tools/tool-use/bash-tool)
- [REMOTE MCP SERVERS](https://docs.anthropic.com/en/docs/agents-and-tools/remote-mcp-servers)

### Quen Code

### Kimi

## TEXTUALIZE
