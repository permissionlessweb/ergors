use rs_derive::CosmwasmExt;
/// Represents the CLI command to execute
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, Eq, ::prost::Message, ::serde::Serialize, ::serde::Deserialize)]
pub struct HoCommand {
    #[prost(oneof = "ho_command::Command", tags = "1, 2, 3")]
    pub command: ::core::option::Option<ho_command::Command>,
}
/// Nested message and enum types in `HoCommand`.
pub mod ho_command {
    use rs_derive::CosmwasmExt;
    #[allow(clippy::derive_partial_eq_without_eq)]
    #[derive(Clone, PartialEq, Eq, ::prost::Oneof, ::serde::Serialize, ::serde::Deserialize)]
    pub enum Command {
        #[prost(message, tag = "1")]
        Start(super::StartHo),
        #[prost(message, tag = "2")]
        Init(super::InitHo),
        #[prost(message, tag = "3")]
        Health(super::HealthHo),
    }
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(
    Clone, Copy, PartialEq, Eq, ::prost::Message, ::serde::Serialize, ::serde::Deserialize,
)]
pub struct StartHo {
    #[prost(uint32, optional, tag = "1")]
    pub port: ::core::option::Option<u32>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, Eq, ::prost::Message, ::serde::Serialize, ::serde::Deserialize)]
pub struct InitHo {
    #[prost(string, tag = "1")]
    pub output: ::prost::alloc::string::String,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, Eq, ::prost::Message, ::serde::Serialize, ::serde::Deserialize)]
pub struct HealthHo {
    #[prost(string, tag = "1")]
    pub endpoint: ::prost::alloc::string::String,
}
/// Basic operation types for state management
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, Eq, ::prost::Message, ::serde::Serialize, ::serde::Deserialize)]
pub struct InsertOperation {
    #[prost(string, tag = "1")]
    pub key: ::prost::alloc::string::String,
    #[prost(message, optional, tag = "2")]
    pub value: ::core::option::Option<crate::shim::Any>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, Eq, ::prost::Message, ::serde::Serialize, ::serde::Deserialize)]
pub struct UpdateOperation {
    #[prost(string, tag = "1")]
    pub key: ::prost::alloc::string::String,
    #[prost(message, optional, tag = "2")]
    pub value: ::core::option::Option<crate::shim::Any>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, Eq, ::prost::Message, ::serde::Serialize, ::serde::Deserialize)]
pub struct DeleteOperation {
    #[prost(string, tag = "1")]
    pub key: ::prost::alloc::string::String,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, Eq, ::prost::Message, ::serde::Serialize, ::serde::Deserialize)]
pub struct FractalOperation {
    #[prost(oneof = "fractal_operation::Operation", tags = "1, 2, 3")]
    pub operation: ::core::option::Option<fractal_operation::Operation>,
}
/// Nested message and enum types in `FractalOperation`.
pub mod fractal_operation {
    use rs_derive::CosmwasmExt;
    #[allow(clippy::derive_partial_eq_without_eq)]
    #[derive(Clone, PartialEq, Eq, ::prost::Oneof, ::serde::Serialize, ::serde::Deserialize)]
    pub enum Operation {
        #[prost(message, tag = "1")]
        Insert(super::InsertOperation),
        #[prost(message, tag = "2")]
        Update(super::UpdateOperation),
        #[prost(message, tag = "3")]
        Delete(super::DeleteOperation),
    }
}
/// Basic network topology
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, Eq, ::prost::Message, ::serde::Serialize, ::serde::Deserialize)]
pub struct NetworkTopology {
    #[prost(message, repeated, tag = "1")]
    pub nodes: ::prost::alloc::vec::Vec<NodeInfo>,
    #[prost(message, repeated, tag = "2")]
    pub connections: ::prost::alloc::vec::Vec<Connection>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, Eq, ::prost::Message, ::serde::Serialize, ::serde::Deserialize)]
pub struct NodeInfo {
    #[prost(string, tag = "1")]
    #[serde(alias = "nodeID")]
    pub node_id: ::prost::alloc::string::String,
    #[prost(string, tag = "2")]
    pub node_type: ::prost::alloc::string::String,
    #[prost(bool, tag = "3")]
    pub online: bool,
    #[prost(uint64, tag = "4")]
    #[serde(
        serialize_with = "crate::serde::as_str::serialize",
        deserialize_with = "crate::serde::as_str::deserialize"
    )]
    pub last_seen: u64,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, Eq, ::prost::Message, ::serde::Serialize, ::serde::Deserialize)]
pub struct Connection {
    #[prost(string, tag = "1")]
    #[serde(alias = "from_nodeID")]
    pub from_node_id: ::prost::alloc::string::String,
    #[prost(string, tag = "2")]
    #[serde(alias = "to_nodeID")]
    pub to_node_id: ::prost::alloc::string::String,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, Eq, ::prost::Message, ::serde::Serialize, ::serde::Deserialize)]
pub struct TaskCoordination {
    #[prost(string, tag = "1")]
    #[serde(alias = "taskID")]
    pub task_id: ::prost::alloc::string::String,
    #[prost(enumeration = "NodeType", tag = "2")]
    #[serde(
        serialize_with = "crate::serde::as_str::serialize",
        deserialize_with = "crate::serde::as_str::deserialize"
    )]
    pub from_role: i32,
    #[prost(enumeration = "NodeType", tag = "3")]
    #[serde(
        serialize_with = "crate::serde::as_str::serialize",
        deserialize_with = "crate::serde::as_str::deserialize"
    )]
    pub to_role: i32,
    #[prost(string, tag = "4")]
    pub task_type: ::prost::alloc::string::String,
    #[prost(message, optional, tag = "5")]
    pub payload: ::core::option::Option<crate::shim::Any>,
}
/// Basic node types used across all services
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
#[derive(::serde::Serialize, ::serde::Deserialize)]
pub enum NodeType {
    Unspecified = 0,
    Coordinator = 1,
    Executor = 2,
    Referee = 3,
    Development = 4,
}
impl NodeType {
    /// String value of the enum field names used in the ProtoBuf definition.
    ///
    /// The values are not transformed in any way and thus are considered stable
    /// (if the ProtoBuf definition does not change) and safe for programmatic use.
    pub fn as_str_name(&self) -> &'static str {
        match self {
            NodeType::Unspecified => "NODE_TYPE_UNSPECIFIED",
            NodeType::Coordinator => "NODE_TYPE_COORDINATOR",
            NodeType::Executor => "NODE_TYPE_EXECUTOR",
            NodeType::Referee => "NODE_TYPE_REFEREE",
            NodeType::Development => "NODE_TYPE_DEVELOPMENT",
        }
    }
    /// Creates an enum from field names used in the ProtoBuf definition.
    pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
        match value {
            "NODE_TYPE_UNSPECIFIED" => Some(Self::Unspecified),
            "NODE_TYPE_COORDINATOR" => Some(Self::Coordinator),
            "NODE_TYPE_EXECUTOR" => Some(Self::Executor),
            "NODE_TYPE_REFEREE" => Some(Self::Referee),
            "NODE_TYPE_DEVELOPMENT" => Some(Self::Development),
            _ => None,
        }
    }
}
