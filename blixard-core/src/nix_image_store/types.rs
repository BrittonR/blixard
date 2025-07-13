//! Core data structures for the Nix-aware P2P VM Image Store
//!
//! This module contains all the type definitions used throughout the Nix image store
//! implementation, including metadata structures, chunk information, and transfer statistics.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::PathBuf;

/// Type of Nix artifact
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum NixArtifactType {
    /// MicroVM configuration and rootfs
    MicroVM {
        /// The microVM system name
        system_name: String,
        /// Whether this includes kernel
        has_kernel: bool,
    },
    /// OCI container image
    Container {
        /// Container name and tag
        image_ref: String,
        /// OCI manifest digest
        manifest_digest: String,
    },
    /// Raw Nix store path
    StorePath {
        /// Original store path (e.g., /nix/store/...)
        path: String,
        /// Whether this is a derivation
        is_derivation: bool,
    },
    /// Nix closure (multiple store paths)
    Closure {
        /// Root store path
        root_path: String,
        /// Total number of paths in closure
        path_count: usize,
    },
}

/// Metadata for a Nix-built image or artifact
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NixImageMetadata {
    /// Unique identifier (content-addressed)
    pub id: String,
    /// Human-readable name
    pub name: String,
    /// Nix derivation hash (if available)
    pub derivation_hash: Option<String>,
    /// Type of artifact
    pub artifact_type: NixArtifactType,
    /// NAR hash from Nix (sha256:xxx format)
    pub nar_hash: Option<String>,
    /// NAR size in bytes
    pub nar_size: Option<u64>,
    /// Total size in bytes
    pub total_size: u64,
    /// Creation timestamp
    pub created_at: DateTime<Utc>,
    /// Iroh hash of the content
    pub content_hash: String,
    /// Chunked hashes for deduplication
    pub chunk_hashes: Vec<ChunkMetadata>,
    /// Dependencies (other Nix paths)
    pub dependencies: Vec<String>,
    /// Runtime requirements
    pub runtime: RuntimeRequirements,
    /// Compression used
    pub compression: CompressionType,
    /// Signature for verification
    pub signature: Option<String>,
    /// Node that uploaded this
    pub uploaded_by: u64,
    /// Nodes that have this cached
    pub cached_on_nodes: HashSet<u64>,
}

/// Chunk metadata for deduplication
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkMetadata {
    /// Offset in the file
    pub offset: u64,
    /// Size of this chunk
    pub size: u64,
    /// Iroh hash of the chunk
    pub hash: String,
    /// Whether this chunk is commonly shared
    pub is_common: bool,
}

/// Runtime requirements for the artifact
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeRequirements {
    /// Minimum kernel version required
    pub min_kernel: Option<String>,
    /// Required CPU features
    pub cpu_features: Vec<String>,
    /// Minimum memory in MB
    pub min_memory_mb: u64,
    /// Required hypervisor features
    pub hypervisor_features: Vec<String>,
}

/// Compression type for images
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum CompressionType {
    None,
    Gzip,
    Zstd,
    Xz,
}

/// Transfer statistics
#[derive(Debug, Clone)]
pub struct TransferStats {
    /// Bytes transferred
    pub bytes_transferred: u64,
    /// Bytes deduplicated (not transferred)
    pub bytes_deduplicated: u64,
    /// Number of chunks transferred
    pub chunks_transferred: usize,
    /// Number of chunks deduplicated
    pub chunks_deduplicated: usize,
    /// Transfer duration
    pub duration: std::time::Duration,
}

/// Location of a chunk
#[derive(Debug, Clone)]
pub struct ChunkLocation {
    /// Which images contain this chunk
    pub image_ids: HashSet<String>,
    /// Local file path if cached
    pub local_path: Option<PathBuf>,
}