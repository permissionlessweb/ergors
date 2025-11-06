# CW-HO Configuration System

## 📋 Overview

The CW-HO configuration system supports both **single-node** and **fractal deployment** configurations, designed around sacred geometric principles including golden ratio resource allocation, tetrahedral topology, and Möbius sandloop feedback cycles.

## 🎯 Configuration Files

### 1. Single Node Configuration (`config-example.toml`)
- **Purpose**: Testing, development, standalone demos
- **Usage**: Single node that can operate independently
- **Format**: Traditional TOML structure for one orchestrator instance

### 2. Fractal Deployment Configuration (`config-fractal.toml`)
- **Purpose**: Full tetrahedral network deployment
- **Usage**: Multi-node sacred geometric network
- **Format**: Contains definitions for all four node types (Coordinator, Executor, Referee, Development)
 

## 🔧 Configuration Structure

### Fractal Config Sections
```toml
[deployment]          # Deployment metadata and active node selection
[geometry]            # Golden ratio constants
[nodes.coordinator]   # Coordinator node configuration
[nodes.executor]      # Executor node configuration
[nodes.referee]       # Referee node configuration 
[nodes.development]   # Development node configuration
[network.shared]      # Shared network parameters
[monitoring]          # Observability configuration
[demo]                # Demo-specific settings
```

### Node-Specific Sections
```toml
[nodes.{type}.network]    # P2P and API ports, peer connections
[nodes.{type}.storage]    # Storage backend, compression, snapshots
[nodes.{type}.agents]     # Agent timeout and retry configuration
[nodes.{type}.sandloops]  # Möbius loop enablement and intervals
```

## 🌐 Environment Variables

| Variable | Purpose | Values |
|----------|---------|---------|
| `CW_HO_NODE_TYPE` | Override active node type | `coordinator`, `executor`, `referee`, `development` |
| `CW_HO_CONFIG_PATH` | Configuration file path | Path to TOML file |

## 🛠️ Demo Usage for Agent Workflows

## ⚙️ Advanced Configuration

### Custom Ports
Override default ports via command line:
```bash
cargo run -- start --config config-fractal.toml --node-type coordinator --port 8100 --p2p-port 9100
```

### Custom Storage Paths
Modify `data_dir` in configuration for persistent storage:
```toml
[storage]
data_dir = "/var/lib/cw-ho/coordinator"  # Persistent storage
```

### Sandloop Tuning
Adjust golden ratio intervals for different performance characteristics:
```toml
[sandloops.intervals]
prompt_request = 30    # Faster iteration
data_ingestion = 48    # 30 * φ
edge_case_testing = 78 # 30 * φ²
audit_snapshot = 127   # 30 * φ³
```

## 🌟 Sacred Computing Philosophy

The configuration system embodies CW-HO's core principle: **the workspace is the API, the interface is the product, and sacred geometry is the path** to manifesting human intention through distributed computational consciousness. Each configuration parameter honors natural mathematical principles, creating harmony between technical precision and artistic expression.

# CW-HO API Keys Management System

## 📋 Overview

The CW-HO API keys management system provides **secure, portable, and scalable** configuration of LLM provider API keys through relative file paths and environment variable expansion. This system enables **recursive scaling** of configurations by allowing modification of key files without changing the core configuration.

## 🔧 Key Features

### ✅ **Relative Path Resolution**
- API key files are referenced relative to the config file location
- Enables portable deployments and version control
- Allows fractal deployment scaling across different environments

### ✅ **Environment Variable Expansion**  
- Supports `${VAR_NAME}` syntax for environment variables
- Fallback to literal values if environment variables are not found
- Secure handling with automatic key masking in logs

### ✅ **Provider Override System**
- Config-level overrides take precedence over JSON files
- Granular control over individual provider settings
- Runtime configuration without file modifications

### ✅ **Golden Ratio Resource Allocation**
- Primary chain: 61.8% allocation (Akash, Kimi, Grok)
- Fallback chain: 38.2% allocation (Ollama, OpenAI, Anthropic)
- Deployment profiles for different environments

## 🚀 Quick Start

### 1. Initialize Configuration
```bash
# Create config files with API keys template
cargo run -- init --output . --create-api-keys-template true

# This creates:
# - config.toml (main configuration)
# - api-keys.json (API keys template)
```

### 2. Configure API Keys
```bash
# Edit the API keys file
vim api-keys.json

# Set environment variables (recommended)
export OPENAI_API_KEY="your-openai-key"
export ANTHROPIC_API_KEY="your-anthropic-key"
export AKASH_API_KEY="your-akash-key"

# Or directly edit the JSON file
```

