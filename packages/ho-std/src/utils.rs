//! Common utility functions for file operations, config loading, and shared logic
//!
//! This module provides reusable helper functions that implement the common patterns
//! used throughout the ERGORS system, respecting the sacred geometric, fractal requirements
//! of the workspace for interoperability and effectiveness in organization.

use crate::{
    error::{HoError, HoResult},
    traits::file_ops::{ConfigLoaderTrait, FileOptsTrait},
};
use chrono::{DateTime, Utc};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::{
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
            HoError::Cfg(format!(
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
            HoError::Cfg(format!(
                "Failed to parse JSON config from '{}': {}",
                path.as_ref().display(),
                e
            ))
        })
    }

    /// Save configuration to TOML file
    fn to_toml_file<T: Serialize, P: AsRef<Path>>(config: &T, path: P) -> HoResult<()> {
        let content = toml::to_string_pretty(config)
            .map_err(|e| HoError::Cfg(format!("Failed to serialize config to TOML: {}", e)))?;
        DefaultFileOps::write_string(path, &content)
    }

    /// Save configuration to JSON file
    fn to_json_file<T: Serialize, P: AsRef<Path>>(config: &T, path: P) -> HoResult<()> {
        let content = serde_json::to_string_pretty(config)
            .map_err(|e| HoError::Cfg(format!("Failed to serialize config to JSON: {}", e)))?;
        DefaultFileOps::write_string(path, &content)
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

/// SDL (Akash Stack Definition Language) Utilities
///
/// SDL files are YAML-based configuration files used by Akash Network.
/// These utilities help convert between JSON and YAML representations.
pub struct SdlConverter;

impl SdlConverter {
    /// Convert JSON value to YAML string (SDL format)
    ///
    /// # Arguments
    /// * `json_value` - A serde_json::Value representing the SDL configuration
    ///
    /// # Returns
    /// Result containing the YAML string or an error
    ///
    /// # Example
    /// ```
    /// use ho_std::utils::SdlConverter;
    /// use serde_json::json;
    ///
    /// let json_sdl = json!({
    ///     "version": "2.0",
    ///     "services": {
    ///         "web": {
    ///             "image": "nginx:latest"
    ///         }
    ///     }
    /// });
    ///
    /// let yaml_sdl = SdlConverter::json_to_yaml(&json_sdl).unwrap();
    /// assert!(yaml_sdl.contains("version:"));
    /// ```
    pub fn json_to_yaml(json_value: &serde_json::Value) -> HoResult<String> {
        serde_yaml::to_string(json_value)
            .map_err(|e| HoError::Cfg(format!("Failed to convert JSON to YAML: {}", e)))
    }

    /// Convert JSON string to YAML string (SDL format)
    ///
    /// # Arguments
    /// * `json_str` - A JSON string representing the SDL configuration
    ///
    /// # Returns
    /// Result containing the YAML string or an error
    pub fn json_string_to_yaml(json_str: &str) -> HoResult<String> {
        let json_value: serde_json::Value = serde_json::from_str(json_str)
            .map_err(|e| HoError::Cfg(format!("Failed to parse JSON: {}", e)))?;
        Self::json_to_yaml(&json_value)
    }

    /// Convert YAML string (SDL format) to JSON value
    ///
    /// # Arguments
    /// * `yaml_str` - A YAML string representing the SDL configuration
    ///
    /// # Returns
    /// Result containing the JSON value or an error
    pub fn yaml_to_json(yaml_str: &str) -> HoResult<serde_json::Value> {
        serde_yaml::from_str(yaml_str)
            .map_err(|e| HoError::Cfg(format!("Failed to convert YAML to JSON: {}", e)))
    }

    /// Convert YAML string (SDL format) to JSON string
    ///
    /// # Arguments
    /// * `yaml_str` - A YAML string representing the SDL configuration
    ///
    /// # Returns
    /// Result containing the JSON string or an error
    pub fn yaml_to_json_string(yaml_str: &str) -> HoResult<String> {
        let json_value = Self::yaml_to_json(yaml_str)?;
        serde_json::to_string_pretty(&json_value)
            .map_err(|e| HoError::Cfg(format!("Failed to serialize JSON: {}", e)))
    }

    /// Write SDL as YAML to a file
    ///
    /// # Arguments
    /// * `json_value` - A serde_json::Value representing the SDL configuration
    /// * `path` - Path where to write the YAML file
    ///
    /// # Returns
    /// Result indicating success or error
    pub fn write_sdl_yaml<P: AsRef<Path>>(json_value: &serde_json::Value, path: P) -> HoResult<()> {
        let yaml_content = Self::json_to_yaml(json_value)?;
        DefaultFileOps::write_string(path, &yaml_content)
    }

    /// Read SDL YAML file and convert to JSON
    ///
    /// # Arguments
    /// * `path` - Path to the YAML SDL file
    ///
    /// # Returns
    /// Result containing the JSON value or an error
    pub fn read_sdl_yaml<P: AsRef<Path>>(path: P) -> HoResult<serde_json::Value> {
        let yaml_content = DefaultFileOps::read_string(path)?;
        Self::yaml_to_json(&yaml_content)
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
            // Create a test file
            let test_content = r#" "#;

            let test_file = "./test_data/example_config.md";
            DefaultFileOps::write_string(test_file, test_content)?;
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

    #[test]
    fn test_json_to_yaml_conversion() {
        use serde_json::json;

        let json_sdl = json!({
            "version": "2.0",
            "services": {
                "web": {
                    "image": "nginx:latest",
                    "expose": [
                        {
                            "port": 80,
                            "as": 80,
                            "to": [{"global": true}]
                        }
                    ]
                }
            },
            "profiles": {
                "compute": {
                    "web": {
                        "resources": {
                            "cpu": {"units": "1.0"},
                            "memory": {"size": "512Mi"},
                            "storage": {"size": "1Gi"}
                        }
                    }
                }
            }
        });

        let yaml_result = SdlConverter::json_to_yaml(&json_sdl).unwrap();

        // Verify YAML contains expected keys
        assert!(yaml_result.contains("version:"));
        assert!(yaml_result.contains("services:"));
        assert!(yaml_result.contains("web:"));
        assert!(yaml_result.contains("nginx:latest"));
        assert!(yaml_result.contains("profiles:"));
        assert!(yaml_result.contains("compute:"));
    }

    #[test]
    fn test_yaml_to_json_conversion() {
        let yaml_sdl = r#"
version: "2.0"
services:
  web:
    image: nginx:latest
    expose:
      - port: 80
        as: 80
        to:
          - global: true
profiles:
  compute:
    web:
      resources:
        cpu:
          units: "1.0"
        memory:
          size: 512Mi
        storage:
          size: 1Gi
"#;

        let json_result = SdlConverter::yaml_to_json(yaml_sdl).unwrap();

        // Verify JSON structure
        assert_eq!(json_result["version"], "2.0");
        assert_eq!(json_result["services"]["web"]["image"], "nginx:latest");
        assert_eq!(
            json_result["profiles"]["compute"]["web"]["resources"]["cpu"]["units"],
            "1.0"
        );
    }

    #[test]
    fn test_roundtrip_conversion() {
        use serde_json::json;

        let original_json = json!({
            "version": "2.0",
            "services": {
                "app": {
                    "image": "myapp:v1.0",
                    "env": ["KEY=value"]
                }
            }
        });

        // Convert to YAML
        let yaml = SdlConverter::json_to_yaml(&original_json).unwrap();

        // Convert back to JSON
        let roundtrip_json = SdlConverter::yaml_to_json(&yaml).unwrap();

        // Verify they match
        assert_eq!(original_json["version"], roundtrip_json["version"]);
        assert_eq!(
            original_json["services"]["app"]["image"],
            roundtrip_json["services"]["app"]["image"]
        );
    }

    #[test]
    fn test_json_string_to_yaml() {
        let json_str = r#"{"version": "2.0", "services": {"web": {"image": "nginx"}}}"#;
        let yaml = SdlConverter::json_string_to_yaml(json_str).unwrap();

        assert!(yaml.contains("version:"));
        assert!(yaml.contains("nginx"));
    }

    #[test]
    fn test_yaml_to_json_string() {
        let yaml_str = "version: \"2.0\"\nservices:\n  web:\n    image: nginx";
        let json_str = SdlConverter::yaml_to_json_string(yaml_str).unwrap();

        assert!(json_str.contains("\"version\""));
        assert!(json_str.contains("\"2.0\""));
        assert!(json_str.contains("nginx"));
    }
}
