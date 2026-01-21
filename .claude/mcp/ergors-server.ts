#!/usr/bin/env bun
/**
 * Ergors MCP Server for Claude Code
 *
 * This MCP server exposes ergors engine tools to Claude Code.
 * Run with: bun run .claude/mcp/ergors-server.ts
 */

import { Server } from "@modelcontextprotocol/sdk/server/index.js"
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js"
import {
  CallToolRequestSchema,
  ListToolsRequestSchema,
  type Tool,
} from "@modelcontextprotocol/sdk/types.js"
import { existsSync, readFileSync } from "fs"
import { join } from "path"
import { homedir } from "os"

// Configuration loading - checks multiple sources in priority order
function loadConfig(): { grpcAddr: string; authToken?: string } {
  // 1. Environment variables (highest priority)
  if (process.env.ERGORS_GRPC_ADDR) {
    return {
      grpcAddr: process.env.ERGORS_GRPC_ADDR,
      authToken: process.env.ERGORS_AUTH_TOKEN,
    }
  }

  // 2. Project config file (.ergors/config.json)
  const projectConfig = join(process.cwd(), ".ergors", "config.json")
  if (existsSync(projectConfig)) {
    try {
      const config = JSON.parse(readFileSync(projectConfig, "utf-8"))
      if (config.grpc_addr) {
        return {
          grpcAddr: config.grpc_addr,
          authToken: config.auth_token || process.env.ERGORS_AUTH_TOKEN,
        }
      }
    } catch {
      // Ignore parse errors
    }
  }

  // 3. User config file (~/.ergors/config.json)
  const userConfig = join(homedir(), ".ergors", "config.json")
  if (existsSync(userConfig)) {
    try {
      const config = JSON.parse(readFileSync(userConfig, "utf-8"))
      if (config.grpc_addr) {
        return {
          grpcAddr: config.grpc_addr,
          authToken: config.auth_token || process.env.ERGORS_AUTH_TOKEN,
        }
      }
    } catch {
      // Ignore parse errors
    }
  }

  // 4. Default
  return {
    grpcAddr: "localhost:50051",
    authToken: process.env.ERGORS_AUTH_TOKEN,
  }
}

const config = loadConfig()
const GRPC_ADDR = config.grpcAddr
const AUTH_TOKEN = config.authToken

