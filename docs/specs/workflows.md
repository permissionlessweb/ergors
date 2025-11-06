# Workflows Specification

## Overview

The Workflows system provides **intelligent orchestration** of multi-agent tasks through **sacred geometry-guided coordination**. It implements sophisticated workflow patterns that distribute work across the tetrahedral node network while maintaining golden ratio resource optimization and fractal consistency.

## Core Workflow Types

### 1. Agent Coordination Workflows

**Purpose**: Orchestrate multiple AI agents working on complex tasks through geometric distribution patterns.

**Sacred Geometry Integration**:
- **Tetrahedral Task Distribution**: Work is distributed across all four node types 
- **Golden Ratio Load Balancing**: 61.8% allocation to primary tasks, 38.2% to validation/oversight
- **Fractal Decomposition**: Complex tasks are broken down using self-similar patterns

```rust
/// Agent coordination with geometric task distribution
#[derive(Debug, Clone)]
pub struct AgentCoordinationWorkflow<E>
where
    E: commonware_runtime::Network 
        + commonware_runtime::Spawner 
        + commonware_runtime::Storage 
        + Clone,
{
    pub workflow_id: String,
    pub coordinator_node: Arc<NetworkNode<E>>,
    pub executor_pool: Vec<Arc<NetworkNode<E>>>,
    pub referee_services: Vec<Arc<RefereeService<E>>>,
    pub development_nodes: Vec<Arc<NetworkNode<E>>>,
    pub task_distributor: TetrahedralTaskDistributor<E>,
    pub resource_balancer: GoldenRatioLoadBalancer,
    pub fractal_decomposer: FractalTaskDecomposer,
    pub workflow_state: WorkflowState,
}

#[derive(Debug, Clone)]
pub struct WorkflowTask {
    pub task_id: String,
    pub task_type: CosmicTaskType,
    pub priority: u8, // 0-255, golden threshold at 161 (0.618 * 255)
    pub complexity_level: u8, // Fractal depth indicator
    pub assigned_node_type: NodeType,
    pub resource_requirements: ResourceRequirements,
    pub dependencies: Vec<String>, // Other task IDs
    pub geometric_constraints: GeometricConstraints,
}

#[derive(Debug, Clone)]
pub enum CosmicTaskType {
    /// Primary computation tasks (golden ratio: ~61.8% of resources)
    PrimaryComputation {
        prompt: String,
        expected_output_type: OutputType,
        llm_provider: LLMProvider,
    },
    /// Validation tasks (golden ratio: ~38.2% of resources)  
    ValidationCheck {
        target_task_id: String,
        validation_criteria: Vec<ValidationCriterion>,
        referee_node: NodeId,
    },
    /// Fractal sub-task decomposition
    TaskDecomposition {
        parent_task_id: String,
        decomposition_pattern: FractalPattern,
        target_depth: u8,
    },
    /// Tetrahedral coordination
    CrossNodeCoordination {
        participating_node_types: Vec<NodeType>,
        coordination_protocol: CoordinationProtocol,
    },
}

impl<E> AgentCoordinationWorkflow<E> {
    pub async fn execute_workflow(
        &mut self,
        workflow_spec: WorkflowSpecification,
    ) -> Result<WorkflowResult, WorkflowError> {
        // Phase 1: Fractal decomposition of the workflow
        let decomposed_tasks = self.fractal_decomposer
            .decompose_workflow(workflow_spec)
            .await?;
        
        // Phase 2: Apply golden ratio resource allocation
        let resource_allocation = self.resource_balancer
            .calculate_golden_allocation(&decomposed_tasks)
            .await?;
        
        // Phase 3: Tetrahedral task distribution
        let distributed_tasks = self.task_distributor
            .distribute_across_tetrahedron(decomposed_tasks, resource_allocation)
            .await?;
        
        // Phase 4: Execute tasks with geometric constraints
        let execution_futures = self.create_execution_futures(distributed_tasks).await?;
        
        // Phase 5: Coordinate execution with Möbius feedback
        let task_results = self.execute_with_mobius_coordination(execution_futures).await?;
        
        // Phase 6: Aggregate results maintaining fractal consistency
        let final_result = self.aggregate_fractal_results(task_results).await?;
        
        // Phase 7: Validate geometric invariants
        self.validate_workflow_geometry(&final_result).await?;
        
        Ok(final_result)
    }
    
    async fn execute_with_mobius_coordination(
        &self,
        execution_futures: Vec<TaskExecutionFuture>,
    ) -> Result<Vec<TaskResult>, WorkflowError> {
        let mut results = Vec::new();
        let mut feedback_state = MobiusCoordinationState::new();
        
        for future in execution_futures {
            // Execute task
            let result = future.await?;
            
            // Apply Möbius feedback: result influences next execution
            feedback_state.incorporate_feedback(&result);
            
            // Adjust remaining executions based on feedback
            self.adjust_execution_parameters(&result, &feedback_state).await?;
            
            results.push(result);
        }
        
        Ok(results)
    }
    
    async fn create_execution_futures(
        &self,
        distributed_tasks: Vec<DistributedTask>,
    ) -> Result<Vec<TaskExecutionFuture>, WorkflowError> {
        let mut futures = Vec::new();
        
        for distributed_task in distributed_tasks {
            let future = match distributed_task.assigned_node_type {
                NodeType::Coordinator => {
                    self.execute_on_coordinator_node(distributed_task).await?
                }
                NodeType::Executor => {
                    self.execute_on_executor_node(distributed_task).await?
                }
                NodeType::Referee => {
                    self.execute_on_referee_node(distributed_task).await?
                }
                NodeType::Development => {
                    self.execute_on_development_node(distributed_task).await?
                }
            };
            
            futures.push(future);
        }
        
        Ok(futures)
    }
}
```

