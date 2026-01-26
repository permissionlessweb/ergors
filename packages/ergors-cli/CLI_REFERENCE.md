# ERGORS CLI Reference

Command-line interface for ERGORS engine management.

```bash
ergors-cli [OPTIONS] <COMMAND>
```

## Global Options

| Flag | Description | Default | Env Var |
|------|-------------|---------|---------|
| `--home <PATH>` | Home directory for configuration | `~/.ergors` | `ERGORS_HOME` |
| `--grpc-addr <URL>` | Engine gRPC address | `http://localhost:50051` | `ERGORS_GRPC_ADDR` |
| `--log-level <LEVEL>` | Log level (trace, debug, info, warn, error) | `warn` | - |
| `--json` | Output in JSON format (for scripting) | `false` | - |

## Command Groups

| Group | Description | Commands |
|-------|-------------|----------|
| `engine` | Engine daemon control | start, stop, status, restart |
| `node` | Node identity management | info, generate, export, address |
| `config` | Configuration management | show, get, set |
| `network` | Network and peer management | peers, topology, add, remove |
| `provider` | LLM provider management | list, add, test, default |
| `sdl` | SDL template management | list, get-template, get-defaults, render |
| `deploy` | Akash deployment management | create, run, list, get, advance, bids, select, cancel, close-lease, status, set-endpoints, configure-proxy, trusted-providers, add-provider, remove-provider, request-grant, approve-grant, revoke-grant, list-grants, query-balance |
| `workspace` | Git workspace management | add, list, show, remove, sync, task |
| `status` | Shortcut for `engine status` | - |

---

## Engine Commands

| Command | Description | Options | Example |
|---------|-------------|---------|---------|
| `engine start` | Start the engine daemon | `-f, --foreground` - Run in foreground<br>`--grpc-port <PORT>` - gRPC port (default: `50051`) | `ergors-cli engine start --foreground` |
| `engine stop` | Stop the running engine | `-f, --force` - Force immediate shutdown | `ergors-cli engine stop --force` |
| `engine status` | Show engine status | - | `ergors-cli engine status --json` |
| `engine restart` | Restart the engine | `-f, --force` - Force shutdown before restart | `ergors-cli engine restart` |

---

## Node Commands

| Command | Description | Options/Arguments | Example |
|---------|-------------|-------------------|---------|
| `node info` | Show node identity | - | `ergors-cli node info --json` |
| `node generate` | Generate new node identity | `--node-type <TYPE>` - coordinator, executor, referee, development (default: development) | `ergors-cli node generate --node-type executor` |
| `node export` | Export node identity | `--public-only` - Export only public key | `ergors-cli node export --public-only` |
| `node address` | Get cosmos address for a stored key (any chain) | `-k, --key-name <NAME>` - Key name (uses default if omitted)<br>`-p, --prefix <HRP>` - Bech32 prefix (default: `akash`)<br>`-c, --coin-type <N>` - BIP-44 coin type (default: `118`)<br>`-i, --account-index <N>` - HD account index (default: `0`) | `ergors-cli node address --prefix cosmos`<br>`ergors-cli node address -p osmo -c 118`<br>`ergors-cli node address --json` |

---

## Config Commands

| Command | Description | Arguments | Example |
|---------|-------------|-----------|---------|
| `config show` | Show full configuration (TOML) | - | `ergors-cli config show` |
| `config get <KEY>` | Get specific config value | `<KEY>` - Dot-separated path (e.g., `network.p2p_port`) | `ergors-cli config get llm.default_provider` |
| `config set <KEY> <VALUE>` | Set config value | `<KEY>` - Config key<br>`<VALUE>` - Config value | `ergors-cli config set network.p2p_port 26656` |

---

## Network Commands

| Command | Description | Arguments | Example |
|---------|-------------|-----------|---------|
| `network peers` | List connected peers | - | `ergors-cli network peers` |
| `network topology` | Show network topology | - | `ergors-cli network topology --json` |
| `network add <ADDRESS>` | Add bootstrap peer | `<ADDRESS>` - Peer address (host:port) | `ergors-cli network add 192.168.1.100:26656` |
| `network remove <NODE_ID>` | Remove a peer | `<NODE_ID>` - Node ID to remove | `ergors-cli network remove test-coordinator-0` |

