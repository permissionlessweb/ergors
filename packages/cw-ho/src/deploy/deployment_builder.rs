//! Deployment message builder.
//!
//! Converts SDL YAML to Akash MsgCreateDeployment.
//! Handles resource parsing, group specs, and deposit calculation.

use anyhow::{anyhow, Result};
use ho_std::types::akash::base::attributes::v1::{Attribute, PlacementRequirements, SignedBy};
use ho_std::types::akash::base::deposit::v1::{Deposit, Source};
use ho_std::types::akash::base::resources::v1beta4::{
    Cpu, Endpoint, Gpu, Memory, ResourceValue, Resources, Storage,
};
use ho_std::types::ergors::akash::deployment::v1::DeploymentId;
use ho_std::types::ergors::akash::deployment::v1beta4::{
    GroupSpec, MsgCloseDeployment, MsgCreateDeployment, ResourceUnit,
};
use ho_std::types::ergors::akash::market::v1beta4::{BidId, MsgCreateLease};
use ho_std::types::ergors::cosmos::base::v1beta1::{Coin, DecCoin};
use sha2::{Digest, Sha256};

/// Minimum deposit for deployment in uakt (0.5 AKT)
pub const MIN_DEPOSIT_UAKT: u64 = 500_000;

/// Default deposit for deployment in uakt (5 AKT)
pub const DEFAULT_DEPOSIT_UAKT: u64 = 5_000_000;

/// Builder for MsgCreateDeployment from SDL.
pub struct DeploymentBuilder {
    owner: String,
    depositor: String,
    dseq: u64,
    deposit_uakt: u64,
}

impl DeploymentBuilder {
    /// Create a new deployment builder.
    pub fn new(owner: &str, dseq: u64) -> Self {
        Self {
            owner: owner.to_string(),
            depositor: owner.to_string(),
            dseq,
            deposit_uakt: DEFAULT_DEPOSIT_UAKT,
        }
    }

    /// Set depositor (different from owner if using feegrant).
    pub fn with_depositor(mut self, depositor: &str) -> Self {
        self.depositor = depositor.to_string();
        self
    }

    /// Set deposit amount in uakt.
    pub fn with_deposit(mut self, deposit_uakt: u64) -> Self {
        self.deposit_uakt = deposit_uakt.max(MIN_DEPOSIT_UAKT);
        self
    }

    /// Build MsgCreateDeployment from SDL YAML content.
    pub fn build_from_sdl(&self, sdl_yaml: &str) -> Result<MsgCreateDeployment> {
        let yaml: serde_yaml::Value = serde_yaml::from_str(sdl_yaml)
            .map_err(|e| anyhow!("Failed to parse SDL YAML: {}", e))?;

        // Extract groups from SDL
        let groups = self.parse_groups(&yaml)?;

        // Compute hash (SHA256 of SDL content)
        let mut hasher = Sha256::new();
        hasher.update(sdl_yaml.as_bytes());
        let hash = hasher.finalize().to_vec();

        Ok(MsgCreateDeployment {
            id: Some(DeploymentId {
                owner: self.owner.clone(),
                dseq: self.dseq,
            }),
            groups,
            hash,
            deposit: Some(Deposit {
                amount: Some(Coin {
                    denom: "uakt".to_string(),
                    amount: self.deposit_uakt.to_string(),
                }),
                // Use both grant and balance sources (official Akash behavior)
                sources: vec![Source::Grant as i32, Source::Balance as i32],
            }),
        })
    }

