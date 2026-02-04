//! Python subprocess wrapper for REPL execution

use crate::{llm_trait::LlmRouterTrait, types::*};
use anyhow::{Context, Result};
use ho_std::types::ergors::orch::v1::{PromptMessage, PromptRequest};
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout};
use tokio::sync::Mutex;
use tracing::{debug, error};

/// Timeout for subprocess I/O operations
const IO_TIMEOUT: Duration = Duration::from_secs(30);

/// Timeout for overall RLM query execution
const QUERY_TIMEOUT: Duration = Duration::from_secs(300); // 5 minutes

/// Python REPL worker subprocess
pub struct ReplWorker {
    id: usize,
    process: Child,
    stdin: Arc<Mutex<ChildStdin>>,
    stdout: Arc<Mutex<BufReader<ChildStdout>>>,
}

impl ReplWorker {
    /// Spawn new Python worker subprocess
    pub fn spawn(id: usize) -> Result<Self> {
        let python_script = concat!(env!("CARGO_MANIFEST_DIR"), "/python/repl_worker.py");

        // Try to use venv python if available, fall back to system python3
        let venv_path_file = concat!(env!("CARGO_MANIFEST_DIR"), "/target/venv_python_path");
        let python_cmd = std::fs::read_to_string(venv_path_file)
            .ok()
            .filter(|p| std::path::Path::new(p.trim()).exists())
            .unwrap_or_else(|| "python3".to_string());

        debug!(
            "Spawning RLM worker {} with python: {} script: {}",
            id, python_cmd, python_script
        );

        let mut process = tokio::process::Command::new(python_cmd.trim())
            .arg(python_script)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::inherit())
            .spawn()
            .context("Failed to spawn Python REPL worker")?;

        let stdin = process.stdin.take().unwrap();
        let stdout = BufReader::new(process.stdout.take().unwrap());

