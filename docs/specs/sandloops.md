# 📜 Sandloops Specification – Unified Agentic Workflow  

*Version: 1.0 | Last Updated: 2025‑08‑10*  

---  
<!-- Q: how can sandloops be more classified for optimized, yet fully functional specification?: composite-key-formula? deployable wasm-vm scripts? -->
## 1. Overview  

The **Sandloops** subsystem implements four Möbius‑strip feedback loops that embed the sacred‑geometry principles of CW‑HO (golden‑ratio resource splits, tetrahedral connectivity, fractal self‑similarity, and Kepler‑optimal packing). Each loop is a deterministic, reusable Rust component that can be invoked via the orchestrator’s HTTP API or directly from other Rust modules.  

All loops share a common **geometric metadata** payload, enabling the orchestrator to reason about resource allocation, connectivity, and continuity across the entire network.

---  

## 2. Core Geometry Invariants (must be enforced by every loop)  

| Invariant | Description | Enforcement |
|---|---|---|
| **Golden‑Ratio Allocation** | Fast‑path ≈ 61.8 % of CPU & I/O, slow‑path ≈ 38.2 % | `ResourceAllocator::allocate()` checks `fast / total ∈ [0.617,0.619]` |
| **Tetrahedral Connectivity** | Every loop operation must involve all four node types (`Coordinator`, `Executor`, `Referee`, `Development`) | Validators (`TetrahedralValidator`) ensure each request touches the four partitions |
| **Möbius Continuity** | Output feeds back as input without a “seam” → hash variance < 1/Φ | `MobiusState::validate_continuity(prev, cur)` |
| **Fractal Self‑Similarity** | API contract identical at every recursion depth | `FractalProcessor::check_contract(level)` |
| **Kepler Packing** | Snapshots stored at ≥ 0.74 density | `KeplerPackingOptimizer::verify_density()` |

If any invariant fails, the loop returns `SandloopError::InvariantViolation`.

---  

## 3. Sandloop Types  

### 3.1 Prompt Request Loop (PR‑Loop)  

*Transforms high‑level intent into an optimized LLM call chain.*

| Struct | Key Fields |
|---|---|
| `PromptRequestLoop<E>` | `loop_id`, `llm_pool: Vec<LLMProvider>`, `golden_ratio_allocator`, `tetrahedral_validator`, `mobius_state` |
| `RefinementIteration` | `iteration_id`, `input_prompt`, `llm_responses`, `consensus_score`, `tetrahedral_coverage`, `mobius_continuity_hash` |

**Public API**

```rust
impl<E> PromptRequestLoop<E>
where
    E: commonware_runtime::Network + commonware_runtime::Spawner + Clone,
{
    pub async fn refine_prompt(
        &mut self,
        initial_prompt: String,
        target_step: AgentStep,
        max_iterations: u8,               // Fibonacci‑capped
    ) -> Result<String, SandloopError>;
}
```

**Geometric Checks** – executed on each iteration: golden‑ratio resource split, tetrahedral distribution of LLM calls, Möbius hash continuity.  

---

### 3.2 Data Ingestion Loop (DI‑Loop)  

*Converts raw artefacts into fractal‑structured embeddings and stores them with Kepler‑optimal packing.*

| Struct | Key Fields |
|---|---|
| `DataIngestionLoop<E>` | `loop_id`, `embedding_model`, `storage_adapter`, `fractal_processor`, `kepler_optimizer`, `meta_prompts` |
| `DataIngestionIteration` | `iteration_id`, `data_source`, `fractal_level`, `embedding_vector`, `kepler_packing_density`, `structure_analysis`, `quality_assessment` |

**Public API**

```rust
impl<E> DataIngestionLoop<E>
where
    E: commonware_runtime::Storage + commonware_runtime::Clock + Clone,
{
    pub async fn process_data_source(
        &mut self,
        data_source: DataSource,
        fractal_depth: u8,
    ) -> Result<ProcessedData, SandloopError>;
}
```

**Geometric Checks** – fractal self‑similarity at each depth, Kepler packing density ≥ 0.74048, golden‑ratio allocation of CPU between analysis (fast) and storage (slow).  

---

### 3.3 Edge‑Case Testing Loop (ET‑Loop)  

*Generates exhaustive unit/integration tests using tetrahedral‑balanced LLM prompts.*

| Struct | Key Fields |
|---|---|
| `TestingEdgeCasesLoop<E>` | `loop_id`, `code_analyzer`, `test_generators`, `golden_ratio_allocator`, `tetrahedral_coverage`, `geometric_validators` |
| `EdgeCaseIteration` | `iteration_id`, `target_module`, `discovered_edge_cases`, `test_coverage_ratio`, `tetrahedral_distribution`, `geometric_validation_results` |

**Public API**

```rust
impl<E> TestingEdgeCasesLoop<E>
where
    E: commonware_runtime::Spawner + commonware_runtime::Clock + Clone,
{
    pub async fn generate_comprehensive_tests(
        &mut self,
        code_module: CodeModule,
    ) -> Result<TestSuite, SandloopError>;
}
```

**Geometric Checks** – 61.8 % resources to exhaustive tests, 38.2 % to fuzz; each edge case is probed from the four node‑type perspectives; Möbius continuity validates that generated tests do not introduce unseen state loops.  

---

### 3.4 Random Audit / Snapshot Loop (RA‑Loop)  

*Continuously audits node state, creates fractal snapshots, and feeds results back into future audits.*

