use rs_derive::CosmwasmExt;
/// Network Configuration
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(
    Clone, Copy, PartialEq, Eq, ::prost::Message, ::serde::Serialize, ::serde::Deserialize,
)]
pub struct NetworkLimits {
    #[prost(uint32, tag = "1")]
    #[serde(
        serialize_with = "crate::serde::as_str::serialize",
        deserialize_with = "crate::serde::as_str::deserialize"
    )]
    pub max_message_size: u32,
    #[prost(uint32, tag = "2")]
    #[serde(
        serialize_with = "crate::serde::as_str::serialize",
        deserialize_with = "crate::serde::as_str::deserialize"
    )]
    pub max_peers: u32,
    #[prost(uint64, tag = "3")]
    #[serde(
        serialize_with = "crate::serde::as_str::serialize",
        deserialize_with = "crate::serde::as_str::deserialize"
    )]
    pub connection_timeout: u64,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(
    Clone, Copy, PartialEq, Eq, ::prost::Message, ::serde::Serialize, ::serde::Deserialize,
)]
pub struct ChannelConfig {
    #[prost(uint32, tag = "1")]
    #[serde(
        serialize_with = "crate::serde::as_str::serialize",
        deserialize_with = "crate::serde::as_str::deserialize"
    )]
    pub discovery_buffer: u32,
    #[prost(uint32, tag = "2")]
    #[serde(
        serialize_with = "crate::serde::as_str::serialize",
        deserialize_with = "crate::serde::as_str::deserialize"
    )]
    pub task_buffer: u32,
    #[prost(uint32, tag = "3")]
    #[serde(
        serialize_with = "crate::serde::as_str::serialize",
        deserialize_with = "crate::serde::as_str::deserialize"
    )]
    pub state_buffer: u32,
    #[prost(uint32, tag = "4")]
    #[serde(
        serialize_with = "crate::serde::as_str::serialize",
        deserialize_with = "crate::serde::as_str::deserialize"
    )]
    pub health_buffer: u32,
}
/// Network Configuration
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(
    Clone, Copy, PartialEq, Eq, ::prost::Message, ::serde::Serialize, ::serde::Deserialize,
)]
pub struct NewNetworkConfig {}
/// Network Configuration
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, Eq, ::prost::Message, ::serde::Serialize, ::serde::Deserialize)]
pub struct NetworkConfig {
    #[prost(string, tag = "1")]
    pub node_type: ::prost::alloc::string::String,
    #[prost(string, repeated, tag = "2")]
    pub bootstrap_peers: ::prost::alloc::vec::Vec<::prost::alloc::string::String>,
    #[prost(string, repeated, tag = "3")]
    pub known_peers: ::prost::alloc::vec::Vec<::prost::alloc::string::String>,
    #[prost(uint32, tag = "4")]
    #[serde(
        serialize_with = "crate::serde::as_str::serialize",
        deserialize_with = "crate::serde::as_str::deserialize"
    )]
    pub listen_port: u32,
    #[prost(string, tag = "5")]
    pub listen_address: ::prost::alloc::string::String,
    #[prost(uint32, tag = "6")]
    #[serde(
        serialize_with = "crate::serde::as_str::serialize",
        deserialize_with = "crate::serde::as_str::deserialize"
    )]
    pub max_peers: u32,
    #[prost(uint32, tag = "7")]
    #[serde(
        serialize_with = "crate::serde::as_str::serialize",
        deserialize_with = "crate::serde::as_str::deserialize"
    )]
    pub connection_timeout_ms: u32,
    #[prost(bool, tag = "8")]
    pub enable_discovery: bool,
    #[prost(message, optional, tag = "9")]
    pub limits: ::core::option::Option<NetworkLimits>,
    #[prost(message, optional, tag = "10")]
    pub channels: ::core::option::Option<ChannelConfig>,
}
/// Network Communication Types
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, Eq, ::prost::Message, ::serde::Serialize, ::serde::Deserialize)]
pub struct NetworkMessage {
    #[prost(oneof = "network_message::MessageType", tags = "1, 2, 3, 4, 5, 6, 7")]
    pub message_type: ::core::option::Option<network_message::MessageType>,
}
/// Nested message and enum types in `NetworkMessage`.
pub mod network_message {
    use rs_derive::CosmwasmExt;
    #[allow(clippy::derive_partial_eq_without_eq)]
    #[derive(Clone, PartialEq, Eq, ::prost::Oneof, ::serde::Serialize, ::serde::Deserialize)]
    pub enum MessageType {
        #[prost(message, tag = "1")]
        NodeAnnounce(super::NodeAnnounce),
        #[prost(message, tag = "2")]
        TaskCoordination(super::super::super::types::v1::TaskCoordination),
        #[prost(message, tag = "3")]
        SandloopState(super::SandloopState),
        #[prost(message, tag = "4")]
        FractalSync(super::FractalSync),
        #[prost(message, tag = "5")]
        TetrahedralPing(super::TetrahedralPing),
        #[prost(message, tag = "6")]
        Request(super::Request),
        #[prost(message, tag = "7")]
        Response(super::Response),
    }
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, Eq, ::prost::Message, ::serde::Serialize, ::serde::Deserialize)]
pub struct TetrahedralPing {
    #[prost(string, tag = "1")]
    pub from_node: ::prost::alloc::string::String,
    #[prost(uint64, tag = "2")]
    #[serde(
        serialize_with = "crate::serde::as_str::serialize",
        deserialize_with = "crate::serde::as_str::deserialize"
    )]
    pub timestamp: u64,
    #[prost(message, optional, tag = "3")]
    pub network_topology: ::core::option::Option<super::super::types::v1::NetworkTopology>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, Eq, ::prost::Message, ::serde::Serialize, ::serde::Deserialize)]
