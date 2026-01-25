//! Test data fixtures
//!
//! Provides deterministic test data generation for all ERGORS components.

use ho_std::types::ergors::network::v1::NodeType;
use ho_std::types::ergors::orch::v1::{
    CosmicContext, CosmicTask, CosmicTaskStatus, FractalRequirements, OrchestrateTask,
};
use ho_std::utils::IdGenerator;
use std::collections::HashMap;

/// Create a deterministic test node identity
pub fn create_test_node_identity(node_type: NodeType, index: u16) -> String {
    format!("test-{}-{}", node_type.as_str_name().to_lowercase(), index)
}

/// Create a test cosmic task with default values
pub fn create_test_cosmic_task(
    task_type: OrchestrateTask,
    prompt: impl Into<String>,
) -> CosmicTask {
    CosmicTask {
        id: IdGenerator::new_uuid_string(),
        task_type: task_type as i32,
        status: CosmicTaskStatus::Pending as i32,
        prompt: prompt.into(),
        result: None,
        error: String::new(),
        fractal_requirements: None,
        created_at: None,
        updated_at: None,
    }
}

/// Create test fractal requirements with configurable depth
pub fn create_test_fractal_requirements(
    recursion_depth: u32,
    golden_ratio_compliance: bool,
) -> FractalRequirements {
    let cosmic_context = CosmicContext {
        task_id: IdGenerator::new_uuid_string(),
        user_input: "Test fractal recursion".to_string(),
        current_step: 0,
        total_steps: recursion_depth,
        fractal_level: 0,
        golden_ratio_state: if golden_ratio_compliance {
            "1.618033988749".to_string()
        } else {
            "1.0".to_string()
        },
        previous_responses: Vec::new(),
        cosmic_metadata: HashMap::new(),
    };

    FractalRequirements {
        context: Some(cosmic_context),
        recursion_depth,
        self_similarity_threshold: 0.618, // Inverse golden ratio
        golden_ratio_compliance,
        fractal_dimension_target: 1.618,
        mobius_continuity: true,
        fractal_coherence: 0.95,
        expansion_criteria: vec![
            "geometric_harmony".to_string(),
            "self_similarity".to_string(),
            "golden_ratio_compliance".to_string(),
        ],
    }
}

/// Create a test network topology with specified number of nodes
pub fn create_test_network_topology(node_count: usize) -> Vec<(String, NodeType)> {
    let mut nodes = Vec::new();

    // Always include at least one coordinator
    nodes.push((
        create_test_node_identity(NodeType::Coordinator, 0),
        NodeType::Coordinator,
    ));

    // Add remaining nodes as executors and referees
    for i in 1..node_count {
        let node_type = if i % 3 == 0 {
            NodeType::Referee
        } else {
            NodeType::Executor
        };
        nodes.push((create_test_node_identity(node_type, i as u16), node_type));
    }

    nodes
}

/// Create a tetrahedral test topology (4 nodes)
pub fn create_tetrahedral_topology() -> Vec<(String, NodeType)> {
    vec![
        (
            create_test_node_identity(NodeType::Coordinator, 0),
            NodeType::Coordinator,
        ),
        (
            create_test_node_identity(NodeType::Executor, 1),
            NodeType::Executor,
        ),
        (
            create_test_node_identity(NodeType::Executor, 2),
            NodeType::Executor,
        ),
        (
            create_test_node_identity(NodeType::Referee, 0),
            NodeType::Referee,
        ),
    ]
}

/// Create a test session hierarchy with parent/child relationships
pub fn create_test_session_hierarchy(depth: u32) -> Vec<String> {
    let mut session_ids = Vec::new();

    // Root session
    let root_id = IdGenerator::new_uuid_string();
    session_ids.push(root_id.clone());

    // Create children recursively
    for level in 1..=depth {
        for _ in 0..level {
            session_ids.push(IdGenerator::new_uuid_string());
        }
    }

    session_ids
}

/// Create a test LLM request prompt
pub fn create_test_llm_request(model: impl Into<String>, prompt: impl Into<String>) -> String {
    serde_json::json!({
        "model": model.into(),
        "prompt": prompt.into(),
        "max_tokens": 100,
        "temperature": 0.7
    })
    .to_string()
}

/// Create a valid Akash SDL template for testing
pub fn create_test_sdl(service_name: impl Into<String>, image: impl Into<String>) -> String {
    let service_name = service_name.into();
    let image = image.into();
    format!(
        r#"---
version: "2.0"

services:
  {0}:
    image: {1}
    expose:
      - port: 8080
        as: 80
        to:
          - global: true

profiles:
  compute:
    {0}:
      resources:
        cpu:
          units: 0.5
        memory:
          size: 512Mi
        storage:
          size: 512Mi
  placement:
    default:
      attributes:
        host: akash
      signedBy:
        anyOf:
          - "akash1vz375dkt0c60annyp6mkzeejfq0qpyevhseu05"
      pricing:
        {0}:
          denom: uakt
          amount: 1000

deployment:
  {0}:
    default:
      profile: {0}
      count: 1
"#,
        service_name, image
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_test_cosmic_task() {
        let task = create_test_cosmic_task(OrchestrateTask::Bootstrap, "Test prompt");
        assert_eq!(task.task_type, OrchestrateTask::Bootstrap as i32);
        assert_eq!(task.status, CosmicTaskStatus::Pending as i32);
        assert_eq!(task.prompt, "Test prompt");
        assert!(!task.id.is_empty());
    }

    #[test]
    fn test_create_tetrahedral_topology() {
        let topology = create_tetrahedral_topology();
        assert_eq!(topology.len(), 4);

        // Verify we have one of each type (coordinator, 2 executors, referee)
        let coordinator_count = topology
            .iter()
            .filter(|(_, t)| matches!(t, NodeType::Coordinator))
            .count();
        let executor_count = topology
            .iter()
            .filter(|(_, t)| matches!(t, NodeType::Executor))
            .count();
        let referee_count = topology
            .iter()
            .filter(|(_, t)| matches!(t, NodeType::Referee))
            .count();

        assert_eq!(coordinator_count, 1);
        assert_eq!(executor_count, 2);
        assert_eq!(referee_count, 1);
    }

    #[test]
    fn test_create_test_fractal_requirements() {
        let fractal = create_test_fractal_requirements(3, true);
        assert_eq!(fractal.recursion_depth, 3);
        assert!(fractal.golden_ratio_compliance);
        assert_eq!(fractal.self_similarity_threshold, 0.618);
        assert_eq!(fractal.fractal_dimension_target, 1.618);
    }
}
