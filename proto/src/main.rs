//! Build CW-HO proto files. This build script uses the local proto files
//! in the hoe/ directory to build the required proto types for the CW-HO system.
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
        .join("cw_ho")
        .join("gen");

    println!("target_dir: {}", target_dir.display());

    // prost_build::Config isn't Clone, so we need to make two.
    let mut config = prost_build::Config::new();

    config.compile_well_known_types();
    // As recommended in pbjson_types docs.
    config.extern_path(".google.protobuf", "::pbjson_types");

    config.type_attribute(".", SERDE_JSON);
    // config.type_attribute("hoe.orchestration.v1.HoConfig", SERDE_JSON);
    // config.type_attribute("hoe.network.v1.NetworkConfig", SERDE_JSON);
    // config.type_attribute("hoe.network.v1.NodeIdentity", SERDE_JSON);
    // config.type_attribute("hoe.orchestration.v1.StorageConfig", SERDE_JSON);
    // config.type_attribute("hoe.orchestration.v1.ApiKeysJson", SERDE_JSON);
    // config.type_attribute("hoe.orchestration.v1.GlobalSettings", SERDE_JSON);
    // config.type_attribute("hoe.orchestration.v1.ApiKeysMetadata", SERDE_JSON);
    // config.type_attribute("hoe.orchestration.v1.OpenAiRequest", SERDE_JSON);
    // config.type_attribute("hoe.orchestration.v1.OpenAiResponse", SERDE_JSON);
    // config.type_attribute("hoe.orchestration.v1.OpenAiChoice", SERDE_JSON);
    // config.type_attribute("hoe.orchestration.v1.OpenAiUsage", SERDE_JSON);
    // config.type_attribute("hoe.orchestration.v1.OpenAiMessage", SERDE_JSON);
    // config.type_attribute("hoe.orchestration.v1.ProviderWithAuth", SERDE_JSON);
    // config.type_attribute("hoe.orchestration.v1.Instructions", SERDE_JSON);
    // config.type_attribute("hoe.orchestration.v1.LlmRouterConfig", SERDE_JSON);
    // config.type_attribute("hoe.orchestration.v1.LlmEntity", SERDE_JSON);
    // config.type_attribute("hoe.orchestration.v1.PromptRequest", SERDE_JSON);
    // config.type_attribute("hoe.orchestration.v1.PromptResponse", SERDE_JSON);
    // config.type_attribute("hoe.orchestration.v1.TokenUsage", SERDE_JSON);
    // config.type_attribute("hoe.orchestration.v1.PromptMessage", SERDE_JSON);
    // config.type_attribute("hoe.orchestration.v1.PromptContext", SERDE_JSON);
    // config.type_attribute("hoe.orchestration.v1.LlmPromptConfig", SERDE_JSON);
    // config.type_attribute("hoe.orchestration.v1.HealthResponse", SERDE_JSON);
    // config.type_attribute("hoe.network.v1.NodeInfo", SERDE_JSON);
    // config.type_attribute("hoe.network.v1.NetworkMessage", SERDE_JSON);
    // config.type_attribute("hoe.network.v1.NetworkLimits", SERDE_JSON);
    // config.type_attribute("hoe.network.v1.ChannelConfig", SERDE_JSON);
    // config.type_attribute("hoe.network.v1.NodeAnnounce", SERDE_JSON);
    // config.type_attribute("hoe.network.v1.TaskCoordination", SERDE_JSON);
    // config.type_attribute("hoe.network.v1.FractalSync", SERDE_JSON);
    // config.type_attribute("hoe.network.v1.SandloopState", SERDE_JSON);
    // config.type_attribute("hoe.network.v1.TetrahedralPing", SERDE_JSON);
    // config.type_attribute("hoe.network.v1.Request", SERDE_JSON);
    // config.type_attribute("hoe.network.v1.Response", SERDE_JSON);
    // config.type_attribute("hoe.network.v1.NetworkEvent", SERDE_JSON);
    // config.type_attribute("hoe.network.v1.PeerConnected", SERDE_JSON);
    // config.type_attribute("hoe.network.v1.PeerDisconnected", SERDE_JSON);
    // config.type_attribute("hoe.network.v1.MessageReceived", SERDE_JSON);
    // config.type_attribute("hoe.network.v1.TopologyChanged", SERDE_JSON);
    // config.type_attribute("hoe.network.v1.NetworkError", SERDE_JSON);
    // config.type_attribute("hoe.network.v1.NetworkTopology", SERDE_JSON);
    // config.type_attribute("hoe.network.v1.Connection", SERDE_JSON);
    // config.type_attribute("hoe.types.v1.FractalOperation", SERDE_JSON);
    // config.type_attribute("hoe.types.v1.InsertOperation", SERDE_JSON);
    // config.type_attribute("hoe.types.v1.UpdateOperation", SERDE_JSON);
    // config.type_attribute("hoe.types.v1.DeleteOperation", SERDE_JSON);
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
                "./hoe/network/v1/network.proto",
                "./hoe/orchestration/v1/orchestration.proto",
                "./hoe/storage/v1/storage.proto",
                "./hoe/types/v1/common.proto",
            ],
            &["./hoe/"],
        )?;

    // Finally, build pbjson Serialize, Deserialize impls:
    // let descriptor_set = std::fs::read(target_dir.join(descriptor_file_name))?;

    pbjson_build::Builder::new()
        // .register_descriptors(&descriptor_set)?
        .ignore_unknown_fields()
        .out_dir(&target_dir)
        .build(&["."])?;

    // std::fs::read_dir(&target_dir)?
    //     .filter_map(|entry| entry.ok())
    //     .filter(|entry| entry.path().extension().map_or(false, |ext| ext == "rs"))
    //     .for_each(|entry| {
    //         let path = entry.path();
    //         let contents = std::fs::read_to_string(&path).unwrap();
    //         let patched = contents.replace(
    //             "#[derive(Clone, PartialEq, ::prost::Oneof)]",
    //             "#[derive(Clone, PartialEq, ::prost::Oneof, serde::Serialize, serde::Deserialize)]",
    //         );
    //         std::fs::write(path, patched).unwrap();
    //     });

    Ok(())
}
