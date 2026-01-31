//! Manifest construction from SDL.
//!
//! The manifest is what gets sent to providers. It's derived from
//! the SDL but in a format the provider understands.
//!
//! This module provides utilities for building manifests.
//! The actual serialization format depends on the Akash provider API version.

use crate::error::DeployError;
use crate::types::Resources;
use serde::{Deserialize, Serialize};

/// A manifest group (corresponds to a deployment group in SDL).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestGroup {
    pub name: String,
    pub services: Vec<ManifestService>,
}

/// A service in the manifest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestService {
    pub name: String,
    pub image: String,
    pub command: Vec<String>,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    pub resources: Resources,
    pub count: u32,
    pub expose: Vec<ManifestExpose>,
}

/// Exposed port configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestExpose {
    pub port: u16,
    pub external_port: u16,
    pub proto: String, // "TCP" or "UDP"
    pub global: bool,
    pub hosts: Vec<String>,
}

/// Full manifest for a deployment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub groups: Vec<ManifestGroup>,
}

impl Manifest {
    /// Create empty manifest.
    pub fn new() -> Self {
        Self { groups: Vec::new() }
    }

    /// Add a group.
    pub fn add_group(&mut self, group: ManifestGroup) {
        self.groups.push(group);
    }

    /// Serialize to JSON bytes (provider API format).
    pub fn to_json(&self) -> Result<Vec<u8>, DeployError> {
        serde_json::to_vec(self).map_err(|e| DeployError::Manifest(format!("json error: {}", e)))
    }
}

impl Default for Manifest {
    fn default() -> Self {
        Self::new()
    }
}

/// Build a manifest from SDL content.
///
/// This parses the SDL and constructs the manifest structure.
/// Returns the manifest as JSON bytes ready to send to provider.
pub fn build_from_sdl(sdl_content: &str, dseq: u64) -> Result<Vec<u8>, DeployError> {
    let doc: serde_yaml::Value = serde_yaml::from_str(sdl_content)
        .map_err(|e| DeployError::Sdl(format!("parse error: {}", e)))?;

    let services = doc
        .get("services")
        .and_then(|s| s.as_mapping())
        .ok_or_else(|| DeployError::Sdl("services must be a mapping".into()))?;

    let profiles = doc
        .get("profiles")
        .and_then(|p| p.as_mapping())
        .ok_or_else(|| DeployError::Sdl("profiles must be a mapping".into()))?;

    let compute = profiles
        .get(serde_yaml::Value::String("compute".into()))
        .and_then(|c| c.as_mapping())
        .ok_or_else(|| DeployError::Sdl("profiles.compute must be a mapping".into()))?;

    let deployment = doc
        .get("deployment")
        .and_then(|d| d.as_mapping())
        .ok_or_else(|| DeployError::Sdl("deployment must be a mapping".into()))?;

    let mut manifest = Manifest::new();

    // For each service in deployment, build manifest entry
    for (svc_name, svc_deploy) in deployment {
        let svc_name = svc_name
            .as_str()
            .ok_or_else(|| DeployError::Sdl("service name must be string".into()))?;

        let svc_def = services
            .get(serde_yaml::Value::String(svc_name.into()))
            .ok_or_else(|| {
                DeployError::Sdl(format!("service {} not found in services", svc_name))
            })?;

        let svc_compute = compute
            .get(serde_yaml::Value::String(svc_name.into()))
            .ok_or_else(|| {
                DeployError::Sdl(format!(
                    "compute profile for {} not found",
                    svc_name
                ))
            })?;

        // Extract image
        let image = svc_def
            .get("image")
            .and_then(|i| i.as_str())
            .unwrap_or("unknown")
            .to_string();

        // Extract resources
        let resources = parse_resources(svc_compute)?;

        // Extract expose
        let expose = parse_expose(svc_def)?;

        // Extract count from deployment
        let count = extract_count(svc_deploy)?;

        // Extract env
        let env = parse_env(svc_def);

        // Extract command/args
        let (command, args) = parse_command(svc_def);

        let manifest_svc = ManifestService {
            name: svc_name.to_string(),
            image,
            command,
            args,
            env,
            resources,
            count,
            expose,
        };

        // Group name comes from placement
        let group_name = extract_group_name(svc_deploy)?;

        // Find or create group
        if let Some(group) = manifest.groups.iter_mut().find(|g| g.name == group_name) {
            group.services.push(manifest_svc);
        } else {
            manifest.groups.push(ManifestGroup {
                name: group_name,
                services: vec![manifest_svc],
            });
        }
    }

    // Include dseq in some way if needed (provider uses it for validation)
    let _ = dseq;

    manifest.to_json()
}