### 2. Multi-LLM Consensus Workflows

**Purpose**: Coordinate multiple LLM providers to reach consensus on complex decisions through geometric validation.

```rust
/// Multi-LLM consensus with geometric validation
#[derive(Debug, Clone)]
pub struct MultiLLMConsensusWorkflow<E>
where
    E: commonware_runtime::Network + commonware_runtime::Storage + Clone,
{
    pub workflow_id: String,
    pub llm_providers: HashMap<NodeType, Vec<LLMProvider>>,
    pub consensus_engine: GeometricConsensusEngine,
    pub golden_ratio_validator: GoldenRatioValidator,
    pub tetrahedral_coordinator: TetrahedralCoordinator<E>,
    pub fractal_agreement_tracker: FractalAgreementTracker,
}

#[derive(Debug, Clone)]
pub struct ConsensusQuery {
    pub query_id: String,
    pub prompt: String,
    pub expected_response_type: ResponseType,
    pub consensus_threshold: f64, // Golden ratio threshold: 0.618
    pub required_node_types: Vec<NodeType>, // Tetrahedral requirement
    pub fractal_validation_levels: u8,
    pub geometric_constraints: ConsensusConstraints,
}

impl<E> MultiLLMConsensusWorkflow<E> {
    pub async fn reach_consensus(
        &mut self,
        query: ConsensusQuery,
    ) -> Result<ConsensusResult, ConsensusError> {
        // Phase 1: Distribute query across tetrahedral topology
        let distributed_queries = self.tetrahedral_coordinator
            .distribute_consensus_query(&query)
            .await?;
        
        // Phase 2: Collect responses with golden ratio weighting
        let weighted_responses = self.collect_weighted_responses(distributed_queries).await?;
        
        // Phase 3: Apply geometric consensus algorithm
        let consensus_candidate = self.consensus_engine
            .calculate_geometric_consensus(&weighted_responses, &query)
            .await?;
        
        // Phase 4: Fractal validation across multiple scales
        let fractal_validation = self.fractal_agreement_tracker
            .validate_across_scales(&consensus_candidate, query.fractal_validation_levels)
            .await?;
        
        // Phase 5: Golden ratio confidence assessment
        let confidence_score = self.golden_ratio_validator
            .assess_consensus_confidence(&consensus_candidate, &fractal_validation)
            .await?;
        
        if confidence_score >= query.consensus_threshold {
            Ok(ConsensusResult {
                consensus_response: consensus_candidate,
                confidence_score,
                participating_nodes: self.get_participating_node_summary(),
                geometric_validation: fractal_validation,
                consensus_metadata: self.create_consensus_metadata(),
            })
        } else {
            // Recursive refinement with Möbius feedback
            let refined_query = self.refine_query_with_mobius_feedback(
                &query,
                &consensus_candidate,
                &weighted_responses,
            ).await?;
            
            // Recursive call with refined query
            self.reach_consensus(refined_query).await
        }
    }
    
    async fn collect_weighted_responses(
        &self,
        distributed_queries: Vec<DistributedConsensusQuery>,
    ) -> Result<Vec<WeightedResponse>, ConsensusError> {
        let mut weighted_responses = Vec::new();
        
        for distributed_query in distributed_queries {
            // Execute query on assigned node type
            let responses = self.execute_consensus_query_on_node_type(
                &distributed_query.query,
                distributed_query.assigned_node_type,
            ).await?;
            
            // Apply golden ratio weighting based on node type
            let weight = self.calculate_node_type_weight(distributed_query.assigned_node_type);
            
            for response in responses {
                weighted_responses.push(WeightedResponse {
                    response,
                    weight,
                    source_node_type: distributed_query.assigned_node_type,
                    geometric_metadata: self.extract_response_geometry(&response),
                });
            }
        }
        
        Ok(weighted_responses)
    }
    
    /// Calculate node type weights following golden ratio principles
    fn calculate_node_type_weight(&self, node_type: NodeType) -> f64 {
        match node_type {
            // Primary execution nodes get higher weight (golden ratio)
            NodeType::Executor => 0.618,
            NodeType::Coordinator => 0.382,
            // Validation nodes provide oversight (complementary weights)
            NodeType::Referee => 0.382,
            NodeType::Development => 0.618,
        }
    }
}
```

