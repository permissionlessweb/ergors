//! Prelude module for easy importing of commonly used proto types
//!
//! This module re-exports the most commonly used types from the CW-HO proto definitions
//! to make them easily accessible with `use cw_hoe_proto::prelude::*;`

// // Re-export commonly used types from different modules
pub use crate::types::cw_ho::types::v1::{
    Connection,
    DeleteOperation,
    FractalOperation,
    // SandloopState, TaskCoordination, FractalSync,
    InsertOperation,
    NetworkTopology,
    NodeInfo,
    NodeType,
    UpdateOperation,
};

pub use crate::types::cw_ho::network::v1::{
    network_event::EventType, network_message::MessageType, HostOs, MessageReceived, NetworkConfig,
    NetworkError, NetworkEvent, NetworkMessage, NodeAnnounce, NodeIdentity, PeerConnected,
    PeerDisconnected, Request, Response, TetrahedralPing, TopologyChanged,
};

pub use crate::types::cw_ho::orchestration::v1::{
    CosmicContext, CosmicTask, CosmicTaskStatus, FractalRequirements, HoConfig, LlmEntity,
    LlmModel, LlmRouterConfig, LocalLlmConfig, OrchestrateTask, PromptContext, PromptMessage,
    PromptRequest, PromptResponse, StorageConfig, TokenUsage,
};

pub use crate::types::cw_ho::storage::v1::{
    BootstrapRequest, BootstrapResponse, ErrorResponse, HealthResponse, QueryRequest, StorageIndex,
    StorageMetrics, StorageQuery, StorageSnapshot,
};

// Re-export shim types that are commonly used with proto messages
pub use crate::shim::{
    Any as ProtobufStruct, Duration as ProtobufDuration, Timestamp as ProtobufTimestamp,
};

// Re-export other prost types that don't need shimming
pub use prost_types::Value as ProtobufValue;
