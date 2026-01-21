//! SessionManager implementation for fractal session management
//!
//! Provides the core session lifecycle management with:
//! - Fractal parent/child session hierarchies
//! - Cross-node session coordination
//! - State snapshot capture and restoration
//! - Metrics rollup from descendants

use crate::storage::ErgorsStorage;
use crate::ErgorsNetworkManifold;
use ho_std::error::{HoError, HoResult};
use ho_std::traits::NetworkTopologyTrait;
use ho_std::types::ergors::management::v1::{
    CreateSessionRequest, FractalMetrics, FractalSession, ParticipantRole, RollupStrategy,
    SessionParticipant, SessionPropagation, SessionScope, SessionStateSnapshot, SessionStatus,
    SessionType, SpawnChildRequest, StateVisibility,
};
use ho_std::types::ergors::network::v1::NodeType;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{debug, info, warn};
use uuid::Uuid;

/// Configuration for the SessionManager
#[derive(Debug, Clone)]
pub struct SessionManagerConfig {
    /// Maximum allowed fractal depth for session hierarchies
    pub max_session_depth: u32,
    /// Maximum children per session
    pub max_children_per_session: u32,
    /// Default propagation rules for new sessions
    pub default_propagation: SessionPropagation,
    /// Enable cross-node session sync
    pub enable_cross_node_sync: bool,
    /// Interval for automatic state snapshots (0 = disabled)
    pub state_snapshot_interval_secs: u64,
    /// Use golden ratio for resource scaling
    pub golden_ratio_resource_scaling: bool,
}

impl Default for SessionManagerConfig {
    fn default() -> Self {
        Self {
            max_session_depth: 10,
            max_children_per_session: 100,
            default_propagation: SessionPropagation {
                inherit_labels: true,
                inherit_metadata: false,
                inherit_participants: true,
                state_visibility: StateVisibility::Read.into(),
                max_children: 100,
                max_depth: 10,
                rollup_strategy: RollupStrategy::OnComplete.into(),
            },
            enable_cross_node_sync: true,
            state_snapshot_interval_secs: 0,
            golden_ratio_resource_scaling: true,
        }
    }
}

/// SessionManager handles fractal session lifecycle management
pub struct SessionManager {
    storage: Arc<ErgorsStorage>,
    network: Arc<Mutex<ErgorsNetworkManifold>>,
    local_node_id: String,
    local_node_type: NodeType,
    config: SessionManagerConfig,
}

impl SessionManager {
    /// Create a new SessionManager
    pub fn new(
        storage: Arc<ErgorsStorage>,
        network: Arc<Mutex<ErgorsNetworkManifold>>,
        local_node_id: String,
        local_node_type: NodeType,
        config: SessionManagerConfig,
    ) -> Self {
        Self {
            storage,
            network,
            local_node_id,
            local_node_type,
            config,
        }
    }

    // ========================================
    // Lifecycle Operations
    // ========================================

    /// Create a new session
    pub async fn create_session(&self, request: CreateSessionRequest) -> HoResult<FractalSession> {
        let session_id = Uuid::new_v4().to_string();

        // Determine fractal depth and root
        let (fractal_depth, root_session_id) = if request.parent_session_id.is_empty() {
            (0, session_id.clone())
        } else {
            let parent = self
                .storage
                .get_fractal_session(&request.parent_session_id)
                .await?
                .ok_or_else(|| HoError::Other("Parent session not found".to_string()))?;
            (parent.fractal_depth + 1, parent.root_session_id.clone())
        };

        // Check depth constraints
        if fractal_depth > self.config.max_session_depth {
            return Err(HoError::Other(format!(
                "Max session depth ({}) exceeded",
                self.config.max_session_depth
            )));
        }

        let now = now_timestamp();

        // Build session with initial values
        let mut session = FractalSession {
            session_id: session_id.clone(),
            session_type: request.session_type,
            status: i32::from(SessionStatus::Created),
            scope: if request.scope == 0 {
                i32::from(SessionScope::Network) // Default to network-wide
            } else {
                request.scope
            },
            parent_session_id: request.parent_session_id.clone(),
            child_session_ids: vec![],
            fractal_depth,
            root_session_id,
            owner_node_id: self.local_node_id.clone(),
            owner_node_type: self.local_node_type.into(),
            participants: vec![SessionParticipant {
                node_id: self.local_node_id.clone(),
                node_type: self.local_node_type.into(),
                role: ParticipantRole::Owner.into(),
                joined_at: Some(now.clone()),
                is_active: true,
            }],
            created_at: Some(now.clone()),
            updated_at: Some(now),
            started_at: None,
            paused_at: None,
            completed_at: None,
            labels: request.labels.clone(),
            metadata: request.metadata.clone(),
            tags: request.tags.clone(),
            state_snapshot: None,
            content: None, // Content will be set based on session type during execution
            metrics: Some(FractalMetrics::default()),
            propagation: Some(
                request
                    .propagation
                    .unwrap_or_else(|| self.config.default_propagation.clone()),
            ),
        };

        // Apply inheritance from parent if configured
        if !request.parent_session_id.is_empty() {
            self.apply_inheritance(&mut session, &request.parent_session_id)
                .await?;
        }

        // Store session
        self.storage.put_fractal_session(&session).await?;

        // Update parent's child list
        if !request.parent_session_id.is_empty() {
            self.add_child_to_parent(&request.parent_session_id, &session_id)
                .await?;
        }

        // Sync to coordinator if network session and we're not the coordinator
        if session.scope == i32::from(SessionScope::Network)
            && self.config.enable_cross_node_sync
            && self.local_node_type != NodeType::Coordinator
        {
            if let Err(e) = self.sync_to_coordinator(&session_id).await {
                warn!("Failed to sync session to coordinator: {}", e);
            }
        }

        info!(
            "Created session {} (type: {:?}, depth: {}, parent: {})",
            session_id,
            SessionType::try_from(session.session_type),
            fractal_depth,
            if request.parent_session_id.is_empty() {
                "none (root)"
            } else {
                &request.parent_session_id
            }
        );

        Ok(session)
    }

