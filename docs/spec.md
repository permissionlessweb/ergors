
# Ergors Specification

This document provides a comprehensive specification for the Ergors codebase, detailing the functionality, flow, and implementation of core components based on existing documentation and code structure. It is organized by the specified categories, drawing from docs/specs files and code in packages/ho-std and packages/cw-ho. The spec emphasizes truthful representation of the current codebase, highlighting key traits, modules, and geometric principles (e.g., golden ratio allocation, tetrahedral topology, Möbius sandloops, fractal recursion).

## Purpose

## Design

## Storage: Cnidarium

## Keys

user keys and node keys. nodes must register user keys to grant authorization to api middleware.

### Key Types

### Custody Client

## Node: Commonware

## Network

## LLM

## Agentic Functions

### prompt note

1. inital request

- request
- function defintion

1. response

- messages field
        - assistant text
        - tool-call function
- where function calls given in api result are passed to ergors and performed.
-

1. request with function call result

sends response back llm inference with:

- chat history: can be preprocessed for syntax & filtering, can be optimized by storing hash of embedding of response and store provide in context
- function call result: can be prepressed before re
- provide original user request

1. response

- same as inital response, creating agentic loop
- use as checkpoint for agentic action merkle tree compression for hashes of actions logged

## Privacy

## auction plan: agentic pair loop

    - both have exchange rate for ratio diffusion over time of various parameters 
    - private prompt and response lp positions during loop
    - action duration hashed and merkleized for proof inclusion

## Programmability

### Cosmwasm-VM

## Interoperability

### IBC Wasm Client

## Trustlessness: Mathemtatical Precision

We will create accountable expect set of data points during agentic sessions due to loggic tracing of loggic actions and errors. This will let us have a known map of sequences per agentic session, as like a proof circuit table, and give the change to introduce transport middlewares for node connection filters on a local-permit basis (essentially firewalls to ensure logs and actions are within bounds consentfully specified between node connections).

We will be able to increment the amount of actions a node takes (write,read,response), and constrain the hash of the transitions of the state of the node during the runitme, creating a thread of hashes that can be used for recursive proof of knowledge of a state commitment.

## Verifiable Release Builds

## Dao Managed Codebase

## Light=Client Certianty

### Logs

## Autonomy

### Workflows

### Reflection

### Sandloops

## Research

- <https://docs.x.ai/docs/guides/function-calling>
