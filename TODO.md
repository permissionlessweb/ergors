# PRIORITIZE SOVEREIGN MODEL USE

- PRIVACY: nanogpt,venice

- DEEP WIKI
- SESSION SNAPSHOT AND STORAGE WORKFLOW TESTS

- notifications on api key request from nodes
- CONSENSUS CONFIRMS COMMITMENTS TO UNIQUE NODE STATES
  - create commmitments to nodes internal data, share with network, bft consensus on network-wide commitment state

- cosmos query + function macro: engine design of server api where an  api query to any cosmos-rpc endpoint i sproxied,  but also incluses a dedicated function to handle the response, creating super modular and clean code for interacting with cosmos chains.
- automate the advamcement of the depoyment workflow to akash ()
  - flags should signal if authz /feegrant is to be used during inital setup
  - balance automatically gets queried for account, engine ends workflow or advances dependendt on balance
  - certificate must be present and available for use, check if we have one saved into cnidarium storage, or one must be created for akash (this is a tx braodcasted to akash, must handle tx lifcycle (broadcast, wait for block finality (~6s), check tx response, parse for certificate (we can use the akash prost types for exact type definitions of request and response)))
  - sdl must be automated and formed from the sdl contract registrar (or if a fully ready sdl is passed, we can skip this step)
  - braodcasting thea ask to the marketplace must be automated, this is a known tx and will need to handle error cases, ensure tx was included in block after finality (~6s)
  - selecting bids should be automated in the sense that the bot will immediately display the options for the user to select from. will need to give network 2 blocks to allow bids to occur, and then query bids for the ask just deployed, diplsya a list of all the bids and their details about provider and cost so user can then select. support for defining default providers to filter bids from non trusted providers must bve added, where we ssave these default providers in a config value. must be able to add, remove providers from this list on runtime. selecting a bid broadcast to the network, need to handle error cases and successful response
  - must be able to handle displaying details regarding the active lease status, including logs and events, uri's and ports, this will be a polling based flow as different sdl leases will take various times for inital settup and configuration
  - support for closing an active lease. this is a tx broadcast to akash, with a knows type from the prost genreated types.
  - we also need to ensure we accuraly map uris with their given ports mapped to the services for extremely effective, idiomatic, and accessablility for our engines to use them for inference calls. we should use the cnidarium storage for this purpose

  right now we need these changes because it seems like the workflow is sequential and manual and expects users to specify incrementing each step in the deployment where we want this much more automated in the sense the user can watch the workflow occur

## COSMWASM

<!-- - contract to keep registry of access to node api -->
- configure cosmwasm contract for api auth middleware
- configure workflow for creating ephemeral accounts for akash deployments + feegrant from main orchestrator node (revents leaking private keys for sensitive account to providers on akahs, since env variables are public)
- akashsdl registrar (store sdk templates for use on akash network, included details about vairables to prompt to populate)

- RECIPIES: EMBEDDING STORAGE: Ability to store embeddings for reuse of known/familiar/common code for agentic workflows. correct storage mapping and data routing.
  - create new session with details about the task we are performing. Saves session information in organized storage structures. Used for creating accurate traiding data for fine tuning.
  - inlcude metadata in data structure with prompts, provide them within context if desired feature that will allow use for guided classification of data during these sessions (prepare,install,start,use,debug,export).
  - SOCRATIC LOOP CUSTOMIZATION: 3-point refactor to make this function -like, where there is a generic template to define for each prompt, with data to include for each granular interfaces: get action to perform, run socratic script with defined actors, even incrementing steps temperature.
  - Agentic Field Focus: reference various agentic building frameworks that exists in the wild, cherrypick features related to agents and ai models we are not focusing on
  - MCP server support: Deployment configuration templates
  - AKASHIC RECORDS: storage layer for active and historical sessions data. Makes use of recursive sublayer structure for accurate data location mappping within 4 dimensions for retroactive fine tuning. Each layer has a dimension to map for its local node data and **a reference (active hash (is this the storage prefix since we make use of cnardium? can we make prefix deterministic))**.