| Struct | Key Fields |
|---|---|
| `RandomAuditLoop<E>` | `loop_id`, `network_nodes`, `referee_service`, `audit_scheduler`, `snapshot_manager`, `mobius_audit_state`, `geometric_metrics` |
| `AuditIteration` | `iteration_id`, `timestamp`, `target_node`, `snapshot_data`, `audit_results`, `recontextualized_prompts`, `geometric_consistency_score`, `mobius_continuity_hash` |

**Public API**

```rust
impl<E> RandomAuditLoop<E>
where
    E: commonware_runtime::Network
        + commonware_runtime::Storage
        + commonware_runtime::Clock
        + Clone,
{
    pub async fn start_audit_loop(&mut self) -> Result<(), SandloopError>;
    pub async fn conduct_single_audit(
        &self,
        target_node: Option<NodeId>,
    ) -> Result<AuditResults, SandloopError>;
}
```

**Geometric Checks** – audit intervals follow the golden‑ratio series, node selection respects tetrahedral balance, each snapshot is a fractal hierarchy, and the Möbius hash feeds forward to bias the next audit’s target selection.  

---  

## 4. Unified API Endpoint  

All loops are reachable via the orchestrator’s REST interface (`/sandloops/*`).  

```rust
#[derive(Debug, Serialize, Deserialize)]
pub enum SandloopRequest {
    TriggerPromptRequest { initial_prompt: String, target_step: AgentStep, max_iterations: u8 },
    TriggerDataIngestion { data_source: DataSourceConfig, fractal_depth: u8 },
    TriggerEdgeCaseTesting { code_module: CodeModuleSpec },
    TriggerRandomAudit { target_node: Option<NodeId> },
}
```

**Handler (simplified)**  

```rust
pub async fn trigger_sandloop(
    State(orchestrator): State<Arc<CwHoOrchestrator<E>>>,
    Json(req): Json<SandloopRequest>,
) -> Result<Json<SandloopResponse>, SandloopError> {
    match req {
        SandloopRequest::TriggerPromptRequest { initial_prompt, target_step, max_iterations } => {
            let out = orchestrator.prompt_loop
                .refine_prompt(initial_prompt, target_step, max_iterations).await?;
            Ok(Json(SandloopResponse::PromptRefinementComplete(out)))
        }
        SandloopRequest::TriggerDataIngestion { data_source, fractal_depth } => {
            let out = orchestrator.data_ingestion_loop
                .process_data_source(data_source.into(), fractal_depth).await?;
            Ok(Json(SandloopResponse::DataIngestionComplete(out)))
        }
        SandloopRequest::TriggerEdgeCaseTesting { code_module } => {
            let out = orchestrator.testing_loop
                .generate_comprehensive_tests(code_module.into()).await?;
            Ok(Json(SandloopResponse::TestGenerationComplete(out)))
        }
        SandloopRequest::TriggerRandomAudit { target_node } => {
            let out = orchestrator.audit_loop
                .conduct_single_audit(target_node).await?;
            Ok(Json(SandloopResponse::AuditComplete(out)))
        }
    }
}
```

All responses embed the **geometric metadata** (`packing_density`, `continuity_hash`, etc.) so downstream agents can reason about compliance.

---  

## 5. Data Structures (shared)  

```rust
/// Generic geometric metadata attached to every loop output
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeometricMetadata {
    pub golden_ratio_fast: f32,   // ≈ 0.618
    pub golden_ratio_slow: f32,   // ≈ 0.382
    pub tetrahedral_coverage: TetrahedralCoverage,
    pub mobius_continuity_hash: Digest,
    pub kepler_packing_density: f32, // 0.74‑0.75 optimal
    pub fractal_depth: u8,
}
```

*All loop‑specific result structs embed `GeometricMetadata` as `metadata`.*  

---  

## 6. Testing Suite  

All loops ship with a **property‑based test harness** that verifies geometric invariants.  

```rust
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test] async fn prompt_loop_golden_ratio() { … }
    #[tokio::test] async fn data_ingestion_fractal_consistency() { … }
    #[tokio::test] async fn edge_case_tetrahedral_coverage() { … }
    #[tokio::test] async fn audit_loop_mobius_continuity() { … }
}
```

Running `cargo test --features sandloops` validates the entire subsystem.  

---  

## 7. Integration Checklist for Agents  

| Step | Action | Expected Outcome |
|---|---|---|
| 1️⃣ | Call `/sandloops/trigger_prompt_request` with a user intent. | Returns a refined prompt **and** `metadata.golden_ratio_fast ≈ 0.618`. |
| 2️⃣ | Pass raw artefacts to `/sandloops/trigger_data_ingestion`. | Receives `ProcessedData` with `metadata.kepler_packing_density ≥ 0.74`. |
| 3️⃣ | Feed a `CodeModule` to `/sandloops/trigger_edge_case_testing`. | Obtains a `TestSuite` where `metadata.tetrahedral_coverage` shows 100 % coverage of the four node types. |
| 4️⃣ | Optionally trigger `/sandloops/trigger_random_audit`. | Audit iteration includes `metadata.mobius_continuity_hash` that matches the previous iteration’s hash variance < 1/Φ. |
| 5️⃣ | Store each result in the orchestrator’s **Cnidarium** state store (via `StateDelta`). | Deterministic snapshots become part of the **fractal state tree** for later replay. |

---  

## 8. Future Extensions  

| Feature | Description | Target Release |
|---|---|---|
| **Dynamic Geometry Tuning** | Auto‑adjust golden‑ratio split based on live load metrics. | v1.1 |
| **Cross‑Chain Tetrahedral Mesh** | Extend tetrahedral connectivity to multi‑cloud clusters. | v1.2 |
| **GPU‑Accelerated Kepler Packing** | Offload packing calculations to CUDA for massive snapshots. | v1.3 |

---  

*End of Specification.*  
