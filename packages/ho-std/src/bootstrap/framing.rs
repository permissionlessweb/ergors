//! File Chunking and Framing for Bootstrap Transfers
//!
//! Splits large files into authenticated chunks with integrity checks.
//! Commonware channels have message size limits (~10MB), so large files
//! (binaries, large configs) need to be chunked.

use crate::error::{HoError, HoResult};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Maximum chunk size (8MB to stay under commonware 10MB limit with overhead)
pub const MAX_CHUNK_SIZE: usize = 8 * 1024 * 1024;

/// File chunk with integrity check
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileChunk {
    /// Chunk sequence number (0-indexed)
    pub sequence: u32,
    /// Total number of chunks in the file
    pub total_chunks: u32,
    /// Chunk data
    pub data: Vec<u8>,
    /// SHA256 checksum of this chunk's data
    pub checksum: [u8; 32],
    /// Optional file metadata (only in first chunk)
    pub metadata: Option<ChunkMetadata>,
}

/// File metadata included in first chunk
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkMetadata {
    /// Original file size
    pub file_size: usize,
    /// Original file name (optional)
    pub file_name: Option<String>,
    /// SHA256 checksum of entire file
    pub file_checksum: [u8; 32],
}

/// File chunker for splitting large files
pub struct FileChunker;

impl FileChunker {
    /// Split file data into chunks
    ///
    /// Returns a vector of chunks, each with its own checksum.
    /// The first chunk includes file metadata.
    pub fn chunk(data: &[u8], file_name: Option<String>) -> Vec<FileChunk> {
        let total_size = data.len();

        // Calculate number of chunks needed
        let total_chunks = if total_size == 0 {
            1 // Empty file still needs one chunk
        } else {
            total_size.div_ceil(MAX_CHUNK_SIZE) as u32
        };

        // Calculate file checksum
        let mut hasher = Sha256::new();
        hasher.update(data);
        let file_checksum: [u8; 32] = hasher.finalize().into();

        let metadata = Some(ChunkMetadata {
            file_size: total_size,
            file_name,
            file_checksum,
        });

        let mut chunks = Vec::new();

        for i in 0..total_chunks {
            let start = (i as usize) * MAX_CHUNK_SIZE;
            let end = std::cmp::min(start + MAX_CHUNK_SIZE, total_size);
            let chunk_data = data[start..end].to_vec();

            // Calculate chunk checksum
            let mut chunk_hasher = Sha256::new();
            chunk_hasher.update(&chunk_data);
            let checksum: [u8; 32] = chunk_hasher.finalize().into();

            chunks.push(FileChunk {
                sequence: i,
                total_chunks,
                data: chunk_data,
                checksum,
                metadata: if i == 0 { metadata.clone() } else { None },
            });
        }

        chunks
    }

