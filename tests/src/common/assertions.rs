//! Custom assertion helpers
//!
//! Provides domain-specific assertions for ERGORS testing.

use anyhow::{anyhow, Result};
use ho_std::types::ergors::network::v1::NodeType;
use std::collections::HashMap;

/// Golden ratio constant (φ)
pub const GOLDEN_RATIO: f64 = 1.618033988749;

/// Tolerance for floating-point comparisons
pub const EPSILON: f64 = 0.0001;

/// Assert that a value is close to the golden ratio
pub fn assert_golden_ratio(value: f64, tolerance: f64) -> Result<()> {
    let diff = (value - GOLDEN_RATIO).abs();
    if diff > tolerance {
        return Err(anyhow!(
            "Value {} is not close to golden ratio {} (diff: {}, tolerance: {})",
            value,
            GOLDEN_RATIO,
            diff,
            tolerance
        ));
    }
    Ok(())
}

/// Assert that a network topology has tetrahedral connectivity (4 nodes)
pub fn assert_tetrahedral_connectivity(nodes: &[(String, NodeType)]) -> Result<()> {
    if nodes.len() != 4 {
        return Err(anyhow!(
            "Tetrahedral topology requires exactly 4 nodes, got {}",
            nodes.len()
        ));
    }

    // Verify node type distribution
    let mut type_counts = HashMap::new();
    for (_, node_type) in nodes {
        *type_counts.entry(node_type).or_insert(0) += 1;
    }

    // Should have at least 1 coordinator
    let coordinator_count = type_counts
        .get(&NodeType::Coordinator)
        .copied()
        .unwrap_or(0);

    if coordinator_count < 1 {
        return Err(anyhow!(
            "Tetrahedral topology requires at least 1 coordinator, got {}",
            coordinator_count
        ));
    }

    Ok(())
}

/// Assert fractal coherence threshold
pub fn assert_fractal_coherence(coherence: f64, min_threshold: f64) -> Result<()> {
    if coherence < min_threshold {
        return Err(anyhow!(
            "Fractal coherence {} is below minimum threshold {}",
            coherence,
            min_threshold
        ));
    }

    if coherence > 1.0 {
        return Err(anyhow!(
            "Fractal coherence {} exceeds maximum value of 1.0",
            coherence
        ));
    }

    Ok(())
}

/// Assert session hierarchy relationships
pub fn assert_session_hierarchy(
    parent_id: &str,
    child_id: &str,
    session_map: &HashMap<String, Option<String>>,
) -> Result<()> {
    match session_map.get(child_id) {
        Some(Some(actual_parent)) if actual_parent == parent_id => Ok(()),
        Some(Some(actual_parent)) => Err(anyhow!(
            "Expected parent {} for child {}, got {}",
            parent_id,
            child_id,
            actual_parent
        )),
        Some(None) => Err(anyhow!("Child {} has no parent set", child_id)),
        None => Err(anyhow!("Child {} not found in session map", child_id)),
    }
}

/// Assert storage consistency between snapshots
pub fn assert_storage_consistency(snapshot1: &[u8], snapshot2: &[u8]) -> Result<()> {
    if snapshot1 != snapshot2 {
        return Err(anyhow!(
            "Storage snapshots are inconsistent (lengths: {} vs {})",
            snapshot1.len(),
            snapshot2.len()
        ));
    }
    Ok(())
}

/// Assert that recursion depth is within bounds
pub fn assert_recursion_depth(depth: u32, max_depth: u32) -> Result<()> {
    if depth > max_depth {
        return Err(anyhow!(
            "Recursion depth {} exceeds maximum {}",
            depth,
            max_depth
        ));
    }
    Ok(())
}

/// Assert self-similarity threshold (should be inverse golden ratio ~0.618)
pub fn assert_self_similarity_threshold(threshold: f64) -> Result<()> {
    let expected = 1.0 / GOLDEN_RATIO;
    let diff = (threshold - expected).abs();
    if diff > EPSILON {
        return Err(anyhow!(
            "Self-similarity threshold {} is not close to inverse golden ratio {} (diff: {})",
            threshold,
            expected,
            diff
        ));
    }
    Ok(())
}

/// Assert that a value is within expected range
pub fn assert_in_range(value: f64, min: f64, max: f64) -> Result<()> {
    if value < min || value > max {
        return Err(anyhow!(
            "Value {} is outside expected range [{}, {}]",
            value,
            min,
            max
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_assert_golden_ratio() {
        assert!(assert_golden_ratio(1.618, 0.001).is_ok());
        assert!(assert_golden_ratio(1.618033988749, 0.0001).is_ok());
        assert!(assert_golden_ratio(1.5, 0.001).is_err());
        assert!(assert_golden_ratio(2.0, 0.001).is_err());
    }

    #[test]
    fn test_assert_fractal_coherence() {
        assert!(assert_fractal_coherence(0.95, 0.9).is_ok());
        assert!(assert_fractal_coherence(1.0, 0.9).is_ok());
        assert!(assert_fractal_coherence(0.8, 0.9).is_err());
        assert!(assert_fractal_coherence(1.1, 0.9).is_err());
    }

    #[test]
    fn test_assert_recursion_depth() {
        assert!(assert_recursion_depth(5, 10).is_ok());
        assert!(assert_recursion_depth(10, 10).is_ok());
        assert!(assert_recursion_depth(11, 10).is_err());
    }

    #[test]
    fn test_assert_in_range() {
        assert!(assert_in_range(5.0, 0.0, 10.0).is_ok());
        assert!(assert_in_range(0.0, 0.0, 10.0).is_ok());
        assert!(assert_in_range(10.0, 0.0, 10.0).is_ok());
        assert!(assert_in_range(-1.0, 0.0, 10.0).is_err());
        assert!(assert_in_range(11.0, 0.0, 10.0).is_err());
    }
}
