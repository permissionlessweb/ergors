//! Document storage CLI commands.
//!
//! Provides commands for ingesting, retrieving, listing, and deleting documents
//! without RAG-specific features.

use anyhow::{anyhow, Context, Result};
use clap::Subcommand;
use ho_std::custody::PasswordEncryptedCustody;
use ho_std::traits::NodeIdentityCustody;
use std::io::{self, IsTerminal, Read, Write};
use std::path::PathBuf;

use crate::client::ManagementClient;

use super::CliContext;

#[derive(Subcommand)]
pub enum DocumentCmd {
    /// Ingest a document into storage
    Ingest {
        /// File path to ingest (or use --stdin)
        file: Option<PathBuf>,

        /// Read content from stdin
        #[arg(long)]
        stdin: bool,

        /// Document name (required for stdin)
        #[arg(long)]
        name: Option<String>,

        /// GitHub repository URL
        #[arg(long)]
        github: Option<String>,
    },

    /// Retrieve a document by ID
    Get {
        /// Document ID (hex hash)
        document_id: String,

        /// Output to file instead of stdout
        #[arg(short, long)]
        output: Option<PathBuf>,
    },

    /// List all stored documents
    List {
        /// Maximum number of documents to return
        #[arg(short, long)]
        limit: Option<usize>,

        /// Number of documents to skip
        #[arg(short, long)]
        offset: Option<usize>,

        /// Output format: table (default), json
        #[arg(long, default_value = "table")]
        format: String,
    },

    /// Delete a document by ID
    Delete {
        /// Document ID (hex hash)
        document_id: String,

        /// Skip confirmation prompt
        #[arg(short, long)]
        yes: bool,
    },

    /// Verify document integrity
    Verify {
        /// Document ID (hex hash)
        document_id: String,
    },
}

impl DocumentCmd {
    pub async fn execute(&self, ctx: &CliContext, client: Result<ManagementClient>) -> Result<()> {
        let mut client = client.context("Failed to connect to engine")?;

        match self {
            DocumentCmd::Ingest {
                file,
                stdin,
                name,
                github,
            } => {
                // Determine source and content
                let (content, doc_name, source) = if let Some(repo_url) = github {
                    // GitHub ingestion
                    Self::ingest_github(&mut client, repo_url).await?
                } else if *stdin {
                    // Stdin ingestion
                    let name = name
                        .as_deref()
                        .ok_or_else(|| anyhow::anyhow!("--name required when using --stdin"))?;
                    let mut content = Vec::new();
                    io::stdin()
                        .read_to_end(&mut content)
                        .context("Failed to read from stdin")?;
                    (content, name.to_string(), "stdin".to_string())
                } else if let Some(file_path) = file {
                    // File ingestion
                    let content = std::fs::read(file_path)
                        .with_context(|| format!("Failed to read file: {}", file_path.display()))?;
                    let name = file_path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("unknown")
                        .to_string();
                    let source = file_path.display().to_string();
                    (content, name, source)
                } else {
                    return Err(anyhow::anyhow!(
                        "Must specify file path, --stdin, or --github"
                    ));
                };

                // Ingest document
                let doc_id = client.ingest_document(content, doc_name, source).await?;
                println!("Document ingested: {}", doc_id);
                Ok(())
            }

            DocumentCmd::Get {
                document_id,
                output,
            } => {
                let (content, _metadata) = client.retrieve_document(document_id).await?;

                if let Some(output_path) = output {
                    std::fs::write(output_path, &content)
                        .context("Failed to write output file")?;
                    println!("Document written to: {}", output_path.display());
                } else {
                    // Write to stdout
                    io::stdout()
                        .write_all(&content)
                        .context("Failed to write to stdout")?;
                }

                Ok(())
            }

            DocumentCmd::List {
                limit,
                offset,
                format,
            } => {
                let documents = client.list_documents(*limit, *offset).await?;

                if format == "json" {
                    println!("{}", serde_json::to_string_pretty(&documents)?);
                } else {
                    // Table format
                    if documents.is_empty() {
                        println!("No documents found");
                        return Ok(());
                    }

                    println!(
                        "{:<66} {:<30} {:<40} {}",
                        "Document ID", "Name", "Source", "Size"
                    );
                    println!("{}", "-".repeat(160));

                    for (doc_id, metadata) in documents {
                        let source_str = metadata.source.as_str();
                        let source_display = if source_str.len() > 37 {
                            format!("{}...", &source_str[..37])
                        } else {
                            source_str
                        };

                        let name_display = if metadata.name.len() > 27 {
                            format!("{}...", &metadata.name[..27])
                        } else {
                            metadata.name.clone()
                        };

                        println!(
                            "{:<66} {:<30} {:<40} {}",
                            doc_id,
                            name_display,
                            source_display,
                            Self::format_size(metadata.size)
                        );
                    }
                }

                Ok(())
            }

            DocumentCmd::Delete { document_id, yes } => {
                // Confirmation prompt unless --yes
                if !yes {
                    print!("Delete document {}? [y/N] ", document_id);
                    io::stdout().flush()?;

                    let mut input = String::new();
                    io::stdin().read_line(&mut input)?;

                    if !input.trim().eq_ignore_ascii_case("y") {
                        println!("Cancelled");
                        return Ok(());
                    }
                }

                // Verify custody password before deletion
                Self::verify_custody_access(&ctx.home).await?;

                client.delete_document(document_id).await?;
                println!("Document deleted: {}", document_id);
                Ok(())
            }

            DocumentCmd::Verify { document_id } => {
                match client.retrieve_document(document_id).await {
                    Ok((content, metadata)) => {
                        // Verify hash
                        if let Err(e) = metadata.verify_content(&content) {
                            println!("CORRUPT: {}", e);
                            std::process::exit(1);
                        } else {
                            println!("OK: Document integrity verified");
                        }
                    }
                    Err(e) => {
                        println!("ERROR: {}", e);
                        std::process::exit(1);
                    }
                }

                Ok(())
            }
        }
    }

