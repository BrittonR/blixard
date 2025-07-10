//! Unified command execution interface for Blixard
//!
//! This module provides a standardized interface for executing system commands
//! across the codebase, eliminating duplication and providing consistent
//! error handling, timeouts, and process management.

use crate::error::{BlixardError, BlixardResult};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;
use tokio::process::{Child, Command};
use tracing::{debug, error, info, instrument, warn};

/// Options for command execution
#[derive(Debug, Clone, Default)]
pub struct CommandOptions {
    /// Working directory for the command
    pub cwd: Option<PathBuf>,
    /// Environment variables to set
    pub env: Option<HashMap<String, String>>,
    /// Timeout for command execution
    pub timeout: Option<Duration>,
    /// Whether to capture stdout and stderr
    pub capture_output: bool,
    /// Input to send to stdin
    pub stdin_input: Option<Vec<u8>>,
    /// Kill signal to use for timeout (default: SIGTERM)
    pub kill_signal: Option<i32>,
}

impl CommandOptions {
    /// Create new CommandOptions with default values
    pub fn new() -> Self {
        Self::default()
    }

    /// Set working directory
    pub fn with_cwd<P: Into<PathBuf>>(mut self, cwd: P) -> Self {
        self.cwd = Some(cwd.into());
        self
    }

    /// Set environment variables
    pub fn with_env(mut self, env: HashMap<String, String>) -> Self {
        self.env = Some(env);
        self
    }

    /// Add a single environment variable
    pub fn with_env_var<K: Into<String>, V: Into<String>>(mut self, key: K, value: V) -> Self {
        self.env.get_or_insert_with(HashMap::new).insert(key.into(), value.into());
        self
    }

    /// Set timeout
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// Enable output capture
    pub fn with_output_capture(mut self) -> Self {
        self.capture_output = true;
        self
    }

    /// Set stdin input
    pub fn with_stdin<I: Into<Vec<u8>>>(mut self, input: I) -> Self {
        self.stdin_input = Some(input.into());
        self
    }

    /// Set kill signal for timeout
    pub fn with_kill_signal(mut self, signal: i32) -> Self {
        self.kill_signal = Some(signal);
        self
    }
}

/// Output from command execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandOutput {
    /// Exit status code
    pub status: i32,
    /// Standard output
    pub stdout: Vec<u8>,
    /// Standard error
    pub stderr: Vec<u8>,
    /// Whether the command succeeded (exit code 0)
    pub success: bool,
    /// Execution duration
    pub duration: Duration,
}

impl CommandOutput {
    /// Get stdout as UTF-8 string
    pub fn stdout_string(&self) -> BlixardResult<String> {
        String::from_utf8(self.stdout.clone()).map_err(|e| BlixardError::Internal {
            message: format!("Invalid UTF-8 in stdout: {}", e),
        })
    }

    /// Get stderr as UTF-8 string
    pub fn stderr_string(&self) -> BlixardResult<String> {
        String::from_utf8(self.stderr.clone()).map_err(|e| BlixardError::Internal {
            message: format!("Invalid UTF-8 in stderr: {}", e),
        })
    }

    /// Check if command succeeded
    pub fn is_success(&self) -> bool {
        self.success
    }

    /// Get combined output string
    pub fn combined_output(&self) -> BlixardResult<String> {
        let stdout = self.stdout_string()?;
        let stderr = self.stderr_string()?;
        Ok(format!("stdout: {}\nstderr: {}", stdout, stderr))
    }
}

/// Handle to a spawned process
#[derive(Debug)]
pub struct ProcessHandle {
    /// Process ID
    pub pid: u32,
    /// Internal child process handle
    child: Option<Child>,
}

impl ProcessHandle {
    /// Create a new process handle
    pub fn new(pid: u32, child: Child) -> Self {
        Self {
            pid,
            child: Some(child),
        }
    }

    /// Wait for the process to complete
    pub async fn wait(mut self) -> BlixardResult<CommandOutput> {
        if let Some(mut child) = self.child.take() {
            let start_time = std::time::Instant::now();
            let status = child.wait().await.map_err(|e| BlixardError::Internal {
                message: format!("Failed to wait for process {}: {}", self.pid, e),
            })?;

            let duration = start_time.elapsed();
            let exit_code = status.code().unwrap_or(-1);

            Ok(CommandOutput {
                status: exit_code,
                stdout: vec![], // TODO: Capture if needed
                stderr: vec![], // TODO: Capture if needed
                success: status.success(),
                duration,
            })
        } else {
            Err(BlixardError::Internal {
                message: format!("Process handle {} already consumed", self.pid),
            })
        }
    }