    /// Get a session by ID
    pub async fn get_session(&self, session_id: &str) -> HoResult<Option<FractalSession>> {
        self.storage.get_fractal_session(session_id).await
    }

    /// Update session labels, metadata, tags
    pub async fn update_session(
        &self,
        session_id: &str,
        labels: Option<HashMap<String, String>>,
        metadata: Option<HashMap<String, String>>,
        tags: Option<Vec<String>>,
    ) -> HoResult<FractalSession> {
        let mut session = self
            .storage
            .get_fractal_session(session_id)
            .await?
            .ok_or_else(|| HoError::Other("Session not found".to_string()))?;

        // Check if session is mutable
        let status = SessionStatus::try_from(session.status).unwrap_or(SessionStatus::Unspecified);
        if !matches!(
            status,
            SessionStatus::Created | SessionStatus::Active | SessionStatus::Paused
        ) {
            return Err(HoError::Other(
                "Session is not in a mutable state".to_string(),
            ));
        }

        if let Some(new_labels) = labels {
            session.labels = new_labels;
        }
        if let Some(new_metadata) = metadata {
            session.metadata = new_metadata;
        }
        if let Some(new_tags) = tags {
            session.tags = new_tags;
        }

        session.updated_at = Some(now_timestamp());

        // Re-store to update indices
        self.storage.put_fractal_session(&session).await?;

        Ok(session)
    }

    /// Delete a session (optionally cascade to children)
    pub async fn delete_session(&self, session_id: &str, cascade: bool) -> HoResult<()> {
        let session = self
            .storage
            .get_fractal_session(session_id)
            .await?
            .ok_or_else(|| HoError::Other("Session not found".to_string()))?;

        if cascade {
            // Delete all children first (depth-first)
            for child_id in &session.child_session_ids {
                Box::pin(self.delete_session(child_id, true)).await?;
            }
        } else if !session.child_session_ids.is_empty() {
            return Err(HoError::Other(
                "Session has children. Use cascade=true to delete".to_string(),
            ));
        }

        // Remove from parent's child list
        if !session.parent_session_id.is_empty() {
            self.remove_child_from_parent(&session.parent_session_id, session_id)
                .await?;
        }

        self.storage.delete_fractal_session(session_id).await?;

        info!("Deleted session {} (cascade: {})", session_id, cascade);
        Ok(())
    }

    // ========================================
    // State Management
    // ========================================

    /// Pause a session, capturing state snapshot
    pub async fn pause_session(
        &self,
        session_id: &str,
        cascade: bool,
    ) -> HoResult<SessionStateSnapshot> {
        let mut session = self
            .storage
            .get_fractal_session(session_id)
            .await?
            .ok_or_else(|| HoError::Other("Session not found".to_string()))?;

        let status = SessionStatus::try_from(session.status).unwrap_or(SessionStatus::Unspecified);
        if status != SessionStatus::Active {
            return Err(HoError::Other(
                "Session must be active to pause".to_string(),
            ));
        }

        // Capture state snapshot
        let snapshot = self.capture_state_snapshot(&session).await?;

        // Update session
        session.status = i32::from(SessionStatus::Paused);
        session.paused_at = Some(now_timestamp());
        session.updated_at = Some(now_timestamp());
        session.state_snapshot = Some(snapshot.clone());

        self.storage.put_fractal_session(&session).await?;
        self.storage
            .put_session_state_snapshot(session_id, &snapshot)
            .await?;

        // Cascade to children if requested
        if cascade {
            for child_id in &session.child_session_ids {
                if let Ok(Some(child)) = self.storage.get_fractal_session(child_id).await {
                    if child.status == i32::from(SessionStatus::Active) {
                        let _ = Box::pin(self.pause_session(child_id, true)).await;
                    }
                }
            }
        }

        info!("Paused session {} (cascade: {})", session_id, cascade);
        Ok(snapshot)
    }

