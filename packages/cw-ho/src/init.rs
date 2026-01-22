use anyhow::Result;

use ho_std::constants::LLM_API_KEYS_FILE;
use ho_std::custody::PasswordEncryptedCustody;
use ho_std::git::GitIdentity;
use ho_std::keys::commonware::NodePrivKey;
use ho_std::llm::{configure_api_keys_interactive, EncryptedApiKeyManager};
use ho_std::storage::identity::EncryptedIdentityBuilder;
use ho_std::traits::{HoConfigTrait, NodeIdentityCustody, NodeIdentityTrait};
use ho_std::types::ergors::network::v1::{
    KeySharingMode, ProviderOwnership, ProviderSharingConfig, SecretSharingConfig,
};
use ho_std::types::keys::v1::SpendKey;

use std::collections::HashMap;
use std::io::{IsTerminal as _, Read, Write};
use std::{env, fs};

use crate::config::ErgorsConfig;

/// Provider sharing configuration file structure
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ProvidersConfigFile {
    pub providers: Vec<ProviderSharingConfig>,
}

impl ProvidersConfigFile {
    /// Load providers config from file
    pub fn load(path: &camino::Utf8Path) -> Result<Self> {
        let contents = fs::read_to_string(path)?;
        Ok(toml::from_str(&contents)?)
    }

    /// Get provider config by name
    pub fn get_provider(&self, name: &str) -> Option<&ProviderSharingConfig> {
        self.providers.iter().find(|p| p.name == name)
    }

    /// List shared providers
    pub fn shared_providers(&self) -> Vec<&ProviderSharingConfig> {
        self.providers
            .iter()
            .filter(|p| p.ownership == ProviderOwnership::Shared as i32)
            .collect()
    }

