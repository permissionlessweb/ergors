//! Ask commands (RAG, RLM, document ingestion)
//!
//! Unified interface for document management and querying.

use anyhow::Result;
use clap::Subcommand;

use crate::client::ManagementClient;
use super::CliContext;

/// Ask commands (document ingestion and querying)
#[derive(Subcommand)]
pub enum AskCmd {
    /// Ingest a file as document (no embeddings, simple chunking)
    IngestFile {
        /// Path to file
        file: String,
        /// Document URI (defaults to file://<path>)
        #[arg(long)]
        uri: Option<String>,
    },

    /// RAG (Retrieval-Augmented Generation) commands
    Rag {
        #[command(subcommand)]
        command: RagSubCmd,
    },

    /// RLM (Recursive Language Model) commands
    Rlm {
        #[command(subcommand)]
        command: RlmSubCmd,
    },

    /// Show combined status (RAG + RLM)
    Status,

    /// List all ingested sources
    List {
        #[arg(long, default_value = "50")]
        limit: usize,
    },

    /// Delete source by URI
    Delete {
        source_uri: String,
    },
}

#[derive(Subcommand)]
pub enum RagSubCmd {
    /// Ingest a file with embeddings
    Ingest {
        file: String,
        #[arg(long)]
        uri: Option<String>,
        #[arg(long, default_value = "text")]
        doc_type: String,
        #[arg(long)]
        tags: Option<String>,
    },

    /// Query vector database
    Query {
        query: String,
        #[arg(short = 'k', long, default_value = "5")]
        top_k: usize,
        #[arg(long)]
        verify: bool,
    },

    /// Configure embedder endpoint
    Configure {
        #[arg(long)]
        endpoint: String,
        #[arg(long, default_value = "all-MiniLM-L6-v2")]
        model: String,
        #[arg(long, default_value = "384")]
        dimension: usize,
    },
}

#[derive(Subcommand)]
pub enum RlmSubCmd {
    /// Query documents using RLM (agentic code execution)
    Query {
        query: String,
        /// Source URI prefix to filter documents (e.g., "file://", "github:")
        #[arg(long, default_value = "")]
        source_prefix: String,
        /// Max documents to load
        #[arg(long, default_value = "10")]
        limit: usize,
    },

    /// Configure RLM provider selection
    Configure {
        /// Primary provider label (e.g., "qwen-coder")
        #[arg(long)]
        primary: String,
        /// Secondary/fallback provider label
        #[arg(long)]
        secondary: Option<String>,
        /// Max iterations
        #[arg(long)]
        max_iterations: Option<u32>,
        /// Max sub-LLM calls
        #[arg(long)]
        max_sub_calls: Option<u32>,
    },
}

impl AskCmd {
    pub async fn execute(&self, ctx: &CliContext, mut client: ManagementClient) -> Result<()> {
        match self {
            AskCmd::IngestFile { file, uri } => {
                let content = std::fs::read_to_string(file)
                    .map_err(|e| anyhow::anyhow!("Failed to read file '{}': {}", file, e))?;

                let doc_uri = uri.clone().unwrap_or_else(|| format!("file://{}", file));

                let response = client.rag_ingest(&content, &doc_uri, "text", vec![], true).await?;

                if ctx.json {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&serde_json::json!({
                            "success": response.success,
                            "chunk_count": response.chunk_count,
                            "chunk_ids": response.chunk_ids,
                            "message": response.message,
                        }))?
                    );
                } else if response.success {
                    println!("Ingested {} chunks from '{}'", response.chunk_count, doc_uri);
                    if response.chunk_count > 0 {
                        println!("  Chunk IDs:");
                        for id in &response.chunk_ids {
                            println!("    - {}", id);
                        }
                    }
                } else {
                    eprintln!("Failed to ingest: {}", response.message);
                }

                Ok(())
            }

            AskCmd::Rag { command } => {
                command.execute(ctx, client).await
            }

            AskCmd::Rlm { command } => {
                command.execute(ctx, client).await
            }

