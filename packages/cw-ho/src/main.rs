use std::fs;

use anyhow::{Context, Result};
use clap::Parser;

use cw_ho::init::InitCmd;
use cw_ho::{init, start, Cli, Commands};
use ho_std::config::env::init_env;

use tracing::{error, info};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

fn main() -> Result<()> {
    let cli = Cli::parse();

    //Ensure that the data_path exists, in case this is a cold start
    fs::create_dir_all(&cli.home)
        .with_context(|| format!("Failed to create home directory {}", cli.home))?;

    init_env();
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
        Commands::Start { port } => start(cli, port)?,
        Commands::Health { endpoint } => health(endpoint)?,
    }

    Ok(())
}

fn health(endpoint: String) -> Result<()> {
    info!("🔍 Checking health at: {}", endpoint);

    // Use a simple tokio runtime for the health check
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        let client = reqwest::Client::new();

        match client.get(&format!("{}/health", endpoint)).send().await {
            Ok(response) => {
                if response.status().is_success() {
                    info!("✅ Service is healthy");
                } else {
                    error!("❌ Service returned status: {}", response.status());
                    std::process::exit(1);
                }
            }
            Err(e) => {
                error!("❌ Failed to connect: {}", e);
                std::process::exit(1);
            }
        }
    });
    Ok(())
}
