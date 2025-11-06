use rs_derive::CosmwasmExt;
/// Cosmic Orchestration Types
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct CosmicTask {
    #[prost(string, tag = "1")]
    pub id: ::prost::alloc::string::String,
    #[prost(enumeration = "OrchestrateTask", tag = "2")]
    pub task_type: i32,
    #[prost(enumeration = "CosmicTaskStatus", tag = "3")]
    pub status: i32,
    #[prost(string, tag = "4")]
    pub prompt: ::prost::alloc::string::String,
    #[prost(message, optional, tag = "5")]
    pub fractal_requirements: ::core::option::Option<FractalRequirements>,
    #[prost(message, optional, tag = "6")]
    pub created_at: ::core::option::Option<crate::shim::Timestamp>,
    #[prost(message, optional, tag = "7")]
    pub updated_at: ::core::option::Option<crate::shim::Timestamp>,
    #[prost(message, optional, tag = "8")]
    pub result: ::core::option::Option<crate::shim::Any>,
    #[prost(string, optional, tag = "9")]
    pub error: ::core::option::Option<::prost::alloc::string::String>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct CosmicContext {
    #[prost(string, tag = "1")]
    pub task_id: ::prost::alloc::string::String,
    #[prost(string, tag = "2")]
    pub user_input: ::prost::alloc::string::String,
    #[prost(uint32, tag = "3")]
    pub current_step: u32,
    #[prost(uint32, tag = "4")]
    pub total_steps: u32,
    #[prost(uint32, tag = "5")]
    pub fractal_level: u32,
    #[prost(string, tag = "6")]
    pub golden_ratio_state: ::prost::alloc::string::String,
    #[prost(message, repeated, tag = "7")]
    pub previous_responses: ::prost::alloc::vec::Vec<PromptResponse>,
    #[prost(map = "string, message", tag = "8")]
    pub cosmic_metadata:
        ::std::collections::HashMap<::prost::alloc::string::String, ::prost_types::Struct>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct FractalRequirements {
    #[prost(message, optional, tag = "1")]
    pub context: ::core::option::Option<CosmicContext>,
    #[prost(uint32, tag = "2")]
    pub recursion_depth: u32,
    #[prost(double, tag = "3")]
    pub self_similarity_threshold: f64,
    #[prost(bool, tag = "4")]
    pub golden_ratio_compliance: bool,
    #[prost(double, tag = "5")]
    pub fractal_dimension_target: f64,
    #[prost(bool, tag = "6")]
    pub mobius_continuity: bool,
    #[prost(double, tag = "7")]
    pub fractal_coherence: f64,
    #[prost(string, repeated, tag = "8")]
    pub expansion_criteria: ::prost::alloc::vec::Vec<::prost::alloc::string::String>,
}
/// LLM and Prompt Types
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct PromptRequest {
    #[prost(message, repeated, tag = "1")]
    pub messages: ::prost::alloc::vec::Vec<PromptMessage>,
    #[prost(string, tag = "2")]
    pub model: ::prost::alloc::string::String,
    #[prost(message, optional, tag = "3")]
    pub context: ::core::option::Option<PromptContext>,
    #[prost(message, optional, tag = "4")]
    pub llm_config: ::core::option::Option<LlmPromptConfig>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message, ::serde::Serialize, ::serde::Deserialize)]
pub struct PromptResponse {
    #[prost(bytes = "vec", tag = "1")]
    pub id: ::prost::alloc::vec::Vec<u8>,
    #[prost(string, tag = "2")]
    pub provider: ::prost::alloc::string::String,
    #[prost(string, tag = "3")]
    pub model: ::prost::alloc::string::String,
    #[prost(string, tag = "4")]
    pub prompt: ::prost::alloc::string::String,
    #[prost(string, tag = "5")]
    pub response: ::prost::alloc::string::String,
    #[prost(message, optional, tag = "6")]
    pub timestamp: ::core::option::Option<crate::shim::Timestamp>,
    #[prost(message, optional, tag = "7")]
    pub tokens_used: ::core::option::Option<TokenUsage>,
    #[prost(double, optional, tag = "8")]
    pub cost: ::core::option::Option<f64>,
    #[prost(uint64, optional, tag = "9")]
    pub latency_ms: ::core::option::Option<u64>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, Eq, ::prost::Message, ::serde::Serialize, ::serde::Deserialize)]
