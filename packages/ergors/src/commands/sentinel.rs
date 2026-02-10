//! CLI command for bootstrapping a remote sentinel node.
//!
//! Orchestrates the full sentinel handshake: fetches the session key,
//! encrypts secrets with X25519 + ChaCha20Poly1305, signs envelopes
//! with the local admin Ed25519 key, and walks through init → api-keys
//! → activate.

use std::collections::HashMap;
use std::io::{IsTerminal as _, Write as _};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, Context, Result};
use chacha20poly1305::{
    aead::{Aead, NewAead},
    ChaCha20Poly1305, Key, Nonce,
};
use commonware_codec::Encode;
use commonware_cryptography::{blake3 as cw_blake3, Hasher, Signer};
use ho_std::{
    custody::PasswordEncryptedCustody,
    keys::commonware::NodePrivKey,
    traits::NodeIdentityCustody,
};
use rand::RngCore;
use serde::Deserialize;
use x25519_dalek::{PublicKey as X25519PublicKey, StaticSecret as X25519Secret};

use crate::client::sentinel::SENTINEL_KDF_CONTEXT;

// =============================================================================
// CLI types
// =============================================================================

#[derive(Debug, clap::Parser)]
pub struct SentinelCmd {
    #[clap(subcommand)]
    pub subcmd: SentinelSubCmd,
}

#[derive(Debug, Clone, clap::Subcommand)]
pub enum SentinelSubCmd {
    /// Bootstrap a remote sentinel node (init + api-keys + activate)
    Bootstrap {
        /// Sentinel HTTP endpoint (e.g. http://host:8080)
        #[arg()]
        url: String,

        /// Raw Ed25519 private key hex (32 bytes). Bypasses local custody loading.
        /// Useful for automation and testing where no node_identity.enc exists.
        #[arg(long)]
        admin_privkey_hex: Option<String>,
    },
}

// =============================================================================
// Response types (from sentinel server)
// =============================================================================

#[derive(Deserialize)]
struct HealthResponse {
    phase: String,
    #[allow(dead_code)]
    version: String,
    session_pubkey: String,
}

#[derive(Deserialize)]
struct StatusResponse {
    ok: bool,
    error: Option<String>,
}

// =============================================================================
// Implementation
// =============================================================================

impl SentinelCmd {
    pub fn exec(&self, home_dir: &camino::Utf8Path) -> Result<()> {
        match &self.subcmd {
            SentinelSubCmd::Bootstrap {
                url,
                admin_privkey_hex,
            } => {
                let url = url.trim_end_matches('/');
                let rt = tokio::runtime::Runtime::new()?;
                rt.block_on(bootstrap(home_dir, url, admin_privkey_hex.as_deref()))
            }
        }
    }
}

/// Full sentinel bootstrap: health → init → api-keys → activate.
async fn bootstrap(
    home_dir: &camino::Utf8Path,
    base_url: &str,
    admin_privkey_hex: Option<&str>,
) -> Result<()> {
    // 1. Load admin signing key
    let admin_key = match admin_privkey_hex {
        Some(hex_key) => NodePrivKey::from_hex(hex_key)
            .ok_or_else(|| anyhow!("invalid --admin-privkey-hex (expected 64 hex chars / 32 bytes)"))?,
        None => load_admin_key(home_dir).await?,
    };
    let admin_pubkey_hex = hex::encode(admin_key.id().0.encode());

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()?;

    // 2. Fetch health to get session pubkey + verify phase
    println!("Connecting to sentinel at {}...", base_url);
    let health: HealthResponse = client
        .get(format!("{}/sentinel/health", base_url))
        .send()
        .await
        .context("failed to reach sentinel")?
        .json()
        .await
        .context("invalid health response")?;

    println!("  Phase:   {}", health.phase);
    println!("  Session: {}...{}", &health.session_pubkey[..8], &health.session_pubkey[56..]);

    let server_pubkey = parse_x25519_pubkey(&health.session_pubkey)?;

    // 3. Phase: init (if sentinel is awaiting it)
    if health.phase == "awaiting_init" {
        let custody_password = prompt_hidden("Enter custody password for remote node: ")?;
        if custody_password.len() < 8 {
            return Err(anyhow!("Password must be at least 8 characters"));
        }

        let mnemonic = prompt_hidden("Enter mnemonic (or press Enter to generate new): ")?;

        let mut init_body = serde_json::json!({
            "custody_password": custody_password,
        });
        if !mnemonic.is_empty() {
            init_body["mnemonic"] = serde_json::Value::String(mnemonic);
        }

        println!("\nSending encrypted init...");
        send_encrypted(
            &client,
            &format!("{}/sentinel/init", base_url),
            &serde_json::to_vec(&init_body)?,
            &server_pubkey,
            &admin_key,
            &admin_pubkey_hex,
        )
        .await
        .context("init failed")?;
        println!("  Init complete.");
    } else {
        println!("  Skipping init (phase: {})", health.phase);
    }

    // Re-check phase
    let health: HealthResponse = client
        .get(format!("{}/sentinel/health", base_url))
        .send()
        .await?
        .json()
        .await?;

    // 4. Phase: api-keys
    if health.phase == "awaiting_api_keys" {
        let api_keys = prompt_api_keys()?;

        if api_keys.is_empty() {
            return Err(anyhow!("At least one API key is required"));
        }

        let body = serde_json::json!({ "api_keys": api_keys });

        println!("\nSending encrypted API keys...");
        send_encrypted(
            &client,
            &format!("{}/sentinel/api-keys", base_url),
            &serde_json::to_vec(&body)?,
            &server_pubkey,
            &admin_key,
            &admin_pubkey_hex,
        )
        .await
        .context("api-keys failed")?;
        println!("  API keys stored.");
    } else {
        println!("  Skipping api-keys (phase: {})", health.phase);
    }

    // Re-check phase
    let health: HealthResponse = client
        .get(format!("{}/sentinel/health", base_url))
        .send()
        .await?
        .json()
        .await?;

    // 5. Phase: activate
    if health.phase == "awaiting_activation" {
        let body = serde_json::json!({});

        println!("\nSending encrypted activate...");
        send_encrypted(
            &client,
            &format!("{}/sentinel/activate", base_url),
            &serde_json::to_vec(&body)?,
            &server_pubkey,
            &admin_key,
            &admin_pubkey_hex,
        )
        .await
        .context("activate failed")?;
        println!("  Activation complete. Sentinel is handing off to full server.");
    } else {
        println!("  Skipping activate (phase: {})", health.phase);
    }

    println!("\nBootstrap finished.");
    Ok(())
}

