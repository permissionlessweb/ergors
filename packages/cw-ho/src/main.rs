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
    config::ErgorsConfig,
    daemon::{Daemon, SignalHandler},
    grpc::management::{start_grpc_server, ManagementServiceImpl},
    init::InitCmd,
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

#[derive(Parser)]
#[command(name = "ergors", version = "0.1.0")]
#[command(about = "Ergors: Ergodic Recursive Systems")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    /// The home directory used to store configuration and data.
    #[clap(long, default_value_t = default_home(), env = "NODE_DATA_PATH")]
    pub home: Utf8PathBuf,

    /// Log level
    #[arg(long, default_value = "info")]
    pub log_level: String,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Start the engine (HTTP API + gRPC management server)
    Start {
        /// gRPC management server port
        #[arg(long, default_value = "50051", env = "ERGORS_GRPC_PORT")]
        grpc_port: u16,
    },
    /// Generate a sample configuration file
    Init(InitCmd),
    /// register/revoke
    ManageAuth(AuthCmd),
    /// register/revoke
    Call(CallCmd),
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    //Ensure that the data_path exists, in case this is a cold start
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

    match cli.command {
        Commands::Init(cmd) => cmd.init(cli.home.as_path())?,
        Commands::Start { grpc_port } => start(cli, grpc_port)?,
        Commands::ManageAuth(cmd) => cmd.exec(cli.home.as_path())?,
        Commands::Call(_) => todo!(),
    }

    Ok(())
}

pub fn start(cli: Cli, grpc_port: u16) -> HoResult<()> {
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
    Runner::new(RuntimeConfig::default()).start(|context| async move {
        // Set up signal handlers
        let (signal_handler, mut shutdown_rx, _reload_rx) = SignalHandler::new();
        if let Err(e) = signal_handler.setup().await {
            error!("Failed to set up signal handlers: {}", e);
            return;
        }

        let config = match ErgorsConfig::load(&cli.home.as_path().join(CONFIG_FILE_NAME)) {
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
        let shutdown_tx = signal_handler.subscribe_shutdown();

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
