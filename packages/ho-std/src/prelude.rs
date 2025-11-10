//! Prelude module for easy importing of commonly used proto types
//!
//! This module re-exports the most commonly used types from the CW-HO proto definitions
//! to make them easily accessible with `use cw_hoe_proto::prelude::*;`

// // Re-export commonly used types from different modules
pub use crate::types::cw_ho::types::v1::{
    DeleteOperation, FractalOperation, InsertOperation, UpdateOperation,
};

pub use crate::types::cw_ho::network::v1::{
    network_event::EventType, network_message::MessageType, HostOs, MessageReceived, NetworkConfig,
    NetworkError, NetworkEvent, NetworkMessage, NetworkTopology, NodeAnnounce, NodeIdentity,
    NodeInfo, NodeType, PeerConnected, PeerDisconnected, Request, Response, TetrahedralPing,
    TopologyChanged,
};

pub use crate::types::cw_ho::orchestration::v1::{
    ApiKeysJson,
    ApiKeysMetadata,
    // Route request/response types
    BootstrapNodeRequest,
    BootstrapNodeResponse,
    // Orchestration types
    CosmicContext,
    CosmicTask,
    CosmicTaskStatus,
    CreateFractalRequest,
    CreateFractalResponse,
    FractalRequirements,
    GetTopologyRequest,
    GetTopologyResponse,
    GlobalSettings,
    HealthRequest,
    HealthResponse,
    HoConfig,
    // Route metadata types
    HttpMethod,
    Instructions,
    LlmEntity,
    LlmModel,
    LlmRouterConfig,
    LocalLlmConfig,
    OrchestrateTask,
    PromptContext,
    PromptMessage,
    PromptRequest,
    PromptResponse,
    ProviderWithAuth,
    PruneNodeRequest,
    PruneNodeResponse,
    QueryPromptsRequest,
    QueryPromptsResponse,
    RouteMetadata,
    RouteRegistry as ProtoRouteRegistry,
    StorageConfig,
    TokenUsage,
};
pub use crate::types::cw_ho::storage::v1::{
    BootstrapRequest, BootstrapResponse, ErrorResponse, HealthResponse as StorageHealthResponse,
    QueryRequest, StorageIndex, StorageMetrics, StorageQuery, StorageSnapshot,
};

// Re-export other prost types that don't need shimming
pub use prost_types::Value as ProtobufValue;
