"""Node configuration schema for config.toml."""

from typing import List, Optional, Dict, Any
from pydantic import BaseModel, Field, validator
from enum import Enum


class DeploymentMode(str, Enum):
    """Deployment mode options."""
    LOCAL = "local"
    REMOTE = "remote"
    HYBRID = "hybrid"


class LLMProviderType(str, Enum):
    """Supported LLM provider types."""
    OLLAMA = "ollama"
    OPENAI = "openai"
    ANTHROPIC = "anthropic"
    GROQ = "groq"
    LLAMAFILE = "llamafile"


class ConsensusEngine(str, Enum):
    """Consensus engine options."""
    COSMOS = "cosmos"
    CUSTOM = "custom"


class Identity(BaseModel):
    """Node identity configuration."""
    node_id: str = Field(..., pattern="^[a-f0-9]{64}$", description="64-char hex node ID")
    name: str = Field(..., min_length=1, max_length=128, description="Human-readable node name")
    role: str = Field(default="agent", description="Node role in the network")
    tags: List[str] = Field(default_factory=list, description="Node tags for discovery")
    
    @validator('node_id')
    def validate_node_id(cls, v):
        """Ensure node ID is valid ed25519 public key format."""
        if len(v) != 64 or not all(c in '0123456789abcdef' for c in v.lower()):
            raise ValueError('Node ID must be 64 hexadecimal characters')
        return v.lower()


class Deployment(BaseModel):
    """Deployment configuration."""
    mode: DeploymentMode = Field(default=DeploymentMode.LOCAL, description="Deployment mode")
    host: str = Field(default="localhost", description="Host address")
    ssh_key_path: Optional[str] = Field(None, description="Path to SSH key for remote deployment")
    remote_data_dir: Optional[str] = Field(None, description="Remote data directory path")
    resource_limits: Dict[str, Any] = Field(default_factory=dict, description="Resource allocation limits")


class Network(BaseModel):
    """P2P networking configuration."""
    api_port: int = Field(default=8080, ge=1024, le=65535, description="API server port")
    p2p_port: int = Field(default=26656, ge=1024, le=65535, description="P2P communication port")
    bootstrap_peers: List[str] = Field(default_factory=list, description="Initial peer addresses")
    enable_dht: bool = Field(default=True, description="Enable distributed hash table")
    max_peers: int = Field(default=50, ge=1, le=1000, description="Maximum peer connections")
    
    @validator('api_port')
    def validate_api_port(cls, v, values):
        """Ensure API port doesn't conflict with P2P port."""
        if 'p2p_port' in values and v == values['p2p_port']:
            raise ValueError('API port and P2P port must be different')
        return v


class Storage(BaseModel):
    """Cnidarium storage configuration."""
    data_dir: str = Field(default="./data", description="Local data directory")
    consensus_engine: ConsensusEngine = Field(default=ConsensusEngine.COSMOS, description="Consensus engine type")
    state_sync: bool = Field(default=True, description="Enable state synchronization")
    pruning_interval: int = Field(default=1000, ge=100, description="Block pruning interval")
    cache_size_mb: int = Field(default=512, ge=64, le=8192, description="Cache size in MB")


class LLMModel(BaseModel):
    """Individual LLM model configuration."""
    id: str = Field(..., description="Model identifier")
    context_length: int = Field(default=4096, ge=512, description="Context window size")
    capabilities: List[str] = Field(default_factory=list, description="Model capabilities")


class LLMProvider(BaseModel):
    """LLM provider configuration."""
    name: str = Field(..., description="Provider name")
    type: LLMProviderType = Field(..., description="Provider type")
    endpoint: str = Field(..., description="API endpoint URL")
    api_key_ref: Optional[str] = Field(None, description="Reference to API key in api-keys.json")
    models: List[LLMModel] = Field(default_factory=list, description="Available models")
    enabled: bool = Field(default=True, description="Provider enabled status")
    priority: int = Field(default=0, ge=0, le=100, description="Provider priority for load balancing")


class LLM(BaseModel):
    """LLM configuration section."""
    providers: List[LLMProvider] = Field(default_factory=list, description="LLM provider configurations")
    default_provider: Optional[str] = Field(None, description="Default provider name")
    fallback_enabled: bool = Field(default=True, description="Enable fallback to other providers")
    
    @validator('default_provider')
    def validate_default_provider(cls, v, values):
        """Ensure default provider exists in providers list."""
        if v and 'providers' in values:
            provider_names = [p.name for p in values['providers']]
            if v not in provider_names:
                raise ValueError(f'Default provider {v} not found in providers')
        return v


class SandloopGeometry(BaseModel):
    """Sacred geometry parameters for Möbius loop."""
    phi_ratio: float = Field(default=1.618033988749, description="Golden ratio φ")
    octave: int = Field(default=3, ge=1, le=8, description="Octave level")
    resonance: float = Field(default=0.618, ge=0.0, le=1.0, description="Resonance factor")


class Sandloop(BaseModel):
    """Möbius loop configuration."""
    enabled: bool = Field(default=True, description="Enable Möbius loop processing")
    geometry: SandloopGeometry = Field(default_factory=SandloopGeometry, description="Sacred geometry parameters")
    max_iterations: int = Field(default=8, ge=1, le=64, description="Maximum loop iterations")
    convergence_threshold: float = Field(default=0.001, ge=0.0001, le=0.1, description="Convergence threshold")


class NodeConfig(BaseModel):
    """Complete node configuration schema."""
    identity: Identity
    deployment: Deployment = Field(default_factory=Deployment)
    network: Network = Field(default_factory=Network)
    storage: Storage = Field(default_factory=Storage)
    llm: LLM = Field(default_factory=LLM)
    sandloop: Sandloop = Field(default_factory=Sandloop)
    
    class Config:
        """Pydantic configuration."""
        use_enum_values = True
        validate_assignment = True
