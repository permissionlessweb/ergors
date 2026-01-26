//! ERGORS - Ergodic Recursive Systems
//!
//! Unified CLI combining local operations and daemon management.

use anyhow::{Context as _, Result};
use camino::Utf8PathBuf;
use clap::{Parser, Subcommand};
use commonware_runtime::{
    tokio::{Config as RuntimeConfig, Runner},
    Runner as _,
};
use ergors::{
    auth::AuthCmd,
    call::CallCmd,
    client::ManagementClient,
    commands::{
        CliContext, DeployCmd, EngineCmd, NetworkCmd, NodeCmd, ProviderCmd, RagCmd,
        RemoteConfigCmd, SdlCmd, WorkspaceCmd,
    },
    config::ErgorsConfig,
    config_cmd::ConfigCmd,
    daemon::{Daemon, SignalHandler},
    grpc::management::{start_grpc_server, ManagementServiceImpl},
    init::InitCmd,
    keys::KeysCmd,
    server::Server as CwHoServer,
};
use ho_std::{
    constants::{default_home, init_env, CONFIG_FILE_NAME},
    error::HoResult,
    traits::HoConfigTrait,
};

use std::fs;
use std::net::SocketAddr;
use tracing::{error, info};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

/// Default gRPC address for the engine
const DEFAULT_GRPC_ADDR: &str = "http://localhost:50051";

#[derive(Parser)]
#[command(name = "ergors", version = "0.1.0")]
#[command(about = "Ergors: Ergodic Recursive Systems - unified CLI and daemon")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    /// The home directory used to store configuration and data.
    #[clap(long, default_value_t = default_home(), env = "ERGORS_HOME")]
    pub home: Utf8PathBuf,

    /// Engine gRPC address (for remote commands)
    #[arg(long, default_value = DEFAULT_GRPC_ADDR, env = "ERGORS_GRPC_ADDR")]
    pub grpc_addr: String,

    /// Log level
    #[arg(long, default_value = "info")]
    pub log_level: String,

    /// Output in JSON format (for scripting)
    #[arg(long)]
    pub json: bool,
}

#[derive(Subcommand)]
pub enum Commands {
    // =========== Daemon Control ===========
    /// Start the engine daemon (HTTP API + gRPC management server)
    Start {
        /// gRPC management server port
        #[arg(long, default_value = "50051", env = "ERGORS_GRPC_PORT")]
        grpc_port: u16,
    },
    /// Stop the running engine daemon
    Stop {
        /// Force immediate shutdown
        #[arg(short, long)]
        force: bool,
    },
    /// Show engine status
    Status,
    /// Restart the engine daemon
    Restart {
        /// Force immediate shutdown before restart
        #[arg(short, long)]
        force: bool,
    },

    // =========== Local Commands (no daemon needed) ===========
    /// Initialize configuration and data directories
    Init(InitCmd),
    /// Manage configuration values (local file operations)
    Config(ConfigCmd),
    /// Manage cosmos funding keys (import mnemonic, list, set-default)
    Keys(KeysCmd),
    /// Manage authorization
    ManageAuth(AuthCmd),

    // =========== gRPC Commands (need daemon running) ===========
    /// Node identity management
    #[command(subcommand)]
    Node(NodeCmd),
    /// Network and peer management
    #[command(subcommand)]
    Network(NetworkCmd),
    /// LLM provider management
    #[command(subcommand)]
    Provider(ProviderCmd),
    /// Git workspace management
    #[command(subcommand)]
    Workspace(WorkspaceCmd),
    /// Akash deployment management
    #[command(subcommand)]
    Deploy(DeployCmd),
    /// SDL template management
    #[command(subcommand)]
    Sdl(SdlCmd),
    /// RAG vector database management
    #[command(subcommand)]
    Rag(RagCmd),
    /// Runtime configuration (via daemon)
    #[command(subcommand)]
    RuntimeConfig(RemoteConfigCmd),

    /// Execute a call (TODO)
    Call(CallCmd),
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    // Ensure that the home directory exists
    fs::create_dir_all(&cli.home)
        .with_context(|| format!("Failed to create home directory {}", cli.home))?;

    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| cli.log_level.clone().into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Route command based on type
    match &cli.command {
        // Local commands (synchronous, no daemon needed)
        Commands::Init(cmd) => cmd.init(cli.home.as_path())?,
        Commands::Config(cmd) => cmd.exec(cli.home.as_path())?,
        Commands::Keys(cmd) => cmd.exec(cli.home.as_path())?,
        Commands::ManageAuth(cmd) => cmd.exec(cli.home.as_path())?,
        Commands::Call(_) => todo!(),

        // Daemon start (special case - runs the server)
        Commands::Start { grpc_port } => start(&cli, *grpc_port)?,

        // gRPC commands (async, need daemon running)
        _ => run_async_command(cli)?,
    }

    Ok(())
}

