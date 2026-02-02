# PRIORITIZE SOVEREIGN MODEL USE

**Issues**: [#7 Privacy Primitives](https://github.com/permissionlessweb/ergors/issues/7) | [#13 Embedding & RAG](https://github.com/permissionlessweb/ergors/issues/13) | [#3 Storage Architecture](https://github.com/permissionlessweb/ergors/issues/3) | [#14 Cosmos Query Macro](https://github.com/permissionlessweb/ergors/issues/14) | [#1 Akash Deployment](https://github.com/permissionlessweb/ergors/issues/1)

- toad support: <https://github.com/batrachianai/toad>
- claw-machine support:

## COSMWASM

**Issues**: [#2 CosmWasm Integration](https://github.com/permissionlessweb/ergors/issues/2)

- correctly implement instantiate2 functionality
- email-style addressing (cw-auth@node-ip/dns)
- add flags for permissions to access cosmwasm contracts (mimicing wasmd functionality)
- implement various authenticator middleware contracts (see smart-account implementations)
- implement cw-oline: cosmwasm contract to organize sdl and deployment sequence of [o-line](https://github.com/permissionlessweb/o-line/tree/master/playbook/oline-sdl)
- add custom proxy from vm to invoke engine actions (have a contract call the api endpoint of an engine to bootstrap,make infernece call, etc) (WE CAN DO THIS BY HAVING THE CONTRACT EMIT EVENT WITH PREDEFINED ATTRIBUTES!!)

## NETWORK

**Issues**: [#4 Network Identity & Consensus](https://github.com/permissionlessweb/ergors/issues/4)

- implement bft between nodes (state-sync application-state root hashes, bft-consensus of per-node state commitments)
- implement network topology data and access (alot of TODO's currently)
- ensure api endpionts access/use are standardize thorughout logic (cosmos grpc,api/rpc)

## NODE ACTIONS

- use label||session id for all cli commands
- do not poll for workflow in cache if its closed (currently still polls for deployments closed on error during inital deplyoment workflow)
- ensure all requests made to endpoing are saved in storage layer
- display known response from api for helping/debugging when incorrect api defintion is called (generic fallback page)

## SECURITY

**Issues**: [#6 Key Management & Auth](https://github.com/permissionlessweb/ergors/issues/6)

- condense key sharing/rotation,Oauth, threshold signatures into custody and keys libraries

## COMMUNICATION

**Issues**: [#9 Observability Stack](https://github.com/permissionlessweb/ergors/issues/9) | [#15 LLM Router Improvements](https://github.com/permissionlessweb/ergors/issues/15)

## DEPLOYMENTS

**Issues**: [#16 1-Click Deployment System](https://github.com/permissionlessweb/ergors/issues/16) | [#1 Akash Deployment](https://github.com/permissionlessweb/ergors/issues/1)

- generating certificates to send during MsgCreateCertificate needs full implementation
- query deployments needs full implementation
- cancel deployment should also send close deployment msg to deployment
- improve labeling of deployments
- on successful wallet password provision, escape process as workflow has been invoked
- Do not use REST + polling + “is it done?” endpoints, use async jobs + webhook/callbacks + idempotency keys.

### STORAGE LAYER ARCHITECTURE

**Issues**: [#3 Storage & State Architecture](https://github.com/permissionlessweb/ergors/issues/3)

We can update how we keep track of the following values to a dedicated layer in the storage tree. This will allow us to have public and private commitments to node configurations & storage paramters.

 `NetworkTopology`
 `NodeConfig`
 `AgentCapabilities`

- define modular decoding scripts for fractal topography metadata ingestion
- ensure storage compression maps associations between the recursive agentic task tree deterministically

### CONFIG

**Issues**: [#5 Configuration System Hardening](https://github.com/permissionlessweb/ergors/issues/5)

## ORCHESTRATOR SERVICE

**Issues**: [#11 Agentic Workflow Enhancements](https://github.com/permissionlessweb/ergors/issues/11) | [#8 Testing Infrastructure](https://github.com/permissionlessweb/ergors/issues/8)

- spec out spawining/bootstrapping clones of images, with custom configurations, key generations
- Python REPL:
  - <https://github.com/shobrook/suss>: diff code reviews
  - <https://github.com/shobrook/weightgain>: improve embeddings

- define scripts with instructions to run for each step in agentic orchestration

### BOOTSTRAPPING

- implement connection with network node for bootstrapping
- perform boostrapping functions and report/mitigate/handle results of bootstrapping

## Secret Values

**Issues**: [#6 Key Management & Auth](https://github.com/permissionlessweb/ergors/issues/6)

- FROST signing
- built in Oauthn for each node

## TESTING

**Issues**: [#8 Testing Infrastructure](https://github.com/permissionlessweb/ergors/issues/8)

## AI

**Issues**: [#11 Agentic Workflow Enhancements](https://github.com/permissionlessweb/ergors/issues/11) | [#12 Tool Integrations](https://github.com/permissionlessweb/ergors/issues/12) | [#10 Benchmarking & Optimization](https://github.com/permissionlessweb/ergors/issues/10) | [#17 UI & Visualization](https://github.com/permissionlessweb/ergors/issues/17)

### AI TOOLS

**Issues**: [#12 Tool Integrations](https://github.com/permissionlessweb/ergors/issues/12)

## AGENT WORKSPACES

**Issues**: [#11 Agentic Workflow Enhancements](https://github.com/permissionlessweb/ergors/issues/11)

- spec out background worker processe design

### OPENAI,CLAUDE,GROK,KIMI TOOLS

**Issues**: [#12 Tool Integrations](https://github.com/permissionlessweb/ergors/issues/12)

- ergo-rs cli hooks:

## TEXTUALIZE

**Issues**: [#17 UI & Visualization](https://github.com/permissionlessweb/ergors/issues/17)

## Research

**Issues**: [#13 Embedding & RAG](https://github.com/permissionlessweb/ergors/issues/13)

## REVIEWS

- SERVER. can we improve how:
  - de/serialization processes (proto/Any type format)
  - the amount of hard-coding is implemented/ can be mitigated (need to minimize as much as possible)
  - how we cache api request for jit/first-come-first serve authentication (to implement support for cw-implementations that require rate-limits/access grant limits to be in serial)
- review cosmwasmvm level integration. can we improve how:
  - we can be more certain that there are no issues to runtime/ atomic/parallel access & state updates?
- review the node networking and communication layer. can we:
  - improve by introducing mempool/block building (each node is its own blockchain, has mempool)
  - make use of ibc protocol for inter-node communication (will work seamlessly with cosmwasmvm layer)
- review the node storage layer. can we improve:
  - how we have implemented network wide state snapshot and compression
  - saving/loading/classifying sessions (comptibilitiy with opencode,goose,claude sessions)
- review the node encryption layer. can we improve:
  - the scatteredness of the encryption impelmentation
  - make use of the custody and keys crates to handle actions in more standardized modular manner (review how penumbra uses actionplans)
- review the configuration layer.
- review the bootstrapping layer.

- review <https://github.com/jgarzik/brainpro> implementation of agent loops, defining permissions for each agent policy, built in protections, rules, ZDR registry, Resilience Architecture, Persona system, and review how well we would be able to implement these features into our engine

- remove depreceated