pub struct FractalSync {
    #[prost(uint64, tag = "1")]
    #[serde(
        serialize_with = "crate::serde::as_str::serialize",
        deserialize_with = "crate::serde::as_str::deserialize"
    )]
    pub state_version: u64,
    #[prost(uint32, tag = "2")]
    #[serde(
        serialize_with = "crate::serde::as_str::serialize",
        deserialize_with = "crate::serde::as_str::deserialize"
    )]
    pub fractal_depth: u32,
    #[prost(string, tag = "3")]
    pub state_root: ::prost::alloc::string::String,
    #[prost(message, repeated, tag = "4")]
    pub delta_operations: ::prost::alloc::vec::Vec<super::super::types::v1::FractalOperation>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, Eq, ::prost::Message, ::serde::Serialize, ::serde::Deserialize)]
pub struct SandloopState {
    #[prost(string, tag = "1")]
    #[serde(alias = "loopID")]
    pub loop_id: ::prost::alloc::string::String,
    #[prost(uint64, tag = "2")]
    #[serde(
        serialize_with = "crate::serde::as_str::serialize",
        deserialize_with = "crate::serde::as_str::deserialize"
    )]
    pub iteration: u64,
    #[prost(string, tag = "3")]
    pub phase: ::prost::alloc::string::String,
    #[prost(message, optional, tag = "4")]
    pub metrics: ::core::option::Option<crate::shim::Any>,
}
/// Network Node Types
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, Eq, ::prost::Message, ::serde::Serialize, ::serde::Deserialize)]
pub struct NodeIdentity {
    #[prost(string, tag = "1")]
    pub host: ::prost::alloc::string::String,
    #[prost(uint32, tag = "2")]
    #[serde(
        serialize_with = "crate::serde::as_str::serialize",
        deserialize_with = "crate::serde::as_str::deserialize"
    )]
    pub p2p_port: u32,
    #[prost(uint32, tag = "3")]
    #[serde(
        serialize_with = "crate::serde::as_str::serialize",
        deserialize_with = "crate::serde::as_str::deserialize"
    )]
    pub api_port: u32,
    #[prost(string, tag = "4")]
    pub user: ::prost::alloc::string::String,
    #[prost(enumeration = "HostOs", tag = "5")]
    #[serde(
        serialize_with = "crate::serde::as_str::serialize",
        deserialize_with = "crate::serde::as_str::deserialize"
    )]
    pub os: i32,
    #[prost(uint32, tag = "6")]
    #[serde(
        serialize_with = "crate::serde::as_str::serialize",
        deserialize_with = "crate::serde::as_str::deserialize"
    )]
    pub ssh_port: u32,
    #[prost(string, tag = "7")]
    pub node_type: ::prost::alloc::string::String,
    #[prost(bytes = "vec", optional, tag = "8")]
    pub public_key: ::core::option::Option<::prost::alloc::vec::Vec<u8>>,
    /// Only included when needed, should be handled carefully
    #[prost(bytes = "vec", optional, tag = "9")]
    pub private_key: ::core::option::Option<::prost::alloc::vec::Vec<u8>>,
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
pub struct NodeAnnounce {
    #[prost(string, tag = "1")]
    #[serde(alias = "nodeID")]
    pub node_id: ::prost::alloc::string::String,
    #[prost(enumeration = "super::super::types::v1::NodeType", tag = "2")]
    #[serde(
        serialize_with = "crate::serde::as_str::serialize",
        deserialize_with = "crate::serde::as_str::deserialize"
    )]
    pub role: i32,
    #[prost(string, repeated, tag = "3")]
    pub capabilities: ::prost::alloc::vec::Vec<::prost::alloc::string::String>,
    #[prost(string, tag = "4")]
    pub load_factor: ::prost::alloc::string::String,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, Eq, ::prost::Message, ::serde::Serialize, ::serde::Deserialize)]
