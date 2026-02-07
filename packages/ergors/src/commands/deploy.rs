//! Akash deployment management CLI commands
//!
//! Commands for managing Akash network deployments through the engine.

use anyhow::Result;
use clap::Subcommand;
use std::collections::HashMap;
use std::io::IsTerminal;

use super::CliContext;
use crate::client::ManagementClient;

/// Akash deployment management commands
#[derive(Subcommand)]
pub enum DeployCmd {
    /// Create a new Akash deployment (automated by default)
    Create {
        /// User-friendly label for this deployment (e.g., "embeddings-gpu", "inference-1")
        /// Must be unique across active deployments. Use this label in other commands instead of session-id.
        #[arg(long)]
        label: Option<String>,
        /// Path to SDL file
        #[arg(long)]
        sdl: Option<String>,
        /// Raw SDL content (alternative to --sdl)
        #[arg(long)]
        sdl_content: Option<String>,
        /// Key name for signing transactions
        #[arg(long, default_value = "default")]
        key_name: String,
        /// HD account index
        #[arg(long, default_value = "0")]
        account_index: u32,
        /// Override node RPC endpoint
        #[arg(long, env = "AKASH_NODE")]
        node: Option<String>,
        /// Override chain ID
        #[arg(long, env = "AKASH_CHAIN_ID")]
        chain_id: Option<String>,
        /// Minimum balance required in uakt (default: 5000000)
        #[arg(long, default_value = "5000000")]
        min_balance: u64,
        /// SDL template variables (key=value pairs)
        #[arg(long, value_parser = parse_key_val)]
        var: Vec<(String, String)>,

        // Grant options (opt-in)
        /// Request grant from node (ergo1... bech32 address). Waits indefinitely for approval.
        #[arg(long)]
        request_grant_from: Option<String>,
        /// Grant duration in seconds (default: 86400 = 24h). Only used with --request-grant-from.
        #[arg(long, default_value = "86400")]
        grant_duration: u64,
        /// Grant spend limit in uakt (default: 5000000 = 5 AKT). Only used with --request-grant-from.
        #[arg(long, default_value = "5000000")]
        grant_spend_limit: u64,

        // Provider selection options
        /// Prompt for manual provider selection instead of auto-selecting cheapest
        #[arg(long)]
        interactive_bid: bool,

        /// Actual model name for inference routing (e.g., "Qwen/Qwen3-235B-A22B-FP8").
        /// The inference server receives this as the "model" field instead of the label.
        #[arg(long)]
        model_name: Option<String>,
    },
    /// Run automated deployment workflow on existing session
    Run {
        /// Session ID or label
        session_id: String,
        /// Minimum balance required in uakt
        #[arg(long, default_value = "5000000")]
        min_balance: u64,

        // Grant options (opt-in)
        /// Request grant from node (ergo1... bech32 address). Waits indefinitely for approval.
        #[arg(long)]
        request_grant_from: Option<String>,
        /// Grant duration in seconds (default: 86400 = 24h). Only used with --request-grant-from.
        #[arg(long, default_value = "86400")]
        grant_duration: u64,
        /// Grant spend limit in uakt (default: 5000000 = 5 AKT). Only used with --request-grant-from.
        #[arg(long, default_value = "5000000")]
        grant_spend_limit: u64,

        // Provider selection options
        /// Prompt for manual provider selection instead of auto-selecting cheapest
        #[arg(long)]
        interactive_bid: bool,
    },
    /// List deployment workflows
    List {
        /// Filter by status (pending, running, completed, failed)
        #[arg(long)]
        status: Option<String>,
        /// Maximum results
        #[arg(long, default_value = "50")]
        limit: u32,
    },
    /// Get deployment workflow details
    Get {
        /// Session ID or label
        session_id: String,
    },
    /// Get comprehensive deployment information (unified view)
    Info {
        /// Session ID or label
        session_id: String,
    },
    /// Query bids for a deployment
    Bids {
        /// Session ID or label
        session_id: String,
    },
    /// Select a provider for the deployment
    Select {
        /// Session ID or label
        session_id: String,
        /// Bid selection: Either a numerical ID (1, 2, 3, ...) from the bids list, or a provider address (akash1...)
        bid: String,
    },
    /// Get service endpoints for a deployment
    Endpoints {
        /// Session ID or label
        session_id: String,
    },
    /// Cancel a deployment workflow
    Cancel {
        /// Session ID or label
        session_id: String,
    },
    /// Set discovered endpoints for a deployment workflow
    SetEndpoints {
        /// Session ID or label
        session_id: String,
        /// Endpoints as key=value pairs (service_name=url)
        #[arg(long, value_parser = parse_key_val)]
        endpoint: Vec<(String, String)>,
    },
    /// Configure proxy routing to discovered services
    ConfigureProxy {
        /// Model routing rules (glob=url pairs)
        #[arg(long, value_parser = parse_key_val)]
        route: Vec<(String, String)>,
    },
    /// Request authz grant from coordinator
    RequestGrant {
        /// Granter address (coordinator who provides funds/permissions)
        #[arg(long)]
        granter: String,
        /// Grantee address (executor requesting permissions)
        #[arg(long)]
        grantee: String,
        /// Message types to grant (e.g., /akash.deployment.v1beta3.MsgCreateDeployment)
        #[arg(long)]
        msg_type: Vec<String>,
        /// Feegrant allowance amount in uakt (0 = no feegrant, only authz)
        #[arg(long, default_value = "0")]
        allowance: u64,
        /// Reason for grant request
        #[arg(long)]
        reason: Option<String>,
    },
    /// Approve pending grant request
    ApproveGrant {
        /// Request ID to approve
        request_id: String,
        /// Reject instead of approve
        #[arg(long)]
        reject: bool,
        /// Reason for decision
        #[arg(long)]
        reason: Option<String>,
    },
    /// Revoke an existing grant
    RevokeGrant {
        /// Granter address
        #[arg(long)]
        granter: String,
        /// Grantee address
        #[arg(long)]
        grantee: String,
        /// Message type to revoke (empty = revoke all)
        #[arg(long)]
        msg_type: Option<String>,
        /// Also revoke feegrant
        #[arg(long)]
        revoke_feegrant: bool,
    },
    /// List pending grant requests
    ListGrants {
        /// Filter by granter address
        #[arg(long)]
        granter: Option<String>,
        /// Filter by grantee address
        #[arg(long)]
        grantee: Option<String>,
        /// Filter by status (pending, approved, rejected)
        #[arg(long)]
        status: Option<String>,
    },
    /// Query account balance
    QueryBalance {
        /// Account address to query
        address: String,
        /// Denom to query (default: uakt)
        #[arg(long, default_value = "uakt")]
        denom: String,
    },
    /// Close an active lease
    CloseLease {
        /// Session ID or label
        session_id: String,
    },
    /// Close a deployment (also closes any active leases)
    CloseDeployment {
        /// Session ID or label
        session_id: String,
    },
    /// Update a deployment with new SDL
    UpdateDeployment {
        /// Session ID or label
        session_id: String,
        /// Path to new SDL file
        #[arg(long)]
        sdl: String,
    },
    /// Top up escrow account for a deployment
    TopupEscrow {
        /// Session ID or label
        session_id: String,
        /// Amount to deposit in uakt
        amount: u64,
    },
    /// Get lease status
    Status {
        /// Session ID or label
        session_id: String,
        /// Follow logs (poll continuously)
        #[arg(short, long)]
        follow: bool,
    },
    /// List trusted providers
    TrustedProviders,
    /// Add a trusted provider
    AddProvider {
        /// Provider address
        address: String,
        /// Optional label for the provider
        #[arg(long, default_value = "")]
        label: String,
    },
    /// Remove a trusted provider
    RemoveProvider {
        /// Provider address
        address: String,
    },
    /// Certificate management (create, revoke, show)
    Cert {
        #[command(subcommand)]
        cmd: CertCmd,
    },
    /// Query and cache provider information
    ProviderInfo {
        /// Provider address to query
        address: String,
        /// Force refresh from chain (ignore cache)
        #[arg(long)]
        refresh: bool,
    },
}