    /// Kill the process
    pub async fn kill(mut self) -> BlixardResult<()> {
        if let Some(mut child) = self.child.take() {
            child.kill().await.map_err(|e| BlixardError::Internal {
                message: format!("Failed to kill process {}: {}", self.pid, e),
            })
        } else {
            Ok(()) // Already consumed or killed
        }
    }
}

/// Unified command execution interface
#[async_trait]
pub trait CommandExecutor: Send + Sync {
    /// Execute a command and wait for completion
    async fn execute(
        &self,
        program: &str,
        args: &[&str],
        options: CommandOptions,
    ) -> BlixardResult<CommandOutput>;

    /// Spawn a command without waiting
    async fn spawn(
        &self,
        program: &str,
        args: &[&str], 
        options: CommandOptions,
    ) -> BlixardResult<ProcessHandle>;

    /// Kill a process by PID
    async fn kill(&self, pid: u32) -> BlixardResult<()>;

    /// Check if a process is running
    async fn is_running(&self, pid: u32) -> BlixardResult<bool>;

    /// Execute a command with a simplified interface
    async fn execute_simple(&self, program: &str, args: &[&str]) -> BlixardResult<CommandOutput> {
        self.execute(program, args, CommandOptions::new().with_output_capture()).await
    }

    /// Execute a command in a specific directory
    async fn execute_in_dir(
        &self,
        program: &str,
        args: &[&str],
        cwd: PathBuf,
    ) -> BlixardResult<CommandOutput> {
        self.execute(
            program,
            args,
            CommandOptions::new().with_cwd(cwd).with_output_capture(),
        ).await
    }

    /// Execute a command with timeout
    async fn execute_with_timeout(
        &self,
        program: &str,
        args: &[&str],
        timeout: Duration,
    ) -> BlixardResult<CommandOutput> {
        self.execute(
            program,
            args,
            CommandOptions::new().with_timeout(timeout).with_output_capture(),
        ).await
    }
}

/// Standard implementation of CommandExecutor using tokio::process
#[derive(Debug, Clone)]
pub struct TokioCommandExecutor {
    /// Default timeout for commands
    pub default_timeout: Option<Duration>,
}

impl TokioCommandExecutor {
    /// Create a new TokioCommandExecutor
    pub fn new() -> Self {
        Self {
            default_timeout: Some(Duration::from_secs(300)), // 5 minutes default
        }
    }

    /// Create a new TokioCommandExecutor with custom default timeout
    pub fn with_default_timeout(timeout: Duration) -> Self {
        Self {
            default_timeout: Some(timeout),
        }
    }

    /// Create a new TokioCommandExecutor with no default timeout
    pub fn without_default_timeout() -> Self {
        Self {
            default_timeout: None,
        }
    }
}

impl Default for TokioCommandExecutor {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl CommandExecutor for TokioCommandExecutor {
    #[instrument(skip(self, options), fields(program, args_count = args.len()))]
    async fn execute(
        &self,
        program: &str,
        args: &[&str],
        options: CommandOptions,
    ) -> BlixardResult<CommandOutput> {
        tracing::Span::current().record("program", program);

        debug!("Executing command: {} {}", program, args.join(" "));

        let start_time = std::time::Instant::now();
        let mut cmd = Command::new(program);
        cmd.args(args);

        // Configure command based on options
        if let Some(cwd) = &options.cwd {
            cmd.current_dir(cwd);
        }

        if let Some(env) = &options.env {
            for (key, value) in env {
                cmd.env(key, value);
            }
        }

        if options.capture_output {
            cmd.stdout(Stdio::piped());
            cmd.stderr(Stdio::piped());
        }

        if options.stdin_input.is_some() {
            cmd.stdin(Stdio::piped());
        }

        // Determine timeout
        let timeout = options.timeout.or(self.default_timeout);

        // Execute with or without timeout
        let result = if let Some(timeout_duration) = timeout {
            tokio::time::timeout(timeout_duration, self.execute_command(cmd, &options)).await
                .map_err(|_| BlixardError::Internal {
                    message: format!("Command '{}' timed out after {:?}", program, timeout_duration),
                })?
        } else {
            self.execute_command(cmd, &options).await
        };

        let duration = start_time.elapsed();

        match result {
            Ok(mut output) => {
                output.duration = duration;
                info!("Command completed successfully in {:?}: {}", duration, program);
                Ok(output)
            }
            Err(e) => {
                error!("Command failed after {:?}: {} - {}", duration, program, e);
                Err(e)
            }
        }
    }