### 3. Recursive Task Decomposition Workflows

**Purpose**: Break down complex problems using fractal decomposition patterns that maintain self-similarity at every level.

```rust
/// Recursive decomposition with fractal self-similarity
#[derive(Debug, Clone)]
pub struct RecursiveDecompositionWorkflow<E>
where
    E: commonware_runtime::Network + commonware_runtime::Storage + Clone,
{
    pub workflow_id: String,
    pub decomposition_engine: FractalDecompositionEngine,
    pub task_executor: TetrahedralTaskExecutor<E>,
    pub golden_ratio_scheduler: GoldenRatioScheduler,
    pub mobius_integrator: MobiusIntegrator<E>,
    pub geometric_validator: GeometricValidator,
}

#[derive(Debug, Clone)]
pub struct DecompositionTask {
    pub task_id: String,
    pub description: String,
    pub complexity_score: f64,
    pub max_decomposition_depth: u8,
    pub fractal_pattern: FractalPattern,
    pub base_case_condition: BaseCaseCondition,
    pub integration_strategy: IntegrationStrategy,
}

impl<E> RecursiveDecompositionWorkflow<E> {
    pub async fn execute_recursive_decomposition(
        &mut self,
        root_task: DecompositionTask,
    ) -> Result<DecompositionResult, DecompositionError> {
        self.decompose_and_execute(root_task, 0).await
    }
    
    async fn decompose_and_execute(
        &mut self,
        task: DecompositionTask,
        current_depth: u8,
    ) -> Result<DecompositionResult, DecompositionError> {
        // Base case check
        if current_depth >= task.max_decomposition_depth 
            || self.meets_base_case(&task, current_depth).await? {
            
            return self.execute_base_case(task).await;
        }
        
        // Fractal decomposition
        let subtasks = self.decomposition_engine
            .decompose_with_fractal_pattern(&task, current_depth)
            .await?;
        
        // Golden ratio resource allocation among subtasks
        let resource_allocation = self.golden_ratio_scheduler
            .allocate_resources_fractally(&subtasks)
            .await?;
        
        // Execute subtasks across tetrahedral topology
        let mut subtask_results = Vec::new();
        
        for (subtask, allocation) in subtasks.into_iter().zip(resource_allocation) {
            // Recursive call for each subtask
            let subtask_result = self.decompose_and_execute(subtask, current_depth + 1).await?;
            subtask_results.push((subtask_result, allocation));
        }
        
        // Möbius integration: results feed back into integration strategy
        let integrated_result = self.mobius_integrator
            .integrate_with_geometric_feedback(subtask_results, &task)
            .await?;
        
        // Validate fractal consistency
        self.geometric_validator
            .validate_fractal_integration(&integrated_result, &task, current_depth)
            .await?;
        
        Ok(integrated_result)
    }
    
    async fn execute_base_case(
        &self,
        task: DecompositionTask,
    ) -> Result<DecompositionResult, DecompositionError> {
        // Select optimal node type for base case execution
        let optimal_node_type = self.select_optimal_node_for_base_case(&task).await?;
        
        // Execute with geometric constraints
        let execution_result = self.task_executor
            .execute_on_node_type(optimal_node_type, &task)
            .await?;
        
        Ok(DecompositionResult::BaseCase {
            task_id: task.task_id,
            execution_result,
            geometric_metadata: self.extract_execution_geometry(&execution_result),
        })
    }
}
```