    /// Ingest GitHub repository using githem.
    async fn ingest_github(
        _client: &mut ManagementClient,
        repo_url: &str,
    ) -> Result<(Vec<u8>, String, String)> {
        #[cfg(not(feature = "github-ingest"))]
        {
            return Err(anyhow::anyhow!(
                "GitHub ingestion not enabled. Rebuild with --features github-ingest. Repository: {}",
                repo_url
            ));
        }

        #[cfg(feature = "github-ingest")]
        {
            use githem_core::{FilterPreset, IngestOptions, Ingester};

            // Parse GitHub URL (githem validates SSRF internally)
            let parsed = githem_core::parse_github_url(repo_url)
                .ok_or_else(|| anyhow::anyhow!("Invalid GitHub URL format: {}", repo_url))?;

            // Configure ingestion options with Standard preset (docs + code)
            let options = IngestOptions {
                filter_preset: Some(FilterPreset::Standard),
                max_file_size: 1_000_000, // 1MB per file
                apply_default_filters: false,
                ..Default::default()
            };

            // Clone and ingest repository
            let repo_full_name = format!("{}/{}", parsed.owner, parsed.repo);
            tracing::info!("Cloning repository: {}", repo_full_name);

            let ingester = Ingester::from_url_cached(repo_url, options)
                .with_context(|| format!("Failed to clone repository: {}", repo_url))?;

            // Capture output from githem (curated content in githem format)
            let mut output = Vec::new();
            ingester
                .ingest(&mut output)
                .context("Failed to ingest repository content")?;

            if output.is_empty() {
                return Err(anyhow::anyhow!(
                    "No files found in repository after filtering"
                ));
            }

            // Use repo name as document name
            let doc_name = repo_full_name.clone();

            // Use GitHub URL as source
            let source = repo_url.to_string();

            tracing::info!(
                "Ingested repository: {} ({} bytes)",
                repo_full_name,
                output.len()
            );

            Ok((output, doc_name, source))
        }
    }

    /// Format byte size as human-readable string.
    fn format_size(bytes: usize) -> String {
        const KB: usize = 1024;
        const MB: usize = KB * 1024;
        const GB: usize = MB * 1024;

        if bytes >= GB {
            format!("{:.2} GB", bytes as f64 / GB as f64)
        } else if bytes >= MB {
            format!("{:.2} MB", bytes as f64 / MB as f64)
        } else if bytes >= KB {
            format!("{:.2} KB", bytes as f64 / KB as f64)
        } else {
            format!("{} B", bytes)
        }
    }

    /// Verify custody access by prompting for password and unlocking identity.
    ///
    /// This ensures only users with custody access can perform destructive operations.
    /// Password never leaves the local machine.
    async fn verify_custody_access(home: &camino::Utf8Path) -> Result<()> {
        let identity_path = home.join("node_identity.enc");

        if !identity_path.exists() {
            return Err(anyhow!(
                "No node identity found at {}. Run 'ergors init' first.",
                identity_path
            ));
        }

        // Get password from env var or prompt
        let password = if let Ok(pw) = std::env::var("ERGORS_CUSTODY_PASSWORD") {
            if pw.is_empty() {
                return Err(anyhow!("ERGORS_CUSTODY_PASSWORD is set but empty"));
            }
            pw
        } else if std::io::stdin().is_terminal() {
            rpassword::prompt_password("Enter custody password: ")
                .context("Failed to read password")?
        } else {
            return Err(anyhow!(
                "ERGORS_CUSTODY_PASSWORD not set and no terminal available for interactive prompt"
            ));
        };

        // Load and verify custody (already in async context, no need for new runtime)
        let mut custody = PasswordEncryptedCustody::new(&identity_path);

        custody
            .unlock(&password)
            .await
            .context("Invalid custody password")?;

        Ok(())
    }
}
