//! Manifest management for Akash deployments.
//!
//! Handles:
//! - SDL to manifest conversion (JSON format for provider API)
//! - Manifest sending to providers via REST API

use anyhow::{anyhow, Result};
use reqwest::Client as HttpClient;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;

/// Manifest builder from SDL YAML.
pub struct ManifestBuilder {
    owner: String,
    dseq: u64,
}

impl ManifestBuilder {
    /// Create a new manifest builder.
    pub fn new(owner: &str, dseq: u64) -> Self {
        Self {
            owner: owner.to_string(),
            dseq,
        }
    }

    /// Build manifest JSON from SDL YAML content.
    ///
    /// The Akash provider API expects manifest in a specific JSON format.
    pub fn build_from_sdl(&self, sdl_yaml: &str) -> Result<ManifestJson> {
        let yaml: serde_yaml::Value = serde_yaml::from_str(sdl_yaml)
            .map_err(|e| anyhow!("Failed to parse SDL YAML: {}", e))?;

        let groups = self.parse_manifest_groups(&yaml)?;

        Ok(ManifestJson { groups })
    }

    /// Parse manifest groups from SDL.
    fn parse_manifest_groups(&self, yaml: &serde_yaml::Value) -> Result<Vec<ManifestGroup>> {
        let mut groups = Vec::new();

        let services_section = yaml
            .get("services")
            .ok_or_else(|| anyhow!("Missing 'services' section"))?;

        let deployment_section = yaml
            .get("deployment")
            .ok_or_else(|| anyhow!("Missing 'deployment' section"))?;

        let profiles_section = yaml.get("profiles");

        let services = self.parse_services(services_section, deployment_section, profiles_section)?;

        if !services.is_empty() {
            groups.push(ManifestGroup {
                name: "akash".to_string(),
                services,
            });
        }

        Ok(groups)
    }

    /// Parse services from SDL.
    fn parse_services(
        &self,
        services_section: &serde_yaml::Value,
        deployment_section: &serde_yaml::Value,
        profiles_section: Option<&serde_yaml::Value>,
    ) -> Result<Vec<ManifestService>> {
        let mut services = Vec::new();

        let services_map = services_section
            .as_mapping()
            .ok_or_else(|| anyhow!("'services' must be a mapping"))?;

        for (name, config) in services_map {
            let service_name = name
                .as_str()
                .ok_or_else(|| anyhow!("Service name must be string"))?;

            let service = self.parse_service(
                service_name,
                config,
                deployment_section,
                profiles_section,
            )?;
            services.push(service);
        }

        Ok(services)
    }

    /// Parse a single service.
    fn parse_service(
        &self,
        name: &str,
        config: &serde_yaml::Value,
        deployment_section: &serde_yaml::Value,
        profiles_section: Option<&serde_yaml::Value>,
    ) -> Result<ManifestService> {
        let image = config
            .get("image")
            .and_then(|i| i.as_str())
            .ok_or_else(|| anyhow!("Service '{}' missing image", name))?
            .to_string();

        let count = self.get_service_count(name, deployment_section);
        let (command, args) = self.parse_command_args(config);
        let env = self.parse_env(config);
        let expose = self.parse_expose(config)?;
        let resources = self.parse_service_resources(name, profiles_section)?;

        Ok(ManifestService {
            name: name.to_string(),
            image,
            command,
            args,
            env,
            expose,
            count,
            resources,
            params: None,
            credentials: None,
        })
    }

    fn get_service_count(&self, service_name: &str, deployment: &serde_yaml::Value) -> u32 {
        deployment
            .get(service_name)
            .and_then(|d| d.as_mapping())
            .and_then(|m| {
                m.values().next().and_then(|v| {
                    v.get("count").and_then(|c| c.as_u64())
                })
            })
            .unwrap_or(1) as u32
    }

