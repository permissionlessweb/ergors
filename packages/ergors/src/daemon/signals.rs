//! Signal handling for graceful daemon shutdown
//!
//! Sets up handlers for SIGTERM, SIGINT, and SIGHUP to enable:
//! - Graceful shutdown on SIGTERM/SIGINT
//! - Configuration reload on SIGHUP

use anyhow::Result;
use tokio::sync::broadcast;

/// Signal handler for daemon lifecycle events
pub struct SignalHandler {
    shutdown_tx: broadcast::Sender<()>,
    reload_tx: tokio::sync::mpsc::Sender<()>,
}

impl SignalHandler {
    /// Create a new signal handler with channels for shutdown and reload
    pub fn new() -> (
        Self,
        broadcast::Receiver<()>,
        tokio::sync::mpsc::Receiver<()>,
    ) {
        let (shutdown_tx, shutdown_rx) = broadcast::channel(1);
        let (reload_tx, reload_rx) = tokio::sync::mpsc::channel(1);

        (
            Self {
                shutdown_tx,
                reload_tx,
            },
            shutdown_rx,
            reload_rx,
        )
    }

    /// Start listening for OS signals
    ///
    /// This spawns a background task that handles:
    /// - SIGTERM/SIGINT -> triggers shutdown
    /// - SIGHUP -> triggers config reload
    #[cfg(unix)]
    pub async fn setup(&self) -> Result<()> {
        use tokio::signal::unix::{signal, SignalKind};

        let shutdown_tx = self.shutdown_tx.clone();
        let reload_tx = self.reload_tx.clone();

        // Handle SIGTERM
        let mut sigterm = signal(SignalKind::terminate())?;
        let shutdown_tx_term = shutdown_tx.clone();
        tokio::spawn(async move {
            sigterm.recv().await;
            tracing::info!("Received SIGTERM, initiating shutdown");
            let _ = shutdown_tx_term.send(());
        });

        // Handle SIGINT (Ctrl+C)
        let mut sigint = signal(SignalKind::interrupt())?;
        let shutdown_tx_int = shutdown_tx.clone();
        tokio::spawn(async move {
            sigint.recv().await;
            tracing::info!("Received SIGINT, initiating shutdown");
            let _ = shutdown_tx_int.send(());
        });

        // Handle SIGHUP (config reload)
        let mut sighup = signal(SignalKind::hangup())?;
        tokio::spawn(async move {
            loop {
                sighup.recv().await;
                tracing::info!("Received SIGHUP, reloading configuration");
                if reload_tx.send(()).await.is_err() {
                    break;
                }
            }
        });

        tracing::debug!("Signal handlers installed");
        Ok(())
    }

    /// Setup signal handlers for non-Unix systems
    #[cfg(not(unix))]
    pub async fn setup(&self) -> Result<()> {
        let shutdown_tx = self.shutdown_tx.clone();

        // Only Ctrl+C is universally supported
        tokio::spawn(async move {
            if let Ok(_) = tokio::signal::ctrl_c().await {
                tracing::info!("Received Ctrl+C, initiating shutdown");
                let _ = shutdown_tx.send(());
            }
        });

        Ok(())
    }

    /// Trigger a shutdown programmatically
    pub fn trigger_shutdown(&self) {
        let _ = self.shutdown_tx.send(());
    }

    /// Get a new shutdown receiver
    pub fn subscribe_shutdown(&self) -> broadcast::Receiver<()> {
        self.shutdown_tx.subscribe()
    }
}

impl Default for SignalHandler {
    fn default() -> Self {
        let (handler, _, _) = Self::new();
        handler
    }
}

/// Wait for a shutdown signal
pub async fn wait_for_shutdown(mut rx: broadcast::Receiver<()>) {
    let _ = rx.recv().await;
}
