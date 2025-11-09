use anyhow::{Context, Result};
use camino::Utf8PathBuf;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{self, Write};
use std::path::Path;
use termion::event::{Event, Key, MouseButton, MouseEvent};
use termion::input::{MouseTerminal, TermRead};
use termion::raw::IntoRawMode;
use termion::{clear, color, cursor, style};

use crate::prelude::{LlmEntity, LlmModel};
use crate::traits::LlmModelTrait;

/// JSON structure for the api-keys.json file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKeysJson {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub _metadata: Option<ApiKeysMetadata>,
    pub providers: HashMap<String, ProviderConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub global_settings: Option<GlobalSettings>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instructions: Option<Instructions>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKeysMetadata {
    pub version: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub golden_ratio_note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub api_key: Option<String>,
    pub enabled: bool,
    pub endpoint: String,
    pub models: Vec<String>,
    pub default_model: String,
    pub temperature: f32,
    pub max_tokens: u32,
    pub timeout_seconds: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalSettings {
    pub default_timeout_seconds: u32,
    pub max_retry_attempts: u32,
    pub golden_ratio_weighting: bool,
    pub fallback_enabled: bool,
    pub health_check_interval_seconds: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Instructions {
    pub setup: Vec<String>,
    pub security: Vec<String>,
}

impl ApiKeysJson {
    /// Create a new default configuration with ollama_local enabled
    pub fn new() -> Self {
        let mut providers = HashMap::new();

        // Add ollama_local by default (no API key needed)
        let ollama = LlmModel::OllamaLocal;
        let (default_model, models) = ollama.models();
        providers.insert(
            "ollama_local".to_string(),
            ProviderConfig {
                api_key: None,
                enabled: true,
                endpoint: ollama.default_base_url(),
                models,
                default_model,
                temperature: 0.7,
                max_tokens: 4096,
                timeout_seconds: 60,
            },
        );

        Self {
            _metadata: Some(ApiKeysMetadata {
                version: "2.0.0".to_string(),
                description: "CW-HO Node API Keys - Configure your LLM providers".to_string(),
                golden_ratio_note: Some(
                    "Provider selection uses φ ≈ 1.618 weighting when strategy = 'GoldenRatio'"
                        .to_string(),
                ),
            }),
            providers,
            global_settings: Some(GlobalSettings {
                default_timeout_seconds: 60,
                max_retry_attempts: 3,
                golden_ratio_weighting: true,
                fallback_enabled: true,
                health_check_interval_seconds: 300,
            }),
            instructions: Some(Instructions {
                setup: vec![
                    "1. Use 'cw-ho init llm-api-keys' to configure providers interactively"
                        .to_string(),
                    "2. Set 'enabled': true for providers you want to use".to_string(),
                    "3. Adjust model selections and parameters as needed".to_string(),
                    "4. Environment variables are supported: ${MY_API_KEY}".to_string(),
                    "5. Local providers (like Ollama) don't need API keys".to_string(),
                ],
                security: vec![
                    "⚠️  Never commit API keys to version control".to_string(),
                    "✅ Add api-keys.json to your .gitignore".to_string(),
                    "✅ Use environment variables for production keys".to_string(),
                    "✅ Restrict file permissions: chmod 600 api-keys.json".to_string(),
                ],
            }),
        }
    }

    /// Load configuration from file
    pub fn load(path: &Utf8PathBuf) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read API keys file: {}", path.as_str()))?;

        let config: ApiKeysJson = serde_json::from_str(&content)
            .with_context(|| format!("Failed to parse API keys JSON from: {}", path.as_str()))?;

        Ok(config)
    }

    /// Save configuration to file
    pub fn save(&self, path: &Utf8PathBuf) -> Result<()> {
        let json =
            serde_json::to_string_pretty(self).context("Failed to serialize API keys config")?;

        std::fs::write(path, json)
            .with_context(|| format!("Failed to write API keys file: {}", path.as_str()))?;

        // Set restrictive permissions (owner read/write only)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(0o600);
            std::fs::set_permissions(path, perms)
                .with_context(|| format!("Failed to set permissions on: {}", path.as_str()))?;
        }

        Ok(())
    }
}