    fn parse_command_args(&self, config: &serde_yaml::Value) -> (Vec<String>, Vec<String>) {
        let command = config
            .get("command")
            .and_then(|c| {
                if c.is_sequence() {
                    c.as_sequence()
                        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                } else {
                    c.as_str().map(|s| vec![s.to_string()])
                }
            })
            .unwrap_or_default();

        let args = config
            .get("args")
            .and_then(|a| {
                a.as_sequence()
                    .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
            })
            .unwrap_or_default();

        (command, args)
    }

    fn parse_env(&self, config: &serde_yaml::Value) -> Vec<String> {
        config
            .get("env")
            .and_then(|e| {
                e.as_sequence().map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
            })
            .unwrap_or_default()
    }

    fn parse_expose(&self, config: &serde_yaml::Value) -> Result<Vec<ManifestExpose>> {
        let mut exposes = Vec::new();

        let expose_section = match config.get("expose") {
            Some(e) => e,
            None => return Ok(exposes),
        };

        let expose_arr = expose_section
            .as_sequence()
            .ok_or_else(|| anyhow!("'expose' must be an array"))?;

        for expose_config in expose_arr {
            let port = expose_config
                .get("port")
                .and_then(|p| p.as_u64())
                .unwrap_or(80) as u32;

            let external_port = expose_config
                .get("as")
                .and_then(|p| p.as_u64())
                .unwrap_or(port as u64) as u32;

            let proto = expose_config
                .get("proto")
                .and_then(|p| p.as_str())
                .unwrap_or("TCP")
                .to_uppercase();

            let global = expose_config
                .get("to")
                .and_then(|t| t.as_sequence())
                .map(|arr| {
                    arr.iter().any(|item| {
                        item.get("global")
                            .and_then(|g| g.as_bool())
                            .unwrap_or(false)
                    })
                })
                .unwrap_or(false);

            exposes.push(ManifestExpose {
                port,
                external_port,
                proto,
                service: String::new(),
                global,
                hosts: Vec::new(),
                http_options: None,
                ip: String::new(),
                endpoint_sequence_number: 0,
            });
        }

        Ok(exposes)
    }

    fn parse_service_resources(
        &self,
        service_name: &str,
        profiles: Option<&serde_yaml::Value>,
    ) -> Result<ManifestResources> {
        let profiles = profiles.ok_or_else(|| anyhow!("Missing profiles section"))?;

        let compute = profiles
            .get("compute")
            .ok_or_else(|| anyhow!("Missing compute profiles"))?;

        let profile = compute
            .get(service_name)
            .ok_or_else(|| anyhow!("Missing compute profile for '{}'", service_name))?;

        let resources = profile
            .get("resources")
            .ok_or_else(|| anyhow!("Missing resources in profile"))?;

        let cpu = resources
            .get("cpu")
            .and_then(|c| c.get("units"))
            .and_then(|u| {
                if u.is_number() {
                    u.as_f64().map(|f| (f * 1000.0) as u32)
                } else {
                    u.as_str().and_then(|s| s.parse::<f64>().ok().map(|f| (f * 1000.0) as u32))
                }
            })
            .unwrap_or(1000);

        let memory = resources
            .get("memory")
            .and_then(|m| m.get("size"))
            .and_then(|s| s.as_str())
            .map(|s| self.parse_size(s))
            .transpose()?
            .unwrap_or(536_870_912);

        let storage = self.parse_storage_resources(resources)?;

        let gpu = resources
            .get("gpu")
            .and_then(|g| g.get("units"))
            .and_then(|u| u.as_u64())
            .map(|units| ManifestGpu {
                units,
                attributes: None,
            });

        Ok(ManifestResources {
            cpu,
            memory,
            storage,
            gpu,
        })
    }

