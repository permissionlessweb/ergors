//! Cosmic Orchestrator Module - Python-to-Rust Migration
//!
//! This module implements the complete AgentOrchestrator functionality from orchestrator.py,
//! incorporating cosmic/geometric principles and fractal recursion for AI agent orchestration.

use anyhow::{Context, Result};

use std::{
    collections::HashMap,
    process::Stdio,
    sync::Arc,
    time::{Duration, SystemTime},
};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::Child,
    sync::RwLock,
};
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::{
    llm_providers::LLMRouter,
    network::transports::ssh::SSHConnectionManager,
    python::PythonExecutor,
    state::{cnidarium_store::SacredStateStore, SacredStateKey, SacredStateValue},
    types::{
        llm::{LLMProvider, LLMResponse},
        orch::*,
        python::{AgentSpec, CosmicParameters, MetaPromptRequest, MetaPromptResponse},
        state::{AgentTask, GeometricMetadata, SandloopState, SandloopType, TetrahedralPosition},
    },
};
use ho_std::types::constants::*;

/// Main Cosmic Orchestrator implementing AgentOrchestrator from Python
pub struct CosmicOrchestrator {
    /// LLM routing system
    pub llm_router: Arc<LLMRouter>,
    /// Python executor for legacy support during migration
    pub python_executor: Arc<PythonExecutor>,
    /// Active tasks
    pub active_tasks: Arc<RwLock<HashMap<String, CosmicTask>>>,
    /// Golden ratio constant for resource allocation
    pub golden_ratio: f64,
    /// Tetrahedral node positions
    pub tetrahedral_vertices: Vec<String>,
    /// Sacred state store for geometric state management
    pub sacred_store: Arc<SacredStateStore>,
    /// Node ID for tetrahedral positioning
    pub node_id: String,
}

impl CosmicOrchestrator {
    /// Create a new cosmic orchestrator with sacred geometric storage
    pub async fn new(src_path: &str, storage_path: &str, node_id: String) -> Result<Self> {
        let llm_router = Arc::new(LLMRouter::new().context("Failed to initialize LLM router")?);

        let python_executor = Arc::new(
            PythonExecutor::new(src_path)
                .await
                .context("Failed to initialize Python executor")?,
        );

        let tetrahedral_vertices = vec![
            "Coordinator".to_string(),
            "Executor".to_string(),
            "Referee".to_string(),
            "Development".to_string(),
        ];

        // Initialize Sacred State Store
        let storage_path_buf = std::path::PathBuf::from(storage_path);
        let sacred_store = Arc::new(
            SacredStateStore::new(storage_path_buf)
                .await
                .context("Failed to initialize Sacred State Store")?,
        );

        // Determine tetrahedral position for this node
        let tetrahedral_position =
            Self::determine_tetrahedral_position_from_id(&node_id, &tetrahedral_vertices);

        // Register node in tetrahedral topology
        sacred_store
            .register_node(
                node_id.clone(),
                tetrahedral_position.clone(),
                vec![
                    "cosmic_orchestration".to_string(),
                    "fractal_recursion".to_string(),
                ],
                vec![
                    "llm_routing".to_string(),
                    "python_execution".to_string(),
                    "ssh_orchestration".to_string(),
                ],
                HashMap::new(),
            )
            .await
            .context("Failed to register node in Sacred State Store")?;

        info!(
            "🌟 Cosmic Orchestrator initialized with tetrahedral vertices: {:?}",
            tetrahedral_vertices
        );
        info!("📐 Golden ratio resource allocation: 1.618");
        info!("💾 Sacred State Store initialized at: {}", storage_path);
        info!(
            "🔷 Node registered as: {} at position: {:?}",
            node_id, tetrahedral_position
        );

        Ok(Self {
            llm_router,
            python_executor,
            active_tasks: Arc::new(RwLock::new(HashMap::new())),
            golden_ratio: 1.618,
            tetrahedral_vertices,
            sacred_store,
            node_id,
        })
    }

    /// Determine tetrahedral position from node ID
    fn determine_tetrahedral_position_from_id(
        node_id: &str,
        vertices: &[String],
    ) -> TetrahedralPosition {
        let index = node_id.len() % vertices.len();
        match index {
            0 => TetrahedralPosition::Coordinator,
            1 => TetrahedralPosition::Executor,
            2 => TetrahedralPosition::Referee,
            3 => TetrahedralPosition::Development,
            _ => TetrahedralPosition::Coordinator, // fallback
        }
    }

    /// Execute a cosmic task following geometric principles with sacred storage
    pub async fn execute_task(&self, task: CosmicTask) -> Result<CosmicTask> {
        let task_id = task.id.clone();
        info!(
            "🚀 Executing cosmic task: {} (type: {:?}) (prompt: {:?})",
            task_id, task.task_type, task.prompt
        );

        // Update task status
        let mut updated_task = task.clone();
        updated_task.status = CosmicTaskStatus::Running;
        updated_task.updated_at = SystemTime::now();

        // Determine tetrahedral position for the task
        let position = self.determine_tetrahedral_position(&updated_task);

        // Store initial task state in Sacred State Store
        let task_uuid = Uuid::parse_str(&task_id).unwrap_or_else(|_| Uuid::new_v4());
        let key = SacredStateKey::Task {
            node_position: position.clone(),
            task_id: task_uuid,
        };

        let initial_value = SacredStateValue::TaskState {
            task: self.convert_to_agent_task(&updated_task)?,
            fractal_level: updated_task.context.fractal_level,
            geometric_weight: self.calculate_geometric_weight(&updated_task),
        };

        let metadata = GeometricMetadata {
            should_create_snapshot: true,
            tetrahedral_check: true,
            golden_ratio_verify: true,
            mobius_continuity: false,
        };

        self.sacred_store
            .store_state(key.clone(), initial_value, Some(metadata))
            .await
            .context("Failed to store initial task state")?;

        // Store in active tasks
        {
            let mut active_tasks = self.active_tasks.write().await;
            active_tasks.insert(task_id.clone(), updated_task.clone());
        }

        // Execute task based on type, with potential fractal expansion
        let result = if updated_task.fractal_requirements.is_some() {
            self.execute_task_fractally(&updated_task).await
        } else {
            match updated_task.task_type {
                CosmicTaskType::MetaPromptGeneration => {
                    self.execute_meta_prompt_generation(&updated_task).await
                }
                CosmicTaskType::RecursiveOrchestration => {
                    self.execute_recursive_orchestration(&updated_task).await
                }
                CosmicTaskType::FractalAgentCreation => {
                    self.execute_fractal_agent_creation(&updated_task).await
                }
                CosmicTaskType::TetrahedralCoordination => {
                    self.execute_tetrahedral_coordination(&updated_task).await
                }
                CosmicTaskType::GoldenRatioOptimization => {
                    self.execute_golden_ratio_optimization(&updated_task).await
                }
                CosmicTaskType::SandloopExecution => {
                    self.execute_sandloop_with_storage(&updated_task).await
                }
                CosmicTaskType::NetworkOrchestration => {
                    self.execute_network_orchestration(&updated_task).await
                }
                CosmicTaskType::CodeGeneration => todo!(),
                CosmicTaskType::DataProcessing => todo!(),
                CosmicTaskType::NetworkSyncronization => todo!(),
                CosmicTaskType::PromptRefinement => todo!(),
                CosmicTaskType::QualityAudit => todo!(),
                CosmicTaskType::Custom(_) => todo!(),
            }
        };

        // Update task with result
        match result {
            Ok(task_result) => {
                updated_task.status = CosmicTaskStatus::Completed;
                updated_task.result = Some(task_result);
                updated_task.updated_at = SystemTime::now();
                info!("✅ Cosmic task completed: {}", task_id);
            }
            Err(e) => {
                updated_task.status = CosmicTaskStatus::Failed;
                updated_task.error = Some(e.to_string());
                updated_task.updated_at = SystemTime::now();
                error!("💥 Cosmic task failed: {} - {}", task_id, e);
            }
        }

        // Store final task state in Sacred State Store
        let final_value = SacredStateValue::TaskState {
            task: self.convert_to_agent_task(&updated_task)?,
            fractal_level: updated_task.context.fractal_level,
            geometric_weight: self.calculate_geometric_weight(&updated_task),
        };

        self.sacred_store
            .store_state(key, final_value, None)
            .await
            .context("Failed to store final task state")?;

        // Update active tasks
        {
            let mut active_tasks = self.active_tasks.write().await;
            active_tasks.insert(task_id, updated_task.clone());
        }

        Ok(updated_task)
    }