    /// Parse deployment groups from SDL YAML.
    fn parse_groups(&self, yaml: &serde_yaml::Value) -> Result<Vec<GroupSpec>> {
        let mut groups = Vec::new();

        // Get deployment section
        let deployment = yaml
            .get("deployment")
            .ok_or_else(|| anyhow!("Missing 'deployment' section in SDL"))?;

        // Get profiles section
        let profiles = yaml
            .get("profiles")
            .ok_or_else(|| anyhow!("Missing 'profiles' section in SDL"))?;

        let compute_profiles = profiles
            .get("compute")
            .ok_or_else(|| anyhow!("Missing 'profiles.compute' section in SDL"))?;

        let placement_profiles = profiles.get("placement");

        // Get services section
        let services = yaml
            .get("services")
            .ok_or_else(|| anyhow!("Missing 'services' section in SDL"))?;

        // Iterate over deployment entries (each creates a group)
        let deployment_map = deployment
            .as_mapping()
            .ok_or_else(|| anyhow!("'deployment' must be a mapping"))?;

        for (service_name, service_deployment) in deployment_map {
            let service_name_str = service_name
                .as_str()
                .ok_or_else(|| anyhow!("Service name must be a string"))?;

            // Each service deployment specifies placement(s)
            let service_deployment_map = service_deployment
                .as_mapping()
                .ok_or_else(|| anyhow!("Service deployment must be a mapping"))?;

            for (placement_name, placement_config) in service_deployment_map {
                let placement_name_str = placement_name
                    .as_str()
                    .ok_or_else(|| anyhow!("Placement name must be a string"))?;

                // Get the profile name from placement config
                let profile_name = placement_config
                    .get("profile")
                    .and_then(|p| p.as_str())
                    .unwrap_or(service_name_str);

                // Get compute profile resources
                let compute_profile = compute_profiles.get(profile_name).ok_or_else(|| {
                    anyhow!(
                        "Compute profile '{}' not found for service '{}'",
                        profile_name,
                        service_name_str
                    )
                })?;

                // Group name is the placement name (e.g., "dcloud"), not service-placement
                let group = GroupSpec {
                    name: placement_name_str.to_string(),
                    requirements: Some(self.parse_placement_requirements(
                        placement_profiles,
                        placement_name_str,
                        compute_profile,
                    )?),
                    resources: vec![ResourceUnit {
                        resource: Some(self.parse_resources(
                            compute_profile,
                            services,
                            service_name_str,
                        )?),
                        count: placement_config
                            .get("count")
                            .and_then(|c| c.as_u64())
                            .unwrap_or(1) as u32,
                        price: Some(self.parse_price(
                            placement_profiles,
                            placement_name_str,
                            service_name_str,
                        )?),
                    }],
                };

                groups.push(group);
            }
        }

        if groups.is_empty() {
            return Err(anyhow!("No deployment groups found in SDL"));
        }

        Ok(groups)
    }

    /// Parse resources from compute profile.
    fn parse_resources(
        &self,
        compute_profile: &serde_yaml::Value,
        services: &serde_yaml::Value,
        service_name: &str,
    ) -> Result<Resources> {
        let resources_section = compute_profile
            .get("resources")
            .ok_or_else(|| anyhow!("Missing 'resources' in compute profile"))?;

        // Parse CPU
        let cpu = resources_section
            .get("cpu")
            .and_then(|c| c.get("units"))
            .map(|u| self.parse_resource_value(u))
            .transpose()?
            .unwrap_or(1000); // Default 1 CPU (1000 millicores)

        // Parse memory
        let memory = resources_section
            .get("memory")
            .and_then(|m| m.get("size"))
            .map(|s| self.parse_memory_size(s))
            .transpose()?
            .unwrap_or(536_870_912); // Default 512Mi

        // Parse storage
        let storage = self.parse_storage(resources_section)?;

        // Parse GPU if present
        let gpu = self.parse_gpu(resources_section)?;

        // Parse endpoints from service definition
        let endpoints = self.parse_endpoints(services, service_name)?;

        Ok(Resources {
            cpu: Some(Cpu {
                units: Some(ResourceValue {
                    val: cpu.to_string().into_bytes(),
                }),
                attributes: vec![],
            }),
            memory: Some(Memory {
                quantity: Some(ResourceValue {
                    val: memory.to_string().into_bytes(),
                }),
                attributes: vec![],
            }),
            storage,
            gpu,
            endpoints,
            id: 1, // Default resource ID
        })
    }