            AskCmd::Status => {
                // Show both RAG and RLM status
                let rag_status = client.rag_status().await?;
                let rlm_config = client.rlm_get_config().await?;

                if ctx.json {
                    let rlm_json = if rlm_config.configured {
                        let cfg = rlm_config.config.as_ref().unwrap();
                        serde_json::json!({
                            "configured": true,
                            "primary_provider": cfg.primary_provider_label,
                            "secondary_provider": cfg.secondary_provider_label,
                            "max_iterations": cfg.max_iterations,
                            "max_sub_calls": cfg.max_sub_calls,
                        })
                    } else {
                        serde_json::json!({ "configured": false })
                    };

                    println!(
                        "{}",
                        serde_json::to_string_pretty(&serde_json::json!({
                            "rag": {
                                "total_chunks": rag_status.total_chunks,
                                "total_sources": rag_status.total_sources,
                                "embedder_configured": rag_status.embedder_configured,
                                "embedder_endpoint": rag_status.embedder_endpoint,
                                "embedder_model": rag_status.embedder_model,
                                "embedding_dimension": rag_status.embedding_dimension,
                            },
                            "rlm": rlm_json,
                        }))?
                    );
                } else {
                    println!("Ask System Status");
                    println!("=================\n");

                    println!("RAG Vector Database:");
                    println!("  Total Chunks:  {}", rag_status.total_chunks);
                    println!("  Total Sources: {}", rag_status.total_sources);
                    println!("  Embedder: {}",
                        if rag_status.embedder_configured {
                            format!("{} ({})", rag_status.embedder_model, rag_status.embedder_endpoint)
                        } else {
                            "Not configured".to_string()
                        }
                    );

                    println!("\nRLM:");
                    if let Some(cfg) = &rlm_config.config {
                        println!("  Primary:    {}", cfg.primary_provider_label);
                        if !cfg.secondary_provider_label.is_empty() {
                            println!("  Secondary:  {}", cfg.secondary_provider_label);
                        }
                        println!("  Max iters:  {}", cfg.max_iterations);
                        println!("  Max calls:  {}", cfg.max_sub_calls);
                    } else {
                        println!("  Not configured");
                    }
                }
                Ok(())
            }

            AskCmd::List { limit } => {
                let response = client.rag_list_sources(*limit as u32).await?;

                if ctx.json {
                    let sources: Vec<_> = response
                        .sources
                        .iter()
                        .map(|s| {
                            serde_json::json!({
                                "uri": s.uri,
                                "chunk_count": s.chunk_count,
                                "doc_type": s.doc_type,
                                "ingested_at": s.ingested_at,
                            })
                        })
                        .collect();
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&serde_json::json!({
                            "sources": sources,
                            "total_count": response.total_count,
                        }))?
                    );
                } else {
                    println!("Ingested Sources ({} total)", response.total_count);
                    println!("==============================");

                    if response.sources.is_empty() {
                        println!("No sources ingested yet.");
                        println!("\nUse 'ergors ask rag ingest <file>' to add documents.");
                    } else {
                        for src in &response.sources {
                            println!(
                                "  {} | {} chunks | {} | {}",
                                src.uri,
                                src.chunk_count,
                                src.doc_type,
                                src.ingested_at
                            );
                        }
                    }
                }
                Ok(())
            }

            AskCmd::Delete { source_uri } => {
                let result = client.rag_delete(source_uri).await?;

                if ctx.json {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&serde_json::json!({
                            "success": result.success,
                            "message": result.message,
                        }))?
                    );
                } else if result.success {
                    println!("Deleted chunks from source: {}", source_uri);
                } else {
                    eprintln!("Failed to delete: {}", result.message);
                }
                Ok(())
            }
        }
    }
}

impl RagSubCmd {
    async fn execute(&self, ctx: &CliContext, mut client: ManagementClient) -> Result<()> {
        match self {
            RagSubCmd::Ingest { file, uri, doc_type, tags } => {
                let content = std::fs::read_to_string(file)
                    .map_err(|e| anyhow::anyhow!("Failed to read file '{}': {}", file, e))?;

                let doc_uri = uri.clone().unwrap_or_else(|| file.clone());
                let tag_list: Vec<String> = tags
                    .as_deref()
                    .map(|t| t.split(',').map(|s| s.trim().to_string()).collect())
                    .unwrap_or_default();

                let response = client.rag_ingest(&content, &doc_uri, doc_type, tag_list, false).await?;

                if ctx.json {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&serde_json::json!({
                            "success": response.success,
                            "chunk_count": response.chunk_count,
                            "chunk_ids": response.chunk_ids,
                            "message": response.message,
                        }))?
                    );
                } else if response.success {
                    println!("Ingested {} chunks from '{}'", response.chunk_count, doc_uri);
                    if response.chunk_count > 0 {
                        println!("  Chunk IDs:");
                        for id in &response.chunk_ids {
                            println!("    - {}", id);
                        }
                    }
                } else {
                    eprintln!("Failed to ingest: {}", response.message);
                }
                Ok(())
            }