    fn parse_storage_resources(&self, resources: &serde_yaml::Value) -> Result<Vec<ManifestStorage>> {
        let mut storage_list = Vec::new();

        let storage_section = match resources.get("storage") {
            Some(s) => s,
            None => {
                storage_list.push(ManifestStorage {
                    name: "default".to_string(),
                    size: 1_073_741_824, // 1Gi
                    attributes: None,
                });
                return Ok(storage_list);
            }
        };

        let storage_arr = if storage_section.is_sequence() {
            storage_section.as_sequence().unwrap().clone()
        } else {
            vec![storage_section.clone()]
        };

        for storage in storage_arr {
            let name = storage
                .get("name")
                .and_then(|n| n.as_str())
                .unwrap_or("default")
                .to_string();

            let size_str = storage
                .get("size")
                .and_then(|s| s.as_str())
                .unwrap_or("1Gi");

            let size = self.parse_size(size_str)?;

            storage_list.push(ManifestStorage {
                name,
                size,
                attributes: None,
            });
        }

        Ok(storage_list)
    }

    fn parse_size(&self, s: &str) -> Result<u64> {
        let (num_str, multiplier) = if s.ends_with("Gi") {
            (&s[..s.len() - 2], 1024 * 1024 * 1024u64)
        } else if s.ends_with("Mi") {
            (&s[..s.len() - 2], 1024 * 1024u64)
        } else if s.ends_with("Ki") {
            (&s[..s.len() - 2], 1024u64)
        } else {
            (s, 1u64)
        };

        let num: u64 = num_str
            .parse()
            .map_err(|_| anyhow!("Invalid size: {}", s))?;

        Ok(num * multiplier)
    }
}

// ============ Manifest JSON Types ============