---

## Provider Commands

| Command | Description | Options/Arguments | Example |
|---------|-------------|-------------------|---------|
| `provider list` | List configured LLM providers | - | `ergors-cli provider list --json` |
| `provider add <NAME>` | Add/configure a provider | `<NAME>` - Provider name (openai, anthropic, ollama, etc.)<br>`--api-key <KEY>` - API key (prompts if omitted)<br>`--default` - Set as default | `ergors-cli provider add openai --api-key sk-... --default` |
| `provider test [NAME]` | Test provider connectivity | `[NAME]` - Provider name (tests all if omitted) | `ergors-cli provider test openai` |
| `provider default <NAME>` | Set default provider | `<NAME>` - Provider name | `ergors-cli provider default ollama` |

---

## SDL Commands

| Command | Description | Options/Arguments | Example |
|---------|-------------|-------------------|---------|
| `sdl list` | List deployed SDL template contracts | - | `ergors-cli sdl list` |
| `sdl get-template <ADDR>` | Get SDL template from contract | `<ADDR>` - Contract address | `ergors-cli sdl get-template akash1abc...` |
| `sdl get-defaults <ADDR>` | Get variable defaults from contract | `<ADDR>` - Contract address | `ergors-cli sdl get-defaults akash1abc...` |
| `sdl render <ADDR>` | Render SDL template with variables | `<ADDR>` - Contract address<br>`-v, --var <KEY=VALUE>` - Variable values (repeatable) | `ergors-cli sdl render akash1abc... --var image=nginx:latest --var cpu=0.5` |

---

## Deploy Commands