### 3. Start Node with API Key Support
```bash
# Single node
cargo run -- start --config config.toml --node-type development

# Fractal deployment
cargo run -- start --config config-fractal.toml --node-type coordinator
```

## 📁 File Structure

```
cw-ho/
├── config.toml                 # Main configuration
├── config-fractal.toml         # Fractal deployment config
├── api-keys.json              # API keys configuration
├── api-keys-template.json     # Template for new deployments
└── .env                       # Environment variables (optional)
```

## 🔐 API Keys Configuration Format

### Basic Structure
```json
{
  "_metadata": {
    "version": "1.0.0",
    "description": "CW-HO API Keys Configuration"
  },
  "providers": {
    "primary_chain": {
      "akash_chat": {
        "api_key": "${AKASH_API_KEY}",
        "enabled": true,
        "priority": 1,
        "model_configs": {
          "default": "akash-7b-chat",
          "temperature": 0.7,
          "max_tokens": 2048
        },
        "endpoints": {
          "base_url": "https://api.akash.network/chat/v1"
        }
      }
    },
    "fallback_chain": {
      "ollama_local": {
        "api_key": null,
        "enabled": true,
        "priority": 4,
        "endpoints": {
          "base_url": "http://localhost:11434"
        }
      }
    }
  }
}
```

### Environment Variable Expansion
```json
{
  "providers": {
    "primary_chain": {
      "openai": {
        "api_key": "${OPENAI_API_KEY}",           // Expands to env var
        "api_key": "sk-literal-key-here",         // Literal key
        "api_key": "${OPENAI_KEY:-fallback}",     // With fallback (not implemented)
      }
    }
  }
}
```

## ⚙️ Configuration Integration

### Single Node Config (config.toml)
```toml
[agents.llm]
# Relative path to API keys file
api_keys_file = "api-keys.json"
# Deployment profile
deployment_profile = "development"
# Enable fallback providers
enable_fallbacks = true
# Enforce golden ratio allocation
enforce_golden_ratio = true

# Optional provider overrides
[agents.llm.provider_overrides.ollama_local]
enabled = true
base_url = "http://localhost:11434"
model = "llama3.2:latest"
```

### Fractal Deployment Config (config-fractal.toml)
```toml
# Each node can reference the same API keys file
[nodes.coordinator.agents.llm]
api_keys_file = "api-keys.json"
deployment_profile = "production"

[nodes.executor.agents.llm]
api_keys_file = "api-keys.json"
deployment_profile = "production"

[nodes.referee.agents.llm]
api_keys_file = "api-keys.json"
deployment_profile = "production"

[nodes.development.agents.llm]
api_keys_file = "api-keys.json"
deployment_profile = "development"
```

## 🌐 Deployment Profiles

### Development Profile
```json
{
  "deployment_profiles": {
    "development": {
      "enabled_providers": ["ollama_local", "openai"],
      "fallback_enabled": true,
      "cost_limits": {
        "daily_max_usd": 10.0,
        "monthly_max_usd": 100.0
      }
    }
  }
}
```

### Production Profile
```json
{
  "deployment_profiles": {
    "production": {
      "enabled_providers": ["akash_chat", "kimi_research", "grok", "ollama_local", "openai", "anthropic"],
      "fallback_enabled": true,
      "cost_limits": {
        "daily_max_usd": 200.0,
        "monthly_max_usd": 5000.0
      }
    }
  }
}
```

## 🔄 Recursive Scaling & Transport

### Scenario 1: Environment Promotion
```bash
# Development environment
cp api-keys.json api-keys-dev.json
# Edit for development settings

# Staging environment
cp api-keys.json api-keys-staging.json
# Update config to reference staging keys
# sed -i 's/api-keys.json/api-keys-staging.json/' config.toml

# Production environment
cp api-keys.json api-keys-prod.json
# Update for production settings
```

### Scenario 2: Multi-Region Deployment
```bash
# US East region
mkdir deployments/us-east
cp config-fractal.toml deployments/us-east/
cp api-keys.json deployments/us-east/
# Modify for region-specific endpoints

# EU West region  
mkdir deployments/eu-west
cp config-fractal.toml deployments/eu-west/
cp api-keys.json deployments/eu-west/
# Update base URLs for EU endpoints
```