    /// Resume a paused session
    pub async fn resume_session(
        &self,
        session_id: &str,
        cascade: bool,
    ) -> HoResult<FractalSession> {
        let mut session = self
            .storage
            .get_fractal_session(session_id)
            .await?
            .ok_or_else(|| HoError::Other("Session not found".to_string()))?;

        let status = SessionStatus::try_from(session.status).unwrap_or(SessionStatus::Unspecified);
        if status != SessionStatus::Paused {
            return Err(HoError::Other(
                "Session must be paused to resume".to_string(),
            ));
        }

        // Update session
        session.status = i32::from(SessionStatus::Active);
        session.updated_at = Some(now_timestamp());

        self.storage.put_fractal_session(&session).await?;

        // Cascade to children if requested
        if cascade {
            for child_id in &session.child_session_ids {
                if let Ok(Some(child)) = self.storage.get_fractal_session(child_id).await {
                    if child.status == i32::from(SessionStatus::Paused) {
                        let _ = Box::pin(self.resume_session(child_id, true)).await;
                    }
                }
            }
        }

        info!("Resumed session {} (cascade: {})", session_id, cascade);
        Ok(session)
    }

    /// Complete a session successfully
    pub async fn complete_session(
        &self,
        session_id: &str,
        result: Option<pbjson_types::Struct>,
    ) -> HoResult<FractalSession> {
        let mut session = self
            .storage
            .get_fractal_session(session_id)
            .await?
            .ok_or_else(|| HoError::Other("Session not found".to_string()))?;

        // Rollup metrics from children first
        let metrics = self.rollup_metrics(session_id).await?;
        session.metrics = Some(metrics);

        // Update session
        session.status = i32::from(SessionStatus::Completed);
        session.completed_at = Some(now_timestamp());
        session.updated_at = Some(now_timestamp());

        self.storage.put_fractal_session(&session).await?;

        info!("Completed session {}", session_id);
        Ok(session)
    }

    /// Fail a session with error
    pub async fn fail_session(
        &self,
        session_id: &str,
        error: &str,
        _error_code: Option<&str>,
    ) -> HoResult<FractalSession> {
        let mut session = self
            .storage
            .get_fractal_session(session_id)
            .await?
            .ok_or_else(|| HoError::Other("Session not found".to_string()))?;

        // Update session
        session.status = i32::from(SessionStatus::Failed);
        session.completed_at = Some(now_timestamp());
        session.updated_at = Some(now_timestamp());
        session
            .metadata
            .insert("error".to_string(), error.to_string());

        self.storage.put_fractal_session(&session).await?;

        warn!("Failed session {}: {}", session_id, error);
        Ok(session)
    }

    /// Start a session (transition from Created to Active)
    pub async fn start_session(&self, session_id: &str) -> HoResult<FractalSession> {
        let mut session = self
            .storage
            .get_fractal_session(session_id)
            .await?
            .ok_or_else(|| HoError::Other("Session not found".to_string()))?;

        let status = SessionStatus::try_from(session.status).unwrap_or(SessionStatus::Unspecified);
        if status != SessionStatus::Created {
            return Err(HoError::Other(
                "Session must be in Created state to start".to_string(),
            ));
        }

        session.status = i32::from(SessionStatus::Active);
        session.started_at = Some(now_timestamp());
        session.updated_at = Some(now_timestamp());

        self.storage.put_fractal_session(&session).await?;

        info!("Started session {}", session_id);
        Ok(session)
    }

    // ========================================
    // Fractal Hierarchy Operations
    // ========================================