| Command | Description | Options/Arguments | Example |
|---------|-------------|-------------------|---------|
| `deploy create` | Create new Akash deployment | `--sdl <PATH>` or `--sdl-content <YAML>` - SDL source<br>`--key-name <NAME>` - Signing key (default: `default`)<br>`--account-index <N>` - HD account index (default: `0`)<br>`--node <URL>` - RPC endpoint (env: `AKASH_NODE`)<br>`--chain-id <ID>` - Chain ID (env: `AKASH_CHAIN_ID`)<br>`--auto` - Auto-advance all steps<br>`--skip-grants` - Skip authz/feegrant setup<br>`--auto-select-bid` - Auto-select cheapest trusted provider<br>`--min-balance <UAKT>` - Minimum balance required (default: `5000000`)<br>`--var <KEY=VALUE>` - Template variables | `ergors-cli deploy create --sdl deployment.yaml --auto --skip-grants` |
| `deploy run <SESSION>` | Run automated workflow on existing session | `<SESSION>` - Session ID<br>`--skip-grants` - Skip authz/feegrant setup<br>`--auto-select-bid` - Auto-select cheapest trusted provider<br>`--min-balance <UAKT>` - Minimum balance required (default: `5000000`) | `ergors-cli deploy run 12345... --auto-select-bid` |
| `deploy list` | List deployment workflows | `--status <STATUS>` - Filter: pending, running, completed, failed<br>`--limit <N>` - Max results (default: `50`) | `ergors-cli deploy list --status running --limit 10` |
| `deploy get <SESSION>` | Get deployment workflow details | `<SESSION>` - Session ID | `ergors-cli deploy get 12345678-abcd-...` |
| `deploy advance <SESSION>` | Advance deployment to next step | `<SESSION>` - Session ID | `ergors-cli deploy advance 12345678-abcd-...` |
| `deploy bids <SESSION>` | Query bids for a deployment | `<SESSION>` - Session ID | `ergors-cli deploy bids 12345678-abcd-...` |
| `deploy select <SESSION>` | Select provider for deployment | `<SESSION>` - Session ID<br>`--provider <ADDR>` - Provider address (required)<br>`--price <UAKT>` - Bid price (default: `0`) | `ergors-cli deploy select 12345... --provider akash1provider... --price 100` |
| `deploy cancel <SESSION>` | Cancel a deployment workflow | `<SESSION>` - Session ID | `ergors-cli deploy cancel 12345678-abcd-...` |
| `deploy close-lease <SESSION>` | Close an active lease | `<SESSION>` - Session ID | `ergors-cli deploy close-lease 12345678-abcd-...` |
| `deploy status <SESSION>` | Get lease status | `<SESSION>` - Session ID<br>`-f, --follow` - Follow updates continuously | `ergors-cli deploy status 12345... --follow` |
| `deploy set-endpoints <SESSION>` | Set discovered endpoints | `<SESSION>` - Session ID<br>`--endpoint <SERVICE=URL>` - Endpoints (repeatable) | `ergors-cli deploy set-endpoints 12345... --endpoint api=https://api.example.com` |
| `deploy configure-proxy` | Configure proxy routing | `--openai-url <URL>` - OpenAI-compatible API base<br>`--anthropic-url <URL>` - Anthropic-compatible base<br>`--ollama-url <URL>` - Ollama-compatible base<br>`--route <GLOB=URL>` - Model routing rules (repeatable) | `ergors-cli deploy configure-proxy --openai-url https://api.akash.example/v1 --route "gpt-*=https://openai.proxy"` |
| `deploy trusted-providers` | List trusted providers | - | `ergors-cli deploy trusted-providers` |
| `deploy add-provider <ADDR>` | Add a trusted provider | `<ADDR>` - Provider address<br>`--label <TEXT>` - Optional label | `ergors-cli deploy add-provider akash1provider... --label "US West"` |
| `deploy remove-provider <ADDR>` | Remove a trusted provider | `<ADDR>` - Provider address | `ergors-cli deploy remove-provider akash1provider...` |
| `deploy request-grant` | Request authz grant from coordinator | `--granter <ADDR>` - Granter address (required)<br>`--grantee <ADDR>` - Grantee address (required)<br>`--msg-type <TYPE>` - Message types (repeatable)<br>`--allowance <UAKT>` - Feegrant amount (default: `0`)<br>`--reason <TEXT>` - Reason for request | `ergors-cli deploy request-grant --granter akash1granter... --grantee akash1grantee... --msg-type /akash.deployment.v1beta3.MsgCreateDeployment --allowance 1000000` |
| `deploy approve-grant <REQ>` | Approve or reject grant request | `<REQ>` - Request ID<br>`--reject` - Reject instead of approve<br>`--reason <TEXT>` - Reason for decision | `ergors-cli deploy approve-grant abc123... --reason "Approved for Q1 campaign"` |
| `deploy revoke-grant` | Revoke an existing grant | `--granter <ADDR>` - Granter address (required)<br>`--grantee <ADDR>` - Grantee address (required)<br>`--msg-type <TYPE>` - Message type (empty = all)<br>`--revoke-feegrant` - Also revoke feegrant | `ergors-cli deploy revoke-grant --granter akash1granter... --grantee akash1grantee... --revoke-feegrant` |
| `deploy list-grants` | List pending grant requests | `--granter <ADDR>` - Filter by granter<br>`--grantee <ADDR>` - Filter by grantee<br>`--status <STATUS>` - Filter: pending, approved, rejected | `ergors-cli deploy list-grants --status pending` |
| `deploy query-balance <ADDR>` | Query account balance | `<ADDR>` - Account address<br>`--denom <DENOM>` - Denom (default: `uakt`) | `ergors-cli deploy query-balance akash1abc... --denom uatom` |

---

## Workspace Commands

| Command | Description | Options/Arguments | Example |
|---------|-------------|-------------------|---------|
| `workspace add <NAME>` | Add new workspace | `<NAME>` - Workspace name<br>`--remote <URL>` - Remote git URL (optional) | `ergors-cli workspace add my-project --remote https://github.com/user/repo.git` |
| `workspace list` | List registered workspaces | `--limit <N>` - Max results (default: `50`) | `ergors-cli workspace list` |
| `workspace show <WS>` | Show workspace details | `<WS>` - Workspace ID | `ergors-cli workspace show ws-12345...` |
| `workspace remove <WS>` | Remove a workspace | `<WS>` - Workspace ID<br>`-f, --force` - Force removal with active worktrees | `ergors-cli workspace remove ws-12345... --force` |
| `workspace sync <WS>` | Sync workspace with remote | `<WS>` - Workspace ID<br>`--remote <NAME>` - Remote name (default: `origin`)<br>`--push` - Push local changes<br>`--fetch` - Fetch remote changes (default: `true`) | `ergors-cli workspace sync ws-12345... --push` |