pub struct PromptMessage {
    #[prost(string, tag = "1")]
    pub role: ::prost::alloc::string::String,
    #[prost(string, tag = "2")]
    pub content: ::prost::alloc::string::String,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, Eq, ::prost::Message, ::serde::Serialize, ::serde::Deserialize)]
pub struct PromptContext {
    #[prost(string, optional, tag = "1")]
    #[serde(alias = "sessionID")]
    pub session_id: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(string, optional, tag = "2")]
    #[serde(alias = "userID")]
    pub user_id: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(string, optional, tag = "3")]
    #[serde(alias = "threadID")]
    pub thread_id: ::core::option::Option<::prost::alloc::string::String>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(
    Clone, Copy, PartialEq, Eq, ::prost::Message, ::serde::Serialize, ::serde::Deserialize,
)]
pub struct TokenUsage {
    #[prost(uint32, tag = "1")]
    #[serde(
        serialize_with = "crate::serde::as_str::serialize",
        deserialize_with = "crate::serde::as_str::deserialize"
    )]
    pub prompt: u32,
    #[prost(uint32, tag = "2")]
    #[serde(
        serialize_with = "crate::serde::as_str::serialize",
        deserialize_with = "crate::serde::as_str::deserialize"
    )]
    pub completion: u32,
    #[prost(uint32, tag = "3")]
    #[serde(
        serialize_with = "crate::serde::as_str::serialize",
        deserialize_with = "crate::serde::as_str::deserialize"
    )]
    pub total: u32,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct LlmPromptConfig {
    #[prost(double, tag = "1")]
    pub temperature: f64,
    #[prost(uint32, tag = "2")]
    pub max_tokens: u32,
    #[prost(double, tag = "3")]
    pub top_p: f64,
    #[prost(string, repeated, tag = "4")]
    pub stop_sequences: ::prost::alloc::vec::Vec<::prost::alloc::string::String>,
}
/// LLM Provider Types
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, Eq, ::prost::Message, ::serde::Serialize, ::serde::Deserialize)]
pub struct LlmProvider {
    #[prost(string, tag = "1")]
    pub name: ::prost::alloc::string::String,
    #[prost(string, tag = "2")]
    pub base_url: ::prost::alloc::string::String,
    #[prost(string, repeated, tag = "3")]
    pub supported_models: ::prost::alloc::vec::Vec<::prost::alloc::string::String>,
    #[prost(enumeration = "LlmModel", tag = "4")]
    #[serde(
        serialize_with = "crate::serde::as_str::serialize",
        deserialize_with = "crate::serde::as_str::deserialize"
    )]
    pub provider_type: i32,
}
/// Local config structure that matches the existing implementation
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, Eq, ::prost::Message, ::serde::Serialize, ::serde::Deserialize)]
pub struct LocalLlmConfig {
    #[prost(uint64, tag = "1")]
    #[serde(
        serialize_with = "crate::serde::as_str::serialize",
        deserialize_with = "crate::serde::as_str::deserialize"
    )]
    pub timeout_seconds: u64,
    #[prost(string, tag = "2")]
    pub api_keys_file: ::prost::alloc::string::String,
}
/// Configuration Types
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, Eq, ::prost::Message, ::serde::Serialize, ::serde::Deserialize)]
pub struct HoConfig {
    #[prost(message, optional, tag = "1")]
    pub network: ::core::option::Option<super::super::network::v1::NetworkConfig>,
    #[prost(message, optional, tag = "2")]
    pub identity: ::core::option::Option<super::super::network::v1::NodeIdentity>,
    #[prost(message, optional, tag = "3")]
    pub storage: ::core::option::Option<StorageConfig>,
    #[prost(message, optional, tag = "4")]
    pub llm: ::core::option::Option<LlmRouterConfig>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, Eq, ::prost::Message, ::serde::Serialize, ::serde::Deserialize)]
