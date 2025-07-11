//! Console reader for microVM health monitoring
//!
//! This module provides functionality to read console output from microVMs
//! and match patterns for health checking.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};
use tokio::net::UnixStream;
use tokio::time::timeout;
use tracing::{debug, error, warn};

use blixard_core::{
    error::{BlixardError, BlixardResult},
};

/// Console reader configuration constants
mod constants {
    use std::time::Duration;
    
    /// Default buffer size for console reading (8KB)
    pub const DEFAULT_BUFFER_SIZE: usize = 8192;
    
    /// Default socket path prefix for VM consoles
    pub const SOCKET_PATH_PREFIX: &str = "/tmp";
    
    /// Default socket path suffix
    pub const SOCKET_PATH_SUFFIX: &str = "-console.sock";
    
    /// Server startup delay in tests
    pub const TEST_SERVER_STARTUP_DELAY: Duration = Duration::from_millis(10);
    
    /// Server connection hold time in tests
    pub const TEST_SERVER_HOLD_TIME: Duration = Duration::from_millis(100);
    
    /// Default pattern check timeout
    pub const DEFAULT_PATTERN_CHECK_TIMEOUT: Duration = Duration::from_secs(1);
}

/// Console reader for monitoring VM output
pub struct ConsoleReader {
    socket_path: String,
    buffer_size: usize,
}

impl ConsoleReader {
    /// Create a new console reader for a VM
    pub fn new(vm_name: &str) -> Self {
        Self {
            socket_path: format!("{}/{}{}", constants::SOCKET_PATH_PREFIX, vm_name, constants::SOCKET_PATH_SUFFIX),
            buffer_size: constants::DEFAULT_BUFFER_SIZE,
        }
    }

    /// Create with custom socket path
    pub fn with_socket_path(socket_path: String) -> Self {
        Self {
            socket_path,
            buffer_size: constants::DEFAULT_BUFFER_SIZE,
        }
    }

    /// Check if console socket exists
    pub async fn is_accessible(&self) -> bool {
        Path::new(&self.socket_path).exists()
    }

    /// Read console output and check for patterns
    ///
    /// Returns Ok(true) if healthy pattern is found, Ok(false) if unhealthy pattern is found,
    /// or error if timeout or connection failure.
    pub async fn check_patterns(
        &self,
        healthy_pattern: &str,
        unhealthy_pattern: Option<&str>,
        timeout_duration: Duration,
    ) -> BlixardResult<(bool, String)> {
        // Connect to console socket
        let stream = match UnixStream::connect(&self.socket_path).await {
            Ok(stream) => stream,
            Err(e) => {
                return Err(BlixardError::VmOperationFailed {
                    operation: "console_connect".to_string(),
                    details: format!("Failed to connect to console socket: {}", e),
                })
            }
        };

        // Compile regex patterns
        let healthy_regex = regex::Regex::new(healthy_pattern).map_err(|e| {
            BlixardError::InvalidInput {
                field: "healthy_pattern".to_string(),
                message: format!("Invalid regex: {}", e),
            }
        })?;

        let unhealthy_regex = if let Some(pattern) = unhealthy_pattern {
            Some(regex::Regex::new(pattern).map_err(|e| BlixardError::InvalidInput {
                field: "unhealthy_pattern".to_string(),
                message: format!("Invalid regex: {}", e),
            })?)
        } else {
            None
        };

        // Read console output with timeout
        match timeout(
            timeout_duration,
            self.read_until_pattern(stream, healthy_regex, unhealthy_regex),
        )
        .await
        {
            Ok(Ok((is_healthy, matched_line))) => Ok((is_healthy, matched_line)),
            Ok(Err(e)) => Err(e),
            Err(_) => Err(BlixardError::Timeout {
                operation: "console_read".to_string(),
                duration: timeout_duration,
            }),
        }
    }

    /// Read console output until a pattern is matched
    async fn read_until_pattern(
        &self,
        stream: UnixStream,
        healthy_regex: regex::Regex,
        unhealthy_regex: Option<regex::Regex>,
    ) -> BlixardResult<(bool, String)> {
        let mut reader = BufReader::with_capacity(self.buffer_size, stream);
        let mut line = String::new();
        let mut recent_lines = Vec::new();
        let max_recent_lines = 10;

        loop {
            line.clear();
            match reader.read_line(&mut line).await {
                Ok(0) => {
                    // EOF reached
                    return Err(BlixardError::VmOperationFailed {
                        operation: "console_read".to_string(),
                        details: "Console closed unexpectedly".to_string(),
                    });
                }
                Ok(_) => {
                    let line_trimmed = line.trim();
                    if !line_trimmed.is_empty() {
                        debug!("Console output: {}", line_trimmed);
                        
                        // Keep track of recent lines for context
                        recent_lines.push(line_trimmed.to_string());
                        if recent_lines.len() > max_recent_lines {
                            recent_lines.remove(0);
                        }

                        // Check unhealthy pattern first (higher priority)
                        if let Some(ref unhealthy) = unhealthy_regex {
                            if unhealthy.is_match(line_trimmed) {
                                warn!("Unhealthy pattern matched: {}", line_trimmed);
                                return Ok((false, format!(
                                    "Unhealthy pattern '{}' found: {}",
                                    unhealthy.as_str(),
                                    line_trimmed
                                )));
                            }
                        }

                        // Check healthy pattern
                        if healthy_regex.is_match(line_trimmed) {
                            debug!("Healthy pattern matched: {}", line_trimmed);
                            return Ok((true, format!(
                                "Healthy pattern '{}' found: {}",
                                healthy_regex.as_str(),
                                line_trimmed
                            )));
                        }
                    }
                }
                Err(e) => {
                    error!("Error reading console: {}", e);
                    return Err(BlixardError::VmOperationFailed {
                        operation: "console_read".to_string(),
                        details: format!("Failed to read console output: {}", e),
                    });
                }
            }
        }
    }

