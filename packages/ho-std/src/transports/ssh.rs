use anyhow::Context;
use std::path::Path;
use tracing::{error, info};

use crate::constants::*;

/// SSH Connection Manager for orchestration
#[derive(Debug)]
pub struct SSHConnectionManager {
    /// Target node name
    pub target_node: String,
    /// Connection status
    pub is_connected: bool,
}

impl SSHConnectionManager {
    /// Create new SSH connection manager
    pub fn new(target_node: String) -> Self {
        Self {
            target_node,
            is_connected: false,
        }
    }

    /// Test SSH connection (simplified approach)
    pub async fn connect(&mut self) -> Result<(), anyhow::Error> {
        info!("🔌 Testing SSH connection to node: {}", self.target_node);

        let ssh_config = "";
        // Use the existing SSH transport script to test connection
        let ssh_test_command = format!(
            "python3 {} --config {} --node {}",
            TOOLS_SSH_TRANSPORT, ssh_config, self.target_node
        );

        let output = tokio::process::Command::new(CMD_BASH)
            .arg("-c")
            .arg(&ssh_test_command)
            .output()
            .await
            .context("Failed to execute SSH connection test")?;

        let success = output.status.success();
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        info!(
            "🔍 SSH Test - Success: {}, Stdout: {}, Stderr: {}",
            success,
            stdout.trim(),
            stderr.trim()
        );

        if success && !stdout.trim().is_empty() {
            self.is_connected = true;
            info!("✅ SSH connection verified for node: {}", self.target_node);
            Ok(())
        } else {
            Err(anyhow::anyhow!(
                "SSH connection test failed - Success: {}, Stdout: {}, Stderr: {}",
                success,
                stdout.trim(),
                stderr.trim()
            ))
        }
    }

