//! Bootstrap workflow integration tests
//!
//! Tests the bootstrap system for deploying engine instances on Akash.
//! Verifies config generation, state machine transitions, P2P framing protocol,
//! and the bootstrap receiver detection logic.
//!
//! These tests exercise the ho-std bootstrap module (config_gen, framing, transport)
//! and the ergors bootstrap module (state_machine, receiver) without requiring
//! live infrastructure or P2P connections.

use ho_std::bootstrap::{
    BootstrapConfigGenerator, FileChunk, FileChunker, ChunkMetadata, NodeBootstrapParams,
    MAX_CHUNK_SIZE,
};
use ho_std::bootstrap::{BootstrapFileMessage, FileType};
use ho_std::types::ergors::network::v1::NodeType;

use ergors::bootstrap::{BootstrapState, BootstrapStep, StepResult};

// ============================================================================
// Test 1: Bootstrap Config Generation
// ============================================================================

/// Verify that a bootstrap password is 64 hex characters (32 bytes).
#[test]
fn test_bootstrap_password_generation_length() {
    let pwd = BootstrapConfigGenerator::generate_bootstrap_password();
    assert_eq!(pwd.len(), 64, "Password must be 64 hex characters (32 bytes)");
    // Verify it is valid hex
    assert!(
        hex::decode(&pwd).is_ok(),
        "Password must be valid hexadecimal"
    );
}

/// Verify that each generated bootstrap password is unique (random).
#[test]
fn test_bootstrap_password_uniqueness() {
    let pwd1 = BootstrapConfigGenerator::generate_bootstrap_password();
    let pwd2 = BootstrapConfigGenerator::generate_bootstrap_password();
    assert_ne!(pwd1, pwd2, "Each bootstrap password must be unique");
}

/// Verify that NodeBootstrapParams default values are sane.
#[test]
fn test_node_bootstrap_params_defaults() {
    let params = NodeBootstrapParams::default();
    assert_eq!(params.node_type, NodeType::Executor);
    assert_eq!(params.host, "0.0.0.0");
    assert_eq!(params.p2p_port, 26969);
    assert_eq!(params.api_port, 8080);
    assert_eq!(params.ssh_port, 22);
    assert!(params.bootstrap_peers.is_empty());
    assert!(params.custody_password.is_empty());
}

/// Verify that config generation fails with empty custody password.
/// This is a security guard -- we must never generate custody with empty password.
#[tokio::test]
async fn test_config_generation_rejects_empty_password() {
    let params = NodeBootstrapParams {
        custody_password: String::new(),
        ..Default::default()
    };

    let generator = BootstrapConfigGenerator::new();
    let result = generator.generate_node_config(params).await;
    assert!(
        result.is_err(),
        "Config generation must fail with empty custody password"
    );
}

/// Verify that a valid config is generated with proper password.
/// Checks identity, network config, and custody data are all present.
#[tokio::test]
async fn test_config_generation_produces_complete_config() {
    let params = NodeBootstrapParams {
        node_type: NodeType::Executor,
        host: "10.0.0.1".to_string(),
        p2p_port: 26969,
        api_port: 8080,
        ssh_port: 22,
        bootstrap_peers: vec!["peer1@1.2.3.4:26969".to_string()],
        custody_password: "test_password_strong_enough".to_string(),
    };

    let generator = BootstrapConfigGenerator::new();
    let config = generator
        .generate_node_config(params)
        .await
        .expect("Config generation should succeed with valid params");

    // Verify identity
    assert!(
        config.identity.public_key.is_some(),
        "Identity must have a public key"
    );
    assert_eq!(config.identity.node_type, "EXECUTOR");
    assert_eq!(config.identity.p2p_port, 26969);
    assert_eq!(config.identity.api_port, 8080);
    assert_eq!(config.identity.host, "10.0.0.1");

    // Verify network config has bootstrap peers
    assert_eq!(config.network.bootstrap_peers.len(), 1);
    assert_eq!(config.network.bootstrap_peers[0], "peer1@1.2.3.4:26969");

    // Verify custody data is non-empty (encrypted key material)
    assert!(
        !config.custody_data.is_empty(),
        "Custody data must be non-empty"
    );
}