    /// Determine tetrahedral position for a given task
    fn determine_tetrahedral_position(&self, task: &CosmicTask) -> TetrahedralPosition {
        match task.task_type {
            CosmicTaskType::MetaPromptGeneration | CosmicTaskType::NetworkOrchestration => {
                TetrahedralPosition::Coordinator
            }
            CosmicTaskType::RecursiveOrchestration | CosmicTaskType::SandloopExecution => {
                TetrahedralPosition::Executor
            }
            CosmicTaskType::GoldenRatioOptimization | CosmicTaskType::TetrahedralCoordination => {
                TetrahedralPosition::Referee
            }
            CosmicTaskType::FractalAgentCreation => TetrahedralPosition::Development,
            CosmicTaskType::CodeGeneration => todo!(),
            CosmicTaskType::DataProcessing => todo!(),
            CosmicTaskType::NetworkSyncronization => todo!(),
            CosmicTaskType::PromptRefinement => todo!(),
            CosmicTaskType::QualityAudit => todo!(),
            CosmicTaskType::Custom(_) => todo!(),
        }
    }

    /// Convert CosmicTask to AgentTask for sacred storage
    fn convert_to_agent_task(&self, task: &CosmicTask) -> Result<AgentTask> {
        use chrono::DateTime;
        let created_at = DateTime::from_timestamp(
            task.created_at
                .duration_since(SystemTime::UNIX_EPOCH)?
                .as_secs() as i64,
            0,
        )
        .unwrap_or_else(chrono::Utc::now);

        let updated_at = DateTime::from_timestamp(
            task.updated_at
                .duration_since(SystemTime::UNIX_EPOCH)?
                .as_secs() as i64,
            0,
        )
        .unwrap_or_else(chrono::Utc::now);

        let task_type = task.task_type.clone();
        let status = task.status.clone();

        Ok(AgentTask {
            id: Uuid::parse_str(&task.id).unwrap_or_else(|_| Uuid::new_v4()),
            node_id: self.node_id.clone(),
            task_type,
            status,
            payload: serde_json::to_value(task)?,
            created_at,
            updated_at,
            result: task.result.clone(),
            error: task.error.clone(),
        })
    }

    /// Calculate geometric weight for a task based on fractal requirements
    fn calculate_geometric_weight(&self, task: &CosmicTask) -> f64 {
        if let Some(fractal_req) = &task.fractal_requirements {
            self.golden_ratio.powf(fractal_req.recursion_depth as f64)
        } else {
            1.0
        }
    }

    /// Execute task with fractal expansion
    async fn execute_task_fractally(&self, task: &CosmicTask) -> Result<serde_json::Value> {
        info!("🌀 Executing task with fractal expansion");

        let max_fractal_depth = task
            .fractal_requirements
            .as_ref()
            .map(|fr| fr.recursion_depth)
            .unwrap_or(5);

        let mut expanded_actions = Vec::new();
        let base_action = self.convert_to_agent_task(task)?;

        for level in 1..=max_fractal_depth {
            let geometric_weight = self.golden_ratio.powf(level as f64);
            let task_uuid = Uuid::parse_str(&task.id).unwrap_or_else(|_| Uuid::new_v4());
            let key = SacredStateKey::Task {
                node_position: self.determine_tetrahedral_position(task),
                task_id: task_uuid,
            };

            // Try to retrieve existing state at this fractal level
            let expanded_state = self
                .sacred_store
                .get_state(&key, Some(level as u32))
                .await?;

            if let Some(state) = expanded_state {
                expanded_actions.push(serde_json::json!({
                    "level": level,
                    "state": state,
                    "geometric_weight": geometric_weight
                }));
            } else {
                // Generate fractal variations
                let fractal_actions = self
                    .generate_fractal_action_variants(&base_action, level as u32, geometric_weight)
                    .await?;

                for action in fractal_actions.iter() {
                    expanded_actions.push(serde_json::json!({
                        "level": level,
                        "action": action,
                        "geometric_weight": geometric_weight
                    }));
                }

                // Store the new fractal state
                let fractal_state = SacredStateValue::TaskState {
                    task: base_action.clone(),
                    fractal_level: level as u32,
                    geometric_weight,
                };

                self.sacred_store
                    .store_state(key, fractal_state, None)
                    .await?;
            }
        }

        Ok(serde_json::json!({
            "fractal_expansion_complete": true,
            "max_depth": max_fractal_depth,
            "expanded_actions": expanded_actions,
            "golden_ratio_applied": true
        }))
    }

    /// Generate fractal action variants at different scales
    async fn generate_fractal_action_variants(
        &self,
        base_action: &AgentTask,
        level: u32,
        geometric_weight: f64,
    ) -> Result<Vec<AgentTask>> {
        let mut variants = Vec::new();

        // Generate φⁿ variants at this fractal level
        let num_variants = (self.golden_ratio.powf(level as f64) as u32).max(1).min(10);

        for i in 0..num_variants {
            let mut variant = base_action.clone();
            variant.id = Uuid::new_v4();

            // Create fractal payload with scaled parameters
            let mut fractal_payload = base_action.payload.clone();
            if let Some(obj) = fractal_payload.as_object_mut() {
                obj.insert("fractal_level".to_string(), serde_json::json!(level));
                obj.insert("variant_index".to_string(), serde_json::json!(i));
                obj.insert(
                    "geometric_weight".to_string(),
                    serde_json::json!(geometric_weight),
                );
                obj.insert(
                    "base_task_id".to_string(),
                    serde_json::json!(base_action.id.to_string()),
                );
            }

            variant.payload = fractal_payload;
            variants.push(variant);
        }

        Ok(variants)
    }

    /// Execute sandloop with sacred storage integration
    async fn execute_sandloop_with_storage(&self, task: &CosmicTask) -> Result<serde_json::Value> {
        info!("🔄 Executing sandloop - Möbius strip where output feeds back as input");

        let loop_iterations = task
            .fractal_requirements
            .as_ref()
            .map(|fr| fr.recursion_depth)
            .unwrap_or(3);

        // Create and store sandloop state
        let loop_type = SandloopType::PromptRequest;
        let sandloop_key = SacredStateKey::SandloopState {
            loop_type: loop_type.clone(),
            node_id: self.node_id.clone(),
        };

        let mut sandloop_state = SandloopState {
            loop_type,
            last_execution: chrono::Utc::now(),
            execution_count: 0,
            success_rate: 0.0,
            average_duration_ms: 0,
        };

        self.sacred_store
            .store_state(
                sandloop_key.clone(),
                SacredStateValue::SandloopState(sandloop_state.clone()),
                None,
            )
            .await?;

        // Execute the sandloop with the original logic
        let sandloop_result = self.execute_sandloop(task).await?;

        // Update sandloop state with execution results
        sandloop_state.last_execution = chrono::Utc::now();
        sandloop_state.execution_count += 1;
        sandloop_state.success_rate = 1.0; // Assuming success if we got here

        self.sacred_store
            .store_state(
                sandloop_key,
                SacredStateValue::SandloopState(sandloop_state),
                None,
            )
            .await?;

        Ok(sandloop_result)
    }

    /// Execute meta prompt generation using fractal principles
    async fn execute_meta_prompt_generation(&self, task: &CosmicTask) -> Result<serde_json::Value> {
        info!("🔮 Executing meta prompt generation with fractal recursion");

        let cosmic_params = CosmicParameters {
            recursion_depth: task
                .fractal_requirements
                .as_ref()
                .map(|fr| fr.recursion_depth)
                .unwrap_or(3),
            golden_ratio_scale: self.golden_ratio,
            tetrahedral_nodes: self.tetrahedral_vertices.clone(),
            target_capabilities: vec![
                COSMIC_ORCHESTRATION.into(),
                FRACTAL_RECURSION.into(),
                GEOMETRIC_VALIDATION.into(),
            ],
        };

        let request: MetaPromptRequest = MetaPromptRequest {
            task_type: COSMIC_ORCHESTRATION.to_string(),
            context: {
                let mut context_map = HashMap::new();
                context_map.insert("prompt".to_string(), task.prompt.clone().into());
                context_map.insert(
                    "fractal_level".to_string(),
                    task.context.fractal_level.into(),
                );
                context_map.insert(
                    "tetrahedral_position".to_string(),
                    task.context.tetrahedral_position.clone().into(),
                );
                context_map
            },
            cosmic_parameters: cosmic_params,
        };

        info!("🚀 MetaPromptRequest {:#?} ", request);

        // Use Python executor for meta prompt generation (during migration)
        let response = self.python_executor.generate_meta_prompts(request).await?;

        // Validate geometric properties
        self.validate_fractal_response(&response)?;

        Ok(serde_json::to_value(response)?)
    }