pub struct StorageConfig {
    #[prost(string, tag = "1")]
    pub data_dir: ::prost::alloc::string::String,
    #[prost(uint32, tag = "2")]
    #[serde(
        serialize_with = "crate::serde::as_str::serialize",
        deserialize_with = "crate::serde::as_str::deserialize"
    )]
    pub max_size_mb: u32,
    #[prost(bool, tag = "3")]
    pub enable_compression: bool,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, Eq, ::prost::Message, ::serde::Serialize, ::serde::Deserialize)]
pub struct OpenAiRequest {
    #[prost(string, tag = "1")]
    pub model: ::prost::alloc::string::String,
    #[prost(message, repeated, tag = "2")]
    pub messages: ::prost::alloc::vec::Vec<OpenAiMessage>,
    #[prost(uint32, optional, tag = "3")]
    pub temperature: ::core::option::Option<u32>,
    #[prost(uint32, optional, tag = "4")]
    pub max_tokens: ::core::option::Option<u32>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(
    Clone, Copy, PartialEq, Eq, ::prost::Message, ::serde::Serialize, ::serde::Deserialize,
)]
pub struct OpenAiUsage {
    #[prost(uint32, tag = "1")]
    #[serde(
        serialize_with = "crate::serde::as_str::serialize",
        deserialize_with = "crate::serde::as_str::deserialize"
    )]
    pub prompt_tokens: u32,
    #[prost(uint32, tag = "2")]
    #[serde(
        serialize_with = "crate::serde::as_str::serialize",
        deserialize_with = "crate::serde::as_str::deserialize"
    )]
    pub completion_tokens: u32,
    #[prost(uint32, tag = "3")]
    #[serde(
        serialize_with = "crate::serde::as_str::serialize",
        deserialize_with = "crate::serde::as_str::deserialize"
    )]
    pub total_tokens: u32,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, Eq, ::prost::Message, ::serde::Serialize, ::serde::Deserialize)]
pub struct OpenAiMessage {
    #[prost(string, tag = "1")]
    pub role: ::prost::alloc::string::String,
    #[prost(string, tag = "2")]
    pub content: ::prost::alloc::string::String,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, Eq, ::prost::Message, ::serde::Serialize, ::serde::Deserialize)]
pub struct OpenAiResponse {
    #[prost(message, repeated, tag = "1")]
    pub choices: ::prost::alloc::vec::Vec<OpenAiChoice>,
    #[prost(message, optional, tag = "2")]
    pub usage: ::core::option::Option<OpenAiUsage>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, Eq, ::prost::Message, ::serde::Serialize, ::serde::Deserialize)]
pub struct OpenAiChoice {
    #[prost(message, optional, tag = "1")]
    pub message: ::core::option::Option<OpenAiMessage>,
}
/// Llm config is the global configuration of
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, Eq, ::prost::Message, ::serde::Serialize, ::serde::Deserialize)]
pub struct LlmRouterConfig {
    #[prost(string, tag = "1")]
    pub api_keys_file: ::prost::alloc::string::String,
    #[prost(message, repeated, tag = "2")]
    pub entities: ::prost::alloc::vec::Vec<LlmEntity>,
    #[prost(enumeration = "ModelSelectionStrategy", tag = "3")]
    #[serde(
        serialize_with = "crate::serde::as_str::serialize",
        deserialize_with = "crate::serde::as_str::deserialize"
    )]
    pub default_strategy: i32,
    #[prost(uint64, tag = "4")]
    #[serde(
        serialize_with = "crate::serde::as_str::serialize",
        deserialize_with = "crate::serde::as_str::deserialize"
    )]
    pub timeout_seconds: u64,
    #[prost(uint32, tag = "5")]
    #[serde(
        serialize_with = "crate::serde::as_str::serialize",
        deserialize_with = "crate::serde::as_str::deserialize"
    )]
    pub max_retries: u32,
    #[prost(uint32, tag = "6")]
    #[serde(
        serialize_with = "crate::serde::as_str::serialize",
        deserialize_with = "crate::serde::as_str::deserialize"
    )]
    pub default_entity: u32,
}
/// / LlmEntity is a single llm model entity.
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, Eq, ::prost::Message, ::serde::Serialize, ::serde::Deserialize)]
pub struct LlmEntity {
    #[prost(string, tag = "1")]
    pub name: ::prost::alloc::string::String,
    #[prost(string, tag = "2")]
    pub base_url: ::prost::alloc::string::String,
    #[prost(string, repeated, tag = "3")]
    pub models: ::prost::alloc::vec::Vec<::prost::alloc::string::String>,
    #[prost(string, tag = "4")]
    pub default_model: ::prost::alloc::string::String,
    /// Using uint32 for u8
    #[prost(uint32, tag = "5")]
    #[serde(
        serialize_with = "crate::serde::as_str::serialize",
        deserialize_with = "crate::serde::as_str::deserialize"
    )]
    pub priority: u32,
    #[prost(bool, tag = "6")]
    pub enabled: bool,
    #[prost(enumeration = "ModelSelectionStrategy", tag = "7")]
    #[serde(
        serialize_with = "crate::serde::as_str::serialize",
        deserialize_with = "crate::serde::as_str::deserialize"
    )]
    pub default_strategy: i32,
    #[prost(uint64, tag = "8")]
    #[serde(
        serialize_with = "crate::serde::as_str::serialize",
        deserialize_with = "crate::serde::as_str::deserialize"
    )]
    pub timeout_seconds: u64,
    #[prost(uint32, tag = "9")]
    #[serde(
        serialize_with = "crate::serde::as_str::serialize",
        deserialize_with = "crate::serde::as_str::deserialize"
    )]
    pub max_retries: u32,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, Eq, ::prost::Message, ::serde::Serialize, ::serde::Deserialize)]