/// Verify that config can be serialized to TOML format.
#[tokio::test]
async fn test_config_serializes_to_valid_toml() {
    let params = NodeBootstrapParams {
        custody_password: "toml_test_password_123".to_string(),
        bootstrap_peers: vec!["seed@5.6.7.8:26969".to_string()],
        ..Default::default()
    };

    let generator = BootstrapConfigGenerator::new();
    let config = generator.generate_node_config(params).await.unwrap();
    let toml_str = generator
        .to_toml(&config)
        .expect("TOML serialization must succeed");

    // Verify TOML is parseable
    let parsed: toml::Value = toml::from_str(&toml_str).expect("Output must be valid TOML");
    assert!(
        parsed.get("identity").is_some(),
        "TOML must contain [identity] section"
    );
    assert!(
        parsed.get("network").is_some(),
        "TOML must contain [network] section"
    );

    // Verify key fields present in the TOML
    assert!(
        toml_str.contains("node_type"),
        "TOML must include node_type field"
    );
    assert!(
        toml_str.contains("p2p_port"),
        "TOML must include p2p_port field"
    );
    assert!(
        toml_str.contains("bootstrap_peers"),
        "TOML must include bootstrap_peers field"
    );
}

// ============================================================================
// Test 2: Bootstrap State Machine
// ============================================================================

/// Verify initial state of a new BootstrapState.
#[test]
fn test_bootstrap_state_initial_values() {
    let state = BootstrapState::new("session-001".to_string(), NodeType::Executor);

    assert_eq!(state.session_id, "session-001");
    assert_eq!(state.step, BootstrapStep::Init);
    assert_eq!(state.target_node_type, NodeType::Executor);
    assert!(!state.is_terminal());
    assert!(!state.is_complete());
    assert!(!state.is_failed());
    assert!(state.errors.is_empty());
    assert!(state.docker_image_tag.is_none());
    assert!(state.generated_identity_pubkey.is_none());
    assert!(state.config_toml.is_none());
    assert!(state.custody_data.is_none());
    assert!(!state.p2p_connected);
}

/// Verify state transitions update the step and timestamp.
#[test]
fn test_bootstrap_state_transitions_update_timestamp() {
    let mut state = BootstrapState::new("ts-test".to_string(), NodeType::Executor);
    let initial_updated = state.updated_at;

    // Small delay to ensure timestamp changes
    std::thread::sleep(std::time::Duration::from_millis(10));

    state.transition(BootstrapStep::GenerateIdentity);
    assert_eq!(state.step, BootstrapStep::GenerateIdentity);
    assert!(
        state.updated_at > initial_updated,
        "updated_at must advance on transition"
    );
}

/// Verify the full happy-path step progression through the state machine.
#[test]
fn test_bootstrap_state_machine_happy_path() {
    let mut state = BootstrapState::new("happy-path".to_string(), NodeType::Executor);

    let steps = [
        BootstrapStep::GenerateIdentity,
        BootstrapStep::BuildDockerImage,
        BootstrapStep::CreateAkashDeployment,
        BootstrapStep::WaitForDeployment,
        BootstrapStep::EstablishP2PConnection,
        BootstrapStep::SendConfig,
        BootstrapStep::SendCustody,
        BootstrapStep::SendApiKeys,
        BootstrapStep::VerifyNodeOnline,
        BootstrapStep::Complete,
    ];

    for step in &steps {
        state.transition(step.clone());
        assert_eq!(&state.step, step);
    }

    assert!(state.is_complete());
    assert!(state.is_terminal());
    assert!(!state.is_failed());
}