/// Get environment variable name for a provider
fn get_env_var_name(provider: LlmModel) -> &'static str {
    use crate::constants::*;

    match provider {
        LlmModel::OpenAi => OPENAI_API_KEY,
        LlmModel::Anthropic => ANTHROPIC_API_KEY,
        LlmModel::Grok => GROK_API_KEY,
        LlmModel::AkashChat => AKASH_API_KEY,
        LlmModel::KimiResearch => KIMI_API_KEY,
        LlmModel::OllamaLocal => "OLLAMA_HOST",
        LlmModel::Custom => "CUSTOM_API_KEY",
    }
}

/// Get provider key name (lowercase identifier)
fn get_provider_key(provider: LlmModel) -> &'static str {
    match provider {
        LlmModel::AkashChat => "akash_chat",
        LlmModel::OllamaLocal => "ollama_local",
        LlmModel::KimiResearch => "kimi",
        LlmModel::Grok => "grok",
        LlmModel::OpenAi => "openai",
        LlmModel::Anthropic => "anthropic",
        LlmModel::Custom => "custom",
    }
}

/// Provider menu item
#[derive(Clone)]
struct ProviderMenuItem {
    model: LlmModel,
    name: String,
    description: String,
    selected: bool,
}

impl ProviderMenuItem {
    fn new(model: LlmModel, description: &str) -> Self {
        Self {
            model,
            name: model.as_str_name().to_string(),
            description: description.to_string(),
            selected: false,
        }
    }
}

enum ConfigStep {
    SelectProviders,
    ConfigureProviders(usize), // Index in selected providers list
    SelectDefaultProvider,
    Done,
}