pub struct Request {
    #[prost(string, tag = "1")]
    #[serde(alias = "requestID")]
    pub request_id: ::prost::alloc::string::String,
    #[prost(message, optional, tag = "2")]
    pub payload: ::core::option::Option<crate::shim::Any>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, Eq, ::prost::Message, ::serde::Serialize, ::serde::Deserialize)]
pub struct Response {
    #[prost(string, tag = "1")]
    #[serde(alias = "requestID")]
    pub request_id: ::prost::alloc::string::String,
    #[prost(bool, tag = "2")]
    pub success: bool,
    #[prost(message, optional, tag = "3")]
    pub payload: ::core::option::Option<crate::shim::Any>,
}
/// Network Events
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, Eq, ::prost::Message, ::serde::Serialize, ::serde::Deserialize)]
pub struct NetworkEvent {
    #[prost(oneof = "network_event::EventType", tags = "1, 2, 3, 4, 5")]
    pub event_type: ::core::option::Option<network_event::EventType>,
}
/// Nested message and enum types in `NetworkEvent`.
pub mod network_event {
    use rs_derive::CosmwasmExt;
    #[allow(clippy::derive_partial_eq_without_eq)]
    #[derive(Clone, PartialEq, Eq, ::prost::Oneof, ::serde::Serialize, ::serde::Deserialize)]
    pub enum EventType {
        #[prost(message, tag = "1")]
        PeerConnected(super::PeerConnected),
        #[prost(message, tag = "2")]
        PeerDisconnected(super::PeerDisconnected),
        #[prost(message, tag = "3")]
        MessageReceived(super::MessageReceived),
        #[prost(message, tag = "4")]
        TopologyChanged(super::TopologyChanged),
        #[prost(message, tag = "5")]
        Error(super::NetworkError),
    }
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, Eq, ::prost::Message, ::serde::Serialize, ::serde::Deserialize)]
pub struct PeerConnected {
    #[prost(bytes = "vec", tag = "1")]
    #[serde(alias = "peerID")]
    #[serde(
        serialize_with = "crate::serde::as_base64_encoded_string::serialize",
        deserialize_with = "crate::serde::as_base64_encoded_string::deserialize"
    )]
    pub peer_id: ::prost::alloc::vec::Vec<u8>,
    #[prost(message, optional, tag = "2")]
    pub node_info: ::core::option::Option<NodeInfo>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, Eq, ::prost::Message, ::serde::Serialize, ::serde::Deserialize)]
pub struct PeerDisconnected {
    #[prost(bytes = "vec", tag = "1")]
    #[serde(alias = "peerID")]
    #[serde(
        serialize_with = "crate::serde::as_base64_encoded_string::serialize",
        deserialize_with = "crate::serde::as_base64_encoded_string::deserialize"
    )]
    pub peer_id: ::prost::alloc::vec::Vec<u8>,
    #[prost(string, tag = "2")]
    pub reason: ::prost::alloc::string::String,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, Eq, ::prost::Message, ::serde::Serialize, ::serde::Deserialize)]
pub struct MessageReceived {
    #[prost(bytes = "vec", tag = "1")]
    #[serde(
        serialize_with = "crate::serde::as_base64_encoded_string::serialize",
        deserialize_with = "crate::serde::as_base64_encoded_string::deserialize"
    )]
    pub from: ::prost::alloc::vec::Vec<u8>,
    #[prost(message, optional, tag = "2")]
    pub message: ::core::option::Option<NetworkMessage>,
    #[prost(uint32, tag = "3")]
    #[serde(
        serialize_with = "crate::serde::as_str::serialize",
        deserialize_with = "crate::serde::as_str::deserialize"
    )]
    pub channel: u32,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, Eq, ::prost::Message, ::serde::Serialize, ::serde::Deserialize)]
pub struct TopologyChanged {
    #[prost(message, optional, tag = "1")]
    pub topology: ::core::option::Option<super::super::types::v1::NetworkTopology>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, Eq, ::prost::Message, ::serde::Serialize, ::serde::Deserialize)]
pub struct NetworkError {
    #[prost(string, tag = "1")]
    pub error: ::prost::alloc::string::String,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, Eq, ::prost::Message, ::serde::Serialize, ::serde::Deserialize)]