/// Verify that fail() transitions to Failed state with a reason.
#[test]
fn test_bootstrap_state_fail_captures_reason() {
    let mut state = BootstrapState::new("fail-test".to_string(), NodeType::Coordinator);

    state.transition(BootstrapStep::BuildDockerImage);
    state.fail("Docker build failed: image not found".to_string());

    assert!(state.is_failed());
    assert!(state.is_terminal());
    assert!(!state.is_complete());
    assert_eq!(state.errors.len(), 1);
    assert!(state.errors[0].contains("Docker build failed"));

    match &state.step {
        BootstrapStep::Failed { reason } => {
            assert_eq!(reason, "Docker build failed: image not found");
        }
        other => panic!("Expected Failed step, got: {:?}", other),
    }
}

/// Verify that add_error accumulates errors without changing step.
#[test]
fn test_bootstrap_state_add_error_accumulates() {
    let mut state = BootstrapState::new("errors-test".to_string(), NodeType::Executor);
    state.transition(BootstrapStep::WaitForDeployment);

    state.add_error("Timeout waiting for deployment".to_string());
    state.add_error("Retry attempt 2 failed".to_string());
    state.add_error("Retry attempt 3 failed".to_string());

    assert_eq!(state.errors.len(), 3);
    assert_eq!(state.step, BootstrapStep::WaitForDeployment);
    assert!(!state.is_terminal(), "Errors alone do not make state terminal");
}

/// Verify the status_string() display method produces readable output.
#[test]
fn test_bootstrap_state_status_string() {
    let mut state = BootstrapState::new("display-test".to_string(), NodeType::Executor);

    assert_eq!(state.status_string(), "Initializing");

    state.transition(BootstrapStep::GenerateIdentity);
    assert_eq!(state.status_string(), "Generating identity");

    state.transition(BootstrapStep::SendConfig);
    assert_eq!(state.status_string(), "Sending configuration");

    state.transition(BootstrapStep::Complete);
    assert_eq!(state.status_string(), "Complete");

    // Reset and test failed
    let mut state2 = BootstrapState::new("display-fail".to_string(), NodeType::Executor);
    state2.fail("Something broke".to_string());
    assert!(state2.status_string().contains("Failed"));
    assert!(state2.status_string().contains("Something broke"));
}

/// Verify that bootstrap state tracks optional fields correctly.
#[test]
fn test_bootstrap_state_optional_fields() {
    let mut state = BootstrapState::new("optional-test".to_string(), NodeType::Executor);

    // Set optional fields as they would be during a real bootstrap
    state.docker_image_tag = Some("ergors:v0.1.0".to_string());
    state.generated_identity_pubkey = Some("abcdef0123456789".to_string());
    state.config_toml = Some("[identity]\nnode_type = \"EXECUTOR\"".to_string());
    state.custody_data = Some(vec![1, 2, 3, 4, 5]);
    state.custody_password = Some("temp_bootstrap_pwd".to_string());
    state.akash_session_id = Some("akash-session-123".to_string());
    state.akash_dseq = Some(12345);
    state.akash_provider = Some("akash1provider".to_string());
    state.akash_endpoints = vec!["https://provider:8443".to_string()];
    state.p2p_connected = true;
    state.bootstrap_peer = Some("coordinator@1.2.3.4:26969".to_string());

    assert_eq!(state.docker_image_tag.as_deref(), Some("ergors:v0.1.0"));
    assert_eq!(state.akash_dseq, Some(12345));
    assert!(state.p2p_connected);
    assert_eq!(state.akash_endpoints.len(), 1);
}

// ============================================================================
// Test 3: P2P Framing Protocol
// ============================================================================

