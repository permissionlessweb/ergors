//! Build ERGORS proto files. This build script uses the local proto files
//! in the ergors/ directory to build the required proto types for the ERGORS system.
//! This is adapted from the proto-compiler code in github.com/informalsystems/ibc-rs

use std::path::PathBuf;

const SERDE_JSON: &str = "#[derive(serde::Serialize, serde::Deserialize)]";
fn main() -> anyhow::Result<()> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    println!("root: {}", root.display());

    let target_dir = root
        .join("..")
        .join("packages")
        .join("ho-std")
        .join("src")
        .join("types")
        .join("ergors")
        .join("gen");

    println!("target_dir: {}", target_dir.display());

    // prost_build::Config isn't Clone, so we need to make two.
    let mut config = prost_build::Config::new();

    // As recommended in pbjson_types docs.
    config.extern_path(".google.protobuf", "::pbjson_types");
    // NOTE: we need this because the rust module that defines the IBC types is external, and not
    // part of this crate.
    // See https://docs.rs/prost-build/0.5.0/prost_build/struct.Config.html#method.extern_path
    config.extern_path(".ibc", "::ibc_proto::ibc");

    config.extern_path(".ics23", "::ics23");
    config.extern_path(".cosmos.ics23", "::ics23");

    // config.extern_path(".cosmos.bank", "::ibc_proto::cosmos::bank");
    // config.extern_path(".cosmos.staking", "::ibc_proto::cosmos::staking");
    // config.extern_path(".cosmos.tx", "::ibc_proto::cosmos::tx");
    // config.extern_path(".cosmos.auth", "::ibc_proto::cosmos::auth");
    // config.extern_path(".cosmos.app", "::ibc_proto::cosmos::app");
    // config.extern_path(".cosmos.crisis", "::ibc_proto::cosmos::crisis");
    // config.extern_path(".cosmos.distribution", "::ibc_proto::cosmos::distribution");
    // config.extern_path(".cosmos.evidence", "::ibc_proto::cosmos::evidence");
    // config.extern_path(".cosmos.feegrant", "::ibc_proto::cosmos::feegrant");
    // config.extern_path(".cosmos.genutil", "::ibc_proto::cosmos::genutil");
    // config.extern_path(".cosmos.gov", "::ibc_proto::cosmos::gov");
    // config.extern_path(".cosmos.group", "::ibc_proto::cosmos::group");
    // config.extern_path(".cosmos.mint", "::ibc_proto::cosmos::mint");
    // config.extern_path(".cosmos.nft", "::ibc_proto::cosmos::nft");
    // config.extern_path(".cosmos.orm", "::ibc_proto::cosmos::orm");
    // config.extern_path(".cosmos.params", "::ibc_proto::cosmos::params");
    // config.extern_path(".cosmos.slashing", "::ibc_proto::cosmos::slashing");
    // config.extern_path(".cosmos.upgrade", "::ibc_proto::cosmos::upgrade");
    // config.extern_path(".cosmos.vesting", "::ibc_proto::cosmos::vesting");
    // config.extern_path(".cosmos.capability", "::ibc_proto::cosmos::capability");
    // config.extern_path(".cosmos.consensus", "::ibc_proto::cosmos::consensus");
    // config.extern_path(".cosmos.circuit", "::ibc_proto::cosmos::circuit");
    // config.extern_path(".cosmos.reflection", "::ibc_proto::cosmos::reflection");
    // config.extern_path(".cosmos.authz", "::ibc_proto::cosmos::authz");
    // config.extern_path(".tendermint", "::tendermint_proto::tendermint");
    // config.extern_path(".cosmos_proto", "::cosmos_proto");

    config.compile_well_known_types();
    config.type_attribute(".", SERDE_JSON);

    config
        .out_dir(&target_dir)
        // .file_descriptor_set_path(&target_dir.join(descriptor_file_name))
        .enable_type_names();

    let rpc_doc_attr = r#"#[cfg(feature = "rpc")]"#;

    tonic_prost_build::configure()
        .out_dir(&target_dir)
        .emit_rerun_if_changed(false)
        // Only in Tonic 0.10
        //.generate_default_stubs(true)
        // We need to feature-gate the RPCs.
        .server_mod_attribute(".", rpc_doc_attr)
        .client_mod_attribute(".", rpc_doc_attr)
        .compile_with_config(
            config,
            &[
                "./ergors/actions/v1/actions.proto",
                "./ergors/asset/v1/asset.proto",
                "./ergors/custody/v1/custody.proto",
                "./ergors/git/v1/git.proto",
                "./ergors/decaf377_frost/v1/decaf377_frost.proto",
                "./ergors/decaf377_rdsa/v1/decaf377_rdsa.proto",
                "./ergors/decaf377_fmd/v1/decaf377_fmd.proto",
                "./ergors/keys/v1/keys.proto",
                "./ergors/management/v1/management.proto",
                "./ergors/network/v1/network.proto",
                "./ergors/view/v1/view.proto",
                "./ergors/orch/v1/orch.proto",
                "./ergors/proxy/v1/proxy.proto",
                "./ergors/storage/v1/storage.proto",
                // "./ergors/sct/v1/sct.proto",
                "./ergors/tct/v1/tct.proto",
                "./ergors/types/v1/common.proto",
                "./headstash/headstash/v1/headstash.proto",
                "./headstash/extendo/v1/extendo.proto",
                "./rust-vendored/tendermint/p2p/types.proto",
                "./rust-vendored/tendermint/abci/types.proto",
                "./rust-vendored/tendermint/types/validator.proto",
                "./rust-vendored/ibc/applications/transfer/v1/query.proto",
                "./rust-vendored/ibc/core/channel/v1/query.proto",
                "./rust-vendored/cosmos/bank/v1beta1/bank.proto",
                "./rust-vendored/cosmos/bank/v1beta1/query.proto",
                "./rust-vendored/cosmos/bank/v1beta1/tx.proto",
                "./rust-vendored/cosmwasm/wasm/v1/authz.proto",
                "./rust-vendored/cosmwasm/wasm/v1/genesis.proto",
                "./rust-vendored/cosmwasm/wasm/v1/ibc.proto",
                "./rust-vendored/cosmwasm/wasm/v1/proposal_legacy.proto",
                "./rust-vendored/cosmwasm/wasm/v1/query.proto",
                "./rust-vendored/cosmwasm/wasm/v1/tx.proto",
                "./rust-vendored/cosmwasm/wasm/v1/types.proto",
                "./rust-vendored/ibc/core/client/v1/query.proto",
                "./rust-vendored/ibc/core/connection/v1/query.proto",
                // akash network
                //
                // certificate
                "./ergors/akash/cert/v1/cert.proto",
                "./ergors/akash/cert/v1/filters.proto",
                "./ergors/akash/cert/v1/genesis.proto",
                "./ergors/akash/cert/v1/msg.proto",
                "./ergors/akash/cert/v1/query.proto",
                "./ergors/akash/cert/v1/service.proto",
                // deployment
                "./ergors/akash/deployment/v1/deployment.proto",
                "./ergors/akash/deployment/v1/event.proto",
                "./ergors/akash/deployment/v1/group.proto",
                "./ergors/akash/deployment/v1beta4/deploymentmsg.proto",
                "./ergors/akash/deployment/v1beta4/filters.proto",
                "./ergors/akash/deployment/v1beta4/genesis.proto",
                "./ergors/akash/deployment/v1beta4/group.proto",
                "./ergors/akash/deployment/v1beta4/groupmsg.proto",
                "./ergors/akash/deployment/v1beta4/groupspec.proto",
                "./ergors/akash/deployment/v1beta4/params.proto",
                "./ergors/akash/deployment/v1beta4/paramsmsg.proto",
                "./ergors/akash/deployment/v1beta4/query.proto",
                "./ergors/akash/deployment/v1beta4/resourceunit.proto",
                "./ergors/akash/deployment/v1beta4/service.proto",
                "./ergors/akash/deployment/v1beta5/deploymentmsg.proto",
                "./ergors/akash/deployment/v1beta5/filters.proto",
                "./ergors/akash/deployment/v1beta5/group.proto",
                "./ergors/akash/deployment/v1beta5/groupmsg.proto",
                "./ergors/akash/deployment/v1beta5/groupspec.proto",
                "./ergors/akash/deployment/v1beta5/params.proto",
                "./ergors/akash/deployment/v1beta5/query.proto",
                "./ergors/akash/deployment/v1beta5/resourceunit.proto",
                "./ergors/akash/deployment/v1beta5/service.proto",
                // discovery
                "./ergors/akash/discovery/v1/akash.proto",
                "./ergors/akash/discovery/v1/client_info.proto",
                // escrow
                "./ergors/akash/escrow/v1/authz.proto",
                "./ergors/akash/escrow/v1/genesis.proto",
                "./ergors/akash/escrow/v1/msg.proto",
                "./ergors/akash/escrow/v1/query.proto",
                "./ergors/akash/escrow/v1/service.proto",
                // inventory
                "./ergors/akash/inventory/v1/cluster.proto",
                "./ergors/akash/inventory/v1/cpu.proto",
                "./ergors/akash/inventory/v1/gpu.proto",
                "./ergors/akash/inventory/v1/memory.proto",
                "./ergors/akash/inventory/v1/node.proto",
                "./ergors/akash/inventory/v1/resourcepair.proto",
                "./ergors/akash/inventory/v1/resources.proto",
                "./ergors/akash/inventory/v1/service.proto",
                "./ergors/akash/inventory/v1/storage.proto",
                // manifest
                "./ergors/akash/manifest/v2beta3/group.proto",
                "./ergors/akash/manifest/v2beta3/httpoptions.proto",
                "./ergors/akash/manifest/v2beta3/service.proto",
                "./ergors/akash/manifest/v2beta3/serviceexpose.proto",
                // market
                "./ergors/akash/market/v1/bid.proto",
                "./ergors/akash/market/v1/event.proto",
                "./ergors/akash/market/v1/filters.proto",
                "./ergors/akash/market/v1/lease.proto",
                "./ergors/akash/market/v1/order.proto",
                "./ergors/akash/market/v1/types.proto",
                "./ergors/akash/market/v1beta5/bid.proto",
                "./ergors/akash/market/v1beta5/bidmsg.proto",
                "./ergors/akash/market/v1beta5/filters.proto",
                "./ergors/akash/market/v1beta5/genesis.proto",
                "./ergors/akash/market/v1beta5/leasemsg.proto",
                "./ergors/akash/market/v1beta5/order.proto",
                "./ergors/akash/market/v1beta5/params.proto",
                "./ergors/akash/market/v1beta5/paramsmsg.proto",
                "./ergors/akash/market/v1beta5/query.proto",
                "./ergors/akash/market/v1beta5/resourcesoffer.proto",
                "./ergors/akash/market/v1beta5/service.proto",
                "./ergors/akash/market/v1beta4/bid.proto",
                "./ergors/akash/market/v1beta4/genesis.proto",
                "./ergors/akash/market/v1beta4/lease.proto",
                "./ergors/akash/market/v1beta4/order.proto",
                "./ergors/akash/market/v1beta4/params.proto",
                "./ergors/akash/market/v2beta1/bid.proto",
                "./ergors/akash/market/v2beta1/bidmsg.proto",
                "./ergors/akash/market/v2beta1/event.proto",
                "./ergors/akash/market/v2beta1/filters.proto",
                "./ergors/akash/market/v2beta1/genesis.proto",
                "./ergors/akash/market/v2beta1/lease.proto",
                "./ergors/akash/market/v2beta1/leasemsg.proto",
                "./ergors/akash/market/v2beta1/order.proto",
                "./ergors/akash/market/v2beta1/params.proto",
                "./ergors/akash/market/v2beta1/paramsmsg.proto",
                "./ergors/akash/market/v2beta1/query.proto",
                "./ergors/akash/market/v2beta1/resourcesoffer.proto",
                "./ergors/akash/market/v2beta1/service.proto",
                "./ergors/akash/market/v2beta1/types.proto",
                // provider
                "./ergors/akash/provider/v1/service.proto",
                "./ergors/akash/provider/v1/status.proto",
                "./ergors/akash/provider/v1beta3/genesis.proto",
                "./ergors/akash/provider/v1beta3/provider.proto",
                "./ergors/akash/provider/v1beta4/event.proto",
                "./ergors/akash/provider/v1beta4/genesis.proto",
                "./ergors/akash/provider/v1beta4/msg.proto",
                "./ergors/akash/provider/v1beta4/provider.proto",
                "./ergors/akash/provider/v1beta4/query.proto",
                "./ergors/akash/provider/v1beta4/service.proto",
                // manifest

                // inventory
            ],
            &["./headstash/", "./ergors/", "./rust-vendored/", "./vendor/"],
        )?;

    // "./rust-vendored/cosmwasm/wasm/v1/authz.proto",
    // "./rust-vendored/cosmwasm/wasm/v1/genesis.proto",
    // "./rust-vendored/cosmwasm/wasm/v1/ibc.proto",
    // "./rust-vendored/cosmwasm/wasm/v1/proposal_legacy.proto",
    // "./rust-vendored/cosmwasm/wasm/v1/query.proto",
    // "./rust-vendored/cosmwasm/wasm/v1/tx.proto",
    // "./rust-vendored/cosmwasm/wasm/v1/types.proto",

    // Finally, build pbjson Serialize, Deserialize impls:
    // let descriptor_set = std::fs::read(target_dir.join(descriptor_file_name))?;

    pbjson_build::Builder::new()
        // .register_descriptors(&descriptor_set)?
        .ignore_unknown_fields()
        .out_dir(&target_dir)
        .build(&["."])?;

    // Post-process generated files to remove serde and problematic derives from types
    use std::fs;

    // Types that have serde derives mixed with other derives in the same #[derive(...)]
    // The post-processor will remove serde traits from these derives
    let types_to_remove_serde_from_mixed_derives = [
        "QueryCertificatesRequest",
        "QueryCertificatesResponse",
        "QueryDeploymentsResponse",
        "QueryDeploymentsRequest",
        "Params",
        "ResourceUnit",
        "MsgCreateDeployment",
        "MsgDepositDeployment",
        "GroupSpec",
        "Account",
        "FractionalPayment",
        "Order",
        "MsgCreateBid",
        "Bid",
        "Lease",
        "QueryOrdersRequest",
        "QueryOrdersResponse",
        "QueryBidsRequest",
        "QueryBidsResponse",
        "QueryLeasesRequest",
        "QueryLeasesResponse",
    ];

    // Types that have a separate #[derive(serde::Serialize, serde::Deserialize)] line
    // The post-processor will remove the entire separate serde derive line
    let types_to_remove_separate_serde_derive = ["SctFrontierResponse"];

    for entry in walkdir::WalkDir::new(&target_dir)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if entry.path().extension().is_some_and(|ext| ext == "rs") {
            let content = fs::read_to_string(entry.path())?;
            let lines: Vec<&str> = content.lines().collect();
            let mut new_lines: Vec<String> = Vec::new();
            let mut i = 0;

            while i < lines.len() {
                let line = lines[i];

                // Check if this is a type with serde in mixed derives (same #[derive(...)] line)
                let mut is_mixed_derive_type = false;
                for type_name in &types_to_remove_serde_from_mixed_derives {
                    if line.contains(&format!("struct {}", type_name))
                        || line.contains(&format!("enum {}", type_name))
                    {
                        is_mixed_derive_type = true;
                        break;
                    }
                }

                // Check if this is a type with separate serde derive line
                let mut is_separate_derive_type = false;
                for type_name in &types_to_remove_separate_serde_derive {
                    if line.contains(&format!("struct {}", type_name))
                        || line.contains(&format!("enum {}", type_name))
                    {
                        is_separate_derive_type = true;
                        break;
                    }
                }

                // Handle types with serde mixed in the same derive line
                if is_mixed_derive_type && i > 0 {
                    let prev_line = lines[i - 1];
                    if prev_line.contains("#[derive(") && prev_line.contains("serde::Serialize") {
                        // Remove serde from the derive
                        let new_derive = prev_line
                            .replace(", serde::Serialize", "")
                            .replace("serde::Serialize, ", "")
                            .replace(", serde::Deserialize", "")
                            .replace("serde::Deserialize, ", "")
                            .replace("serde::Serialize", "")
                            .replace("serde::Deserialize", "");

                        if new_derive.contains("#[derive()]") || new_derive == "#[derive" {
                            // Remove the derive line entirely if empty
                            new_lines.pop();
                        } else {
                            // Replace the last added derive line with the cleaned version
                            new_lines.pop();
                            new_lines.push(new_derive);
                        }
                    }
                }

                // Handle types with separate serde derive line
                // Example:
                //   #[derive(serde::Serialize, serde::Deserialize)]  <- Remove this entire line
                //   #[derive(Clone, PartialEq, ::prost::Message)]    <- Keep this
                //   pub struct SctFrontierResponse {
                if is_separate_derive_type && i > 1 {
                    let prev_line = lines[i - 1];
                    let prev_prev_line = lines[i - 2];

                    // Check if i-2 is a serde-only derive and i-1 is a non-serde derive
                    if prev_prev_line.contains("#[derive(")
                        && (prev_prev_line.contains("serde::Serialize")
                            || prev_prev_line.contains("serde::Deserialize"))
                        && prev_line.contains("#[derive(")
                        && !prev_line.contains("serde::")
                    {
                        // Remove the serde derive line (second-to-last in new_lines)
                        if new_lines.len() >= 2 {
                            let last = new_lines.pop().unwrap(); // Pop non-serde derive
                            new_lines.pop(); // Pop serde derive (discard)
                            new_lines.push(last); // Push non-serde derive back
                        }
                    }
                }

                // Check for types containing ibc_proto::cosmos::base::v1beta1::Coin
                // and remove Hash, Eq derives
                if line.contains("#[derive(") && line.contains("Hash") && i < lines.len() - 1 {
                    // Look ahead to find the struct/enum and check if it contains Coin
                    let mut j = i + 1;
                    let mut found_struct = false;
                    let mut struct_start = 0;
                    let mut struct_end = 0;

                    while j < lines.len() {
                        let next_line = lines[j];
                        if next_line.contains("struct ") || next_line.contains("enum ") {
                            found_struct = true;
                            struct_start = j;
                            break;
                        }
                        j += 1;
                    }

                    if found_struct {
                        // Find the end of the struct
                        let mut brace_count = 0;
                        let mut k = struct_start;
                        while k < lines.len() {
                            let struct_line = lines[k];
                            if struct_line.contains("{") {
                                brace_count += struct_line.matches("{").count();
                            }
                            if struct_line.contains("}") {
                                brace_count -= struct_line.matches("}").count();
                            }
                            if brace_count == 0 && k > struct_start {
                                struct_end = k;
                                break;
                            }
                            k += 1;
                        }

                        // Check if the struct contains problematic cosmos types
                        let mut has_problematic_type = false;
                        for l in struct_start..=struct_end {
                            if lines[l].contains("ibc_proto::cosmos::base::v1beta1::Coin")
                                || lines[l].contains("ibc_proto::cosmos::base::v1beta1::DecCoin")
                                || lines[l]
                                    .contains("ibc_proto::ibc::core::commitment::v1::MerkleProof")
                                || lines[l].contains(
                                    "ibc_proto::cosmos::base::query::v1beta1::PageRequest",
                                )
                                || lines[l].contains(
                                    "ibc_proto::cosmos::base::query::v1beta1::PageResponse",
                                )
                            {
                                has_problematic_type = true;
                                break;
                            }
                        }

                        if has_problematic_type && line.contains("Hash") {
                            // Remove Hash and Eq from the derive
                            let new_derive = line
                                .replace(", Hash", "")
                                .replace("Hash, ", "")
                                .replace(" Eq,", "")
                                .replace("Hash", "");

                            new_lines.push(new_derive);
                            i += 1;
                            continue;
                        }
                    }
                }

                new_lines.push(line.to_string());
                i += 1;
            }

            let new_content = new_lines.join("\n");
            if new_content != content {
                fs::write(entry.path(), new_content)?;
            }
        }
    }

    Ok(())
}