    async fn spawn(
        &self,
        program: &str,
        args: &[&str],
        options: CommandOptions,
    ) -> BlixardResult<ProcessHandle> {
        debug!("Spawning command: {} {}", program, args.join(" "));

        let mut cmd = Command::new(program);
        cmd.args(args);

        if let Some(cwd) = &options.cwd {
            cmd.current_dir(cwd);
        }

        if let Some(env) = &options.env {
            for (key, value) in env {
                cmd.env(key, value);
            }
        }

        let child = cmd.spawn().map_err(|e| BlixardError::Internal {
            message: format!("Failed to spawn command '{}': {}", program, e),
        })?;

        let pid = child.id().unwrap_or(0);
        debug!("Spawned process with PID: {}", pid);

        Ok(ProcessHandle::new(pid, child))
    }

    async fn kill(&self, pid: u32) -> BlixardResult<()> {
        debug!("Killing process: {}", pid);

        #[cfg(unix)]
        {
            use std::process::Command as StdCommand;
            let output = StdCommand::new("kill")
                .args(&["-TERM", &pid.to_string()])
                .output()
                .map_err(|e| BlixardError::Internal {
                    message: format!("Failed to kill process {}: {}", pid, e),
                })?;

            if !output.status.success() {
                return Err(BlixardError::Internal {
                    message: format!(
                        "Kill command failed for PID {}: {}",
                        pid,
                        String::from_utf8_lossy(&output.stderr)
                    ),
                });
            }
        }

        #[cfg(windows)]
        {
            use std::process::Command as StdCommand;
            let output = StdCommand::new("taskkill")
                .args(&["/PID", &pid.to_string(), "/F"])
                .output()
                .map_err(|e| BlixardError::Internal {
                    message: format!("Failed to kill process {}: {}", pid, e),
                })?;

            if !output.status.success() {
                return Err(BlixardError::Internal {
                    message: format!(
                        "Taskkill command failed for PID {}: {}",
                        pid,
                        String::from_utf8_lossy(&output.stderr)
                    ),
                });
            }
        }

        info!("Successfully killed process: {}", pid);
        Ok(())
    }

    async fn is_running(&self, pid: u32) -> BlixardResult<bool> {
        #[cfg(unix)]
        {
            use std::process::Command as StdCommand;
            let output = StdCommand::new("kill")
                .args(&["-0", &pid.to_string()])
                .output()
                .map_err(|e| BlixardError::Internal {
                    message: format!("Failed to check process {}: {}", pid, e),
                })?;

            Ok(output.status.success())
        }

        #[cfg(windows)]
        {
            use std::process::Command as StdCommand;
            let output = StdCommand::new("tasklist")
                .args(&["/FI", &format!("PID eq {}", pid)])
                .output()
                .map_err(|e| BlixardError::Internal {
                    message: format!("Failed to check process {}: {}", pid, e),
                })?;

            let stdout = String::from_utf8_lossy(&output.stdout);
            Ok(stdout.contains(&pid.to_string()))
        }
    }
}

impl TokioCommandExecutor {
    /// Internal method to execute a command
    async fn execute_command(&self, mut cmd: Command, options: &CommandOptions) -> BlixardResult<CommandOutput> {
        let mut child = cmd.spawn().map_err(|e| BlixardError::Internal {
            message: format!("Failed to spawn command: {}", e),
        })?;

        // Handle stdin if provided
        if let Some(stdin_data) = &options.stdin_input {
            if let Some(stdin) = child.stdin.take() {
                use tokio::io::AsyncWriteExt;
                let mut stdin = stdin;
                stdin.write_all(stdin_data).await.map_err(|e| BlixardError::Internal {
                    message: format!("Failed to write to stdin: {}", e),
                })?;
                stdin.shutdown().await.map_err(|e| BlixardError::Internal {
                    message: format!("Failed to close stdin: {}", e),
                })?;
            }
        }

        // Wait for completion
        let output = child.wait_with_output().await.map_err(|e| BlixardError::Internal {
            message: format!("Failed to wait for command: {}", e),
        })?;

        let exit_code = output.status.code().unwrap_or(-1);
        let success = output.status.success();

        Ok(CommandOutput {
            status: exit_code,
            stdout: output.stdout,
            stderr: output.stderr,
            success,
            duration: Duration::from_secs(0), // Will be set by caller
        })
    }
}

/// Mock implementation for testing
#[derive(Debug, Clone)]
pub struct MockCommandExecutor {
    /// Expected commands and their responses
    pub expectations: std::sync::Arc<std::sync::Mutex<Vec<MockExpectation>>>,
}

#[derive(Debug, Clone)]
pub struct MockExpectation {
    pub program: String,
    pub args: Vec<String>,
    pub response: BlixardResult<CommandOutput>,
}

impl MockCommandExecutor {
    /// Create a new mock executor
    pub fn new() -> Self {
        Self {
            expectations: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
        }
    }

