use std::fs;

use anyhow::{Context, Result};
use clap::Parser;

use cw_ho::{start, Cli, Commands};
use ho_std::config::env::init_env;

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
        Commands::ManageAuth(cmd) => cmd.exec(cli.home.as_path())?,
    }

    Ok(())
}