// =============================================================================
// Helpers
// =============================================================================

/// Load the admin Ed25519 private key from the local encrypted custody.
async fn load_admin_key(home_dir: &camino::Utf8Path) -> Result<NodePrivKey> {
    let identity_path = home_dir.join("node_identity.enc");
    if !identity_path.exists() {
        return Err(anyhow!(
            "No local identity found at {}. Run 'ergors init new' first.",
            identity_path
        ));
    }

    let custody = PasswordEncryptedCustody::new(&identity_path);

    // Try env var first, then interactive prompt
    let password = if let Ok(pw) = std::env::var("ERGORS_CUSTODY_PASSWORD") {
        if !pw.is_empty() {
            pw
        } else {
            prompt_hidden("Enter local custody password (for signing): ")?
        }
    } else {
        prompt_hidden("Enter local custody password (for signing): ")?
    };

    custody
        .unlock(&password)
        .await
        .map_err(|_| anyhow!("Invalid local custody password"))?;

    custody
        .get_private_key()
        .await
        .map_err(|e| anyhow!("Failed to load signing key: {}", e))
}

/// Hidden interactive prompt (uses rpassword when terminal, stdin otherwise).
fn prompt_hidden(msg: &str) -> Result<String> {
    if std::io::stdin().is_terminal() {
        Ok(rpassword::prompt_password(msg)?)
    } else {
        let mut buf = String::new();
        std::io::stdin().read_line(&mut buf)?;
        Ok(buf.trim().to_string())
    }
}

/// Prompt for API keys interactively (hidden input).
fn prompt_api_keys() -> Result<HashMap<String, String>> {
    let providers = [
        ("anthropic", "Anthropic (Claude)"),
        ("openai", "OpenAI (GPT)"),
        ("akashml", "Akash ML"),
        ("grok", "xAI (Grok)"),
    ];

    println!("\nAPI Key Configuration (press Enter to skip):");
    let mut keys = HashMap::new();

    for (key, label) in &providers {
        print!("  {} API key: ", label);
        std::io::stdout().flush()?;

        let value = if std::io::stdin().is_terminal() {
            rpassword::read_password()?
        } else {
            let mut buf = String::new();
            std::io::stdin().read_line(&mut buf)?;
            buf.trim().to_string()
        };

        if value.is_empty() {
            println!("  skipped");
        } else {
            println!("  configured");
            keys.insert(key.to_string(), value);
        }
    }

    // Allow custom key names
    loop {
        print!("  Custom provider name (or Enter to finish): ");
        std::io::stdout().flush()?;
        let mut name = String::new();
        std::io::stdin().read_line(&mut name)?;
        let name = name.trim().to_string();
        if name.is_empty() {
            break;
        }

        print!("  {} API key: ", name);
        std::io::stdout().flush()?;

        let value = if std::io::stdin().is_terminal() {
            rpassword::read_password()?
        } else {
            let mut buf = String::new();
            std::io::stdin().read_line(&mut buf)?;
            buf.trim().to_string()
        };

        if !value.is_empty() {
            println!("  configured");
            keys.insert(name, value);
        }
    }

    Ok(keys)
}

