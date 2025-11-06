//! Build CW-HO proto files. This build script uses the local proto files
//! in the hoe/ directory to build the required proto types for the CW-HO system.
//! This is adapted from the proto-compiler code in github.com/informalsystems/ibc-rs

use std::{env, path::PathBuf};

use cw_hoe_proto::code_generator::{CodeGenerator, ProtoProject};

// All paths must end with a / and either be absolute or include a ./ to reference the current
// working directory.

/// The directory generated ho proto files go into in this repo  
const OUT_DIR: &str = "../packages/ho-std/src/types/";
/// Directory where the ho proto files are located
const HO_PROTO_DIR: &str = "./";
/// A temporary directory for proto building
const TMP_BUILD_DIR: &str = "/tmp/tmp-ho-protobuf/";

pub fn generate() {
    let tmp_build_dir: PathBuf = TMP_BUILD_DIR.parse().unwrap();
    let out_dir: PathBuf = OUT_DIR.parse().unwrap();

    let ho_project = ProtoProject {
        name: "cw_ho".to_string(),
        version: "v1".to_string(),
        project_dir: HO_PROTO_DIR.to_string(),
        exclude_mods: vec![],
    };

    let ho_code_generator = CodeGenerator::new(out_dir, tmp_build_dir, ho_project, vec![]);

    ho_code_generator.generate();
}

fn main() {
    pretty_env_logger::init();
    // TODO: PromptResponse needs serialize/deserialize
    // TODO: enable utopia utoipa = "5" derived attributes
    generate();
}
