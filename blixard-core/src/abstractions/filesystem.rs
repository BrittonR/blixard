//! Filesystem abstractions for testability
//!
//! This module provides trait-based abstractions for filesystem operations,
//! enabling testing without actual file I/O.

use crate::error::BlixardResult;
use async_trait::async_trait;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// File metadata information
#[derive(Debug, Clone)]
pub struct FileMetadata {
    /// File size in bytes
    pub size: u64,
    /// Whether this is a directory
    pub is_dir: bool,
    /// Whether this is a regular file
    pub is_file: bool,
    /// Last modified time
    pub modified: Option<SystemTime>,
    /// Creation time (if available)
    pub created: Option<SystemTime>,
}

/// Abstraction for filesystem operations
#[async_trait]
pub trait FileSystem: Send + Sync {
    /// Check if a path exists
    async fn exists(&self, path: &Path) -> BlixardResult<bool>;

    /// Create a directory (including parents)
    async fn create_dir_all(&self, path: &Path) -> BlixardResult<()>;

    /// Read file contents as bytes
    async fn read(&self, path: &Path) -> BlixardResult<Vec<u8>>;

    /// Read file contents as string
    async fn read_to_string(&self, path: &Path) -> BlixardResult<String>;

    /// Write bytes to file
    async fn write(&self, path: &Path, contents: &[u8]) -> BlixardResult<()>;

    /// Write string to file
    async fn write_string(&self, path: &Path, contents: &str) -> BlixardResult<()>;

    /// Remove a file
    async fn remove_file(&self, path: &Path) -> BlixardResult<()>;

    /// Remove a directory (recursively)
    async fn remove_dir_all(&self, path: &Path) -> BlixardResult<()>;

    /// Copy a file
    async fn copy(&self, from: &Path, to: &Path) -> BlixardResult<()>;

    /// Move/rename a file
    async fn rename(&self, from: &Path, to: &Path) -> BlixardResult<()>;

    /// Set Unix permissions (no-op on non-Unix)
    async fn set_permissions(&self, path: &Path, mode: u32) -> BlixardResult<()>;

    /// Get canonical path
    async fn canonicalize(&self, path: &Path) -> BlixardResult<PathBuf>;

    /// Read directory entries
    async fn read_dir(&self, path: &Path) -> BlixardResult<Vec<PathBuf>>;

    /// Get file metadata (size, modified time, etc.)
    async fn metadata(&self, path: &Path) -> BlixardResult<FileMetadata>;
}

// Production implementation using tokio::fs

/// Production filesystem implementation
pub struct TokioFileSystem;

impl TokioFileSystem {
    /// Create new instance
    pub fn new() -> Self {
        Self
    }
}

impl Default for TokioFileSystem {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl FileSystem for TokioFileSystem {
    async fn exists(&self, path: &Path) -> BlixardResult<bool> {
        Ok(tokio::fs::try_exists(path).await?)
    }

    async fn create_dir_all(&self, path: &Path) -> BlixardResult<()> {
        tokio::fs::create_dir_all(path).await?;
        Ok(())
    }

    async fn read(&self, path: &Path) -> BlixardResult<Vec<u8>> {
        Ok(tokio::fs::read(path).await?)
    }

    async fn read_to_string(&self, path: &Path) -> BlixardResult<String> {
        Ok(tokio::fs::read_to_string(path).await?)
    }

    async fn write(&self, path: &Path, contents: &[u8]) -> BlixardResult<()> {
        tokio::fs::write(path, contents).await?;
        Ok(())
    }

    async fn write_string(&self, path: &Path, contents: &str) -> BlixardResult<()> {
        tokio::fs::write(path, contents).await?;
        Ok(())
    }

    async fn remove_file(&self, path: &Path) -> BlixardResult<()> {
        tokio::fs::remove_file(path).await?;
        Ok(())
    }

    async fn remove_dir_all(&self, path: &Path) -> BlixardResult<()> {
        tokio::fs::remove_dir_all(path).await?;
        Ok(())
    }

    async fn copy(&self, from: &Path, to: &Path) -> BlixardResult<()> {
        tokio::fs::copy(from, to).await?;
        Ok(())
    }

    async fn rename(&self, from: &Path, to: &Path) -> BlixardResult<()> {
        tokio::fs::rename(from, to).await?;
        Ok(())
    }

    async fn set_permissions(&self, path: &Path, mode: u32) -> BlixardResult<()> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let permissions = std::fs::Permissions::from_mode(mode);
            tokio::fs::set_permissions(path, permissions).await?;
        }
        Ok(())
    }

    async fn canonicalize(&self, path: &Path) -> BlixardResult<PathBuf> {
        Ok(tokio::fs::canonicalize(path).await?)
    }

    async fn read_dir(&self, path: &Path) -> BlixardResult<Vec<PathBuf>> {
        let mut entries = tokio::fs::read_dir(path).await?;
        let mut paths = Vec::new();

        while let Some(entry) = entries.next_entry().await? {
            paths.push(entry.path());
        }

        Ok(paths)
    }

    async fn metadata(&self, path: &Path) -> BlixardResult<FileMetadata> {
        let metadata = tokio::fs::metadata(path).await?;

        Ok(FileMetadata {
            size: metadata.len(),
            is_dir: metadata.is_dir(),
            is_file: metadata.is_file(),
            modified: metadata.modified().ok(),
            created: metadata.created().ok(),
        })
    }
}