- CONFIG PATH SANITY TEST: ENSURE OUR CURRENT CONFIGURATION PATHS ARE WIRED IN PROPERLY WITH OUR NEW INTERFACES WITH ACCURATE LOGIC FOR CW-HOE
- CONSTANT-IZE REPO: ensure we parse through codebased and make constant values for classification into proto for easy iterations and mitigation of changes coming from errors.
- NETOWRK COMPRESSION TEST:  ensure we can compress network storage upstream to orchestration node via e2e tests
  - create snapshot & export data into fodler for future classification
- protobuf & swagger api definitions:
- HIFI PROMPT DEFINITION TEMPLATES:
  - Dense.high frequency, prompt generation with recursive refinement: Prompt templates for each prefix/postfix of agent action. reusable, includes context & schema for specific agent workflow.

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
- cw-jsonfilter chips: resulable chips for filtering out data during agentic instruct prompts for simpler tasks
- rust analyser filter: filter for token count minimization

## NETWORK

- improve node_identity definitions:
  - validate state thoughout network by taking advantage of merkle cnidarium merkle tree
  - update node type
  - grant permissions for auth bypass
  - grant filters on node communication requests

## NODE ACTIONS

## SECURITY

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

- save non sensitive config values into storage layer
- embedding layer: local agest task embedding, project specific embeddings shared throughout network for access a nd proof verification

### CONFIG

- Capabilities: Description of the resources for the local environment this nodes engine is running on.
- Networking Consensus: snapshots of health status for nodes, net sure.
- Node Identity & Registry: node communication info circut layer.

## ORCHESTRATOR SERVICE

- unit + integration tetss
- [structured outputs for functional calling during sandloops and other agentic sessions](https://docs.x.ai/docs/guides/structured-outputs)
- Sandloops: Extend sandloop execution so outputs from one node become inputs for another node, maintaining Möbius continuity over the network. This requires state synchronization and inter-node messaging.

## Secret Values

- update api keys support to be raw encrypted kv map

## TESTING

<!-- - ci testing: simulated llm api for determinstic responses and known workflows -->
- mutation testing
- proptest: <https://proptest-rs.github.io/>
- akash automation unit tests:
  - assurance of grant internal grant request/ approve workflow
  - assurance of support for sdl options (using cw-sdl if enabled, bypassing if sdl flag is set)
  - parsing/accepting bids from trusted providers
  - allowing selection of bids from any providers support
  - parsing accepted bid status for uri(s)
  - support for deploying multiple asks to markeplace for rendting in parallet separates concerns
- migrations: ensure new features have support for dro-in replacements

## AI

### AI TOOLS

## AGENT WORKSPACES

- <https://arxiv.org/html/2403.08299v1>
- PROJECT ALICE
- PHIDATA: <https://docs.phidata.com/>
- <https://www.gradio.app/>

### GROK TOOLS

### OPENAI TOOLS

refactor mesage to derive openapi defintion structure from our api messages.

### CLAUDE TOOLS

- ergo-rs cli hooks:

### KIMI TOOLS

<https://github.com/MoonshotAI/kosong>

> API CALL GRANULARITY:
>
> - [`container`](https://docs.claude.com/en/api/messages#body-container)
> - [`context_management`](https://docs.claude.com/en/api/messages#body-context-management).
> - [`mcp_servers`](https://docs.claude.com/en/api/messages#body-mcp-servers)
> - [`metadata`](https://docs.claude.com/en/api/messages#body-metadata)
> - [`tools`](https://docs.claude.com/en/api/messages#body-tool-choice)

- JSON EXTRACTOR,CALCULATOR,
- TOKEN EFFECIENT TOOL USE
- [BASH TOOL](https://docs.anthropic.com/en/docs/agents-and-tools/tool-use/bash-tool)
- [REMOTE MCP SERVERS](https://docs.anthropic.com/en/docs/agents-and-tools/remote-mcp-servers)

### Quen Code

### Kimi

## TEXTUALIZE

## Research

<https://medium.com/@farissyariati/ask-your-codebase-anything-using-ollama-embeddings-and-rag-c65081a5ef20>
<https://github.com/AsyncFuncAI/deepwiki-open>
<https://medium.com/@sjng/deepwiki-why-i-open-sourced-an-ai-powered-wiki-generator-b67b624e4679>

## COMPLETE

- ~~FIRST PRINCIPLE INITIALIZATION SCRIPTS: ensure reusable, multi-environment compatiblility/ guided deployment steps~~ my-first-ho.md.