    /// Read recent console output (last N lines)
    pub async fn read_recent_output(
        &self,
        max_lines: usize,
        timeout_duration: Duration,
    ) -> BlixardResult<Vec<String>> {
        let stream = UnixStream::connect(&self.socket_path).await.map_err(|e| {
            BlixardError::VmOperationFailed {
                operation: "console_connect".to_string(),
                details: format!("Failed to connect to console socket: {}", e),
            }
        })?;

        let mut reader = BufReader::with_capacity(self.buffer_size, stream);
        let lines = Arc::new(tokio::sync::Mutex::new(Vec::new()));
        let lines_clone = lines.clone();
        let mut line = String::new();

        // Read with timeout
        let result = timeout(timeout_duration, async move {
            while lines_clone.lock().await.len() < max_lines {
                line.clear();
                match reader.read_line(&mut line).await {
                    Ok(0) => break, // EOF
                    Ok(_) => {
                        let line_trimmed = line.trim();
                        if !line_trimmed.is_empty() {
                            lines_clone.lock().await.push(line_trimmed.to_string());
                        }
                    }
                    Err(e) => {
                        return Err(BlixardError::VmOperationFailed {
                            operation: "console_read".to_string(),
                            details: format!("Failed to read console output: {}", e),
                        });
                    }
                }
            }
            Ok(())
        })
        .await;

        let lines_vec = lines.lock().await.clone();
        match result {
            Ok(Ok(())) => Ok(lines_vec),
            Ok(Err(e)) => Err(e),
            Err(_) => {
                // Timeout - return what we have
                Ok(lines_vec)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use tokio::io::AsyncWriteExt;
    use tokio::net::UnixListener;

    #[tokio::test]
    async fn test_console_reader_accessibility() {
        let reader = ConsoleReader::new("test-vm");
        assert!(!reader.is_accessible().await);
    }

    #[tokio::test]
    async fn test_console_pattern_matching() -> BlixardResult<()> {
        let dir = tempdir().map_err(|e| BlixardError::Io(e))?;
        let socket_path = dir.path().join("test.sock");
        let socket_path_str = socket_path.to_str()
            .ok_or_else(|| BlixardError::InvalidConfiguration {
                message: "Socket path contains invalid UTF-8".to_string(),
            })?
            .to_string();

        // Create a mock console server
        let listener = UnixListener::bind(&socket_path).map_err(|e| BlixardError::Io(e))?;
        
        // Spawn server task
        let server_handle = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.map_err(|e| BlixardError::Io(e))?;
            
            // Write some console output
            stream.write_all(b"Starting VM...\n").await.map_err(|e| BlixardError::Io(e))?;
            stream.write_all(b"Network initialized\n").await.map_err(|e| BlixardError::Io(e))?;
            stream.write_all(b"System ready\n").await.map_err(|e| BlixardError::Io(e))?;
            stream.flush().await.map_err(|e| BlixardError::Io(e))?;
            
            // Keep connection open briefly
            tokio::time::sleep(constants::TEST_SERVER_HOLD_TIME).await;
        });

        // Create reader and check patterns
        let reader = ConsoleReader::with_socket_path(socket_path_str);
        
        // Give server time to start
        tokio::time::sleep(constants::TEST_SERVER_STARTUP_DELAY).await;
        
        let (is_healthy, message) = reader
            .check_patterns("System ready", None, constants::DEFAULT_PATTERN_CHECK_TIMEOUT)
            .await
            ?;
        
        assert!(is_healthy);
        assert!(message.contains("System ready"));
        
        server_handle.abort();
        Ok(())
    }

    #[tokio::test]
    async fn test_unhealthy_pattern_detection() -> BlixardResult<()> {
        let dir = tempdir().map_err(|e| BlixardError::Io(e))?;
        let socket_path = dir.path().join("test.sock");
        let socket_path_str = socket_path.to_str()
            .ok_or_else(|| BlixardError::InvalidConfiguration {
                message: "Socket path contains invalid UTF-8".to_string(),
            })?
            .to_string();

        // Create a mock console server
        let listener = UnixListener::bind(&socket_path).map_err(|e| BlixardError::Io(e))?;
        
        // Spawn server task
        let server_handle = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.map_err(|e| BlixardError::Io(e))?;
            
            // Write some console output with error
            stream.write_all(b"Starting VM...\n").await.map_err(|e| BlixardError::Io(e))?;
            stream.write_all(b"PANIC: kernel panic\n").await.map_err(|e| BlixardError::Io(e))?;
            stream.write_all(b"System halted\n").await.map_err(|e| BlixardError::Io(e))?;
            stream.flush().await.map_err(|e| BlixardError::Io(e))?;
            
            // Keep connection open briefly
            tokio::time::sleep(constants::TEST_SERVER_HOLD_TIME).await;
        });

        // Create reader and check patterns
        let reader = ConsoleReader::with_socket_path(socket_path_str);
        
        // Give server time to start
        tokio::time::sleep(constants::TEST_SERVER_STARTUP_DELAY).await;
        
        let (is_healthy, message) = reader
            .check_patterns("System ready", Some("PANIC"), constants::DEFAULT_PATTERN_CHECK_TIMEOUT)
            .await
            ?;
        
        assert!(!is_healthy);
        assert!(message.contains("PANIC"));
        
        server_handle.abort();
        Ok(())
    }
}