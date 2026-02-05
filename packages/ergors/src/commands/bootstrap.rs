//! Bootstrap management CLI commands
//!
//! Commands for bootstrapping new ergors nodes via Akash or SSH.
//! Uses HTTP API endpoints (not gRPC) since bootstrap operations
//! are served via the HTTP server.

use anyhow::Result;
use clap::Subcommand;

use super::CliContext;
use ho_std::types::ergors::network::v1::NodeIdentity;
use ho_std::types::ergors::orch::v1::{
    bootstrap_method, cloud, Akash, BootstrapMethod, BootstrapRequest, Cloud,
};

/// Default HTTP API port (matches server default)
const DEFAULT_API_PORT: u16 = 8080;

/// Bootstrap management commands
#[derive(Subcommand)]
pub enum BootstrapCmd {
    /// Bootstrap a new node via Akash deployment
    Node {
        /// Node type to bootstrap (coordinator, executor)
        #[arg(long, default_value = "executor")]
        node_type: String,

        /// Docker image tag to use (default: latest from registry)
        #[arg(long)]
        image: Option<String>,

        /// Bootstrap method: akash or ssh
        #[arg(long, default_value = "akash")]
        method: String,

        /// Bootstrap peer addresses (comma-separated)
        /// If not specified, uses the coordinator's own address
        #[arg(long)]
        peers: Option<String>,

        /// Custom environment variables (key=value pairs)
        #[arg(long, value_parser = parse_key_val)]
        env: Vec<(String, String)>,

        /// SSH connection string (for SSH method: user@host:port)
        #[arg(long)]
        ssh: Option<String>,
    },

    /// List all bootstrap sessions
    List {
        /// Show only active (in-progress) sessions
        #[arg(long)]
        active: bool,
    },

    /// Get status of a bootstrap session
    Status {
        /// Session ID to query
        session_id: String,
    },

    /// Delete a bootstrap session
    Delete {
        /// Session ID to delete
        session_id: String,

        /// Skip confirmation prompt
        #[arg(long)]
        force: bool,
    },
}

/// Parse key=value pairs for environment variables
fn parse_key_val(s: &str) -> Result<(String, String)> {
    let pos = s
        .find('=')
        .ok_or_else(|| anyhow::anyhow!("invalid KEY=VALUE: no `=` found in `{}`", s))?;
    Ok((s[..pos].to_string(), s[pos + 1..].to_string()))
}

/// Derive the HTTP API base URL from the gRPC address.
/// Uses ERGORS_API_ADDR env var if set, otherwise extracts host from gRPC addr
/// and uses default API port (8080).
fn api_base_url(grpc_addr: &str) -> String {
    if let Ok(addr) = std::env::var("ERGORS_API_ADDR") {
        return addr;
    }

    // Extract host from gRPC address (e.g., "http://localhost:50051" -> "localhost")
    let host = grpc_addr
        .trim_start_matches("http://")
        .trim_start_matches("https://")
        .split(':')
        .next()
        .unwrap_or("localhost");

    format!("http://{}:{}", host, DEFAULT_API_PORT)
}

impl BootstrapCmd {
    /// Execute the bootstrap command
    pub async fn execute(&self, ctx: &CliContext) -> Result<()> {
        let base_url = api_base_url(&ctx.grpc_addr);
        let http = reqwest::Client::new();

        match self {
            BootstrapCmd::Node {
                node_type,
                image,
                method,
                peers,
                env: _,
                ssh,
            } => {
                cmd_bootstrap_node(&http, &base_url, node_type, image.as_deref(), method, peers.as_deref(), ssh.as_deref(), ctx.json).await
            }
            BootstrapCmd::List { active } => {
                cmd_list_bootstrap_sessions(&http, &base_url, *active, ctx.json).await
            }
            BootstrapCmd::Status { session_id } => {
                cmd_bootstrap_status(&http, &base_url, session_id, ctx.json).await
            }
            BootstrapCmd::Delete { session_id, force } => {
                cmd_delete_bootstrap_session(&http, &base_url, session_id, *force, ctx.json).await
            }
        }
    }
}

