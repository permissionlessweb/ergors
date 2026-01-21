# ERGORS CLI & Engine Guide

## Overview

ERGORS consists of two components that work together:

- **ergors** (engine) - The node daemon that handles storage, networking, and LLM orchestration
- **ergors-cli** - A lightweight command-line client for managing the engine

The CLI communicates with the engine via gRPC, allowing you to manage your node without restarting services.

---

## Quick Start

### Starting the Engine

```bash
# Start as a background daemon
ergors start

# Or run in foreground for debugging
ergors start --foreground
```

### Check Status

```bash
ergors-cli status
```

### Stop the Engine

```bash
ergors-cli engine stop
```

---

## CLI Commands

### Engine Management

```bash
ergors-cli engine start       # Start the engine daemon
ergors-cli engine stop        # Stop the engine gracefully
ergors-cli engine stop -f     # Force stop
ergors-cli engine restart     # Restart the engine
ergors-cli status             # Show engine status (shortcut)
```

### Node Identity

```bash
ergors-cli node info          # Show node identity
ergors-cli node generate      # Generate new keypair
ergors-cli node export        # Export identity as JSON
ergors-cli node export --public-only  # Export only public key
```

### Configuration

```bash
ergors-cli config show        # Display full config
ergors-cli config get <key>   # Get specific value (e.g., network.listen_port)
ergors-cli config set <key> <value>  # Update a setting
```

### Network & Peers

```bash
ergors-cli network peers      # List connected peers
ergors-cli network topology   # Show full network topology
ergors-cli network add <addr> # Add a bootstrap peer
ergors-cli network remove <id> # Remove a peer
```

### LLM Providers

```bash
ergors-cli provider list      # List configured providers
ergors-cli provider add openai        # Add a provider (prompts for API key)
ergors-cli provider add anthropic --api-key sk-...  # Add with key
ergors-cli provider test              # Test all providers
ergors-cli provider test openai       # Test specific provider
ergors-cli provider default anthropic # Set default provider
```

---

## Global Options

All commands support these options:

| Option | Description | Default |
|--------|-------------|---------|
| `--home <PATH>` | Home directory | `~/.ergors` |
| `--grpc-addr <ADDR>` | Engine gRPC address | `http://localhost:50051` |
| `--log-level <LEVEL>` | Log verbosity | `warn` |
| `--json` | Output as JSON (for scripting) | off |

### Environment Variables

- `ERGORS_HOME` - Home directory path
- `ERGORS_GRPC_ADDR` - Engine gRPC address

---

## File Locations

```
~/.ergors/
├── config.toml          # Main configuration
├── ergors.pid           # PID file (when running)
├── api-keys.json        # LLM provider API keys
├── data/                # Storage data
│   └── cnidarium/       # State database
└── logs/                # Log files
    └── engine.log
```

---

## Configuration Reference

### config.toml Structure

```toml
[identity]
host = "0.0.0.0"
p2p_port = 26656
api_port = 8080
node_type = "development"

[network]
listen_address = "0.0.0.0"
listen_port = 26656
bootstrap_peers = []
enable_discovery = true
connection_timeout_ms = 5000

[network.limits]
max_message_size = 1048576
max_peers = 50

[storage]
data_dir = "~/.ergors/data"

[llm]
api_keys_file = "~/.ergors/api-keys.json"
default_entity = "anthropic"
timeout_seconds = 30
max_retries = 3
```

---

## Node Types

When initializing a node, choose the appropriate type:

| Type | Description |
|------|-------------|
| `coordinator` | Orchestrates task distribution across the network |
| `executor` | Executes assigned tasks from coordinators |
| `referee` | Validates execution results |
| `development` | Local development mode (all capabilities) |

---

## Signals

The engine responds to these Unix signals:

| Signal | Action |
|--------|--------|
| `SIGTERM` | Graceful shutdown |
| `SIGINT` | Graceful shutdown (Ctrl+C) |
| `SIGHUP` | Reload configuration |

---

## Scripting Examples

### JSON Output for Scripts

```bash
# Get status as JSON
ergors-cli status --json | jq '.state'

# List providers as JSON
ergors-cli provider list --json | jq '.providers[].name'
```

### Health Check Script

```bash
#!/bin/bash
status=$(ergors-cli status --json 2>/dev/null)
if [ $? -ne 0 ]; then
    echo "Engine not running"
    exit 1
fi
state=$(echo "$status" | jq -r '.state')
if [ "$state" != "running" ]; then
    echo "Engine in state: $state"
    exit 1
fi
echo "Engine healthy"
```

---

## Troubleshooting

### Engine Won't Start

1. Check if already running: `ergors-cli status`
2. Check PID file: `cat ~/.ergors/ergors.pid`
3. Check logs: `tail -f ~/.ergors/logs/engine.log`

### Can't Connect to Engine

1. Verify engine is running: `ps aux | grep ergors`
2. Check gRPC port: `netstat -an | grep 50051`
3. Try explicit address: `ergors-cli --grpc-addr http://127.0.0.1:50051 status`

### Provider Test Fails

1. Verify API key is set: `ergors-cli provider list`
2. Check network connectivity
3. Test manually: `curl https://api.openai.com/v1/models -H "Authorization: Bearer $OPENAI_API_KEY"`
