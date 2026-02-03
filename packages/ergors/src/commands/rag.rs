//! RAG (Retrieval-Augmented Generation) CLI commands
//!
//! Commands for managing the vector database and performing semantic search.

use anyhow::Result;
use clap::Subcommand;

use crate::client::ManagementClient;
use super::CliContext;

/// RAG vector database commands
#[derive(Subcommand)]
pub enum RagCmd {
    /// Ingest a file into the vector database
    Ingest {
        /// Path to file to ingest
        file: String,
        /// Document URI (defaults to file path)
        #[arg(long)]
        uri: Option<String>,
        /// Document type (e.g., markdown, text, code)
        #[arg(long, default_value = "text")]
        doc_type: String,
        /// Tags for the document (comma-separated)
        #[arg(long)]
        tags: Option<String>,
    },
    /// Query the vector database
    Query {
        /// Search query text
        query: String,
        /// Number of results to return
        #[arg(short = 'k', long, default_value = "5")]
        top_k: usize,
        /// Enable verification (slower but includes provenance)
        #[arg(long)]
        verify: bool,
    },
    /// Show vector database status
    Status,
    /// Delete chunks by source URI
    Delete {
        /// Source URI to delete
        source_uri: String,
    },
    /// List ingested sources
    List {
        /// Maximum sources to show
        #[arg(long, default_value = "50")]
        limit: usize,
    },
    /// Configure the embedding endpoint
    Configure {
        /// Embedding service endpoint URL (e.g., from Akash deployment)
        #[arg(long)]
        endpoint: String,
        /// Model name
        #[arg(long, default_value = "all-MiniLM-L6-v2")]
        model: String,
        /// Embedding dimension
        #[arg(long, default_value = "384")]
        dimension: usize,
    },
}

impl RagCmd {
    pub async fn execute(&self, ctx: &CliContext, mut client: ManagementClient) -> Result<()> {
        match self {
            RagCmd::Ingest { file, uri, doc_type, tags } => {
                // Read file content
                let content = std::fs::read_to_string(file)
                    .map_err(|e| anyhow::anyhow!("Failed to read file '{}': {}", file, e))?;

                let doc_uri = uri.clone().unwrap_or_else(|| file.clone());
                let tag_list: Vec<String> = tags
                    .as_deref()
                    .map(|t| t.split(',').map(|s| s.trim().to_string()).collect())
                    .unwrap_or_default();

                let response = client.rag_ingest(&content, &doc_uri, doc_type, tag_list).await?;

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
            RagCmd::Query { query, top_k, verify } => {
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
            RagCmd::Status => {
                let response = client.rag_status().await?;

                if ctx.json {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&serde_json::json!({
                            "total_chunks": response.total_chunks,
                            "total_sources": response.total_sources,
                            "embedder_configured": response.embedder_configured,
                            "embedder_endpoint": response.embedder_endpoint,
                            "embedder_model": response.embedder_model,
                            "embedding_dimension": response.embedding_dimension,
                        }))?
                    );
                } else {
                    println!("RAG Vector Database Status");
                    println!("==========================");
                    println!("Total Chunks:  {}", response.total_chunks);
                    println!("Total Sources: {}", response.total_sources);
                    println!();
                    println!("Embedder Configuration:");
                    if response.embedder_configured {
                        println!("  Endpoint:  {}", response.embedder_endpoint);
                        println!("  Model:     {}", response.embedder_model);
                        println!("  Dimension: {}", response.embedding_dimension);
                    } else {
                        println!("  Not configured");
                        println!("  Use 'ergors rag configure --endpoint <url>' to set up");
                    }
                }
                Ok(())
            }
            RagCmd::Delete { source_uri } => {
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
            RagCmd::List { limit } => {
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
                        println!("\nUse 'ergors rag ingest <file>' to add documents.");
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
            RagCmd::Configure { endpoint, model, dimension } => {
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
