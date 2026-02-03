//! Integration tests for RLM service

#[cfg(test)]
mod tests {
    use ergors_rlm::{Document, LlmRouterTrait, RlmQuery};
    use ho_std::types::ergors::orch::v1::{PromptRequest, PromptResponse};
    use std::sync::Arc;
    use tokio::sync::RwLock;

    /// Mock LLM router for testing
    struct MockLlmRouter {
        call_count: Arc<RwLock<usize>>,
    }

    impl MockLlmRouter {
        fn new() -> Self {
            Self {
                call_count: Arc::new(RwLock::new(0)),
            }
        }

        async fn get_call_count(&self) -> usize {
            *self.call_count.read().await
        }
    }

    #[async_trait::async_trait]
    impl LlmRouterTrait for MockLlmRouter {
        async fn handle_request(
            &self,
            _req: &PromptRequest,
            _model: &str,
        ) -> anyhow::Result<PromptResponse> {
            let mut count = self.call_count.write().await;
            *count += 1;
            let call_num = *count;

            // First call: system prompt + query -> ask to explore context
            // Second call: exploration result -> provide final answer
            let response = if call_num == 1 {
                r#"Let me explore the documents to answer this query.

```python
# Check how many documents we have
print(f"Found {len(context)} documents")
for i, doc in enumerate(context):
    print(f"Document {i}: {doc['source_uri']}")
```
"#
                .to_string()
            } else {
                r#"Based on my exploration, I can now provide the final answer.

FINAL("The documents contain information about testing the RLM system. There are 2 test documents.")
"#
                .to_string()
            };

            Ok(PromptResponse {
                id: vec![],
                provider: "mock".to_string(),
                model: "test-model".to_string(),
                prompt: _req.messages.first().map(|m| m.content.clone()).unwrap_or_default(),
                response: vec![response],
                timestamp: None,
                tokens_used: None,
                cost: 0.0,
                latency_ms: 0,
                status: Some("completed".to_string()),
                output: vec![],
                response_metadata: None,
            })
        }
    }

    // Skip tests if Python is not available
    fn skip_if_no_python() -> bool {
        !std::process::Command::new("python3")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    #[tokio::test]
    async fn test_rlm_worker_spawn_and_ping() {
        if skip_if_no_python() {
            println!("Skipping test: Python 3 not available");
            return;
        }

        // Test that we can spawn a worker process
        let worker = ergors_rlm::process::ReplWorker::spawn(0);
        assert!(worker.is_ok(), "Failed to spawn worker: {:?}", worker.err());

        let worker = worker.unwrap();
        assert_eq!(worker.id(), 0);
    }

    #[tokio::test]
    async fn test_document_types() {
        // Test Document type creation and field access
        let doc = Document {
            source_uri: "test://doc1".to_string(),
            content: "Test content for RLM".to_string(),
            doc_type: "text/plain".to_string(),
            tags: vec!["test".to_string(), "rlm".to_string()],
            ingested_at: 1234567890,
        };

        assert_eq!(doc.source_uri, "test://doc1");
        assert_eq!(doc.content, "Test content for RLM");
        assert_eq!(doc.doc_type, "text/plain");
        assert_eq!(doc.tags.len(), 2);
        assert_eq!(doc.ingested_at, 1234567890);
    }

    #[tokio::test]
    async fn test_rlm_query_structure() {
        // Test RlmQuery type creation
        let query = RlmQuery {
            query: "What is machine learning?".to_string(),
            guild_id: "12345".to_string(),
            max_iterations: 10,
            max_sub_calls: 50,
        };

        assert_eq!(query.query, "What is machine learning?");
        assert_eq!(query.guild_id, "12345");
        assert_eq!(query.max_iterations, 10);
        assert_eq!(query.max_sub_calls, 50);
    }

    #[tokio::test]
    async fn test_pool_acquire_release() {
        if skip_if_no_python() {
            println!("Skipping test: Python 3 not available");
            return;
        }

        // Test worker pool operations
        let pool = ergors_rlm::pool::ReplPool::new(2).await;
        assert!(pool.is_ok(), "Failed to create pool: {:?}", pool.err());

        let pool = pool.unwrap();

        // Acquire a worker
        let worker = pool.acquire().await;
        assert!(worker.is_ok(), "Failed to acquire worker: {:?}", worker.err());

        let worker = worker.unwrap();
        let worker_id = worker.id();

        // Release it back
        pool.release(worker).await;

        // Should be able to acquire again
        let worker2 = pool.acquire().await;
        assert!(worker2.is_ok(), "Failed to re-acquire worker");

        println!("Pool test passed - worker {} acquired and released", worker_id);
    }

    #[tokio::test]
    async fn test_end_to_end_rlm_query() {
        if skip_if_no_python() {
            println!("Skipping test: Python 3 not available");
            return;
        }

        // Skip if RestrictedPython is not installed
        let check_restrictedpython = std::process::Command::new("python3")
            .arg("-c")
            .arg("from RestrictedPython import compile_restricted")
            .output();

        if check_restrictedpython.map(|o| !o.status.success()).unwrap_or(true) {
            println!("Skipping test: RestrictedPython not installed");
            return;
        }

        // Create mock router
        let router = Arc::new(MockLlmRouter::new());

        // Create test documents
        let documents = vec![
            Document {
                source_uri: "test://doc1".to_string(),
                content: "This document explains RLM testing.".to_string(),
                doc_type: "text/plain".to_string(),
                tags: vec!["test".to_string()],
                ingested_at: 1000000,
            },
            Document {
                source_uri: "test://doc2".to_string(),
                content: "This is the second test document about RLM.".to_string(),
                doc_type: "text/plain".to_string(),
                tags: vec!["test".to_string()],
                ingested_at: 1000001,
            },
        ];

        // Create RLM query
        let query = RlmQuery {
            query: "What do the documents say about testing?".to_string(),
            guild_id: "test-guild".to_string(),
            max_iterations: 5,
            max_sub_calls: 10,
        };

        // Spawn a worker and execute query
        let worker = ergors_rlm::process::ReplWorker::spawn(99).expect("Failed to spawn worker");

        let result = worker
            .execute(query, documents, router.clone() as Arc<dyn LlmRouterTrait>)
            .await;

        assert!(result.is_ok(), "RLM query failed: {:?}", result.err());

        let response = result.unwrap();

        // Verify response structure
        assert!(!response.answer.is_empty(), "Answer should not be empty");
        assert!(response.iterations > 0, "Should have at least 1 iteration");
        assert!(response.iterations <= 5, "Should not exceed max iterations");
        assert_eq!(response.source_uris.len(), 0); // Mock doesn't extract sources

        // Verify LLM was called
        let call_count = router.get_call_count().await;
        assert!(call_count >= 2, "Should have called LLM at least twice (got {})", call_count);

        println!("End-to-end test passed:");
        println!("  Answer: {}", response.answer);
        println!("  Iterations: {}", response.iterations);
        println!("  Sub-LLM calls: {}", response.sub_llm_calls);
        println!("  LLM router calls: {}", call_count);
    }
}