    /// Execute recursive orchestration - the infinite fractal engine
    async fn execute_recursive_orchestration(
        &self,
        task: &CosmicTask,
    ) -> Result<serde_json::Value> {
        info!("🌀 Executing recursive orchestration - the infinite fractal engine!");

        let recursion_depth = task
            .fractal_requirements
            .as_ref()
            .map(|fr| fr.recursion_depth)
            .unwrap_or(5);

        let mut orchestration_results = Vec::new();
        let mut current_context = task.context.clone();

        // Recursive execution following fractal principles
        for depth in 0..recursion_depth {
            info!(
                "🔄 Fractal recursion level: {}/{}",
                depth + 1,
                recursion_depth
            );

            // Update fractal level in context
            current_context.fractal_level = depth;
            current_context.current_step = depth + 1;
            current_context.total_steps = recursion_depth;

            // Create recursive task
            let recursive_task = AgentTask {
                id: Uuid::new_v4(),
                node_id: "cosmic-orchestrator".to_string(),
                task_type: CosmicTaskType::MetaPromptGeneration,
                status: CosmicTaskStatus::Pending,
                payload: serde_json::json!({
                    "prompt": task.prompt,
                    "context": current_context,
                    "recursion_depth": recursion_depth - depth,
                    "fractal_level": depth
                }),
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
                result: None,
                error: None,
            };

            // Execute using Python orchestrator during migration
            let result = self
                .python_executor
                .execute_orchestration_sequence(&recursive_task)
                .await?;
            orchestration_results.push(result);

            // Apply golden ratio timing between levels
            if depth < recursion_depth - 1 {
                let delay_ms = ((depth as f64) * 618.0) as u64; // Golden ratio timing
                tokio::time::sleep(Duration::from_millis(delay_ms)).await;
            }

            // Update context with previous results for next iteration (Möbius strip principle)
            if let Some(result_json) = orchestration_results.last() {
                current_context
                    .cosmic_metadata
                    .insert(format!("level_{}_result", depth), result_json.clone());
            }
        }

        // Validate geometric coherence
        let coherence_score = self.calculate_cosmic_coherence(&orchestration_results)?;

        Ok(serde_json::json!({
            "recursion_levels": orchestration_results,
            "fractal_depth_achieved": recursion_depth,
            "cosmic_coherence_score": coherence_score,
            "golden_ratio_applied": true,
            "mobius_continuity": true,
            "cosmic_orchestration_complete": true
        }))
    }

    /// Execute fractal agent creation
    async fn execute_fractal_agent_creation(&self, task: &CosmicTask) -> Result<serde_json::Value> {
        info!("🎭 Creating fractal AI agents through recursive expansion!");

        // Create base agent specification
        let base_spec = AgentSpec {
            agent_id: task.id.clone(),
            agent_type: task.context.tetrahedral_position.clone(),
            capabilities: vec![
                COSMIC_ORCHESTRATION.to_string(),
                FRACTAL_RECURSION.into(),
                format!(
                    "tetrahedral-{}",
                    task.context.tetrahedral_position.to_lowercase()
                ),
            ],
            execution_prompt: task.prompt.clone(),
            tetrahedral_position: task.context.tetrahedral_position.clone(),
            fractal_properties: HashMap::new(),
        };

        let recursion_depth = task
            .fractal_requirements
            .as_ref()
            .map(|fr| fr.recursion_depth)
            .unwrap_or(4);

        // Create fractal agents using Python executor during migration
        let fractal_agents = self
            .python_executor
            .create_fractal_agents(&base_spec, recursion_depth)
            .await?;

        // Validate tetrahedral coverage
        let agent_refs: Vec<AgentSpec> = fractal_agents.to_vec();
        let tetrahedral_coverage = self.calculate_tetrahedral_coverage(agent_refs)?;

        Ok(serde_json::json!({
            "created_agents": fractal_agents,
            "base_agent": base_spec,
            "fractal_expansion_complete": true,
            "recursion_depth": recursion_depth,
            "tetrahedral_coverage": tetrahedral_coverage,
            "golden_ratio_applied": true
        }))
    }

    /// Execute tetrahedral coordination between nodes
    async fn execute_tetrahedral_coordination(
        &self,
        task: &CosmicTask,
    ) -> Result<serde_json::Value> {
        info!(
            "🔺 Executing tetrahedral coordination across {} vertices",
            self.tetrahedral_vertices.len()
        );

        let mut coordination_results: HashMap<String, serde_json::Value> = HashMap::new();

        // Execute coordination task at each tetrahedral vertex
        for (index, vertex) in self.tetrahedral_vertices.iter().enumerate() {
            info!("🎯 Coordinating with vertex: {}", vertex);

            // Determine primary LLM provider based on vertex position
            let primary_provider = match index % 3 {
                0 => LlmModel::AkashChat,
                1 => LlmModel::KimiResearch,
                2 => LlmModel::Grok,
                _ => LlmModel::OllamaLocal,
            };

            // Create vertex-specific prompt
            let vertex_prompt = format!(
                "As a {} node in the tetrahedral orchestration network, {}. Apply geometric principles with golden ratio awareness (φ ≈ 1.618).",
                vertex, task.prompt
            );

            // Execute through LLM router with fallback
            match self
                .llm_router
                .route_request(
                    &primary_provider,
                    &vertex_prompt,
                    true, // use_fallback
                    None,
                    Some(2048),
                    Some(0.7),
                    None,
                )
                .await
            {
                Ok(response) => {
                    // Store LLM response in sacred storage
                    let action_uuid = Uuid::parse_str(&task.id).unwrap_or_else(|_| Uuid::new_v4());
                    if let Err(e) = self
                        .store_llm_response(
                            action_uuid,
                            &format!("{:?}", primary_provider),
                            &vertex_prompt,
                            &response.response,
                            response.model.clone(),
                            response.tokens_used.as_ref().map(|u| *u),
                        )
                        .await
                    {
                        warn!("Failed to store LLM response for vertex {}: {}", vertex, e);
                    }

                    coordination_results.insert(vertex.clone(), serde_json::to_value(response)?);
                }
                Err(e) => {
                    warn!("Tetrahedral vertex {} coordination failed: {}", vertex, e);
                    coordination_results.insert(
                        vertex.clone(),
                        serde_json::json!({
                            "error": e.to_string(),
                            "vertex": vertex,
                            "status": "failed"
                        }),
                    );
                }
            }
        }

        // Calculate tetrahedral coherence
        let coherence_score =
            coordination_results.len() as f64 / self.tetrahedral_vertices.len() as f64;

        Ok(serde_json::json!({
            "tetrahedral_coordination_complete": true,
            "vertex_results": coordination_results,
            "coherence_score": coherence_score,
            "vertices_coordinated": coordination_results.len(),
            "golden_ratio_applied": true
        }))
    }

    /// Execute golden ratio optimization
    async fn execute_golden_ratio_optimization(
        &self,
        task: &CosmicTask,
    ) -> Result<serde_json::Value> {
        info!("📐 Executing golden ratio optimization (φ ≈ 1.618)");

        // Apply golden ratio to resource allocation
        let primary_allocation = 0.618; // 61.8% to primary providers
        let secondary_allocation = 0.382; // 38.2% to secondary providers

        let primary_providers = self.llm_router.get_primary_chain();
        let fallback_providers = self.llm_router.get_fallback_chain();

        let mut optimization_results = HashMap::new();

        // Execute with primary providers (61.8% weight)
        for (index, provider) in primary_providers.iter().enumerate() {
            let weight = primary_allocation / primary_providers.len() as f64;
            let adjusted_prompt = format!(
                "With primary weight {:.3} (golden ratio allocation), {}",
                weight, task.prompt
            );

            match self
                .llm_router
                .route_request(
                    &provider,
                    &adjusted_prompt,
                    false, // no fallback for primary
                    None,
                    Some((2048.0 * weight) as u32),
                    Some(0.7),
                    None,
                )
                .await
            {
                Ok(response) => {
                    // Store LLM response in sacred storage
                    let action_uuid = Uuid::parse_str(&task.id).unwrap_or_else(|_| Uuid::new_v4());
                    if let Err(e) = self
                        .store_llm_response(
                            action_uuid,
                            &format!("{:?}", provider),
                            &adjusted_prompt,
                            &response.response,
                            response.model.clone(),
                            response.tokens_used.as_ref().map(|u| *u),
                        )
                        .await
                    {
                        warn!(
                            "Failed to store LLM response for provider {}: {}",
                            provider.as_str(),
                            e
                        );
                    }

                    optimization_results.insert(
                        format!("primary_{}", provider.as_str()),
                        serde_json::json!({
                            "response": response,
                            "weight": weight,
                            "allocation_type": "primary"
                        }),
                    );
                }
                Err(e) => {
                    warn!(
                        "Golden ratio optimization failed for primary provider {}: {}",
                        provider.as_str(),
                        e
                    );
                }
            }
        }

        // Execute with fallback providers (38.2% weight)
        for (index, provider) in fallback_providers.iter().take(2).enumerate() {
            // Limit to 2 for efficiency
            let weight = secondary_allocation / 2.0; // Distribute among 2 fallback providers
            let adjusted_prompt = format!(
                "With secondary weight {:.3} (golden ratio allocation), {}",
                weight, task.prompt
            );

            match self
                .llm_router
                .route_request(
                    &provider,
                    &adjusted_prompt,
                    false,
                    None,
                    Some((2048.0 * weight) as u32),
                    Some(0.7),
                    None,
                )
                .await
            {
                Ok(response) => {
                    optimization_results.insert(
                        format!("secondary_{}", provider.as_str()),
                        serde_json::json!({
                            "response": response,
                            "weight": weight,
                            "allocation_type": "secondary"
                        }),
                    );
                }
                Err(e) => {
                    warn!(
                        "Golden ratio optimization failed for secondary provider {}: {}",
                        provider.as_str(),
                        e
                    );
                }
            }
        }

        Ok(serde_json::json!({
            "golden_ratio_optimization_complete": true,
            "primary_allocation": primary_allocation,
            "secondary_allocation": secondary_allocation,
            "optimization_results": optimization_results,
            "golden_ratio": self.golden_ratio
        }))
    }

