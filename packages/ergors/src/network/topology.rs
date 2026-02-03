//! Network topology management
//! // TODO: refactor into storage layer

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Statistics about the network topology
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopologyStats {
    pub total_nodes: usize,
    pub online_nodes: usize,
    pub total_connections: usize,
    pub is_complete: bool,
    pub nodes_by_type: HashMap<String, usize>,
}
