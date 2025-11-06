use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::llm::HoResult;

pub trait IdGeneratorTrait {}

pub trait ConfigLoaderTrait {
    fn from_toml_file<T: DeserializeOwned, P: AsRef<Path>>(path: P) -> HoResult<T>;
    fn to_toml_file<T: Serialize, P: AsRef<Path>>(config: &T, path: P) -> HoResult<()>;
    fn from_json_file<T: DeserializeOwned, P: AsRef<Path>>(path: P) -> HoResult<T>;
    fn to_json_file<T: Serialize, P: AsRef<Path>>(config: &T, path: P) -> HoResult<()>;
    fn load_api_keys(path: &str) -> HoResult<HashMap<String, String>>;
}

pub trait FileOptsTrait {
    fn read_string<P: AsRef<Path>>(path: P) -> HoResult<String>;
    fn write_string<P: AsRef<Path>>(path: P, content: &str) -> HoResult<()>;
    fn size<P: AsRef<Path>>(path: P) -> HoResult<u64>;
    fn create_dir_all<P: AsRef<Path>>(path: P) -> HoResult<()>;
    fn list_files<P: AsRef<Path>>(path: P, content: Option<&str>) -> HoResult<Vec<PathBuf>>;
    fn exists<P: AsRef<Path>>(path: P) -> bool;
}

pub trait FileShareTraits {
    fn share_file<P: AsRef<Path>>(path: P) -> HoResult<()>;
    fn save_file<P: AsRef<Path>>(path: P, content: &str) -> HoResult<()>;
    fn backup_file<P: AsRef<Path>>(path: P, content: &str) -> HoResult<()>;
    fn sync_files<P: AsRef<Path>>(path: P, content: &str) -> HoResult<()>;
}