    /// Spawn a child session
    pub async fn spawn_child(&self, request: SpawnChildRequest) -> HoResult<FractalSession> {
        // Validate parent exists and is in valid state
        let parent = self
            .storage
            .get_fractal_session(&request.parent_session_id)
            .await?
            .ok_or_else(|| HoError::Other("Parent session not found".to_string()))?;

        let status = SessionStatus::try_from(parent.status).unwrap_or(SessionStatus::Unspecified);
        if matches!(status, SessionStatus::Completed | SessionStatus::Failed) {
            return Err(HoError::Other(
                "Cannot spawn child on completed/failed session".to_string(),
            ));
        }

        // Check child count limit
        let max_children = parent
            .propagation
            .as_ref()
            .map(|p| p.max_children)
            .unwrap_or(self.config.max_children_per_session);
        if parent.child_session_ids.len() as u32 >= max_children {
            return Err(HoError::Other(format!(
                "Max children limit ({}) reached",
                max_children
            )));
        }

        // Create child session
        let create_request = CreateSessionRequest {
            session_type: request.child_type,
            scope: request.child_scope,
            parent_session_id: request.parent_session_id.clone(),
            labels: request.labels,
            metadata: HashMap::new(),
            tags: vec![],
            propagation: None,
            initial_content: None,
        };

        let child = self.create_session(create_request).await?;

        // If cross-node spawn requested
        if !request.assigned_node_id.is_empty()
            && request.assigned_node_id != self.local_node_id
            && self.config.enable_cross_node_sync
        {
            // TODO: Implement cross-node migration
            debug!(
                "Cross-node spawn requested to {}, migration not yet implemented",
                request.assigned_node_id
            );
        }

        Ok(child)
    }

    /// Get session hierarchy (ancestors and/or descendants)
    pub async fn get_hierarchy(
        &self,
        session_id: &str,
        include_ancestors: bool,
        include_descendants: bool,
        max_depth: Option<u32>,
    ) -> HoResult<(FractalSession, Vec<FractalSession>, Vec<FractalSession>)> {
        let session = self
            .storage
            .get_fractal_session(session_id)
            .await?
            .ok_or_else(|| HoError::Other("Session not found".to_string()))?;

        let mut ancestors = Vec::new();
        let mut descendants = Vec::new();

        // Get ancestors (root first)
        if include_ancestors && !session.parent_session_id.is_empty() {
            let mut current_id = session.parent_session_id.clone();
            while !current_id.is_empty() {
                if let Some(ancestor) = self.storage.get_fractal_session(&current_id).await? {
                    current_id = ancestor.parent_session_id.clone();
                    ancestors.insert(0, ancestor); // Insert at front for root-first order
                } else {
                    break;
                }
            }
        }

        // Get descendants (BFS)
        if include_descendants {
            let limit = max_depth.unwrap_or(self.config.max_session_depth);
            let mut queue: Vec<(String, u32)> = session
                .child_session_ids
                .iter()
                .map(|id| (id.clone(), 1))
                .collect();

            while let Some((child_id, depth)) = queue.pop() {
                if depth > limit {
                    continue;
                }
                if let Some(child) = self.storage.get_fractal_session(&child_id).await? {
                    for grandchild_id in &child.child_session_ids {
                        queue.push((grandchild_id.clone(), depth + 1));
                    }
                    descendants.push(child);
                }
            }

            // Sort by depth for BFS order
            descendants.sort_by_key(|s| s.fractal_depth);
        }

        Ok((session, ancestors, descendants))
    }

    /// Rollup metrics from all descendants
    pub async fn rollup_metrics(&self, session_id: &str) -> HoResult<FractalMetrics> {
        let session = self
            .storage
            .get_fractal_session(session_id)
            .await?
            .ok_or_else(|| HoError::Other("Session not found".to_string()))?;

        let mut aggregated = session.metrics.clone().unwrap_or_default();

        // Recursively collect metrics from all descendants
        for child_id in &session.child_session_ids {
            let child_metrics = Box::pin(self.rollup_metrics(child_id)).await?;

            aggregated.aggregated_tokens +=
                child_metrics.total_tokens_consumed + child_metrics.aggregated_tokens;
            aggregated.aggregated_cost += child_metrics.total_cost + child_metrics.aggregated_cost;
            aggregated.aggregated_latency_ms +=
                child_metrics.total_latency_ms + child_metrics.aggregated_latency_ms;
            aggregated.total_descendant_count += 1 + child_metrics.total_descendant_count;
            aggregated.max_depth_reached = aggregated
                .max_depth_reached
                .max(child_metrics.max_depth_reached + 1);
        }

        aggregated.child_session_count = session.child_session_ids.len() as u32;

        // Calculate golden ratio efficiency
        let total = aggregated.total_tokens_consumed + aggregated.aggregated_tokens;
        if total > 0 {
            let efficiency_ratio = aggregated.total_tokens_consumed as f64 / total as f64;
            // 0.618 is 1 - golden ratio (the "minor" portion)
            aggregated.golden_ratio_efficiency = 1.0 - (efficiency_ratio - 0.618).abs() / 0.618;
        }

        Ok(aggregated)
    }

