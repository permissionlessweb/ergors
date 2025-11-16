impl crate::types::storage::v1::StorageConfig {
    pub fn new(data_dir: &camino::Utf8Path) -> Self {
        let mut memories = Self::default();
        memories.data_dir = data_dir
            .join(crate::constants::DATA_FOLDER_NAME)
            .to_string();
        memories
    }
}
