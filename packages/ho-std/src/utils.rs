//! Common utility functions for file operations, config loading, and shared logic
//!
//! This module provides reusable helper functions that implement the common patterns
//! used throughout the CW-HO system, respecting the sacred geometric, fractal requirements
//! of the workspace for interoperability and effectiveness in organization.

use crate::{
    constants::ENV_KEYS,
    error::{HoError, HoResult},
    traits::file_ops::{ConfigLoaderTrait, FileOptsTrait},
};
use chrono::{DateTime, Utc};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
use uuid::Uuid;

/// Common file operations helper.
/// implements all of the file_opts traits for a one-size-fits-all generic implementation.
#[derive(Deserialize, Serialize)]
pub struct DefaultFileOps;

impl ConfigLoaderTrait for DefaultFileOps {
    /// Load configuration from TOML file
    fn from_toml_file<T: DeserializeOwned, P: AsRef<Path>>(path: P) -> HoResult<T> {
        let content = DefaultFileOps::read_string(&path)?;
        toml::from_str(&content).map_err(|e| {
            HoError::Config(format!(
                "Failed to parse TOML config from '{}': {}",
                path.as_ref().display(),
                e
            ))
        })
    }

    /// Load configuration from JSON file
    fn from_json_file<T: DeserializeOwned, P: AsRef<Path>>(path: P) -> HoResult<T> {
        let content = DefaultFileOps::read_string(&path)?;
        serde_json::from_str(&content).map_err(|e| {
            HoError::Config(format!(
                "Failed to parse JSON config from '{}': {}",
                path.as_ref().display(),
                e
            ))
        })
    }

    /// Save configuration to TOML file
    fn to_toml_file<T: Serialize, P: AsRef<Path>>(config: &T, path: P) -> HoResult<()> {
        let content = toml::to_string_pretty(config)
            .map_err(|e| HoError::Config(format!("Failed to serialize config to TOML: {}", e)))?;
        DefaultFileOps::write_string(path, &content)
    }

    /// Save configuration to JSON file
    fn to_json_file<T: Serialize, P: AsRef<Path>>(config: &T, path: P) -> HoResult<()> {
        let content = serde_json::to_string_pretty(config)
            .map_err(|e| HoError::Config(format!("Failed to serialize config to JSON: {}", e)))?;
        DefaultFileOps::write_string(path, &content)
    }

    /// Load API keys from JSON file or environment variables
    fn load_api_keys(path: &str) -> HoResult<HashMap<String, String>> {
        if DefaultFileOps::exists(path) {
            Self::from_json_file(path)
        } else {
            // Fallback to environment variables
            let mut keys = HashMap::new();
            for env_keys in ENV_KEYS {
                if let Ok(value) = std::env::var(env_keys.1.to_string()) {
                    keys.insert(env_keys.0.to_string(), value);
                }
            }

            Ok(keys)
        }
    }
}

impl FileOptsTrait for DefaultFileOps {
    /// Read file contents as string with error handling
    fn read_string<P: AsRef<Path>>(path: P) -> HoResult<String> {
        fs::read_to_string(&path).map_err(|e| {
            HoError::from(format!(
                "Failed to read file '{}': {}",
                path.as_ref().display(),
                e
            ))
        })
    }

    /// Write string to file with error handling
    fn write_string<P: AsRef<Path>>(path: P, content: &str) -> HoResult<()> {
        // Create parent directories if they don't exist
        if let Some(parent) = path.as_ref().parent() {
            fs::create_dir_all(parent).map_err(|e| {
                HoError::from(format!(
                    "Failed to create directory '{}': {}",
                    parent.display(),
                    e
                ))
            })?;
        }

        fs::write(&path, content).map_err(|e| {
            HoError::from(format!(
                "Failed to write file '{}': {}",
                path.as_ref().display(),
                e
            ))
        })
    }

    /// Check if file exists
    fn exists<P: AsRef<Path>>(path: P) -> bool {
        path.as_ref().exists()
    }

