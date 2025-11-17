mod anthro;
mod openai;

/// Joints are flexible modules that are able to perform as middleware between the orchestration and the external api call
pub use anthro::AnthropticJoint;
pub use openai::OpenAiJoint;