/// Run async commands that require gRPC connection to daemon
fn run_async_command(cli: Cli) -> Result<()> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(async move { execute_grpc_command(&cli).await })
}

/// Execute a gRPC command against the daemon
async fn execute_grpc_command(cli: &Cli) -> Result<()> {
    let ctx = CliContext {
        home: cli.home.clone(),
        grpc_addr: cli.grpc_addr.clone(),
        json: cli.json,
    };

    // Create gRPC client (lazy - some commands check if daemon is running first)
    let client = ManagementClient::connect(&cli.grpc_addr).await;

    match &cli.command {
        // Daemon control commands
        Commands::Stop { force } => {
            let mut client = client?;
            let result = client.shutdown(*force).await?;
            if result.success {
                println!("Engine shutdown initiated");
            } else {
                println!("Shutdown failed: {}", result.message);
            }
        }
        Commands::Status => {
            EngineCmd::Status.execute(&ctx, client).await?;
        }
        Commands::Restart { force } => {
            EngineCmd::Restart { force: *force }.execute(&ctx, client).await?;
        }

        // gRPC commands
        Commands::Node(cmd) => cmd.execute(&ctx, client?).await?,
        Commands::Network(cmd) => cmd.execute(&ctx, client?).await?,
        Commands::Provider(cmd) => cmd.execute(&ctx, client?).await?,
        Commands::Workspace(cmd) => cmd.execute(&ctx, client?).await?,
        Commands::Deploy(cmd) => cmd.execute(&ctx, client?).await?,
        Commands::Sdl(cmd) => cmd.execute(&ctx, client?).await?,
        Commands::Rag(cmd) => cmd.execute(&ctx, client?).await?,
        Commands::RuntimeConfig(cmd) => cmd.execute(&ctx, client?).await?,

        // Local commands handled in main()
        Commands::Start { .. }
        | Commands::Init(_)
        | Commands::Config(_)
        | Commands::Keys(_)
        | Commands::ManageAuth(_)
        | Commands::Call(_) => {
            unreachable!("Local commands should be handled in main()")
        }
    }

    Ok(())
}

/// Start the daemon
pub fn start(cli: &Cli, grpc_port: u16) -> HoResult<()> {
    init_env(cli.home.as_path())?;

    // Initialize daemon manager (PID file handling)
    let daemon = Daemon::new(cli.home.as_path());

    // Check if already running
    if daemon.is_running() {
        if let Some(pid) = daemon.get_pid() {
            error!("Engine is already running (PID: {})", pid);
            return Err(ho_std::llm::HoError::Cfg(format!(
                "Engine already running (PID: {})",
                pid
            )));
        }
    }

    // Acquire PID lock
    daemon
        .acquire_lock()
        .map_err(|e| ho_std::llm::HoError::Cfg(format!("Failed to acquire PID lock: {}", e)))?;

    info!("Starting ERGORS engine...");

    // commonware runtime of the server with the config defined.
    let home = cli.home.clone();
    Runner::new(RuntimeConfig::default()).start(|context| async move {
        // Set up signal handlers
        let (signal_handler, mut shutdown_rx, _reload_rx) = SignalHandler::new();
        if let Err(e) = signal_handler.setup().await {
            error!("Failed to set up signal handlers: {}", e);
            return;
        }

        let config = match ErgorsConfig::load(home.as_path().join(CONFIG_FILE_NAME)) {
            Ok(c) => c,
            Err(e) => {
                error!("Failed to load config: {}", e);
                return;
            }
        };

        let s = match CwHoServer::new(config, context).await {
            Ok(s) => s,
            Err(e) => {
                error!("Failed to create server: {}", e);
                return;
            }
        };

        // Get app state for gRPC service
        let app_state = s.state();
        let _shutdown_tx = signal_handler.subscribe_shutdown();

        // Create gRPC management service
        let grpc_service =
            ManagementServiceImpl::new(app_state, tokio::sync::broadcast::channel(1).0);

        // Spawn gRPC management server
        let grpc_addr: SocketAddr = format!("0.0.0.0:{}", grpc_port)
            .parse()
            .expect("Invalid gRPC address");

        let grpc_handle = tokio::spawn(async move {
            if let Err(e) = start_grpc_server(grpc_addr, grpc_service).await {
                error!("gRPC server error: {}", e);
            }
        });

        info!("gRPC management server listening on {}", grpc_addr);

        // Run HTTP API server in parallel with shutdown monitoring
        tokio::select! {
            result = s.run() => {
                if let Err(e) = result {
                    error!("HTTP server error: {}", e);
                    error!("Backtrace: {:#?}", e.backtrace());
                }
            }
            _ = shutdown_rx.recv() => {
                info!("Shutdown signal received, stopping servers...");
            }
        }

        // Cleanup
        grpc_handle.abort();
        info!("ERGORS engine stopped");
    });

    // Release PID lock on exit
    if let Err(e) = daemon.release_lock() {
        error!("Failed to release PID lock: {}", e);
    }

    Ok(())
}