/// Interactive CLI for configuring API keys using termion TUI - 3 step process
pub fn configure_api_keys_interactive(api_keys_path: &Utf8PathBuf) -> Result<()> {
    // Load existing config or create new one
    let mut config = if api_keys_path.exists() {
        ApiKeysJson::load(api_keys_path)?
    } else {
        ApiKeysJson::new()
    };

    // Setup termion
    let stdin = io::stdin();
    let mut stdout = MouseTerminal::from(io::stdout().into_raw_mode()?);

    // Clear screen and hide cursor
    write!(stdout, "{}{}{}", clear::All, cursor::Goto(1, 1), cursor::Hide)?;
    stdout.flush()?;

    // All available providers
    let mut all_providers = vec![
        ProviderMenuItem::new(LlmModel::AkashChat, "Decentralized AI Network"),
        ProviderMenuItem::new(LlmModel::OllamaLocal, "Local Ollama (No API key needed)"),
        ProviderMenuItem::new(LlmModel::KimiResearch, "Kimi Research AI"),
        ProviderMenuItem::new(LlmModel::Grok, "X.AI Grok"),
        ProviderMenuItem::new(LlmModel::OpenAi, "OpenAI GPT Models"),
        ProviderMenuItem::new(LlmModel::Anthropic, "Anthropic Claude Models"),
        ProviderMenuItem::new(LlmModel::Custom, "Custom Provider"),
    ];

    // Mark already enabled providers as selected
    for provider in &mut all_providers {
        let key = get_provider_key(provider.model);
        if let Some(cfg) = config.providers.get(key) {
            provider.selected = cfg.enabled;
        }
    }

    let mut cursor_pos: usize = 0;
    let mut step = ConfigStep::SelectProviders;
    let mut default_provider_index: usize = 0;

    // Create events iterator once
    let mut events = stdin.events();

    let mut running = true;
    while running {
        match &step {
            ConfigStep::SelectProviders => {
                draw_select_providers(&mut stdout, &all_providers, cursor_pos)?;
            }
            ConfigStep::ConfigureProviders(idx) => {
                let selected_providers: Vec<_> = all_providers
                    .iter()
                    .filter(|p| p.selected)
                    .cloned()
                    .collect();
                if *idx < selected_providers.len() {
                    draw_configure_provider(
                        &mut stdout,
                        &selected_providers[*idx],
                        *idx,
                        selected_providers.len(),
                    )?;
                }
            }
            ConfigStep::SelectDefaultProvider => {
                let selected_providers: Vec<_> = all_providers
                    .iter()
                    .filter(|p| p.selected)
                    .cloned()
                    .collect();
                draw_select_default(&mut stdout, &selected_providers, default_provider_index)?;
            }
            ConfigStep::Done => {
                running = false;
                continue;
            }
        }

        // Handle events
        if let Some(event) = events.next() {
            let evt = event?;
            match &step {
                ConfigStep::SelectProviders => {
                    match evt {
                        Event::Key(Key::Char('q')) | Event::Key(Key::Esc) => {
                            running = false;
                        }
                        Event::Key(Key::Up) => {
                            if cursor_pos > 0 {
                                cursor_pos -= 1;
                            }
                        }
                        Event::Key(Key::Down) => {
                            if cursor_pos < all_providers.len() - 1 {
                                cursor_pos += 1;
                            }
                        }
                        Event::Key(Key::Char(' ')) => {
                            // Toggle selection
                            all_providers[cursor_pos].selected =
                                !all_providers[cursor_pos].selected;
                        }
                        Event::Key(Key::Char('\n')) => {
                            // Move to configuration step
                            let selected_count =
                                all_providers.iter().filter(|p| p.selected).count();
                            if selected_count > 0 {
                                cursor_pos = 0;
                                step = ConfigStep::ConfigureProviders(0);
                            }
                        }
                        Event::Mouse(me) => {
                            if let MouseEvent::Press(MouseButton::Left, _, y) = me {
                                let menu_start = 7;
                                if y >= menu_start && (y - menu_start) < all_providers.len() as u16
                                {
                                    cursor_pos = (y - menu_start) as usize;
                                }
                            }
                        }
                        _ => {}
                    }
                }
                ConfigStep::ConfigureProviders(provider_idx) => {
                    match evt {
                        Event::Key(Key::Char('q')) | Event::Key(Key::Esc) => {
                            // Go back to provider selection
                            step = ConfigStep::SelectProviders;
                            cursor_pos = 0;
                        }
                        Event::Key(Key::Char('\n')) => {
                            // Move to next provider or default selection
                            let selected_providers: Vec<_> =
                                all_providers.iter().filter(|p| p.selected).collect();
                            if *provider_idx + 1 < selected_providers.len() {
                                step = ConfigStep::ConfigureProviders(*provider_idx + 1);
                            } else {
                                // Done configuring, move to default selection
                                step = ConfigStep::SelectDefaultProvider;
                                default_provider_index = 0;
                            }
                        }
                        _ => {}
                    }
                }
                ConfigStep::SelectDefaultProvider => {
                    match evt {
                        Event::Key(Key::Char('q')) | Event::Key(Key::Esc) => {
                            // Go back
                            step = ConfigStep::SelectProviders;
                            cursor_pos = 0;
                        }
                        Event::Key(Key::Up) => {
                            if default_provider_index > 0 {
                                default_provider_index -= 1;
                            }
                        }
                        Event::Key(Key::Down) => {
                            let selected_count =
                                all_providers.iter().filter(|p| p.selected).count();
                            if default_provider_index < selected_count.saturating_sub(1) {
                                default_provider_index += 1;
                            }
                        }
                        Event::Key(Key::Char('\n')) | Event::Key(Key::Char('s')) => {
                            // Save configuration
                            save_configuration(&mut config, &all_providers)?;
                            config.save(api_keys_path)?;
                            step = ConfigStep::Done;
                        }
                        _ => {}
                    }
                }
                ConfigStep::Done => {}
            }
        }
    }

    // Cleanup
    write!(stdout, "{}{}{}", clear::All, cursor::Goto(1, 1), cursor::Show)?;
    stdout.flush()?;

    Ok(())
}

