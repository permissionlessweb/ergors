use camino::Utf8Path;

use crate::constants::DATA_FOLDER_NAME;
use crate::prelude::StorageConfig;

impl StorageConfig {
    pub fn new(data_dir: &Utf8Path) -> Self {
        let mut memories = Self::default();
        memories.data_dir = data_dir.join(DATA_FOLDER_NAME).to_string();
        memories
    }
}
