//! Akash deployment management CLI commands
//!
//! Commands for managing Akash network deployments through the engine.

use anyhow::Result;
use clap::Subcommand;
use std::collections::HashMap;
use std::io::IsTerminal;

use crate::client::ManagementClient;
use super::CliContext;

/// Akash deployment management commands
#[derive(Subcommand)]
pub enum DeployCmd {
    /// Create a new Akash deployment (automated by default)
    Create {
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
    },
    /// Run automated deployment workflow on existing session
    Run {
        /// Session ID
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
        /// Session ID
        session_id: String,
    },
    /// Query bids for a deployment
    Bids {
        /// Session ID
        session_id: String,
    },
    /// Select a provider for the deployment
    Select {
        /// Session ID
        session_id: String,
        /// Provider address
        #[arg(long)]
        provider: String,
        /// Bid price in uakt
        #[arg(long, default_value = "0")]
        price: u64,
    },
    /// Cancel a deployment workflow
    Cancel {
        /// Session ID
        session_id: String,
    },
    /// Set discovered endpoints for a deployment workflow
    SetEndpoints {
        /// Session ID
        session_id: String,
        /// Endpoints as key=value pairs (service_name=url)
        #[arg(long, value_parser = parse_key_val)]
        endpoint: Vec<(String, String)>,
    },
    /// Configure proxy routing to discovered services
    ConfigureProxy {
        /// OpenAI-compatible API base URL
        #[arg(long)]
        openai_url: Option<String>,
        /// Anthropic-compatible API base URL
        #[arg(long)]
        anthropic_url: Option<String>,
        /// Ollama-compatible API base URL
        #[arg(long)]
        ollama_url: Option<String>,
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
        /// Session ID
        session_id: String,
    },
    /// Get lease status
    Status {
        /// Session ID
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
            } => {
                // Resolve SDL content
                let content = match (sdl, sdl_content) {
                    (Some(path), _) => std::fs::read_to_string(path)
                        .map_err(|e| anyhow::anyhow!("Failed to read SDL file '{}': {}", path, e))?,
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
                    )
                    .await?;

                if response.success {
                    let session_id = response.workflow.as_ref().map(|wf| wf.session_id.clone()).unwrap_or_default();

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
                                Ok(resp) => resp.providers.iter().map(|p| p.address.clone()).collect(),
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
                                json["current_step"] = serde_json::json!(format_step(wf.current_step));
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

                let response = client
                    .list_akash_deployments(status_filter, *limit)
                    .await?;

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
                        for bid in &response.bids {
                            println!(
                                "  {} | {} uakt/block",
                                bid.provider_address, bid.price_uakt
                            );
                        }
                    }
                }
                Ok(())
            }
            DeployCmd::Select {
                session_id,
                provider,
                price,
            } => {
                let response = client
                    .select_akash_provider(session_id, provider, *price)
                    .await?;

                if ctx.json {
                    let mut json = serde_json::json!({
                        "success": response.success,
                        "provider": provider,
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
                    println!("Provider selected: {}", provider);
                    if let Some(wf) = &response.workflow {
                        println!("  Step: {}", format_step(wf.current_step));
                        println!("  Status: {}", format_status(wf.status));
                    }
                } else {
                    eprintln!("Failed to select provider: {}", response.error_message);
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
                let response = client
                    .set_workflow_endpoints(session_id, endpoints)
                    .await?;

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
            DeployCmd::ConfigureProxy {
                openai_url,
                anthropic_url,
                ollama_url,
                route,
            } => {
                let model_routes: HashMap<String, String> = route.iter().cloned().collect();
                let result = client
                    .configure_proxy_routes(
                        openai_url.as_deref().unwrap_or(""),
                        anthropic_url.as_deref().unwrap_or(""),
                        ollama_url.as_deref().unwrap_or(""),
                        model_routes,
                    )
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
                    println!("Proxy routes configured");
                    if let Some(url) = openai_url {
                        println!("  OpenAI:    {}", url);
                    }
                    if let Some(url) = anthropic_url {
                        println!("  Anthropic: {}", url);
                    }
                    if let Some(url) = ollama_url {
                        println!("  Ollama:    {}", url);
                    }
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
                    .list_grant_requests(
                        granter.as_deref(),
                        grantee.as_deref(),
                        status.as_deref(),
                    )
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
                    println!("{}", serde_json::to_string_pretty(&serde_json::json!({"requests": requests}))?);
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
                            json["endpoints"] = serde_json::json!(response.endpoints.iter().map(|e| {
                                serde_json::json!({
                                    "service": e.service_name,
                                    "uri": e.external_uri,
                                    "port": e.external_port,
                                })
                            }).collect::<Vec<_>>());
                        }
                        println!("{}", serde_json::to_string_pretty(&json)?);
                    } else {
                        println!("Lease Status: {}", response.deployment_status);
                        if let Some(lease) = &response.lease {
                            println!("  Owner:    {}", lease.owner);
                            println!("  DSEQ:     {}", lease.dseq);
                            println!("  Provider: {}", lease.provider);
                        }
                        println!("  Balance:  {} uakt remaining", response.balance_remaining_uakt);
                        if !response.endpoints.is_empty() {
                            println!("  Endpoints:");
                            for ep in &response.endpoints {
                                println!("    {} -> {}:{}", ep.service_name, ep.external_uri, ep.external_port);
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
                    println!("{}", serde_json::to_string_pretty(&serde_json::json!({"providers": providers}))?);
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
        }
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
