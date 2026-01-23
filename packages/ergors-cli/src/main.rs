//! ERGORS CLI - Lightweight client for engine management
//!
//! This binary communicates with the ergors-engine daemon via gRPC.

use anyhow::Result;
use camino::Utf8PathBuf;
use clap::{Parser, Subcommand};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

mod client;
mod commands;

use client::ManagementClient;
use commands::{ConfigCmd, DeployCmd, EngineCmd, NetworkCmd, NodeCmd, ProviderCmd, WorkspaceCmd};

/// Default gRPC address for the engine
const DEFAULT_GRPC_ADDR: &str = "http://localhost:50051";

/// Default home directory
fn default_home() -> Utf8PathBuf {
    dirs::home_dir()
        .map(|p| Utf8PathBuf::from_path_buf(p).unwrap_or_default())
        .unwrap_or_else(|| Utf8PathBuf::from("."))
        .join(".ergors")
}

#[derive(Parser)]
#[command(name = "ergors-cli", version = "0.1.0")]
#[command(about = "CLI client for ERGORS engine management")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    /// Home directory for configuration
    #[arg(long, default_value_t = default_home(), env = "ERGORS_HOME")]
    pub home: Utf8PathBuf,

    /// Engine gRPC address
    #[arg(long, default_value = DEFAULT_GRPC_ADDR, env = "ERGORS_GRPC_ADDR")]
    pub grpc_addr: String,

    /// Log level
    #[arg(long, default_value = "warn")]
    pub log_level: String,

    /// Output in JSON format (for scripting)
    #[arg(long)]
    pub json: bool,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Engine daemon control
    #[command(subcommand)]
    Engine(EngineCmd),

    /// Node identity management
    #[command(subcommand)]
    Node(NodeCmd),

    /// Configuration management
    #[command(subcommand)]
    Config(ConfigCmd),

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

    /// Show engine status (shortcut for engine status)
    Status,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| cli.log_level.clone().into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Create gRPC client
    let client = ManagementClient::connect(&cli.grpc_addr).await;

    match &cli.command {
        Commands::Engine(cmd) => cmd.execute(&cli, client).await?,
        Commands::Node(cmd) => cmd.execute(&cli, client?).await?,
        Commands::Config(cmd) => cmd.execute(&cli, client?).await?,
        Commands::Network(cmd) => cmd.execute(&cli, client?).await?,
        Commands::Provider(cmd) => cmd.execute(&cli, client?).await?,
        Commands::Workspace(cmd) => cmd.execute(&cli, client?).await?,
        Commands::Deploy(cmd) => cmd.execute(&cli, client?).await?,
        Commands::Status => {
            // Shortcut for engine status
            EngineCmd::Status.execute(&cli, client).await?;
        }
    }

    Ok(())
}