/// Verify that a small file produces a single chunk with metadata.
#[test]
fn test_chunker_small_file_produces_single_chunk() {
    let data = b"small config file content here";
    let chunks = FileChunker::chunk(data, Some("config.toml".to_string()));

    assert_eq!(chunks.len(), 1);
    assert_eq!(chunks[0].sequence, 0);
    assert_eq!(chunks[0].total_chunks, 1);
    assert_eq!(chunks[0].data, data.to_vec());

    let metadata = chunks[0].metadata.as_ref().expect("First chunk must have metadata");
    assert_eq!(metadata.file_size, data.len());
    assert_eq!(metadata.file_name.as_deref(), Some("config.toml"));
}

/// Verify that a large file is split into multiple chunks correctly.
#[test]
fn test_chunker_large_file_splits_correctly() {
    let data = vec![0xABu8; MAX_CHUNK_SIZE + 500];
    let chunks = FileChunker::chunk(&data, Some("big_file.bin".to_string()));

    assert_eq!(chunks.len(), 2);
    assert_eq!(chunks[0].sequence, 0);
    assert_eq!(chunks[0].total_chunks, 2);
    assert_eq!(chunks[1].sequence, 1);
    assert_eq!(chunks[1].total_chunks, 2);

    // First chunk should be MAX_CHUNK_SIZE, second should be remainder
    assert_eq!(chunks[0].data.len(), MAX_CHUNK_SIZE);
    assert_eq!(chunks[1].data.len(), 500);

    // Only first chunk has metadata
    assert!(chunks[0].metadata.is_some());
    assert!(chunks[1].metadata.is_none());
}

/// Verify chunk -> reassemble round-trip for small data.
#[test]
fn test_chunker_reassemble_small_data_roundtrip() {
    let original = b"test data for chunking and reassembly verification";
    let chunks = FileChunker::chunk(original, Some("test.dat".to_string()));
    let reassembled = FileChunker::reassemble(chunks)
        .expect("Reassembly must succeed for valid chunks");

    assert_eq!(reassembled, original.to_vec());
}

/// Verify chunk -> reassemble round-trip for large multi-chunk data.
#[test]
fn test_chunker_reassemble_large_data_roundtrip() {
    let original = vec![0x42u8; MAX_CHUNK_SIZE * 3 + 777];
    let chunks = FileChunker::chunk(&original, None);
    assert_eq!(chunks.len(), 4);

    let reassembled =
        FileChunker::reassemble(chunks).expect("Reassembly must succeed for multi-chunk data");
    assert_eq!(reassembled, original);
}

/// Verify that out-of-order chunks are sorted and reassembled correctly.
#[test]
fn test_chunker_reassemble_handles_out_of_order() {
    let data = vec![0xFFu8; MAX_CHUNK_SIZE + 100];
    let mut chunks = FileChunker::chunk(&data, None);

    // Swap chunk order
    chunks.swap(0, 1);
    assert_eq!(chunks[0].sequence, 1, "Chunks should be swapped");

    // Reassembly should still work (it sorts internally)
    let reassembled = FileChunker::reassemble(chunks)
        .expect("Reassembly must handle out-of-order chunks");
    assert_eq!(reassembled, data);
}

/// Verify that reassembly fails when a chunk is missing.
#[test]
fn test_chunker_reassemble_fails_on_missing_chunk() {
    let data = vec![0x42u8; MAX_CHUNK_SIZE + 100];
    let mut chunks = FileChunker::chunk(&data, None);
    assert_eq!(chunks.len(), 2);

    // Remove last chunk
    chunks.pop();

    let result = FileChunker::reassemble(chunks);
    assert!(result.is_err(), "Reassembly must fail with missing chunk");
}

/// Verify that reassembly fails when chunk data is corrupted.
#[test]
fn test_chunker_reassemble_fails_on_corrupted_data() {
    let data = b"integrity check test data";
    let mut chunks = FileChunker::chunk(data, None);

    // Corrupt the data (flip a bit)
    chunks[0].data[0] ^= 0xFF;

    let result = FileChunker::reassemble(chunks);
    assert!(result.is_err(), "Reassembly must fail with corrupted chunk");
}

