// Daemon management for ERGORS engine
//
// Handles PID file management and signal handling for daemonized operation.

use anyhow::Result;
use camino::Utf8Path;
use std::path::PathBuf;

/// Daemon configuration and state
pub struct Daemon {
    home_dir: PathBuf,
    pid_file: PidFile,
}

impl Daemon {
    /// Create a new daemon manager
    pub fn new(home: &Utf8Path) -> Self {
        let home_dir = home.as_std_path().to_path_buf();
        let pid_path = home_dir.join("ergors.pid");

        Self {
            home_dir,
            pid_file: PidFile::new(pid_path),
        }
    }

    /// Check if the daemon is currently running
    pub fn is_running(&self) -> bool {
        self.pid_file.is_process_running()
    }

    /// Get the running daemon's PID, if any
    pub fn get_pid(&self) -> Option<u32> {
        self.pid_file.read_pid().ok()
    }

    /// Acquire the PID file lock (called when starting)
    pub fn acquire_lock(&self) -> Result<()> {
        if self.is_running() {
            anyhow::bail!("Daemon is already running (PID: {:?})", self.get_pid());
        }
        self.pid_file.write_current_pid()
    }

    /// Release the PID file lock (called when stopping)
    pub fn release_lock(&self) -> Result<()> {
        self.pid_file.remove()
    }

    /// Get the log directory path
    pub fn log_dir(&self) -> PathBuf {
        self.home_dir.join("logs")
    }

    /// Get the data directory path
    pub fn data_dir(&self) -> PathBuf {
        self.home_dir.join("data")
    }
}

// PID file management for daemon lifecycle
//
// Handles creating, reading, and cleaning up PID files to prevent
// multiple daemon instances and enable graceful shutdown.

use anyhow::Context;
use std::fs;
use std::process;

/// PID file manager
pub struct PidFile {
    path: PathBuf,
}

impl PidFile {
    /// Create a new PID file manager
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// Write the current process PID to the file
    pub fn write_current_pid(&self) -> Result<()> {
        let pid = process::id();
        self.write_pid(pid)
    }

    /// Write a specific PID to the file
    pub fn write_pid(&self, pid: u32) -> Result<()> {
        // Ensure parent directory exists
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).context("Failed to create PID file directory")?;
        }

        fs::write(&self.path, pid.to_string()).context("Failed to write PID file")?;

        tracing::info!("Created PID file at {:?} with PID {}", self.path, pid);
        Ok(())
    }

    /// Read the PID from the file
    pub fn read_pid(&self) -> Result<u32> {
        let content = fs::read_to_string(&self.path).context("Failed to read PID file")?;

        content.trim().parse::<u32>().context("Invalid PID in file")
    }

    /// Remove the PID file
    pub fn remove(&self) -> Result<()> {
        if self.path.exists() {
            fs::remove_file(&self.path).context("Failed to remove PID file")?;
            tracing::info!("Removed PID file at {:?}", self.path);
        }
        Ok(())
    }

    /// Check if a process with the stored PID is running.
    /// Returns false and cleans up the PID file if the process is stale.
    pub fn is_process_running(&self) -> bool {
        match self.read_pid() {
            Ok(pid) => {
                if process_is_ergors(pid) {
                    true
                } else {
                    // Stale PID file - clean it up
                    tracing::info!("Cleaning up stale PID file (PID {} is not running)", pid);
                    let _ = self.remove();
                    false
                }
            }
            Err(_) => false,
        }
    }

    /// Get the path to the PID file
    pub fn path(&self) -> &PathBuf {
        &self.path
    }
}

/// Check if a process with the given PID exists AND is an ergors process.
/// This prevents false positives when PIDs are reused by different processes.
#[cfg(unix)]
fn process_is_ergors(pid: u32) -> bool {
    use std::process::Command;

    // First, check if the process exists using kill -0
    let exists = unsafe { libc::kill(pid as i32, 0) } == 0;
    if !exists {
        return false;
    }

    // Verify it's actually ergors by checking the process name via ps
    // This handles PID reuse by other processes
    match Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "comm="])
        .output()
    {
        Ok(output) if output.status.success() => {
            let name = String::from_utf8_lossy(&output.stdout);
            let name = name.trim();
            // Check if process name contains "ergors"
            name.contains("ergors")
        }
        _ => {
            // ps failed, fall back to just checking if process exists
            // (might be a permission issue)
            true
        }
    }
}

#[cfg(not(unix))]
fn process_is_ergors(pid: u32) -> bool {
    // On non-Unix systems, just assume running if PID file exists
    // A more robust implementation would use platform-specific APIs
    let _ = pid;
    true
}

impl Drop for PidFile {
    fn drop(&mut self) {
        // Only remove PID file if this process created it
        if let Ok(pid) = self.read_pid() {
            if pid == process::id() {
                let _ = self.remove();
            }
        }
    }
}



// Signal handling for graceful daemon shutdown
//
// Sets up handlers for SIGTERM, SIGINT, and SIGHUP to enable:
// - Graceful shutdown on SIGTERM/SIGINT
// - Configuration reload on SIGHUP

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


#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_pid_file_lifecycle() {
        let dir = tempdir().unwrap();
        let pid_path = dir.path().join("test.pid");
        let pid_file = PidFile::new(pid_path.clone());

        // Write current PID
        pid_file.write_current_pid().unwrap();
        assert!(pid_path.exists());

        // Read it back
        let pid = pid_file.read_pid().unwrap();
        assert_eq!(pid, process::id());

        // Process should be detected as running
        assert!(pid_file.is_process_running());

        // Remove it
        pid_file.remove().unwrap();
        assert!(!pid_path.exists());
    }
}