/// Draw Step 1: Select Providers
fn draw_select_providers<W: Write>(
    stdout: &mut W,
    providers: &[ProviderMenuItem],
    cursor: usize,
) -> Result<()> {
    write!(stdout, "{}{}", clear::All, cursor::Goto(1, 1))?;

    // Title
    write!(
        stdout,
        "{}{}╔══════════════════════════════════════════════════════════════════════╗\r\n",
        color::Fg(color::Cyan),
        style::Bold
    )?;
    write!(
        stdout,
        "║  {}🔧 Step 1/3: Select LLM Providers{}                                 ║\r\n",
        color::Fg(color::Yellow),
        color::Fg(color::Cyan)
    )?;
    write!(
        stdout,
        "╚══════════════════════════════════════════════════════════════════════╝{}\r\n",
        style::Reset
    )?;
    write!(stdout, "\r\n")?;

    // Instructions
    write!(
        stdout,
        "{}Use ↑/↓ to navigate, Space to toggle, Enter to continue, 'q' to quit{}\r\n",
        color::Fg(color::LightBlack),
        style::Reset
    )?;
    write!(stdout, "\r\n")?;

    // Provider list
    for (i, provider) in providers.iter().enumerate() {
        let is_cursor = i == cursor;
        let prefix = if is_cursor {
            format!("{}▶ ", color::Fg(color::Green))
        } else {
            "  ".to_string()
        };

        let checkbox = if provider.selected {
            format!("{}[✓]{}", color::Fg(color::Green), color::Fg(color::Reset))
        } else {
            format!("{}[ ]{}", color::Fg(color::Red), color::Fg(color::Reset))
        };

        let style_start = if is_cursor {
            format!("{}{}", style::Bold, color::Fg(color::White))
        } else {
            "".to_string()
        };

        let style_end = if is_cursor {
            format!("{}", style::Reset)
        } else {
            "".to_string()
        };

        write!(
            stdout,
            "{}{}{} {} - {}{}\r\n",
            prefix, style_start, checkbox, provider.name, provider.description, style_end
        )?;
    }

    // Footer
    let selected_count = providers.iter().filter(|p| p.selected).count();
    write!(stdout, "\r\n")?;
    write!(
        stdout,
        "{}{}{} provider(s) selected{}",
        color::Fg(color::LightBlack),
        style::Italic,
        selected_count,
        style::Reset
    )?;

    stdout.flush()?;
    Ok(())
}

/// Draw Step 2: Configure Provider
fn draw_configure_provider<W: Write>(
    stdout: &mut W,
    provider: &ProviderMenuItem,
    current: usize,
    total: usize,
) -> Result<()> {
    write!(stdout, "{}{}", clear::All, cursor::Goto(1, 1))?;

    // Title
    write!(
        stdout,
        "{}{}╔══════════════════════════════════════════════════════════════════════╗\r\n",
        color::Fg(color::Cyan),
        style::Bold
    )?;
    write!(
        stdout,
        "║  {}🔧 Step 2/3: Configure {} ({}/{}){}                     ║\r\n",
        color::Fg(color::Yellow),
        provider.name,
        current + 1,
        total,
        color::Fg(color::Cyan)
    )?;
    write!(
        stdout,
        "╚══════════════════════════════════════════════════════════════════════╝{}\r\n",
        style::Reset
    )?;
    write!(stdout, "\r\n")?;

    let needs_api_key = !matches!(provider.model, LlmModel::OllamaLocal);

    if needs_api_key {
        write!(
            stdout,
            "{}API Key: {}Use environment variable ${{{}}}{}\r\n",
            style::Bold,
            color::Fg(color::Green),
            get_env_var_name(provider.model),
            style::Reset
        )?;
        write!(
            stdout,
            "{}(Recommended for security){}\r\n",
            color::Fg(color::LightBlack),
            style::Reset
        )?;
    } else {
        write!(
            stdout,
            "{}No API key needed - Local provider{}\r\n",
            color::Fg(color::Green),
            style::Reset
        )?;
    }

    write!(stdout, "\r\n")?;

    let (default_model, models) = provider.model.models();
    write!(
        stdout,
        "{}Default Model:{} {}\r\n",
        style::Bold,
        style::Reset,
        default_model
    )?;
    write!(stdout, "\r\n{}Available models:{}\r\n", style::Bold, style::Reset)?;
    for model in models.iter().take(5) {
        write!(stdout, "  • {}\r\n", model)?;
    }
    if models.len() > 5 {
        write!(
            stdout,
            "  {}... and {} more{}\r\n",
            color::Fg(color::LightBlack),
            models.len() - 5,
            style::Reset
        )?;
    }

    write!(stdout, "\r\n")?;
    write!(
        stdout,
        "{}Configuration:{}\r\n",
        style::Bold,
        style::Reset
    )?;
    write!(stdout, "  • Temperature: 0.7\r\n")?;
    write!(stdout, "  • Max Tokens: 4096\r\n")?;
    write!(stdout, "  • Timeout: 60s\r\n")?;

    write!(stdout, "\r\n")?;
    write!(
        stdout,
        "{}{}Press Enter to continue | ESC to go back{}",
        color::Fg(color::LightBlack),
        style::Italic,
        style::Reset
    )?;

    stdout.flush()?;
    Ok(())
}