/// Verify that empty file produces one chunk and round-trips correctly.
#[test]
fn test_chunker_empty_file_roundtrip() {
    let data = b"";
    let chunks = FileChunker::chunk(data, None);

    assert_eq!(chunks.len(), 1);
    assert_eq!(chunks[0].data.len(), 0);

    let reassembled = FileChunker::reassemble(chunks).unwrap();
    assert_eq!(reassembled.len(), 0);
}

/// Verify that MAX_CHUNK_SIZE is 8MB (under commonware 10MB limit).
#[test]
fn test_max_chunk_size_is_under_commonware_limit() {
    assert_eq!(MAX_CHUNK_SIZE, 8 * 1024 * 1024);
    // Commonware channel limit is ~10MB, so 8MB with overhead should fit
    assert!(MAX_CHUNK_SIZE < 10 * 1024 * 1024);
}

// ============================================================================
// Test 4: Bootstrap File Message Encoding/Decoding
// ============================================================================

/// Verify BootstrapFileMessage encode/decode round-trip for Config type.
#[test]
fn test_file_message_config_roundtrip() {
    let msg = BootstrapFileMessage {
        file_type: FileType::Config,
        data: b"[identity]\nnode_type = \"EXECUTOR\"".to_vec(),
    };

    let encoded = msg.encode();
    let decoded = BootstrapFileMessage::decode(&encoded)
        .expect("Decode must succeed for valid encoded message");

    assert_eq!(decoded.file_type, FileType::Config);
    assert_eq!(decoded.data, b"[identity]\nnode_type = \"EXECUTOR\"");
}

/// Verify BootstrapFileMessage encode/decode round-trip for Custody type.
#[test]
fn test_file_message_custody_roundtrip() {
    let custody_bytes = vec![0xDE, 0xAD, 0xBE, 0xEF, 0x01, 0x02, 0x03, 0x04];
    let msg = BootstrapFileMessage {
        file_type: FileType::Custody,
        data: custody_bytes.clone(),
    };

    let encoded = msg.encode();
    let decoded = BootstrapFileMessage::decode(&encoded).unwrap();

    assert_eq!(decoded.file_type, FileType::Custody);
    assert_eq!(decoded.data, custody_bytes);
}

/// Verify BootstrapFileMessage encode/decode for Binary type.
#[test]
fn test_file_message_binary_roundtrip() {
    let binary_data = vec![0u8; 1024]; // 1KB of zeros
    let msg = BootstrapFileMessage {
        file_type: FileType::Binary,
        data: binary_data.clone(),
    };

    let encoded = msg.encode();
    let decoded = BootstrapFileMessage::decode(&encoded).unwrap();

    assert_eq!(decoded.file_type, FileType::Binary);
    assert_eq!(decoded.data.len(), 1024);
}

/// Verify BootstrapFileMessage encode/decode for Mnemonic type.
#[test]
fn test_file_message_mnemonic_roundtrip() {
    let mnemonic_bytes = b"abandon ability able about above absent".to_vec();
    let msg = BootstrapFileMessage {
        file_type: FileType::Mnemonic,
        data: mnemonic_bytes.clone(),
    };

    let encoded = msg.encode();
    let decoded = BootstrapFileMessage::decode(&encoded).unwrap();

    assert_eq!(decoded.file_type, FileType::Mnemonic);
    assert_eq!(decoded.data, mnemonic_bytes);
}

/// Verify that decoding a too-short message fails gracefully.
#[test]
fn test_file_message_decode_rejects_short_message() {
    let short = vec![1u8, 0, 0, 0]; // Only 4 bytes, need at least 5
    let result = BootstrapFileMessage::decode(&short);
    assert!(result.is_err(), "Must reject message shorter than header");
}