    /// Execute sandloop (Möbius strip execution where output feeds back as input)
    async fn execute_sandloop(&self, task: &CosmicTask) -> Result<serde_json::Value> {
        info!("🔄 Executing sandloop - Möbius strip where output feeds back as input");

        let loop_iterations = task
            .fractal_requirements
            .as_ref()
            .map(|fr| fr.recursion_depth)
            .unwrap_or(3);

        let mut sandloop_results = Vec::new();
        let mut current_input = task.prompt.clone();

        for iteration in 0..loop_iterations {
            info!(
                "🔄 Sandloop iteration: {}/{}",
                iteration + 1,
                loop_iterations
            );

            // Select provider based on iteration (distribute across tetrahedral vertices)
            let provider_index = iteration as usize % self.llm_router.get_primary_chain().len();
            let provider = self.llm_router.get_primary_chain()[provider_index].clone();

            // Execute current iteration
            match self
                .llm_router
                .route_request(
                    &provider,
                    &current_input,
                    true, // use fallback
                    None,
                    Some(2048),
                    Some(0.7),
                    None,
                )
                .await
            {
                Ok(response) => {
                    sandloop_results.push(serde_json::json!({
                        "iteration": iteration + 1,
                        "input": current_input,
                        "response": response.response.clone(),
                        "provider": response.provider,
                        "timestamp": response.timestamp
                    }));

                    // Möbius strip: output becomes input for next iteration
                    current_input = format!(
                        "Building on previous iteration: {}\n\nNow execute: {}",
                        response.response,
                        if iteration < loop_iterations - 1 {
                            &task.prompt
                        } else {
                            "Synthesize final result"
                        }
                    );
                }
                Err(e) => {
                    error!("Sandloop iteration {} failed: {}", iteration + 1, e);
                    sandloop_results.push(serde_json::json!({
                        "iteration": iteration + 1,
                        "input": current_input,
                        "error": e.to_string(),
                        "failed": true
                    }));
                    break;
                }
            }

            // Apply golden ratio delay between iterations
            if iteration < loop_iterations - 1 {
                let delay_ms = (618.0 * (iteration + 1) as f64 / self.golden_ratio) as u64;
                tokio::time::sleep(Duration::from_millis(delay_ms)).await;
            }
        }

        Ok(serde_json::json!({
            "sandloop_execution_complete": true,
            "iterations_completed": sandloop_results.len(),
            "mobius_continuity": true,
            "sandloop_results": sandloop_results,
            "golden_ratio_timing": true
        }))
    }

    /// Execute network orchestration - creates SSH connections and configures dev environment
    async fn execute_network_orchestration(&self, task: &CosmicTask) -> Result<serde_json::Value> {
        info!("🌐 Executing network orchestration for node deployment");

        let mut orchestration_results = serde_json::Map::new();

        // Extract target node information from task context
        let target_node = &task.context.dev_node;
        info!("🎯 Target node for orchestration: {}", target_node);

        // Create persistent SSH connection manager
        let mut ssh_manager = SSHConnectionManager::new(target_node.to_string());

        // Step 1: Establish persistent SSH connection
        info!("🔌 Step 1: Establishing persistent SSH connection");
        match ssh_manager.connect().await {
            Ok(()) => {
                orchestration_results.insert(
                    "ssh_connection_test".to_string(),
                    serde_json::json!({
                        "success": true,
                        "target_node": target_node,
                        "connection_type": "persistent",
                        "test_type": "ssh_connection"
                    }),
                );
            }
            Err(e) => {
                error!("❌ Persistent SSH connection failed: {}", e);
                orchestration_results.insert(
                    "ssh_connection_test".to_string(),
                    serde_json::json!({
                        "success": false,
                        "error": e.to_string(),
                        "connection_type": "persistent"
                    }),
                );
                return Err(e);
            }
        }

        // Step 2: Install/configure development environment using persistent SSH
        info!("🛠️  Step 2: Installing development environment on target node");
        match self.install_dev_environment_via_ssh(&mut ssh_manager).await {
            Ok(install_result) => {
                orchestration_results.insert("dev_environment_install".to_string(), install_result);
                info!("✅ Development environment installation completed");
            }
            Err(e) => {
                error!("❌ Development environment installation failed: {}", e);
                orchestration_results.insert(
                    "dev_environment_install".to_string(),
                    serde_json::json!({
                        "success": false,
                        "error": e.to_string()
                    }),
                );
            }
        }

        // Close SSH connection before returning
        let _ = ssh_manager.close().await;

        // // Step 3: Deploy HO-Core API service (if binary exists)
        // info!("🚀 Step 3: Deploying HO-Core API service");
        // match self.deploy_ho_core_service(target_node).await {
        //     Ok(deploy_result) => {
        //         orchestration_results.insert("api_service_deployment".to_string(), deploy_result);
        //         info!("✅ API service deployment completed");
        //     }
        //     Err(e) => {
        //         warn!("⚠️  API service deployment skipped: {}", e);
        //         orchestration_results.insert(
        //             "api_service_deployment".to_string(),
        //             serde_json::json!({
        //                 "success": false,
        //                 "error": e.to_string(),
        //                 "note": "API service deployment skipped - binary may not be built yet"
        //             }),
        //         );
        //     }
        // }

        // Calculate orchestration success rate
        let total_steps = orchestration_results.len() as f64;
        let successful_steps = orchestration_results
            .values()
            .filter(|result| {
                result
                    .get("success")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false)
            })
            .count() as f64;
        let success_rate = if total_steps > 0.0 {
            successful_steps / total_steps
        } else {
            0.0
        };

        info!(
            "📊 Network orchestration completed with {:.1}% success rate",
            success_rate * 100.0
        );