/// Bootstrap a new node
async fn cmd_bootstrap_node(
    http: &reqwest::Client,
    base_url: &str,
    node_type: &str,
    _image: Option<&str>, // Image tag set server-side via BOOTSTRAP_IMAGE_TAG env var
    method: &str,
    peers: Option<&str>,
    _ssh: Option<&str>, // SSH method not yet implemented
    json: bool,
) -> Result<()> {
    let node_type_upper = match node_type.to_lowercase().as_str() {
        "coordinator" => "COORDINATOR",
        "executor" => "EXECUTOR",
        _ => anyhow::bail!("Invalid node type: {}. Use 'coordinator' or 'executor'", node_type),
    };

    // Build the bootstrap method using actual proto types to guarantee correct JSON format
    let method_proto = match method.to_lowercase().as_str() {
        "akash" => BootstrapMethod {
            method: Some(bootstrap_method::Method::Cloud(Cloud {
                provider: Some(cloud::Provider::Akash(Akash::default())),
            })),
        },
        "ssh" => {
            anyhow::bail!("SSH bootstrap not yet supported. Use --method akash");
        }
        _ => anyhow::bail!("Unknown bootstrap method: {}. Use 'akash'", method),
    };

    // Parse peers into identity host/port if provided
    // Format: "host:port" - used by server to override default bootstrap peer
    let mut identity = NodeIdentity {
        node_type: node_type_upper.to_string(),
        ..Default::default()
    };
    if let Some(peers_str) = peers {
        if let Some(first_peer) = peers_str.split(',').next() {
            let parts: Vec<&str> = first_peer.trim().rsplitn(2, ':').collect();
            if parts.len() == 2 {
                identity.host = parts[1].to_string();
                identity.p2p_port = parts[0].parse().unwrap_or(26969);
            } else {
                identity.host = first_peer.trim().to_string();
            }
        }
    }

    let request = BootstrapRequest {
        bootstrap_method: Some(method_proto),
        identity: Some(identity),
        ..Default::default()
    };

    let body = serde_json::to_value(&request)?;

    let url = format!("{}/orchestrate/bootstrap", base_url);
    let resp = http.post(&url).json(&body).send().await?;
    let status = resp.status();
    let result: serde_json::Value = resp.json().await?;

    if !status.is_success() {
        anyhow::bail!("Bootstrap request failed ({}): {}", status, result);
    }

    if json {
        println!("{}", serde_json::to_string_pretty(&result)?);
    } else {
        if let Some(id) = result.get("id").and_then(|v| v.as_str()) {
            println!("Bootstrap session started: {}", id);
            println!("  Status: {}", result.get("status").and_then(|v| v.as_str()).unwrap_or("unknown"));
            println!("\nUse 'ergors bootstrap status {}' to check progress", id);
        } else if let Some(err) = result.get("error").and_then(|v| v.as_str()) {
            println!("Error: {}", err);
        } else {
            println!("{}", serde_json::to_string_pretty(&result)?);
        }
    }

    Ok(())
}

/// List bootstrap sessions
async fn cmd_list_bootstrap_sessions(
    http: &reqwest::Client,
    base_url: &str,
    active_only: bool,
    json: bool,
) -> Result<()> {
    let mut url = format!("{}/orchestrate/bootstrap/sessions", base_url);
    if active_only {
        url.push_str("?active=true");
    }

    let resp = http.get(&url).send().await?;
    let status = resp.status();
    let result: serde_json::Value = resp.json().await?;

    if !status.is_success() {
        anyhow::bail!("Failed to list sessions ({}): {}", status, result);
    }

    if json {
        println!("{}", serde_json::to_string_pretty(&result)?);
    } else {
        let sessions = result.get("sessions").and_then(|v| v.as_array());
        match sessions {
            Some(list) if !list.is_empty() => {
                println!("Bootstrap Sessions ({})", list.len());
                println!("{}", "=".repeat(60));
                for s in list {
                    let id = s.get("session_id").and_then(|v| v.as_str()).unwrap_or("?");
                    let step = s.get("step").and_then(|v| v.as_str()).unwrap_or("?");
                    let node = s.get("node_type").and_then(|v| v.as_str()).unwrap_or("?");
                    let complete = s.get("is_complete").and_then(|v| v.as_bool()).unwrap_or(false);
                    let failed = s.get("is_failed").and_then(|v| v.as_bool()).unwrap_or(false);

                    let status_icon = if complete { "done" } else if failed { "FAIL" } else { "..." };
                    println!("  [{}] {} ({}) - {}", status_icon, id, node, step);
                }
            }
            _ => {
                println!("No bootstrap sessions found.");
            }
        }
    }

    Ok(())
}