    /// Get file size in bytes
    fn size<P: AsRef<Path>>(path: P) -> HoResult<u64> {
        fs::metadata(&path).map(|m| m.len()).map_err(|e| {
            HoError::from(format!(
                "Failed to get file size for '{}': {}",
                path.as_ref().display(),
                e
            ))
        })
    }

    /// Create directory recursively
    fn create_dir_all<P: AsRef<Path>>(path: P) -> HoResult<()> {
        fs::create_dir_all(&path).map_err(|e| {
            HoError::from(format!(
                "Failed to create directory '{}': {}",
                path.as_ref().display(),
                e
            ))
        })
    }

    /// List files in directory with optional extension filter
    fn list_files<P: AsRef<Path>>(dir: P, extension: Option<&str>) -> HoResult<Vec<PathBuf>> {
        let entries = fs::read_dir(&dir).map_err(|e| {
            HoError::from(format!(
                "Failed to read directory '{}': {}",
                dir.as_ref().display(),
                e
            ))
        })?;

        let mut files = Vec::new();
        for entry in entries {
            let entry = entry
                .map_err(|e| HoError::from(format!("Failed to read directory entry: {}", e)))?;

            let path = entry.path();
            if path.is_file() {
                if let Some(ext) = extension {
                    if path.extension().and_then(|s| s.to_str()) == Some(ext) {
                        files.push(path);
                    }
                } else {
                    files.push(path);
                }
            }
        }

        Ok(files)
    }
}

/// Common ID generation helper
pub struct IdGenerator;

impl IdGenerator {
    /// Generate a new UUID as bytes
    pub fn new_uuid_bytes() -> Vec<u8> {
        Uuid::new_v4().as_bytes().to_vec()
    }

    /// Generate a new UUID as string
    pub fn new_uuid_string() -> String {
        Uuid::new_v4().to_string()
    }

    /// Generate timestamp as seconds since epoch
    pub fn timestamp_seconds() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    }

    /// Generate timestamp in milliseconds since epoch
    pub fn timestamp_millis() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64
    }

    /// Generate current UTC timestamp
    pub fn utc_timestamp() -> DateTime<Utc> {
        Utc::now()
    }
}

#[cfg(test)]
mod tests {
    use tracing::info;

    use super::*;
    use std::env;

    #[test]
    fn test_file_operations() {
        let test_content = "test content";
        let test_path = env::temp_dir().join("test_file.txt");

        // Test write and read
        DefaultFileOps::write_string(&test_path, test_content).unwrap();
        let read_content = DefaultFileOps::read_string(&test_path).unwrap();
        assert_eq!(read_content, test_content);

        // Test exists
        assert!(DefaultFileOps::exists(&test_path));

        // Cleanup
        fs::remove_file(&test_path).ok();

        async fn demonstrate_file_operations() -> HoResult<()> {
            use std::path::Path;

            // Create a test file
            let test_content = r#" "#;

            let test_file = "./test_data/example_config.md";
            DefaultFileOps::write_string(&test_file, test_content)?;
            info!("Created test file: {}", test_file);

            // // Demonstrate file sharing:
            // // 1. two nodes on same machine
            // // 2. two nodes on different maching through transport
            // let shared_path = FileShareImpl::share_file(&test_file, Path::new("./shared"))?;
            // info!("Shared file to: {}", shared_path);

            // // Create a backup:
            // // 1. backup local nodes config via snapshot
            // // 2. transport backups up to main
            // let backup_path = FileShareImpl::backup_file(&test_file)?;
            // info!("Created backup at: {}", backup_path);

            // // Sync files to another directory
            // let synced_files = FileShareImpl::sync_files("./test_data", "./synced", Some("md"))?;
            // info!("Synced {} files", synced_files.len());
            Ok(())
        }
    }

    #[test]
    fn test_id_generation() {
        let uuid_bytes = IdGenerator::new_uuid_bytes();
        assert_eq!(uuid_bytes.len(), 16);

        let uuid_string = IdGenerator::new_uuid_string();
        assert_eq!(uuid_string.len(), 36);

        let timestamp = IdGenerator::timestamp_seconds();
        assert!(timestamp > 0);
    }
}
