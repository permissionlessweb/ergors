Step 1: Define Networking Architecture and Protocol

## implement the networking support between nodes in cw-ho. 

We must use the commonware libraries for implmeenting the networking for nodes. Decide which libraries to use for this implementation out of the commonware ones:  
- commonware-broadcast: Disseminate data over a wide-area network.
- commonware-cryptography: powering unique node identities via Ed25519 pubkeys.
- commonware-collector: powering Collect responses to committable requests.
- commonware-p2p: powering Collect responses to committable requests.

This is what we asked for implementation of the networking in ho-core, but we must implement a minimal version without the fractal bloat that occured.


the commonware monorepo currently exists in submodules/monorepo, and the first try at the networking implementation for ho-core is in packages/ho-core/network
Objective: Establish a clear design for inter-node communication aligned with tetrahedral topology and sacred geometry, extending the current node identity .
Tasks: Document the network architecture, specifying how tetrahedral roles map to network interactions (e.g., Coordinator broadcasts tasks, Executors process tasks).
Specify how we make use of the commonware-crypto for networking communication reliable and securely,( suitable for sandloop feedback).

Outcome: A design specification (as code comments or a separate doc) for the networking layer, guiding subsequent implementation.

Step 2: Implement Inter-Node Communication Protocol
Objective: Build a runtime communication layer for agent nodes to exchange messages post-deployment.
Implement a NetworkAgent struct with methods for sending/receiving messages using the commonware library
Integrate with NetworkQuickStart to configure agents with network endpoints during deployment (leverage existing Consul for service discovery).


Step 3 – Distributed State Synchronisation
Objective: extend function of existing state store  that the CosmicOrchestrator always possesses a recent, coherent view of the whole network. State is pushed from each node to the coordinator at configurable intervals using the cnardim storage stack (JMT + RocksDB).
 