pub struct LoggingConfig {
    #[prost(string, tag = "1")]
    pub level: ::prost::alloc::string::String,
    #[prost(string, optional, tag = "2")]
    pub file: ::core::option::Option<::prost::alloc::string::String>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
#[derive(::serde::Serialize, ::serde::Deserialize)]
pub enum OrchestrateTask {
    Unspecified = 0,
    Bootstrap = 1,
    Recursive = 2,
}
impl OrchestrateTask {
    /// String value of the enum field names used in the ProtoBuf definition.
    ///
    /// The values are not transformed in any way and thus are considered stable
    /// (if the ProtoBuf definition does not change) and safe for programmatic use.
    pub fn as_str_name(&self) -> &'static str {
        match self {
            OrchestrateTask::Unspecified => "ORCHESTRATE_TASK_UNSPECIFIED",
            OrchestrateTask::Bootstrap => "ORCHESTRATE_TASK_BOOTSTRAP",
            OrchestrateTask::Recursive => "ORCHESTRATE_TASK_RECURSIVE",
        }
    }
    /// Creates an enum from field names used in the ProtoBuf definition.
    pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
        match value {
            "ORCHESTRATE_TASK_UNSPECIFIED" => Some(Self::Unspecified),
            "ORCHESTRATE_TASK_BOOTSTRAP" => Some(Self::Bootstrap),
            "ORCHESTRATE_TASK_RECURSIVE" => Some(Self::Recursive),
            _ => None,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
#[derive(::serde::Serialize, ::serde::Deserialize)]
pub enum CosmicTaskStatus {
    Unspecified = 0,
    Pending = 1,
    Running = 2,
    Completed = 3,
    Failed = 4,
    FractalExpansion = 5,
    GeometricValidation = 6,
}
impl CosmicTaskStatus {
    /// String value of the enum field names used in the ProtoBuf definition.
    ///
    /// The values are not transformed in any way and thus are considered stable
    /// (if the ProtoBuf definition does not change) and safe for programmatic use.
    pub fn as_str_name(&self) -> &'static str {
        match self {
            CosmicTaskStatus::Unspecified => "COSMIC_TASK_STATUS_UNSPECIFIED",
            CosmicTaskStatus::Pending => "COSMIC_TASK_STATUS_PENDING",
            CosmicTaskStatus::Running => "COSMIC_TASK_STATUS_RUNNING",
            CosmicTaskStatus::Completed => "COSMIC_TASK_STATUS_COMPLETED",
            CosmicTaskStatus::Failed => "COSMIC_TASK_STATUS_FAILED",
            CosmicTaskStatus::FractalExpansion => "COSMIC_TASK_STATUS_FRACTAL_EXPANSION",
            CosmicTaskStatus::GeometricValidation => "COSMIC_TASK_STATUS_GEOMETRIC_VALIDATION",
        }
    }
    /// Creates an enum from field names used in the ProtoBuf definition.
    pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
        match value {
            "COSMIC_TASK_STATUS_UNSPECIFIED" => Some(Self::Unspecified),
            "COSMIC_TASK_STATUS_PENDING" => Some(Self::Pending),
            "COSMIC_TASK_STATUS_RUNNING" => Some(Self::Running),
            "COSMIC_TASK_STATUS_COMPLETED" => Some(Self::Completed),
            "COSMIC_TASK_STATUS_FAILED" => Some(Self::Failed),
            "COSMIC_TASK_STATUS_FRACTAL_EXPANSION" => Some(Self::FractalExpansion),
            "COSMIC_TASK_STATUS_GEOMETRIC_VALIDATION" => Some(Self::GeometricValidation),
            _ => None,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
#[derive(::serde::Serialize, ::serde::Deserialize)]
pub enum LlmModel {
    AkashChat = 0,
    OllamaLocal = 1,
    KimiResearch = 2,
    Grok = 3,
    OpenAi = 4,
    Anthropic = 5,
    Custom = 6,
}
impl LlmModel {
    /// String value of the enum field names used in the ProtoBuf definition.
    ///
    /// The values are not transformed in any way and thus are considered stable
    /// (if the ProtoBuf definition does not change) and safe for programmatic use.
    pub fn as_str_name(&self) -> &'static str {
        match self {
            LlmModel::AkashChat => "AkashChat",
            LlmModel::OllamaLocal => "OllamaLocal",
            LlmModel::KimiResearch => "KimiResearch",
            LlmModel::Grok => "Grok",
            LlmModel::OpenAi => "OpenAI",
            LlmModel::Anthropic => "Anthropic",
            LlmModel::Custom => "Custom",
        }
    }
    /// Creates an enum from field names used in the ProtoBuf definition.
    pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
        match value {
            "AkashChat" => Some(Self::AkashChat),
            "OllamaLocal" => Some(Self::OllamaLocal),
            "KimiResearch" => Some(Self::KimiResearch),
            "Grok" => Some(Self::Grok),
            "OpenAI" => Some(Self::OpenAi),
            "Anthropic" => Some(Self::Anthropic),
            "Custom" => Some(Self::Custom),
            _ => None,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
#[derive(::serde::Serialize, ::serde::Deserialize)]
pub enum ModelSelectionStrategy {
    Unspecified = 0,
    /// Always use highest priority available
    Priority = 1,
    /// Round-robin across available providers
    RoundRobin = 2,
    /// Golden ratio weighted selection
    GoldenRatio = 3,
    /// Load-based selection
    LoadBalanced = 4,
}
impl ModelSelectionStrategy {
    /// String value of the enum field names used in the ProtoBuf definition.
    ///
    /// The values are not transformed in any way and thus are considered stable
    /// (if the ProtoBuf definition does not change) and safe for programmatic use.
    pub fn as_str_name(&self) -> &'static str {
        match self {
            ModelSelectionStrategy::Unspecified => "MODEL_SELECTION_STRATEGY_UNSPECIFIED",
            ModelSelectionStrategy::Priority => "MODEL_SELECTION_STRATEGY_PRIORITY",
            ModelSelectionStrategy::RoundRobin => "MODEL_SELECTION_STRATEGY_ROUND_ROBIN",
            ModelSelectionStrategy::GoldenRatio => "MODEL_SELECTION_STRATEGY_GOLDEN_RATIO",
            ModelSelectionStrategy::LoadBalanced => "MODEL_SELECTION_STRATEGY_LOAD_BALANCED",
        }
    }
    /// Creates an enum from field names used in the ProtoBuf definition.
    pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
        match value {
            "MODEL_SELECTION_STRATEGY_UNSPECIFIED" => Some(Self::Unspecified),
            "MODEL_SELECTION_STRATEGY_PRIORITY" => Some(Self::Priority),
            "MODEL_SELECTION_STRATEGY_ROUND_ROBIN" => Some(Self::RoundRobin),
            "MODEL_SELECTION_STRATEGY_GOLDEN_RATIO" => Some(Self::GoldenRatio),
            "MODEL_SELECTION_STRATEGY_LOAD_BALANCED" => Some(Self::LoadBalanced),
            _ => None,
        }
    }
}
