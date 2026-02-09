use anyhow::Result;
use poise::Context as PoiseContext;

use crate::gateway::discord::log_rag_audit;
use ho_std::types::ergors::gateway::v1::GuildRagConfig;

const MAX_FILE_SIZE: usize = 1_000_000; // 1MB - matches existing constant

/// Ingest a GitHub repository into guild RAG storage
pub async fn ingest_github_repo(
    ctx: &PoiseContext<'_, crate::gateway::discord::DiscordData, anyhow::Error>,
    url: &str,
    label: Option<String>,
    doc_type: Option<String>,
) -> Result<(), anyhow::Error> {
    #[cfg(not(feature = "github-ingest"))]
    {
        ctx.say("GitHub ingestion not enabled. Rebuild with --features github-ingest")
            .await?;
        return Ok(());
    }

    #[cfg(feature = "github-ingest")]
    {
        use githem_core::{FilterPreset, IngestOptions, Ingester};

        let guild_id = ctx.guild_id().unwrap().to_string();
        let user_id = ctx.author().id.to_string();

        // Parse GitHub URL (githem validates SSRF internally)
        let parsed = match githem_core::parse_github_url(url) {
            Some(p) => p,
            None => {
                ctx.say("Invalid GitHub URL format").await?;
                return Ok(());
            }
        };

        // Determine filter preset based on doc_type hint or default to Standard
        let preset = match doc_type.as_deref() {
            Some("documentation") | Some("docs") => FilterPreset::Standard, // Includes docs + code
            Some("code") => FilterPreset::CodeOnly,
            Some("minimal") => FilterPreset::Minimal,
            _ => FilterPreset::Standard, // Default to standard filtering
        };

        // Configure ingestion options
        let options = IngestOptions {
            filter_preset: Some(preset),
            max_file_size: MAX_FILE_SIZE,
            apply_default_filters: false, // Use preset only
            ..Default::default()
        };

        // Clone and ingest repository
        let repo_full_name = format!("{}/{}", parsed.owner, parsed.repo);
        ctx.say(format!("Cloning repository: {}", repo_full_name))
            .await?;

        let ingester = match Ingester::from_url_cached(url, options) {
            Ok(i) => i,
            Err(e) => {
                ctx.say(format!("Failed to clone repository: {}", e))
                    .await?;
                log_rag_audit(
                    &ctx.data().storage,
                    &guild_id,
                    &user_id,
                    "github_ingest",
                    url,
                    false,
                    &e.to_string(),
                )
                .await;
                return Ok(());
            }
        };

        // Capture output from githem
        let mut output = Vec::new();
        if let Err(e) = ingester.ingest(&mut output) {
            ctx.say(format!("Failed to ingest repository: {}", e))
                .await?;
            log_rag_audit(
                &ctx.data().storage,
                &guild_id,
                &user_id,
                "github_ingest",
                url,
                false,
                &e.to_string(),
            )
            .await;
            return Ok(());
        }

        let output_str = String::from_utf8_lossy(&output);

        // Parse githem output into individual files
        let files = parse_githem_output(&output_str);

        if files.is_empty() {
            ctx.say("No files found in repository after filtering")
                .await?;
            return Ok(());
        }

        ctx.say(format!("Processing {} files...", files.len()))
            .await?;

        // Check RAG config
        let rag_config = match ctx.data().storage.get_rag_config().await {
            Ok(Some(config)) => config,
            Ok(None) => {
                ctx.say("RAG not configured. Ask the bot admin to run `ergors rag configure`.")
                    .await?;
                return Ok(());
            }
            Err(e) => {
                ctx.say(format!("Error checking RAG config: {}", e))
                    .await?;
                return Ok(());
            }
        };

        // Create RAG instance
        let rag = match crate::proxy::rag::new_remote_with_client(
            &ctx.data().storage,
            ctx.data().rag_client.clone(),
            &rag_config.endpoint,
            &rag_config.model,
            rag_config.dimension as usize,
        ) {
            Ok(r) => r,
            Err(e) => {
                ctx.say(format!("Failed to initialize RAG: {}", e))
                    .await?;
                return Ok(());
            }
        };

        // Ingest each file individually for better retrieval granularity
        let repo_name = format!("{}/{}", parsed.owner, parsed.repo);
        let mut total_chunks = 0;
        let file_count = files.len();

        for (file_path, content) in files {
            let source_uri = format!("github:{}/{}", repo_name, file_path);
            let detected_type = detect_file_type(&file_path);

            let doc = ergors_rag::Document {
                content,
                uri: source_uri,
                doc_type: detected_type,
                tags: vec![
                    format!("guild:{}", guild_id),
                    format!("repo:{}", repo_name),
                    format!("user:{}", user_id),
                ],
            };

            match rag.ingest(doc, None).await {
                Ok(chunk_ids) => total_chunks += chunk_ids.len(),
                Err(e) => {
                    tracing::warn!("Failed to ingest {}: {}", file_path, e);
                    // Continue with other files
                }
            }
        }

        // Update guild stats
        let mut guild_config: GuildRagConfig = ctx
            .data()
            .storage
            .get_guild_rag_config(&guild_id)
            .await?
            .unwrap_or_else(|| GuildRagConfig {
                guild_id: guild_id.clone(),
                auto_context_enabled: true,
                max_context_chunks: 3,
                min_similarity: 0.5,
                ..Default::default()
            });

        guild_config.total_documents += file_count as u32;
        guild_config.total_chunks += total_chunks as u32;
        guild_config.last_ingestion_at = chrono::Utc::now().timestamp();
        ctx.data()
            .storage
            .put_guild_rag_config(&guild_config)
            .await?;

        // Audit log
        log_rag_audit(
            &ctx.data().storage,
            &guild_id,
            &user_id,
            "github_ingest",
            url,
            true,
            &format!("{} files, {} chunks", files.len(), total_chunks),
        )
        .await;

        let display_name = label.unwrap_or_else(|| repo_name.to_string());
        ctx.say(format!(
            "✓ Ingested **{}** ({} files, {} chunks)",
            display_name,
            files.len(),
            total_chunks
        ))
        .await?;

        Ok(())
    }
}

