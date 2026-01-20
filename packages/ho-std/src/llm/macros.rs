/// Macro for defining LLM entity providers with automatic trait implementations
///
/// This macro generates all the boilerplate for LLM providers, allowing declarative
/// definitions that can be used in scripting, API clients, and internal integrations.
///
/// # Usage
///
/// ```ignore
/// use ho_std::llm_entity;
///
/// llm_entity! {
///     OpenAI {
///         name: "openai",
///         env_key: "OPENAI_API_KEY",
///         base_url: "https://api.openai.com/v1",
///         models: ["gpt-4", "gpt-3.5-turbo"],
///         api_type: OpenAICompatible,
///     }
/// }
/// ```
#[macro_export]
macro_rules! llm_entity {
    // Case 1: Inline model list: models: ["m1", "m2"]
    (
        $name:ident {
            name: $provider_name:expr,
            env_key: $env_key:expr,
            base_url: $base_url:expr,
            models: [$($model:expr),* $(,)?],
            api_type: $api_type:ident,
            $($extra:tt)*
        }
    ) => {
        pub struct $name {
            api_key: Option<String>,
            key_accessor: Option<std::sync::Arc<dyn $crate::traits::ApiKeyMethod>>,
            extra_models: Vec<String>,
        }

        impl $name {
            pub fn new(api_key: Option<String>) -> Self {
                Self {
                    api_key,
                    key_accessor: None,
                    extra_models: Vec::new(),
                }
            }

            pub fn with_accessor(key_accessor: std::sync::Arc<dyn $crate::traits::ApiKeyMethod>) -> Self {
                Self {
                    api_key: None,
                    key_accessor: Some(key_accessor),
                    extra_models: Vec::new(),
                }
            }

            pub const ENV_KEY: &'static str = $env_key;
            pub const PROVIDER_NAME: &'static str = $provider_name;
            pub const BASE_URL: &'static str = $base_url;
            pub const MODELS: &'static [&'static str] = &[$($model),*];
        }

        $crate::llm_entity!(@impl_traits $name, $provider_name, $api_type);
    };

    // Case 2: Constant model list: models: MY_MODELS_CONST
    (
        $name:ident {
            name: $provider_name:expr,
            env_key: $env_key:expr,
            base_url: $base_url:expr,
            models: $models_const:path,
            api_type: $api_type:ident,
            $($extra:tt)*
        }
    ) => {
        pub struct $name {
            api_key: Option<String>,
            key_accessor: Option<std::sync::Arc<dyn $crate::traits::ApiKeyMethod>>,
            extra_models: Vec<String>,
        }

        impl $name {
            pub fn new(api_key: Option<String>) -> Self {
                Self {
                    api_key,
                    key_accessor: None,
                    extra_models: Vec::new(),
                }
            }

            pub fn with_accessor(key_accessor: std::sync::Arc<dyn $crate::traits::ApiKeyMethod>) -> Self {
                Self {
                    api_key: None,
                    key_accessor: Some(key_accessor),
                    extra_models: Vec::new(),
                }
            }

            pub const ENV_KEY: &'static str = $env_key;
            pub const PROVIDER_NAME: &'static str = $provider_name;
            pub const BASE_URL: &'static str = $base_url;
            pub const MODELS: &'static [&'static str] = $models_const;
        }

        $crate::llm_entity!(@impl_traits $name, $provider_name, $api_type);
    };

    // Shared trait impls to reduce duplication
    (@impl_traits $name:ident, $provider_name:expr, $api_type:ident) => {
        #[async_trait::async_trait]
        impl $crate::traits::ApiKeyProvider for $name {
            async fn get_api_key(&self) -> $crate::llm::HoResult<String> {
                if let Some(accessor) = &self.key_accessor {
                    if let Ok(Some(key)) = accessor.get_key($provider_name).await {
                        return Ok(key);
                    }
                }
                if let Some(key) = &self.api_key {
                    return Ok(key.clone());
                }
                std::env::var(Self::ENV_KEY)
                    .map_err(|_| $crate::error::HoError::Llm(
                        format!("{} API key not configured", $provider_name)
                    ))
            }
        }

        #[async_trait::async_trait]
        impl $crate::traits::LlmProviderTrait for $name {
            fn name(&self) -> &str {
                Self::PROVIDER_NAME
            }

            fn base_url(&self) -> &str {
                Self::BASE_URL
            }

            fn supports_model(&self, model: &str) -> bool {
                Self::MODELS.contains(&model)
                    || model.contains(Self::PROVIDER_NAME)
                    || self.extra_models.iter().any(|m| m == model)
            }

            fn supported_models(&self) -> &[&str] {
                Self::MODELS
            }

            async fn call(
                &self,
                client: &reqwest::Client,
                request: &crate::orchestrate::PromptRequest,
            ) -> $crate::llm::HoResult<crate::orchestrate::PromptResponse> {
                $crate::llm::joints::$api_type::handle_request(
                    self,
                    client,
                    request,
                    Self::BASE_URL,
                    Self::PROVIDER_NAME,
                ).await
            }

            fn is_configured(&self) -> bool {
                self.api_key.is_some()
                    || self.key_accessor.is_some()
                    || std::env::var(Self::ENV_KEY).is_ok()
            }

            fn set_api_key(&mut self, api_key: String) {
                self.api_key = Some(api_key);
            }

            fn add_supported_model(&mut self, model: String) {
                self.extra_models.push(model);
            }
        }
    };
}
