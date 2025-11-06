"""SSH configuration schema for ssh-config.json."""

from typing import Optional, List, Dict
from pydantic import BaseModel, Field, validator
from enum import Enum


class AuthMethod(str, Enum):
    """SSH authentication methods."""
    KEY = "key"
    PASSWORD = "password"
    KEY_WITH_PASSPHRASE = "key_with_passphrase"


class SSHHost(BaseModel):
    """Individual SSH host configuration."""
    alias: str = Field(..., description="Host alias for easy reference")
    hostname: str = Field(..., description="Actual hostname or IP address")
    port: int = Field(default=22, ge=1, le=65535, description="SSH port")
    username: str = Field(..., description="SSH username")
    auth_method: AuthMethod = Field(default=AuthMethod.KEY, description="Authentication method")
    
    # Authentication fields
    key_path: Optional[str] = Field(None, description="Path to SSH private key")
    password: Optional[str] = Field(None, description="SSH password (not recommended)")
    passphrase: Optional[str] = Field(None, description="Key passphrase if needed")
    
    # Advanced options
    compression: bool = Field(default=True, description="Enable SSH compression")
    timeout: int = Field(default=30, ge=5, le=300, description="Connection timeout in seconds")
    keepalive_interval: int = Field(default=60, ge=0, description="Keepalive interval in seconds")
    strict_host_key_checking: bool = Field(default=True, description="Strict host key checking")
    proxy_jump: Optional[str] = Field(None, description="ProxyJump host alias")
    
    # Custom options
    environment_vars: Dict[str, str] = Field(default_factory=dict, description="Environment variables to set")
    forward_agent: bool = Field(default=False, description="Enable SSH agent forwarding")
    
    @validator('key_path')
    def validate_key_path(cls, v, values):
        """Validate key path based on auth method."""
        auth_method = values.get('auth_method')
        if auth_method in [AuthMethod.KEY, AuthMethod.KEY_WITH_PASSPHRASE] and not v:
            raise ValueError(f"key_path is required for auth method {auth_method}")
        return v
    
    @validator('password')
    def validate_password(cls, v, values):
        """Validate password based on auth method."""
        auth_method = values.get('auth_method')
        if auth_method == AuthMethod.PASSWORD and not v:
            raise ValueError("password is required for password auth method")
        return v
    
    @validator('proxy_jump')
    def validate_proxy_jump(cls, v, values):
        """Ensure proxy jump doesn't reference self."""
        if v and 'alias' in values and v == values['alias']:
            raise ValueError("Host cannot use itself as proxy jump")
        return v
    
    def get_connection_string(self) -> str:
        """Generate SSH connection string."""
        return f"{self.username}@{self.hostname}:{self.port}"
    
    def to_ssh_config_format(self) -> str:
        """Convert to OpenSSH config format."""
        lines = [f"Host {self.alias}"]
        lines.append(f"  HostName {self.hostname}")
        lines.append(f"  Port {self.port}")
        lines.append(f"  User {self.username}")
        
        if self.key_path:
            lines.append(f"  IdentityFile {self.key_path}")
        
        if self.compression:
            lines.append("  Compression yes")
        
        lines.append(f"  ConnectTimeout {self.timeout}")
        lines.append(f"  ServerAliveInterval {self.keepalive_interval}")
        
        if not self.strict_host_key_checking:
            lines.append("  StrictHostKeyChecking no")
        
        if self.proxy_jump:
            lines.append(f"  ProxyJump {self.proxy_jump}")
        
        if self.forward_agent:
            lines.append("  ForwardAgent yes")
        
        return "\n".join(lines)


class SSHConfig(BaseModel):
    """Complete SSH configuration."""
    hosts: List[SSHHost] = Field(default_factory=list, description="SSH host configurations")
    global_options: Dict[str, str] = Field(default_factory=dict, description="Global SSH options")
    
    @validator('hosts')
    def validate_unique_aliases(cls, v):
        """Ensure all host aliases are unique."""
        aliases = [host.alias for host in v]
        if len(aliases) != len(set(aliases)):
            raise ValueError("All host aliases must be unique")
        return v
    
    def get_host_by_alias(self, alias: str) -> Optional[SSHHost]:
        """Get host configuration by alias."""
        for host in self.hosts:
            if host.alias == alias:
                return host
        return None
    
    def validate_proxy_jumps(self) -> List[str]:
        """Validate all proxy jump references exist."""
        errors = []
        aliases = {host.alias for host in self.hosts}
        
        for host in self.hosts:
            if host.proxy_jump and host.proxy_jump not in aliases:
                errors.append(f"Host {host.alias} references non-existent proxy jump {host.proxy_jump}")
        
        return errors
    
    def to_ssh_config_format(self) -> str:
        """Convert entire config to OpenSSH format."""
        sections = []
        
        # Add global options if any
        if self.global_options:
            for key, value in self.global_options.items():
                sections.append(f"{key} {value}")
            sections.append("")  # Empty line
        
        # Add host configurations
        for host in self.hosts:
            sections.append(host.to_ssh_config_format())
            sections.append("")  # Empty line between hosts
        
        return "\n".join(sections).strip()
    
    class Config:
        """Pydantic configuration."""
        use_enum_values = True
        validate_assignment = True