    /// Parse a resource value (could be string or number).
    /// Always converts to millicores (x1000) to match Akash format.
    fn parse_resource_value(&self, value: &serde_yaml::Value) -> Result<u64> {
        match value {
            serde_yaml::Value::Number(n) => {
                let cores = n
                    .as_u64()
                    .or_else(|| n.as_f64().map(|f| f as u64))
                    .ok_or_else(|| anyhow!("Invalid resource number"))?;
                // Convert cores to millicores (Akash uses millicores: 1 core = 1000 millicores)
                Ok(cores * 1000)
            }
            serde_yaml::Value::String(s) => {
                // Handle strings like "4" or "1.5"
                s.parse::<f64>()
                    .map(|f| (f * 1000.0) as u64) // Convert to millicores
                    .map_err(|_| anyhow!("Invalid resource string: {}", s))
            }
            _ => Err(anyhow!("Resource value must be number or string")),
        }
    }

    /// Parse memory size (handles suffixes like Gi, Mi, etc).
    fn parse_memory_size(&self, value: &serde_yaml::Value) -> Result<u64> {
        let s = value
            .as_str()
            .ok_or_else(|| anyhow!("Memory size must be a string"))?;

        let (num_str, multiplier) = if s.ends_with("Gi") {
            (&s[..s.len() - 2], 1024 * 1024 * 1024u64)
        } else if s.ends_with("Mi") {
            (&s[..s.len() - 2], 1024 * 1024u64)
        } else if s.ends_with("Ki") {
            (&s[..s.len() - 2], 1024u64)
        } else if s.ends_with("G") {
            (&s[..s.len() - 1], 1000 * 1000 * 1000u64)
        } else if s.ends_with("M") {
            (&s[..s.len() - 1], 1000 * 1000u64)
        } else if s.ends_with("K") {
            (&s[..s.len() - 1], 1000u64)
        } else {
            (s, 1u64)
        };

        let num: u64 = num_str
            .parse()
            .map_err(|_| anyhow!("Invalid memory size: {}", s))?;

        Ok(num * multiplier)
    }

    /// Parse storage from resources section.
    fn parse_storage(&self, resources: &serde_yaml::Value) -> Result<Vec<Storage>> {
        let mut storage_list = Vec::new();

        if let Some(storage_section) = resources.get("storage") {
            let storage_array = if storage_section.is_sequence() {
                storage_section.as_sequence().unwrap()
            } else {
                // Single storage entry
                return Ok(vec![self.parse_single_storage(storage_section)?]);
            };

            for storage_item in storage_array {
                storage_list.push(self.parse_single_storage(storage_item)?);
            }
        } else {
            // Default storage
            storage_list.push(Storage {
                name: "default".to_string(),
                quantity: Some(ResourceValue {
                    val: "1073741824".to_string().into_bytes(), // 1Gi default
                }),
                attributes: vec![],
            });
        }

        Ok(storage_list)
    }

    /// Parse a single storage entry.
    fn parse_single_storage(&self, storage: &serde_yaml::Value) -> Result<Storage> {
        let name = storage
            .get("name")
            .and_then(|n| n.as_str())
            .unwrap_or("default")
            .to_string();

        let size_str = storage
            .get("size")
            .and_then(|s| s.as_str())
            .unwrap_or("1Gi");

        let size = self.parse_memory_size(&serde_yaml::Value::String(size_str.to_string()))?;

        let mut attributes = Vec::new();

        // Parse storage class/type if specified
        if let Some(attr_section) = storage.get("attributes") {
            if let Some(class) = attr_section.get("class").and_then(|c| c.as_str()) {
                attributes.push(Attribute {
                    key: "class".to_string(),
                    value: class.to_string(),
                });
            }
            if let Some(persistent) = attr_section.get("persistent").and_then(|p| p.as_bool()) {
                attributes.push(Attribute {
                    key: "persistent".to_string(),
                    value: persistent.to_string(),
                });
            }
        }

        Ok(Storage {
            name,
            quantity: Some(ResourceValue {
                val: size.to_string().into_bytes(),
            }),
            attributes,
        })
    }