        Ok(Self {
            id,
            process,
            stdin: Arc::new(Mutex::new(stdin)),
            stdout: Arc::new(Mutex::new(stdout)),
        })
    }

    /// Execute RLM query in this worker with timeout
    pub async fn execute(
        &self,
        query: RlmQuery,
        documents: Vec<Document>,
        router: Arc<dyn LlmRouterTrait>,
    ) -> Result<RlmResponse> {
        debug!("Worker {} executing RLM query: {}", self.id, query.query);

        // Wrap entire execution in timeout
        tokio::time::timeout(QUERY_TIMEOUT, self.execute_inner(query, documents, router))
            .await
            .unwrap_or_else(|_| {
                error!("Worker {} query timeout after {:?}", self.id, QUERY_TIMEOUT);
                Err(anyhow::anyhow!("RLM query timeout"))
            })
    }

    /// Inner execution logic without timeout wrapper
    async fn execute_inner(
        &self,
        query: RlmQuery,
        documents: Vec<Document>,
        router: Arc<dyn LlmRouterTrait>,
    ) -> Result<RlmResponse> {
        // Convert documents to JSON
        let docs_json: Vec<serde_json::Value> = documents
            .iter()
            .map(|d| {
                json!({
                    "source_uri": d.source_uri,
                    "content": d.content,
                    "doc_type": d.doc_type,
                    "tags": d.tags,
                    "ingested_at": d.ingested_at,
                })
            })
            .collect();

        // Send JSON-RPC request to Python worker
        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            method: "execute".to_string(),
            params: json!({
                "query": query.query,
                "documents": docs_json,
                "max_iterations": query.max_iterations,
                "max_sub_calls": query.max_sub_calls,
            }),
            id: json!(1),
        };

        self.send_request(&request).await?;

        // Handle responses (including sub-LLM callbacks)
        loop {
            let response = self.recv_response().await?;

            // Check if this is a sub-LLM callback request from Python
            if let Some(method) = response
                .get("method")
                .and_then(|m| m.as_str())
            {
                if method == "llm_query" {
                    debug!("Worker {} handling sub-LLM callback", self.id);

                    let params = response
                        .get("params")
                        .context("Missing params in llm_query")?;
                    let prompt = params
                        .get("prompt")
                        .and_then(|p| p.as_str())
                        .context("Missing prompt in llm_query")?;
                    let model = params
                        .get("model")
                        .and_then(|m| m.as_str())
                        .unwrap_or("default");

                    // Call LLM router with timeout
                    let llm_response = tokio::time::timeout(
                        Duration::from_secs(60),
                        router.handle_request(
                            &PromptRequest {
                                messages: vec![PromptMessage {
                                    role: "user".to_string(),
                                    content: prompt.to_string(),
                                    ..Default::default()
                                }],
                                model: model.to_string(),
                                ..Default::default()
                            },
                            model,
                        ),
                    )
                    .await
                    .map_err(|_| anyhow::anyhow!("Sub-LLM call timeout"))?
                    .context("LLM router request failed")?;

                    // Send result back to Python
                    let callback_response = JsonRpcResponse {
                        jsonrpc: "2.0".to_string(),
                        result: Some(json!(llm_response.response.join("\n"))),
                        error: None,
                        id: response.get("id").cloned().unwrap_or(json!(null)),
                    };

                    self.send_response(&callback_response).await?;
                }
            } else if let Some(result) = response.get("result") {
                // Final result from execute() call
                debug!("Worker {} received final result", self.id);
                let rlm_response: RlmResponse = serde_json::from_value(result.clone())
                    .context("Failed to parse RlmResponse")?;
                return Ok(rlm_response);
            } else if let Some(error) = response.get("error") {
                let error_msg = error
                    .get("message")
                    .and_then(|m| m.as_str())
                    .unwrap_or("Unknown error");
                let error_data = error
                    .get("data")
                    .and_then(|d| d.as_str())
                    .unwrap_or("");

                error!("Worker {} Python error: {}\n{}", self.id, error_msg, error_data);
                return Err(anyhow::anyhow!("Python REPL error: {}", error_msg));
            }
        }
    }

    /// Send JSON-RPC request to Python worker with timeout
    async fn send_request(&self, request: &JsonRpcRequest) -> Result<()> {
        tokio::time::timeout(IO_TIMEOUT, async {
            let mut stdin = self.stdin.lock().await;
            let json_str = serde_json::to_string(request)?;
            stdin.write_all(json_str.as_bytes()).await?;
            stdin.write_all(b"\n").await?;
            stdin.flush().await?;
            Ok::<(), anyhow::Error>(())
        })
        .await
        .map_err(|_| anyhow::anyhow!("Send request timeout"))??;
        Ok(())
    }

    /// Send JSON-RPC response to Python worker with timeout
    async fn send_response(&self, response: &JsonRpcResponse) -> Result<()> {
        tokio::time::timeout(IO_TIMEOUT, async {
            let mut stdin = self.stdin.lock().await;
            let json_str = serde_json::to_string(response)?;
            stdin.write_all(json_str.as_bytes()).await?;
            stdin.write_all(b"\n").await?;
            stdin.flush().await?;
            Ok::<(), anyhow::Error>(())
        })
        .await
        .map_err(|_| anyhow::anyhow!("Send response timeout"))??;
        Ok(())
    }

    /// Receive JSON-RPC message from Python worker with timeout
    async fn recv_response(&self) -> Result<serde_json::Value> {
        tokio::time::timeout(IO_TIMEOUT, async {
            let mut stdout = self.stdout.lock().await;
            let mut line = String::new();
            let bytes_read = stdout.read_line(&mut line).await?;

            if bytes_read == 0 {
                return Err(anyhow::anyhow!("Worker process closed stdout"));
            }

            Ok::<serde_json::Value, anyhow::Error>(serde_json::from_str(&line)?)
        })
        .await
        .map_err(|_| anyhow::anyhow!("Receive response timeout"))?
    }

    /// Get worker ID
    pub fn id(&self) -> usize {
        self.id
    }
}

impl Drop for ReplWorker {
    fn drop(&mut self) {
        debug!("Dropping RLM worker {}", self.id);
        let _ = self.process.start_kill();
    }
}