    /// List local providers
    pub fn local_providers(&self) -> Vec<&ProviderSharingConfig> {
        self.providers
            .iter()
            .filter(|p| p.ownership == ProviderOwnership::Local as i32)
            .collect()
    }
}

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
    // configure provider ownership and key sharing settings
    #[clap(display_order = 300)]
    Providers {},
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

                // Create encrypted node identity (secure by default)
                let identity_path = config.identity_path();
                let custody = PasswordEncryptedCustody::new(&identity_path);

                let password = if custody.exists() {
                    println!("✅ Encrypted node identity already exists at: {}", identity_path);
                    // Need to get password for API key encryption
                    prompt_for_password("Enter custody password for API key encryption: ")?
                } else {
                    println!("\n🔐 Creating encrypted node identity...");
                    println!("This will be used for network authentication and API key encryption.");
                    println!();

                    let password = self.create_custody_password()?;

                    let metadata = EncryptedIdentityBuilder::new()
                        .user(config.identity().user.clone())
                        .host(config.identity().host.clone())
                        .p2p_port(config.identity().p2p_port)
                        .api_port(config.identity().api_port)
                        .node_type(config.identity().node_type.clone())
                        .build();

                    custody.create_identity(&password, Some(metadata))
                        .map_err(|e| anyhow::anyhow!("Failed to create encrypted identity: {}", e))?;

                    println!("✅ Created encrypted node identity at: {}", identity_path);
                    password
                };

                // Generate SSH keys from the encrypted custody
                let ssh_dir = home_dir.as_ref().join("ssh");
                if let Err(e) = self.generate_ssh_keys_from_custody(&ssh_dir, &custody) {
                    eprintln!("Warning: Failed to generate SSH keys: {}", e);
                    eprintln!("You can manually generate them later with 'ergors init ssh-keys'");
                }

                // Configure and encrypt API keys
                println!("\n🔑 LLM Provider API Key Configuration");
                println!("   ────────────────────────────────────");
                println!("   Your API keys will be encrypted using your custody password");
                println!("   and stored securely. Press Enter to skip any provider.\n");

                let encrypted_keys_path = home_dir.as_ref().join("api-keys.enc");
                self.configure_api_keys_encrypted(&encrypted_keys_path, &password)?;

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
            InitTopSubCmd::Providers {} => {
                // Run interactive provider ownership configuration
                let providers_path = home_dir.as_ref().join("providers.toml");
                self.configure_providers_interactive(&providers_path)?;
                println!("\n✅ Provider sharing configuration saved!");
                println!("   File: {}", providers_path);
                ErgorsConfig::load(&config_path)?
            }
            InitTopSubCmd::UnsafeWipe {} => {
                println!("Deleting all data in {}...", home_dir.as_ref());
                if home_dir.as_ref().exists() {
                    std::fs::remove_dir_all(home_dir.as_ref())?;
                }
                // Recreate the directory for fresh config
                std::fs::create_dir_all(home_dir.as_ref())?;
                self.fresh(home_dir.as_ref())
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

    /// Create a custody password with confirmation
    fn create_custody_password(&self) -> Result<String> {
        // Check environment variable first for non-interactive setup
        if let Ok(password) = std::env::var("ERGORS_CUSTODY_PASSWORD") {
            if !password.is_empty() {
                return Ok(password);
            }
        }

        // Interactive password creation
        let password = prompt_for_password("Create custody password: ")?;
        let confirm = prompt_for_password("Confirm custody password: ")?;

        if password != confirm {
            return Err(anyhow::anyhow!("Passwords do not match"));
        }

        if password.len() < 8 {
            return Err(anyhow::anyhow!("Password must be at least 8 characters"));
        }

        Ok(password)
    }

    /// Generate SSH keys from encrypted custody
    ///
    /// Creates ED25519 SSH keys from the custody-protected node identity.
    /// Requires unlocking the custody first (prompts for password if needed).
    fn generate_ssh_keys_from_custody(
        &self,
        ssh_dir: &camino::Utf8Path,
        custody: &PasswordEncryptedCustody,
    ) -> Result<()> {
        if !custody.exists() {
            return Err(anyhow::anyhow!("No encrypted identity found"));
        }

        // Export SSH keys using the custody's export method
        // This requires unlocking if not already unlocked
        let rt = tokio::runtime::Runtime::new()?;
        rt.block_on(async {
            if !custody.is_unlocked() {
                let password = prompt_for_password("Enter custody password for SSH key generation: ")?;
                custody.unlock(&password).await.map_err(|e| {
                    anyhow::anyhow!("Failed to unlock custody: {}", e)
                })?;
            }

            custody.export_ssh_keys(ssh_dir.as_std_path()).await.map_err(|e| {
                anyhow::anyhow!("Failed to export SSH keys: {}", e)
            })?;

            // Get public key for display
            let pubkey = custody.public_key().map_err(|e| {
                anyhow::anyhow!("Failed to get public key: {}", e)
            })?;

            println!("\n🔑 SSH keys generated:");
            println!("  Private: {}/id_ed25519", ssh_dir);
            println!("  Public:  {}/id_ed25519.pub", ssh_dir);
            println!("  Public key (hex): {}...", hex::encode(&pubkey.0.to_vec()[..8]));
            println!();
            println!("Add the public key to your git remotes for authentication.");

            Ok::<(), anyhow::Error>(())
        })?;

        Ok(())
    }

    /// Configure provider ownership and key sharing settings interactively
    ///
    /// Prompts the user to configure each provider's ownership model:
    /// - Shared: Distributed via Shamir secret sharing from coordinator
    /// - Local: Per-node only, not distributed
    fn configure_providers_interactive(&self, path: &camino::Utf8Path) -> Result<()> {
        let providers = vec!["anthropic", "openai", "ollama"];

        println!("\n🔐 Provider Key Distribution Configuration");
        println!("   ----------------------------------------");
        println!("   Configure how API keys are shared across nodes.\n");
        println!("   Shared: Keys distributed via Shamir secret sharing");
        println!("   Local:  Keys stay on this node only\n");

        let mut configs: Vec<ProviderSharingConfig> = Vec::new();

        for provider in &providers {
            println!("\n📦 Provider: {}", provider);

            // Default ownership based on provider type
            let default_ownership = if *provider == "ollama" {
                "local"
            } else {
                "shared"
            };

            let ownership = self.prompt_ownership(provider, default_ownership)?;

            let mut config = ProviderSharingConfig {
                name: provider.to_string(),
                ownership: ownership.into(),
                sharing_config: None,
            };

            // If shared, prompt for Shamir threshold
            if ownership == ProviderOwnership::Shared {
                let (threshold, total) = self.prompt_shamir_config(provider)?;
                config.sharing_config = Some(SecretSharingConfig {
                    mode: KeySharingMode::Shamir.into(),
                    threshold,
                    total_shares: total,
                });
            } else {
                config.sharing_config = Some(SecretSharingConfig {
                    mode: KeySharingMode::Direct.into(),
                    threshold: 1,
                    total_shares: 1,
                });
            }

            configs.push(config);
        }

        // Serialize and save
        let config_wrapper = ProvidersConfigFile { providers: configs };
        let toml_content = toml::to_string_pretty(&config_wrapper)?;
        fs::write(path, toml_content)?;

        Ok(())
    }

    /// Prompt for provider ownership mode
    fn prompt_ownership(&self, provider: &str, default: &str) -> Result<ProviderOwnership> {
        print!("   Ownership [shared/local] (default: {}): ", default);
        std::io::stdout().flush()?;

        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        let input = input.trim().to_lowercase();

        let ownership = if input.is_empty() {
            if default == "shared" {
                ProviderOwnership::Shared
            } else {
                ProviderOwnership::Local
            }
        } else {
            match input.as_str() {
                "shared" | "s" => ProviderOwnership::Shared,
                "local" | "l" => ProviderOwnership::Local,
                _ => {
                    println!("   Invalid input, using default: {}", default);
                    if default == "shared" {
                        ProviderOwnership::Shared
                    } else {
                        ProviderOwnership::Local
                    }
                }
            }
        };

        let ownership_str = match ownership {
            ProviderOwnership::Shared => "shared",
            ProviderOwnership::Local => "local",
            _ => "unspecified",
        };
        println!("   → {}: {}", provider, ownership_str);

        Ok(ownership)
    }

    /// Prompt for Shamir threshold configuration
    fn prompt_shamir_config(&self, provider: &str) -> Result<(u32, u32)> {
        // Default: 2-of-3 threshold
        let default_threshold = 2u32;
        let default_total = 3u32;

        print!(
            "   Shamir threshold (k-of-n) [default: {}-of-{}]: ",
            default_threshold, default_total
        );
        std::io::stdout().flush()?;

        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        let input = input.trim();

        if input.is_empty() {
            println!(
                "   → {}: {}-of-{} threshold sharing",
                provider, default_threshold, default_total
            );
            return Ok((default_threshold, default_total));
        }

        // Parse k-of-n format
        let parts: Vec<&str> = input.split("-of-").collect();
        if parts.len() == 2 {
            if let (Ok(k), Ok(n)) = (parts[0].parse::<u32>(), parts[1].parse::<u32>()) {
                if k > 0 && k <= n && n <= 255 {
                    println!("   → {}: {}-of-{} threshold sharing", provider, k, n);
                    return Ok((k, n));
                }
            }
        }

        println!(
            "   Invalid format, using default: {}-of-{}",
            default_threshold, default_total
        );
        Ok((default_threshold, default_total))
    }

    /// Configure API keys with encryption during init new
    ///
    /// Prompts for each provider's API key and encrypts them using the custody password.
    /// Keys are stored in an encrypted file that can only be decrypted with the password.
    fn configure_api_keys_encrypted(
        &self,
        encrypted_path: &camino::Utf8Path,
        password: &str,
    ) -> Result<()> {
        // Providers to configure (ordered by common usage)
        let providers = [
            ("anthropic", "Anthropic (Claude)", "ANTHROPIC_API_KEY", true),
            ("openai", "OpenAI (GPT)", "OPENAI_API_KEY", true),
            ("ollama", "Ollama (Local)", "", false),  // No API key needed
            ("grok", "xAI (Grok)", "GROK_API_KEY", true),
            ("akashml", "Akash ML", "AKASHML_API_KEY", true),
        ];

        let mut api_keys: HashMap<String, String> = HashMap::new();

        for (key, name, env_var, needs_key) in &providers {
            if !needs_key {
                println!("   ✓ {} - No API key required (local provider)", name);
                continue;
            }

            // Check environment variable first
            if let Ok(env_key) = std::env::var(env_var) {
                if !env_key.is_empty() {
                    println!("   ✓ {} - Found in environment ({})", name, env_var);
                    api_keys.insert(key.to_string(), env_key);
                    continue;
                }
            }

            // Prompt for API key
            print!("   {} API key (or Enter to skip): ", name);
            std::io::stdout().flush()?;

            let api_key = if std::io::stdin().is_terminal() {
                rpassword::read_password()?
            } else {
                let mut input = String::new();
                std::io::stdin().read_line(&mut input)?;
                input.trim().to_string()
            };

            if api_key.is_empty() {
                println!("   ⏭ Skipped {}", name);
            } else {
                println!("   ✓ {} configured", name);
                api_keys.insert(key.to_string(), api_key);
            }
        }

        // Encrypt and save if any keys were configured
        if api_keys.is_empty() {
            println!("\n   No API keys configured. You can add them later with 'ergors init llms'");
            return Ok(());
        }

        // Create encrypted store
        let mut manager = EncryptedApiKeyManager::new();
        manager.unlock(password)?;

        let store = manager.create_store(&api_keys)?;
        let encrypted_bytes = EncryptedApiKeyManager::serialize_store(&store);

        // Write to file
        std::fs::write(encrypted_path, &encrypted_bytes)?;

        // Set restrictive permissions
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(0o600);
            std::fs::set_permissions(encrypted_path, perms)?;
        }

        println!("\n✅ {} API key(s) encrypted and saved to:", api_keys.len());
        println!("   {}", encrypted_path);
        println!("   Keys are protected with your custody password.");

        Ok(())
    }

    /// Generate SSH keys for git workspace operations (legacy, generates ephemeral key)
    ///
    /// Creates ED25519 SSH keys for use with git operations (clone, push, pull).
    /// Note: This generates a new key each time. For persistent keys, use custody-based approach.
    #[deprecated(note = "Use generate_ssh_keys_from_custody instead")]
    #[allow(dead_code)]
    fn generate_ssh_keys(&self, ssh_dir: &camino::Utf8Path, _config: &ErgorsConfig) -> Result<()> {
        // Generate a new ephemeral key
        let private_key = NodePrivKey::new(&mut rand::rngs::OsRng);
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
