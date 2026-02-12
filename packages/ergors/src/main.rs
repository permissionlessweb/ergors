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
    auth::grpc::AuthorizedCliKeys,
    auth::AuthCmd,
    client::grpc::{start_grpc_server, ManagementServiceImpl},
    client::sentinel::SentinelServer,
    client::ManagementClient,
    client::RlmDocService,
    commands::{
        call::CallCmd, config::ConfigCmd, init::InitCmd, sentinel::SentinelCmd, AskCmd,
        BootstrapCmd, CliContext, DeployCmd, DocumentCmd, EngineCmd, GatewayCmd, NetworkCmd,
        NodeCmd, ProviderCmd, RemoteConfigCmd, SdlCmd, WorkspaceCmd,
    },
    config::ErgorsConfig,
    daemon::{Daemon, SignalHandler},
    keys::KeysCmd,
    server::Server as CwHoServer,
};
use ho_std::{
    constants::{default_home, init_env, CONFIG_FILE_NAME,DEFAULT_GRPC_ADDR},
    error::HoResult,
    traits::HoConfigTrait,
};

use std::fs;
use std::net::SocketAddr;
use tracing::{error, info};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

/// Install a panic hook to restore terminal state on crash.
///
/// This prevents terminal corruption (echo disabled) when a panic occurs
/// during password input (rpassword disables echo while reading).
fn install_terminal_restore_hook() {
    let original_hook = std::panic::take_hook();

    std::panic::set_hook(Box::new(move |info| {
        // Restore terminal state before panicking
        #[cfg(unix)]
        {
            use std::os::unix::io::AsRawFd;
            if let Ok(mut termios) = termios::Termios::from_fd(std::io::stdin().as_raw_fd()) {
                termios.c_lflag |= termios::ECHO | termios::ICANON;
                let _ =
                    termios::tcsetattr(std::io::stdin().as_raw_fd(), termios::TCSANOW, &termios);
            }
        }
        eprintln!();
        original_hook(info);
    }));
}

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

    /// Ed25519 signing key for authenticated remote access (64 hex chars)
    #[arg(long, env = "ERGORS_SIGNING_KEY_HEX")]
    pub signing_key_hex: Option<String>,
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
    /// Sentinel: bootstrap a remote sentinel node
    Sentinel(SentinelCmd),

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
    /// Bootstrap new nodes via Akash or SSH
    #[command(subcommand)]
    Bootstrap(BootstrapCmd),
    /// Akash deployment management
    #[command(subcommand)]
    Deploy(DeployCmd),
    /// SDL template management
    #[command(subcommand)]
    Sdl(SdlCmd),
    /// Ask: Document ingestion and querying (RAG + RLM)
    #[command(subcommand)]
    Ask(AskCmd),
    /// Document storage (non-RAG): ingest, retrieve, list, delete
    #[command(subcommand)]
    Document(DocumentCmd),
    /// Runtime configuration (via daemon)
    #[command(subcommand)]
    RuntimeConfig(RemoteConfigCmd),

    /// Make inference calls through the node's HTTP proxy
    Call(CallCmd),

    /// Communication gateway management (Discord, Nostr, Element)
    #[command(subcommand)]
    Gateway(GatewayCmd),
}

