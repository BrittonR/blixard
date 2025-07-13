//! Unified file I/O utilities to reduce code duplication
//!
//! This module provides common file reading and writing patterns
//! with consistent error handling and context.

use crate::error::{BlixardError, BlixardResult};
use serde::de::DeserializeOwned;
use std::path::Path;
use tracing::debug;

/// Read a text file with context-aware error handling
pub async fn read_text_file_with_context<P: AsRef<Path>>(
    path: P,
    context: &str,
) -> BlixardResult<String> {
    let path = path.as_ref();
    debug!("Reading {} from {:?}", context, path);

    tokio::fs::read_to_string(path)
        .await
        .map_err(|e| BlixardError::IoError(Box::new(e)))
        .map_err(|e| match &e {
            BlixardError::IoError(io_err) => BlixardError::ConfigurationError {
                component: "file_io".to_string(),
                message: format!("Failed to read {}: {} (path: {:?})", context, io_err, path),
            },
            _ => e,
        })
}

/// Read a binary file with context-aware error handling
pub async fn read_binary_file_with_context<P: AsRef<Path>>(
    path: P,
    context: &str,
) -> BlixardResult<Vec<u8>> {
    let path = path.as_ref();
    debug!("Reading {} from {:?}", context, path);

    tokio::fs::read(path)
        .await
        .map_err(|e| BlixardError::IoError(Box::new(e)))
        .map_err(|e| match &e {
            BlixardError::IoError(io_err) => BlixardError::ConfigurationError {
                component: "file_io".to_string(),
                message: format!("Failed to read {}: {} (path: {:?})", context, io_err, path),
            },
            _ => e,
        })
}

/// Read and deserialize a configuration file (JSON/TOML/YAML)
pub async fn read_config_file<T, P>(path: P, file_type: &str) -> BlixardResult<T>
where
    T: DeserializeOwned,
    P: AsRef<Path>,
{
    let path = path.as_ref();
    let content =
        read_text_file_with_context(path, &format!("{} configuration", file_type)).await?;

    // Determine format based on extension or file_type hint
    let extension = path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or(file_type);

    match extension {
        "json" => serde_json::from_str(&content).map_err(|e| {
            BlixardError::ConfigurationError {
                component: "json_parser".to_string(),
                message: format!("Failed to parse JSON {}: {}", file_type, e),
            }
        }),
        "toml" => toml::from_str(&content).map_err(|e| {
            BlixardError::ConfigurationError {
                component: "toml_parser".to_string(),
                message: format!("Failed to parse TOML {}: {}", file_type, e),
            }
        }),
        "yaml" | "yml" => serde_yaml::from_str(&content).map_err(|e| {
            BlixardError::ConfigurationError {
                component: "yaml_parser".to_string(),
                message: format!("Failed to parse YAML {}: {}", file_type, e),
            }
        }),
        _ => {
            // Try to auto-detect format
            if let Ok(parsed) = serde_json::from_str(&content) {
                Ok(parsed)
            } else if let Ok(parsed) = toml::from_str(&content) {
                Ok(parsed)
            } else if let Ok(parsed) = serde_yaml::from_str(&content) {
                Ok(parsed)
            } else {
                Err(BlixardError::ConfigurationError {
                    component: "format_detection".to_string(),
                    message: format!("Unable to parse {} - unknown format", file_type),
                })
            }
        }
    }
}

/// Write text to a file with context-aware error handling
pub async fn write_text_file_with_context<P: AsRef<Path>>(
    path: P,
    content: &str,
    context: &str,
) -> BlixardResult<()> {
    let path = path.as_ref();
    debug!("Writing {} to {:?}", context, path);

    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await.map_err(|e| {
            BlixardError::ConfigurationError {
                component: "directory_creation".to_string(),
                message: format!("Failed to create directory for {}: {}", context, e),
            }
        })?;
    }

    tokio::fs::write(path, content).await.map_err(|e| {
        BlixardError::ConfigurationError {
            component: "file_write".to_string(),
            message: format!("Failed to write {}: {} (path: {:?})", context, e, path),
        }
    })
}

/// Write binary data to a file with context-aware error handling
pub async fn write_binary_file_with_context<P: AsRef<Path>>(
    path: P,
    content: &[u8],
    context: &str,
) -> BlixardResult<()> {
    let path = path.as_ref();
    debug!("Writing {} to {:?}", context, path);

    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await.map_err(|e| {
            BlixardError::ConfigurationError {
                component: "directory_creation".to_string(),
                message: format!("Failed to create directory for {}: {}", context, e),
            }
        })?;
    }

    tokio::fs::write(path, content).await.map_err(|e| {
        BlixardError::ConfigurationError {
            component: "file_write".to_string(),
            message: format!("Failed to write {}: {} (path: {:?})", context, e, path),
        }
    })
}

