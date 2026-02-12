use anyhow::Result;
use poise::Context as PoiseContext;

use crate::gateway::discord::log_rag_audit;
use ho_std::types::ergors::gateway::v1::GuildRagConfig;

const MAX_FILE_SIZE: usize = 1_000_000; // 1MB - matches existing constant

/// File index for navigating within a consolidated githem document
#[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq)]
struct FileIndex {
    repo: String,
    file_count: usize,
    files: Vec<FileEntry>,
}

/// Single file entry with char offset into the consolidated document
#[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq)]
struct FileEntry {
    path: String,
    offset: usize,  // char offset within consolidated doc
    length: usize,  // char length of file content (excludes delimiter)
}

/// Build a file index mapping file paths to char offsets in the consolidated output.
/// Uses the already-parsed `files` vec to know which paths to look for, then scans
/// the consolidated text for `=== path ===\n` markers to compute content offsets.
fn build_file_index(repo: &str, consolidated: &str, files: &[(String, String)]) -> FileIndex {
    let mut entries = Vec::with_capacity(files.len());

    for (path, _) in files {
        let marker = format!("=== {} ===\n", path);
        if let Some(marker_pos) = consolidated.find(&marker) {
            let content_start = marker_pos + marker.len();
            // Content extends until the next marker or end of string
            let content_end = if let Some(next_marker) = consolidated[content_start..].find("\n=== ") {
                // Trim trailing newlines before next marker
                let raw_end = content_start + next_marker;
                consolidated[content_start..raw_end].trim_end().len() + content_start
            } else {
                // Last file — trim trailing whitespace
                consolidated[content_start..].trim_end().len() + content_start
            };
            entries.push(FileEntry {
                path: path.clone(),
                offset: content_start,
                length: content_end - content_start,
            });
        }
    }

    FileIndex {
        repo: repo.to_string(),
        file_count: entries.len(),
        files: entries,
    }
}

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
            Some("documentation") | Some("docs") => FilterPreset::Standard,
            Some("code") => FilterPreset::CodeOnly,
            Some("minimal") => FilterPreset::Minimal,
            _ => FilterPreset::Standard,
        };

        // Configure ingestion options
        let options = IngestOptions {
            filter_preset: Some(preset),
            max_file_size: MAX_FILE_SIZE,
            apply_default_filters: false,
            ..Default::default()
        };

        // Clone and ingest repository
        let repo_name = format!("{}/{}", parsed.owner, parsed.repo);
        ctx.say(format!("Cloning repository: {}", repo_name))
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

        // Capture consolidated output from githem
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

        // Parse for file count reporting + index building
        let files = parse_githem_output(&output_str);
        if files.is_empty() {
            ctx.say("No files found in repository after filtering")
                .await?;
            return Ok(());
        }

        ctx.say(format!("Processing {} files...", files.len()))
            .await?;

        let source_uri = format!("discord:guild_{}/github:{}", guild_id, repo_name);
        let total_bytes = output.len();
        let file_count = files.len();

        let mut delta = cnidarium::StateDelta::new(ctx.data().storage.cs.latest_snapshot());

        // 1. Store consolidated document (single doc for entire repo)
        ho_std::document::DocumentStorage::store_document(
            &mut delta,
            &output,
            &repo_name,
            &source_uri,
        )
        .await
        .map_err(|e| anyhow::anyhow!("{}", e))?;

        // 2. Store file index (maps file paths to char offsets)
        let index = build_file_index(&repo_name, &output_str, &files);
        let index_bytes = serde_json::to_vec(&index).unwrap();
        ho_std::document::DocumentStorage::store_document(
            &mut delta,
            &index_bytes,
            &format!("{}/.index", repo_name),
            &format!("{}/.index", source_uri),
        )
        .await
        .map_err(|e| anyhow::anyhow!("{}", e))?;

        ctx.data().storage.commit_delta(delta).await?;

        // Update guild stats (1 logical document per repo)
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

        guild_config.total_documents += 1;
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
            &format!("{} files, {} bytes", file_count, total_bytes),
        )
        .await;

        let display_name = label.unwrap_or_else(|| repo_name.to_string());
        let size_display = if total_bytes > 1024 * 1024 {
            format!("{:.1} MB", total_bytes as f64 / (1024.0 * 1024.0))
        } else if total_bytes > 1024 {
            format!("{:.1} KB", total_bytes as f64 / 1024.0)
        } else {
            format!("{} bytes", total_bytes)
        };
        ctx.say(format!(
            "Ingested **{}** ({} files, {})",
            display_name, file_count, size_display
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
    fn test_build_file_index_offsets() {
        let consolidated = "=== src/main.rs ===\nfn main() {}\n\n=== README.md ===\n# Hello\n";
        let files = parse_githem_output(consolidated);
        let index = build_file_index("owner/repo", consolidated, &files);

        assert_eq!(index.repo, "owner/repo");
        assert_eq!(index.file_count, 2);
        assert_eq!(index.files.len(), 2);

        // Verify first file offset points to actual content
        let f0 = &index.files[0];
        assert_eq!(f0.path, "src/main.rs");
        let slice = &consolidated[f0.offset..f0.offset + f0.length];
        assert_eq!(slice, "fn main() {}");

        // Verify second file offset points to actual content
        let f1 = &index.files[1];
        assert_eq!(f1.path, "README.md");
        let slice = &consolidated[f1.offset..f1.offset + f1.length];
        assert_eq!(slice, "# Hello");
    }

    #[test]
    fn test_build_file_index_single_file() {
        let consolidated = "=== only.txt ===\nsome content here\n";
        let files = parse_githem_output(consolidated);
        let index = build_file_index("a/b", consolidated, &files);

        assert_eq!(index.file_count, 1);
        let f = &index.files[0];
        assert_eq!(f.path, "only.txt");
        let slice = &consolidated[f.offset..f.offset + f.length];
        assert_eq!(slice, "some content here");
    }

    #[test]
    fn test_build_file_index_roundtrip_json() {
        let consolidated = "=== a.rs ===\nlet x = 1;\n=== b.rs ===\nlet y = 2;\n";
        let files = parse_githem_output(consolidated);
        let index = build_file_index("test/repo", consolidated, &files);

        let json = serde_json::to_vec(&index).unwrap();
        let decoded: FileIndex = serde_json::from_slice(&json).unwrap();
        assert_eq!(index, decoded);
    }
}