    /// Execute command via SSH (uses individual SSH calls)
    pub async fn execute_command(&mut self, command: &str) -> Result<String, anyhow::Error> {
        if !self.is_connected {
            self.connect().await?;
        }

        info!("🔧 Executing SSH command: {}", command);

        // Execute command via SSH (reading config dynamically)
        let ssh_config_content = tokio::fs::read_to_string(SSH_JSON_PATH)
            .await
            .context("Failed to read SSH config")?;
        let ssh_config: serde_json::Value =
            serde_json::from_str(&ssh_config_content).context("Failed to parse SSH config")?;

        let node_config = ssh_config
            .get(&self.target_node)
            .ok_or_else(|| anyhow::anyhow!("Node {} not found in SSH config", self.target_node))?;

        let host = node_config
            .get("host")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("No host found for node {}", self.target_node))?;
        let username = node_config
            .get("username")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("No username found for node {}", self.target_node))?;
        let password = node_config.get("password").and_then(|v| v.as_str());
        let port = node_config
            .get("port")
            .and_then(|v| v.as_u64())
            .unwrap_or(22);
        let is_wsl = node_config
            .get("wsl")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if is_wsl {
            info!("🪟 WSL node detected - wrapping command with 'wsl bash -c'");
        }

        // If it's a WSL node, wrap the command to enter WSL first
        let final_command = if is_wsl {
            format!("wsl bash -c '{}'", command.replace("'", "\\'"))
        } else {
            command.to_string()
        };

        let ssh_command = if let Some(pwd) = password {
            format!(
                "sshpass -p '{}' ssh -p {} -o StrictHostKeyChecking=no {}@{} '{}'",
                pwd, port, username, host, final_command
            )
        } else {
            format!(
                "ssh -i ~/.ssh/id_rsa -p {} -o StrictHostKeyChecking=no {}@{} '{}'",
                port, username, host, final_command
            )
        };

        let output = tokio::process::Command::new(CMD_BASH)
            .arg("-c")
            .arg(&ssh_command)
            .output()
            .await
            .context("Failed to execute SSH command")?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        if output.status.success() {
            Ok(stdout.trim().to_string())
        } else {
            Err(anyhow::anyhow!(
                "SSH command failed: {} (stderr: {})",
                stdout.trim(),
                stderr.trim()
            ))
        }
    }

    /// Execute multiple commands in sequence
    pub async fn execute_commands(
        &mut self,
        commands: &[&str],
    ) -> Result<Vec<String>, anyhow::Error> {
        let mut results = Vec::new();
        for command in commands {
            let result = self.execute_command(command).await?;
            results.push(result);
        }
        Ok(results)
    }

    /// Mark SSH connection as closed
    pub async fn close(&mut self) -> Result<(), anyhow::Error> {
        info!(
            "🔌 Marking SSH connection closed for node: {}",
            self.target_node
        );
        self.is_connected = false;
        Ok(())
    }

    /// Check if connection is active
    pub fn is_connected(&self) -> bool {
        self.is_connected
    }

    /// Bootstrap a new node with workspace and dependencies
    pub async fn bootstrap_node(&mut self) -> Result<String, anyhow::Error> {
        info!(
            "🚀 Starting node bootstrap process for: {}",
            self.target_node
        );

        // Step 1: Ensure SSH connection
        if !self.is_connected {
            self.connect().await?;
        }

        // Step 2: Create and transfer workspace archive
        let archive_result = self.create_workspace_archive().await?;
        let transfer_result = self.transfer_workspace().await?;

        // Step 3: Install development environment
        let install_result = self.install_dev_environment().await?;

        // Step 4: Extract workspace and setup
        let setup_result = self.setup_workspace().await?;

        let summary = format!(
            "Bootstrap completed:\n- Archive: {}\n- Transfer: {}\n- Install: {}\n- Setup: {}",
            archive_result, transfer_result, install_result, setup_result
        );

        info!(
            "✅ Bootstrap process completed for node: {}",
            self.target_node
        );
        Ok(summary)
    }

    /// Create compressed workspace archive
    pub async fn create_workspace_archive(&mut self) -> Result<String, anyhow::Error> {
        info!("📦 Creating workspace archive");

        let create_archive_cmd = format!(
            "cd {} && tar -czf {} --exclude=target --exclude=node_modules --exclude=.git --exclude='*.log' .",
            WORKSPACE_HOME,
            WORKSPACE_ARCHIVE_PATH
        );

        let output = tokio::process::Command::new(CMD_BASH)
            .arg("-c")
            .arg(&create_archive_cmd)
            .output()
            .await
            .context("Failed to create workspace archive")?;

        if output.status.success() {
            let size_output = tokio::process::Command::new(CMD_BASH)
                .arg("-c")
                .arg(&format!(
                    "ls -lh {} | awk '{{print $5}}'",
                    WORKSPACE_ARCHIVE_PATH
                ))
                .output()
                .await
                .context("Failed to get archive size")?;

            let size = String::from_utf8_lossy(&size_output.stdout)
                .trim()
                .to_string();
            Ok(format!("Archive created ({} size)", size))
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(anyhow::anyhow!("Archive creation failed: {}", stderr))
        }
    }

    /// Transfer workspace archive to target node
    pub async fn transfer_workspace(&mut self) -> Result<String, anyhow::Error> {
        info!("📤 Transferring workspace to node: {}", self.target_node);

        // Read SSH config to get connection details
        let ssh_config_content = tokio::fs::read_to_string(SSH_JSON_PATH)
            .await
            .context("Failed to read SSH config")?;
        let ssh_config: serde_json::Value =
            serde_json::from_str(&ssh_config_content).context("Failed to parse SSH config")?;

        let node_config = ssh_config
            .get(&self.target_node)
            .ok_or_else(|| anyhow::anyhow!("Node {} not found in SSH config", self.target_node))?;

        let host = node_config
            .get("host")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("No host found for node {}", self.target_node))?;
        let username = node_config
            .get("username")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("No username found for node {}", self.target_node))?;
        let password = node_config.get("password").and_then(|v| v.as_str());
        let port = node_config
            .get("port")
            .and_then(|v| v.as_u64())
            .unwrap_or(22);

        // Use SCP to transfer the archive
        let scp_command = if let Some(pwd) = password {
            format!(
                "sshpass -p '{}' scp -P {} -o StrictHostKeyChecking=no {} {}@{}:~/workspace.tar.gz",
                pwd, port, WORKSPACE_ARCHIVE_PATH, username, host
            )
        } else {
            format!(
                "scp -i ~/.ssh/id_rsa -P {} -o StrictHostKeyChecking=no {} {}@{}:~/workspace.tar.gz",
                port, WORKSPACE_ARCHIVE_PATH, username, host
            )
        };

        let output = tokio::process::Command::new(CMD_BASH)
            .arg("-c")
            .arg(&scp_command)
            .output()
            .await
            .context("Failed to transfer workspace archive")?;

        if output.status.success() {
            Ok("Workspace transferred successfully".to_string())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(anyhow::anyhow!("Transfer failed: {}", stderr))
        }
    }

    /// Install development environment on target node
    pub async fn install_dev_environment(&mut self) -> Result<String, anyhow::Error> {
        info!("🛠️ Installing development environment on target node");

        // Transfer the installation script first
        let ssh_config_content = tokio::fs::read_to_string(SSH_JSON_PATH)
            .await
            .context("Failed to read SSH config")?;
        let ssh_config: serde_json::Value =
            serde_json::from_str(&ssh_config_content).context("Failed to parse SSH config")?;

        let node_config = ssh_config
            .get(&self.target_node)
            .ok_or_else(|| anyhow::anyhow!("Node {} not found in SSH config", self.target_node))?;

        let host = node_config
            .get("host")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("No host found for node {}", self.target_node))?;
        let username = node_config
            .get("username")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("No username found for node {}", self.target_node))?;
        let password = node_config.get("password").and_then(|v| v.as_str());
        let port = node_config
            .get("port")
            .and_then(|v| v.as_u64())
            .unwrap_or(22);

        // Transfer installation script
        let script_transfer_cmd = if let Some(pwd) = password {
            format!(
                "sshpass -p '{}' scp -P {} -o StrictHostKeyChecking=no tools/deploy/install-dev-environment.sh {}@{}:~/install-dev-environment.sh",
                pwd, port, username, host
            )
        } else {
            format!(
                "scp -i ~/.ssh/id_rsa -P {} -o StrictHostKeyChecking=no tools/deploy/install-dev-environment.sh {}@{}:~/install-dev-environment.sh",
                port, username, host
            )
        };

        let transfer_output = tokio::process::Command::new(CMD_BASH)
            .arg("-c")
            .arg(&script_transfer_cmd)
            .output()
            .await
            .context("Failed to transfer installation script")?;

        if !transfer_output.status.success() {
            let stderr = String::from_utf8_lossy(&transfer_output.stderr);
            return Err(anyhow::anyhow!("Script transfer failed: {}", stderr));
        }

        // Execute installation script remotely
        let install_result = self
            .execute_command(
                "chmod +x ~/install-dev-environment.sh && ~/install-dev-environment.sh",
            )
            .await?;

        Ok(format!(
            "Development environment installed: {}",
            install_result.len()
        ))
    }

    /// Setup workspace on target node
    pub async fn setup_workspace(&mut self) -> Result<String, anyhow::Error> {
        info!("🔧 Setting up workspace on target node");

        let setup_commands = vec![
            "mkdir -p ~/CW-AGENT",
            "cd ~/CW-AGENT && tar -xzf ~/workspace.tar.gz",
            "cd ~/CW-AGENT && ls -la",
            "rm ~/workspace.tar.gz",
        ];

        let mut results = Vec::new();
        for cmd in setup_commands {
            let result = self.execute_command(cmd).await?;
            results.push(format!(
                "{}: {}",
                cmd,
                result.chars().take(50).collect::<String>()
            ));
        }

        Ok(format!(
            "Workspace setup completed: {} steps",
            results.len()
        ))
    }
}