/// Parse a hex-encoded 32-byte X25519 public key.
fn parse_x25519_pubkey(hex_str: &str) -> Result<X25519PublicKey> {
    let bytes: [u8; 32] = hex::decode(hex_str)
        .map_err(|_| anyhow!("invalid session_pubkey hex"))?
        .try_into()
        .map_err(|_| anyhow!("session_pubkey must be 32 bytes"))?;
    Ok(X25519PublicKey::from(bytes))
}

/// Encrypt plaintext into an envelope, sign it, and POST to the sentinel.
async fn send_encrypted(
    client: &reqwest::Client,
    url: &str,
    plaintext: &[u8],
    server_pubkey: &X25519PublicKey,
    admin_key: &NodePrivKey,
    admin_pubkey_hex: &str,
) -> Result<()> {
    // Build encrypted envelope
    let envelope_json = build_envelope(plaintext, server_pubkey)?;

    // Sign envelope body
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)?
        .as_secs()
        .to_string();

    let mut contents = Vec::new();
    contents.extend_from_slice(&envelope_json);
    contents.extend_from_slice(timestamp.as_bytes());
    let message = cw_blake3::Blake3::hash(&contents);

    let signature = admin_key.sign(None, &message);
    let sig_hex = hex::encode(signature.encode());

    // Send
    let resp = client
        .post(url)
        .header("Content-Type", "application/json")
        .header("x-signature", &sig_hex)
        .header("x-timestamp", &timestamp)
        .header("x-public-key", admin_pubkey_hex)
        .body(envelope_json)
        .send()
        .await?;

    let status = resp.status();
    let body: StatusResponse = resp
        .json()
        .await
        .context("failed to parse sentinel response")?;

    if !body.ok {
        return Err(anyhow!(
            "sentinel returned {} — {}",
            status,
            body.error.unwrap_or_default()
        ));
    }

    Ok(())
}

/// Build an encrypted JSON envelope for the sentinel.
fn build_envelope(plaintext: &[u8], server_pubkey: &X25519PublicKey) -> Result<Vec<u8>> {
    let client_secret = X25519Secret::random_from_rng(rand::thread_rng());
    let client_pubkey = X25519PublicKey::from(&client_secret);

    // X25519 DH → blake3 KDF → ChaCha20Poly1305
    let shared = client_secret.diffie_hellman(server_pubkey);
    let derived = ::blake3::derive_key(SENTINEL_KDF_CONTEXT, shared.as_bytes());
    let cipher = ChaCha20Poly1305::new(Key::from_slice(&derived));

    let mut nonce_bytes = [0u8; 12];
    rand::thread_rng().fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ct = cipher
        .encrypt(nonce, plaintext)
        .map_err(|_| anyhow!("encryption failed"))?;

    let envelope = serde_json::json!({
        "ephemeral_pubkey": hex::encode(client_pubkey.as_bytes()),
        "nonce": hex::encode(nonce_bytes),
        "ciphertext": hex::encode(ct),
    });

    Ok(serde_json::to_vec(&envelope)?)
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::sentinel::SENTINEL_KDF_CONTEXT;

    #[test]
    fn build_envelope_decryptable_by_server() {
        let server_secret = X25519Secret::random_from_rng(rand::thread_rng());
        let server_pubkey = X25519PublicKey::from(&server_secret);

        let plaintext = br#"{"custody_password":"test12345678"}"#;
        let envelope_json = build_envelope(plaintext, &server_pubkey).unwrap();

        // Parse envelope
        let envelope: serde_json::Value = serde_json::from_slice(&envelope_json).unwrap();

        let epk_bytes: [u8; 32] = hex::decode(envelope["ephemeral_pubkey"].as_str().unwrap())
            .unwrap()
            .try_into()
            .unwrap();
        let client_pubkey = X25519PublicKey::from(epk_bytes);

        let nonce_bytes: [u8; 12] = hex::decode(envelope["nonce"].as_str().unwrap())
            .unwrap()
            .try_into()
            .unwrap();

        let ct = hex::decode(envelope["ciphertext"].as_str().unwrap()).unwrap();

        // Server-side decryption
        let shared = server_secret.diffie_hellman(&client_pubkey);
        let derived = ::blake3::derive_key(SENTINEL_KDF_CONTEXT, shared.as_bytes());
        let cipher = ChaCha20Poly1305::new(Key::from_slice(&derived));
        let nonce = Nonce::from_slice(&nonce_bytes);

        let decrypted = cipher.decrypt(nonce, ct.as_ref()).expect("decrypt");
        assert_eq!(decrypted, plaintext);
    }
}
