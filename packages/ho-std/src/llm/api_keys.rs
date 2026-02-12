use crate::traits::LlmModelTrait;
// use crate::types::ergors::custody::v1::*;
// use crate::types::ergors::keys::v1::*;
// use crate::types::ergors::network::v1::*;
// use crate::types::ergors::types::v1::*;
use crate::types::ergors::orch::v1::*;
use anyhow::{Context, Result};
use camino::Utf8PathBuf;

use std::collections::HashMap;
use std::io::{self, Write};
use termion::event::{Event, Key, MouseButton, MouseEvent};
use termion::input::{MouseTerminal, TermRead};
use termion::raw::IntoRawMode;
use termion::{clear, color, cursor, style};

/// All LlmModel variants for iteration (excluding Custom)
const ALL_LLM_MODELS: &[LlmModel] = &[
    LlmModel::OpenAi,
    LlmModel::Anthropic,
    LlmModel::OllamaLocal,
    LlmModel::AkashMl,
    LlmModel::Grok,
    LlmModel::KimiResearch,
];

impl ApiKeysJson {
    /// Create a new default configuration with ollama_local enabled
    pub fn new(api: &str) -> Self {
        let mut providers = HashMap::new();

        // Add ollama_local by default (no API key needed)
        let ollama = LlmModel::OllamaLocal;
        providers.insert(
            "ollama_local".to_string(),
            ProviderWithAuth {
                api_key: api.into(),
                entity: Some(ollama.default_entity()),
            },
        );

        Self {
            metadata: Some(ApiKeysMetadata {
                version: "2.0.0".to_string(),
                description: "ERGORS Node API Keys - Configure your LLM providers".to_string(),
                golden_ratio_note:
                    "Provider selection uses φ ≈ 1.618 weighting when strategy = 'GoldenRatio'"
                        .to_string(),
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
                    "1. Use 'ergors init llm-api-keys' to configure providers interactively"
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

    /// Generate a complete template with all providers from LlmModel variants.
    /// This ensures the template always matches the expected types.
    pub fn generate_template() -> Self {
        let mut providers = HashMap::new();

        for model in ALL_LLM_MODELS {
            let key = get_provider_key(*model);
        

            providers.insert(
                key.to_string(),
                ProviderWithAuth {
                    api_key: "".to_string(),
                    entity: Some(model.default_entity()),
                },
            );
        }

        Self {
            metadata: Some(ApiKeysMetadata {
                version: "2.0.0".to_string(),
                description: "ERGORS Node API Keys - Configure your LLM providers".to_string(),
                golden_ratio_note:
                    "Provider selection uses φ ≈ 1.618 weighting when strategy = 'GoldenRatio'"
                        .to_string(),
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
                    "1. Use 'ergors init llm-api-keys' to configure providers interactively"
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

/// Get provider key name (lowercase identifier)
fn get_provider_key(provider: LlmModel) -> &'static str {
    match provider {
        LlmModel::AkashMl => "akashml",
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
    // Load existing config or create from template
    // If file exists but can't be parsed (e.g., has null values), regenerate from template
    let mut config = if api_keys_path.exists() {
        match ApiKeysJson::load(api_keys_path) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("⚠️  Existing config at {} is invalid: {}", api_keys_path, e);
                eprintln!("   Regenerating from template...");
                ApiKeysJson::generate_template()
            }
        }
    } else {
        ApiKeysJson::generate_template()
    };

    // Setup termion
    let stdin = io::stdin();
    let mut stdout = MouseTerminal::from(io::stdout().into_raw_mode()?);

    // Clear screen and hide cursor
    write!(
        stdout,
        "{}{}{}",
        clear::All,
        cursor::Goto(1, 1),
        cursor::Hide
    )?;
    stdout.flush()?;

    // All available providers
    let mut all_providers = vec![
        ProviderMenuItem::new(LlmModel::AkashMl, "Decentralized AI Network"),
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
            provider.selected = cfg.entity.clone().expect("dange").enabled;
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
                            cursor_pos = cursor_pos.saturating_sub(1);
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
                            default_provider_index = default_provider_index.saturating_sub(1);
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
    write!(
        stdout,
        "{}{}{}",
        clear::All,
        cursor::Goto(1, 1),
        cursor::Show
    )?;
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

    let _needs_api_key = !matches!(provider.model, LlmModel::OllamaLocal);

    write!(stdout, "\r\n")?;

    let (default_model, models) = provider.model.models();
    write!(
        stdout,
        "{}Default Model:{} {}\r\n",
        style::Bold,
        style::Reset,
        default_model
    )?;
    write!(
        stdout,
        "\r\n{}Available models:{}\r\n",
        style::Bold,
        style::Reset
    )?;
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
    write!(stdout, "{}Configuration:{}\r\n", style::Bold, style::Reset)?;
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
                cfg.entity.clone().unwrap().enabled = false;
            }
            continue;
        }

        // Create configuration for selected providers
        let _key = get_provider_key(provider.model);
 
 
    }

    Ok(())
}
