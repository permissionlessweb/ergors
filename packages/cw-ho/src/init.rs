use crate::CwHoConfig;
use anyhow::{Context, Result};
use camino::Utf8Path;
use ho_std::config::api_keys::configure_api_keys_interactive;
use ho_std::constants::{ENV_VARIABLES_FILE, LLM_API_KEYS_FILE};
use ho_std::traits::DomainType;
use ho_std::traits::HoConfigTrait;
use ho_std_keys::keys::{SeedPhrase, SpendKey};
use rand_core::OsRng;
use std::{env, fs};
use std::{
    io::{stdin, IsTerminal as _, Read, Write},
    str::FromStr,
};
use termion::screen::IntoAlternateScreen;

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

/// Which kind of initialization are we doing?
#[derive(Clone, Debug, Copy)]
enum InitType {
    /// Initialize from scratch with a spend key.
    SpendKey,
    /// Add a governance key to an existing configuration.
    GovernanceKey,
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
fn prompt_for_password(msg: &str) -> Result<String> {
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
    pub fn init(&self, home_dir: impl AsRef<camino::Utf8Path>) -> Result<()> {
        let config_path = home_dir.as_ref().join(ho_std::constants::CONFIG_FILE_NAME);
        let config = match self.subcmd.clone() {
            InitTopSubCmd::New {} => {
                let config = CwHoConfig::new(home_dir.as_ref());
                let current = env::current_dir().unwrap();
                let template_path = camino::Utf8Path::new(current.to_str().unwrap());
                let output_path = home_dir.as_ref().join(".env");

                println!("{:#?}", output_path);
                let env_content = fs::read_to_string(template_path.join("templates/example.env"))
                    .expect("Failed to read templates/example.env. Make sure it exists.");
                std::fs::write(output_path, env_content).expect("Failed to write.");
                config
            }
            InitTopSubCmd::LlmApiKeys {} => {
                // Run interactive API keys configuration
                let api_keys_path = home_dir.as_ref().join(LLM_API_KEYS_FILE);
                configure_api_keys_interactive(&api_keys_path)?;
                println!("\n✅ API keys configured successfully!");
                println!("   File: {}", api_keys_path);
                println!("   Remember to add this file to .gitignore!");
                CwHoConfig::load(&config_path)?
            }
            InitTopSubCmd::UnsafeWipe {} => {
                let new_config = self.fresh(home_dir.as_ref());
                println!("Deleting all data in {}...", home_dir.as_ref());
                std::fs::remove_dir_all(home_dir.as_ref())?;
                new_config
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

    fn fresh(&self, home_dir: impl AsRef<camino::Utf8Path>) -> CwHoConfig {
        let config = CwHoConfig::new(home_dir.as_ref());
        // generate default env file in home dir as well

        config
    }
}

impl SoftKmsInitCmd {
    fn spend_key(&self, init_type: InitType) -> Result<SpendKey> {
        Ok(match self {
            SoftKmsInitCmd::Generate { stdout } => {
                let seed_phrase = SeedPhrase::generate(OsRng);
                let seed_msg = format!(
                    "YOUR PRIVATE SEED PHRASE ({init_type:?}):\n\n\
                   {seed_phrase}\n\n\
                   Save this in a safe place!\n\
                   DO NOT SHARE WITH ANYONE!\n"
                );

                let mut output = std::io::stdout();
                let mut screen = output.into_alternate_screen()?;
                writeln!(screen, "{seed_msg}")?;
                screen.flush()?;
                println!("Press enter to proceed.");
                let _ = stdin().bytes().next();

                SpendKey::from_seed_phrase_bip39(seed_phrase, 0)
            }
            SoftKmsInitCmd::ImportPhrase {} => {
                let seed_phrase = prompt_for_password("Enter seed phrase: ")?;
                let seed_phrase = SeedPhrase::from_str(&seed_phrase)
                    .context("failed to parse input as seed phrase")?;

                SpendKey::from_seed_phrase_bip39(seed_phrase, 0)
            }
        })
    }
}