---

## Workspace Task Commands

| Command | Description | Options/Arguments | Example |
|---------|-------------|-------------------|---------|
| `workspace task create <WS>` | Create new task worktree | `<WS>` - Workspace ID<br>`--task-id <ID>` - Task ID (generates UUID if omitted)<br>`--assign-to <NODE>` - Node to assign task | `ergors-cli workspace task create ws-12345... --assign-to executor-node-1` |
| `workspace task list` | List task worktrees | `--workspace <ID>` - Filter by workspace<br>`--node <NODE>` - Filter by assigned node | `ergors-cli workspace task list --workspace ws-12345...` |
| `workspace task complete <TASK>` | Complete a task worktree | `<TASK>` - Task ID<br>`-m, --message <TEXT>` - Commit message (required)<br>`--merge` - Merge to main branch | `ergors-cli workspace task complete task-abc... -m "Implement feature X" --merge` |
| `workspace task fail <TASK>` | Fail/abandon a task worktree | `<TASK>` - Task ID<br>`-r, --reason <TEXT>` - Reason (required)<br>`--cleanup` - Cleanup the worktree | `ergors-cli workspace task fail task-xyz... -r "Blocked by dependency" --cleanup` |

---

## Workflow Steps (Deploy)

| Step | Name | Description |
|------|------|-------------|
| 1 | `key_selection` | Select signing key |
| 2 | `balance_check` | Check account balance |
| 3 | `grant_request` | Request authz/feegrant |
| 4 | `grant_wait` | Wait for grant approval |
| 5 | `authz_setup` | Configure authz permissions |
| 6 | `feegrant_setup` | Configure feegrant allowance |
| 7 | `sdl_configuration` | Validate and prepare SDL |
| 8 | `certificate_setup` | Generate/upload certificates |
| 9 | `deployment_create` | Broadcast deployment transaction |
| 10 | `bid_wait` | Wait for provider bids |
| 11 | `provider_selection` | Select provider and accept bid |
| 12 | `lease_create` | Create lease with provider |
| 13 | `manifest_send` | Send manifest to provider |
| 14 | `endpoint_retrieval` | Retrieve service endpoints |
| 15 | `endpoint_testing` | Test endpoint connectivity |
| 16 | `complete` | Deployment successful |
| 17 | `failed` | Deployment failed |

---

## Workflow Status (Deploy)

| Status | Description |
|--------|-------------|
| `pending` | Workflow created, not started |
| `running` | Workflow executing steps |
| `paused` | Workflow paused (waiting for user input) |
| `completed` | Workflow finished successfully |
| `failed` | Workflow encountered error |
| `cancelled` | Workflow manually cancelled |

---

## Environment Variables

| Variable | Description |
|----------|-------------|
| `ERGORS_HOME` | Override default home directory (`~/.ergors`) |
| `ERGORS_GRPC_ADDR` | Override default gRPC address (`http://localhost:50051`) |
| `ERGORS_CUSTODY_PASSWORD` | Password to unlock encrypted key store (required for `node address`) |
| `AKASH_NODE` | Default Akash node RPC endpoint |
| `AKASH_CHAIN_ID` | Default Akash chain ID |

---

## JSON Output

All commands support `--json` flag for machine-readable output:

```bash
ergors-cli engine status --json
```

```json
{
  "version": "0.1.0",
  "state": "RUNNING",
  "uptime_seconds": 3600,
  "storage_status": "healthy",
  "network_status": "connected",
  "connected_peers": 3,
  "total_requests": 142
}
```

---

## Exit Codes

| Code | Description |
|------|-------------|
| `0` | Success |
| `1` | General error |
| Non-zero | Connection error (with message on stderr) |