### 4. Pipeline Workflows

**Purpose**: Execute sequential or parallel processing pipelines with geometric optimization and quality assurance.

```rust
/// Pipeline workflows with geometric processing stages
#[derive(Debug, Clone)]
pub struct PipelineWorkflow<E>
where
    E: commonware_runtime::Network + commonware_runtime::Storage + Clone,
{
    pub workflow_id: String,
    pub pipeline_stages: Vec<PipelineStage>,
    pub stage_coordinator: TetrahedralStageCoordinator<E>,
    pub golden_ratio_balancer: PipelineResourceBalancer,
    pub quality_assurance: FractalQualityAssurance<E>,
    pub mobius_pipeline_state: MobiusPipelineState,
}

#[derive(Debug, Clone)]
pub struct PipelineStage {
    pub stage_id: String,
    pub stage_name: String,
    pub stage_type: StageType,
    pub assigned_node_types: Vec<NodeType>,
    pub resource_weight: f64, // For golden ratio distribution
    pub quality_gates: Vec<QualityGate>,
    pub fractal_validation_level: u8,
}

#[derive(Debug, Clone)]
pub enum StageType {
    /// Data ingestion and preprocessing
    DataIngestion {
        input_sources: Vec<DataSource>,
        preprocessing_rules: Vec<PreprocessingRule>,
        fractal_chunking_strategy: ChunkingStrategy,
    },
    /// LLM processing stage
    LLMProcessing {
        llm_providers: Vec<LLMProvider>,
        prompt_template: String,
        consensus_requirements: ConsensusRequirements,
    },
    /// Validation and quality assurance
    Validation {
        validation_rules: Vec<ValidationRule>,
        referee_nodes: Vec<NodeId>,
        quality_thresholds: QualityThresholds,
    },
    /// Output generation and formatting
    OutputGeneration {
        output_format: OutputFormat,
        post_processing: Vec<PostProcessingStep>,
        delivery_targets: Vec<DeliveryTarget>,
    },
}

impl<E> PipelineWorkflow<E> {
    pub async fn execute_pipeline(
        &mut self,
        pipeline_input: PipelineInput,
    ) -> Result<PipelineResult, PipelineError> {
        let mut current_data = pipeline_input.data;
        let mut pipeline_state = self.mobius_pipeline_state.clone();
        
        for stage in &self.pipeline_stages {
            // Apply golden ratio resource allocation to stage
            let stage_resources = self.golden_ratio_balancer
                .allocate_stage_resources(stage, &current_data)
                .await?;
            
            // Execute stage across tetrahedral topology
            let stage_result = self.stage_coordinator
                .execute_stage(stage, current_data, stage_resources)
                .await?;
            
            // Quality assurance with fractal validation
            let qa_result = self.quality_assurance
                .validate_stage_output(stage, &stage_result)
                .await?;
            
            if !qa_result.passes_quality_gates() {
                // Möbius feedback: use stage result to improve pipeline state
                pipeline_state.incorporate_stage_feedback(stage, &stage_result);
                
                // Retry stage with improved parameters
                let improved_stage = self.improve_stage_with_feedback(
                    stage,
                    &stage_result,
                    &pipeline_state,
                ).await?;
                
                let retry_result = self.stage_coordinator
                    .execute_stage(&improved_stage, current_data, stage_resources)
                    .await?;
                
                current_data = retry_result.output_data;
            } else {
                current_data = stage_result.output_data;
            }
            
            // Update Möbius state for next iteration
            pipeline_state.update_with_successful_stage(stage, &current_data);
        }
        
        Ok(PipelineResult {
            final_data: current_data,
            pipeline_metadata: self.create_pipeline_metadata(&pipeline_state),
            geometric_validation: self.validate_pipeline_geometry(&pipeline_state),
            quality_report: self.generate_quality_report(&pipeline_state),
        })
    }
    
    async fn improve_stage_with_feedback(
        &self,
        stage: &PipelineStage,
        stage_result: &StageResult,
        pipeline_state: &MobiusPipelineState,
    ) -> Result<PipelineStage, PipelineError> {
        let mut improved_stage = stage.clone();
        
        // Apply golden ratio resource reallocation
        improved_stage.resource_weight *= 1.618; // Increase resources using golden ratio
        
        // Adjust quality gates based on fractal feedback
        for quality_gate in &mut improved_stage.quality_gates {
            self.adjust_quality_gate_with_fractal_feedback(
                quality_gate,
                stage_result,
                pipeline_state,
            ).await?;
        }
        
        // Redistribute across tetrahedral topology if needed
        if stage_result.geometric_metrics.tetrahedral_balance < 0.618 {
            improved_stage.assigned_node_types = self.rebalance_node_assignment(
                &improved_stage.assigned_node_types,
            ).await?;
        }
        
        Ok(improved_stage)
    }
}
```

