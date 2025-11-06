//! Network topology management

use ho_std::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Simplified tetrahedral network topology
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkTopology {
    pub nodes: HashMap<String, NodeInfo>,
    pub connections: Vec<(String, String)>,
}

impl NetworkTopology {
    /// Create a new empty topology
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            connections: Vec::new(),
        }
    }

    /// Add a node to the topology
    pub fn add_node(&mut self, info: NodeInfo) {
        self.nodes.insert(info.node_id.clone(), info);
    }

    /// Remove a node from the topology
    pub fn remove_node(&mut self, node_id: &str) {
        self.nodes.remove(node_id);
        self.connections
            .retain(|(from, to)| from != node_id && to != node_id);
    }

    /// Add a connection between two nodes
    pub fn add_connection(&mut self, from: String, to: String) {
        if !self.has_connection(&from, &to) {
            self.connections.push((from, to));
        }
    }

    /// Check if a connection exists
    pub fn has_connection(&self, from: &str, to: &str) -> bool {
        self.connections
            .iter()
            .any(|(a, b)| (a == from && b == to) || (a == to && b == from))
    }

    /// Get all nodes of a specific type
    pub fn nodes_by_type(&self, node_type: NodeType) -> Vec<&NodeInfo> {
        self.nodes
            .values()
            .filter(|info| info.node_type == node_type.as_str_name())
            .collect()
    }

    /// Get all online nodes
    pub fn online_nodes(&self) -> Vec<&NodeInfo> {
        self.nodes.values().filter(|info| info.online).collect()
    }

    /// Check if the topology forms a complete tetrahedral structure
    pub fn is_complete_tetrahedron(&self) -> bool {
        let online_nodes = self.online_nodes();

        // Need exactly 4 nodes (one of each type)
        if online_nodes.len() != 4 {
            return false;
        }

        // Check we have one of each type
        let types: Vec<NodeType> = online_nodes
            .iter()
            .map(|n| NodeType::from_str_name(&n.node_type.clone()).unwrap())
            .collect();

        let has_coordinator = types.contains(&NodeType::Coordinator);
        let has_executor = types.contains(&NodeType::Executor);
        let has_referee = types.contains(&NodeType::Referee);
        let has_development = types.contains(&NodeType::Development);

        if !(has_coordinator && has_executor && has_referee && has_development) {
            return false;
        }

        // Check each node is connected to all others (6 edges for 4 nodes)
        let expected_connections = 6;
        let actual_connections = self.connections.len();

        actual_connections >= expected_connections
    }

    /// Get the nearest node of a specific type
    pub fn nearest_node_of_type(&self, node_type: NodeType) -> Option<&NodeInfo> {
        self.nodes_by_type(node_type).into_iter().find(|n| n.online)
    }

    /// Get statistics about the topology
    pub fn stats(&self) -> TopologyStats {
        TopologyStats {
            total_nodes: self.nodes.len(),
            online_nodes: self.online_nodes().len(),
            total_connections: self.connections.len(),
            is_complete: self.is_complete_tetrahedron(),
            nodes_by_type: self.count_nodes_by_type(),
        }
    }

    fn count_nodes_by_type(&self) -> HashMap<String, usize> {
        let mut counts = HashMap::new();
        for node in self.nodes.values() {
            *counts.entry(node.node_type.clone()).or_insert(0) += 1;
        }
        counts
    }
}

/// Statistics about the network topology
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopologyStats {
    pub total_nodes: usize,
    pub online_nodes: usize,
    pub total_connections: usize,
    pub is_complete: bool,
    pub nodes_by_type: HashMap<String, usize>,
}

impl Default for NetworkTopology {
    fn default() -> Self {
        Self::new()
    }
}