/// Verify that decoding a message with length mismatch fails.
#[test]
fn test_file_message_decode_rejects_length_mismatch() {
    // Header says 10 bytes of data, but we only provide 3
    let bad = vec![1u8, 0, 0, 0, 10, 1, 2, 3];
    let result = BootstrapFileMessage::decode(&bad);
    assert!(result.is_err(), "Must reject message with length mismatch");
}

/// Verify that decoding a message with invalid file type fails.
#[test]
fn test_file_message_decode_rejects_invalid_file_type() {
    // File type 99 is invalid
    let mut msg = BootstrapFileMessage {
        file_type: FileType::Config,
        data: b"test".to_vec(),
    };
    let mut encoded = msg.encode();
    encoded[0] = 99; // Corrupt file type byte

    let result = BootstrapFileMessage::decode(&encoded);
    assert!(result.is_err(), "Must reject invalid file type");
}

/// Verify FileType conversion from u8 values.
#[test]
fn test_file_type_from_u8_values() {
    assert_eq!(FileType::from_u8(1).unwrap(), FileType::Config);
    assert_eq!(FileType::from_u8(2).unwrap(), FileType::Custody);
    assert_eq!(FileType::from_u8(3).unwrap(), FileType::Binary);
    assert_eq!(FileType::from_u8(4).unwrap(), FileType::Mnemonic);
    assert!(FileType::from_u8(0).is_err());
    assert!(FileType::from_u8(5).is_err());
    assert!(FileType::from_u8(255).is_err());
}

// ============================================================================
// Test 5: Bootstrap Receiver Detection
// ============================================================================

/// Verify that is_bootstrap_mode returns true when config files are missing.
#[tokio::test]
async fn test_bootstrap_mode_detected_when_files_missing() {
    use ergors::bootstrap::BootstrapReceiver;

    let temp_dir = std::env::temp_dir().join(format!(
        "ergors_test_bootstrap_detect_{}",
        uuid::Uuid::new_v4()
    ));
    let _ = tokio::fs::remove_dir_all(&temp_dir).await;
    tokio::fs::create_dir_all(&temp_dir).await.unwrap();

    // No files exist yet -> bootstrap mode
    assert!(
        BootstrapReceiver::is_bootstrap_mode(temp_dir.to_str().unwrap()).await,
        "Should be in bootstrap mode when no config files exist"
    );

    // Create only config.toml -> still bootstrap mode (custody missing)
    tokio::fs::write(temp_dir.join("config.toml"), "# test config")
        .await
        .unwrap();
    assert!(
        BootstrapReceiver::is_bootstrap_mode(temp_dir.to_str().unwrap()).await,
        "Should be in bootstrap mode when only config.toml exists"
    );

    // Create identity.custody -> no longer bootstrap mode
    tokio::fs::write(temp_dir.join("identity.custody"), b"encrypted custody data")
        .await
        .unwrap();
    assert!(
        !BootstrapReceiver::is_bootstrap_mode(temp_dir.to_str().unwrap()).await,
        "Should NOT be in bootstrap mode when both files exist"
    );

    // Cleanup
    let _ = tokio::fs::remove_dir_all(&temp_dir).await;
}

/// Verify that is_bootstrap_mode returns true for a completely empty directory.
#[tokio::test]
async fn test_bootstrap_mode_empty_directory() {
    use ergors::bootstrap::BootstrapReceiver;

    let temp_dir = std::env::temp_dir().join(format!(
        "ergors_test_bootstrap_empty_{}",
        uuid::Uuid::new_v4()
    ));
    let _ = tokio::fs::remove_dir_all(&temp_dir).await;
    tokio::fs::create_dir_all(&temp_dir).await.unwrap();

    assert!(
        BootstrapReceiver::is_bootstrap_mode(temp_dir.to_str().unwrap()).await,
        "Empty directory should trigger bootstrap mode"
    );

    let _ = tokio::fs::remove_dir_all(&temp_dir).await;
}