## Workflow API Integration

### REST Endpoints

```rust
/// HTTP API for workflow management
#[derive(Debug, Serialize, Deserialize)]
pub enum WorkflowRequest {
    ExecuteAgentCoordination {
        specification: WorkflowSpecification,
        resource_limits: ResourceLimits,
    },
    ReachMultiLLMConsensus {
        query: ConsensusQuery,
    },
    StartRecursiveDecomposition {
        root_task: DecompositionTask,
    },
    RunPipeline {
        pipeline_config: PipelineConfiguration,
        input_data: PipelineInput,
    },
}

pub async fn handle_workflow_request(
    State(workflow_engine): State<Arc<WorkflowEngine<E>>>,
    Json(request): Json<WorkflowRequest>,
) -> Result<Json<WorkflowResponse>, WorkflowError> {
    match request {
        WorkflowRequest::ExecuteAgentCoordination { specification, resource_limits } => {
            let mut coordination_workflow = workflow_engine
                .create_agent_coordination_workflow()
                .await?;
            
            let result = coordination_workflow
                .execute_workflow(specification)
                .await?;
            
            Ok(Json(WorkflowResponse::AgentCoordinationComplete(result)))
        }
        WorkflowRequest::ReachMultiLLMConsensus { query } => {
            let mut consensus_workflow = workflow_engine
                .create_multi_llm_consensus_workflow()
                .await?;
            
            let result = consensus_workflow
                .reach_consensus(query)
                .await?;
            
            Ok(Json(WorkflowResponse::ConsensusReached(result)))
        }
        WorkflowRequest::StartRecursiveDecomposition { root_task } => {
            let mut decomposition_workflow = workflow_engine
                .create_recursive_decomposition_workflow()
                .await?;
            
            let result = decomposition_workflow
                .execute_recursive_decomposition(root_task)
                .await?;
            
            Ok(Json(WorkflowResponse::DecompositionComplete(result)))
        }
        WorkflowRequest::RunPipeline { pipeline_config, input_data } => {
            let mut pipeline_workflow = workflow_engine
                .create_pipeline_workflow(pipeline_config)
                .await?;
            
            let result = pipeline_workflow
                .execute_pipeline(input_data)
                .await?;
            
            Ok(Json(WorkflowResponse::PipelineComplete(result)))
        }
    }
}
```

