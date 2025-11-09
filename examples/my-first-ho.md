# My first Ho: Guide for deploying your first helper-orchestrator

We will walkthrough the entire workflow for using this engine to orchestrate a conversation between two llms on a specific topic. Our main ho will use an external llm api request, and our side ho will use a local llm request.

- First we will test out orchestrating our main ho to make a request to an external llm.
- Next we will test out orchestrating our main ho to make a request and retrieve it from the llm running that we have deployed via the side hoe.
- Then we will test out orchestrating the main ho to make a request to the side ho to request from the main ho to make an external api request.

## GOALS

- install hoe,create config files necessary(CREATING/USING CONFIG/API/SSH FILES):
- route first prompt through a single hoe e (ACTUALLY USING CORE PRODUCT):
- use ssh to connect to dev env and boostrap second hoe to network
- route LLM chat request: (Ollama, embedding model, etc) from orchestrator node to another node in network,
- create network snapshot folder for that deveoper session

## Requirements

- 1 LLM API (Private,ideally)
- 2 Comnputers (both using linux instances)
- ho-core source code

## Step 1: Prepare Resources & Goals For Agentic Task

First, we generate a default template with the following command:

```sh
# build node binary
cargo build
# initalize node config
RUST_BACKTRACE=1 cargo run -- init # TODO: use CONFIG_UI=1 for ui to manage config
```

```
Next, we can start the main ho node to begin use:
```sh
RUST_BACKTRACE=1 cargo run --bin cw-ho -- start
```

Now, we can use the ho node to self-replicate, via the network command. Lets call the command to create an ssh client between the coordinator & the dev environment. This will install everything we need on linux as well:

```sh
cargo run -- orchestrate --task-type meta-prompt --prompt "Use the ssh client script to create a client between our agents"
```

We can see from the logs that the python script request has been triggered and saved to state, as well as the response.

To query the state with our requests (just to check that it was saved to our nodes storage), use the following:

```

```

Now we want to connect another node to our network. We can command our conductor node to SSH into the dev node, and configure our node so it is running an instance of ollama we can use to make api calls

```sh
 cargo run -- orchestrate --task-type network  --prompt "Use the ssh client script to create a client between our agents" --dev-node conductor-1
```

## Step 2: Create Config File (Use LLM to help)

Heres a prompt you can try to help you configure the config file:

```
We are going to run the my-first-ho example, and need to configure my local envrionment with the actual values of my config files we are using:
``sh
DEV_HELPER_IP=192.168.1.104
DEV_HELPER_SSH_PORT=21212
DEV_HELPER_HO_PORT=7100
``
create the folder and config/env variable format so i can add my api keys manually into the env variables, and show me the command to use to start the process. Ensure we specify the correct file path for the sample command.
```

### Single Node (Development/Testing)

```bash
# Using single node config
cargo run -- start --config config-example.toml --node-type development

# Using fractal config with auto-detection
cargo run -- start --config config-fractal.toml --node-type auto

# Override ports
cargo run -- start --config config-example.toml --port 8080 --p2p-port 9003
```

<!-- Lets use our prompt-for-prompt generator to create the prompt to configure the config file for use:
```
```
 -->

### Single Node (Development/Testing)

```bash
# Using single node config
cargo run -- start --config config-example.toml --node-type development

# Using fractal config with auto-detection
cargo run -- start --config config-fractal.toml --node-type auto

# Override ports
cargo run -- start --config config-example.toml --port 8080 --p2p-port 9003
```

### Fractal Deployment (Full Tetrahedral Network)

Start each node type in separate terminals:

```bash
# Terminal 1: Coordinator Node (Apex Consciousness)
CW_HO_NODE_TYPE=coordinator cargo run -- start --config config-fractal.toml --node-type coordinator

# Terminal 2: Executor Node (Processing Vertex) 
CW_HO_NODE_TYPE=executor cargo run -- start --config config-fractal.toml --node-type executor

# Terminal 3: Referee Node (Validation Vertex)
CW_HO_NODE_TYPE=referee cargo run -- start --config config-fractal.toml --node-type referee

# Terminal 4: Development Node (Innovation Vertex)
CW_HO_NODE_TYPE=development cargo run -- start --config config-fractal.toml --node-type development
```

### Quick Demo Setup

```bash
# 1. Start a single development node for testing
cargo run -- start --config config-example.toml

# 2. Test API endpoints
curl http://localhost:8080/health
curl http://localhost:8080/nodes

# 3. Execute agent workflows
cargo run -- orchestrate --task-type meta-prompt --prompt "Generate a creative story"
```

### Full Tetrahedral Demo

```bash
# 1. Start all four nodes using fractal config (separate terminals)
# 2. Monitor network topology formation
# 3. Execute distributed agent workflows
cargo run -- orchestrate --task-type tetrahedral --prompt "Distributed creativity test"
```

## 🔍 Troubleshooting

### Common Issues

1. **Port Conflicts**: Ensure ports in fractal config don't conflict
2. **Storage Permissions**: Verify write access to `data_dir`
3. **Network Discovery**: Check firewall settings for P2P ports
4. **Config Parsing**: Validate TOML syntax with `toml` command

### Debug Mode

```bash
cargo run -- start --config config-fractal.toml --log-level debug
```

### Health Checks

```bash
# Check node health
curl http://localhost:8000/health  # coordinator
curl http://localhost:8001/health  # executor
curl http://localhost:8002/health  # referee
curl http://localhost:8003/health  # development
```

## 🧪 Testing Configuration

### Validate Configuration

```bash
# Test config loading
cargo run -- init --output ./test-config
cd test-config
cargo run -- start --config config.toml --node-type development --dry-run

# Test API key resolution
cargo run -- status --endpoint http://localhost:8080
```

### Debug API Key Loading

```bash
# Enable debug logging
RUST_LOG=debug cargo run -- start --config config.toml --node-type development
```

## 🚨 Troubleshooting

### Common Issues

#### 1. API Keys File Not Found

```
Error: Failed to read API keys file: "./api-keys.json"
```

**Solution:** Ensure the file exists relative to your config file location.

#### 2. Environment Variable Not Found

```
Warning: Environment variable 'OPENAI_API_KEY' not found
```

**Solution:** Set the environment variable or use a literal key in the JSON file.

#### 3. Invalid JSON Format

```
Error: Failed to parse API keys JSON file
```

**Solution:** Validate JSON syntax with `jq . api-keys.json` or similar tool.

### Debug Commands

```bash
# Check config parsing
cargo run -- init --output ./debug-test

# Validate API keys file
python3 -m json.tool api-keys.json

# Test environment variables
printenv | grep -E "(OPENAI|ANTHROPIC|AKASH|KIMI|GROK)"
```