pub struct DeploymentConfig {
    #[prost(message, optional, tag = "1")]
    pub bootstrap_method: ::core::option::Option<BootstrapMethod>,
    #[prost(string, optional, tag = "2")]
    pub ssh_key_path: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(string, optional, tag = "3")]
    pub env_file_path: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(bool, tag = "4")]
    pub install_dependencies: bool,
    #[prost(bool, tag = "5")]
    pub auto_start_services: bool,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, Eq, ::prost::Message, ::serde::Serialize, ::serde::Deserialize)]
pub struct BootstrapMethod {
    #[prost(oneof = "bootstrap_method::Method", tags = "1, 2, 3")]
    pub method: ::core::option::Option<bootstrap_method::Method>,
}
/// Nested message and enum types in `BootstrapMethod`.
pub mod bootstrap_method {
    use rs_derive::CosmwasmExt;
    #[allow(clippy::derive_partial_eq_without_eq)]
    #[derive(Clone, PartialEq, Eq, ::prost::Oneof, ::serde::Serialize, ::serde::Deserialize)]
    pub enum Method {
        #[prost(message, tag = "1")]
        SshFullInstall(super::SshFullInstall),
        #[prost(message, tag = "2")]
        CloudDeployment(super::CloudDeployment),
        #[prost(message, tag = "3")]
        LocalDevelopment(super::LocalDevelopment),
    }
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(
    Clone, Copy, PartialEq, Eq, ::prost::Message, ::serde::Serialize, ::serde::Deserialize,
)]
pub struct SshFullInstall {}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, Eq, ::prost::Message, ::serde::Serialize, ::serde::Deserialize)]
pub struct CloudDeployment {
    #[prost(string, tag = "1")]
    pub provider: ::prost::alloc::string::String,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(
    Clone, Copy, PartialEq, Eq, ::prost::Message, ::serde::Serialize, ::serde::Deserialize,
)]
pub struct LocalDevelopment {}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(
    Clone, Copy, PartialEq, Eq, ::prost::Message, ::serde::Serialize, ::serde::Deserialize,
)]
pub struct NodeType {
    #[prost(oneof = "node_type::Method", tags = "1, 2, 3, 4")]
    pub method: ::core::option::Option<node_type::Method>,
}
/// Nested message and enum types in `NodeType`.
pub mod node_type {
    use rs_derive::CosmwasmExt;
    #[allow(clippy::derive_partial_eq_without_eq)]
    #[derive(
        Clone, Copy, PartialEq, Eq, ::prost::Oneof, ::serde::Serialize, ::serde::Deserialize,
    )]
    pub enum Method {
        #[prost(message, tag = "1")]
        Coordinator(super::Coordinator),
        #[prost(message, tag = "2")]
        Executor(super::Executor),
        #[prost(message, tag = "3")]
        Referee(super::Referee),
        #[prost(message, tag = "4")]
        Development(super::Development),
    }
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(
    Clone, Copy, PartialEq, Eq, ::prost::Message, ::serde::Serialize, ::serde::Deserialize,
)]
pub struct Coordinator {}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(
    Clone, Copy, PartialEq, Eq, ::prost::Message, ::serde::Serialize, ::serde::Deserialize,
)]
pub struct Executor {}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(
    Clone, Copy, PartialEq, Eq, ::prost::Message, ::serde::Serialize, ::serde::Deserialize,
)]
pub struct Referee {}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(
    Clone, Copy, PartialEq, Eq, ::prost::Message, ::serde::Serialize, ::serde::Deserialize,
)]
pub struct Development {}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
#[derive(::serde::Serialize, ::serde::Deserialize)]
pub enum HostOs {
    Unspecified = 0,
    Linux = 1,
    Macos = 2,
    Windows = 3,
    ContainerLinux = 4,
}
impl HostOs {
    /// String value of the enum field names used in the ProtoBuf definition.
    ///
    /// The values are not transformed in any way and thus are considered stable
    /// (if the ProtoBuf definition does not change) and safe for programmatic use.
    pub fn as_str_name(&self) -> &'static str {
        match self {
            HostOs::Unspecified => "HOST_OS_UNSPECIFIED",
            HostOs::Linux => "HOST_OS_LINUX",
            HostOs::Macos => "HOST_OS_MACOS",
            HostOs::Windows => "HOST_OS_WINDOWS",
            HostOs::ContainerLinux => "HOST_OS_CONTAINER_LINUX",
        }
    }
    /// Creates an enum from field names used in the ProtoBuf definition.
    pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
        match value {
            "HOST_OS_UNSPECIFIED" => Some(Self::Unspecified),
            "HOST_OS_LINUX" => Some(Self::Linux),
            "HOST_OS_MACOS" => Some(Self::Macos),
            "HOST_OS_WINDOWS" => Some(Self::Windows),
            "HOST_OS_CONTAINER_LINUX" => Some(Self::ContainerLinux),
            _ => None,
        }
    }
}
