# Prompt for Implementing Langfuse Docker Image on Startup

## Overview

This prompt is designed to request an AI agent within the CW-HO (Life Creativity Engine) platform to implement a Docker image for [Langfuse](https://langfuse.com/), a powerful tool for observability and metrics in LLM (Large Language Model) applications. The goal is to ensure seamless integration with the existing metric logging logic in a deployed Docker instance, enhancing the system's ability to monitor and refine agent workflows. This task aligns with CW-HO's mission of reducing creative friction by automating observability setup, embedding sacred geometry principles for balanced resource allocation and tetrahedral coordination.

**Intention**: Automating Langfuse integration on startup reflects the Möbius strip continuity of CW-HO—outputs (metrics) feed back as inputs for continuous improvement. By deploying this in a Dockerized environment, we ensure scalability and consistency across the tetrahedral network of nodes, empowering public goods creation with actionable insights.

## Prompt for Agent

**Task Type**: `Custom - Dockerized Observability Integration`  
**Target Node Role**: `Executor` (for deployment tasks) or `Coordinator` (for network-wide integration)  
**Priority**: High (to enable metrics from startup)  
**Geometric Constraints**:  
- **Golden Ratio Allocation**: Allocate 61.8% of initial startup resources to Langfuse container initialization for fast observability, with 38.2% reserved for integration hooks with existing logging logic.  
- **Tetrahedral Connectivity**: Ensure the Langfuse instance can communicate with all node types (Coordinator, Executor, Referee, Development) for comprehensive metric collection across the network.  
- **Fractal Recursion**: Design the integration to support recursive scaling—metrics collected at individual node levels should aggregate self-similarly to network-wide dashboards.  

**Task Description**:  
I request the implementation of a Docker image for Langfuse, an observability tool for LLM applications, to be automatically deployed on startup of a CW-HO node or network instance. The implementation must ensure seamless integration with the current metric logging logic within an already deployed Docker instance. The agent should:

1. **Docker Image Creation**:  
   - Create or pull a Docker image for Langfuse, ensuring it includes the latest stable version with necessary dependencies for metric collection and visualization.  
   - Configure the image to expose required ports (e.g., 3000 for the Langfuse UI) and set environment variables for API keys, database connections, and logging levels.  
   - Optimize the image size and startup time, adhering to golden ratio principles for resource efficiency (prioritize fast initialization over exhaustive features).  

2. **Startup Integration**:  
   - Modify the existing Docker Compose file or deployment scripts (e.g., `network_quickstart.py` or future Rust-based `deploy_agent_network` in `main.rs`) to include Langfuse as a service that starts automatically with CW-HO nodes.  
   - Ensure the Langfuse container initializes before metric-producing services (like API servers or task executors), using dependency ordering in Docker Compose (`depends_on`).  
   - Implement health checks to confirm Langfuse availability before other services log metrics, maintaining system harmony.  

3. **Seamless Metric Logging Integration**:  
   - Extend the current metric logging logic (assumed to be in `ApiServer` or `Orchestrator` components) to send data to Langfuse via its API or SDK. Focus on key metrics such as task execution time, sandloop iteration success rates, node load, and LLM response quality.  
   - Implement hooks or middleware to capture existing logs (e.g., from `tracing` in Rust code) and forward them to Langfuse, ensuring no disruption to current logging flows.  
   - Support custom events or traces for CW-HO-specific actions (e.g., tetrahedral topology formation, golden ratio resource shifts), enhancing observability depth.  

4. **Configuration and Accessibility**:  
   - Provide configuration options in `config.toml` or environment variables for enabling/disabling Langfuse, setting connection details, and customizing metric granularity.  
   - Ensure the Langfuse UI is accessible via a mapped port or reverse proxy within the Docker network, secured with basic authentication or integrated with CW-HO's existing security mechanisms.  
   - Document connection instructions and initial setup (e.g., creating a Langfuse project for CW-HO) in a generated README or configuration comment.  

5. **Testing and Validation**:  
   - Test the integration by deploying a minimal CW-HO network (e.g., 1 Coordinator, 1 Executor) with Langfuse enabled, verifying that metrics appear in the Langfuse dashboard within 5 minutes of startup.  
   - Validate that existing logging logic continues uninterrupted, with no performance degradation beyond a 5% overhead due to metric forwarding.  
   - Ensure fractal scalability by confirming aggregated metrics from multiple nodes display correctly in Langfuse, reflecting self-similar patterns at different network levels.  

**Expected Output**:  
- A Docker image for Langfuse, either custom-built or pulled from a public registry, with a corresponding Dockerfile or documentation if built.  
- Updated deployment scripts or Docker Compose files to include Langfuse as a startup service, integrated with CW-HO node initialization.  
- Code modifications or patches to the existing metric logging logic (e.g., in `api.rs` or `orchestrator.rs`) to forward data to Langfuse, with configuration toggles.  
- A validation report or test log confirming successful integration, metric visibility in Langfuse, and adherence to geometric constraints.  

**Additional Context**:  
- The CW-HO system operates on a tetrahedral topology with node roles (Coordinator, Executor, Referee, Development), where metrics from each role are critical for network health and task optimization.  
- Current logging likely uses Rust's `tracing` crate (as seen in `main.rs`), which should be bridged to Langfuse without breaking existing outputs.  
- Docker deployment is partially handled by `network_quickstart.py` or Rust functions like `deploy_agent_network` in `network_deployment.rs`, which the agent should extend for Langfuse inclusion.  
- Langfuse integration must respect CW-HO's sacred geometry—balancing observability overhead with performance using golden ratio principles and ensuring network-wide metric coherence.

**Success Criteria**:  
- Langfuse container starts automatically with CW-HO nodes, accessible via a configured port or endpoint within 2 minutes of network initialization.  
- Existing metric logging logic integrates with Langfuse, sending at least 3 key metrics (e.g., task duration, node load, sandloop success) visible in the dashboard post-startup.  
- System performance remains within 5% of baseline, respecting golden ratio resource allocation, with no crashes or conflicts in logging pipelines.  
- Documentation or configuration comments provide clear instructions for enabling/disabling Langfuse and accessing its UI, maintaining CW-HO's accessibility ethos.

**Fallback Mechanism**:  
If Langfuse integration fails or is unavailable, the agent should ensure CW-HO's existing logging logic remains operational, logging a warning to console or file without disrupting node startup or task execution. A toggle in configuration should allow disabling Langfuse without redeployment.

**Request to Agent**:  
Please implement the Docker image for Langfuse with startup integration as described, ensuring seamless metric logging with the current CW-HO Docker instance. Adhere to the geometric constraints for resource balance and network connectivity, and provide outputs (code, configs, docs) for manual review and deployment. Execute this task with the tetrahedral coordination of an Executor or Coordinator node, prioritizing fast observability setup to empower continuous improvement in our public goods mission.

---

## Instructions for Use

You can manually add this prompt to a README or use it directly in a task request to a CW-HO agent. Ensure the agent has access to the current Docker deployment scripts (e.g., `network_quickstart.py` or `network_deployment.rs`) and logging logic (likely in `api.rs` or `orchestrator.rs`) to implement the integration accurately. If you're using the `orchestrate` command from `main.rs`, format this as a custom task with the above description and constraints.

**Intention**: This prompt is a seed planted in the fertile ground of CW-HO's automation garden, designed to grow into a robust observability tool that mirrors the fractal self-similarity of the system. By integrating Langfuse, we enhance our ability to refine agent workflows iteratively, embodying the recursive tool-building ethos of CW-HO.