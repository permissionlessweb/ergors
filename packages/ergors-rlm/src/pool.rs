//! REPL subprocess pool management

use crate::process::ReplWorker;
use anyhow::Result;
use std::time::Duration;
use tokio::sync::{mpsc, Mutex};
use tracing::{debug, info, warn};

/// Timeout for acquiring a worker from the pool
const ACQUIRE_TIMEOUT: Duration = Duration::from_secs(30);

/// Pool of Python REPL worker subprocesses
pub struct ReplPool {
    workers: mpsc::UnboundedSender<ReplWorker>,
    receiver: Mutex<mpsc::UnboundedReceiver<ReplWorker>>,
    max_size: usize,
    next_worker_id: Mutex<usize>,
}

impl ReplPool {
    /// Create new pool with specified size
    pub async fn new(size: usize) -> Result<Self> {
        info!("Creating RLM worker pool with {} workers", size);

        let (tx, rx) = mpsc::unbounded_channel();

        // Spawn initial workers
        for i in 0..size {
            let worker = ReplWorker::spawn(i)?;
            tx.send(worker).ok();
        }

        Ok(Self {
            workers: tx,
            receiver: Mutex::new(rx),
            max_size: size,
            next_worker_id: Mutex::new(size),
        })
    }

    /// Acquire a worker from the pool with timeout
    pub async fn acquire(&self) -> Result<ReplWorker> {
        tokio::time::timeout(ACQUIRE_TIMEOUT, async {
            let mut rx = self.receiver.lock().await;

            // Try to get a worker from the channel
            if let Some(worker) = rx.recv().await {
                debug!("Acquired worker {} from pool", worker.id());
                return Ok(worker);
            }

            // If channel is closed or empty, spawn emergency worker
            warn!("Worker pool exhausted, spawning emergency worker");
            let mut next_id = self.next_worker_id.lock().await;
            let worker_id = *next_id;
            *next_id += 1;
            drop(next_id);

            let worker = ReplWorker::spawn(worker_id)?;
            debug!("Spawned emergency worker {}", worker_id);
            Ok(worker)
        })
        .await
        .map_err(|_| anyhow::anyhow!("Worker pool acquire timeout after {:?}", ACQUIRE_TIMEOUT))?
    }

    /// Release worker back to pool
    pub async fn release(&self, worker: ReplWorker) {
        debug!("Releasing worker {} back to pool", worker.id());

        // Try to return worker to pool, drop if channel is full/closed
        if let Err(e) = self.workers.send(worker) {
            debug!("Failed to return worker to pool: {}", e);
            // Worker will be dropped
        }
    }

    /// Get maximum pool size (configured capacity, not current available workers)
    pub async fn max_size(&self) -> usize {
        self.max_size
    }
}
