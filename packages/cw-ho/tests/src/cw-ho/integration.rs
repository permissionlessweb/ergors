//! Example main.rs showing complete CW-HO workflow
//!
//! This demonstrates how to use all the components together to create a fully
//! functional CW-HO instance that respects the sacred geometric principles.

use cw_ho::{HoConfig, HoOrchestrator};
use ho_std::types::cw_ho::orchestration::v1::{CosmicTaskStatus, OrchestrateTask};
use std::time::Duration;
use tokio;
use tracing::{error, info};

async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    info!("Starting CW-HO System Example");

    // Create and initialize the orchestrator
    let orchestrator = match HoOrchestrator::new().await {
        Ok(orch) => orch,
        Err(e) => {
            error!("Failed to create orchestrator: {}", e);
            return Err(Box::new(e));
        }
    };

    // Initialize the system
    if let Err(e) = orchestrator.initialize().await {
        error!("Failed to initialize orchestrator: {}", e);
        return Err(Box::new(e));
    }

    info!("CW-HO Orchestrator initialized successfully");

    // Demonstrate bootstrap task processing
    info!("Creating and processing bootstrap task...");
    let bootstrap_task = orchestrator.create_cosmic_task(
        OrchestrateTask::Bootstrap,
        "Initialize the sacred geometric network topology for optimal fractal communication patterns.".to_string(),
        None,
    );
    let completed_bootstrap = orchestrator.process_cosmic_task(bootstrap_task).await?;
    info!(
        "Bootstrap task status: {:?}",
        CosmicTaskStatus::try_from(completed_bootstrap.status)
    );
    // Demonstrate recursive task with fractal requirements
    info!("Creating and processing recursive fractal task...");
    let fractal_requirements = orchestrator.create_fractal_requirements(3, 0.618).await;
    let recursive_task = orchestrator.create_cosmic_task(
        OrchestrateTask::Recursive,
        "Expand the network capabilities using recursive fractal patterns that maintain golden ratio harmony.".to_string(),
        Some(fractal_requirements),
    );

    let completed_recursive = orchestrator.process_cosmic_task(recursive_task).await?;
    info!(
        "Recursive task status: {:?}",
        CosmicTaskStatus::try_from(completed_recursive.status)
    );
    // Show system health status
    let health = orchestrator.get_health_status().await;
    info!("System Health Status:");
    info!("  Overall: {}", health.overall_status);
    info!("  Network Nodes: {}", health.network_nodes);
    info!("  Active Tasks: {}", health.active_tasks);
    info!("  Geometric Coherence: {:.4}", health.geometric_coherence);
    info!("  Storage Usage: {:.2} MB", health.storage_usage_mb);

    // Demonstrate file sharing functionality
    info!("Demonstrating file operations...");
    demonstrate_file_operations().await?;

    // Demonstrate network operations
    info!("Demonstrating network operations...");
    demonstrate_network_operations().await?;

    // Run for a bit to simulate ongoing operations
    info!("Running system for 10 seconds to demonstrate ongoing operations...");
    tokio::time::sleep(Duration::from_secs(10)).await;

    // Graceful shutdown
    info!("Shutting down CW-HO System...");
    orchestrator.shutdown().await?;
    info!("CW-HO System Example completed successfully!");
    Ok(())
}

async fn demonstrate_network_operations() -> Result<(), Box<dyn std::error::Error>> {
    use ho_std::types::cw_ho::network::v1::NodeIdentity;
    use ho_std::types::cw_ho::types::v1::{NodeInfo, NodeType};
    use ho_std::shared_impl::NetworkShareImpl;

    // Create a mock node identity
    let identity = NodeIdentity {
        host: "127.0.0.1".to_string(),
        p2p_port: 26969,
        api_port: 8080,
        user: "cw-ho-example".to_string(),
        os: 0, // Linux
        ssh_port: 22,
        node_type: NodeType::Coordinator.as_str_name().to_string(),
        public_key: Some(vec![0u8; 32]),
        private_key: None, // Don't include in announcement
    };

    // Create node announcement
    let announcement = NetworkShareImpl::create_node_announcement(&identity);
    info!(
        "Created node announcement for: {}:{}",
        identity.host, identity.p2p_port
    );

    // Create a sample network topology
    let nodes = vec![
        NodeInfo {
            node_id: "node-1".to_string(),
            node_type: NodeType::Coordinator.as_str_name().to_string(),
            online: true,
            last_seen: ho_std::utils::IdGenerator::timestamp_seconds(),
        },
        NodeInfo {
            node_id: "node-2".to_string(),
            node_type: NodeType::Executor.as_str_name().to_string(),
            online: true,
            last_seen: ho_std::utils::IdGenerator::timestamp_seconds(),
        },
    ];

    let topology = NetworkShareImpl::create_topology(nodes);
    info!(
        "Created network topology with {} nodes and {} connections",
        topology.nodes.len(),
        topology.connections.len()
    );

    // Create a tetrahedral ping
    let ping = NetworkShareImpl::create_ping("node-1", topology);
    info!("Created tetrahedral ping message");

    Ok(())
}

/// Example of creating custom fractal requirements
pub fn create_advanced_fractal_requirements(
) -> ho_std::types::cw_ho::orchestration::v1::FractalRequirements {
    use ho_std::types::cw_ho::orchestration::v1::*;
    use ho_std::utils::IdGenerator;

    let cosmic_context = CosmicContext {
        task_id: IdGenerator::new_uuid_string(),
        user_input: "Advanced fractal processing with golden ratio optimization".to_string(),
        current_step: 0,
        total_steps: 7, // Sacred number
        fractal_level: 0,
        golden_ratio_state: "1.618033988749".to_string(),
        previous_responses: Vec::new(),
        cosmic_metadata: std::collections::HashMap::new(),
    };

    FractalRequirements {
        context: Some(cosmic_context),
        recursion_depth: 7,
        self_similarity_threshold: 0.618, // Inverse golden ratio
        golden_ratio_compliance: true,
        fractal_dimension_target: 1.618,
        mobius_continuity: true,
        fractal_coherence: 0.95,
        expansion_criteria: vec![
            "geometric_harmony".to_string(),
            "self_similarity".to_string(),
            "golden_ratio_compliance".to_string(),
            "fractal_coherence_maintained".to_string(),
            "sacred_proportions".to_string(),
        ],
    }
}
