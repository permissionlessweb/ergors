//! Daemon management for ERGORS engine
//!
//! Handles PID file management and signal handling for daemonized operation.

pub mod pid;
pub mod signals;

use anyhow::Result;
use camino::Utf8Path;
use std::path::PathBuf;

pub use pid::PidFile;
pub use signals::SignalHandler;

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