/// Certificate management subcommands
#[derive(Subcommand)]
pub enum CertCmd {
    /// Create a new certificate for mTLS authentication with providers
    Create {
        /// Key name for signing transactions
        #[arg(long, default_value = "default")]
        key_name: String,
        /// HD account index
        #[arg(long, default_value = "0")]
        account_index: u32,
    },
    /// Revoke an existing certificate
    Revoke {
        /// Key name for signing transactions
        #[arg(long, default_value = "default")]
        key_name: String,
        /// HD account index
        #[arg(long, default_value = "0")]
        account_index: u32,
        /// Certificate serial number (optional - uses latest if not specified)
        #[arg(long)]
        serial: Option<String>,
    },
    /// Show certificate status for an address
    Show {
        /// Key name to check (derives address)
        #[arg(long, default_value = "default")]
        key_name: String,
        /// HD account index
        #[arg(long, default_value = "0")]
        account_index: u32,
    },
}

/// Parse a key=value pair
fn parse_key_val(s: &str) -> Result<(String, String), String> {
    let pos = s
        .find('=')
        .ok_or_else(|| format!("invalid KEY=value: no `=` found in `{s}`"))?;
    Ok((s[..pos].to_string(), s[pos + 1..].to_string()))
}

impl DeployCmd {
    pub async fn execute(&self, ctx: &CliContext, mut client: ManagementClient) -> Result<()> {
        match self {
            DeployCmd::Create {
                label,
                sdl,
                sdl_content,
                key_name,
                account_index,
                node,
                chain_id,
                min_balance,
                var,
                request_grant_from,
                grant_duration,
                grant_spend_limit,
                interactive_bid,
                model_name,
            } => {
                // Resolve SDL content
                let content = match (sdl, sdl_content) {
                    (Some(path), _) => std::fs::read_to_string(path).map_err(|e| {
                        anyhow::anyhow!("Failed to read SDL file '{}': {}", path, e)
                    })?,
                    (_, Some(raw)) => raw.clone(),
                    (None, None) => {
                        anyhow::bail!("Either --sdl <path> or --sdl-content <yaml> is required");
                    }
                };

                let variables: HashMap<String, String> = var.iter().cloned().collect();

                // Create deployment (auto is now default)
                let response = client
                    .create_akash_deployment(
                        key_name,
                        *account_index,
                        &content,
                        "",
                        variables,
                        node.as_deref().unwrap_or(""),
                        chain_id.as_deref().unwrap_or("akashnet-2"),
                        true, // auto is always true now
                        label.as_deref().unwrap_or(""),
                        model_name.as_deref().unwrap_or(""),
                    )
                    .await?;

                if response.success {
                    let session_id = response
                        .workflow
                        .as_ref()
                        .map(|wf| wf.session_id.clone())
                        .unwrap_or_default();

                    if !ctx.json {
                        println!("Deployment workflow created!");
                        if let Some(wf) = &response.workflow {
                            println!("  Session ID: {}", wf.session_id);
                            println!("  Account:    {}", wf.account_address);
                            println!("  Chain:      {}", wf.chain_id);
                            println!("  Node:       {}", wf.node_endpoint);
                            println!("  Step:       {}", format_step(wf.current_step));
                            println!("  Status:     {}", format_status(wf.status));
                        }
                    }

                    // Always run automated workflow
                    if !session_id.is_empty() {
                        if !ctx.json {
                            println!("\nRunning automated workflow...");
                        }

                        // Prompt for key password
                        let key_password = if std::io::stdin().is_terminal() {
                            rpassword::prompt_password("Enter Cosmos key password: ")
                                .map_err(|e| anyhow::anyhow!("Failed to read password: {}", e))?
                        } else {
                            String::new()
                        };

                        // Get trusted providers list for auto-selection (unless interactive mode)
                        let trusted_providers = if !*interactive_bid {
                            match client.list_trusted_providers().await {
                                Ok(resp) => {
                                    resp.providers.iter().map(|p| p.address.clone()).collect()
                                }
                                Err(_) => vec![],
                            }
                        } else {
                            vec![]
                        };

                        let run_response = client
                            .run_akash_deployment(
                                &session_id,
                                *interactive_bid,
                                *min_balance,
                                trusted_providers,
                                request_grant_from.as_deref().unwrap_or(""),
                                *grant_duration,
                                *grant_spend_limit,
                                &key_password,
                            )
                            .await?;

                        if ctx.json {
                            let mut json = serde_json::json!({
                                "session_id": session_id,
                                "completed": run_response.completed,
                            });
                            if let Some(wf) = &run_response.workflow {
                                json["current_step"] =
                                    serde_json::json!(format_step(wf.current_step));
                                json["status"] = serde_json::json!(format_status(wf.status));
                            }
                            if let Some(input) = &run_response.input_required {
                                json["input_required"] = serde_json::json!(&input.message);
                            }
                            println!("{}", serde_json::to_string_pretty(&json)?);
                        } else if run_response.completed {
                            println!("Deployment workflow completed!");
                        } else {
                            if let Some(wf) = &run_response.workflow {
                                println!("  Step:    {}", format_step(wf.current_step));
                                println!("  Status:  {}", format_status(wf.status));
                            }
                            if let Some(input) = &run_response.input_required {
                                println!("  Needs:   {}", input.message);
                            }
                        }
                    } else if ctx.json {
                        if let Some(wf) = &response.workflow {
                            println!(
                                "{}",
                                serde_json::to_string_pretty(&serde_json::json!({
                                    "session_id": wf.session_id,
                                    "status": wf.status,
                                    "current_step": wf.current_step,
                                    "account_address": wf.account_address,
                                    "chain_id": wf.chain_id,
                                    "node_endpoint": wf.node_endpoint,
                                }))?
                            );
                        }
                    }
                } else {
                    eprintln!("Failed to create deployment: {}", response.error_message);
                }
                Ok(())
            }
            DeployCmd::List { status, limit } => {
                let status_filter = status
                    .as_deref()
                    .map(|s| match s.to_lowercase().as_str() {
                        "pending" => 1,
                        "running" => 2,
                        "paused" => 3,
                        "completed" => 4,
                        "failed" => 5,
                        "cancelled" => 6,
                        _ => 0,
                    })
                    .unwrap_or(0);

                let response = client.list_akash_deployments(status_filter, *limit).await?;

                if ctx.json {
                    let workflows: Vec<_> = response
                        .workflows
                        .iter()
                        .map(|wf| {
                            serde_json::json!({
                                "session_id": wf.session_id,
                                "status": format_status(wf.status),
                                "current_step": format_step(wf.current_step),
                                "account_address": wf.account_address,
                            })
                        })
                        .collect();
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&serde_json::json!({
                            "workflows": workflows,
                            "total_count": response.total_count,
                        }))?
                    );
                } else {
                    println!("Akash Deployments ({} total)", response.total_count);
                    println!("==========================");

                    if response.workflows.is_empty() {
                        println!("No deployment workflows found.");
                    } else {
                        for wf in &response.workflows {
                            println!(
                                "  {} | {} | {} | {}",
                                &wf.session_id[..8.min(wf.session_id.len())],
                                format_status(wf.status),
                                format_step(wf.current_step),
                                wf.account_address,
                            );
                        }
                    }
                }
                Ok(())
            }
            DeployCmd::Get { session_id } => {
                let response = client.get_akash_deployment(session_id).await?;

                if let Some(wf) = &response.workflow {
                    if ctx.json {
                        println!(
                            "{}",
                            serde_json::to_string_pretty(&serde_json::json!({
                                "session_id": wf.session_id,
                                "status": format_status(wf.status),
                                "current_step": format_step(wf.current_step),
                                "account_address": wf.account_address,
                                "chain_id": wf.chain_id,
                                "node_endpoint": wf.node_endpoint,
                                "selected_key_name": wf.selected_key_name,
                                "last_error": wf.last_error,
                                "retry_count": wf.retry_count,
                            }))?
                        );
                    } else {
                        println!("Deployment Workflow: {}", wf.session_id);
                        println!("====================");
                        println!("Status:     {}", format_status(wf.status));
                        println!("Step:       {}", format_step(wf.current_step));
                        println!("Account:    {}", wf.account_address);
                        println!("Key:        {}", wf.selected_key_name);
                        println!("Chain:      {}", wf.chain_id);
                        println!("Node:       {}", wf.node_endpoint);

                        if let Some(runtime) = &wf.deployment {
                            println!("\nDeployment Info:");
                            println!("  DSEQ:     {}", runtime.deployment_sequence);
                            println!("  Provider: {}", runtime.provider_address);
                            println!("  Lease:    {}", runtime.lease_id);
                            if !runtime.service_endpoints.is_empty() {
                                println!("  Endpoints:");
                                for ep in &runtime.service_endpoints {
                                    println!("    - {}", ep);
                                }
                            }
                        }

                        if !wf.last_error.is_empty() {
                            println!("\nLast Error: {}", wf.last_error);
                        }
                    }
                } else {
                    println!("Deployment not found: {}", session_id);
                }
                Ok(())
            }
            DeployCmd::Info { session_id } => {
                // Get workflow info
                let wf_response = client.get_akash_deployment(session_id).await?;

                if let Some(wf) = wf_response.workflow {
                    if ctx.json {
                        // Build comprehensive JSON output
                        let mut info = serde_json::json!({
                            "session_id": wf.session_id,
                            "status": format_status(wf.status),
                            "current_step": format_step(wf.current_step),
                            "account_address": wf.account_address,
                            "chain_id": wf.chain_id,
                            "node_endpoint": wf.node_endpoint,
                        });

                        if let Some(runtime) = &wf.deployment {
                            info["deployment"] = serde_json::json!({
                                "dseq": runtime.deployment_sequence,
                                "provider": runtime.provider_address,
                                "lease_id": runtime.lease_id,
                            });
                        }

                        if let Some(lease_id) = &wf.lease_id_info {
                            info["lease"] = serde_json::json!({
                                "owner": lease_id.owner,
                                "dseq": lease_id.dseq,
                                "gseq": lease_id.gseq,
                                "oseq": lease_id.oseq,
                                "provider": lease_id.provider,
                            });
                        }

                        if !wf.service_endpoints.is_empty() {
                            info["endpoints"] = serde_json::json!(wf
                                .service_endpoints
                                .iter()
                                .map(|e| {
                                    serde_json::json!({
                                        "service": e.service_name,
                                        "uri": e.external_uri,
                                        "internal_port": e.internal_port,
                                        "external_port": e.external_port,
                                        "protocol": e.protocol,
                                    })
                                })
                                .collect::<Vec<_>>());
                        }

                        println!("{}", serde_json::to_string_pretty(&info)?);
                    } else {
                        println!(
                            "╔══════════════════════════════════════════════════════════════╗"
                        );
                        println!(
                            "║             Akash Deployment Information                     ║"
                        );
                        println!(
                            "╠══════════════════════════════════════════════════════════════╣"
                        );
                        println!("║ Session ID: {:44} ║", truncate_or_pad(&wf.session_id, 44));
                        println!("║ Status:     {:44} ║", format_status(wf.status));
                        println!("║ Step:       {:44} ║", format_step(wf.current_step));
                        println!(
                            "╠══════════════════════════════════════════════════════════════╣"
                        );
                        println!(
                            "║ Account                                                      ║"
                        );
                        println!(
                            "╠══════════════════════════════════════════════════════════════╣"
                        );
                        println!(
                            "║ Address:    {:44} ║",
                            truncate_or_pad(&wf.account_address, 44)
                        );
                        println!(
                            "║ Key:        {:44} ║",
                            truncate_or_pad(&wf.selected_key_name, 44)
                        );
                        println!("║ Chain:      {:44} ║", truncate_or_pad(&wf.chain_id, 44));

                        if let Some(runtime) = &wf.deployment {
                            println!(
                                "╠══════════════════════════════════════════════════════════════╣"
                            );
                            println!(
                                "║ Deployment                                                   ║"
                            );
                            println!(
                                "╠══════════════════════════════════════════════════════════════╣"
                            );
                            println!(
                                "║ DSEQ:       {:44} ║",
                                format!("{}", runtime.deployment_sequence)
                            );
                            if !runtime.provider_address.is_empty() {
                                println!(
                                    "║ Provider:   {:44} ║",
                                    truncate_or_pad(&runtime.provider_address, 44)
                                );
                            }
                        }

                        if let Some(lease_id) = &wf.lease_id_info {
                            println!(
                                "╠══════════════════════════════════════════════════════════════╣"
                            );
                            println!(
                                "║ Lease                                                        ║"
                            );
                            println!(
                                "╠══════════════════════════════════════════════════════════════╣"
                            );
                            println!("║ DSEQ:       {:44} ║", format!("{}", lease_id.dseq));
                            println!("║ GSEQ:       {:44} ║", format!("{}", lease_id.gseq));
                            println!("║ OSEQ:       {:44} ║", format!("{}", lease_id.oseq));
                            println!(
                                "║ Provider:   {:44} ║",
                                truncate_or_pad(&lease_id.provider, 44)
                            );
                        }

                        if !wf.service_endpoints.is_empty() {
                            println!(
                                "╠══════════════════════════════════════════════════════════════╣"
                            );
                            println!(
                                "║ Service Endpoints                                            ║"
                            );
                            println!(
                                "╠══════════════════════════════════════════════════════════════╣"
                            );
                            for ep in &wf.service_endpoints {
                                println!(
                                    "║ Service:    {:44} ║",
                                    truncate_or_pad(&ep.service_name, 44)
                                );
                                println!(
                                    "║   URI:      {:44} ║",
                                    truncate_or_pad(&ep.external_uri, 44)
                                );
                                println!(
                                    "║   Port:     {:44} ║",
                                    format!(
                                        "{}:{} ({})",
                                        ep.external_port, ep.internal_port, ep.protocol
                                    )
                                );
                            }
                        }

                        if !wf.last_error.is_empty() {
                            println!(
                                "╠══════════════════════════════════════════════════════════════╣"
                            );
                            println!(
                                "║ Last Error                                                   ║"
                            );
                            println!(
                                "╠══════════════════════════════════════════════════════════════╣"
                            );
                            for line in wf.last_error.lines() {
                                println!("║ {:60} ║", truncate_or_pad(line, 60));
                            }
                        }

                        println!(
                            "╚══════════════════════════════════════════════════════════════╝"
                        );
                    }
                } else {
                    eprintln!("Deployment not found: {}", session_id);
                }
                Ok(())
            }
            DeployCmd::Bids { session_id } => {
                let response = client.query_akash_bids(session_id).await?;

                if ctx.json {
                    let bids: Vec<_> = response
                        .bids
                        .iter()
                        .map(|b| {
                            serde_json::json!({
                                "provider": b.provider_address,
                                "price_uakt": b.price_uakt,
                            })
                        })
                        .collect();
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&serde_json::json!({
                            "bids": bids,
                            "total": response.total_bids,
                        }))?
                    );
                } else {
                    println!("Bids ({} total)", response.total_bids);
                    println!("==========");

                    if response.bids.is_empty() {
                        println!("No bids received yet.");
                    } else {
                        for (idx, bid) in response.bids.iter().enumerate() {
                            let price_akt = bid.price_uakt as f64 / 1_000_000.0;
                            println!(
                                "  [{}] {} | {:.6} AKT/block ({} uakt)",
                                idx + 1,
                                bid.provider_address,
                                price_akt,
                                bid.price_uakt
                            );
                        }
                        println!();
                        println!("To select a bid: ergors deploy select <session-id> <bid-number>");
                        println!("Example: ergors deploy select {} 1", session_id);
                    }
                }
                Ok(())
            }
            DeployCmd::Select { session_id, bid } => {
                // Determine if bid is a numeric ID or provider address
                let (provider_address, price) = if let Ok(bid_idx) = bid.parse::<usize>() {
                    // Numeric ID: query bids and get the provider at that index
                    let bids_response = client.query_akash_bids(session_id).await?;

                    if bid_idx == 0 || bid_idx > bids_response.bids.len() {
                        return Err(anyhow::anyhow!(
                            "Invalid bid number {}. Valid range: 1-{}",
                            bid_idx,
                            bids_response.bids.len()
                        ));
                    }

                    let selected_bid = &bids_response.bids[bid_idx - 1];
                    println!(
                        "Selected bid [{}]: {} ({} uakt/block)",
                        bid_idx, selected_bid.provider_address, selected_bid.price_uakt
                    );

                    (
                        selected_bid.provider_address.clone(),
                        selected_bid.price_uakt,
                    )
                } else if bid.starts_with("akash1") {
                    // Provider address: use it directly
                    (bid.clone(), 0)
                } else {
                    return Err(anyhow::anyhow!(
                        "Invalid bid selection '{}'. Must be a number (e.g., 1, 2, 3) or provider address (akash1...)",
                        bid
                    ));
                };

                let response = client
                    .select_akash_provider(session_id, &provider_address, price)
                    .await?;

                if ctx.json {
                    let mut json = serde_json::json!({
                        "success": response.success,
                        "provider": provider_address,
                        "price_uakt": price,
                    });
                    if let Some(wf) = &response.workflow {
                        json["current_step"] = serde_json::json!(format_step(wf.current_step));
                        json["status"] = serde_json::json!(format_status(wf.status));
                    }
                    if !response.error_message.is_empty() {
                        json["error"] = serde_json::json!(&response.error_message);
                    }
                    println!("{}", serde_json::to_string_pretty(&json)?);
                } else if response.success {
                    println!("Provider selected: {}", provider_address);
                    if price > 0 {
                        println!("  Price: {} uakt/block", price);
                    }
                    if let Some(wf) = &response.workflow {
                        println!("  Step: {}", format_step(wf.current_step));
                        println!("  Status: {}", format_status(wf.status));
                    }
                } else {
                    eprintln!("Failed to select provider: {}", response.error_message);
                }
                Ok(())
            }
            DeployCmd::Endpoints { session_id } => {
                // Query the workflow to get endpoints
                let wf_response = client.get_akash_deployment(session_id).await?;

                if let Some(wf) = wf_response.workflow {
                    if ctx.json {
                        let endpoints: Vec<_> = wf
                            .service_endpoints
                            .iter()
                            .map(|ep| {
                                serde_json::json!({
                                    "service": ep.service_name,
                                    "uri": ep.external_uri,
                                    "internal_port": ep.internal_port,
                                    "external_port": ep.external_port,
                                    "protocol": ep.protocol,
                                })
                            })
                            .collect();
                        println!(
                            "{}",
                            serde_json::to_string_pretty(&serde_json::json!({
                                "session_id": session_id,
                                "endpoints": endpoints,
                                "total": wf.service_endpoints.len(),
                            }))?
                        );
                    } else {
                        println!("Service Endpoints for {}", session_id);
                        println!("═══════════════════════════════════════════");

                        if wf.service_endpoints.is_empty() {
                            println!("No endpoints available yet.");
                            println!();
                            println!("The deployment may still be starting up.");
                            println!("Wait a moment and try again.");
                        } else {
                            for (idx, ep) in wf.service_endpoints.iter().enumerate() {
                                if idx > 0 {
                                    println!();
                                }
                                println!("Service: {}", ep.service_name);
                                println!("  URI:          {}", ep.external_uri);
                                println!("  Internal Port: {}", ep.internal_port);
                                println!("  External Port: {}", ep.external_port);
                                println!("  Protocol:      {}", ep.protocol);
                            }
                            println!();
                            println!("Total: {} endpoint(s)", wf.service_endpoints.len());
                        }
                    }
                } else {
                    eprintln!("Deployment not found: {}", session_id);
                }
                Ok(())
            }
            DeployCmd::Cancel { session_id } => {
                let result = client.cancel_akash_deployment(session_id).await?;

                if result.success {
                    println!("Deployment cancelled: {}", session_id);
                } else {
                    eprintln!("Failed to cancel: {}", result.message);
                }
                Ok(())
            }
            DeployCmd::SetEndpoints {
                session_id,
                endpoint,
            } => {
                let endpoints: HashMap<String, String> = endpoint.iter().cloned().collect();
                let response = client.set_workflow_endpoints(session_id, endpoints).await?;

                if ctx.json {
                    let mut json = serde_json::json!({
                        "success": response.success,
                    });
                    if let Some(wf) = &response.workflow {
                        json["current_step"] = serde_json::json!(format_step(wf.current_step));
                        json["endpoints"] = serde_json::json!(&wf.endpoints);
                    }
                    if !response.error_message.is_empty() {
                        json["error"] = serde_json::json!(&response.error_message);
                    }
                    println!("{}", serde_json::to_string_pretty(&json)?);
                } else if response.success {
                    println!("Endpoints set for workflow {}", session_id);
                    if let Some(wf) = &response.workflow {
                        for (name, uri) in &wf.endpoints {
                            println!("  {} -> {}", name, uri);
                        }
                    }
                } else {
                    eprintln!("Failed to set endpoints: {}", response.error_message);
                }
                Ok(())
            }
            DeployCmd::ConfigureProxy { route } => {
                let model_routes: HashMap<String, String> = route.iter().cloned().collect();
                let result = client.configure_proxy_routes(model_routes).await?;
                if ctx.json || result.success {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&serde_json::json!({
                            "success": result.success,
                            "message": result.message,
                        }))?
                    );
                } else {
                    eprintln!("Failed to configure proxy: {}", result.message);
                }
                Ok(())
            }
            DeployCmd::RequestGrant {
                granter,
                grantee,
                msg_type,
                allowance,
                reason,
            } => {
                let response = client
                    .request_grant(
                        granter,
                        grantee,
                        msg_type.clone(),
                        *allowance,
                        reason.as_deref(),
                    )
                    .await?;

                if ctx.json {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&serde_json::json!({
                            "success": response.success,
                            "message": response.message,
                            "request_id": response.request_id,
                        }))?
                    );
                } else if response.success {
                    println!("Grant request submitted");
                    println!("  Request ID: {}", response.request_id);
                    println!("  Message:    {}", response.message);
                } else {
                    eprintln!("Failed to request grant: {}", response.message);
                }
                Ok(())
            }
            DeployCmd::ApproveGrant {
                request_id,
                reject,
                reason,
            } => {
                let result = client
                    .approve_grant(request_id, !reject, reason.as_deref())
                    .await?;

                if ctx.json {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&serde_json::json!({
                            "success": result.success,
                            "message": result.message,
                        }))?
                    );
                } else if result.success {
                    if *reject {
                        println!("Grant request rejected: {}", request_id);
                    } else {
                        println!("Grant request approved: {}", request_id);
                    }
                } else {
                    eprintln!("Failed to process grant: {}", result.message);
                }
                Ok(())
            }
            DeployCmd::RevokeGrant {
                granter,
                grantee,
                msg_type,
                revoke_feegrant,
            } => {
                let result = client
                    .revoke_grant(granter, grantee, msg_type.as_deref(), *revoke_feegrant)
                    .await?;

                if ctx.json {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&serde_json::json!({
                            "success": result.success,
                            "message": result.message,
                        }))?
                    );
                } else if result.success {
                    println!("Grant revoked");
                    if *revoke_feegrant {
                        println!("  Feegrant also revoked");
                    }
                } else {
                    eprintln!("Failed to revoke grant: {}", result.message);
                }
                Ok(())
            }
            DeployCmd::ListGrants {
                granter,
                grantee,
                status,
            } => {
                let response = client
                    .list_grant_requests(granter.as_deref(), grantee.as_deref(), status.as_deref())
                    .await?;

                if ctx.json {
                    let requests: Vec<_> = response
                        .requests
                        .iter()
                        .map(|r| {
                            serde_json::json!({
                                "request_id": r.request_id,
                                "granter": r.granter_address,
                                "grantee": r.grantee_address,
                                "msg_types": r.msg_types,
                                "allowance": r.allowance_amount,
                                "status": r.status,
                            })
                        })
                        .collect();
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&serde_json::json!({"requests": requests}))?
                    );
                } else {
                    println!("Grant Requests ({} total)", response.requests.len());
                    println!("=======================");
                    if response.requests.is_empty() {
                        println!("No grant requests found.");
                    } else {
                        for req in &response.requests {
                            println!(
                                "  {} | {} -> {} | {} | {}",
                                &req.request_id[..8.min(req.request_id.len())],
                                &req.granter_address[..12],
                                &req.grantee_address[..12],
                                req.status,
                                req.allowance_amount
                            );
                        }
                    }
                }
                Ok(())
            }
            DeployCmd::QueryBalance { address, denom } => {
                let response = client.query_balance(address, denom).await?;

                if ctx.json {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&serde_json::json!({
                            "address": response.address,
                            "denom": response.denom,
                            "amount": response.amount,
                        }))?
                    );
                } else {
                    println!("Account Balance:");
                    println!("  Address: {}", response.address);
                    println!("  Denom:   {}", response.denom);
                    println!("  Amount:  {}", response.amount);
                }
                Ok(())
            }
            DeployCmd::Run {
                session_id,
                min_balance,
                request_grant_from,
                grant_duration,
                grant_spend_limit,
                interactive_bid,
            } => {
                // Prompt for key password
                let key_password = if std::io::stdin().is_terminal() {
                    rpassword::prompt_password("Enter Cosmos key password: ")
                        .map_err(|e| anyhow::anyhow!("Failed to read password: {}", e))?
                } else {
                    String::new()
                };

                // Get trusted providers list for auto-selection (unless interactive mode)
                let trusted_providers = if !*interactive_bid {
                    match client.list_trusted_providers().await {
                        Ok(resp) => resp.providers.iter().map(|p| p.address.clone()).collect(),
                        Err(_) => vec![],
                    }
                } else {
                    vec![]
                };

                let response = client
                    .run_akash_deployment(
                        session_id,
                        *interactive_bid,
                        *min_balance,
                        trusted_providers,
                        request_grant_from.as_deref().unwrap_or(""),
                        *grant_duration,
                        *grant_spend_limit,
                        &key_password,
                    )
                    .await?;

                if ctx.json {
                    let mut json = serde_json::json!({
                        "completed": response.completed,
                    });
                    if let Some(wf) = &response.workflow {
                        json["session_id"] = serde_json::json!(&wf.session_id);
                        json["current_step"] = serde_json::json!(format_step(wf.current_step));
                        json["status"] = serde_json::json!(format_status(wf.status));
                    }
                    if let Some(input) = &response.input_required {
                        json["input_required"] = serde_json::json!(&input.message);
                    }
                    println!("{}", serde_json::to_string_pretty(&json)?);
                } else if response.completed {
                    println!("Deployment workflow completed!");
                    if let Some(wf) = &response.workflow {
                        println!("  Session: {}", wf.session_id);
                        println!("  Status:  {}", format_status(wf.status));
                    }
                } else {
                    println!("Deployment workflow running...");
                    if let Some(wf) = &response.workflow {
                        println!("  Session: {}", wf.session_id);
                        println!("  Step:    {}", format_step(wf.current_step));
                        println!("  Status:  {}", format_status(wf.status));
                    }
                    if let Some(input) = &response.input_required {
                        println!("  Needs:   {}", input.message);
                    }
                }
                Ok(())
            }
            DeployCmd::CloseLease { session_id } => {
                let result = client.close_akash_lease(session_id).await?;

                if ctx.json {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&serde_json::json!({
                            "success": result.success,
                            "message": result.message,
                        }))?
                    );
                } else if result.success {
                    println!("Lease closed for session: {}", session_id);
                } else {
                    eprintln!("Failed to close lease: {}", result.message);
                }
                Ok(())
            }
            DeployCmd::CloseDeployment { session_id } => {
                let result = client.close_akash_deployment(session_id).await?;

                if ctx.json {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&serde_json::json!({
                            "success": result.success,
                            "message": result.message,
                        }))?
                    );
                } else if result.success {
                    println!("Deployment closed for session: {}", session_id);
                    println!("  {}", result.message);
                } else {
                    eprintln!("Failed to close deployment: {}", result.message);
                }
                Ok(())
            }
            DeployCmd::UpdateDeployment { session_id, sdl } => {
                // Read SDL file
                let sdl_content = std::fs::read_to_string(sdl)
                    .map_err(|e| anyhow::anyhow!("Failed to read SDL file '{}': {}", sdl, e))?;

                let result = client
                    .update_akash_deployment(session_id, &sdl_content)
                    .await?;

                if ctx.json {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&serde_json::json!({
                            "success": result.success,
                            "message": result.message,
                        }))?
                    );
                } else if result.success {
                    println!("Deployment updated for session: {}", session_id);
                    println!("  {}", result.message);
                    println!("\nNote: You may need to send a new manifest to the provider");
                } else {
                    eprintln!("Failed to update deployment: {}", result.message);
                }
                Ok(())
            }
            DeployCmd::TopupEscrow { session_id, amount } => {
                let result = client.topup_akash_escrow(session_id, *amount).await?;

                if ctx.json {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&serde_json::json!({
                            "success": result.success,
                            "message": result.message,
                            "amount_uakt": amount,
                        }))?
                    );
                } else if result.success {
                    let amount_akt = *amount as f64 / 1_000_000.0;
                    println!("Escrow topped up for session: {}", session_id);
                    println!("  Amount: {} uakt ({:.6} AKT)", amount, amount_akt);
                    println!("  {}", result.message);
                } else {
                    eprintln!("Failed to top up escrow: {}", result.message);
                }
                Ok(())
            }
            DeployCmd::Status { session_id, follow } => {
                loop {
                    let response = client.get_lease_status(session_id).await?;

                    if ctx.json {
                        let mut json = serde_json::json!({
                            "deployment_status": response.deployment_status,
                            "balance_remaining_uakt": response.balance_remaining_uakt,
                        });
                        if let Some(lease) = &response.lease {
                            json["lease"] = serde_json::json!({
                                "owner": lease.owner,
                                "dseq": lease.dseq,
                                "provider": lease.provider,
                                "state": lease.state,
                            });
                        }
                        if !response.endpoints.is_empty() {
                            json["endpoints"] = serde_json::json!(response
                                .endpoints
                                .iter()
                                .map(|e| {
                                    serde_json::json!({
                                        "service": e.service_name,
                                        "uri": e.external_uri,
                                        "port": e.external_port,
                                    })
                                })
                                .collect::<Vec<_>>());
                        }
                        println!("{}", serde_json::to_string_pretty(&json)?);
                    } else {
                        println!("Lease Status: {}", response.deployment_status);
                        if let Some(lease) = &response.lease {
                            println!("  Owner:    {}", lease.owner);
                            println!("  DSEQ:     {}", lease.dseq);
                            println!("  Provider: {}", lease.provider);
                        }
                        println!(
                            "  Balance:  {} uakt remaining",
                            response.balance_remaining_uakt
                        );
                        if !response.endpoints.is_empty() {
                            println!("  Endpoints:");
                            for ep in &response.endpoints {
                                println!(
                                    "    {} -> {}:{}",
                                    ep.service_name, ep.external_uri, ep.external_port
                                );
                            }
                        }
                    }

                    if !*follow {
                        break;
                    }
                    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                }
                Ok(())
            }
            DeployCmd::TrustedProviders => {
                let response = client.list_trusted_providers().await?;

                if ctx.json {
                    let providers: Vec<_> = response
                        .providers
                        .iter()
                        .map(|p| {
                            serde_json::json!({
                                "address": p.address,
                                "label": p.label,
                            })
                        })
                        .collect();
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&serde_json::json!({"providers": providers}))?
                    );
                } else {
                    println!("Trusted Providers ({} total)", response.providers.len());
                    println!("=======================");
                    if response.providers.is_empty() {
                        println!("No trusted providers configured.");
                        println!("\nAdd providers with: ergors-cli deploy add-provider <address>");
                    } else {
                        for p in &response.providers {
                            if p.label.is_empty() {
                                println!("  {}", p.address);
                            } else {
                                println!("  {} ({})", p.address, p.label);
                            }
                        }
                    }
                }
                Ok(())
            }
            DeployCmd::AddProvider { address, label } => {
                let result = client.add_trusted_provider(address, label).await?;

                if ctx.json {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&serde_json::json!({
                            "success": result.success,
                            "message": result.message,
                        }))?
                    );
                } else if result.success {
                    println!("Trusted provider added: {}", address);
                } else {
                    eprintln!("Failed to add provider: {}", result.message);
                }
                Ok(())
            }
            DeployCmd::RemoveProvider { address } => {
                let result = client.remove_trusted_provider(address).await?;

                if ctx.json {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&serde_json::json!({
                            "success": result.success,
                            "message": result.message,
                        }))?
                    );
                } else if result.success {
                    println!("Trusted provider removed: {}", address);
                } else {
                    eprintln!("Failed to remove provider: {}", result.message);
                }
                Ok(())
            }
            DeployCmd::Cert { cmd } => {
                match cmd {
                    CertCmd::Create {
                        key_name,
                        account_index,
                    } => {
                        // Check if keys exist first
                        let keys = client.list_cosmos_keys().await?;
                        if keys.is_empty() {
                            println!(
                                "No Cosmos keys found. A key is required to create certificates."
                            );
                            println!();

                            // Check if stdin is a terminal for interactive prompt
                            if !std::io::stdin().is_terminal() {
                                eprintln!(
                                    "Run 'ergors keys import-mnemonic' to import a key first."
                                );
                                std::process::exit(1);
                            }

                            print!("Import a mnemonic now? [Y/n]: ");
                            std::io::Write::flush(&mut std::io::stdout())?;
                            let mut answer = String::new();
                            std::io::stdin().read_line(&mut answer)?;
                            let answer = answer.trim().to_lowercase();

                            if answer.is_empty() || answer == "y" || answer == "yes" {
                                println!();
                                println!("Importing key as 'default' for Akash (akashnet-2)...");
                                println!();

                                // Get mnemonic (hidden input)
                                let mnemonic = crate::keys::get_mnemonic()?;

                                // Import via gRPC (daemon uses custody password automatically)
                                let import_resp = client
                                    .import_cosmos_key(
                                        &mnemonic,
                                        "Akash Deployment Key", // label
                                        "default",              // key_name
                                        "akashnet-2",           // chain_id
                                        "akash",                // address_prefix
                                        true,                   // make_default
                                        "",                     // password (daemon uses custody)
                                    )
                                    .await?;

                                if !import_resp.success {
                                    eprintln!(
                                        "Failed to import key: {}",
                                        import_resp.error_message
                                    );
                                    std::process::exit(1);
                                }

                                if let Some(key_info) = &import_resp.key {
                                    println!("Key imported successfully!");
                                    println!("  Address: {}", key_info.address);
                                    println!();
                                }
                            } else {
                                println!(
                                    "Run 'ergors keys import-mnemonic' to import a key first."
                                );
                                return Ok(());
                            }
                        }

                        println!("Creating Akash mTLS certificate...");
                        println!("  Key:   {} (index {})", key_name, account_index);
                        println!();

                        let response = client
                            .create_akash_certificate(key_name, *account_index)
                            .await?;

                        if response.success {
                            println!("Certificate created successfully!");
                            println!("  Tx Hash: {}", response.tx_hash);
                            println!("  Serial:  {}", response.serial);
                            println!();
                            println!("The encrypted private key has been stored locally.");
                            println!(
                                "You can now run automated deployments with mTLS authentication."
                            );
                        } else {
                            eprintln!("Failed to create certificate: {}", response.error_message);
                            eprintln!();
                            eprintln!("If a certificate already exists, revoke it first:");
                            eprintln!("  ergors deploy cert revoke --key-name {}", key_name);
                            std::process::exit(1);
                        }
                        Ok(())
                    }
                    CertCmd::Revoke {
                        key_name,
                        account_index,
                        serial,
                    } => {
                        println!("Revoking Akash certificate...");
                        println!("  Key:   {} (index {})", key_name, account_index);
                        if let Some(s) = serial {
                            println!("  Serial: {}", s);
                        } else {
                            println!("  Serial: (first valid certificate)");
                        }

                        let result = client
                            .revoke_akash_certificate(
                                key_name,
                                *account_index,
                                serial.as_deref().unwrap_or(""),
                            )
                            .await?;

                        if result.success {
                            println!();
                            println!("Certificate revoked successfully!");
                            // tx_hash is included in the message
                            if !result.message.is_empty() {
                                println!("  {}", result.message);
                            }
                            println!();
                            println!("The local private key has been deleted.");
                            println!(
                                "Run 'ergors deploy cert create' to create a new certificate."
                            );
                        } else {
                            eprintln!();
                            eprintln!("Failed to revoke certificate: {}", result.message);
                            std::process::exit(1);
                        }
                        Ok(())
                    }
                    CertCmd::Show {
                        key_name,
                        account_index,
                    } => {
                        println!("Querying certificates from chain...");
                        println!("  Key: {} (index {})", key_name, account_index);
                        println!();

                        let response = client
                            .list_akash_certificates(key_name, *account_index, "")
                            .await?;

                        println!("Address: {}", response.address);
                        println!();

                        if response.certificates.is_empty() {
                            println!("No certificates found for this address.");
                            println!();
                            println!(
                                "Run 'ergors deploy cert create' to create a new certificate."
                            );
                        } else {
                            println!("Certificates:");
                            println!("╔════════════════════════════════╦══════════╦═════════════╗");
                            println!("║ Serial                         ║ State    ║ Key Stored? ║");
                            println!("╠════════════════════════════════╬══════════╬═════════════╣");
                            for cert in &response.certificates {
                                let key_status = if cert.has_stored_key { "Yes" } else { "No" };
                                println!(
                                    "║ {:30} ║ {:8} ║ {:11} ║",
                                    truncate_str(&cert.serial, 30),
                                    cert.state,
                                    key_status
                                );
                            }
                            println!("╚════════════════════════════════╩══════════╩═════════════╝");

                            // Check if valid cert has stored key
                            let valid_with_key = response
                                .certificates
                                .iter()
                                .any(|c| c.state == "valid" && c.has_stored_key);

                            if !valid_with_key {
                                println!();
                                let has_valid =
                                    response.certificates.iter().any(|c| c.state == "valid");
                                if has_valid {
                                    println!("⚠️  Warning: Valid certificate exists but private key is not stored locally.");
                                    println!("   mTLS authentication will fail. Consider revoking and creating a new certificate:");
                                    println!("     ergors deploy cert revoke");
                                    println!("     ergors deploy cert create");
                                } else {
                                    println!("No valid certificate found.");
                                    println!("Run 'ergors deploy cert create' to create one.");
                                }
                            }
                        }
                        Ok(())
                    }
                }
            }
            DeployCmd::ProviderInfo { address, refresh } => {
                // Provider info is queried and cached during bid selection.
                println!("Provider info is automatically queried during bid selection.");
                println!();
                println!("Provider: {}", address);
                println!(
                    "Refresh:  {}",
                    if *refresh {
                        "force chain query"
                    } else {
                        "use cache if available"
                    }
                );
                println!();
                println!("Run 'ergors deploy create' to see provider info during deployment.");
                Ok(())
            }
        }
    }
}