/// Write and serialize a configuration object to file
pub async fn write_config_file<T, P>(
    path: P,
    data: &T,
    file_type: &str,
    pretty: bool,
) -> BlixardResult<()>
where
    T: serde::Serialize,
    P: AsRef<Path>,
{
    let path = path.as_ref();

    // Determine format based on extension or file_type hint
    let extension = path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or(file_type);

    let content = match extension {
        "json" => if pretty {
            serde_json::to_string_pretty(data)
        } else {
            serde_json::to_string(data)
        }
        .map_err(|e| {
            BlixardError::ConfigurationError {
                component: "json_serializer".to_string(),
                message: format!("Failed to serialize JSON {}: {}", file_type, e),
            }
        })?,
        "toml" => if pretty {
            toml::to_string_pretty(data)
        } else {
            toml::to_string(data)
        }
        .map_err(|e| {
            BlixardError::ConfigurationError {
                component: "toml_serializer".to_string(),
                message: format!("Failed to serialize TOML {}: {}", file_type, e),
            }
        })?,
        "yaml" | "yml" => serde_yaml::to_string(data).map_err(|e| {
            BlixardError::ConfigurationError {
                component: "yaml_serializer".to_string(),
                message: format!("Failed to serialize YAML {}: {}", file_type, e),
            }
        })?,
        _ => {
            return Err(BlixardError::ConfigurationError {
                component: "format_support".to_string(),
                message: format!("Unsupported configuration format: {}", extension),
            });
        }
    };

    write_text_file_with_context(path, &content, &format!("{} configuration", file_type)).await
}

/// Check if a file exists
pub async fn file_exists<P: AsRef<Path>>(path: P) -> bool {
    tokio::fs::try_exists(path).await.unwrap_or(false)
}

/// Create directory if it doesn't exist
pub async fn ensure_directory<P: AsRef<Path>>(path: P) -> BlixardResult<()> {
    let path = path.as_ref();

    if !file_exists(path).await {
        tokio::fs::create_dir_all(path).await.map_err(|e| {
            BlixardError::ConfigurationError {
                component: "directory_creation".to_string(),
                message: format!("Failed to create directory {:?}: {}", path, e),
            }
        })?;
    }

    Ok(())
}

/// Read directory entries with error context
pub async fn read_directory_with_context<P: AsRef<Path>>(
    path: P,
    context: &str,
) -> BlixardResult<Vec<std::path::PathBuf>> {
    let path = path.as_ref();
    let mut entries = tokio::fs::read_dir(path).await.map_err(|e| {
        BlixardError::ConfigurationError {
            component: "directory_read".to_string(),
            message: format!("Failed to read {} directory {:?}: {}", context, path, e),
        }
    })?;

    let mut paths = Vec::new();
    while let Some(entry) = entries.next_entry().await.map_err(|e| {
        BlixardError::ConfigurationError {
            component: "directory_read".to_string(),
            message: format!("Failed to read directory entry in {}: {}", context, e),
        }
    })? {
        paths.push(entry.path());
    }

    Ok(paths)
}

/// Read all files with a specific extension from a directory
pub async fn read_files_with_extension<P: AsRef<Path>>(
    dir_path: P,
    extension: &str,
    context: &str,
) -> BlixardResult<Vec<(std::path::PathBuf, String)>> {
    let paths = read_directory_with_context(dir_path, context).await?;
    let mut files = Vec::new();

    for path in paths {
        if path.extension().and_then(|s| s.to_str()) == Some(extension) {
            let content =
                read_text_file_with_context(&path, &format!("{} file", extension)).await?;
            files.push((path, content));
        }
    }

    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[derive(Debug, serde::Serialize, serde::Deserialize, PartialEq)]
    struct TestConfig {
        name: String,
        value: u32,
    }

    #[tokio::test]
    async fn test_read_write_text_file() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");

        // Write
        write_text_file_with_context(&file_path, "Hello, World!", "test file")
            .await
            .unwrap();

        // Read
        let content = read_text_file_with_context(&file_path, "test file")
            .await
            .unwrap();

        assert_eq!(content, "Hello, World!");
    }

    #[tokio::test]
    async fn test_config_file_json() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("config.json");

        let config = TestConfig {
            name: "test".to_string(),
            value: 42,
        };

        // Write
        write_config_file(&file_path, &config, "json", true)
            .await
            .unwrap();

        // Read
        let loaded: TestConfig = read_config_file(&file_path, "json").await.unwrap();

        assert_eq!(loaded, config);
    }

    #[tokio::test]
    async fn test_config_file_toml() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("config.toml");

        let config = TestConfig {
            name: "test".to_string(),
            value: 42,
        };

        // Write
        write_config_file(&file_path, &config, "toml", true)
            .await
            .unwrap();

        // Read
        let loaded: TestConfig = read_config_file(&file_path, "toml").await.unwrap();

        assert_eq!(loaded, config);
    }

    #[tokio::test]
    async fn test_read_files_with_extension() {
        let temp_dir = TempDir::new().unwrap();

        // Create test files
        let file1 = temp_dir.path().join("test1.txt");
        let file2 = temp_dir.path().join("test2.txt");
        let file3 = temp_dir.path().join("test3.log");

        tokio::fs::write(&file1, "Content 1").await.unwrap();
        tokio::fs::write(&file2, "Content 2").await.unwrap();
        tokio::fs::write(&file3, "Log content").await.unwrap();

        // Read only .txt files
        let files = read_files_with_extension(temp_dir.path(), "txt", "test")
            .await
            .unwrap();

        assert_eq!(files.len(), 2);
        assert!(files.iter().any(|(_, content)| content == "Content 1"));
        assert!(files.iter().any(|(_, content)| content == "Content 2"));
    }

    #[tokio::test]
    async fn test_error_handling() {
        let result = read_text_file_with_context("/non/existent/file", "test").await;

        assert!(result.is_err());
        match result.unwrap_err() {
            BlixardError::ConfigurationError { component: _, message } => {
                assert!(message.contains("Failed to read test"));
            }
            _ => panic!("Expected ConfigError"),
        }
    }
}