## Geometric Invariants

All workflows must maintain these sacred geometry principles:

1. **Tetrahedral Distribution**: Work distributed across all four node types
2. **Golden Ratio Resource Allocation**: 61.8% primary tasks, 38.2% validation
3. **Fractal Decomposition**: Self-similar task breakdown at every level
4. **Möbius Continuity**: Results feed back to improve subsequent executions
5. **Kepler Optimization**: Resource utilization follows optimal packing principles

## Testing and Validation

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_agent_coordination_tetrahedral_distribution() {
        let mut workflow = create_test_coordination_workflow().await;
        let spec = create_test_workflow_specification();
        
        let result = workflow.execute_workflow(spec).await.unwrap();
        
        // Verify all node types participated
        let participating_node_types: HashSet<NodeType> = result
            .task_results
            .iter()
            .map(|r| r.executed_node_type)
            .collect();
            
        assert!(participating_node_types.contains(&NodeType::Coordinator));
        assert!(participating_node_types.contains(&NodeType::Executor));
        assert!(participating_node_types.contains(&NodeType::Referee));
        assert!(participating_node_types.contains(&NodeType::Development));
    }
    
    #[tokio::test]
    async fn test_multi_llm_consensus_golden_ratio() {
        let mut consensus_workflow = create_test_consensus_workflow().await;
        let query = create_test_consensus_query();
        
        let result = consensus_workflow.reach_consensus(query).await.unwrap();
        
        // Verify confidence score meets golden ratio threshold
        assert!(result.confidence_score >= 0.618);
        
        // Verify resource allocation follows golden ratio
        let resource_allocation = result.geometric_validation.resource_allocation;
        let primary_ratio = resource_allocation.primary_resources as f64 
            / resource_allocation.total_resources as f64;
        assert!((primary_ratio - 0.618).abs() < 0.05);
    }
    
    #[tokio::test]
    async fn test_recursive_decomposition_fractal_consistency() {
        let mut decomposition_workflow = create_test_decomposition_workflow().await;
        let root_task = create_test_decomposition_task();
        
        let result = decomposition_workflow
            .execute_recursive_decomposition(root_task)
            .await
            .unwrap();
        
        // Verify fractal self-similarity across all levels
        assert!(result.validates_fractal_consistency());
        
        // Verify geometric metadata is preserved at each level
        for level in 0..result.max_depth {
            assert!(result.geometric_metadata.fractal_consistency_at_level(level));
        }
    }
    
    #[tokio::test]
    async fn test_pipeline_mobius_continuity() {
        let mut pipeline_workflow = create_test_pipeline_workflow().await;
        let input_data = create_test_pipeline_input();
        
        let result = pipeline_workflow.execute_pipeline(input_data).await.unwrap();
        
        // Verify Möbius continuity across pipeline stages
        assert!(result.pipeline_metadata.mobius_continuity_maintained);
        
        // Verify no visible seams between stages
        for i in 0..result.stage_results.len() - 1 {
            let current_output_hash = result.stage_results[i].output_hash;
            let next_input_hash = result.stage_results[i + 1].input_hash;
            assert!(pipeline_workflow.validates_mobius_continuity(
                current_output_hash, 
                next_input_hash
            ));
        }
    }
}
```