/// Manifest JSON for provider API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestJson {
    pub groups: Vec<ManifestGroup>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestGroup {
    pub name: String,
    pub services: Vec<ManifestService>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestService {
    pub name: String,
    pub image: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub command: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub env: Vec<String>,
    pub expose: Vec<ManifestExpose>,
    pub count: u32,
    pub resources: ManifestResources,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credentials: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestExpose {
    pub port: u32,
    #[serde(rename = "externalPort")]
    pub external_port: u32,
    pub proto: String,
    #[serde(default)]
    pub service: String,
    pub global: bool,
    #[serde(default)]
    pub hosts: Vec<String>,
    #[serde(rename = "httpOptions", skip_serializing_if = "Option::is_none")]
    pub http_options: Option<serde_json::Value>,
    #[serde(default)]
    pub ip: String,
    #[serde(rename = "endpointSequenceNumber", default)]
    pub endpoint_sequence_number: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestResources {
    pub cpu: u32,
    pub memory: u64,
    pub storage: Vec<ManifestStorage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gpu: Option<ManifestGpu>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestStorage {
    pub name: String,
    pub size: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attributes: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestGpu {
    pub units: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attributes: Option<serde_json::Value>,
}

// ============ Manifest Sender ============

/// Manifest sender for provider communication.
pub struct ManifestSender {
    provider_uri: String,
    http: HttpClient,
}

impl ManifestSender {
    /// Create a new manifest sender.
    pub fn new(provider_uri: &str) -> Self {
        let http = HttpClient::builder()
            .timeout(Duration::from_secs(60))
            .danger_accept_invalid_certs(true) // Providers often use self-signed certs
            .build()
            .expect("http client");

        Self {
            provider_uri: provider_uri.to_string(),
            http,
        }
    }

    /// Send manifest to provider via REST API.
    pub async fn send_manifest(
        &self,
        owner: &str,
        dseq: u64,
        gseq: u32,
        oseq: u32,
        manifest: &ManifestJson,
    ) -> Result<()> {
        // Akash provider manifest endpoint
        let url = format!(
            "{}/deployment/{}/{}/{}/{}/manifest",
            self.provider_uri.trim_end_matches('/'),
            owner,
            dseq,
            gseq,
            oseq
        );

        tracing::info!("Sending manifest to {}", url);

        let response = self
            .http
            .put(&url)
            .json(manifest)
            .send()
            .await
            .map_err(|e| anyhow!("Failed to send manifest: {}", e))?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(anyhow!("Manifest send failed ({}): {}", status, body));
        }

        tracing::info!("Manifest sent successfully");
        Ok(())
    }

    /// Send manifest from SDL YAML.
    pub async fn send_manifest_from_sdl(
        &self,
        owner: &str,
        dseq: u64,
        gseq: u32,
        oseq: u32,
        sdl_yaml: &str,
    ) -> Result<()> {
        let builder = ManifestBuilder::new(owner, dseq);
        let manifest = builder.build_from_sdl(sdl_yaml)?;
        self.send_manifest(owner, dseq, gseq, oseq, &manifest).await
    }
}

// ============ Endpoint Query ============

/// Service endpoint information.
#[derive(Debug, Clone)]
pub struct ServiceEndpoint {
    pub service_name: String,
    pub external_uri: String,
    pub internal_port: u16,
    pub external_port: u16,
    pub protocol: String,
}

/// Query service endpoints from provider.
pub async fn query_service_endpoints(
    provider_uri: &str,
    owner: &str,
    dseq: u64,
    gseq: u32,
    oseq: u32,
) -> Result<HashMap<String, ServiceEndpoint>> {
    let http = HttpClient::builder()
        .timeout(Duration::from_secs(30))
        .danger_accept_invalid_certs(true)
        .build()?;

    // Provider lease status endpoint
    let url = format!(
        "{}/lease/{}/{}/{}/{}/status",
        provider_uri.trim_end_matches('/'),
        owner,
        dseq,
        gseq,
        oseq
    );

    tracing::info!("Querying endpoints from {}", url);

    let response = http.get(&url).send().await?;

    if !response.status().is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(anyhow!("Status query failed: {}", body));
    }

    let json: serde_json::Value = response.json().await?;
    let mut endpoints = HashMap::new();

    // Parse services from response
    if let Some(services) = json.get("services").and_then(|s| s.as_object()) {
        for (name, service) in services {
            if let Some(uris) = service.get("uris").and_then(|u| u.as_array()) {
                if let Some(uri) = uris.first().and_then(|u| u.as_str()) {
                    let port = service
                        .get("ports")
                        .and_then(|p| p.as_array())
                        .and_then(|arr| arr.first())
                        .and_then(|p| p.get("port"))
                        .and_then(|p| p.as_u64())
                        .unwrap_or(80) as u16;

                    endpoints.insert(
                        name.clone(),
                        ServiceEndpoint {
                            service_name: name.clone(),
                            external_uri: uri.to_string(),
                            internal_port: port,
                            external_port: port,
                            protocol: "tcp".to_string(),
                        },
                    );
                }
            }
        }
    }

    Ok(endpoints)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_SDL: &str = r#"
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
          units: 1
        memory:
          size: 512Mi
        storage:
          - size: 1Gi
  placement:
    akash:
      pricing:
        web:
          denom: uakt
          amount: 10000

deployment:
  web:
    akash:
      profile: web
      count: 1
"#;

    #[test]
    fn test_build_manifest_from_sdl() {
        let builder = ManifestBuilder::new("akash1owner", 12345);
        let manifest = builder.build_from_sdl(SAMPLE_SDL).unwrap();

        assert!(!manifest.groups.is_empty());
        let group = &manifest.groups[0];
        assert!(!group.services.is_empty());

        let service = &group.services[0];
        assert_eq!(service.name, "web");
        assert_eq!(service.image, "nginx:latest");
        assert_eq!(service.count, 1);
    }

    #[test]
    fn test_parse_size() {
        let builder = ManifestBuilder::new("akash1owner", 12345);
        assert_eq!(builder.parse_size("512Mi").unwrap(), 536_870_912);
        assert_eq!(builder.parse_size("1Gi").unwrap(), 1_073_741_824);
    }
}