    // ========================================
    // Cross-Node Coordination
    // ========================================

    /// Sync session to coordinator node
    async fn sync_to_coordinator(&self, session_id: &str) -> HoResult<()> {
        let nm = self.network.lock().await;
        let topology = nm.get_topology().await;

        if let Some(_coordinator) = topology.nearest_node_of_type(NodeType::Coordinator) {
            // TODO: Implement network sync protocol
            debug!(
                "Would sync session {} to coordinator (not yet implemented)",
                session_id
            );
        }

        Ok(())
    }

    // ========================================
    // Helper Methods
    // ========================================

    /// Apply inheritance from parent session
    async fn apply_inheritance(
        &self,
        session: &mut FractalSession,
        parent_id: &str,
    ) -> HoResult<()> {
        let parent = self
            .storage
            .get_fractal_session(parent_id)
            .await?
            .ok_or_else(|| HoError::Other("Parent not found".to_string()))?;

        if let Some(prop) = &parent.propagation {
            if prop.inherit_labels {
                for (k, v) in &parent.labels {
                    if !session.labels.contains_key(k) {
                        session.labels.insert(k.clone(), v.clone());
                    }
                }
            }
            if prop.inherit_metadata {
                for (k, v) in &parent.metadata {
                    if !session.metadata.contains_key(k) {
                        session.metadata.insert(k.clone(), v.clone());
                    }
                }
            }
            if prop.inherit_participants {
                for participant in &parent.participants {
                    if !session
                        .participants
                        .iter()
                        .any(|p| p.node_id == participant.node_id)
                    {
                        let mut inherited = participant.clone();
                        inherited.role = ParticipantRole::Observer.into(); // Inherited as observer
                        session.participants.push(inherited);
                    }
                }
            }
        }

        Ok(())
    }

    /// Add child to parent's child list
    async fn add_child_to_parent(&self, parent_id: &str, child_id: &str) -> HoResult<()> {
        let mut parent = self
            .storage
            .get_fractal_session(parent_id)
            .await?
            .ok_or_else(|| HoError::Other("Parent not found".to_string()))?;

        parent.child_session_ids.push(child_id.to_string());
        parent.updated_at = Some(now_timestamp());

        self.storage.put_fractal_session(&parent).await?;
        Ok(())
    }

    /// Remove child from parent's child list
    async fn remove_child_from_parent(&self, parent_id: &str, child_id: &str) -> HoResult<()> {
        let mut parent = self
            .storage
            .get_fractal_session(parent_id)
            .await?
            .ok_or_else(|| HoError::Other("Parent not found".to_string()))?;

        parent.child_session_ids.retain(|id| id != child_id);
        parent.updated_at = Some(now_timestamp());

        self.storage.put_fractal_session(&parent).await?;
        Ok(())
    }

    /// Capture a state snapshot of a session
    async fn capture_state_snapshot(
        &self,
        session: &FractalSession,
    ) -> HoResult<SessionStateSnapshot> {
        let serialized = serde_json::to_vec(session)?;
        // Simple hash using std hasher for now
        let hash = {
            use std::collections::hash_map::DefaultHasher;
            use std::hash::Hasher;
            let mut hasher = DefaultHasher::new();
            hasher.write(&serialized);
            format!("{:016x}", hasher.finish())
        };

        let mut child_hashes: HashMap<String, String> = HashMap::new();
        for child_id in &session.child_session_ids {
            if let Some(child) = self.storage.get_fractal_session(child_id).await? {
                if let Some(child_snapshot) = &child.state_snapshot {
                    child_hashes.insert(child_id.clone(), child_snapshot.state_hash.clone());
                }
            }
        }

        let version = session
            .state_snapshot
            .as_ref()
            .map(|s| s.state_version + 1)
            .unwrap_or(1);

        Ok(SessionStateSnapshot {
            serialized_state: serialized,
            state_hash: hash,
            state_version: version,
            captured_at: Some(now_timestamp()),
            child_state_hashes: child_hashes,
        })
    }
}

/// Get current timestamp
fn now_timestamp() -> pbjson_types::Timestamp {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap();
    pbjson_types::Timestamp {
        seconds: now.as_secs() as i64,
        nanos: now.subsec_nanos() as i32,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_defaults() {
        let config = SessionManagerConfig::default();
        assert_eq!(config.max_session_depth, 10);
        assert_eq!(config.max_children_per_session, 100);
        assert!(config.enable_cross_node_sync);
    }
}