// Define tools
const tools: Tool[] = [
  {
    name: "ergors_deploy",
    description:
      "Deploy an ergors node to a specified platform (SSH, Akash, Docker, EC2, Phala, WAVS)",
    inputSchema: {
      type: "object" as const,
      properties: {
        name: {
          type: "string",
          description: "Deployment name for identification",
        },
        platform: {
          type: "string",
          enum: ["ssh", "akash", "docker", "ec2", "phala", "wavs"],
          description: "Target platform for deployment",
        },
        node_type: {
          type: "string",
          enum: ["COORDINATOR", "EXECUTOR", "REFEREE", "DEVELOPMENT"],
          description: "Node role in the tetrahedral topology",
        },
        host: {
          type: "string",
          description: "Target host address",
        },
        p2p_port: {
          type: "number",
          default: 26656,
          description: "P2P communication port",
        },
        api_port: {
          type: "number",
          default: 8080,
          description: "API server port",
        },
        platform_config: {
          type: "string",
          description: "JSON-encoded platform-specific configuration",
        },
      },
      required: ["name", "platform", "node_type", "host"],
    },
  },
  {
    name: "ergors_topology",
    description:
      "Get current network topology including connected nodes and their roles",
    inputSchema: {
      type: "object" as const,
      properties: {
        include_connections: {
          type: "boolean",
          default: true,
          description: "Include connection graph between nodes",
        },
      },
    },
  },
  {
    name: "ergors_announce",
    description:
      "Announce this node to the network with capabilities and load factor",
    inputSchema: {
      type: "object" as const,
      properties: {
        capabilities: {
          type: "string",
          default: "minimal",
          description: "Comma-separated list of capabilities",
        },
        load_factor: {
          type: "number",
          default: 0.5,
          description: "Current load factor (0.0-1.0)",
        },
      },
    },
  },
  {
    name: "ergors_route",
    description:
      "Route a message or task to nodes by role or broadcast to all peers",
    inputSchema: {
      type: "object" as const,
      properties: {
        action: {
          type: "string",
          enum: ["send_to_role", "broadcast", "request"],
          description: "Routing action type",
        },
        target_role: {
          type: "string",
          description:
            "Target node role (COORDINATOR, EXECUTOR, REFEREE) for send_to_role action",
        },
        target_node_id: {
          type: "string",
          description: "Specific node ID for request action",
        },
        message_type: {
          type: "string",
          description:
            "Type of message (task_coordination, workspace_sync, request)",
        },
        payload: {
          type: "string",
          description: "JSON-encoded message payload",
        },
        timeout_ms: {
          type: "number",
          default: 5000,
          description: "Request timeout in milliseconds",
        },
      },
      required: ["action", "message_type", "payload"],
    },
  },
  {
    name: "ergors_health",
    description:
      "Check health status of ergors services including storage and network",
    inputSchema: {
      type: "object" as const,
      properties: {
        include_peers: {
          type: "boolean",
          default: true,
          description: "Include peer connectivity details",
        },
      },
    },
  },
  {
    name: "ergors_session",
    description: "Query and manage proxy sessions for LLM request history",
    inputSchema: {
      type: "object" as const,
      properties: {
        action: {
          type: "string",
          enum: ["list", "get", "stats"],
          description: "Action to perform on sessions",
        },
        session_id: {
          type: "string",
          description: "Session ID for get action",
        },
        limit: {
          type: "number",
          default: 50,
          description: "Max results for list",
        },
        offset: {
          type: "number",
          default: 0,
          description: "Offset for pagination",
        },
      },
      required: ["action"],
    },
  },
  {
    name: "ergors_workspace",
    description: "Manage git workspaces for task execution",
    inputSchema: {
      type: "object" as const,
      properties: {
        action: {
          type: "string",
          enum: ["list", "create", "remove", "sync"],
          description: "Workspace action",
        },
        workspace_id: {
          type: "string",
          description: "Workspace ID for specific operations",
        },
        name: {
          type: "string",
          description: "Workspace name for create",
        },
        remote_url: {
          type: "string",
          description: "Git remote URL for create",
        },
      },
      required: ["action"],
    },
  },
  {
    name: "ergors_worktree",
    description: "Manage git worktrees for parallel task execution",
    inputSchema: {
      type: "object" as const,
      properties: {
        action: {
          type: "string",
          enum: ["list", "create", "complete", "fail"],
          description: "Worktree action",
        },
        task_id: {
          type: "string",
          description: "Task ID for the worktree",
        },
        workspace_id: {
          type: "string",
          description: "Parent workspace ID",
        },
        commit_message: {
          type: "string",
          description: "Commit message for complete action",
        },
        merge_to_main: {
          type: "boolean",
          default: true,
          description: "Merge task branch to main on complete",
        },
        reason: {
          type: "string",
          description: "Failure reason for fail action",
        },
      },
      required: ["action"],
    },
  },
]

// Tool execution handlers
async function executeTool(
  name: string,
  args: Record<string, unknown>
): Promise<unknown> {
  switch (name) {
    case "ergors_deploy":
      return handleDeploy(args)
    case "ergors_topology":
      return handleTopology(args)
    case "ergors_announce":
      return handleAnnounce(args)
    case "ergors_route":
      return handleRoute(args)
    case "ergors_health":
      return handleHealth(args)
    case "ergors_session":
      return handleSession(args)
    case "ergors_workspace":
      return handleWorkspace(args)
    case "ergors_worktree":
      return handleWorktree(args)
    default:
      throw new Error(`Unknown tool: ${name}`)
  }
}

// Handler implementations (placeholders - replace with actual gRPC calls)
async function handleDeploy(args: Record<string, unknown>) {
  let platformConfig = {}
  if (args.platform_config) {
    try {
      platformConfig = JSON.parse(args.platform_config as string)
    } catch {
      return { success: false, error: "Invalid platform_config JSON" }
    }
  }

  // TODO: Call gRPC BootstrapNode
  return {
    success: true,
    deployment_id: `deploy-${Date.now()}`,
    platform: args.platform,
    node_type: args.node_type,
    host: args.host,
    status: "PENDING",
    message: `Deployment request created for ${args.name} on ${args.platform}`,
    grpc_endpoint: GRPC_ADDR,
  }
}

async function handleTopology(args: Record<string, unknown>) {
  // TODO: Call gRPC GetNetworkTopology
  return {
    nodes: [
      {
        node_id: "local-dev-node",
        node_type: "DEVELOPMENT",
        online: true,
        last_seen: Date.now(),
      },
    ],
    connections: args.include_connections ? [] : undefined,
    total_nodes: 1,
    online_nodes: 1,
  }
}

async function handleAnnounce(args: Record<string, unknown>) {
  const capabilities = ((args.capabilities as string) || "minimal")
    .split(",")
    .map((s) => s.trim())
  // TODO: Call gRPC AnnounceNode
  return {
    acknowledged: true,
    peers_notified: 0,
    capabilities,
    load_factor: args.load_factor || 0.5,
  }
}