    /// Reassemble chunks into original file
    ///
    /// Validates:
    /// 1. All chunks are present and in sequence
    /// 2. Each chunk's checksum is valid
    /// 3. Final file checksum matches metadata
    pub fn reassemble(chunks: Vec<FileChunk>) -> HoResult<Vec<u8>> {
        if chunks.is_empty() {
            return Err(HoError::BootstrapError("No chunks provided".to_string()));
        }

        // Verify we have all chunks
        let total_chunks = chunks[0].total_chunks;
        if chunks.len() != total_chunks as usize {
            return Err(HoError::BootstrapError(format!(
                "Missing chunks: expected {}, got {}",
                total_chunks,
                chunks.len()
            )));
        }

        // Sort chunks by sequence FIRST (before accessing metadata)
        let mut sorted_chunks = chunks;
        sorted_chunks.sort_by_key(|c| c.sequence);

        // Get metadata from first chunk (after sorting)
        let metadata = sorted_chunks[0]
            .metadata
            .as_ref()
            .ok_or_else(|| HoError::BootstrapError("First chunk missing metadata".to_string()))?
            .clone();

        // Verify sequence continuity and checksums
        let mut reassembled = Vec::with_capacity(metadata.file_size);

        for (i, chunk) in sorted_chunks.iter().enumerate() {
            // Check sequence
            if chunk.sequence != i as u32 {
                return Err(HoError::BootstrapError(format!(
                    "Chunk sequence mismatch: expected {}, got {}",
                    i, chunk.sequence
                )));
            }

            // Verify chunk checksum
            let mut hasher = Sha256::new();
            hasher.update(&chunk.data);
            let computed_checksum: [u8; 32] = hasher.finalize().into();

            if computed_checksum != chunk.checksum {
                return Err(HoError::BootstrapError(format!(
                    "Chunk {} checksum mismatch",
                    i
                )));
            }

            // Append data
            reassembled.extend_from_slice(&chunk.data);
        }

        // Verify final file size
        if reassembled.len() != metadata.file_size {
            return Err(HoError::BootstrapError(format!(
                "File size mismatch: expected {}, got {}",
                metadata.file_size,
                reassembled.len()
            )));
        }

        // Verify final file checksum
        let mut hasher = Sha256::new();
        hasher.update(&reassembled);
        let computed_checksum: [u8; 32] = hasher.finalize().into();

        if computed_checksum != metadata.file_checksum {
            return Err(HoError::BootstrapError(
                "File checksum mismatch after reassembly".to_string(),
            ));
        }

        Ok(reassembled)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chunk_small_file() {
        let data = b"small file content";
        let chunks = FileChunker::chunk(data, Some("test.txt".to_string()));

        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].sequence, 0);
        assert_eq!(chunks[0].total_chunks, 1);
        assert_eq!(chunks[0].data, data);
        assert!(chunks[0].metadata.is_some());
    }

    #[test]
    fn test_chunk_large_file() {
        // Create a file larger than MAX_CHUNK_SIZE
        let data = vec![0x42u8; MAX_CHUNK_SIZE + 1000];
        let chunks = FileChunker::chunk(&data, None);

        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].total_chunks, 2);
        assert_eq!(chunks[1].total_chunks, 2);
        assert_eq!(chunks[0].data.len(), MAX_CHUNK_SIZE);
        assert_eq!(chunks[1].data.len(), 1000);
        assert!(chunks[0].metadata.is_some());
        assert!(chunks[1].metadata.is_none());
    }

    #[test]
    fn test_reassemble() {
        let original = b"test data for chunking and reassembly";
        let chunks = FileChunker::chunk(original, Some("test.dat".to_string()));
        let reassembled = FileChunker::reassemble(chunks).unwrap();

        assert_eq!(reassembled, original);
    }

    #[test]
    fn test_reassemble_out_of_order() {
        let data = vec![0x42u8; MAX_CHUNK_SIZE + 1000];
        let mut chunks = FileChunker::chunk(&data, None);

        // Swap chunk order
        chunks.swap(0, 1);

        // Should still work (reassemble sorts)
        let reassembled = FileChunker::reassemble(chunks).unwrap();
        assert_eq!(reassembled, data);
    }

    #[test]
    fn test_reassemble_missing_chunk() {
        let data = vec![0x42u8; MAX_CHUNK_SIZE + 1000];
        let mut chunks = FileChunker::chunk(&data, None);

        // Remove one chunk
        chunks.pop();

        let result = FileChunker::reassemble(chunks);
        assert!(result.is_err());
    }

    #[test]
    fn test_reassemble_corrupted_chunk() {
        let data = b"test data";
        let mut chunks = FileChunker::chunk(data, None);

        // Corrupt the data
        chunks[0].data[0] ^= 0xFF;

        let result = FileChunker::reassemble(chunks);
        assert!(result.is_err());
    }

    #[test]
    fn test_empty_file() {
        let data = b"";
        let chunks = FileChunker::chunk(data, None);

        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].data.len(), 0);

        let reassembled = FileChunker::reassemble(chunks).unwrap();
        assert_eq!(reassembled.len(), 0);
    }
}