fn parse_resources(compute: &serde_yaml::Value) -> Result<Resources, DeployError> {
    let resources = compute
        .get("resources")
        .ok_or_else(|| DeployError::Sdl("missing resources in compute profile".into()))?;

    let cpu = resources
        .get("cpu")
        .and_then(|c| c.get("units"))
        .and_then(|u| u.as_f64())
        .unwrap_or(0.5);

    let memory = resources
        .get("memory")
        .and_then(|m| m.get("size"))
        .and_then(|s| s.as_str())
        .unwrap_or("512Mi");

    let storage = resources
        .get("storage")
        .and_then(|s| s.get("size"))
        .and_then(|s| s.as_str())
        .unwrap_or("1Gi");

    Ok(Resources {
        cpu_millicores: (cpu * 1000.0) as u32,
        memory_bytes: parse_size(memory),
        storage_bytes: parse_size(storage),
        gpu_count: 0,
    })
}

fn parse_size(s: &str) -> u64 {
    let s = s.trim();
    if let Some(num) = s.strip_suffix("Gi") {
        num.parse::<u64>().unwrap_or(1) * 1024 * 1024 * 1024
    } else if let Some(num) = s.strip_suffix("Mi") {
        num.parse::<u64>().unwrap_or(512) * 1024 * 1024
    } else if let Some(num) = s.strip_suffix("Ki") {
        num.parse::<u64>().unwrap_or(1024) * 1024
    } else {
        s.parse().unwrap_or(1024 * 1024 * 1024)
    }
}

fn parse_expose(svc_def: &serde_yaml::Value) -> Result<Vec<ManifestExpose>, DeployError> {
    let expose = match svc_def.get("expose") {
        Some(e) => e.as_sequence().unwrap_or(&vec![]).clone(),
        None => return Ok(vec![]),
    };

    let mut result = Vec::new();
    for exp in expose {
        let port = exp
            .get("port")
            .and_then(|p| p.as_u64())
            .unwrap_or(80) as u16;

        let external_port = exp
            .get("as")
            .and_then(|a| a.as_u64())
            .unwrap_or(port as u64) as u16;

        let proto = exp
            .get("proto")
            .and_then(|p| p.as_str())
            .unwrap_or("TCP")
            .to_uppercase();

        let global = exp
            .get("to")
            .and_then(|t| t.as_sequence())
            .map(|seq| seq.iter().any(|v| v.get("global").and_then(|g| g.as_bool()).unwrap_or(false)))
            .unwrap_or(false);

        result.push(ManifestExpose {
            port,
            external_port,
            proto,
            global,
            hosts: vec![],
        });
    }

    Ok(result)
}

fn extract_count(deploy: &serde_yaml::Value) -> Result<u32, DeployError> {
    // deployment.service.placement.count
    if let Some(mapping) = deploy.as_mapping() {
        for (_placement_name, placement) in mapping {
            if let Some(count) = placement.get("count").and_then(|c| c.as_u64()) {
                return Ok(count as u32);
            }
        }
    }
    Ok(1) // default
}

fn extract_group_name(deploy: &serde_yaml::Value) -> Result<String, DeployError> {
    if let Some(mapping) = deploy.as_mapping() {
        if let Some((name, _)) = mapping.iter().next() {
            if let Some(s) = name.as_str() {
                return Ok(s.to_string());
            }
        }
    }
    Ok("dcloud".to_string()) // default
}

fn parse_env(svc_def: &serde_yaml::Value) -> Vec<(String, String)> {
    let env = match svc_def.get("env") {
        Some(e) => e.as_sequence().unwrap_or(&vec![]).clone(),
        None => return vec![],
    };

    env.iter()
        .filter_map(|v| v.as_str())
        .filter_map(|s| {
            let parts: Vec<&str> = s.splitn(2, '=').collect();
            if parts.len() == 2 {
                Some((parts[0].to_string(), parts[1].to_string()))
            } else {
                None
            }
        })
        .collect()
}

fn parse_command(svc_def: &serde_yaml::Value) -> (Vec<String>, Vec<String>) {
    let command = svc_def
        .get("command")
        .and_then(|c| c.as_sequence())
        .map(|seq| {
            seq.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    let args = svc_def
        .get("args")
        .and_then(|a| a.as_sequence())
        .map(|seq| {
            seq.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    (command, args)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_size() {
        assert_eq!(parse_size("1Gi"), 1024 * 1024 * 1024);
        assert_eq!(parse_size("512Mi"), 512 * 1024 * 1024);
        assert_eq!(parse_size("1024Ki"), 1024 * 1024);
    }

    #[test]
    fn test_empty_manifest() {
        let m = Manifest::new();
        assert!(m.groups.is_empty());
    }
}