/// Get bootstrap session status
async fn cmd_bootstrap_status(
    http: &reqwest::Client,
    base_url: &str,
    session_id: &str,
    json: bool,
) -> Result<()> {
    let url = format!("{}/orchestrate/bootstrap/sessions/{}", base_url, session_id);
    let resp = http.get(&url).send().await?;
    let status = resp.status();
    let result: serde_json::Value = resp.json().await?;

    if !status.is_success() {
        anyhow::bail!("Failed to get session ({}): {}", status, result);
    }

    if json {
        println!("{}", serde_json::to_string_pretty(&result)?);
    } else {
        println!("Bootstrap Session: {}", session_id);
        println!("{}", "=".repeat(50));
        println!("  Step:       {}", result.get("step").and_then(|v| v.as_str()).unwrap_or("?"));
        println!("  Node Type:  {}", result.get("node_type").and_then(|v| v.as_str()).unwrap_or("?"));
        println!("  P2P:        {}", if result.get("p2p_connected").and_then(|v| v.as_bool()).unwrap_or(false) { "connected" } else { "not connected" });

        if let Some(dseq) = result.get("akash_dseq").and_then(|v| v.as_u64()) {
            println!("  Akash DSEQ: {}", dseq);
        }
        if let Some(provider) = result.get("akash_provider").and_then(|v| v.as_str()) {
            println!("  Provider:   {}", provider);
        }
        if let Some(pubkey) = result.get("generated_pubkey").and_then(|v| v.as_str()) {
            println!("  Pubkey:     {}", pubkey);
        }
        if let Some(errors) = result.get("errors").and_then(|v| v.as_array()) {
            if !errors.is_empty() {
                println!("  Errors:");
                for e in errors {
                    println!("    - {}", e.as_str().unwrap_or("?"));
                }
            }
        }

        let complete = result.get("is_complete").and_then(|v| v.as_bool()).unwrap_or(false);
        let failed = result.get("is_failed").and_then(|v| v.as_bool()).unwrap_or(false);
        if complete {
            println!("\n  Status: COMPLETE");
        } else if failed {
            println!("\n  Status: FAILED");
        } else {
            println!("\n  Status: IN PROGRESS");
        }
    }

    Ok(())
}

/// Delete bootstrap session
async fn cmd_delete_bootstrap_session(
    http: &reqwest::Client,
    base_url: &str,
    session_id: &str,
    force: bool,
    json: bool,
) -> Result<()> {
    if !force {
        print!("Delete bootstrap session {}? (y/N): ", session_id);
        use std::io::Write;
        std::io::stdout().flush()?;
        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        if !input.trim().eq_ignore_ascii_case("y") {
            println!("Cancelled.");
            return Ok(());
        }
    }

    let url = format!("{}/orchestrate/bootstrap/sessions/{}", base_url, session_id);
    let resp = http.delete(&url).send().await?;
    let status = resp.status();
    let result: serde_json::Value = resp.json().await?;

    if !status.is_success() {
        anyhow::bail!("Failed to delete session ({}): {}", status, result);
    }

    if json {
        println!("{}", serde_json::to_string_pretty(&result)?);
    } else {
        println!("Deleted session: {}", session_id);
    }

    Ok(())
}
