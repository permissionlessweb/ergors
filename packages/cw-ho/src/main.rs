use anyhow::Result;
use clap::Parser;

use cw_ho::{init, start, Cli, Commands};

use tracing::{error, info};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| cli.log_level.clone().into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    match cli.command {
        Commands::Init { output } => init(output)?,
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