fn main() -> Result<()> {
    // Install panic hook to restore terminal state on crash/abort
    // This prevents terminal corruption if process is killed during password input
    install_terminal_restore_hook();

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

    // Initialize rustls crypto provider (required for layer-climb/tonic TLS)
    // This must be called before any gRPC/TLS operations
    rustls::crypto::aws_lc_rs::default_provider()
        .install_default()
        .map_err(|_| anyhow::anyhow!("Failed to install rustls crypto provider"))?;

    // Route command based on type
    match &cli.command {
        // Local commands (synchronous, no daemon needed)
        Commands::Init(cmd) => cmd.init(cli.home.as_path())?,
        Commands::Config(cmd) => cmd.exec(cli.home.as_path(), cli.json)?,
        Commands::Keys(cmd) => cmd.exec(cli.home.as_path(), cli.json)?,
        Commands::ManageAuth(cmd) => cmd.exec(cli.home.as_path())?,
        Commands::Sentinel(cmd) => cmd.exec(cli.home.as_path())?,
        Commands::Call(cmd) => cmd.exec(cli.home.as_path(), &cli.grpc_addr, cli.json)?,

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

/// Resolve signing key for remote gRPC connections.
/// Local connections don't need a key; remote connections require one.
fn resolve_signing_key(
    addr: &str,
    key_hex: Option<&str>,
) -> Result<Option<ho_std::keys::commonware::NodePrivKey>> {
    let is_local = is_loopback_addr(addr);

    match (is_local, key_hex) {
        (true, _) => Ok(None),
        (false, Some(hex)) => {
            let key = ho_std::keys::commonware::NodePrivKey::from_hex(hex).ok_or_else(|| {
                anyhow::anyhow!("Invalid --signing-key-hex (expected 64 hex chars)")
            })?;
            Ok(Some(key))
        }
        (false, None) => {
            anyhow::bail!(
                "Remote gRPC target requires --signing-key-hex or ERGORS_SIGNING_KEY_HEX.\n\
                 Register your public key on the engine with: ergors config register-cli-key <pubkey_hex>"
            );
        }
    }
}

fn is_loopback_addr(addr: &str) -> bool {
    // Strip scheme (http://, https://) and port to extract host
    let stripped = addr
        .strip_prefix("http://")
        .or_else(|| addr.strip_prefix("https://"))
        .unwrap_or(addr);
    let host = stripped.split(':').next().unwrap_or(stripped);

    matches!(
        host,
        "localhost" | "127.0.0.1" | "::1" | "0.0.0.0" | "[::1]"
    )
}

/// Execute a gRPC command against the daemon
async fn execute_grpc_command(cli: &Cli) -> Result<()> {
    let signing_key = resolve_signing_key(&cli.grpc_addr, cli.signing_key_hex.as_deref())?;

    let ctx = CliContext {
        home: cli.home.clone(),
        grpc_addr: cli.grpc_addr.clone(),
        json: cli.json,
        signing_key: signing_key.clone(),
    };

    // Create gRPC client (lazy - some commands check if daemon is running first)
    let client = ManagementClient::connect(&cli.grpc_addr, signing_key).await;

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
            EngineCmd::Restart { force: *force }
                .execute(&ctx, client)
                .await?;
        }

        // gRPC commands
        Commands::Node(cmd) => cmd.execute(&ctx, client?).await?,
        Commands::Network(cmd) => cmd.execute(&ctx, client?).await?,
        Commands::Provider(cmd) => cmd.execute(&ctx, client?).await?,
        Commands::Workspace(cmd) => cmd.execute(&ctx, client?).await?,
        Commands::Bootstrap(cmd) => cmd.execute(&ctx).await?,
        Commands::Deploy(cmd) => cmd.execute(&ctx, client?).await?,
        Commands::Sdl(cmd) => cmd.execute(&ctx, client?).await?,
        Commands::Ask(cmd) => cmd.execute(&ctx, client?).await?,
        Commands::Document(cmd) => cmd.execute(&ctx, client).await?,
        Commands::RuntimeConfig(cmd) => cmd.execute(&ctx, client?).await?,
        Commands::Gateway(cmd) => cmd.execute(&ctx, client?).await?,

        // Local commands handled in main()
        Commands::Start { .. }
        | Commands::Init(_)
        | Commands::Config(_)
        | Commands::Keys(_)
        | Commands::ManageAuth(_)
        | Commands::Sentinel(_)
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

    // Sentinel mode: run lightweight init server when ERGORS_ADMIN_PUBKEY is
    // set and no identity file exists yet. The sentinel collects secrets via
    // Ed25519-signed HTTP requests, then falls through to normal startup.
    let admin_pubkey = std::env::var("ERGORS_ADMIN_PUBKEY").ok();
    let identity_path = cli.home.join("node_identity.enc");

    if let Some(ref pubkey) = admin_pubkey {
        if !identity_path.exists() {
            info!("Sentinel mode: ERGORS_ADMIN_PUBKEY set, no identity — starting sentinel");
            let rt = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .map_err(|e| ho_std::llm::HoError::Cfg(format!("tokio runtime: {}", e)))?;
            let sentinel_pw = rt
                .block_on(async { SentinelServer::new(pubkey, cli.home.clone()).run().await })
                .map_err(|e| ho_std::llm::HoError::Cfg(format!("sentinel failed: {}", e)))?;
            // Sentinel done — config.toml, node_identity.enc, api-keys.enc now exist.
            // Set env var here while still single-threaded (before commonware Runner spawns threads).
            if let Some(pw) = sentinel_pw {
                std::env::set_var("ERGORS_CUSTODY_PASSWORD", &pw);
            }
        }
    }

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

        // Load authorized CLI keys from storage into runtime set
        let authorized_keys = AuthorizedCliKeys::new();
        if let Ok(stored) = app_state.s.get_authorized_cli_keys().await {
            authorized_keys.load_from(&stored.keys);
            if !stored.keys.is_empty() {
                info!(
                    "Loaded {} authorized CLI keys from storage",
                    stored.keys.len()
                );
            }
        }

        // Create gRPC management service
        let grpc_service = ManagementServiceImpl::new(
            app_state.clone(),
            tokio::sync::broadcast::channel(1).0,
            authorized_keys.clone(),
        );

        // Create RLM document service
        let rlm_service = Some(RlmDocService::new(app_state.s.clone()));

        // Spawn gRPC management server
        let grpc_addr: SocketAddr = format!("0.0.0.0:{}", grpc_port)
            .parse()
            .expect("Invalid gRPC address");

        let grpc_handle = tokio::spawn(async move {
            if let Err(e) =
                start_grpc_server(grpc_addr, grpc_service, rlm_service, authorized_keys).await
            {
                error!("gRPC server error: {}", e);
            }
        });

        info!("gRPC management server listening on {}", grpc_addr);

        // Create shutdown signal for HTTP server

        // Run HTTP API server with graceful shutdown
        if let Err(e) = s
            .run(async move {
                let _ = shutdown_rx.recv().await;
                info!("Shutdown signal received, stopping servers...");
            })
            .await
        {
            error!("HTTP server error: {}", e);
        }

        // Cleanup gRPC server - abort and wait for it to finish
        grpc_handle.abort();
        let _ = grpc_handle.await;

        // Give async tasks time to clean up their Arc references
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        info!("ERGORS engine stopped");
    });

    // Release PID lock on exit
    if let Err(e) = daemon.release_lock() {
        error!("Failed to release PID lock: {}", e);
    }

    Ok(())
}