/// Parse githem output format into individual files
/// Format: === path/to/file.rs ===\n<content>\n\n
fn parse_githem_output(output: &str) -> Vec<(String, String)> {
    let mut files = Vec::new();
    let mut current_path: Option<String> = None;
    let mut current_content = String::new();

    for line in output.lines() {
        if line.starts_with("=== ") && line.ends_with(" ===") {
            // Save previous file if exists
            if let Some(path) = current_path.take() {
                if !current_content.trim().is_empty() {
                    files.push((path, current_content.trim().to_string()));
                }
                current_content.clear();
            }

            // Extract new file path
            let path = line
                .trim_start_matches("=== ")
                .trim_end_matches(" ===")
                .to_string();
            current_path = Some(path);
        } else if current_path.is_some() {
            current_content.push_str(line);
            current_content.push('\n');
        }
    }

    // Save last file
    if let Some(path) = current_path {
        if !current_content.trim().is_empty() {
            files.push((path, current_content.trim().to_string()));
        }
    }

    files
}

/// Detect document type from file extension
fn detect_file_type(path: &str) -> String {
    let ext = path.rsplit('.').next().unwrap_or("");
    match ext {
        "md" | "mdx" => "markdown".to_string(),
        "rs" | "py" | "js" | "ts" | "go" | "java" | "c" | "cpp" | "h" | "hpp" => {
            "code".to_string()
        }
        "json" | "yaml" | "yml" | "toml" => "config".to_string(),
        "txt" => "text".to_string(),
        _ => "text".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_githem_output() {
        let input = r#"=== src/main.rs ===
fn main() {
    println!("hello");
}

=== README.md ===
# Project

Description here.

"#;

        let files = parse_githem_output(input);
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].0, "src/main.rs");
        assert!(files[0].1.contains("fn main()"));
        assert_eq!(files[1].0, "README.md");
        assert!(files[1].1.contains("# Project"));
    }

    #[test]
    fn test_detect_file_type() {
        assert_eq!(detect_file_type("README.md"), "markdown");
        assert_eq!(detect_file_type("src/main.rs"), "code");
        assert_eq!(detect_file_type("config.yaml"), "config");
        assert_eq!(detect_file_type("notes.txt"), "text");
    }
}
