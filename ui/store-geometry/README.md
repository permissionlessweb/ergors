 AI Implementation Prompt: CW-HOE Cnidarium Storage State Visualizer

  Project Overview

  You are tasked with implementing a comprehensive visualization system for CW-HOE's Cnidarium-based storage state. This involves creating a gRPC layer for efficient bulk data retrieval and a Python Textual-based visualizer that displays the distributed storage
  contents with real-time network topology awareness.

  Current Storage Implementation Analysis

  Based on the existing cw-ho codebase, the storage system consists of:

  Core Storage Architecture

  - Cnidarium Storage: RocksDB-based storage with prefix-based multistore routing
  - Storage Prefixes: task, sandloop, peer, consensus, snapshot, capabilities, params, llm_response, node_registry, node_identity
  - Data Types:
    - PromptResponse (LLM requests/responses with context)
    - NodeInfo (network topology and node metadata)
    - NetworkTopology (tetrahedral mesh connections)
    - Indexed data by session_id, user_id, and timestamp

  Current HTTP API Endpoints

  - POST /api/prompt - Store LLM prompt/response
  - GET /api/prompts - Query stored prompts with filters
  - GET /health - Storage and network health status
  - GET /network/topology - Current network topology
  - POST /orchestrate/bootstrap - Node bootstrapping operations

  Data Schema (Current Implementation)

  // Primary data stored in Cnidarium
  struct PromptResponse {
      id: Uuid,
      prompt: String,
      response: String,
      model: String,
      timestamp: DateTime<Utc>,
      tokens_used: Option<u32>,
      context: Option<PromptContext>,
      provider: String,
      cost: f64,
      latency_ms: u64,
  }

  struct NetworkTopology {
      nodes: HashMap<String, NodeInfo>,
      connections: Vec<(String, String)>,
  }

  // Storage keys with prefixes
  const PROMPT_PREFIX: &str = "prompts/";
  const SESSION_INDEX_PREFIX: &str = "sessions/";
  const USER_INDEX_PREFIX: &str = "users/";
  const TIMESTAMP_INDEX_PREFIX: &str = "timestamps/";

  Implementation Requirements

  1. gRPC Service Layer (Rust)

  Add to Cargo.toml:
  [dependencies]
  tonic = { version = "0.11", features = ["transport"] }
  prost = "0.13"
  prost-types = "0.13"

  [build-dependencies]
  tonic-build = "0.11"

  Create hoe/storage_state.proto:
  syntax = "proto3";
  package storage_state;

  import "google/protobuf/timestamp.proto";
  import "google/protobuf/struct.proto";

  service StorageStateService {
    rpc ListPrompts(ListPromptsRequest) returns (ListPromptsResponse);
    rpc ListNodes(ListNodesRequest) returns (ListNodesResponse);
    rpc GetNetworkTopology(GetNetworkTopologyRequest) returns (GetNetworkTopologyResponse);
    rpc ExportKeys(ExportKeysRequest) returns (ExportKeysResponse);
    rpc HealthCheck(HealthCheckRequest) returns (HealthCheckResponse);
  }

  message ListPromptsRequest {
    string session_id = 1;
    string user_id = 2;
    google.protobuf.Timestamp start_time = 3;
    google.protobuf.Timestamp end_time = 4;
    uint32 limit = 5;
    uint32 offset = 6;
  }

  message ListPromptsResponse {
    repeated StoredPrompt prompts = 1;
    uint64 total_count = 2;
    bool has_more = 3;
  }

  message StoredPrompt {
    string id = 1;
    string prompt = 2;
    string response = 3;
    string model = 4;
    google.protobuf.Timestamp timestamp = 5;
    uint32 tokens_used = 6;
    google.protobuf.Struct context = 7;
    string provider = 8;
    double cost = 9;
    uint64 latency_ms = 10;
  }

  message ListNodesRequest {
    string node_type = 1;
    bool online_only = 2;
    uint32 limit = 3;
  }

  message ListNodesResponse {
    repeated NetworkNode nodes = 1;
    uint64 total_count = 2;
  }

  message NetworkNode {
    string node_id = 1;
    string node_type = 2;
    string address = 3;
    bool online = 4;
    google.protobuf.Timestamp last_seen = 5;
    google.protobuf.Struct metadata = 6;
  }

  message GetNetworkTopologyRequest {}

  message GetNetworkTopologyResponse {
    repeated NetworkNode nodes = 1;
    repeated Connection connections = 2;
    TopologyStats stats = 3;
  }

  message Connection {
    string from_node = 1;
    string to_node = 2;
  }

  message TopologyStats {
    uint32 total_nodes = 1;
    uint32 online_nodes = 2;
    uint32 total_connections = 3;
    bool is_complete_tetrahedron = 4;
  }

  message ExportKeysRequest {
    string key_pattern = 1;
    uint32 limit = 2;
  }

  message ExportKeysResponse {
    repeated string keys = 1;
    uint64 total_count = 2;
  }

  message HealthCheckRequest {}

  message HealthCheckResponse {
    bool healthy = 1;
    string version = 2;
    google.protobuf.Timestamp timestamp = 3;
    google.protobuf.Struct status = 4;
  }

  Rust gRPC Service Implementation (src/grpc_service.rs):
  use std::sync::Arc;
  use tonic::{Request, Response, Status};
  use tracing::{debug, error, info};

  use crate::storage::Storage;
  use crate::network::CwHoNetworkManifold;

  pub mod storage_state {
      tonic::include_proto!("storage_state");
  }

  pub struct StorageStateService {
      storage: Arc<Storage>,
      network_manifold: Arc<tokio::sync::Mutex<CwHoNetworkManifold>>,
  }

  impl StorageStateService {
      pub fn new(
          storage: Arc<Storage>,
          network_manifold: Arc<tokio::sync::Mutex<CwHoNetworkManifold>>
      ) -> Self {
          Self { storage, network_manifold }
      }
  }

  #[tonic::async_trait]
  impl storage_state::storage_state_service_server::StorageStateService for StorageStateService {
      async fn list_prompts(
          &self,
          request: Request<storage_state::ListPromptsRequest>,
      ) -> Result<Response<storage_state::ListPromptsResponse>, Status> {
          let req = request.into_inner();

          // Convert protobuf request to QueryRequest
          let query = crate::types::QueryRequest {
              session_id: if req.session_id.is_empty() { None } else { Some(req.session_id) },
              user_id: if req.user_id.is_empty() { None } else { Some(req.user_id) },
              start_time: req.start_time.map(|ts|
                  chrono::DateTime::from_timestamp(ts.seconds, ts.nanos as u32)
                      .unwrap_or_else(chrono::Utc::now)
              ),
              end_time: req.end_time.map(|ts|
                  chrono::DateTime::from_timestamp(ts.seconds, ts.nanos as u32)
                      .unwrap_or_else(chrono::Utc::now)
              ),
              limit: Some(req.limit.max(1).min(200) as usize),
          };

          match self.storage.query_prompts(&query).await {
              Ok(prompts) => {
                  let proto_prompts: Vec<storage_state::StoredPrompt> = prompts
                      .into_iter()
                      .skip(req.offset as usize)
                      .map(|p| storage_state::StoredPrompt {
                          id: p.id.to_string(),
                          prompt: p.prompt,
                          response: p.response,
                          model: p.model,
                          timestamp: Some(prost_types::Timestamp {
                              seconds: p.timestamp.timestamp(),
                              nanos: 0,
                          }),
                          tokens_used: p.tokens_used.unwrap_or(0),
                          context: p.context.map(|c| /* convert to Struct */),
                          provider: p.provider,
                          cost: p.cost,
                          latency_ms: p.latency_ms,
                      })
                      .collect();

                  Ok(Response::new(storage_state::ListPromptsResponse {
                      prompts: proto_prompts,
                      total_count: prompts.len() as u64,
                      has_more: false, // Implement proper pagination
                  }))
              }
              Err(e) => Err(Status::internal(format!("Failed to query prompts: {}", e)))
          }
      }

      async fn get_network_topology(
          &self,
          _request: Request<storage_state::GetNetworkTopologyRequest>,
      ) -> Result<Response<storage_state::GetNetworkTopologyResponse>, Status> {
          let network_manifold = self.network_manifold.lock().await;
          let topology = network_manifold.get_topology().await;

          let proto_nodes: Vec<storage_state::NetworkNode> = topology.nodes
              .values()
              .map(|node| storage_state::NetworkNode {
                  node_id: node.node_id.clone(),
                  node_type: format!("{:?}", node.node_type),
                  address: node.address.clone(),
                  online: node.online,
                  last_seen: Some(prost_types::Timestamp {
                      seconds: node.last_seen.timestamp(),
                      nanos: 0,
                  }),
                  metadata: None, // Convert node metadata if available
              })
              .collect();

          let proto_connections: Vec<storage_state::Connection> = topology.connections
              .iter()
              .map(|(from, to)| storage_state::Connection {
                  from_node: from.clone(),
                  to_node: to.clone(),
              })
              .collect();

          let stats = topology.stats();
          let proto_stats = storage_state::TopologyStats {
              total_nodes: stats.total_nodes as u32,
              online_nodes: stats.online_nodes as u32,
              total_connections: stats.total_connections as u32,
              is_complete_tetrahedron: stats.is_complete,
          };

          Ok(Response::new(storage_state::GetNetworkTopologyResponse {
              nodes: proto_nodes,
              connections: proto_connections,
              stats: Some(proto_stats),
          }))
      }

      // Implement other methods...
  }

  Update Server to Include gRPC (src/server.rs):
  use tonic::transport::Server as TonicServer;
  use storage_state::storage_state_service_server::StorageStateServiceServer;

  impl Server {
      pub async fn run_with_grpc(self, http_port: u16, grpc_port: u16) -> Result<()> {
          let http_app = self.create_http_router();
          let grpc_service = StorageStateService::new(
              self.state.storage.clone(),
              self.state.network_manifold.clone(),
          );

          let http_addr = SocketAddr::from(([0, 0, 0, 0], http_port));
          let grpc_addr = SocketAddr::from(([0, 0, 0, 0], grpc_port));

          info!("🌐 Starting HTTP server on {}", http_addr);
          info!("⚡ Starting gRPC server on {}", grpc_addr);

          let http_listener = TcpListener::bind(&http_addr).await?;
          let http_server = axum::serve(http_listener, http_app);

          let grpc_server = TonicServer::builder()
              .add_service(StorageStateServiceServer::new(grpc_service))
              .serve(grpc_addr);

          tokio::select! {
              result = http_server => {
                  if let Err(e) = result {
                      error!("HTTP server error: {}", e);
                  }
              }
              result = grpc_server => {
                  if let Err(e) = result {
                      error!("gRPC server error: {}", e);
                  }
              }
          }

          Ok(())
      }
  }

  2. Python Textual Visualizer

  Dependencies (requirements.txt):
  textual==0.47.1
  grpcio==1.60.0
  grpcio-tools==1.60.0
  protobuf==4.25.1
  httpx==0.26.0
  rich==13.7.0

  Project Structure:
  cw_hoe_visualizer/
  ├── main.py                    # Application entry point
  ├── app.py                     # Main Textual application
  ├── models/
  │   ├── __init__.py
  │   ├── storage_data.py        # Data models
  │   └── network_topology.py    # Network topology models
  ├── clients/
  │   ├── __init__.py
  │   ├── grpc_client.py         # gRPC client
  │   └── http_client.py         # HTTP REST client
  ├── widgets/
  │   ├── __init__.py
  │   ├── storage_browser.py     # Storage data browser
  │   ├── network_view.py        # Network topology view
  │   ├── prompt_viewer.py       # Prompt/response viewer
  │   └── metrics_panel.py       # Health and metrics
  ├── hoe/
  │   ├── storage_state_pb2.py   # Generated protobuf
  │   └── storage_state_pb2_grpc.py
  └── utils/
      ├── __init__.py
      ├── cache.py               # LRU caching
      └── formatters.py          # Data formatting utilities

  Main Application (app.py):
  from textual.app import App, ComposeResult
  from textual.containers import Container, Horizontal, Vertical
  from textual.widgets import Header, Footer, TabbedContent, TabPane
  from textual.reactive import reactive

  from .widgets.storage_browser import StorageBrowser
  from .widgets.network_view import NetworkView
  from .widgets.prompt_viewer import PromptViewer
  from .widgets.metrics_panel import MetricsPanel
  from .clients.grpc_client import StorageGrpcClient
  from .clients.http_client import HttpClient

  class CwHoeVisualizerApp(App):
      """CW-HOE Storage Visualizer - Sacred Geometric State Explorer"""

      CSS_PATH = "visualizer.css"
      TITLE = "🌌 CW-HOE Storage Visualizer"

      # Reactive properties
      selected_node = reactive(None)
      connection_status = reactive("disconnected")

      def __init__(self, host: str = "localhost", http_port: int = 8080, grpc_port: int = 8081):
          super().__init__()
          self.grpc_client = StorageGrpcClient(f"{host}:{grpc_port}")
          self.http_client = HttpClient(f"http://{host}:{http_port}")

      def compose(self) -> ComposeResult:
          yield Header()

          with TabbedContent(initial="storage"):
              with TabPane("Storage Browser", id="storage"):
                  yield StorageBrowser(
                      grpc_client=self.grpc_client,
                      http_client=self.http_client
                  )

              with TabPane("Network Topology", id="network"):
                  yield NetworkView(
                      grpc_client=self.grpc_client,
                      http_client=self.http_client
                  )

              with TabPane("Prompt Viewer", id="prompts"):
                  yield PromptViewer(
                      grpc_client=self.grpc_client,
                      http_client=self.http_client
                  )

              with TabPane("Metrics & Health", id="metrics"):
                  yield MetricsPanel(
                      grpc_client=self.grpc_client,
                      http_client=self.http_client
                  )

          yield Footer()

      async def on_mount(self) -> None:
          """Initialize connections and start periodic updates"""
          await self.grpc_client.connect()
          self.set_interval(5.0, self.update_health_status)

      async def update_health_status(self) -> None:
          """Periodically check connection health"""
          try:
              health = await self.grpc_client.health_check()
              self.connection_status = "connected" if health.healthy else "degraded"
          except Exception:
              self.connection_status = "disconnected"

  Storage Browser Widget (widgets/storage_browser.py):
  from textual.widgets import DataTable, Static, Button, Input, Select
  from textual.containers import Container, Horizontal, Vertical
  from textual.reactive import reactive
  from textual import on
  from rich.text import Text
  from rich.syntax import Syntax
  import json

  class StorageBrowser(Container):
      """Interactive browser for Cnidarium storage contents"""

      current_prefix = reactive("prompts/")
      selected_item = reactive(None)

      def compose(self):
          with Horizontal(classes="browser-layout"):
              with Vertical(classes="browser-controls"):
                  yield Static("🗂️ Storage Prefixes", classes="section-header")
                  yield Select([
                      ("prompts/", "prompts/"),
                      ("sessions/", "sessions/"),
                      ("users/", "users/"),
                      ("timestamps/", "timestamps/"),
                      ("nodes/", "nodes/"),
                      ("topology/", "topology/"),
                  ], value="prompts/", id="prefix_select")

                  yield Static("🔍 Filters", classes="section-header")
                  yield Input(placeholder="Session ID", id="session_filter")
                  yield Input(placeholder="User ID", id="user_filter")
                  yield Button("Load Data", id="load_button", variant="primary")
                  yield Button("Export Keys", id="export_button")

              with Vertical(classes="data-view"):
                  yield Static("📋 Storage Contents", classes="section-header")
                  yield DataTable(id="storage_table")

              with Vertical(classes="detail-view"):
                  yield Static("🔎 Item Details", classes="section-header")
                  yield Static(id="detail_content", classes="detail-panel")

      @on(Select.Changed, "#prefix_select")
      async def on_prefix_changed(self, event: Select.Changed):
          self.current_prefix = event.value
          await self.load_storage_data()

      @on(Button.Pressed, "#load_button")
      async def on_load_pressed(self):
          await self.load_storage_data()

      @on(Button.Pressed, "#export_button") 
      async def on_export_pressed(self):
          await self.export_keys()

      @on(DataTable.RowSelected, "#storage_table")
      async def on_row_selected(self, event: DataTable.RowSelected):
          await self.show_item_details(event.row_key)

      async def load_storage_data(self):
          """Load data based on current prefix and filters"""
          table = self.query_one("#storage_table", DataTable)
          session_filter = self.query_one("#session_filter", Input).value
          user_filter = self.query_one("#user_filter", Input).value

          try:
              if self.current_prefix == "prompts/":
                  prompts = await self.grpc_client.list_prompts(
                      session_id=session_filter or None,
                      user_id=user_filter or None,
                      limit=100
                  )

                  table.clear()
                  table.add_columns("ID", "Timestamp", "Model", "Tokens", "Provider")

                  for prompt in prompts:
                      table.add_row(
                          prompt.id[:8],
                          prompt.timestamp.strftime("%H:%M:%S"),
                          prompt.model,
                          str(prompt.tokens_used),
                          prompt.provider,
                          key=prompt.id
                      )

              elif self.current_prefix.startswith("topology"):
                  topology = await self.grpc_client.get_network_topology()

                  table.clear()
                  table.add_columns("Node ID", "Type", "Address", "Status", "Last Seen")

                  for node in topology.nodes:
                      status = "🟢 Online" if node.online else "🔴 Offline"
                      table.add_row(
                          node.node_id[:12],
                          node.node_type,
                          node.address,
                          status,
                          node.last_seen.strftime("%H:%M:%S"),
                          key=node.node_id
                      )

          except Exception as e:
              self.notify(f"Error loading data: {e}", severity="error")

      async def show_item_details(self, item_key: str):
          """Show detailed view of selected item"""
          detail_panel = self.query_one("#detail_content", Static)

          try:
              if self.current_prefix == "prompts/":
                  # Fetch full prompt details via HTTP API for single item
                  prompt = await self.http_client.get_prompt(item_key)
                  if prompt:
                      detail_content = json.dumps(prompt, indent=2, default=str)
                      syntax = Syntax(detail_content, "json", theme="monokai")
                      detail_panel.update(syntax)

          except Exception as e:
              detail_panel.update(f"Error loading details: {e}")

      async def export_keys(self):
          """Export all keys matching current prefix"""
          try:
              keys = await self.grpc_client.export_keys(
                  pattern=self.current_prefix,
                  limit=1000
              )
              # Save to file or show in dialog
              self.notify(f"Exported {len(keys)} keys", severity="information")
          except Exception as e:
              self.notify(f"Export failed: {e}", severity="error")

  3. Implementation Specifications

  Key Features to Implement:

  1. Real-time Data Streaming: Auto-refresh storage contents every 5-10 seconds
  2. Tetrahedral Network Visualization: 3D-like node layout showing geometric relationships
  3. Advanced Filtering: Time ranges, node types, session contexts
  4. Data Export: CSV/JSON export of filtered data
  5. Health Monitoring: Connection status, storage health, network topology completeness
  6. Caching Layer: LRU cache to minimize redundant gRPC calls
  7. Search Functionality: Full-text search across stored prompts
  8. Pagination: Efficient loading of large datasets

  Performance Requirements:
  - Maximum 200 items per gRPC call
  - Cache responses for 30 seconds
  - Lazy loading for large result sets
  - Background health checks every 5 seconds

  Sacred Geometric Elements:
  - Use golden ratio (1.618) for UI proportions
  - Tetrahedral node layout visualization
  - Fractal-like data organization in tree views
  - Golden ratio-based color scheme

  Testing Requirements:
  - Unit tests for gRPC service methods
  - Integration tests with live Cnidarium storage
  - UI automation tests for Textual widgets
  - Load testing with 1000+ stored prompts

  This implementation provides a comprehensive storage visualization system that maintains the sacred geometric principles while offering practical real-time insights into the CW-HOE network state and storage contents.
