//! Cluster state export/import functionality
//!
//! This module provides functionality to export and import entire cluster state,
//! including node configurations, VM definitions, and metadata.

use crate::error::{BlixardError, BlixardResult};
// TODO: Re-enable when P2P is fixed
// use crate::iroh_transport::{IrohTransport, DocumentType};
use crate::types::{NodeConfig, VmConfig};
use crate::storage::Storage;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use chrono::{DateTime, Utc};
use tracing::{info, warn};

/// Version of the cluster state export format
const EXPORT_FORMAT_VERSION: &str = "1.0";

/// Complete cluster state for export/import
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterState {
    /// Export format version
    pub version: String,
    
    /// Timestamp when the export was created
    pub exported_at: DateTime<Utc>,
    
    /// Cluster name
    pub cluster_name: String,
    
    /// Exporting node ID
    pub exported_by_node: u64,
    
    /// Node configurations
    pub nodes: Vec<NodeConfig>,
    
    /// VM configurations
    pub vms: HashMap<String, VmConfig>,
    
    /// Raft configuration
    pub raft_state: RaftExportState,
    
    /// Custom metadata
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Raft state export
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RaftExportState {
    /// Current term
    pub term: u64,
    
    /// Current leader ID
    pub leader_id: Option<u64>,
    
    /// Cluster membership
    pub members: Vec<u64>,
}

/// Options for cluster state export
#[derive(Debug, Clone)]
pub struct ExportOptions {
    /// Include VM disk images in export
    pub include_vm_images: bool,
    
    /// Include logs and metrics data
    pub include_telemetry: bool,
    
    /// Compress the export file
    pub compress: bool,
    
    /// Encryption key for sensitive data
    pub encryption_key: Option<String>,
}

impl Default for ExportOptions {
    fn default() -> Self {
        Self {
            include_vm_images: false,
            include_telemetry: false,
            compress: true,
            encryption_key: None,
        }
    }
}

/// Cluster state manager for export/import operations
pub struct ClusterStateManager {
    node_id: u64,
    storage: Arc<dyn Storage>,
    // TODO: Re-enable when P2P is fixed
    // transport: Option<Arc<IrohTransport>>,
}

impl ClusterStateManager {
    /// Create a new cluster state manager
    pub fn new(node_id: u64, storage: Arc<dyn Storage>, _transport: Option<()>) -> Self {
        Self {
            node_id,
            storage,
            // transport,
        }
    }
    
    /// Export the current cluster state
    pub async fn export_state(
        &self,
        cluster_name: &str,
        options: &ExportOptions,
    ) -> BlixardResult<ClusterState> {
        info!("Exporting cluster state for cluster: {}", cluster_name);
        
        // Collect node configurations
        let nodes = self.collect_node_configs().await?;
        
        // Collect VM configurations
        let vms = self.collect_vm_configs().await?;
        
        // Get Raft state
        let raft_state = self.get_raft_state().await?;
        
        // Collect metadata
        let mut metadata = HashMap::new();
        metadata.insert("node_count".to_string(), serde_json::json!(nodes.len()));
        metadata.insert("vm_count".to_string(), serde_json::json!(vms.len()));
        
        if options.include_telemetry {
            // Add telemetry data to metadata
            metadata.insert("telemetry_included".to_string(), serde_json::json!(true));
        }
        
        let state = ClusterState {
            version: EXPORT_FORMAT_VERSION.to_string(),
            exported_at: Utc::now(),
            cluster_name: cluster_name.to_string(),
            exported_by_node: self.node_id,
            nodes,
            vms,
            raft_state,
            metadata,
        };
        
        info!("Cluster state export completed: {} nodes, {} VMs", 
              state.nodes.len(), state.vms.len());
        
        Ok(state)
    }
    
    /// Export cluster state to a file
    pub async fn export_to_file(
        &self,
        cluster_name: &str,
        output_path: &Path,
        options: &ExportOptions,
    ) -> BlixardResult<()> {
        let state = self.export_state(cluster_name, options).await?;
        
        // Serialize to JSON
        let json_data = serde_json::to_string_pretty(&state)
            .map_err(|e| BlixardError::JsonError(e))?;
        
        if options.compress {
            // Compress with gzip
            use flate2::write::GzEncoder;
            use flate2::Compression;
            use std::io::Write as IoWrite;
            
            let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
            encoder.write_all(json_data.as_bytes())
                .map_err(|e| BlixardError::IoError(e))?;
            let compressed = encoder.finish()
                .map_err(|e| BlixardError::IoError(e))?;
            
            tokio::fs::write(output_path, compressed).await
                .map_err(|e| BlixardError::IoError(e))?;
        } else {
            tokio::fs::write(output_path, json_data).await
                .map_err(|e| BlixardError::IoError(e))?;
        }
        
        info!("Cluster state exported to: {:?}", output_path);
        Ok(())
    }
    
    /// Import cluster state
    pub async fn import_state(
        &self,
        state: &ClusterState,
        merge: bool,
    ) -> BlixardResult<()> {
        info!("Importing cluster state: {} (merge: {})", state.cluster_name, merge);
        
        // Validate export format version
        if state.version != EXPORT_FORMAT_VERSION {
            return Err(BlixardError::Internal {
                message: format!("Unsupported export format version: {}", state.version),
            });
        }
        
        // Import node configurations
        self.import_node_configs(&state.nodes, merge).await?;
        
        // Import VM configurations
        self.import_vm_configs(&state.vms, merge).await?;
        
        // Update metadata
        if let Err(e) = self.update_import_metadata(state).await {
            warn!("Failed to update import metadata: {}", e);
        }
        
        info!("Cluster state import completed");
        Ok(())
    }
    
    /// Import cluster state from a file
    pub async fn import_from_file(
        &self,
        input_path: &Path,
        merge: bool,
    ) -> BlixardResult<()> {
        let file_data = tokio::fs::read(input_path).await
            .map_err(|e| BlixardError::IoError(e))?;
        
        // Try to decompress if it's gzipped
        let json_data = if file_data.starts_with(&[0x1f, 0x8b]) {
            // Gzip magic bytes
            use flate2::read::GzDecoder;
            use std::io::Read as IoRead;
            
            let mut decoder = GzDecoder::new(&file_data[..]);
            let mut decompressed = String::new();
            decoder.read_to_string(&mut decompressed)
                .map_err(|e| BlixardError::IoError(e))?;
            decompressed
        } else {
            String::from_utf8(file_data)
                .map_err(|e| BlixardError::Internal {
                    message: format!("Invalid UTF-8 in export file: {}", e),
                })?
        };
        
        let state: ClusterState = serde_json::from_str(&json_data)
            .map_err(|e| BlixardError::JsonError(e))?;
        
        self.import_state(&state, merge).await
    }
    
    // TODO: Re-enable when P2P is fixed
    // /// Share cluster state via P2P
    // pub async fn share_state_p2p(
    //     &self,
    //     cluster_name: &str,
    //     options: &ExportOptions,
    // ) -> BlixardResult<String> {
    //     let transport = self.transport.as_ref()
    //         .ok_or_else(|| BlixardError::Internal {
    //             message: "P2P transport not available".to_string(),
    //         })?;
    //     
    //     let state = self.export_state(cluster_name, options).await?;
    //     
    //     // Serialize state
    //     let json_data = serde_json::to_vec(&state)
    //         .map_err(|e| BlixardError::JsonError(e))?;
    //     
    //     // Write to cluster config document
    //     let key = format!("cluster-state-{}", Utc::now().timestamp()).into_bytes();
    //     transport.write_to_doc(DocumentType::ClusterConfig, &key, &json_data).await?;
    //     
    //     // Get document ticket for sharing
    //     let ticket = transport.get_doc_ticket(DocumentType::ClusterConfig).await?;
    //     
    //     info!("Cluster state shared via P2P");
    //     Ok(ticket.to_string())
    // }
    
    /// Import cluster state from P2P
    pub async fn import_state_p2p(
        &self,
        ticket: &str,
        merge: bool,
    ) -> BlixardResult<()> {
        // TODO: Re-enable when P2P is fixed
        return Err(BlixardError::NotImplemented {
            feature: "P2P import".to_string(),
        });
        #[allow(unreachable_code)]
        // let transport = self.transport.as_ref()
        //     .ok_or_else(|| BlixardError::Internal {
        //         message: "P2P transport not available".to_string(),
        //     })?;
        
        // Parse and join document
        // let doc_ticket = ticket.parse()
        //     .map_err(|e| BlixardError::Internal {
        //         message: format!("Invalid document ticket: {}", e),
        //     })?;
        
        // transport.join_doc_from_ticket(&doc_ticket, DocumentType::ClusterConfig).await?;
        
        // Read the latest cluster state
        // For now, we'll use a simple approach - in production, you'd want to
        // list all keys and find the most recent one
        let key = b"cluster-state-latest";
        let data = Vec::new(); // transport.read_from_doc(DocumentType::ClusterConfig, key).await?
        if data.is_empty() {
            return Err(BlixardError::NotFound {
                resource: "Cluster state in P2P document".to_string(),
            });
        }
        
        let state: ClusterState = serde_json::from_slice(&data)
            .map_err(|e| BlixardError::JsonError(e))?;
        
        self.import_state(&state, merge).await
    }
    
    // Private helper methods
    
    async fn collect_node_configs(&self) -> BlixardResult<Vec<NodeConfig>> {
        // In a real implementation, this would query the storage
        // and collect all node configurations
        Ok(vec![])
    }
    
    async fn collect_vm_configs(&self) -> BlixardResult<HashMap<String, VmConfig>> {
        // In a real implementation, this would query the storage
        // and collect all VM configurations
        Ok(HashMap::new())
    }
    
    async fn get_raft_state(&self) -> BlixardResult<RaftExportState> {
        // In a real implementation, this would query the Raft state
        Ok(RaftExportState {
            term: 0,
            leader_id: None,
            members: vec![self.node_id],
        })
    }
    
    async fn import_node_configs(&self, nodes: &[NodeConfig], merge: bool) -> BlixardResult<()> {
        // In a real implementation, this would update the storage
        // with the imported node configurations
        if !merge {
            info!("Replacing existing node configurations");
        } else {
            info!("Merging with existing node configurations");
        }
        
        for node in nodes {
            info!("Importing node configuration: {}", node.id);
        }
        
        Ok(())
    }
    
    async fn import_vm_configs(&self, vms: &HashMap<String, VmConfig>, merge: bool) -> BlixardResult<()> {
        // In a real implementation, this would update the storage
        // with the imported VM configurations
        if !merge {
            info!("Replacing existing VM configurations");
        } else {
            info!("Merging with existing VM configurations");
        }
        
        for (name, config) in vms {
            info!("Importing VM configuration: {}", name);
        }
        
        Ok(())
    }
    
    async fn update_import_metadata(&self, state: &ClusterState) -> BlixardResult<()> {
        // Store metadata about the import operation
        let import_info = serde_json::json!({
            "imported_at": Utc::now(),
            "imported_from_node": state.exported_by_node,
            "original_export_time": state.exported_at,
            "cluster_name": state.cluster_name,
        });
        
        // In a real implementation, this would store the metadata
        info!("Import metadata updated: {:?}", import_info);
        
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    
    fn create_test_storage(temp_dir: &TempDir) -> Arc<dyn Storage> {
        use redb::Database;
        use crate::storage::RedbRaftStorage;
        
        let db_path = temp_dir.path().join("test.db");
        let database = Arc::new(Database::create(&db_path).unwrap());
        Arc::new(RedbRaftStorage { database })
    }
    
    #[tokio::test]
    async fn test_cluster_state_export() {
        let temp_dir = TempDir::new().unwrap();
        let storage = create_test_storage(&temp_dir);
        
        let manager = ClusterStateManager::new(1, storage, None);
        let options = ExportOptions::default();
        
        let state = manager.export_state("test-cluster", &options).await.unwrap();
        
        assert_eq!(state.version, EXPORT_FORMAT_VERSION);
        assert_eq!(state.cluster_name, "test-cluster");
        assert_eq!(state.exported_by_node, 1);
    }
    
    #[tokio::test]
    async fn test_cluster_state_file_operations() {
        let temp_dir = TempDir::new().unwrap();
        let storage = create_test_storage(&temp_dir);
        
        let manager = ClusterStateManager::new(1, storage, None);
        let options = ExportOptions {
            compress: true,
            ..Default::default()
        };
        
        let export_path = temp_dir.path().join("cluster-export.json.gz");
        
        // Export to file
        manager.export_to_file("test-cluster", &export_path, &options).await.unwrap();
        
        // Verify file exists
        assert!(export_path.exists());
        
        // Import from file
        manager.import_from_file(&export_path, false).await.unwrap();
    }
}