/// Draw Step 3: Select Default Provider
fn draw_select_default<W: Write>(
    stdout: &mut W,
    providers: &[ProviderMenuItem],
    cursor: usize,
) -> Result<()> {
    write!(stdout, "{}{}", clear::All, cursor::Goto(1, 1))?;

    // Title
    write!(
        stdout,
        "{}{}╔══════════════════════════════════════════════════════════════════════╗\r\n",
        color::Fg(color::Cyan),
        style::Bold
    )?;
    write!(
        stdout,
        "║  {}🔧 Step 3/3: Select Default Provider{}                             ║\r\n",
        color::Fg(color::Yellow),
        color::Fg(color::Cyan)
    )?;
    write!(
        stdout,
        "╚══════════════════════════════════════════════════════════════════════╝{}\r\n",
        style::Reset
    )?;
    write!(stdout, "\r\n")?;

    // Instructions
    write!(
        stdout,
        "{}Use ↑/↓ to navigate, Enter to save and finish{}\r\n",
        color::Fg(color::LightBlack),
        style::Reset
    )?;
    write!(stdout, "\r\n")?;

    // Provider list
    for (i, provider) in providers.iter().enumerate() {
        let is_cursor = i == cursor;
        let prefix = if is_cursor {
            format!("{}▶ ", color::Fg(color::Green))
        } else {
            "  ".to_string()
        };

        let style_start = if is_cursor {
            format!("{}{}", style::Bold, color::Fg(color::White))
        } else {
            "".to_string()
        };

        let style_end = if is_cursor {
            format!("{}", style::Reset)
        } else {
            "".to_string()
        };

        write!(
            stdout,
            "{}{}{}{}\r\n",
            prefix, style_start, provider.name, style_end
        )?;
    }

    write!(stdout, "\r\n")?;
    write!(
        stdout,
        "{}{}Press Enter to save configuration | ESC to go back{}",
        color::Fg(color::LightBlack),
        style::Italic,
        style::Reset
    )?;

    stdout.flush()?;
    Ok(())
}

/// Save the configuration
fn save_configuration(config: &mut ApiKeysJson, providers: &[ProviderMenuItem]) -> Result<()> {
    for provider in providers {
        if !provider.selected {
            // Disable non-selected providers
            let key = get_provider_key(provider.model);
            if let Some(cfg) = config.providers.get_mut(key) {
                cfg.enabled = false;
            }
            continue;
        }

        // Create configuration for selected providers
        let key = get_provider_key(provider.model);
        let (default_model, models) = provider.model.models();

        let api_key = if !matches!(provider.model, LlmModel::OllamaLocal) {
            Some(format!("${{{}}}", get_env_var_name(provider.model)))
        } else {
            None
        };

        let provider_config = ProviderConfig {
            api_key,
            enabled: true,
            endpoint: provider.model.default_base_url(),
            models,
            default_model,
            temperature: 0.7,
            max_tokens: 4096,
            timeout_seconds: 60,
        };

        config.providers.insert(key.to_string(), provider_config);
    }

    Ok(())
}
