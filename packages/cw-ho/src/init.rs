use anyhow::Result;

use ho_std::constants::LLM_API_KEYS_FILE;
use ho_std::git::GitIdentity;
use ho_std::keys::commonware::NodePrivKey;
use ho_std::llm::configure_api_keys_interactive;
use ho_std::traits::{HoConfigTrait, NodeIdentityTrait};
use ho_std::types::keys::v1::SpendKey;

use std::io::{IsTerminal as _, Read};
use std::{env, fs};

use crate::config::ErgorsConfig;

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
    Llms {},
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
                let config = ErgorsConfig::new(home_dir.as_ref());
                let current = env::current_dir().unwrap();
                let template_path = camino::Utf8Path::new(current.to_str().unwrap());
                let output_path = home_dir.as_ref().join(".env");

                println!("{:#?}", output_path);
                let env_content = fs::read_to_string(template_path.join("templates/example.env"))
                    .expect("Failed to read templates/example.env. Make sure it exists.");
                std::fs::write(output_path, env_content).expect("Failed to write.");

                // Generate SSH keys for git workspace operations
                let ssh_dir = home_dir.as_ref().join("ssh");
                if let Err(e) = self.generate_ssh_keys(&ssh_dir, &config) {
                    eprintln!("Warning: Failed to generate SSH keys: {}", e);
                    eprintln!("You can manually generate them later with 'ergors init ssh-keys'");
                }

                config
            }
            InitTopSubCmd::Llms {} => {
                // Run interactive API keys configuration
                let api_keys_path = home_dir.as_ref().join(LLM_API_KEYS_FILE);
                configure_api_keys_interactive(&api_keys_path)?;
                println!("\n✅ API keys configured successfully!");
                println!("   File: {}", api_keys_path);
                println!("   Remember to add this file to .gitignore!");
                ErgorsConfig::load(&config_path)?
            }
            InitTopSubCmd::UnsafeWipe {} => {
                let new_config = self.fresh(home_dir.as_ref());
                println!("Deleting all data in {}...", home_dir.as_ref());
                std::fs::remove_dir_all(home_dir.as_ref())?;
                new_config
            }
            InitTopSubCmd::Migrate {} => {
                // TODO: implement interface for modular migrations
                ErgorsConfig::load(&config_path)?
            }
        };

        println!("Writing generated config to {}", &config_path);
        config.save(config_path)?;

        Ok(())
    }

    fn fresh(&self, home_dir: impl AsRef<camino::Utf8Path>) -> ErgorsConfig {
        let config = ErgorsConfig::new(home_dir.as_ref());
        // generate default env file in home dir as well

        config
    }

    /// Generate SSH keys for git workspace operations
    ///
    /// Creates ED25519 SSH keys derived from the node identity for use with
    /// git operations (clone, push, pull).
    fn generate_ssh_keys(&self, ssh_dir: &camino::Utf8Path, config: &ErgorsConfig) -> Result<()> {
        use ho_std::traits::NodeIdentityTrait;

        // Get node identity from config
        let identity = config.identity();

        // Get or generate private key
        let private_key = if let Some(pk_bytes) = &identity.private_key {
            NodePrivKey::from_bytes(pk_bytes)
                .ok_or_else(|| anyhow::anyhow!("Invalid private key in config"))?
        } else {
            // Generate new key if none exists
            NodePrivKey::new(&mut rand::rngs::OsRng)
        };

        let public_key = private_key.id();

        // Create git identity and write SSH keys
        let git_identity = GitIdentity::from_node_keys(&private_key, &public_key)
            .map_err(|e| anyhow::anyhow!("Failed to create git identity: {}", e))?;

        git_identity
            .write_ssh_keys(ssh_dir.as_std_path())
            .map_err(|e| anyhow::anyhow!("Failed to write SSH keys: {}", e))?;

        println!("SSH keys generated:");
        println!("  Private: {}/id_ed25519", ssh_dir);
        println!("  Public:  {}/id_ed25519.pub", ssh_dir);
        println!("  Fingerprint: {}", git_identity.ssh_fingerprint());
        println!();
        println!("Add the public key to your git remotes for authentication.");

        Ok(())
    }
}

impl SoftKmsInitCmd {
    fn spend_key(&self, init_type: InitType) -> Result<SpendKey> {
        // Ok(match self {
        //     SoftKmsInitCmd::Generate { stdout } => {
        //         let seed_phrase = SeedPhrase::generate(OsRng);
        //         let seed_msg = format!(
        //             "YOUR PRIVATE SEED PHRASE ({init_type:?}):\n\n\
        //            {seed_phrase}\n\n\
        //            Save this in a safe place!\n\
        //            DO NOT SHARE WITH ANYONE!\n"
        //         );

        //         let mut output = std::io::stdout();
        //         let mut screen = output.into_alternate_screen()?;
        //         writeln!(screen, "{seed_msg}")?;
        //         screen.flush()?;
        //         println!("Press enter to proceed.");
        //         let _ = stdin().bytes().next();

        //         SpendKey::from_seed_phrase_bip39(seed_phrase, 0)
        //     }
        //     SoftKmsInitCmd::ImportPhrase {} => {
        //         let seed_phrase = prompt_for_password("Enter seed phrase: ")?;
        //         let seed_phrase = SeedPhrase::from_str(&seed_phrase)
        //             .context("failed to parse input as seed phrase")?;

        //         SpendKey::from_seed_phrase_bip39(seed_phrase, 0)
        //     }
        // })
        unimplemented!()
    }
}