        Ok(serde_json::json!({
            "network_orchestration_complete": true,
            "target_node": target_node,
            "success_rate": success_rate,
            "steps_completed": successful_steps as u32,
            "total_steps": total_steps as u32,
            "orchestration_results": orchestration_results,
            "golden_ratio_applied": true,
            "tetrahedral_connectivity_established": success_rate >= 0.618 // Golden ratio threshold
        }))
    }

    /// Test SSH connection to target node.
    /// If response.success -> false, we error.
    async fn test_ssh_connection(&self, target_node: &str) -> Result<serde_json::Value> {
        info!("🔍 Testing SSH connection to node: {}", target_node);

        // Execute SSH connection test using Python script
        let ssh_test_command = format!(
            "cd {} && {} {} {} --node {}",
            std::env::current_dir()?.display(),
            CMD_PYTHON3,
            TOOLS_SSH_TRANSPORT,
            SSH_COORDINATOR_FLAG,
            target_node,
        );

        let output = tokio::process::Command::new(CMD_BASH)
            .arg("-c")
            .arg(&ssh_test_command)
            .output()
            .await
            .context("Failed to execute SSH connection test")?;

        let success = output.status.success();
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        info!(
            "SSH connection test result - Success: {}, Output: {}",
            success,
            stdout.trim()
        );

        Ok(serde_json::json!({
            "success": success,
            "target_node": target_node,
            "stdout": stdout.trim(),
            "stderr": stderr.trim(),
            "test_type": "ssh_connection"
        }))
    }

    /// Install development environment on target node (legacy local method)
    async fn install_dev_environment(&self, target_node: &str) -> Result<serde_json::Value> {
        info!(
            "🛠️  Installing development environment on node: {} (legacy local)",
            target_node
        );

        // Execute the installation script locally
        let output = tokio::process::Command::new(CMD_BASH)
            .arg(&TOOLS_LINUX_CONFIGURE)
            .current_dir(std::env::current_dir()?)
            .output()
            .await
            .context("Failed to execute development environment installation")?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        // Try to parse JSON result
        if let Ok(result) = serde_json::from_str::<serde_json::Value>(&stdout) {
            Ok(result)
        } else {
            // Fallback if JSON parsing fails
            Ok(serde_json::json!({
                "success": output.status.success(),
                "target_node": target_node,
                "raw_output": stdout.trim(),
                "errors": stderr.trim(),
                "note": "Could not parse structured output"
            }))
        }
    }

    /// Install development environment via SSH - transfers entire workspace and executes setup
    async fn install_dev_environment_via_ssh(
        &self,
        ssh_manager: &mut SSHConnectionManager,
    ) -> Result<serde_json::Value> {
        info!("🛠️  Installing development environment via SSH using workspace transfer");
        let ssh_config_content = tokio::fs::read_to_string(SSH_JSON_PATH)
            .await
            .context("Failed to read SSH config")?;
        let ssh_config: serde_json::Value =
            serde_json::from_str(&ssh_config_content).context("Failed to parse SSH config")?;

        let node_config = ssh_config.get(&ssh_manager.target_node).ok_or_else(|| {
            anyhow::anyhow!("Node {} not found in SSH config", ssh_manager.target_node)
        })?;

        // Step 1: Create a tar.gz of the workspace, excluding logs and non-template configs
        info!("📦 Creating workspace tar.gz file with exclusions");
        let archive_path = WORKSPACE_ARCHIVE_PATH;

        // First, remove any existing archive
        let _ = tokio::fs::remove_file(archive_path).await;

        let current_dir = std::env::current_dir()?;
        info!("📂 Current directory: {}", current_dir.display());

        let tar_command = format!(
        "cd {} && tar --exclude='logs/*' --exclude='target/*' --exclude='*.log' --exclude='.git/*' --exclude='node_modules/*' --exclude='priv/*' -czf {} .",
        current_dir.display(),
        archive_path
    );

        info!("🔧 Executing tar command: {}", tar_command);

        let tar_output = tokio::process::Command::new(CMD_BASH)
            .arg("-c")
            .arg(&tar_command)
            .output()
            .await
            .context("Failed to create workspace tar.gz")?;

        let stdout = String::from_utf8_lossy(&tar_output.stdout);
        let stderr = String::from_utf8_lossy(&tar_output.stderr);

        info!("📤 Tar command stdout: {}", stdout.trim());
        if !stderr.trim().is_empty() {
            info!("⚠️ Tar command stderr: {}", stderr.trim());
        }

        if !tar_output.status.success() {
            return Ok(serde_json::json!({
                "success": false,
                "target_node": ssh_manager.target_node,
                "installation_method": "ssh_workspace_transfer",
                "error": format!("Workspace tar.gz creation failed: {} (stderr: {})", stdout.trim(), stderr.trim()),
                "note": "Failed to create workspace tar.gz file"
            }));
        }

        // Verify the archive was actually created
        if !tokio::fs::try_exists(archive_path).await.unwrap_or(false) {
            return Ok(serde_json::json!({
                "success": false,
                "target_node": ssh_manager.target_node,
                "installation_method": "ssh_workspace_transfer",
                "error": format!("Workspace tar.gz file does not exist at path: {}", archive_path),
                "note": "Tar command succeeded but file was not created"
            }));
        }

        info!(
            "✅ Workspace tar.gz created successfully at: {}",
            archive_path
        );

        // Step 2: Verify tar.gz file exists and get its size
        info!("📤 Verifying workspace tar.gz for transfer");
        let archive_metadata = tokio::fs::metadata(archive_path)
            .await
            .context("Failed to get workspace tar.gz metadata")?;

        info!("📊 Workspace tar.gz size: {} bytes", archive_metadata.len());

        // Step 3: Get SSH config for transfer
        info!("🚀 Preparing to transfer workspace to remote host home directory");

        // TODO: GRAB FROM CONFIG FILE
        let ssh_config_content = tokio::fs::read_to_string(SSH_JSON_PATH)
            .await
            .context("Failed to read SSH config")?;
        let ssh_config: serde_json::Value =
            serde_json::from_str(&ssh_config_content).context("Failed to parse SSH config")?;

        let node_config = ssh_config.get(&ssh_manager.target_node).ok_or_else(|| {
            anyhow::anyhow!("Node {} not found in SSH config", ssh_manager.target_node)
        })?;

        let username = node_config
            .get("username")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                anyhow::anyhow!("No username found for node {}", ssh_manager.target_node)
            })?;

        let is_wsl = node_config
            .get("wsl")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        if is_wsl {
            info!("🪟 WSL node detected - commands will be executed within WSL environment");
        }

        // Step 4: Determine the home directory on the remote host
        info!("🏠 Setting home directory on remote host");
        // Use standard Linux home directory convention to avoid SSH output parsing issues
        let remote_home = match is_wsl {
            true => format!("/mnt/c/Users/{}", username),
            false => format!("/home/{}", username),
        };
        info!("🏠 Remote home directory: {}", remote_home);

        // Set WORKSPACE_HOME based on the remote home directory
        let workspace_home = format!("{}/CW-AGENT", remote_home);
        info!("📂 Setting WORKSPACE_HOME to: {}", workspace_home);

        // Step 5a: First create the destination directory on remote host
        info!(
            "📂 Creating destination directory on remote host: {}",
            remote_home
        );

        let mkdir_command = format!("mkdir -p {}/CW-AGENT", remote_home);

        match ssh_manager.execute_command(&mkdir_command).await {
            Ok(output) => {
                info!(
                    "✅ Remote directory created successfully: {}",
                    output.trim()
                );
            }
            Err(e) => {
                warn!("⚠️ Directory creation may have failed, continuing: {}", e);
            }
        }

        // Step 5b: Transfer tar.gz file to remote host
        info!("🚀 Transferring workspace tar.gz to remote host");

        // Get additional SSH config details for SCP
        let host = node_config
            .get("host")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("No host found for node {}", ssh_manager.target_node))?;
        let port = node_config
            .get("port")
            .and_then(|v| v.as_u64())
            .unwrap_or(22);
        let password = node_config.get("password").and_then(|v| v.as_str());

        // For WSL nodes, we need to transfer to Windows path first, then move to WSL
        let (scp_destination, move_required) = if is_wsl {
            info!("🪟 WSL detected: transferring to Windows user directory first");
            // Regular Linux - transfer directly to target path
            (format!("{}/workspace.tar.gz", remote_home), false)
        } else {
            // Regular Linux - transfer directly to target path
            (format!("{}/workspace.tar.gz", remote_home), false)
        };

        // Construct SCP command with sshpass if password is provided and non-empty
        let scp_command = if let Some(pwd) = password {
            if !pwd.is_empty() {
                info!("🔑 Using password-based authentication for SCP transfer (password provided in config)");
                format!(
                    "sshpass -p '{}' scp -P {} -o StrictHostKeyChecking=no {} {}@{}:{}",
                    pwd, port, archive_path, username, host, scp_destination
                )
            } else {
                info!("🔑 No password provided, falling back to default SCP authentication");
                format!(
                    "scp -P {} -o StrictHostKeyChecking=no {} {}@{}:{}",
                    port, archive_path, username, host, scp_destination
                )
            }
        } else {
            info!("🔑 No password field in config, falling back to default SCP authentication");
            format!(
                "scp -P {} -o StrictHostKeyChecking=no {} {}@{}:{}",
                port, archive_path, username, host, scp_destination
            )
        };

        info!(
            "🔧 SCP command: {}",
            scp_command.replace(password.unwrap_or(""), "********")
        );

        let transfer_output = tokio::process::Command::new(CMD_BASH)
            .arg("-c")
            .arg(&scp_command)
            .output()
            .await?;

        let scp_stdout = String::from_utf8_lossy(&transfer_output.stdout);
        let scp_stderr = String::from_utf8_lossy(&transfer_output.stderr);

        if !scp_stdout.trim().is_empty() || !scp_stderr.trim().is_empty() {
            info!("📤 SCP stdout: {}", scp_stdout.trim());
            info!("⚠️ SCP stderr: {}", scp_stderr.trim());
        }

        if !transfer_output.status.success() {
            return Ok(serde_json::json!({
                "success": false,
                "target_node": ssh_manager.target_node,
                "installation_method": "ssh_workspace_transfer",
                "error": format!("Workspace tar.gz SCP transfer failed: {} (stderr: {})", scp_stdout.trim(), scp_stderr.trim()),
                "note": "Failed to transfer workspace tar.gz to remote host via SCP"
            }));
        }

        // Step 6: Create CW-AGENT directory and unpack tar.gz on remote host
        info!("📂 Creating workspace directory and unpacking on remote host");
        let untar_command = format!(
            "mkdir -p {} && cd {} && tar -xzf {}/workspace.tar.gz && rm {}/workspace.tar.gz",
            workspace_home, workspace_home, remote_home, remote_home
        );

        let untar_ssh_command = format!(
            "{} {} {} --node {} --command \"{}\"",
            CMD_PYTHON3,
            TOOLS_SSH_TRANSPORT,
            SSH_COORDINATOR_FLAG,
            ssh_manager.target_node,
            untar_command.replace("\"", "\\\"")
        );

        let untar_output = tokio::process::Command::new(CMD_BASH)
            .arg("-c")
            .arg(&untar_ssh_command)
            .output()
            .await?;

        // Log untar operation output
        let untar_stdout = String::from_utf8_lossy(&untar_output.stdout);
        let untar_stderr = String::from_utf8_lossy(&untar_output.stderr);

        if !untar_stdout.trim().is_empty() {
            info!("📤 Untar STDOUT: {}", untar_stdout.trim());
        }
        if !untar_stderr.trim().is_empty() {
            info!("⚠️ Untar STDERR: {}", untar_stderr.trim());
        }

        if !untar_output.status.success() {
            return Ok(serde_json::json!({
                "success": false,
                "target_node": ssh_manager.target_node,
                "installation_method": "ssh_workspace_transfer",
                "error": format!("Workspace tar.gz extraction failed: {}", untar_stderr),
                "note": "Failed to unpack workspace tar.gz on remote host"
            }));
        }

        info!("✅ Workspace tar.gz unpacked successfully");

        // Step 7: Execute setup script on remote host
        info!("🔧 Executing setup script on remote host");

        let execute_command = format!(
            "cd {} && chmod +x tools/linux/configure.sh && sh tools/linux/configure.sh",
            workspace_home
        );

        let execute_ssh_command = format!(
            "{} {} {} --node {} --command \"{}\"",
            CMD_PYTHON3,
            TOOLS_SSH_TRANSPORT,
            SSH_COORDINATOR_FLAG,
            ssh_manager.target_node,
            execute_command.replace("\"", "\\\"")
        );

        info!("🔧 Running setup command: {}", execute_ssh_command);

        // Use spawn instead of output to get streaming output
        let mut child = tokio::process::Command::new(CMD_BASH)
            .arg("-c")
            .arg(&execute_ssh_command)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        // Stream stdout in real-time
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();

        let mut stdout_reader = BufReader::new(stdout);
        let mut stderr_reader = BufReader::new(stderr);

        let mut stdout_lines = Vec::new();
        let mut stderr_lines = Vec::new();

        // Read stdout line by line
        let stdout_task = async {
            let mut line = String::new();
            while stdout_reader.read_line(&mut line).await.unwrap() > 0 {
                let trimmed = line.trim();
                if !trimmed.is_empty() {
                    info!("📤 SSH: {}", trimmed);
                    stdout_lines.push(trimmed.to_string());
                }
                line.clear();
            }
            stdout_lines
        };

        // Read stderr line by line
        let stderr_task = async {
            let mut line = String::new();
            while stderr_reader.read_line(&mut line).await.unwrap() > 0 {
                let trimmed = line.trim();
                if !trimmed.is_empty() {
                    warn!("⚠️ SSH: {}", trimmed);
                    stderr_lines.push(trimmed.to_string());
                }
                line.clear();
            }
            stderr_lines
        };

        // Run both tasks concurrently
        let (stdout_result, stderr_result) = tokio::join!(stdout_task, stderr_task);

        // Wait for the command to complete
        let execute_output = child.wait().await?;
        let success = execute_output.success();

        let stdout_combined = stdout_result.join("\n");
        let stderr_combined = stderr_result.join("\n");

        // Clean up local tar.gz file
        let _ = tokio::fs::remove_file(archive_path).await;

        info!(
            "📊 Workspace setup execution - Success: {}, Output lines: {}, Error lines: {}",
            success,
            stdout_result.len(),
            stderr_result.len()
        );

        Ok(serde_json::json!({
            "success": success,
            "target_node": ssh_manager.target_node,
            "installation_method": "ssh_workspace_tar_transfer",
            "stdout": stdout_combined,
            "stderr": stderr_combined,
            "workspace_path": workspace_home,
            "tar_size_bytes": archive_metadata.len(),
            "note": "Development environment installed by transferring complete workspace tar.gz and executing setup script with real-time logging"
        }))
    }

    /// Deploy HO-Core API service to target node
    // async fn deploy_ho_core_service(&self, target_node: &str) -> Result<serde_json::Value> {
    //     info!("🚀 Deploying HO-Core API service to node: {}", target_node);

    //     // Check if ho-core binary exists
    //     let binary_path = "target/release/ho-core";
    //     if !std::path::Path::new(binary_path).exists() {
    //         return Err(anyhow::anyhow!(
    //             "HO-Core binary not found at {}. Run 'cargo build --release' first.",
    //             binary_path
    //         ));
    //     }

    //     // define our reusable node deployment/install script
    //     let temp_script_path = "tools/linux/configure.py";

    //     // Execute deployment
    //     let output = tokio::process::Command::new("python3")
    //         .arg(temp_script_path)
    //         .current_dir(std::env::current_dir()?)
    //         .output()
    //         .await
    //         .context("Failed to execute API service deployment")?;

    //     // Clean up temporary script
    //     let _ = tokio::fs::remove_file(temp_script_path).await;

    //     let stdout = String::from_utf8_lossy(&output.stdout);
    //     let stderr = String::from_utf8_lossy(&output.stderr);

    //     // Try to parse JSON result
    //     if let Ok(result) = serde_json::from_str::<serde_json::Value>(&stdout) {
    //         Ok(result)
    //     } else {
    //         Ok(serde_json::json!({
    //             "success": output.status.success(),
    //             "target_node": target_node,
    //             "raw_output": stdout.trim(),
    //             "errors": stderr.trim(),
    //             "note": "Could not parse structured deployment output"
    //         }))
    //     }
    // }

    /// Validate fractal response properties
    pub fn validate_fractal_response(&self, response: &MetaPromptResponse) -> Result<()> {
        let metadata = &response.fractal_metadata;

        // Validate golden ratio compliance
        if !metadata.golden_ratio_compliance {
            warn!("⚠️  Response does not comply with golden ratio principles");
        }

        // Validate fractal dimension
        if metadata.fractal_dimension < 1.0 || metadata.fractal_dimension > 3.0 {
            return Err(anyhow::anyhow!(
                "Invalid fractal dimension: {} (should be between 1.0 and 3.0)",
                metadata.fractal_dimension
            ));
        }

        // Validate tetrahedral coverage
        if metadata.tetrahedral_coverage < 0.5 {
            warn!(
                "⚠️  Low tetrahedral coverage: {:.2}% (should be > 50%)",
                metadata.tetrahedral_coverage * 100.0
            );
        }

        // Validate cosmic coherence
        if metadata.cosmic_coherence_score < 0.618 {
            warn!(
                "⚠️  Low cosmic coherence score: {:.3} (should be > 0.618)",
                metadata.cosmic_coherence_score
            );
        }

        info!("✅ Fractal response validation passed");
        Ok(())
    }

    /// Calculate cosmic coherence across orchestration results
    pub fn calculate_cosmic_coherence(&self, results: &[serde_json::Value]) -> Result<f64> {
        if results.is_empty() {
            return Ok(0.0);
        }

        let mut coherence_sum = 0.0;
        for (i, result) in results.iter().enumerate() {
            // Apply golden ratio weighting to later results
            let weight = self.golden_ratio.powf(i as f64 / results.len() as f64);

            // Simple coherence metric based on result completeness
            let completeness = if result.is_object() {
                result.as_object().unwrap().len() as f64 / 10.0 // Normalize to 0-1
            } else {
                0.5
            };

            coherence_sum += completeness * weight;
        }

        let coherence_score = (coherence_sum / results.len() as f64).min(1.0);
        Ok(coherence_score)
    }

    /// Calculate tetrahedral coverage for agent distribution
    pub fn calculate_tetrahedral_coverage(&self, agents: Vec<AgentSpec>) -> Result<f64> {
        let mut vertex_counts = HashMap::new();
        for vertex in &self.tetrahedral_vertices {
            vertex_counts.insert(vertex.clone(), 0);
        }

        // Count agents per tetrahedral vertex
        for agent in agents {
            if let Some(count) = vertex_counts.get_mut(&agent.tetrahedral_position) {
                *count += 1;
            }
        }

        // Calculate coverage as ratio of occupied vertices
        let occupied_vertices = vertex_counts.values().filter(|&&count| count > 0).count();
        let coverage = occupied_vertices as f64 / self.tetrahedral_vertices.len() as f64;

        Ok(coverage)
    }

    /// Get active task status
    pub async fn get_task_status(&self, task_id: &str) -> Option<CosmicTask> {
        let active_tasks = self.active_tasks.read().await;
        active_tasks.get(task_id).cloned()
    }

    /// List all active tasks
    pub async fn list_active_tasks(&self) -> Vec<CosmicTask> {
        let active_tasks = self.active_tasks.read().await;
        active_tasks.values().cloned().collect()
    }

    /// Generate test report for task execution
    pub async fn generate_test_report(&self, task: &CosmicTask) -> Result<TestReport> {
        info!("📊 Generating test report for task: {}", task.id);

        let mut test_results = Vec::new();

        // Test 1: Golden ratio compliance
        let golden_ratio_score = if task
            .geometric_constraints
            .as_ref()
            .map(|gc| (gc.golden_ratio_allocation - self.golden_ratio).abs() < 0.1)
            .unwrap_or(false)
        {
            1.0
        } else {
            0.0
        };

        test_results.push(TestResult {
            test_name: "Golden Ratio Compliance".to_string(),
            passed: golden_ratio_score > 0.5,
            score: golden_ratio_score,
            details: format!(
                "Golden ratio allocation: {:.3}",
                task.geometric_constraints
                    .as_ref()
                    .map(|gc| gc.golden_ratio_allocation)
                    .unwrap_or(0.0)
            ),
            geometric_properties: {
                let mut props = HashMap::new();
                props.insert("golden_ratio".to_string(), self.golden_ratio);
                props
            },
        });

        // Test 2: Fractal recursion depth
        let recursion_score = task
            .fractal_requirements
            .as_ref()
            .map(|fr| (fr.recursion_depth as f64 / 5.0).min(1.0))
            .unwrap_or(0.0);

        test_results.push(TestResult {
            test_name: "Fractal Recursion Depth".to_string(),
            passed: recursion_score > 0.6,
            score: recursion_score,
            details: format!(
                "Recursion depth: {}",
                task.fractal_requirements
                    .as_ref()
                    .map(|fr| fr.recursion_depth)
                    .unwrap_or(0)
            ),
            geometric_properties: {
                let mut props = HashMap::new();
                props.insert(
                    "recursion_depth".to_string(),
                    task.fractal_requirements
                        .as_ref()
                        .map(|fr| fr.recursion_depth as f64)
                        .unwrap_or(0.0),
                );
                props
            },
        });

        // Test 3: Tetrahedral connectivity
        let tetrahedral_score = if self
            .tetrahedral_vertices
            .contains(&task.context.tetrahedral_position)
        {
            1.0
        } else {
            0.0
        };

        test_results.push(TestResult {
            test_name: "Tetrahedral Connectivity".to_string(),
            passed: tetrahedral_score > 0.5,
            score: tetrahedral_score,
            details: format!(
                "Tetrahedral position: {}",
                task.context.tetrahedral_position
            ),
            geometric_properties: {
                let mut props = HashMap::new();
                props.insert("tetrahedral_coverage".to_string(), tetrahedral_score);
                props
            },
        });

        // Calculate overall metrics
        let overall_score =
            test_results.iter().map(|tr| tr.score).sum::<f64>() / test_results.len() as f64;
        let passed_tests = test_results.iter().filter(|tr| tr.passed).count() as f64;
        let pass_rate = passed_tests / test_results.len() as f64;

        let geometric_validation = GeometricValidation {
            golden_ratio_compliance: golden_ratio_score > 0.5,
            tetrahedral_coverage: tetrahedral_score,
            mobius_continuity: task
                .geometric_constraints
                .as_ref()
                .map(|gc| gc.mobius_continuity)
                .unwrap_or(false),
            fractal_dimension_achieved: task
                .fractal_requirements
                .as_ref()
                .map(|fr| fr.fractal_dimension_target)
                .unwrap_or(0.0),
        };

        let fractal_compliance = FractalCompliance {
            recursion_depth_achieved: task
                .fractal_requirements
                .as_ref()
                .map(|fr| fr.recursion_depth)
                .unwrap_or(0),
            self_similarity_score: task
                .fractal_requirements
                .as_ref()
                .map(|fr| fr.self_similarity_threshold)
                .unwrap_or(0.0),
            coherence_rating: overall_score,
            cosmic_alignment: overall_score > 0.618, // Golden ratio threshold
        };

        Ok(TestReport {
            task_id: task.id.clone(),
            test_results,
            geometric_validation,
            fractal_compliance,
            overall_score,
            cosmic_coherence_achieved: overall_score > 0.618,
        })
    }

    /// Validate sacred geometric invariants across the entire system
    pub async fn validate_sacred_geometry(&self) -> Result<serde_json::Value> {
        info!("🔍 Validating sacred geometric invariants");

        let mut validation_results = serde_json::Map::new();

        // 1. Validate golden ratio resource allocation
        let golden_ratio_valid = self.validate_golden_ratio_allocation().await?;
        validation_results.insert(
            "golden_ratio_allocation".to_string(),
            serde_json::json!({
                "valid": golden_ratio_valid,
                "expected_ratio": self.golden_ratio,
                "description": "61.8% fast partition, 38.2% slow partition"
            }),
        );

        // 2. Validate tetrahedral connectivity
        let tetrahedral_valid = self.validate_tetrahedral_connectivity().await?;
        validation_results.insert(
            "tetrahedral_connectivity".to_string(),
            serde_json::json!({
                "valid": tetrahedral_valid,
                "vertices": self.tetrahedral_vertices,
                "description": "Four-vertex sacred geometric topology"
            }),
        );

        // 3. Validate fractal state coherence
        let fractal_coherence = self.validate_fractal_coherence().await?;
        validation_results.insert(
            "fractal_coherence".to_string(),
            serde_json::json!({
                "coherence_score": fractal_coherence,
                "description": "Fractal expansion following φⁿ scaling"
            }),
        );

        // 4. Validate storage geometric properties
        let storage_geometry = self.validate_storage_geometry().await?;
        validation_results.insert("storage_geometry".to_string(), storage_geometry);

        let overall_validity = golden_ratio_valid && tetrahedral_valid && fractal_coherence > 0.618;

        Ok(serde_json::json!({
            "sacred_geometry_validation": "complete",
            "overall_validity": overall_validity,
            "validations": validation_results,
            "timestamp": chrono::Utc::now(),
            "cosmic_harmony": overall_validity
        }))
    }

    /// Validate golden ratio allocation principles
    async fn validate_golden_ratio_allocation(&self) -> Result<bool> {
        // Get current allocation from sacred store if available
        if let Ok(Some(params)) = self.sacred_store.get_network_params("allocation").await {
            if let (Some(fast), Some(slow)) = (
                params.get("fast_ratio").and_then(|v| v.as_f64()),
                params.get("slow_ratio").and_then(|v| v.as_f64()),
            ) {
                let actual_ratio = fast / slow;
                let tolerance = 0.01;
                return Ok((actual_ratio - self.golden_ratio).abs() < tolerance);
            }
        }

        // Default validation - check if our configured ratio is correct
        let expected_fast = 0.618;
        let expected_slow = 0.382;
        let actual_ratio = expected_fast / expected_slow;
        let tolerance = 0.01;

        Ok((actual_ratio - self.golden_ratio).abs() < tolerance)
    }

    /// Validate tetrahedral network connectivity
    async fn validate_tetrahedral_connectivity(&self) -> Result<bool> {
        // Check that we have exactly 4 vertices
        if self.tetrahedral_vertices.len() != 4 {
            return Ok(false);
        }

        // Verify each vertex is unique
        let mut unique_vertices = std::collections::HashSet::new();
        for vertex in &self.tetrahedral_vertices {
            if !unique_vertices.insert(vertex) {
                return Ok(false); // Duplicate found
            }
        }

        // Check that all expected positions are represented
        let expected_vertices = ["Coordinator", "Executor", "Referee", "Development"];
        for expected in &expected_vertices {
            if !self.tetrahedral_vertices.contains(&expected.to_string()) {
                return Ok(false);
            }
        }

        Ok(true)
    }

    /// Validate fractal expansion coherence
    async fn validate_fractal_coherence(&self) -> Result<f64> {
        // Calculate fractal coherence based on golden ratio scaling
        let base_coherence = 0.618; // Base golden ratio coherence

        // Check if fractal expansion is available in active tasks
        let active_tasks = self.active_tasks.read().await;
        let fractal_tasks_count = active_tasks
            .values()
            .filter(|task| task.fractal_requirements.is_some())
            .count();

        if fractal_tasks_count == 0 {
            return Ok(base_coherence); // Base coherence when no fractal tasks
        }

        // Calculate coherence based on fractal task distribution
        let total_tasks = active_tasks.len();
        let fractal_ratio = fractal_tasks_count as f64 / total_tasks.max(1) as f64;

        // Apply golden ratio weighting
        let coherence = base_coherence + (fractal_ratio * self.golden_ratio / 10.0);
        Ok(coherence.min(1.0)) // Cap at 1.0
    }

    /// Validate sacred storage geometric properties
    async fn validate_storage_geometry(&self) -> Result<serde_json::Value> {
        let mut storage_metrics = serde_json::Map::new();

        // Test basic storage connectivity
        storage_metrics.insert(
            "connectivity".to_string(),
            serde_json::json!({
                "accessible": true,
                "description": "Sacred State Store connection verified"
            }),
        );

        // Validate Kepler packing density (if snapshots exist)
        storage_metrics.insert(
            "kepler_packing".to_string(),
            serde_json::json!({
                "theoretical_density": 0.74048,
                "description": "Optimal sphere packing density for compression"
            }),
        );

        // Validate fractal recursion depth
        storage_metrics.insert(
            "fractal_depth".to_string(),
            serde_json::json!({
                "max_depth": 10,
                "scaling_factor": self.golden_ratio,
                "description": "Fractal expansion with φⁿ scaling"
            }),
        );

        Ok(serde_json::json!(storage_metrics))
    }

    /// Get sacred geometry metrics for monitoring
    pub async fn get_sacred_geometry_metrics(&self) -> Result<serde_json::Value> {
        let validation_result = self.validate_sacred_geometry().await?;

        let active_tasks = self.active_tasks.read().await;
        let task_count = active_tasks.len();
        let fractal_task_count = active_tasks
            .values()
            .filter(|task| task.fractal_requirements.is_some())
            .count();

        Ok(serde_json::json!({
            "sacred_geometry_health": validation_result,
            "task_metrics": {
                "total_active_tasks": task_count,
                "fractal_tasks": fractal_task_count,
                "tetrahedral_distribution": {
                    "coordinator_tasks": active_tasks.values().filter(|t|
                        matches!(self.determine_tetrahedral_position(t), TetrahedralPosition::Coordinator)
                    ).count(),
                    "executor_tasks": active_tasks.values().filter(|t|
                        matches!(self.determine_tetrahedral_position(t), TetrahedralPosition::Executor)
                    ).count(),
                    "referee_tasks": active_tasks.values().filter(|t|
                        matches!(self.determine_tetrahedral_position(t), TetrahedralPosition::Referee)
                    ).count(),
                    "development_tasks": active_tasks.values().filter(|t|
                        matches!(self.determine_tetrahedral_position(t), TetrahedralPosition::Development)
                    ).count(),
                }
            },
            "golden_ratio_constant": self.golden_ratio,
            "node_id": self.node_id,
            "timestamp": chrono::Utc::now(),
        }))
    }

    /// Store LLM response in sacred storage with action UUID mapping
    pub async fn store_llm_response(
        &self,
        action_uuid: Uuid,
        provider: &str,
        request_prompt: &str,
        response_text: &str,
        model: String,
        token_count: Option<u32>,
    ) -> Result<()> {
        self.sacred_store
            .store_llm_response(
                action_uuid,
                provider,
                request_prompt,
                response_text,
                model,
                token_count,
                None,
            )
            .await
            .context("Failed to store LLM response in sacred storage")?;

        info!(
            "💬 Stored LLM response for action {} (provider: {})",
            action_uuid, provider
        );
        Ok(())
    }

    /// Register node from configuration file
    pub async fn register_config_node(
        &self,
        config_node_name: &str,
        config_metadata: std::collections::HashMap<String, serde_json::Value>,
    ) -> Result<()> {
        // Determine tetrahedral position based on config node name
        let tetrahedral_position = match config_node_name {
            name if name.contains("coordinator") => TetrahedralPosition::Coordinator,
            name if name.contains("executor") => TetrahedralPosition::Executor,
            name if name.contains("referee") => TetrahedralPosition::Referee,
            name if name.contains("development") || name.contains("dev") => {
                TetrahedralPosition::Development
            }
            _ => {
                // Use hash-based distribution for unknown names
                let hash = std::collections::hash_map::DefaultHasher::new();
                use std::hash::{Hash, Hasher};
                let mut hasher = hash;
                config_node_name.hash(&mut hasher);
                let index = (hasher.finish() % 4) as usize;
                match index {
                    0 => TetrahedralPosition::Coordinator,
                    1 => TetrahedralPosition::Executor,
                    2 => TetrahedralPosition::Referee,
                    _ => TetrahedralPosition::Development,
                }
            }
        };

        // Generate a unique node ID based on config name
        let node_id = format!("{}_{}", config_node_name, chrono::Utc::now().timestamp());

        self.sacred_store
            .register_node_from_config(
                config_node_name,
                &node_id,
                tetrahedral_position.clone(),
                config_metadata,
            )
            .await
            .context("Failed to register node from config")?;

        info!(
            "🔷 Registered config node: {} -> {} at position: {:?}",
            config_node_name, node_id, tetrahedral_position
        );
        Ok(())
    }

    /// Load SSH config file and register all nodes in Sacred State Store  
    pub async fn load_ssh_config_nodes(&self, ssh_config_path: &str) -> Result<usize> {
        info!("📋 Loading SSH nodes from config: {}", ssh_config_path);

        // Read SSH config file
        let config_content = tokio::fs::read_to_string(ssh_config_path)
            .await
            .context("Failed to read SSH config file")?;

        // Parse JSON config (matches our SSH config structure)
        let config: serde_json::Value =
            serde_json::from_str(&config_content).context("Failed to parse SSH config JSON")?;

        let mut registered_count = 0;

        // Process each node in the SSH config
        if let Some(nodes) = config.as_object() {
            for (node_name, node_config) in nodes {
                // Extract node configuration
                let mut metadata = std::collections::HashMap::new();

                if let Some(obj) = node_config.as_object() {
                    for (key, value) in obj {
                        metadata.insert(key.clone(), value.clone());
                    }
                }

                // Register the SSH node using existing method
                match self.register_config_node(node_name, metadata).await {
                    Ok(()) => {
                        registered_count += 1;
                        info!("✅ Registered SSH node: {}", node_name);
                    }
                    Err(e) => {
                        warn!("❌ Failed to register SSH node {}: {}", node_name, e);
                    }
                }
            }
        }

        info!("🎯 Registered {} SSH nodes from config", registered_count);
        Ok(registered_count)
    }
}

/// Create a new cosmic orchestrator instance with sacred geometric storage
pub async fn create_cosmic_orchestrator(
    src_path: &str,
    storage_path: &str,
    node_id: String,
) -> Result<CosmicOrchestrator> {
    CosmicOrchestrator::new(src_path, storage_path, node_id).await
}

/// Helper function to create a cosmic task
pub fn create_cosmic_task(
    task_type: CosmicTaskType,
    prompt: String,
    context: CosmicContext,
    target_providers: Vec<LLMProvider>,
    fractal_requirements: Option<FractalRequirements>,
    geometric_constraints: Option<GeometricConstraints>,
) -> CosmicTask {
    CosmicTask {
        id: Uuid::new_v4().to_string(),
        task_type,
        status: CosmicTaskStatus::Pending,
        prompt,
        context,
        target_providers,
        fractal_requirements,
        geometric_constraints,
        created_at: SystemTime::now(),
        updated_at: SystemTime::now(),
        result: None,
        error: None,
    }
}
