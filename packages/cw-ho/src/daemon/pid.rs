//! PID file management for daemon lifecycle
//!
//! Handles creating, reading, and cleaning up PID files to prevent
//! multiple daemon instances and enable graceful shutdown.

use anyhow::{Context, Result};
use std::fs;
use std::path::PathBuf;
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

    /// Check if a process with the stored PID is running
    pub fn is_process_running(&self) -> bool {
        match self.read_pid() {
            Ok(pid) => process_exists(pid),
            Err(_) => false,
        }
    }

    /// Get the path to the PID file
    pub fn path(&self) -> &PathBuf {
        &self.path
    }
}

/// Check if a process with the given PID exists
#[cfg(unix)]
fn process_exists(pid: u32) -> bool {
    use std::process::Command;

    // Use kill -0 to check if process exists without actually killing it
    Command::new("kill")
        .args(["-0", &pid.to_string()])
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn process_exists(pid: u32) -> bool {
    // On non-Unix systems, just check if PID file exists
    // A more robust implementation would use platform-specific APIs
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
