use anyhow::{Context as _, Result};
use camino::Utf8PathBuf;
use clap::{Parser, Subcommand};
use commonware_runtime::{
    tokio::{Config as RuntimeConfig, Runner},
    Runner as _,
};
use ergors::{
    auth::AuthCmd, call::CallCmd, config::ErgorsConfig, init::InitCmd, server::Server as CwHoServer,
};
use ho_std::{
    constants::{default_home, init_env, CONFIG_FILE_NAME},
    error::HoResult,
    traits::HoConfigTrait,
};

use std::fs;
use tracing::error;
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
    /// Start the HTTP API server
    Start {},
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
        Commands::Start {} => start(cli)?,
        Commands::ManageAuth(cmd) => cmd.exec(cli.home.as_path())?,
        Commands::Call(_) => todo!(),
    }

    Ok(())
}

pub fn start(cli: Cli) -> HoResult<()> {
    init_env(cli.home.as_path())?;
    // commonware runtime of the server with the config defined.
    Runner::new(RuntimeConfig::default()).start(|context| async move {
        let s = match CwHoServer::new(
            ErgorsConfig::load(&cli.home.as_path().join(CONFIG_FILE_NAME))
                .expect("loading config error"),
            context,
        )
        .await
        {
            Ok(s) => s,
            Err(e) => {
                error!("Failed to start server: {}", e);
                return;
            }
        };
        if let Err(e) = s.run().await {
            error!("Ergors Error: {}", e);
            error!("Ergors Error: {:#?}", e.backtrace());
        }
    });
    Ok(())
}