### Scenario 3: Team-Specific Configurations
```bash
# Team A configuration
mkdir teams/team-a
cp api-keys-template.json teams/team-a/api-keys.json
# Configure with team-specific quotas and providers

# Team B configuration
mkdir teams/team-b
cp api-keys-template.json teams/team-b/api-keys.json
# Different provider priorities and cost limits
```

## 🛠️ API Usage Examples

### Loading API Keys Programmatically
```rust
use crate::config::{OrchestratorConfig, FractalDeploymentConfig};

// Single node config
let config = OrchestratorConfig::from_file("config.toml")?;
let api_key = config.get_api_key(Path::new("config.toml"), "openai")?;

// Fractal deployment config
let fractal_config = FractalDeploymentConfig::from_file("config-fractal.toml")?;
let api_keys = fractal_config.load_api_keys(Path::new("config-fractal.toml"))?;
```

### Environment Variable Priority
1. **Provider Overrides** in config files (highest priority)
2. **API Keys JSON file** with environment variable expansion
3. **Environment variables** directly (fallback)

### Path Resolution Examples
```toml
# Relative to config file directory
api_keys_file = "api-keys.json"                    # ./api-keys.json
api_keys_file = "keys/production.json"             # ./keys/production.json
api_keys_file = "../shared/api-keys.json"          # ../shared/api-keys.json

# Absolute paths also supported
api_keys_file = "/etc/cw-ho/api-keys.json"         # Absolute path
```

## 🔐 Security Best Practices

### 1. Environment Variables (Recommended)
```bash
# Set in shell environment
export OPENAI_API_KEY="sk-..."
export ANTHROPIC_API_KEY="sk-ant-..."

# Or use .env file (with direnv)
echo "OPENAI_API_KEY=sk-..." > .env
echo "api-keys.json" >> .gitignore
```

### 2. Git Security
```bash
# Add to .gitignore
echo "api-keys.json" >> .gitignore
echo ".env" >> .gitignore

# Use template for version control
git add api-keys-template.json
git add config.toml config-fractal.toml
```

### 3. Production Deployment
```bash
# Use secrets management
kubectl create secret generic cw-ho-api-keys \
  --from-env-file=.env

# Or use external secret stores
# - HashiCorp Vault
# - AWS Secrets Manager
# - Azure Key Vault
```


### Custom Provider Configuration
```json
{
  "providers": {
    "specialized": {
      "custom_local": {
        "api_key": null,
        "enabled": true,
        "priority": 10,
        "endpoints": {
          "base_url": "http://custom-server:8080",
          "health_check": "/health",
          "models": "/v1/models"
        },
        "model_configs": {
          "default": "custom-model-v1",
          "temperature": 0.5,
          "max_tokens": 1024
        }
      }
    }
  }
}
```

### Dynamic Provider Discovery
```rust
// Load and inspect available providers
let api_keys = config.load_api_keys(config_path)?;
for (name, provider) in &api_keys.providers.primary_chain {
    if provider.enabled {
        println!("Available provider: {} ({})", name, provider.endpoints.base_url);
    }
}
```

This API keys management system provides the foundation for **scalable, secure, and maintainable** LLM provider configuration in CW-HO, enabling both development flexibility and production robustness through sacred geometric principles and recursive configuration patterns.


### ORCHESTRATOR: REMOTE CONFIG 
### Step 1: Workspace Zip Creation 
```bash
cd /current/workspace && zip -r /tmp/cw-agent-workspace.zip . -x "logs/*" "target/*" "*.log" ".git/*" "node_modules/*" "priv/*" "!templates/*"
```
- Excludes: logs, build artifacts, git history, node modules, private configs
- Includes: source code, tools, template configs, documentation

### Step 2: Base64 Encoding 
- Encodes entire zip file as base64 for safe binary transfer over SSH
- Logs zip size and encoded size for monitoring

### Step 3: Remote Directory Creation 
- WSL: Creates `/mnt/c/cw-agent`
- Linux: Creates `~/cw-agent`
- Properly handles WSL vs native Linux paths

### Step 4: Workspace Transfer 
```bash
echo '[base64_encoded_zip]' | base64 -d > /path/to/workspace.zip
```
- Transfers entire workspace as single base64-encoded command
- Avoids complex file transfer protocols

### Step 5: Remote Unpacking 
```bash
cd /workspace && unzip -o workspace.zip && rm workspace.zip
```
- Unpacks complete workspace on remote host
- Cleans up zip file after extraction

### Step 6: Setup Execution 
```bash
chmod +x /workspace/tools/linux/configure.sh && cd /workspace && sh tools/linux/configure.sh
```
- Executes setup script from transferred workspace
- Maintains proper working directory context