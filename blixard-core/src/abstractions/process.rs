//! Process execution abstractions for testability
//!
//! This module provides trait-based abstractions for process execution,
//! enabling testing without actually spawning system processes.

use crate::error::BlixardResult;
use async_trait::async_trait;
use std::collections::HashMap;

/// Result of a process execution
#[derive(Debug, Clone)]
pub struct ProcessOutput {
    /// Exit status code
    pub status: i32,
    /// Standard output
    pub stdout: Vec<u8>,
    /// Standard error
    pub stderr: Vec<u8>,
}

impl ProcessOutput {
    /// Check if the process exited successfully
    pub fn success(&self) -> bool {
        self.status == 0
    }
}

/// Abstraction for process execution
#[async_trait]
pub trait ProcessExecutor: Send + Sync {
    /// Execute a command with arguments
    async fn execute(
        &self,
        command: &str,
        args: &[&str],
        env: Option<HashMap<String, String>>,
    ) -> BlixardResult<ProcessOutput>;

    /// Execute a command and return only the exit status
    async fn execute_status(&self, command: &str, args: &[&str]) -> BlixardResult<bool>;

    /// Execute a command in the background (fire and forget)
    async fn spawn(
        &self,
        command: &str,
        args: &[&str],
        env: Option<HashMap<String, String>>,
    ) -> BlixardResult<u32>; // Returns process ID

    /// Kill a process by ID
    async fn kill(&self, pid: u32) -> BlixardResult<()>;

    /// Check if a process is running
    async fn is_running(&self, pid: u32) -> BlixardResult<bool>;
}

// Production implementation using tokio::process

use tokio::process::Command;

/// Production process executor
pub struct TokioProcessExecutor;

impl TokioProcessExecutor {
    /// Create new instance
    pub fn new() -> Self {
        Self
    }
}

impl Default for TokioProcessExecutor {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ProcessExecutor for TokioProcessExecutor {
    async fn execute(
        &self,
        command: &str,
        args: &[&str],
        env: Option<HashMap<String, String>>,
    ) -> BlixardResult<ProcessOutput> {
        let mut cmd = Command::new(command);
        cmd.args(args);

        if let Some(env_vars) = env {
            for (key, value) in env_vars {
                cmd.env(key, value);
            }
        }

        let output = cmd.output().await?;

        Ok(ProcessOutput {
            status: output.status.code().unwrap_or(-1),
            stdout: output.stdout,
            stderr: output.stderr,
        })
    }

    async fn execute_status(&self, command: &str, args: &[&str]) -> BlixardResult<bool> {
        let output = self.execute(command, args, None).await?;
        Ok(output.success())
    }

    async fn spawn(
        &self,
        command: &str,
        args: &[&str],
        env: Option<HashMap<String, String>>,
    ) -> BlixardResult<u32> {
        let mut cmd = Command::new(command);
        cmd.args(args);

        if let Some(env_vars) = env {
            for (key, value) in env_vars {
                cmd.env(key, value);
            }
        }

        let child = cmd.spawn()?;
        Ok(child.id().unwrap_or(0))
    }

    async fn kill(&self, pid: u32) -> BlixardResult<()> {
        // TODO: Implement with nix crate for Unix systems
        // For now, use a simple approach
        #[cfg(unix)]
        {
            use std::process::Command;
            Command::new("kill")
                .arg("-TERM")
                .arg(pid.to_string())
                .output()
                .map_err(|e| crate::error::BlixardError::SystemError(e.to_string()))?;
        }

        #[cfg(not(unix))]
        {
            return Err(crate::error::BlixardError::NotImplemented {
                feature: "Process killing on non-Unix systems".to_string(),
            });
        }

        Ok(())
    }

    async fn is_running(&self, pid: u32) -> BlixardResult<bool> {
        #[cfg(unix)]
        {
            // Use kill -0 to check if process exists
            use std::process::Command;
            let output = Command::new("kill")
                .arg("-0")
                .arg(pid.to_string())
                .output()
                .map_err(|e| crate::error::BlixardError::SystemError(e.to_string()))?;

            Ok(output.status.success())
        }

        #[cfg(not(unix))]
        {
            Ok(false) // Conservative default on non-Unix
        }
    }
}

// Mock implementation for testing

use std::sync::Arc;
use tokio::sync::RwLock;

/// Mock process executor for testing
pub struct MockProcessExecutor {
    /// Expected outputs for commands
    outputs: Arc<RwLock<HashMap<String, ProcessOutput>>>,
    /// Running processes
    processes: Arc<RwLock<HashMap<u32, String>>>,
    /// Next process ID
    next_pid: Arc<RwLock<u32>>,
}

impl MockProcessExecutor {
    /// Create new mock executor
    pub fn new() -> Self {
        Self {
            outputs: Arc::new(RwLock::new(HashMap::new())),
            processes: Arc::new(RwLock::new(HashMap::new())),
            next_pid: Arc::new(RwLock::new(1000)),
        }
    }

    /// Set expected output for a command
    pub async fn set_output(&self, command: &str, output: ProcessOutput) {
        self.outputs
            .write()
            .await
            .insert(command.to_string(), output);
    }

    /// Set successful output with stdout
    pub async fn set_success(&self, command: &str, stdout: &str) {
        self.set_output(
            command,
            ProcessOutput {
                status: 0,
                stdout: stdout.as_bytes().to_vec(),
                stderr: Vec::new(),
            },
        )
        .await;
    }

    /// Set failed output with stderr
    pub async fn set_failure(&self, command: &str, stderr: &str) {
        self.set_output(
            command,
            ProcessOutput {
                status: 1,
                stdout: Vec::new(),
                stderr: stderr.as_bytes().to_vec(),
            },
        )
        .await;
    }
}

impl Default for MockProcessExecutor {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ProcessExecutor for MockProcessExecutor {
    async fn execute(
        &self,
        command: &str,
        _args: &[&str],
        _env: Option<HashMap<String, String>>,
    ) -> BlixardResult<ProcessOutput> {
        self.outputs
            .read()
            .await
            .get(command)
            .cloned()
            .ok_or_else(|| crate::error::BlixardError::Internal {
                message: format!("No mock output configured for command: {}", command),
            })
    }

    async fn execute_status(&self, command: &str, args: &[&str]) -> BlixardResult<bool> {
        let output = self.execute(command, args, None).await?;
        Ok(output.success())
    }

    async fn spawn(
        &self,
        command: &str,
        _args: &[&str],
        _env: Option<HashMap<String, String>>,
    ) -> BlixardResult<u32> {
        let mut next_pid = self.next_pid.write().await;
        let pid = *next_pid;
        *next_pid += 1;

        self.processes
            .write()
            .await
            .insert(pid, command.to_string());
        Ok(pid)
    }

    async fn kill(&self, pid: u32) -> BlixardResult<()> {
        self.processes.write().await.remove(&pid);
        Ok(())
    }

    async fn is_running(&self, pid: u32) -> BlixardResult<bool> {
        Ok(self.processes.read().await.contains_key(&pid))
    }
}