/// Truncate a string to max length with ellipsis
fn truncate_str(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else if max_len > 3 {
        format!("{}...", &s[..max_len - 3])
    } else {
        s[..max_len].to_string()
    }
}

fn format_step(step: i32) -> &'static str {
    match step {
        0 => "unspecified",
        1 => "key_selection",
        2 => "balance_check",
        3 => "grant_request",
        4 => "grant_wait",
        5 => "authz_setup",
        6 => "feegrant_setup",
        7 => "sdl_configuration",
        8 => "certificate_setup",
        9 => "deployment_create",
        10 => "bid_wait",
        11 => "provider_selection",
        12 => "lease_create",
        13 => "manifest_send",
        14 => "endpoint_retrieval",
        15 => "endpoint_testing",
        16 => "complete",
        17 => "failed",
        18 => "connectivity_check",
        _ => "unknown",
    }
}

fn format_status(status: i32) -> &'static str {
    match status {
        0 => "unspecified",
        1 => "pending",
        2 => "running",
        3 => "paused",
        4 => "completed",
        5 => "failed",
        6 => "cancelled",
        _ => "unknown",
    }
}

/// Truncate or pad a string to a specific length
fn truncate_or_pad(s: &str, len: usize) -> String {
    if s.len() > len {
        format!("{}...", &s[..len - 3])
    } else {
        format!("{:width$}", s, width = len)
    }
}
