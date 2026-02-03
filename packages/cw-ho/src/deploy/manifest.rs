//! Manifest management for Akash deployments.
//!
//! Handles:
//! - SDL to manifest conversion using proto-generated types
//! - JWT authentication with providers (self-attested, validated on-chain)
//! - Manifest sending to providers via REST API
//!
//! ## JWT Authentication
//!
//! JWTs are self-attested by the client:
//! 1. Client creates JWT with claims (issuer = account address, timestamps)
//! 2. Client signs JWT with their secp256k1 private key (ES256K)
//! 3. Client sends JWT in `Authorization: Bearer` header
//! 4. Provider validates by fetching public key from on-chain account state
//!
//! There is NO challenge-response flow or registration step.

use anyhow::{anyhow, Result};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use reqwest::Client as HttpClient;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;

use ho_std::keys::cosmos::CosmosKeyPair;

// JSON types matching provider's expected format.
// Note: Proto types use bytes for ResourceValue.val, but provider JSON API expects strings.
// These types mirror the proto structure but serialize correctly for the REST API.

/// Full manifest structure for hashing (mirrors akash.manifest.v2beta3.Manifest).
///
/// This wrapper is used when computing the manifest version hash, which must
/// match what the provider computes: SHA256(json.Marshal(manifest)).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub groups: Vec<ManifestGroup>,
}

/// Manifest group for provider API (mirrors akash.manifest.v2beta3.Group).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestGroup {
    pub name: String,
    pub services: Vec<ManifestService>,
}

/// Service definition (mirrors akash.manifest.v2beta3.Service).
///
/// Fields serialize as `null` when empty/None to match Go's encoding/json behavior.
/// Provider validation requires these fields to be present (even as null).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestService {
    pub name: String,
    pub image: String,
    pub command: Option<Vec<String>>,
    pub args: Option<Vec<String>>,
    pub env: Option<Vec<String>>,
    pub resources: ManifestResources,
    pub count: u32,
    pub expose: Vec<ManifestServiceExpose>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<ManifestServiceParams>,
    pub credentials: Option<ManifestCredentials>,
}

/// Service expose (mirrors akash.manifest.v2beta3.ServiceExpose).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestServiceExpose {
    pub port: u32,
    #[serde(rename = "externalPort")]
    pub external_port: u32,
    pub proto: String,
    #[serde(default)]
    pub service: String,
    pub global: bool,
    pub hosts: Option<Vec<String>>,
    #[serde(rename = "httpOptions")]
    pub http_options: ManifestHttpOptions,
    #[serde(default)]
    pub ip: String,
    #[serde(rename = "endpointSequenceNumber", default)]
    pub endpoint_sequence_number: u32,
}

/// HTTP options for service expose (mirrors provider defaults).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestHttpOptions {
    #[serde(rename = "maxBodySize")]
    pub max_body_size: u32,
    #[serde(rename = "readTimeout")]
    pub read_timeout: u32,
    #[serde(rename = "sendTimeout")]
    pub send_timeout: u32,
    #[serde(rename = "nextTries")]
    pub next_tries: u32,
    #[serde(rename = "nextTimeout")]
    pub next_timeout: u32,
    #[serde(rename = "nextCases")]
    pub next_cases: Vec<String>,
}

impl Default for ManifestHttpOptions {
    fn default() -> Self {
        Self {
            max_body_size: 1_048_576,
            read_timeout: 60_000,
            send_timeout: 60_000,
            next_tries: 3,
            next_timeout: 0,
            next_cases: vec!["error".to_string(), "timeout".to_string()],
        }
    }
}

/// Resources (mirrors akash.base.resources.v1beta4.Resources).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestResources {
    pub id: u32,
    pub cpu: ManifestCpu,
    pub memory: ManifestMemory,
    pub storage: Vec<ManifestStorage>,
    pub gpu: ManifestGpu,
    #[serde(default)]
    pub endpoints: Vec<serde_json::Value>,
}

/// CPU resource (mirrors akash.base.resources.v1beta4.CPU).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestCpu {
    pub units: ManifestResourceValue,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attributes: Vec<serde_json::Value>,
}

/// Memory resource (mirrors akash.base.resources.v1beta4.Memory).
/// Note: Field is "size" to match Go provider's JSON (not proto's "quantity").
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestMemory {
    pub size: ManifestResourceValue,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attributes: Vec<serde_json::Value>,
}

/// Storage resource (mirrors akash.base.resources.v1beta4.Storage).
/// Note: Field is "size" to match Go provider's JSON (not proto's "quantity").
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestStorage {
    pub name: String,
    pub size: ManifestResourceValue,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attributes: Vec<serde_json::Value>,
}

/// GPU resource (mirrors akash.base.resources.v1beta4.GPU).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestGpu {
    pub units: ManifestResourceValue,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attributes: Vec<serde_json::Value>,
}

/// Resource value - string representation of numeric value.
/// Note: Proto uses bytes, but JSON API expects string.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestResourceValue {
    pub val: String,
}

/// Service params (mirrors akash.manifest.v2beta3.ServiceParams).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestServiceParams {
    #[serde(default)]
    pub storage: Vec<ManifestStorageParams>,
}

/// Storage params for mounts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestStorageParams {
    pub name: String,
    pub mount: String,
    #[serde(rename = "readOnly", default)]
    pub read_only: bool,
}