            RagSubCmd::Query { query, top_k, verify } => {
                let response = client.rag_query(query, *top_k, *verify).await?;

                if ctx.json {
                    let results: Vec<_> = response
                        .results
                        .iter()
                        .map(|r| {
                            serde_json::json!({
                                "chunk_id": r.chunk_id,
                                "similarity": r.similarity,
                                "content_preview": r.content_preview,
                                "source_uri": r.source_uri,
                            })
                        })
                        .collect();
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&serde_json::json!({
                            "query": query,
                            "results": results,
                            "verified": response.verified,
                        }))?
                    );
                } else {
                    println!("Query: {}", query);
                    println!("Results ({}):", response.results.len());
                    println!("================");

                    if response.results.is_empty() {
                        println!("No results found.");
                    } else {
                        for (i, r) in response.results.iter().enumerate() {
                            println!(
                                "\n[{}] Similarity: {:.4} | Source: {}",
                                i + 1,
                                r.similarity,
                                r.source_uri
                            );
                            println!("    {}", r.content_preview);
                        }
                    }

                    if response.verified {
                        println!("\n(Results verified with provenance)");
                    }
                }
                Ok(())
            }

            RagSubCmd::Configure { endpoint, model, dimension } => {
                let result = client.rag_configure(endpoint, model, *dimension as u32).await?;

                if ctx.json {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&serde_json::json!({
                            "success": result.success,
                            "message": result.message,
                        }))?
                    );
                } else if result.success {
                    println!("RAG embedder configured:");
                    println!("  Endpoint:  {}", endpoint);
                    println!("  Model:     {}", model);
                    println!("  Dimension: {}", dimension);
                } else {
                    eprintln!("Failed to configure: {}", result.message);
                }
                Ok(())
            }
        }
    }
}

impl RlmSubCmd {
    async fn execute(&self, ctx: &CliContext, mut client: ManagementClient) -> Result<()> {
        match self {
            RlmSubCmd::Query { query, source_prefix, limit } => {
                let response = client.rlm_query(query, source_prefix, *limit).await?;

                if ctx.json {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&serde_json::json!({
                            "answer": response.answer,
                            "sources": response.source_uris,
                            "iterations": response.iterations,
                            "sub_llm_calls": response.sub_llm_calls,
                            "latency_ms": response.latency_ms,
                        }))?
                    );
                } else {
                    println!("RLM Query Result");
                    println!("================\n");
                    println!("{}\n", response.answer);
                    println!("Sources used:");
                    for src in &response.source_uris {
                        println!("  - {}", src);
                    }
                    println!("\nMetrics:");
                    println!("  Iterations: {}", response.iterations);
                    println!("  Sub-LLM calls: {}", response.sub_llm_calls);
                    println!("  Latency: {}ms", response.latency_ms);
                }
                Ok(())
            }

            RlmSubCmd::Configure { primary, secondary, max_iterations, max_sub_calls } => {
                let result = client.rlm_configure(
                    primary,
                    secondary.as_deref(),
                    *max_iterations,
                    *max_sub_calls,
                ).await?;

                if ctx.json {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&serde_json::json!({
                            "success": result.success,
                            "message": result.message,
                        }))?
                    );
                } else if result.success {
                    println!("RLM configured:");
                    println!("  Primary provider:   {}", primary);
                    if let Some(sec) = secondary {
                        println!("  Secondary provider: {}", sec);
                    }
                    if let Some(max_iter) = max_iterations {
                        println!("  Max iterations:     {}", max_iter);
                    }
                    if let Some(max_calls) = max_sub_calls {
                        println!("  Max sub-LLM calls:  {}", max_calls);
                    }
                } else {
                    eprintln!("Failed to configure: {}", result.message);
                }
                Ok(())
            }
        }
    }
}