// Mock implementation for testing

use std::collections::HashMap;
use tokio::sync::RwLock;

/// Mock filesystem for testing
pub struct MockFileSystem {
    files: Arc<RwLock<HashMap<PathBuf, Vec<u8>>>>,
    permissions: Arc<RwLock<HashMap<PathBuf, u32>>>,
}

impl MockFileSystem {
    /// Create new mock filesystem
    pub fn new() -> Self {
        Self {
            files: Arc::new(RwLock::new(HashMap::new())),
            permissions: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Add a file to the mock filesystem
    pub async fn add_file(&self, path: PathBuf, contents: Vec<u8>) {
        self.files.write().await.insert(path, contents);
    }

    /// Add a text file to the mock filesystem
    pub async fn add_text_file(&self, path: PathBuf, contents: &str) {
        self.add_file(path, contents.as_bytes().to_vec()).await;
    }
}

impl Default for MockFileSystem {
    fn default() -> Self {
        Self::new()
    }
}

use std::sync::Arc;

#[async_trait]
impl FileSystem for MockFileSystem {
    async fn exists(&self, path: &Path) -> BlixardResult<bool> {
        Ok(self.files.read().await.contains_key(path))
    }

    async fn create_dir_all(&self, _path: &Path) -> BlixardResult<()> {
        // No-op for mock - directories are implicit
        Ok(())
    }

    async fn read(&self, path: &Path) -> BlixardResult<Vec<u8>> {
        self.files.read().await.get(path).cloned().ok_or_else(|| {
            crate::error::BlixardError::IoError(Box::new(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "File not found",
            )))
        })
    }

    async fn read_to_string(&self, path: &Path) -> BlixardResult<String> {
        let bytes = self.read(path).await?;
        String::from_utf8(bytes).map_err(|e| crate::error::BlixardError::Internal {
            message: format!("Invalid UTF-8: {}", e),
        })
    }

    async fn write(&self, path: &Path, contents: &[u8]) -> BlixardResult<()> {
        self.files
            .write()
            .await
            .insert(path.to_path_buf(), contents.to_vec());
        Ok(())
    }

    async fn write_string(&self, path: &Path, contents: &str) -> BlixardResult<()> {
        self.write(path, contents.as_bytes()).await
    }

    async fn remove_file(&self, path: &Path) -> BlixardResult<()> {
        self.files.write().await.remove(path);
        self.permissions.write().await.remove(path);
        Ok(())
    }

    async fn remove_dir_all(&self, path: &Path) -> BlixardResult<()> {
        let mut files = self.files.write().await;
        let mut permissions = self.permissions.write().await;

        // Remove all files under this directory
        let to_remove: Vec<_> = files
            .keys()
            .filter(|p| p.starts_with(path))
            .cloned()
            .collect();

        for p in to_remove {
            files.remove(&p);
            permissions.remove(&p);
        }

        Ok(())
    }

    async fn copy(&self, from: &Path, to: &Path) -> BlixardResult<()> {
        let contents = self.read(from).await?;
        self.write(to, &contents).await?;

        // Copy permissions if they exist
        if let Some(perms) = self.permissions.read().await.get(from) {
            self.permissions
                .write()
                .await
                .insert(to.to_path_buf(), *perms);
        }

        Ok(())
    }

    async fn rename(&self, from: &Path, to: &Path) -> BlixardResult<()> {
        self.copy(from, to).await?;
        self.remove_file(from).await?;
        Ok(())
    }

    async fn set_permissions(&self, path: &Path, mode: u32) -> BlixardResult<()> {
        self.permissions
            .write()
            .await
            .insert(path.to_path_buf(), mode);
        Ok(())
    }

    async fn canonicalize(&self, path: &Path) -> BlixardResult<PathBuf> {
        // Simple mock - just return the path as-is if it exists
        if self.exists(path).await? {
            Ok(path.to_path_buf())
        } else {
            Err(crate::error::BlixardError::IoError(Box::new(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "Path not found",
            ))))
        }
    }

    async fn read_dir(&self, path: &Path) -> BlixardResult<Vec<PathBuf>> {
        let files = self.files.read().await;

        // Find all files that have this path as a parent
        let mut entries = Vec::new();
        for file_path in files.keys() {
            if let Some(parent) = file_path.parent() {
                if parent == path {
                    entries.push(file_path.clone());
                }
            }
        }

        Ok(entries)
    }

    async fn metadata(&self, path: &Path) -> BlixardResult<FileMetadata> {
        let files = self.files.read().await;

        if let Some(content) = files.get(path) {
            Ok(FileMetadata {
                size: content.len() as u64,
                is_dir: false, // Mock filesystem treats everything as files
                is_file: true,
                modified: Some(SystemTime::now()),
                created: Some(SystemTime::now()),
            })
        } else {
            Err(crate::error::BlixardError::IoError(Box::new(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "File not found",
            ))))
        }
    }
}