/// Image credentials.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestCredentials {
    pub host: String,
    pub email: String,
    pub username: String,
    pub password: String,
}

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

    /// Build manifest from SDL YAML content.
    ///
    /// Returns a vector of ManifestGroups for the provider API.
    pub fn build_from_sdl(&self, sdl_yaml: &str) -> Result<Vec<ManifestGroup>> {
        let yaml: serde_yaml::Value = serde_yaml::from_str(sdl_yaml)
            .map_err(|e| anyhow!("Failed to parse SDL YAML: {}", e))?;

        self.parse_manifest_groups(&yaml)
    }

    /// Parse manifest groups from SDL.
    ///
    /// The manifest group name must match the deployment group name (placement name in SDL).
    fn parse_manifest_groups(&self, yaml: &serde_yaml::Value) -> Result<Vec<ManifestGroup>> {
        let mut groups = Vec::new();

        let services_section = yaml
            .get("services")
            .ok_or_else(|| anyhow!("Missing 'services' section"))?;

        let deployment_section = yaml
            .get("deployment")
            .ok_or_else(|| anyhow!("Missing 'deployment' section"))?;

        let profiles_section = yaml.get("profiles");

        // Extract group name from deployment section (the placement name)
        // SDL structure: deployment: { <service>: { <placement>: { ... } } }
        let group_name = self.extract_group_name(deployment_section)?;

        let mut services = self.parse_services(services_section, deployment_section, profiles_section)?;

        // Provider requires services sorted by name
        services.sort_by(|a, b| a.name.cmp(&b.name));

        if !services.is_empty() {
            groups.push(ManifestGroup {
                name: group_name,
                services,
            });
        }

        Ok(groups)
    }

    /// Extract group name (placement name) from deployment section.
    fn extract_group_name(&self, deployment: &serde_yaml::Value) -> Result<String> {
        let deployment_map = deployment
            .as_mapping()
            .ok_or_else(|| anyhow!("'deployment' must be a mapping"))?;

        // Get first service's first placement name as the group name
        for (_service_name, service_config) in deployment_map {
            if let Some(config_map) = service_config.as_mapping() {
                for (placement_name, _) in config_map {
                    if let Some(name) = placement_name.as_str() {
                        return Ok(name.to_string());
                    }
                }
            }
        }

        // Fallback to "dcloud" which is common default
        Ok("dcloud".to_string())
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

        // Convert empty vecs to None (Go serializes missing fields as null)
        let command = if command.is_empty() { None } else { Some(command) };
        let args = if args.is_empty() { None } else { Some(args) };
        let env = if env.is_empty() { None } else { Some(env) };

        // Parse storage params (mount points)
        let params = self.parse_storage_params(config);

        Ok(ManifestService {
            name: name.to_string(),
            image,
            command,
            args,
            env,
            expose,
            count,
            resources,
            params,
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

    fn parse_storage_params(&self, config: &serde_yaml::Value) -> Option<ManifestServiceParams> {
        let params_section = config.get("params")?.get("storage")?;
        let params_map = params_section.as_mapping()?;

        let mut storage_params = Vec::new();
        for (name, value) in params_map {
            let name = name.as_str()?;
            let mount = value.get("mount")?.as_str()?;
            let read_only = value
                .get("readOnly")
                .and_then(|r| r.as_bool())
                .unwrap_or(false);
            storage_params.push(ManifestStorageParams {
                name: name.to_string(),
                mount: mount.to_string(),
                read_only,
            });
        }

        if storage_params.is_empty() {
            None
        } else {
            Some(ManifestServiceParams {
                storage: storage_params,
            })
        }
    }

    fn parse_expose(&self, config: &serde_yaml::Value) -> Result<Vec<ManifestServiceExpose>> {
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

            // external_port: 0 when not explicitly set (matches Go provider behavior)
            let external_port = expose_config
                .get("as")
                .and_then(|p| p.as_u64())
                .unwrap_or(0) as u32;

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

            // Parse accept hosts
            let hosts = expose_config
                .get("accept")
                .and_then(|a| a.as_sequence())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect::<Vec<_>>()
                });
            // Go serializes missing hosts as null, present-but-empty as []
            let hosts = hosts.filter(|h| !h.is_empty());

            exposes.push(ManifestServiceExpose {
                port,
                external_port,
                proto,
                service: String::new(),
                global,
                hosts,
                http_options: ManifestHttpOptions::default(),
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

        // CPU: parse units - SDL uses millicores notation (e.g. "100m" = 100 millicores)
        // Provider expects millicores as the value (100m → "100")
        let cpu_millicpus = resources
            .get("cpu")
            .and_then(|c| c.get("units"))
            .and_then(|u| {
                if u.is_number() {
                    // Numeric value: treat as whole CPUs → convert to millicores
                    u.as_f64().map(|f| (f * 1000.0) as u64)
                } else {
                    u.as_str().map(|s| {
                        if let Some(millis) = s.strip_suffix('m') {
                            // Already in millicores (e.g. "100m" → 100)
                            millis.parse::<u64>().unwrap_or(1000)
                        } else {
                            // Whole CPUs as string (e.g. "1" → 1000)
                            s.parse::<f64>()
                                .map(|f| (f * 1000.0) as u64)
                                .unwrap_or(1000)
                        }
                    })
                }
            })
            .unwrap_or(1000);

        let cpu = ManifestCpu {
            units: ManifestResourceValue { val: cpu_millicpus.to_string() },
            attributes: Vec::new(),
        };

        // Memory: parse size string to bytes
        let memory_bytes = resources
            .get("memory")
            .and_then(|m| m.get("size"))
            .and_then(|s| s.as_str())
            .map(|s| self.parse_size(s))
            .transpose()?
            .unwrap_or(536_870_912);

        let memory = ManifestMemory {
            size: ManifestResourceValue { val: memory_bytes.to_string() },
            attributes: Vec::new(),
        };

        let storage = self.parse_storage_resources(resources)?;

        // GPU: always include (provider requires it, default to 0 units)
        let gpu_units = resources
            .get("gpu")
            .and_then(|g| g.get("units"))
            .and_then(|u| u.as_u64())
            .unwrap_or(0);

        let mut gpu_attributes: Vec<serde_json::Value> = Vec::new();

        // Parse GPU attributes (vendor/model/ram composite keys)
        // Must match deployment_builder's parse_gpu() format for provider cross-validation
        if gpu_units > 0 {
            if let Some(gpu_section) = resources.get("gpu") {
                if let Some(attrs) = gpu_section.get("attributes") {
                    if let Some(vendor_section) = attrs.get("vendor") {
                        if let Some(vendor_map) = vendor_section.as_mapping() {
                            for (vendor_name, vendor_config) in vendor_map {
                                let vendor = vendor_name.as_str().unwrap_or("nvidia");
                                if let Some(models) = vendor_config.as_sequence() {
                                    for model_entry in models {
                                        if let Some(model_map) = model_entry.as_mapping() {
                                            let model_name = model_map
                                                .get(serde_yaml::Value::String("model".to_string()))
                                                .and_then(|v| v.as_str())
                                                .unwrap_or("");

                                            let ram = model_map
                                                .get(serde_yaml::Value::String("ram".to_string()))
                                                .and_then(|v| v.as_str());

                                            let iface = model_map
                                                .get(serde_yaml::Value::String("interface".to_string()))
                                                .and_then(|v| v.as_str());

                                            if !model_name.is_empty() {
                                                let mut key = format!("vendor/{}/model/{}", vendor, model_name);
                                                if let Some(ram_value) = ram {
                                                    key.push_str(&format!("/ram/{}", ram_value));
                                                }
                                                if let Some(iface_value) = iface {
                                                    key.push_str(&format!("/interface/{}", iface_value));
                                                }
                                                gpu_attributes.push(serde_json::json!({
                                                    "key": key,
                                                    "value": "true"
                                                }));
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        let gpu = ManifestGpu {
            units: ManifestResourceValue { val: gpu_units.to_string() },
            attributes: gpu_attributes,
        };

        Ok(ManifestResources {
            id: 1,
            cpu,
            memory,
            storage,
            gpu,
            endpoints: Vec::new(),
        })
    }

    fn parse_storage_resources(&self, resources: &serde_yaml::Value) -> Result<Vec<ManifestStorage>> {
        let mut storage_list = Vec::new();

        let storage_section = match resources.get("storage") {
            Some(s) => s,
            None => {
                storage_list.push(ManifestStorage {
                    name: "default".to_string(),
                    size: ManifestResourceValue { val: "1073741824".to_string() }, // 1Gi
                    attributes: Vec::new(),
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

            let size_bytes = self.parse_size(size_str)?;

            // Parse storage attributes (persistent, class)
            let mut storage_attrs: Vec<serde_json::Value> = Vec::new();
            if let Some(attrs) = storage.get("attributes") {
                if let Some(persistent) = attrs.get("persistent") {
                    let val = match persistent {
                        serde_yaml::Value::Bool(b) => b.to_string(),
                        serde_yaml::Value::String(s) => s.clone(),
                        _ => "false".to_string(),
                    };
                    storage_attrs.push(serde_json::json!({
                        "key": "persistent",
                        "value": val
                    }));
                }
                if let Some(class) = attrs.get("class") {
                    let val = class.as_str().unwrap_or("default");
                    storage_attrs.push(serde_json::json!({
                        "key": "class",
                        "value": val
                    }));
                }
                // Sort attributes by key for consistency with Go
                storage_attrs.sort_by(|a, b| {
                    let ak = a.get("key").and_then(|k| k.as_str()).unwrap_or("");
                    let bk = b.get("key").and_then(|k| k.as_str()).unwrap_or("");
                    ak.cmp(bk)
                });
            }

            storage_list.push(ManifestStorage {
                name,
                size: ManifestResourceValue { val: size_bytes.to_string() },
                attributes: storage_attrs,
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

// ============ JWT Authentication ============

/// JWT token with expiry tracking.
#[derive(Debug, Clone)]
pub struct JwtToken {
    pub token: String,
    pub expires_at: std::time::Instant,
}

impl JwtToken {
    /// Check if token is expired (with 60s buffer for safety).
    pub fn is_expired(&self) -> bool {
        self.expires_at.checked_duration_since(std::time::Instant::now())
            .map(|remaining| remaining < Duration::from_secs(60))
            .unwrap_or(true)
    }
}

/// JWT claims for Akash provider authentication.
///
/// JWTs are self-attested: the client creates and signs them.
/// The provider validates by fetching the issuer's public key from on-chain state.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct JwtClaims {
    /// Issuer - the account address (e.g., "akash1...")
    iss: String,
    /// Issued at - Unix timestamp
    iat: i64,
    /// Expiration - Unix timestamp
    exp: i64,
    /// Not before - Unix timestamp
    nbf: i64,
    /// JWT ID - unique identifier to prevent replay attacks
    #[serde(skip_serializing_if = "Option::is_none")]
    jti: Option<String>,
    /// Version identifier
    version: String,
    /// Lease access permissions
    leases: JwtLeases,
}

/// Lease access permissions for JWT claims.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct JwtLeases {
    /// Access type: "full" for full lease access
    access: String,
}

/// JWT header for ES256K signing.
#[derive(Debug, Clone, Serialize)]
struct JwtHeader {
    alg: String,
    typ: String,
}

/// JWT authentication client for Akash providers.
///
/// JWTs are self-attested by the client:
/// 1. Client creates JWT with claims (issuer = account address, timestamps)
/// 2. Client signs JWT with their secp256k1 private key (ES256K)
/// 3. Client sends JWT in Authorization: Bearer header
/// 4. Provider validates by fetching public key from on-chain account state
///
/// There is NO challenge-response or registration - each request is independently validated.
pub struct JwtAuthClient {
    http: HttpClient,
    provider_uri: String,
    /// Cached token (refreshed when expired)
    cached_token: Option<JwtToken>,
}

impl JwtAuthClient {
    /// Create a new JWT auth client for a provider.
    pub fn new(provider_uri: &str) -> Self {
        let http = HttpClient::builder()
            .timeout(Duration::from_secs(30))
            .danger_accept_invalid_certs(true) // Providers use self-signed certs
            .build()
            .expect("http client");

        Self {
            http,
            provider_uri: provider_uri.trim_end_matches('/').to_string(),
            cached_token: None,
        }
    }

    /// Validate JWT claims before signing.
    ///
    /// Ensures that:
    /// - Issuer is valid Akash bech32 format (akash1 + 38 chars)
    /// - Time relationships are correct (nbf <= iat <= exp)
    /// - Token is not expired or not yet valid
    /// - Version is exactly "v1"
    /// - Access type is valid ("full", "scoped", or "granular")
    fn validate_claims(&self, claims: &JwtClaims) -> Result<()> {
        // Validate issuer format: akash1 + 38 chars = 44 total
        if !claims.iss.starts_with("akash1") || claims.iss.len() != 44 {
            return Err(anyhow::anyhow!("Invalid issuer format: {}", claims.iss));
        }

        // Validate time relationships
        if claims.nbf > claims.iat {
            return Err(anyhow::anyhow!(
                "nbf ({}) cannot be after iat ({})",
                claims.nbf,
                claims.iat
            ));
        }
        if claims.iat > claims.exp {
            return Err(anyhow::anyhow!(
                "iat ({}) cannot be after exp ({})",
                claims.iat,
                claims.exp
            ));
        }

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| anyhow::anyhow!("System time error: {}", e))?
            .as_secs() as i64;

        if now < claims.nbf {
            return Err(anyhow::anyhow!("Token not yet valid (nbf in future)"));
        }
        if now > claims.exp {
            return Err(anyhow::anyhow!("Token expired"));
        }

        // Validate version
        if claims.version != "v1" {
            return Err(anyhow::anyhow!(
                "Unsupported version: {}",
                claims.version
            ));
        }

        // Validate access type
        match claims.leases.access.as_str() {
            "full" | "scoped" | "granular" => Ok(()),
            _ => Err(anyhow::anyhow!(
                "Invalid access type: {}",
                claims.leases.access
            )),
        }
    }

    /// Get a valid JWT token, creating a new one if expired.
    ///
    /// This is the main entry point - handles caching automatically.
    pub async fn get_token(&mut self, address: &str, keypair: &CosmosKeyPair) -> Result<String> {
        // Return cached token if still valid
        if let Some(ref token) = self.cached_token {
            if !token.is_expired() {
                return Ok(token.token.clone());
            }
        }

        // Create fresh self-signed JWT
        let token = self.create_jwt(address, keypair)?;
        Ok(token)
    }

    /// Create a self-signed JWT for provider authentication.
    ///
    /// The JWT is created entirely client-side:
    /// - Header: {"alg": "ES256K", "typ": "JWT"}
    /// - Claims: issuer (address), iat, exp, nbf, jti
    /// - Signature: secp256k1 signature with double-SHA256 over header.claims
    fn create_jwt(&mut self, address: &str, keypair: &CosmosKeyPair) -> Result<String> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| anyhow::anyhow!("System time error: {}", e))?
            .as_secs() as i64;

        // JWT valid for 15 minutes
        let exp = now + 15 * 60;

        let header = JwtHeader {
            alg: "ES256K".to_string(),
            typ: "JWT".to_string(),
        };

        // Generate unique JWT ID for replay protection
        let jti = format!("{}-{}", &address[..12.min(address.len())], uuid::Uuid::new_v4());

        let claims = JwtClaims {
            iss: address.to_string(),
            iat: now,
            exp,
            nbf: now,
            jti: Some(jti),
            version: "v1".to_string(),
            leases: JwtLeases {
                access: "full".to_string(),
            },
        };

        // VALIDATE BEFORE SIGNING
        self.validate_claims(&claims)?;

        // Base64url encode header and claims
        let header_json = serde_json::to_string(&header)?;
        let claims_json = serde_json::to_string(&claims)?;

        let header_b64 = base64url_encode(&header_json);
        let claims_b64 = base64url_encode(&claims_json);

        // Create signing input: header.claims
        let signing_input = format!("{}.{}", header_b64, claims_b64);

        // USE THE CORRECT ES256K SIGNING METHOD (single-SHA256)
        // ES256K (RFC 8812) = ECDSA with secp256k1 + SHA-256 (NOT Bitcoin's double-SHA256)
        let signature = keypair.sign_jwt_es256k(signing_input.as_bytes())?;
        let signature_b64 = base64url_encode_bytes(&signature);

        // Construct JWT: header.claims.signature
        let jwt = format!("{}.{}", signing_input, signature_b64);

        // Cache with 14-minute expiry (tokens valid for 15 min)
        self.cached_token = Some(JwtToken {
            token: jwt.clone(),
            expires_at: std::time::Instant::now() + Duration::from_secs(14 * 60),
        });

        tracing::debug!(
            "Created ES256K JWT for {}..{}",
            &address[..12.min(address.len())],
            if address.len() > 6 {
                &address[address.len() - 6..]
            } else {
                ""
            }
        );
        Ok(jwt)
    }

    /// Make an authenticated request with automatic token refresh.
    pub async fn authenticated_request(
        &mut self,
        method: reqwest::Method,
        path: &str,
        address: &str,
        keypair: &CosmosKeyPair,
    ) -> Result<reqwest::RequestBuilder> {
        let token = self.get_token(address, keypair).await?;
        let url = format!("{}{}", self.provider_uri, path);

        Ok(self.http
            .request(method, &url)
            .header("Authorization", format!("Bearer {}", token)))
    }
}

/// Base64url encode a string (no padding).
fn base64url_encode(input: &str) -> String {
    base64url_encode_bytes(input.as_bytes())
}

/// Base64url encode bytes (no padding).
fn base64url_encode_bytes(input: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(input)
}

// ============ Manifest Sender ============

/// Manifest sender for provider communication with JWT authentication.
pub struct ManifestSender {
    auth: JwtAuthClient,
    provider_uri: String,
}

impl ManifestSender {
    /// Create a new manifest sender with JWT authentication.
    pub fn new(provider_uri: &str) -> Self {
        Self {
            auth: JwtAuthClient::new(provider_uri),
            provider_uri: provider_uri.trim_end_matches('/').to_string(),
        }
    }

    /// Send manifest to provider via REST API with JWT auth.
    ///
    /// The provider expects an array of groups directly (v2beta3.Manifest format).
    /// CRITICAL: Must use canonical JSON (sorted keys) to match on-chain manifest hash.
    pub async fn send_manifest(
        &mut self,
        _owner: &str,
        dseq: u64,
        _gseq: u32,
        _oseq: u32,
        manifest: &[ManifestGroup],
        address: &str,
        keypair: &CosmosKeyPair,
    ) -> Result<()> {
        let path = format!("/deployment/{}/manifest", dseq);

        tracing::info!("Sending manifest to {}{}", self.provider_uri, path);

        // CRITICAL: Use canonical JSON (sorted keys) to match the hash computed on-chain
        // Provider recomputes SHA256(manifest_json) and compares to on-chain hash
        use crate::deploy::deployment_builder::to_canonical_json;
        let manifest_json = to_canonical_json(manifest)?;

        tracing::debug!("Manifest canonical JSON: {}", manifest_json);

        // Provider expects array of groups directly (v2beta3.Manifest = []Group)
        let request = self.auth
            .authenticated_request(reqwest::Method::PUT, &path, address, keypair)
            .await?
            .header("Content-Type", "application/json")
            .body(manifest_json);

        let response = request
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
        &mut self,
        owner: &str,
        dseq: u64,
        gseq: u32,
        oseq: u32,
        sdl_yaml: &str,
        address: &str,
        keypair: &CosmosKeyPair,
    ) -> Result<()> {
        let builder = ManifestBuilder::new(owner, dseq);
        let manifest = builder.build_from_sdl(sdl_yaml)?;
        self.send_manifest(owner, dseq, gseq, oseq, &manifest, address, keypair).await
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

/// Parse service endpoints from a lease status JSON response.
///
/// Handles two endpoint types returned by Akash providers:
/// - `services.{name}.uris` — reverse-proxied HTTP services (port 80/443)
/// - `forwarded_ports.{name}` — direct port forwards (e.g. GPU inference on port 8000)
///
/// URIs take precedence: if a service has both `uris` and `forwarded_ports`, the URI is used.
fn parse_lease_status_endpoints(json: &serde_json::Value) -> HashMap<String, ServiceEndpoint> {
    let mut endpoints = HashMap::new();

    // Parse services.{name}.uris (reverse-proxied HTTP endpoints)
    if let Some(services) = json.get("services").and_then(|s| s.as_object()) {
        for (name, service) in services {
            if let Some(uris) = service.get("uris").and_then(|u| u.as_array()) {
                if let Some(uri) = uris.first().and_then(|u| u.as_str()) {
                    let ports_arr = service
                        .get("ports")
                        .and_then(|p| p.as_array());

                    let internal_port = ports_arr
                        .and_then(|arr| arr.first())
                        .and_then(|p| p.get("port"))
                        .and_then(|p| p.as_u64())
                        .unwrap_or(80) as u16;

                    let external_port = ports_arr
                        .and_then(|arr| arr.first())
                        .and_then(|p| p.get("externalPort"))
                        .and_then(|p| p.as_u64())
                        .map(|p| p as u16)
                        .unwrap_or(internal_port);

                    endpoints.insert(
                        name.clone(),
                        ServiceEndpoint {
                            service_name: name.clone(),
                            external_uri: uri.to_string(),
                            internal_port,
                            external_port,
                            protocol: "tcp".to_string(),
                        },
                    );
                }
            }
        }
    }

    // Parse forwarded_ports (for non-HTTP services like GPU inference on port 8000)
    if let Some(forwarded) = json.get("forwarded_ports").and_then(|f| f.as_object()) {
        for (name, ports) in forwarded {
            if endpoints.contains_key(name) {
                continue; // uris take precedence
            }
            if let Some(entry) = ports.as_array().and_then(|a| a.first()) {
                let host = entry.get("host").and_then(|h| h.as_str()).unwrap_or("");
                let internal_port =
                    entry.get("port").and_then(|p| p.as_u64()).unwrap_or(0) as u16;
                let external_port = entry
                    .get("externalPort")
                    .and_then(|p| p.as_u64())
                    .unwrap_or(0) as u16;
                let proto = entry
                    .get("proto")
                    .and_then(|p| p.as_str())
                    .unwrap_or("tcp");

                if !host.is_empty() && external_port > 0 {
                    endpoints.insert(
                        name.clone(),
                        ServiceEndpoint {
                            service_name: name.clone(),
                            external_uri: format!("http://{}:{}", host, external_port),
                            internal_port,
                            external_port,
                            protocol: proto.to_lowercase(),
                        },
                    );
                }
            }
        }
    }

    endpoints
}

/// Query service endpoints from provider with JWT authentication.
pub async fn query_service_endpoints(
    provider_uri: &str,
    owner: &str,
    dseq: u64,
    gseq: u32,
    oseq: u32,
    address: &str,
    keypair: &CosmosKeyPair,
) -> Result<HashMap<String, ServiceEndpoint>> {
    let mut auth = JwtAuthClient::new(provider_uri);

    let path = format!("/lease/{}/{}/{}/{}/status", owner, dseq, gseq, oseq);

    tracing::info!("Querying endpoints from {}{}", provider_uri, path);

    let request = auth
        .authenticated_request(reqwest::Method::GET, &path, address, keypair)
        .await?;

    let response = request.send().await?;

    if !response.status().is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(anyhow!("Status query failed: {}", body));
    }

    let json: serde_json::Value = response.json().await?;

    tracing::debug!(
        "Lease status response: {}",
        serde_json::to_string_pretty(&json).unwrap_or_default()
    );

    Ok(parse_lease_status_endpoints(&json))
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
        let groups = builder.build_from_sdl(SAMPLE_SDL).unwrap();

        assert!(!groups.is_empty());
        let group = &groups[0];
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

    #[test]
    fn test_jwt_token_expiry() {
        let token = JwtToken {
            token: "test".to_string(),
            expires_at: std::time::Instant::now() + Duration::from_secs(30),
        };
        // Within 60s buffer, should be considered expired
        assert!(token.is_expired());

        let token2 = JwtToken {
            token: "test".to_string(),
            expires_at: std::time::Instant::now() + Duration::from_secs(120),
        };
        assert!(!token2.is_expired());
    }

    fn unix_now() -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64
    }

    fn valid_claims(now: i64) -> JwtClaims {
        JwtClaims {
            iss: "akash1abcdefghijklmnopqrstuvwxyz123456789012".to_string(),
            iat: now,
            nbf: now,
            exp: now + 900,
            jti: Some("test-jti".to_string()),
            version: "v1".to_string(),
            leases: JwtLeases {
                access: "full".to_string(),
            },
        }
    }

    fn generate_test_address() -> (String, ho_std::keys::cosmos::CosmosKeyPair) {
        use ho_std::keys::cosmos::CosmosMnemonic;

        let mnemonic = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
        let cosmos_mnemonic = CosmosMnemonic::from_phrase(mnemonic).unwrap();
        let keypair = cosmos_mnemonic.derive_keypair(0).unwrap();
        let address = keypair.address("akash").unwrap();

        (address, keypair)
    }

    #[test]
    fn test_jwt_claims_validation_success() {
        let client = JwtAuthClient::new("http://provider");
        let now = unix_now();
        let (address, _) = generate_test_address();

        let mut claims = valid_claims(now);
        claims.iss = address;

        assert!(client.validate_claims(&claims).is_ok());
    }

    #[test]
    fn test_jwt_claims_validation_invalid_issuer_prefix() {
        let client = JwtAuthClient::new("http://provider");
        let now = unix_now();

        let mut claims = valid_claims(now);
        claims.iss = "notakash1address12345678901234567890123456".to_string();

        assert!(client.validate_claims(&claims).is_err());
    }

    #[test]
    fn test_jwt_claims_validation_invalid_issuer_length() {
        let client = JwtAuthClient::new("http://provider");
        let now = unix_now();

        let mut claims = valid_claims(now);
        claims.iss = "akash1tooshort".to_string();

        assert!(client.validate_claims(&claims).is_err());
    }

    #[test]
    fn test_jwt_claims_validation_time_inversion_nbf_after_iat() {
        let client = JwtAuthClient::new("http://provider");
        let now = unix_now();
        let (address, _) = generate_test_address();

        let mut claims = valid_claims(now);
        claims.iss = address;
        claims.nbf = now + 100;
        claims.iat = now;

        assert!(client.validate_claims(&claims).is_err());
    }

    #[test]
    fn test_jwt_claims_validation_time_inversion_iat_after_exp() {
        let client = JwtAuthClient::new("http://provider");
        let now = unix_now();
        let (address, _) = generate_test_address();

        let mut claims = valid_claims(now);
        claims.iss = address;
        claims.iat = now + 1000;
        claims.exp = now + 500;

        assert!(client.validate_claims(&claims).is_err());
    }

    #[test]
    fn test_jwt_claims_validation_expired() {
        let client = JwtAuthClient::new("http://provider");
        let now = unix_now();
        let (address, _) = generate_test_address();

        let mut claims = valid_claims(now);
        claims.iss = address;
        claims.exp = now - 100; // Already expired

        assert!(client.validate_claims(&claims).is_err());
    }

    #[test]
    fn test_jwt_claims_validation_not_yet_valid() {
        let client = JwtAuthClient::new("http://provider");
        let now = unix_now();
        let (address, _) = generate_test_address();

        let mut claims = valid_claims(now);
        claims.iss = address;
        claims.nbf = now + 100; // Not yet valid
        claims.iat = now + 100;
        claims.exp = now + 1000;

        assert!(client.validate_claims(&claims).is_err());
    }

    #[test]
    fn test_jwt_claims_validation_invalid_version() {
        let client = JwtAuthClient::new("http://provider");
        let now = unix_now();
        let (address, _) = generate_test_address();

        let mut claims = valid_claims(now);
        claims.iss = address;
        claims.version = "v2".to_string();

        assert!(client.validate_claims(&claims).is_err());
    }

    #[test]
    fn test_jwt_claims_validation_invalid_access_type() {
        let client = JwtAuthClient::new("http://provider");
        let now = unix_now();
        let (address, _) = generate_test_address();

        let mut claims = valid_claims(now);
        claims.iss = address;
        claims.leases.access = "invalid".to_string();

        assert!(client.validate_claims(&claims).is_err());
    }

    #[test]
    fn test_jwt_claims_validation_valid_access_types() {
        let client = JwtAuthClient::new("http://provider");
        let now = unix_now();
        let (address, _) = generate_test_address();

        for access in &["full", "scoped", "granular"] {
            let mut claims = valid_claims(now);
            claims.iss = address.clone();
            claims.leases.access = access.to_string();
            assert!(client.validate_claims(&claims).is_ok());
        }
    }

    #[test]
    fn test_es256k_signature_format() {
        let (_, keypair) = generate_test_address();

        let message = b"test message for ES256K signature";
        let sig = keypair.sign_jwt_es256k(message).unwrap();

        // ES256K signature must be exactly 64 bytes (r || s compact format)
        assert_eq!(sig.len(), 64, "ES256K signature should be 64 bytes");
    }

    #[test]
    fn test_jwt_creation_includes_jti() {
        let (address, keypair) = generate_test_address();

        let mut client = JwtAuthClient::new("http://provider");
        let jwt = client.create_jwt(&address, &keypair).unwrap();

        // JWT should have 3 parts: header.claims.signature
        let parts: Vec<&str> = jwt.split('.').collect();
        assert_eq!(parts.len(), 3, "JWT should have 3 parts");

        // Decode and verify claims contain jti
        let claims_json = String::from_utf8(
            base64::engine::general_purpose::URL_SAFE_NO_PAD
                .decode(parts[1])
                .unwrap(),
        )
        .unwrap();

        assert!(
            claims_json.contains("\"jti\""),
            "JWT claims should contain jti field"
        );
    }

    #[test]
    fn test_jwt_structure_format() {
        let (address, keypair) = generate_test_address();

        let mut client = JwtAuthClient::new("http://provider");
        let jwt = client.create_jwt(&address, &keypair).unwrap();

        // Verify JWT structure
        let parts: Vec<&str> = jwt.split('.').collect();
        assert_eq!(parts.len(), 3);

        // Decode header
        let header_json = String::from_utf8(
            base64::engine::general_purpose::URL_SAFE_NO_PAD
                .decode(parts[0])
                .unwrap(),
        )
        .unwrap();

        assert!(header_json.contains("\"alg\":\"ES256K\""));
        assert!(header_json.contains("\"typ\":\"JWT\""));

        // Decode claims
        let claims_json = String::from_utf8(
            base64::engine::general_purpose::URL_SAFE_NO_PAD
                .decode(parts[1])
                .unwrap(),
        )
        .unwrap();

        assert!(claims_json.contains(&format!("\"iss\":\"{}\"", address)));
        assert!(claims_json.contains("\"version\":\"v1\""));
        assert!(claims_json.contains("\"access\":\"full\""));

        // Signature should be 64 bytes when decoded
        let signature = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(parts[2])
            .unwrap();
        assert_eq!(signature.len(), 64, "Signature should be 64 bytes");
    }

    #[test]
    fn test_parse_forwarded_ports() {
        let json: serde_json::Value = serde_json::json!({
            "services": {
                "sglang": { "uris": [], "ports": [] }
            },
            "forwarded_ports": {
                "sglang": [{
                    "host": "provider.example.com",
                    "port": 8000,
                    "externalPort": 32145,
                    "proto": "TCP"
                }]
            }
        });

        let endpoints = parse_lease_status_endpoints(&json);
        assert_eq!(endpoints.len(), 1);

        let ep = endpoints.get("sglang").unwrap();
        assert_eq!(ep.service_name, "sglang");
        assert_eq!(ep.external_uri, "http://provider.example.com:32145");
        assert_eq!(ep.internal_port, 8000);
        assert_eq!(ep.external_port, 32145);
        assert_eq!(ep.protocol, "tcp");
    }

    #[test]
    fn test_uris_take_precedence() {
        let json: serde_json::Value = serde_json::json!({
            "services": {
                "web": {
                    "uris": ["https://abc123.provider.com"],
                    "ports": [{ "port": 80, "externalPort": 80 }]
                }
            },
            "forwarded_ports": {
                "web": [{
                    "host": "provider.example.com",
                    "port": 80,
                    "externalPort": 31000,
                    "proto": "TCP"
                }]
            }
        });

        let endpoints = parse_lease_status_endpoints(&json);
        assert_eq!(endpoints.len(), 1);

        let ep = endpoints.get("web").unwrap();
        assert_eq!(ep.external_uri, "https://abc123.provider.com");
        assert_eq!(ep.internal_port, 80);
        assert_eq!(ep.external_port, 80);
    }

    #[test]
    fn test_mixed_uris_and_forwarded_ports() {
        let json: serde_json::Value = serde_json::json!({
            "services": {
                "web": {
                    "uris": ["https://abc123.provider.com"],
                    "ports": [{ "port": 80, "externalPort": 443 }]
                },
                "sglang": {
                    "uris": [],
                    "ports": []
                }
            },
            "forwarded_ports": {
                "sglang": [{
                    "host": "provider.example.com",
                    "port": 8000,
                    "externalPort": 32145,
                    "proto": "TCP"
                }]
            }
        });

        let endpoints = parse_lease_status_endpoints(&json);
        assert_eq!(endpoints.len(), 2);

        let web = endpoints.get("web").unwrap();
        assert_eq!(web.external_uri, "https://abc123.provider.com");
        assert_eq!(web.internal_port, 80);
        assert_eq!(web.external_port, 443);

        let sglang = endpoints.get("sglang").unwrap();
        assert_eq!(sglang.external_uri, "http://provider.example.com:32145");
        assert_eq!(sglang.internal_port, 8000);
        assert_eq!(sglang.external_port, 32145);
    }

    #[test]
    fn test_empty_forwarded_ports() {
        // Empty forwarded_ports object
        let json: serde_json::Value = serde_json::json!({
            "services": {},
            "forwarded_ports": {}
        });
        let endpoints = parse_lease_status_endpoints(&json);
        assert!(endpoints.is_empty());

        // Missing host
        let json: serde_json::Value = serde_json::json!({
            "forwarded_ports": {
                "svc": [{ "host": "", "port": 8000, "externalPort": 32000, "proto": "TCP" }]
            }
        });
        let endpoints = parse_lease_status_endpoints(&json);
        assert!(endpoints.is_empty());

        // Missing externalPort (zero)
        let json: serde_json::Value = serde_json::json!({
            "forwarded_ports": {
                "svc": [{ "host": "provider.example.com", "port": 8000, "externalPort": 0, "proto": "TCP" }]
            }
        });
        let endpoints = parse_lease_status_endpoints(&json);
        assert!(endpoints.is_empty());

        // Empty array for service
        let json: serde_json::Value = serde_json::json!({
            "forwarded_ports": {
                "svc": []
            }
        });
        let endpoints = parse_lease_status_endpoints(&json);
        assert!(endpoints.is_empty());

        // No forwarded_ports key at all
        let json: serde_json::Value = serde_json::json!({
            "services": {
                "web": { "uris": [], "ports": [] }
            }
        });
        let endpoints = parse_lease_status_endpoints(&json);
        assert!(endpoints.is_empty());
    }

    #[test]
    fn test_jwt_end_to_end_with_real_keys() {
        let (address, keypair) = generate_test_address();

        let mut client = JwtAuthClient::new("http://provider");
        let jwt = client.create_jwt(&address, &keypair).unwrap();

        // Verify the JWT structure and signature
        let parts: Vec<&str> = jwt.split('.').collect();
        assert_eq!(parts.len(), 3, "JWT should have header.claims.signature format");

        // Decode and verify all components
        let _header_bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(parts[0])
            .unwrap();
        let claims_bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(parts[1])
            .unwrap();
        let signature_bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(parts[2])
            .unwrap();

        // Parse claims
        let claims: JwtClaims = serde_json::from_slice(&claims_bytes).unwrap();

        // Verify issuer matches the generated address
        assert_eq!(claims.iss, address, "Issuer should match generated address");

        // Verify signature format
        assert_eq!(
            signature_bytes.len(),
            64,
            "ES256K signature should be 64 bytes"
        );

        // Verify claims are valid
        assert!(client.validate_claims(&claims).is_ok());

        // Verify jti is present and unique
        assert!(claims.jti.is_some(), "JWT should have jti for replay protection");
        let jti = claims.jti.unwrap();
        assert!(jti.len() > 10, "jti should be sufficiently long and unique");
    }
}