    /// Parse GPU resources.
    fn parse_gpu(&self, resources: &serde_yaml::Value) -> Result<Option<Gpu>> {
        let gpu_section = match resources.get("gpu") {
            Some(g) => g,
            None => return Ok(None),
        };

        let units = gpu_section
            .get("units")
            .and_then(|u| u.as_u64())
            .unwrap_or(0);

        if units == 0 {
            return Ok(None);
        }

        let mut attributes = Vec::new();

        // Parse GPU attributes with composite keys (vendor/model/ram format)
        // Example: vendor/nvidia/model/h100/ram/80Gi
        // SDL format:
        //   vendor:
        //     nvidia:
        //       - model: h100
        //         ram: 80Gi
        if let Some(attrs) = gpu_section.get("attributes") {
            if let Some(vendor_section) = attrs.get("vendor") {
                if let Some(vendor_map) = vendor_section.as_mapping() {
                    for (vendor_name, vendor_config) in vendor_map {
                        let vendor = vendor_name.as_str().unwrap_or("nvidia");

                        // Parse models array
                        if let Some(models) = vendor_config.as_sequence() {
                            for model_entry in models {
                                if let Some(model_map) = model_entry.as_mapping() {
                                    // Extract model name and ram from the mapping
                                    let model_name = model_map
                                        .get(serde_yaml::Value::String("model".to_string()))
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("");

                                    let ram = model_map
                                        .get(serde_yaml::Value::String("ram".to_string()))
                                        .and_then(|v| v.as_str());

                                    if !model_name.is_empty() {
                                        // Build composite key
                                        let mut key =
                                            format!("vendor/{}/model/{}", vendor, model_name);

                                        // Add ram if specified
                                        if let Some(ram_value) = ram {
                                            key.push_str(&format!("/ram/{}", ram_value));
                                        }

                                        tracing::debug!("  Adding GPU attribute: {} = true", key);
                                        attributes.push(Attribute {
                                            key,
                                            value: "true".to_string(),
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        tracing::debug!("  Total GPU attributes: {}", attributes.len());
        for attr in &attributes {
            tracing::debug!("    - {}: {}", attr.key, attr.value);
        }

        Ok(Some(Gpu {
            units: Some(ResourceValue {
                val: units.to_string().into_bytes(),
            }),
            attributes,
        }))
    }

    /// Parse endpoints from service definition.
    fn parse_endpoints(
        &self,
        services: &serde_yaml::Value,
        service_name: &str,
    ) -> Result<Vec<Endpoint>> {
        let mut endpoints = Vec::new();

        let service = match services.get(service_name) {
            Some(s) => s,
            None => return Ok(endpoints),
        };

        if let Some(expose) = service.get("expose") {
            if let Some(expose_array) = expose.as_sequence() {
                for (idx, expose_item) in expose_array.iter().enumerate() {
                    let _port = expose_item
                        .get("port")
                        .and_then(|p| p.as_u64())
                        .unwrap_or(80) as u32;

                    let _proto = expose_item
                        .get("proto")
                        .and_then(|p| p.as_str())
                        .unwrap_or("TCP");

                    // Check if globally exposed
                    let global = expose_item
                        .get("to")
                        .and_then(|t| t.as_sequence())
                        .map(|arr| arr.iter().any(|item| item.get("global").is_some()))
                        .unwrap_or(false);

                    let kind = if global {
                        1 // SHARED_HTTP or RANDOM_PORT
                    } else {
                        0 // INTERNAL
                    };

                    endpoints.push(Endpoint {
                        kind,
                        sequence_number: idx as u32,
                    });
                }
            }
        }

        Ok(endpoints)
    }

    /// Parse placement requirements.
    fn parse_placement_requirements(
        &self,
        placement_profiles: Option<&serde_yaml::Value>,
        placement_name: &str,
        _compute_profile: &serde_yaml::Value,
    ) -> Result<PlacementRequirements> {
        let mut attributes = Vec::new();

        // Get signed by requirements (if any)
        let signed_by = if let Some(profiles) = placement_profiles {
            if let Some(placement) = profiles.get(placement_name) {
                // Parse attributes
                if let Some(attrs) = placement.get("attributes") {
                    if let Some(attr_map) = attrs.as_mapping() {
                        for (key, value) in attr_map {
                            if let (Some(k), Some(v)) = (key.as_str(), value.as_str()) {
                                attributes.push(Attribute {
                                    key: k.to_string(),
                                    value: v.to_string(),
                                });
                            }
                        }
                    }
                }

                // Parse signedBy if present
                placement.get("signedBy").and_then(|sb| {
                    let any_of: Vec<String> = sb
                        .get("anyOf")
                        .and_then(|a| a.as_sequence())
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                                .collect()
                        })
                        .unwrap_or_default();

                    let all_of: Vec<String> = sb
                        .get("allOf")
                        .and_then(|a| a.as_sequence())
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                                .collect()
                        })
                        .unwrap_or_default();

                    if any_of.is_empty() && all_of.is_empty() {
                        None
                    } else {
                        Some(SignedBy { any_of, all_of })
                    }
                })
            } else {
                None
            }
        } else {
            None
        };

        // NOTE: GPU attributes are handled in the GPU resource itself, not in placement requirements
        // The valid deployment message shows requirements.attributes = null when GPU is specified

        Ok(PlacementRequirements {
            signed_by,
            attributes,
        })
    }

    /// Parse price from placement profile.
    /// Returns price as DecCoin with Cosmos SDK decimal format (18 decimal places).
    fn parse_price(
        &self,
        placement_profiles: Option<&serde_yaml::Value>,
        placement_name: &str,
        service_name: &str,
    ) -> Result<DecCoin> {
        // Default price: 1000000 uakt
        let mut amount_base = 1_000_000u64;
        let mut denom = "uakt".to_string();

        if let Some(profiles) = placement_profiles {
            if let Some(placement) = profiles.get(placement_name) {
                if let Some(pricing) = placement.get("pricing") {
                    if let Some(service_price) = pricing.get(service_name) {
                        // Get denom
                        if let Some(d) = service_price.get("denom").and_then(|d| d.as_str()) {
                            denom = d.to_string();
                        }
                        // Get amount
                        if let Some(amt) = service_price.get("amount") {
                            if let Some(n) = amt.as_u64() {
                                amount_base = n;
                            } else if let Some(s) = amt.as_str() {
                                amount_base = s.parse().unwrap_or(amount_base);
                            }
                        }
                    }
                }
            }
        }

        // Use integer string format (decimal format causes simulation error)
        Ok(DecCoin {
            denom,
            amount: amount_base.to_string(),
        })
    }
}

#[cfg(test)]
mod deccoin_tests {
    use super::*;
    use prost::Message;

    #[test]
    fn test_deccoin_formats_comparison() {
        let amount_base = 1_000_000u64;
        let denom = "uakt";

        println!("\n=== DecCoin Format Comparison ===\n");

        // 1. cosmwasm_std::DecCoin
        let cosmwasm_deccoin = cosmwasm_std::DecCoin::new(
            Decimal::from_ratio(amount_base, 1u64),
            denom,
        );
        println!("1. cosmwasm_std::DecCoin:");
        println!("   - amount field: {}", cosmwasm_deccoin.amount);
        println!("   - JSON: {}",cosmwasm_deccoin.amount.to_string());

        // 2. Our proto DecCoin (ho_std)
        let proto_deccoin = DecCoin {
            denom: denom.to_string(),
            amount: cosmwasm_deccoin.amount.to_string(),
        };
        println!("\n2. Proto DecCoin (ho_std):");
        println!("   - amount field: {}", proto_deccoin.amount);
        println!("   - JSON: {}", serde_json::to_string(&proto_deccoin).unwrap());

        // Encode to protobuf bytes
        let proto_bytes = proto_deccoin.encode_to_vec();
        println!("   - Protobuf bytes (hex): {}", hex::encode(&proto_bytes));
        println!("   - Protobuf size: {} bytes", proto_bytes.len());

        // 3. Proto DecCoin with integer format
        let proto_deccoin_int = DecCoin {
            denom: denom.to_string(),
            amount: amount_base.to_string(),
        };
        println!("\n3. Proto DecCoin (integer format):");
        println!("   - amount field: {}", proto_deccoin_int.amount);
        println!("   - JSON: {}", serde_json::to_string(&proto_deccoin_int).unwrap());

        let proto_int_bytes = proto_deccoin_int.encode_to_vec();
        println!("   - Protobuf bytes (hex): {}", hex::encode(&proto_int_bytes));
        println!("   - Protobuf size: {} bytes", proto_int_bytes.len());

        // 4. Proto DecCoin with full decimal format
        let proto_deccoin_decimal = DecCoin {
            denom: denom.to_string(),
            amount: format!("{}.000000000000000000", amount_base),
        };
        println!("\n4. Proto DecCoin (full decimal format):");
        println!("   - amount field: {}", proto_deccoin_decimal.amount);
        println!("   - JSON: {}", serde_json::to_string(&proto_deccoin_decimal).unwrap());

        let proto_decimal_bytes = proto_deccoin_decimal.encode_to_vec();
        println!("   - Protobuf bytes (hex): {}", hex::encode(&proto_decimal_bytes));
        println!("   - Protobuf size: {} bytes", proto_decimal_bytes.len());

        // Compare byte sizes
        println!("\n=== Size Comparison ===");
        println!("Integer format:        {} bytes", proto_int_bytes.len());
        println!("cosmwasm_std format:   {} bytes", proto_bytes.len());
        println!("Full decimal format:   {} bytes", proto_decimal_bytes.len());

        // Check if they decode properly
        println!("\n=== Decode Test ===");
        match DecCoin::decode(&proto_decimal_bytes[..]) {
            Ok(decoded) => {
                println!("Full decimal format decodes successfully:");
                println!("   - denom: {}", decoded.denom);
                println!("   - amount: {}", decoded.amount);
            }
            Err(e) => println!("Full decimal format decode FAILED: {}", e),
        }
    }

    #[test]
    fn test_parse_price_output() {
        let builder = DeploymentBuilder::new("akash1test", 1);

        // Create minimal SDL YAML with pricing
        let sdl = r#"
version: "2.0"
services:
  test:
    image: nginx:1.25.3
    expose:
      - port: 80
        to:
          - global: true

profiles:
  compute:
    test:
      resources:
        cpu:
          units: 1
        memory:
          size: 512Mi
        storage:
          size: 1Gi
  placement:
    dcloud:
      pricing:
        test:
          denom: uakt
          amount: 1000000.000000000000000000

deployment:
  test:
    dcloud:
      profile: test
      count: 1
"#;

        let yaml: serde_yaml::Value = serde_yaml::from_str(sdl).unwrap();
        let placement_profiles = yaml.get("profiles").and_then(|p| p.get("placement"));

        let price = builder.parse_price(placement_profiles, "dcloud", "test").unwrap();

        println!("\n=== parse_price() Output ===");
        println!("denom: {}", price.denom);
        println!("amount: {}", price.amount);
        println!("amount length: {} chars", price.amount.len());

        // Check format
        assert_eq!(price.denom, "uakt");
        println!("Format check: {}",
            if price.amount.contains('.') {
                "DECIMAL format (has decimal point)"
            } else {
                "INTEGER format (no decimal point)"
            }
        );
    }
}

/// Build MsgCreateLease from bid info.
pub fn build_create_lease_msg(
    owner: &str,
    dseq: u64,
    gseq: u32,
    oseq: u32,
    provider: &str,
) -> MsgCreateLease {
    MsgCreateLease {
        bid_id: Some(BidId {
            owner: owner.to_string(),
            dseq,
            gseq,
            oseq,
            provider: provider.to_string(),
        }),
    }
}

/// Build MsgCloseDeployment.
pub fn build_close_deployment_msg(owner: &str, dseq: u64) -> MsgCloseDeployment {
    MsgCloseDeployment {
        id: Some(DeploymentId {
            owner: owner.to_string(),
            dseq,
        }),
    }
}

/// Build MsgUpdateDeployment.
pub fn build_update_deployment_msg(
    owner: &str,
    dseq: u64,
    hash: Vec<u8>,
) -> ho_std::types::ergors::akash::deployment::v1beta5::MsgUpdateDeployment {
    use ho_std::types::ergors::akash::deployment::v1beta5::MsgUpdateDeployment;

    MsgUpdateDeployment {
        id: Some(DeploymentId {
            owner: owner.to_string(),
            dseq,
        }),
        hash,
    }
}

/// Build MsgAccountDeposit for topping up escrow.
pub fn build_escrow_deposit_msg(
    signer: &str,
    owner: &str,
    dseq: u64,
    amount_uakt: u64,
) -> Result<ho_std::types::ergors::akash::escrow::v1::MsgAccountDeposit> {
    use ho_std::types::ergors::{
        akash::base::deposit::v1::{Deposit, Source},
        akash::escrow::{id::v1 as escrow_id, v1::MsgAccountDeposit},
        cosmos::base::v1beta1::Coin,
    };

    Ok(MsgAccountDeposit {
        signer: signer.to_string(),
        id: Some(escrow_id::Account {
            scope: escrow_id::Scope::Deployment as i32,
            xid: format!("{}/{}", owner, dseq),
        }),
        deposit: Some(Deposit {
            amount: Some(Coin {
                denom: "uakt".to_string(),
                amount: amount_uakt.to_string(),
            }),
            sources: vec![Source::Balance as i32],
        }),
    })
}

/// Get next available dseq by querying account's deployment count.
pub async fn get_next_dseq(rest_endpoint: &str, _owner: &str) -> Result<u64> {
    // Query current block height to use as dseq
    let client = reqwest::Client::new();
    let url = format!(
        "{}/cosmos/base/tendermint/v1beta1/blocks/latest",
        rest_endpoint.trim_end_matches('/')
    );

    let response = client.get(&url).send().await?;

    if !response.status().is_success() {
        return Err(anyhow!("Failed to query latest block"));
    }

    let json: serde_json::Value = response.json().await?;

    let height = json
        .get("block")
        .and_then(|b| b.get("header"))
        .and_then(|h| h.get("height"))
        .and_then(|h| h.as_str())
        .and_then(|s| s.parse::<u64>().ok())
        .ok_or_else(|| anyhow!("Failed to parse block height"))?;

    // Use block height as dseq (standard Akash practice)
    Ok(height)
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
    fn test_parse_memory_size() {
        let builder = DeploymentBuilder::new("akash1test", 1);

        assert_eq!(
            builder
                .parse_memory_size(&serde_yaml::Value::String("512Mi".to_string()))
                .unwrap(),
            536_870_912
        );
        assert_eq!(
            builder
                .parse_memory_size(&serde_yaml::Value::String("1Gi".to_string()))
                .unwrap(),
            1_073_741_824
        );
        assert_eq!(
            builder
                .parse_memory_size(&serde_yaml::Value::String("256Ki".to_string()))
                .unwrap(),
            262_144
        );
    }

    #[test]
    fn test_build_from_sdl() {
        let builder = DeploymentBuilder::new("akash1testowner", 12345);
        let msg = builder.build_from_sdl(SAMPLE_SDL).unwrap();

        assert!(msg.id.is_some());
        let id = msg.id.as_ref().unwrap();
        assert_eq!(id.owner, "akash1testowner");
        assert_eq!(id.dseq, 12345);

        assert!(!msg.groups.is_empty());
        // assert_eq!(msg.deposit.as_ref().unwrap(), 1);
    }

    #[test]
    fn test_build_create_lease_msg() {
        let msg = build_create_lease_msg("akash1owner", 12345, 1, 1, "akash1provider");

        let bid_id = msg.bid_id.as_ref().unwrap();
        assert_eq!(bid_id.owner, "akash1owner");
        assert_eq!(bid_id.dseq, 12345);
        assert_eq!(bid_id.gseq, 1);
        assert_eq!(bid_id.oseq, 1);
        assert_eq!(bid_id.provider, "akash1provider");
    }
}