async function handleRoute(args: Record<string, unknown>) {
  if (args.action === "send_to_role" && !args.target_role) {
    return {
      success: false,
      error: "target_role is required for send_to_role action",
    }
  }
  if (args.action === "request" && !args.target_node_id) {
    return {
      success: false,
      error: "target_node_id is required for request action",
    }
  }

  // TODO: Call gRPC RouteMessage
  return {
    success: true,
    nodes_reached: args.action === "broadcast" ? 0 : 1,
    action: args.action,
    message_type: args.message_type,
  }
}

async function handleHealth(args: Record<string, unknown>) {
  // TODO: Call gRPC GetStatus
  return {
    status: "ok",
    version: "0.1.0",
    uptime_seconds: 0,
    storage_status: "healthy",
    network_status: "no peers connected",
    connected_peers: 0,
    grpc_endpoint: GRPC_ADDR,
    auth_configured: !!AUTH_TOKEN,
  }
}

async function handleSession(args: Record<string, unknown>) {
  const action = args.action as string
  // TODO: Call gRPC QuerySessions
  switch (action) {
    case "list":
      return {
        sessions: [],
        total_count: 0,
        limit: args.limit || 50,
        offset: args.offset || 0,
      }
    case "get":
      if (!args.session_id) {
        return { error: "session_id required for get action" }
      }
      return {
        session_id: args.session_id,
        status: "not_found",
      }
    case "stats":
      return {
        total_sessions: 0,
        total_tokens: 0,
        total_cost: 0,
      }
    default:
      return { error: "Unknown action" }
  }
}

async function handleWorkspace(args: Record<string, unknown>) {
  const action = args.action as string
  // TODO: Call gRPC workspace operations
  switch (action) {
    case "list":
      return { workspaces: [], total: 0 }
    case "create":
      if (!args.name || !args.remote_url) {
        return { error: "name and remote_url required for create" }
      }
      return {
        success: true,
        workspace_id: `ws-${Date.now()}`,
        name: args.name,
        remote_url: args.remote_url,
      }
    case "remove":
      if (!args.workspace_id) {
        return { error: "workspace_id required for remove" }
      }
      return { success: true, workspace_id: args.workspace_id }
    case "sync":
      if (!args.workspace_id) {
        return { error: "workspace_id required for sync" }
      }
      return { success: true, workspace_id: args.workspace_id, synced: true }
    default:
      return { error: "Unknown action" }
  }
}

async function handleWorktree(args: Record<string, unknown>) {
  const action = args.action as string
  // TODO: Call gRPC worktree operations
  switch (action) {
    case "list":
      return { worktrees: [], total: 0 }
    case "create":
      if (!args.task_id || !args.workspace_id) {
        return { error: "task_id and workspace_id required for create" }
      }
      return {
        success: true,
        task_id: args.task_id,
        workspace_id: args.workspace_id,
        branch: `task/${args.task_id}`,
        worktree_path: `/tmp/ergors/worktrees/${args.task_id}`,
      }
    case "complete":
      if (!args.task_id) {
        return { error: "task_id required for complete" }
      }
      return {
        success: true,
        task_id: args.task_id,
        merged: args.merge_to_main !== false,
        commit_hash: "placeholder",
      }
    case "fail":
      if (!args.task_id) {
        return { error: "task_id required for fail" }
      }
      return {
        success: true,
        task_id: args.task_id,
        reason: args.reason || "Unknown failure",
      }
    default:
      return { error: "Unknown action" }
  }
}

// Create and start the server
const server = new Server(
  {
    name: "ergors",
    version: "0.1.0",
  },
  {
    capabilities: {
      tools: {},
    },
  }
)

// Register tool handlers
server.setRequestHandler(ListToolsRequestSchema, async () => ({
  tools,
}))

server.setRequestHandler(CallToolRequestSchema, async (request) => {
  const { name, arguments: args } = request.params
  try {
    const result = await executeTool(name, args || {})
    return {
      content: [
        {
          type: "text" as const,
          text: JSON.stringify(result, null, 2),
        },
      ],
    }
  } catch (error) {
    return {
      content: [
        {
          type: "text" as const,
          text: JSON.stringify({
            error: error instanceof Error ? error.message : String(error),
          }),
        },
      ],
      isError: true,
    }
  }
})

// Start the server
async function main() {
  const transport = new StdioServerTransport()
  await server.connect(transport)
  console.error("Ergors MCP server running on stdio")
}

main().catch(console.error)
