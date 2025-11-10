use anyhow::Result;
use camino::Utf8Path;
use ho_std::config::api_keys::configure_api_keys_interactive;
use ho_std::constants::{ENV_VARIABLES_FILE, LLM_API_KEYS_FILE};
use ho_std::traits::HoConfigTrait;
use std::io::{IsTerminal, Read};

use crate::CwHoConfig;

#[derive(Debug, clap::Parser)]
pub struct InitCmd {
    #[clap(subcommand)]
    pub subcmd: InitTopSubCmd,
}

#[derive(Debug, Clone, clap::Subcommand)]
pub enum InitTopSubCmd {
    // configure llm api keys
    // #[clap(flatten)]
    #[clap(display_order = 100)]
    New {},
    // prompt cli helper for guiding through configuring api keys
    #[clap(display_order = 200)]
    LlmApiKeys {},
    // configure
    #[clap(display_order = 900)]
    UnsafeWipe {},
    // used for migrating from major versions if applicable
    #[clap(display_order = 1000)]
    Migrate {},
}

#[derive(Debug, Clone, clap::Subcommand)]
pub enum InitSubCmd {
    /// Initialize using a basic, file-based custody backend.
    #[clap(subcommand, display_order = 100)]
    SoftKms(SoftKmsInitCmd),
}

#[derive(Debug, Clone, clap::Subcommand)]
pub enum SoftKmsInitCmd {
    /// Generate a new seed phrase and import its corresponding key.
    #[clap(display_order = 100)]
    Generate {
        /// If set, will write the seed phrase to stdout.
        #[clap(long, action)]
        stdout: bool,
    },
    /// Import a spend key from an existing seed phrase.
    #[clap(display_order = 200)]
    ImportPhrase {},
}

// Reusable function for prompting interactively for key material.
fn _prompt_for_password(msg: &str) -> Result<String> {
    let mut password = String::new();
    // The `rpassword` crate doesn't support reading from stdin, so we check
    // for an interactive session. We must support non-interactive use cases,
    // for integration with other tooling.
    if std::io::stdin().is_terminal() {
        password = rpassword::prompt_password(msg)?;
    } else {
        while let Ok(n_bytes) = std::io::stdin().lock().read_to_string(&mut password) {
            if n_bytes == 0 {
                break;
            }
            password = password.trim().to_string();
        }
    }
    Ok(password)
}

impl InitCmd {
    pub fn init(&self, home_dir: &Utf8Path) -> Result<()> {
        let config_path = home_dir.join(ho_std::constants::CONFIG_FILE_NAME);
        let config = match self.subcmd.clone() {
            InitTopSubCmd::New {} => {
                let config = CwHoConfig::new(home_dir);
                // generate env file in home dir as well
                let env_file_path = home_dir.join(ENV_VARIABLES_FILE);
                config
            }
            InitTopSubCmd::LlmApiKeys {} => {
                // Run interactive API keys configuration
                let api_keys_path = home_dir.join(LLM_API_KEYS_FILE);
                configure_api_keys_interactive(&api_keys_path)?;
                println!("\n✅ API keys configured successfully!");
                println!("   File: {}", api_keys_path);
                println!("   Remember to add this file to .gitignore!");
                CwHoConfig::load(&config_path)?
            }
            InitTopSubCmd::UnsafeWipe {} => {
                let config = CwHoConfig::load(&config_path)?;
                config
            }
            InitTopSubCmd::Migrate {} => {
                // TODO: implement interface for modular migrations
                CwHoConfig::load(&config_path)?
            }
        };

        println!("Writing generated config to {}", &config_path);
        config.save(config_path)?;

        Ok(())
    }
}
