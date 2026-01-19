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
                "./ergors/decaf377_frost/v1/decaf377_frost.proto",
                "./ergors/decaf377_rdsa/v1/decaf377_rdsa.proto",
                "./ergors/decaf377_fmd/v1/decaf377_fmd.proto",
                "./ergors/keys/v1/keys.proto",
                "./ergors/network/v1/network.proto",
                "./ergors/view/v1/view.proto",
                "./ergors/orch/v1/orch.proto",
                "./ergors/storage/v1/storage.proto",
                "./ergors/sct/v1/sct.proto",
                "./ergors/tct/v1/tct.proto",
                "./ergors/types/v1/common.proto",
                "./headstash/headstash/v1/headstash.proto",
                "./headstash/extendo/v1/extendo.proto",
                "./rust-vendored/tendermint/p2p/types.proto",
                "./rust-vendored/tendermint/abci/types.proto",
                "./rust-vendored/tendermint/types/validator.proto",
                "./rust-vendored/ibc/applications/transfer/v1/query.proto",
                "./rust-vendored/ibc/core/channel/v1/query.proto",
                "./rust-vendored/ibc/core/client/v1/query.proto",
                "./rust-vendored/ibc/core/connection/v1/query.proto",
                "./ergors/akash/node/akash/deployment/v1beta3/deployment.proto",
                "./ergors/akash/node/akash/deployment/v1beta3/deploymentmsg.proto",
                "./ergors/akash/node/akash/deployment/v1beta3/service.proto",
                "./ergors/akash/node/akash/deployment/v1beta3/query.proto",
                "./ergors/akash/node/akash/deployment/v1beta3/group.proto",
                "./ergors/akash/node/akash/deployment/v1beta3/groupid.proto",
                "./ergors/akash/node/akash/deployment/v1beta3/groupspec.proto",
                "./ergors/akash/node/akash/deployment/v1beta3/groupmsg.proto",
                "./ergors/akash/node/akash/deployment/v1beta3/resourceunit.proto",
                "./ergors/akash/node/akash/deployment/v1beta3/params.proto",
                "./ergors/akash/node/akash/escrow/v1beta3/types.proto",
                "./ergors/akash/node/akash/market/v1beta4/service.proto",
                "./ergors/akash/node/akash/market/v1beta4/query.proto",
                "./ergors/akash/node/akash/market/v1beta4/bid.proto",
                "./ergors/akash/node/akash/market/v1beta4/lease.proto",
                "./ergors/akash/node/akash/market/v1beta4/order.proto",
                "./ergors/akash/node/akash/cert/v1beta3/query.proto",
                "./ergors/akash/node/akash/cert/v1beta3/cert.proto",
                "./ergors/akash/provider/akash/provider/lease/v1/service.proto",
            ],
            &["./headstash/", "./ergors/", "./rust-vendored/"],
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

    let types_to_remove_serde = [
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
        "SctFrontierResponse",
    ];

    for entry in walkdir::WalkDir::new(&target_dir)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if entry.path().extension().map_or(false, |ext| ext == "rs") {
            let content = fs::read_to_string(entry.path())?;
            let lines: Vec<&str> = content.lines().collect();
            let mut new_lines = Vec::new();
            let mut i = 0;

            while i < lines.len() {
                let line = lines[i];

                // Check if this line defines one of the types we want to modify
                let mut is_target_type = false;
                for type_name in &types_to_remove_serde {
                    if line.contains(&format!("struct {}", type_name))
                        || line.contains(&format!("enum {}", type_name))
                    {
                        is_target_type = true;
                        break;
                    }
                }

                if is_target_type && i > 0 {
                    // Check if the previous line is a derive with serde
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

                        if new_derive.contains("#[derive()") || new_derive == "#[derive" {
                            // Remove the derive line entirely if empty
                            new_lines.pop(); // Remove the derive line
                        } else {
                            new_lines[i - 1] = new_derive;
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
