//! Orchestrator-related traits for CW-HO system

use crate::error::HoResult;
use async_trait::async_trait;
use std::collections::HashMap;

/// Core trait for cosmic task management
pub trait CosmicTaskTrait {
    type TaskType;
    type TaskStatus;
    type FractalRequirements;
    type Timestamp;
    type StructData;

    /// Get task ID
    fn id(&self) -> &str;

    /// Get task type
    fn task_type(&self) -> &Self::TaskType;

    /// Get current status
    fn status(&self) -> &Self::TaskStatus;

    /// Get prompt
    fn prompt(&self) -> &str;

    /// Get fractal requirements
    fn fractal_requirements(&self) -> Option<&Self::FractalRequirements>;

    /// Get creation timestamp
    fn created_at(&self) -> &Self::Timestamp;

    /// Get update timestamp
    fn updated_at(&self) -> &Self::Timestamp;

    /// Get task result
    fn result(&self) -> Option<&Self::StructData>;

    /// Get error message
    fn error(&self) -> Option<&str>;

    /// Update task status
    fn set_status(&mut self, status: Self::TaskStatus);

    /// Set task result
    fn set_result(&mut self, result: Self::StructData);

    /// Set error message
    fn set_error(&mut self, error: String);
}

/// Core trait for cosmic context management
pub trait CosmicContextTrait {
    type PromptResponse;
    type StructData;

    /// Create new context
    fn new_context(task_id: String, user_input: String, total_steps: u32) -> Self;

    /// Get task ID
    fn task_id(&self) -> &str;

    /// Get user input
    fn user_input(&self) -> &str;

    /// Get current step
    fn current_step(&self) -> u32;

    /// Get total steps
    fn total_steps(&self) -> u32;

    /// Get fractal level
    fn fractal_level(&self) -> u32;

    /// Get golden ratio state
    fn golden_ratio_state(&self) -> f64;

    /// Get previous responses
    fn previous_responses(&self) -> &[Self::PromptResponse];

    /// Get cosmic metadata
    fn cosmic_metadata(&self) -> &HashMap<String, Self::StructData>;

    /// Add previous response
    fn add_response(&mut self, response: Self::PromptResponse);

    /// Set metadata
    fn set_metadata(&mut self, key: String, value: Self::StructData);

    /// Increment step
    fn next_step(&mut self);

    /// Increment fractal level
    fn next_fractal_level(&mut self);
}

/// Core trait for fractal requirements
pub trait FractalRequirementsTrait {
    type CosmicContext;

    /// Create default fractal requirements
    fn new_default() -> Self
    where
        Self: Sized;

    /// Get context
    fn context(&self) -> Option<&Self::CosmicContext>;

    /// Get recursion depth
    fn recursion_depth(&self) -> u32;

    /// Get self-similarity threshold
    fn self_similarity_threshold(&self) -> f64;

    /// Check golden ratio compliance
    fn golden_ratio_compliance(&self) -> bool;

    /// Get fractal dimension target
    fn fractal_dimension_target(&self) -> f64;

    /// Check mobius continuity
    fn mobius_continuity(&self) -> bool;

    /// Get fractal coherence
    fn fractal_coherence(&self) -> f64;

    /// Get expansion criteria
    fn expansion_criteria(&self) -> &[String];

    /// Set recursion depth
    fn set_recursion_depth(&mut self, depth: u32);

    /// Set threshold
    fn set_threshold(&mut self, threshold: f64);

    /// Add expansion criterion
    fn add_expansion_criterion(&mut self, criterion: String);
}

/// Core trait for orchestrator execution
#[async_trait]
pub trait OrchestratorTrait {
    type Task: CosmicTaskTrait;
    type Context: CosmicContextTrait;
    type Config;

    /// Create new orchestrator
    async fn new(config: Self::Config) -> HoResult<Self>
    where
        Self: Sized;

    /// Execute a cosmic task
    async fn execute_task(&mut self, task: &mut Self::Task) -> HoResult<()>;

    /// Execute recursive orchestration
    async fn execute_recursive_orchestration(&mut self, task: &mut Self::Task) -> HoResult<()>;

    /// Execute fractal agent creation
    async fn execute_fractal_agent_creation(&mut self, task: &mut Self::Task) -> HoResult<()>;

    /// Create fractal context
    fn create_fractal_context(&self, task_id: String, prompt: &str, depth: u32) -> Self::Context;

    /// Validate fractal requirements
    fn validate_fractal_requirements<F>(&self, requirements: &F) -> bool
    where
        F: FractalRequirementsTrait<CosmicContext = Self::Context>;

    /// Apply golden ratio scaling
    fn apply_golden_ratio_scaling(&self, value: f64) -> f64 {
        value * 1.618033988749894
    }
}
