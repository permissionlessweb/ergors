"""API keys configuration schema for api-keys.json."""

from typing import Optional, Dict, Any
from pydantic import BaseModel, Field, validator, SecretStr
import os


class ProviderAPIKey(BaseModel):
    """Individual provider API key configuration."""
    key: Optional[SecretStr] = Field(None, description="API key (can use env var)")
    env_var: Optional[str] = Field(None, description="Environment variable name")
    endpoint_override: Optional[str] = Field(None, description="Custom endpoint URL")
    
    @validator('key', pre=True)
    def resolve_env_var(cls, v, values):
        """Resolve environment variable if specified."""
        if 'env_var' in values and values['env_var']:
            env_value = os.getenv(values['env_var'])
            if env_value:
                return SecretStr(env_value)
        return v if v else None
    
    def get_key_value(self) -> Optional[str]:
        """Get the actual key value, resolving env vars."""
        if self.env_var:
            env_value = os.getenv(self.env_var)
            if env_value:
                return env_value
        return self.key.get_secret_value() if self.key else None
    
    class Config:
        """Pydantic configuration."""
        json_encoders = {
            SecretStr: lambda v: v.get_secret_value() if v else None
        }


class OpenAIKeys(ProviderAPIKey):
    """OpenAI-specific API configuration."""
    organization_id: Optional[str] = Field(None, description="OpenAI organization ID")
    model_permissions: Dict[str, bool] = Field(default_factory=dict, description="Model access permissions")


class AnthropicKeys(ProviderAPIKey):
    """Anthropic-specific API configuration."""
    version: str = Field(default="2023-06-01", description="API version")


class GroqKeys(ProviderAPIKey):
    """Groq-specific API configuration."""
    rate_limit_override: Optional[int] = Field(None, description="Custom rate limit")


class OllamaKeys(BaseModel):
    """Ollama-specific configuration (usually no API key needed)."""
    endpoint: str = Field(default="http://localhost:11434", description="Ollama endpoint")
    auth_enabled: bool = Field(default=False, description="Whether authentication is enabled")
    username: Optional[str] = Field(None, description="Username if auth enabled")
    password: Optional[SecretStr] = Field(None, description="Password if auth enabled")


class LlamafileKeys(BaseModel):
    """Llamafile-specific configuration."""
    endpoint: str = Field(default="http://localhost:8080", description="Llamafile endpoint")
    model_path: Optional[str] = Field(None, description="Path to model file")


class APIKeys(BaseModel):
    """Complete API keys configuration."""
    openai: Optional[OpenAIKeys] = Field(None, description="OpenAI API configuration")
    anthropic: Optional[AnthropicKeys] = Field(None, description="Anthropic API configuration")
    groq: Optional[GroqKeys] = Field(None, description="Groq API configuration")
    ollama: Optional[OllamaKeys] = Field(None, description="Ollama configuration")
    llamafile: Optional[LlamafileKeys] = Field(None, description="Llamafile configuration")
    custom: Dict[str, ProviderAPIKey] = Field(default_factory=dict, description="Custom provider keys")
    
    @validator('custom')
    def validate_custom_keys(cls, v):
        """Validate custom provider keys."""
        for provider_name, config in v.items():
            if not isinstance(config, (dict, ProviderAPIKey)):
                raise ValueError(f"Invalid configuration for custom provider {provider_name}")
        return v
    
    def get_provider_config(self, provider_name: str) -> Optional[Any]:
        """Get configuration for a specific provider."""
        # Check standard providers first
        if hasattr(self, provider_name.lower()):
            return getattr(self, provider_name.lower())
        # Check custom providers
        return self.custom.get(provider_name)
    
    def mask_sensitive_data(self) -> Dict[str, Any]:
        """Return a version of the config with masked sensitive data."""
        data = self.dict()
        
        def mask_dict(d: Dict[str, Any]) -> Dict[str, Any]:
            masked = {}
            for k, v in d.items():
                if k in ['key', 'password'] and v:
                    masked[k] = "***MASKED***"
                elif isinstance(v, dict):
                    masked[k] = mask_dict(v)
                else:
                    masked[k] = v
            return masked
        
        return mask_dict(data)
    
    class Config:
        """Pydantic configuration."""
        validate_assignment = True
        extra = "forbid"
