//! Test setup and teardown utilities
//!
//! Provides common setup patterns and test context management, including
//! real Cnidarium storage initialization for integration tests.

use anyhow::Result;
use std::path::PathBuf;
use std::sync::Arc;
use tempfile::TempDir;

// Re-exports for integration tests
pub use ergors::storage::ErgorsStorage;

/// All storage prefixes used by ERGORS - required for Cnidarium initialization
/// NOTE: Prefixes must NOT have trailing slashes. Cnidarium adds the delimiter internally.
/// NOTE: Longer/more specific prefixes MUST come before shorter ones (e.g. "sessions_by_parent" before "sessions")
///       because cnidarium's find_substore does a linear search with starts_with matching.
pub const STORAGE_PREFIXES: &[&str] = &[
    // Session-related prefixes (longer ones first to prevent "sessions" from matching all)
    "sessions_by_parent",
    "sessions_by_root",
    "sessions_by_owner",
    "sessions_by_status",
    "sessions_by_type",
    "sessions_by_label",
    "sessions_by_tag",
    "session_states",
    "session_locks",
    "sessions",
    // Proxy session prefixes (longer first)
    "proxy_sessions_by_client",
    "proxy_sessions",
    "proxy_router_config",
    // Worktree prefixes (longer first)
    "worktrees_by_workspace",
    "worktrees_by_node",
    // Authenticator prefixes (longer first)
    "authenticators/metadata",
    "authenticators",
    // Custody prefixes (longer first)
    "custody/api_keys",
    "custody",
    // Other prefixes (no ordering conflicts)
    "prompts",
    "users",
    "timestamps",
    "operations",
    "akash_workflows",
    "workspaces",
    "task_worktrees",
    "fractal_sessions",
    "open_responses",
    "sdl_template_contracts",
    // RAG prefixes
    "rag_source_index",
    "rag_chunks",
];

/// Test context encapsulating temporary directories and configuration
pub struct TestContext {
    /// Temporary directory for test data
    pub temp_dir: TempDir,
    /// Storage directory path
    pub storage_path: PathBuf,
    /// Config directory path
    pub config_path: PathBuf,
    /// Test name for logging
    pub test_name: String,
}

impl TestContext {
    /// Create a new test context with a given test name
    pub fn new(test_name: impl Into<String>) -> Result<Self> {
        let test_name = test_name.into();
        let temp_dir = tempfile::tempdir()?;
        let storage_path = temp_dir.path().join("storage");
        let config_path = temp_dir.path().join("config");

        // Create subdirectories
        std::fs::create_dir_all(&storage_path)?;
        std::fs::create_dir_all(&config_path)?;

        tracing::info!("Created test context for '{}'", test_name);
        tracing::debug!("Temp dir: {:?}", temp_dir.path());

        Ok(Self {
            temp_dir,
            storage_path,
            config_path,
            test_name,
        })
    }

    /// Get a path for a test file within the temp directory
    pub fn test_file(&self, name: &str) -> PathBuf {
        self.temp_dir.path().join(name)
    }

    /// Clean up the test context (called automatically on drop)
    pub fn cleanup(&self) {
        tracing::info!("Cleaning up test context for '{}'", self.test_name);
    }
}

impl Drop for TestContext {
    fn drop(&mut self) {
        self.cleanup();
    }
}

/// Initialize tracing for tests
pub fn init_test_tracing() {
    use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

    let _ = tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "debug,cnidarium=warn,jmt=warn".into()),
        )
        .with(tracing_subscriber::fmt::layer().with_test_writer())
        .try_init();
}

/// Setup REAL test storage with Cnidarium.
///
/// This creates an actual Cnidarium-backed ErgorsStorage instance in a temporary
/// directory, suitable for integration tests that need to verify real storage
/// behavior including:
/// - Atomic commits via StateDelta
/// - Prefix-based indexing and queries
/// - Snapshot isolation
/// - Persistence across operations
pub async fn setup_test_storage(ctx: &TestContext) -> Result<Arc<ErgorsStorage>> {
    tracing::info!("Setting up REAL Cnidarium storage at {:?}", ctx.storage_path);

    let prefixes: Vec<String> = STORAGE_PREFIXES.iter().map(|s| s.to_string()).collect();

    let storage = ErgorsStorage::new(&ctx.storage_path, prefixes).await?;

    tracing::info!("Cnidarium storage initialized successfully");
    Ok(Arc::new(storage))
}

/// Integration test harness with all real components.
///
/// This struct provides a complete test environment with real ERGORS components:
/// - Real Cnidarium-backed storage
/// - Temp directory for isolation
/// - Ready-to-use storage reference
///
/// The harness automatically cleans up on drop.
pub struct IntegrationTestHarness {
    /// Test context with temp directories
    pub ctx: TestContext,
    /// Real ErgorsStorage with Cnidarium backend
    pub storage: Arc<ErgorsStorage>,
}

impl IntegrationTestHarness {
    /// Create a new integration test harness.
    ///
    /// This initializes real Cnidarium storage in a temporary directory.
    pub async fn new(test_name: impl Into<String>) -> Result<Self> {
        let ctx = TestContext::new(test_name)?;
        let storage = setup_test_storage(&ctx).await?;

        Ok(Self { ctx, storage })
    }

    /// Get reference to storage for test operations.
    pub fn storage(&self) -> &Arc<ErgorsStorage> {
        &self.storage
    }

    /// Get the test context for additional setup.
    pub fn context(&self) -> &TestContext {
        &self.ctx
    }
}

/// Teardown helper for async tests
pub async fn teardown() {
    tracing::debug!("Test teardown complete");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_context_creation() {
        let ctx = TestContext::new("test_example").unwrap();
        assert!(ctx.storage_path.exists());
        assert!(ctx.config_path.exists());
        assert_eq!(ctx.test_name, "test_example");
    }

    #[test]
    fn test_context_test_file() {
        let ctx = TestContext::new("test_file_example").unwrap();
        let test_file = ctx.test_file("test.txt");
        assert!(test_file.starts_with(ctx.temp_dir.path()));
        assert!(test_file.ends_with("test.txt"));
    }

    #[test]
    fn test_storage_prefixes_complete() {
        // Ensure we have all necessary prefixes (without trailing slashes)
        assert!(STORAGE_PREFIXES.contains(&"prompts"));
        assert!(STORAGE_PREFIXES.contains(&"fractal_sessions"));
        assert!(STORAGE_PREFIXES.contains(&"akash_workflows"));
        assert!(STORAGE_PREFIXES.len() >= 25); // Sanity check
    }

    #[tokio::test]
    async fn test_real_storage_initialization() {
        init_test_tracing();
        let harness = IntegrationTestHarness::new("storage_init_test").await.unwrap();

        // Verify storage is accessible
        let storage = harness.storage();

        // Health check should pass
        let health = storage.health_check().await;
        assert!(health.is_ok(), "Storage health check failed: {:?}", health);
    }
}