    /// Add an expectation
    pub fn expect(&self, program: &str, args: &[&str], response: BlixardResult<CommandOutput>) {
        let mut expectations = self.expectations.lock().unwrap();
        expectations.push(MockExpectation {
            program: program.to_string(),
            args: args.iter().map(|s| s.to_string()).collect(),
            response,
        });
    }

    /// Verify all expectations were met
    pub fn verify(&self) -> BlixardResult<()> {
        let expectations = self.expectations.lock().unwrap();
        if expectations.is_empty() {
            Ok(())
        } else {
            Err(BlixardError::Internal {
                message: format!("{} expected commands were not executed", expectations.len()),
            })
        }
    }
}

#[async_trait]
impl CommandExecutor for MockCommandExecutor {
    async fn execute(
        &self,
        program: &str,
        args: &[&str],
        _options: CommandOptions,
    ) -> BlixardResult<CommandOutput> {
        let mut expectations = self.expectations.lock().unwrap();
        
        if let Some(pos) = expectations.iter().position(|exp| {
            exp.program == program && exp.args == args
        }) {
            let expectation = expectations.remove(pos);
            expectation.response
        } else {
            Err(BlixardError::Internal {
                message: format!("Unexpected command: {} {}", program, args.join(" ")),
            })
        }
    }

    async fn spawn(
        &self,
        program: &str,
        args: &[&str],
        _options: CommandOptions,
    ) -> BlixardResult<ProcessHandle> {
        // For mock, just return a fake handle
        debug!("Mock spawn: {} {}", program, args.join(" "));
        Ok(ProcessHandle::new(12345, tokio::process::Command::new("true").spawn().unwrap()))
    }

    async fn kill(&self, pid: u32) -> BlixardResult<()> {
        debug!("Mock kill: {}", pid);
        Ok(())
    }

    async fn is_running(&self, _pid: u32) -> BlixardResult<bool> {
        Ok(false) // Default to not running
    }
}

/// Factory for creating command executors
pub struct CommandExecutorFactory;

impl CommandExecutorFactory {
    /// Create a standard tokio-based executor
    pub fn create_tokio() -> Box<dyn CommandExecutor> {
        Box::new(TokioCommandExecutor::new())
    }

    /// Create a tokio executor with custom timeout
    pub fn create_tokio_with_timeout(timeout: Duration) -> Box<dyn CommandExecutor> {
        Box::new(TokioCommandExecutor::with_default_timeout(timeout))
    }

    /// Create a mock executor for testing
    pub fn create_mock() -> MockCommandExecutor {
        MockCommandExecutor::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_command_options_builder() {
        let options = CommandOptions::new()
            .with_cwd("/tmp")
            .with_env_var("TEST", "value")
            .with_timeout(Duration::from_secs(30))
            .with_output_capture();

        assert_eq!(options.cwd, Some(PathBuf::from("/tmp")));
        assert_eq!(options.env.as_ref().unwrap().get("TEST"), Some(&"value".to_string()));
        assert_eq!(options.timeout, Some(Duration::from_secs(30)));
        assert!(options.capture_output);
    }

    #[tokio::test]
    async fn test_mock_executor() {
        let executor = MockCommandExecutor::new();
        
        let expected_output = CommandOutput {
            status: 0,
            stdout: b"test output".to_vec(),
            stderr: vec![],
            success: true,
            duration: Duration::from_millis(100),
        };
        
        executor.expect("echo", &["hello"], Ok(expected_output.clone()));
        
        let result = executor.execute_simple("echo", &["hello"]).await.unwrap();
        assert_eq!(result.stdout, expected_output.stdout);
        assert!(result.success);
        
        executor.verify().unwrap();
    }

    #[tokio::test]
    async fn test_tokio_executor_simple() {
        let executor = TokioCommandExecutor::new();
        let result = executor.execute_simple("echo", &["hello"]).await.unwrap();
        
        assert!(result.success);
        assert_eq!(result.stdout_string().unwrap().trim(), "hello");
    }
}