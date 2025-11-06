use rs_derive::CosmwasmExt;
/// Storage-specific types
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct StorageQuery {
    #[prost(string, optional, tag = "1")]
    pub session_id: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(string, optional, tag = "2")]
    pub user_id: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(message, optional, tag = "3")]
    pub start_time: ::core::option::Option<crate::shim::Timestamp>,
    #[prost(message, optional, tag = "4")]
    pub end_time: ::core::option::Option<crate::shim::Timestamp>,
    #[prost(uint32, optional, tag = "5")]
    pub limit: ::core::option::Option<u32>,
    #[prost(uint32, optional, tag = "6")]
    pub offset: ::core::option::Option<u32>,
    #[prost(map = "string, string", tag = "7")]
    pub filters:
        ::std::collections::HashMap<::prost::alloc::string::String, ::prost::alloc::string::String>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, Eq, ::prost::Message, ::serde::Serialize, ::serde::Deserialize)]
pub struct StorageIndex {
    #[prost(string, tag = "1")]
    pub key: ::prost::alloc::string::String,
    #[prost(string, tag = "2")]
    pub value: ::prost::alloc::string::String,
    #[prost(message, optional, tag = "3")]
    pub created_at: ::core::option::Option<crate::shim::Timestamp>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct StorageSnapshot {
    #[prost(string, tag = "1")]
    pub id: ::prost::alloc::string::String,
    #[prost(message, optional, tag = "2")]
    pub created_at: ::core::option::Option<crate::shim::Timestamp>,
    #[prost(string, tag = "3")]
    pub state_root: ::prost::alloc::string::String,
    #[prost(uint64, tag = "4")]
    pub version: u64,
    #[prost(map = "string, bytes", tag = "5")]
    pub data:
        ::std::collections::HashMap<::prost::alloc::string::String, ::prost::alloc::vec::Vec<u8>>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, Copy, PartialEq, ::prost::Message)]
pub struct StorageMetrics {
    #[prost(uint64, tag = "1")]
    pub total_entries: u64,
    #[prost(uint64, tag = "2")]
    pub storage_size_bytes: u64,
    #[prost(uint64, tag = "3")]
    pub index_size_bytes: u64,
    #[prost(message, optional, tag = "4")]
    pub last_compaction: ::core::option::Option<crate::shim::Timestamp>,
    #[prost(double, tag = "5")]
    pub fragmentation_ratio: f64,
}
/// Keep existing API types for backward compatibility
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, Eq, ::prost::Message, ::serde::Serialize, ::serde::Deserialize)]
pub struct QueryRequest {
    #[prost(string, optional, tag = "1")]
    #[serde(alias = "sessionID")]
    pub session_id: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(string, optional, tag = "2")]
    #[serde(alias = "userID")]
    pub user_id: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(message, optional, tag = "3")]
    pub start_time: ::core::option::Option<crate::shim::Timestamp>,
    #[prost(message, optional, tag = "4")]
    pub end_time: ::core::option::Option<crate::shim::Timestamp>,
    #[prost(uint32, optional, tag = "5")]
    pub limit: ::core::option::Option<u32>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, Eq, ::prost::Message, ::serde::Serialize, ::serde::Deserialize)]
pub struct HealthResponse {
    #[prost(string, tag = "1")]
    pub status: ::prost::alloc::string::String,
    #[prost(string, tag = "2")]
    pub version: ::prost::alloc::string::String,
    #[prost(uint64, tag = "3")]
    #[serde(
        serialize_with = "crate::serde::as_str::serialize",
        deserialize_with = "crate::serde::as_str::deserialize"
    )]
    pub uptime_seconds: u64,
    #[prost(string, tag = "4")]
    pub storage_status: ::prost::alloc::string::String,
    #[prost(string, optional, tag = "5")]
    pub network_status: ::core::option::Option<::prost::alloc::string::String>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, Eq, ::prost::Message, ::serde::Serialize, ::serde::Deserialize)]
pub struct ErrorResponse {
    #[prost(string, tag = "1")]
    pub error: ::prost::alloc::string::String,
    #[prost(string, tag = "2")]
    pub code: ::prost::alloc::string::String,
    #[prost(message, optional, tag = "3")]
    pub timestamp: ::core::option::Option<crate::shim::Timestamp>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, Eq, ::prost::Message, ::serde::Serialize, ::serde::Deserialize)]
pub struct BootstrapRequest {
    #[prost(string, tag = "1")]
    pub target_node: ::prost::alloc::string::String,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, Eq, ::prost::Message, ::serde::Serialize, ::serde::Deserialize)]
pub struct BootstrapResponse {
    #[prost(string, tag = "1")]
    #[serde(alias = "ID")]
    pub id: ::prost::alloc::string::String,
    #[prost(string, tag = "2")]
    pub target_node: ::prost::alloc::string::String,
    #[prost(string, tag = "3")]
    pub status: ::prost::alloc::string::String,
    #[prost(string, tag = "4")]
    pub summary: ::prost::alloc::string::String,
    #[prost(message, optional, tag = "5")]
    pub timestamp: ::core::option::Option<crate::shim::Timestamp>,
    #[prost(uint64, tag = "6")]
    #[serde(
        serialize_with = "crate::serde::as_str::serialize",
        deserialize_with = "crate::serde::as_str::deserialize"
    )]
    pub duration_ms: u64,
}
