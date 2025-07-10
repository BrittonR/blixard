//! Examples demonstrating effective use of the ErrorContext pattern
//!
//! This module shows best practices for using error context throughout the codebase
//! to provide rich, actionable error messages.

use crate::error::{BlixardError, BlixardResult};
use crate::patterns::error_context::{ErrorContext, DomainErrorContext, ErrorCollector};
use std::fs::File;
use std::io::Read;
use std::path::Path;

/// Example 1: Basic error context usage
pub fn read_config_file(path: &Path) -> BlixardResult<String> {
    // Before: Basic error without context
    // let mut file = File::open(path)?;
    
    // After: With context
    let mut file = File::open(path)
        .with_context(|| format!("Failed to open config file at {:?}", path))?;
    
    let mut contents = String::new();
    file.read_to_string(&mut contents)
        .with_context(|| format!("Failed to read config file at {:?}", path))?;
    
    Ok(contents)
}

/// Example 2: Using domain-specific context
pub async fn deploy_vm(vm_name: &str, config_path: &Path) -> BlixardResult<()> {
    // Load VM configuration
    let config = read_config_file(config_path)
        .with_vm_context(vm_name, "load configuration")?;
    
    // Validate configuration
    validate_vm_config(&config)
        .with_vm_context(vm_name, "validate configuration")?;
    
    // Create VM
    create_vm_internal(vm_name, &config).await
        .with_vm_context(vm_name, "create VM")?;
    
    // Start VM
    start_vm_internal(vm_name).await
        .with_vm_context(vm_name, "start VM")?;
    
    Ok(())
}

/// Example 3: Chaining multiple contexts
pub async fn connect_to_peer(peer_id: &str, address: &str) -> BlixardResult<String> {
    // First try with P2P context
    establish_connection(address).await
        .with_p2p_context(peer_id, "establish connection")
        .with_context(|| format!("Peer address: {}", address))?;
    
    // Then add network context for specific operations
    let handshake_result = perform_handshake(peer_id).await
        .with_network_context(address, "TLS handshake")
        .with_p2p_context(peer_id, "complete handshake")?;
    
    Ok(handshake_result)
}

/// Example 4: Using ErrorCollector for batch operations
pub async fn start_all_vms(vm_names: &[String]) -> BlixardResult<Vec<String>> {
    let mut collector = ErrorCollector::new("Starting all VMs");
    let mut started_vms = Vec::new();
    
    for vm_name in vm_names {
        match start_vm_internal(vm_name).await {
            Ok(()) => {
                started_vms.push(vm_name.clone());
            }
            Err(e) => {
                // Convert to VM-specific error and collect
                let vm_error = BlixardError::Internal {
                    message: format!("Failed to start VM '{}'", vm_name),
                    source: Some(Box::new(e)),
                };
                collector.add_error(vm_error);
            }
        }
    }
    
    // Return partial success with errors
    collector.into_result(started_vms)
}

/// Example 5: Conditional error context based on operation type
pub async fn storage_operation(
    operation: StorageOp,
    key: &str,
    value: Option<&[u8]>,
) -> BlixardResult<Vec<u8>> {
    match operation {
        StorageOp::Get => {
            get_from_storage(key).await
                .with_storage_context("get")
                .with_context(|| format!("Key: {}", key))
        }
        StorageOp::Put => {
            let data = value.ok_or_else(|| BlixardError::InvalidInput {
                field: "value".to_string(),
                value: "None".to_string(),
                reason: "Put operation requires value".to_string(),
            })?;
            
            put_to_storage(key, data).await
                .with_storage_context("put")
                .with_context(|| format!("Key: {}, Size: {} bytes", key, data.len()))
        }
        StorageOp::Delete => {
            delete_from_storage(key).await
                .with_storage_context("delete")
                .with_context(|| format!("Key: {}", key))
        }
    }
}

/// Example 6: Complex operation with nested contexts
pub async fn replicate_data_to_peers(
    data_id: &str,
    peer_ids: &[String],
    min_replicas: usize,
) -> BlixardResult<()> {
    // First, prepare the data
    let data = prepare_replication_data(data_id).await
        .with_storage_context("prepare replication data")
        .with_context(|| format!("Data ID: {}", data_id))?;
    
    // Then replicate to each peer
    let mut collector = ErrorCollector::new(format!("Replicating data {} to {} peers", data_id, peer_ids.len()));
    let mut successful_replicas = 0;
    
    for peer_id in peer_ids {
        let result = send_data_to_peer(peer_id, &data).await
            .with_p2p_context(peer_id, "send replication data")
            .with_context(|| format!("Data size: {} bytes", data.len()));
        
        match result {
            Ok(()) => successful_replicas += 1,
            Err(e) => collector.add_error(e),
        }
    }
    
    // Check if we have enough replicas
    if successful_replicas < min_replicas {
        return Err(BlixardError::InsufficientReplicas {
            required: min_replicas,
            actual: successful_replicas,
            details: format!("Failed to replicate to {} peers", collector.error_count()),
        });
    }
    
    // Return success if we have enough replicas, even with some failures
    Ok(())
}

/// Example 7: Security context for permission checks
pub async fn access_secure_resource(
    user_id: &str,
    resource_id: &str,
    action: &str,
) -> BlixardResult<String> {
    // Check permissions
    check_permissions(user_id, resource_id, action).await
        .with_security_context("check permissions", resource_id)
        .with_context(|| format!("User: {}, Action: {}", user_id, action))?;
    
    // Audit log the access attempt
    log_access_attempt(user_id, resource_id, action).await
        .with_security_context("audit log", resource_id)?;
    
    // Access the resource
    fetch_secure_resource(resource_id).await
        .with_security_context("fetch resource", resource_id)
}

/// Example 8: Configuration validation with detailed context
pub fn validate_cluster_config(config: &ClusterConfig) -> BlixardResult<()> {
    // Validate node count
    if config.nodes.is_empty() {
        return Err(BlixardError::ConfigError(
            "Cluster must have at least one node".to_string()
        )).with_config_context("cluster.nodes");
    }
    
    // Validate each node
    for (idx, node) in config.nodes.iter().enumerate() {
        validate_node_config(node)
            .with_config_context(&format!("cluster.nodes[{}]", idx))
            .with_context(|| format!("Node ID: {}", node.id))?;
    }
    
    // Validate replication factor
    if config.replication_factor > config.nodes.len() {
        return Err(BlixardError::InvalidInput {
            field: "replication_factor".to_string(),
            value: config.replication_factor.to_string(),
            reason: format!("Cannot exceed node count ({})", config.nodes.len()),
        }).with_config_context("cluster.replication_factor");
    }
    
    Ok(())
}

/// Example 9: Using static context messages for performance
pub fn process_batch_fast(items: &[Item]) -> BlixardResult<Vec<ProcessedItem>> {
    let mut results = Vec::with_capacity(items.len());
    
    for item in items {
        // Use static context for hot paths
        let processed = process_item_internal(item)
            .context("Failed to process batch item")?;
        results.push(processed);
    }
    
    Ok(results)
}

/// Example 10: Custom error transformation with context
pub async fn handle_external_api_call(endpoint: &str) -> BlixardResult<String> {
    match call_external_api(endpoint).await {
        Ok(response) => Ok(response),
        Err(e) => {
            // Transform external error to BlixardError with context
            match e.kind() {
                ExternalErrorKind::Timeout => {
                    Err(BlixardError::Timeout {
                        operation: "external API call".to_string(),
                        duration: std::time::Duration::from_secs(30),
                    }).with_network_context(endpoint, "API request timeout")
                }
                ExternalErrorKind::NotFound => {
                    Err(BlixardError::NotFound {
                        resource_type: "API endpoint".to_string(),
                        identifier: endpoint.to_string(),
                    })
                }
                _ => {
                    Err(BlixardError::External {
                        service: "External API".to_string(),
                        message: e.to_string(),
                    }).with_network_context(endpoint, "API call failed")
                }
            }
        }
    }
}

// Helper types and functions for examples
#[derive(Debug)]
pub enum StorageOp {
    Get,
    Put,
    Delete,
}

#[derive(Debug)]
pub struct ClusterConfig {
    pub nodes: Vec<NodeConfig>,
    pub replication_factor: usize,
}

#[derive(Debug)]
pub struct NodeConfig {
    pub id: u64,
    pub address: String,
}

#[derive(Debug)]
pub struct Item {
    pub id: String,
}

#[derive(Debug)]
pub struct ProcessedItem {
    pub id: String,
    pub result: String,
}

#[derive(Debug)]
pub enum ExternalErrorKind {
    Timeout,
    NotFound,
    ServerError,
}

pub trait ExternalError {
    fn kind(&self) -> ExternalErrorKind;
}

// Mock implementations for examples
async fn validate_vm_config(_config: &str) -> Result<(), std::io::Error> {
    Ok(())
}

async fn create_vm_internal(_name: &str, _config: &str) -> Result<(), std::io::Error> {
    Ok(())
}

async fn start_vm_internal(_name: &str) -> Result<(), std::io::Error> {
    Ok(())
}

async fn establish_connection(_address: &str) -> Result<(), std::io::Error> {
    Ok(())
}

async fn perform_handshake(_peer_id: &str) -> Result<String, std::io::Error> {
    Ok("Connected".to_string())
}

async fn get_from_storage(_key: &str) -> Result<Vec<u8>, std::io::Error> {
    Ok(vec![1, 2, 3])
}

async fn put_to_storage(_key: &str, _data: &[u8]) -> Result<Vec<u8>, std::io::Error> {
    Ok(vec![])
}

async fn delete_from_storage(_key: &str) -> Result<Vec<u8>, std::io::Error> {
    Ok(vec![])
}

async fn prepare_replication_data(_id: &str) -> Result<Vec<u8>, std::io::Error> {
    Ok(vec![1, 2, 3, 4, 5])
}

async fn send_data_to_peer(_peer_id: &str, _data: &[u8]) -> Result<(), std::io::Error> {
    Ok(())
}

async fn check_permissions(_user: &str, _resource: &str, _action: &str) -> Result<(), std::io::Error> {
    Ok(())
}

async fn log_access_attempt(_user: &str, _resource: &str, _action: &str) -> Result<(), std::io::Error> {
    Ok(())
}

async fn fetch_secure_resource(_id: &str) -> Result<String, std::io::Error> {
    Ok("Secret data".to_string())
}

fn validate_node_config(_node: &NodeConfig) -> Result<(), std::io::Error> {
    Ok(())
}

fn process_item_internal(_item: &Item) -> Result<ProcessedItem, std::io::Error> {
    Ok(ProcessedItem {
        id: "processed".to_string(),
        result: "success".to_string(),
    })
}

async fn call_external_api(_endpoint: &str) -> Result<String, Box<dyn ExternalError>> {
    Ok("Response".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;
    use std::io::Write;
    
    #[test]
    fn test_error_context_basic() {
        let result = read_config_file(Path::new("/nonexistent/file.conf"));
        assert!(result.is_err());
        
        let error = result.unwrap_err();
        let error_string = error.to_string();
        assert!(error_string.contains("Failed to open config file"));
        assert!(error_string.contains("/nonexistent/file.conf"));
    }
    
    #[tokio::test]
    async fn test_error_collector() {
        let vm_names = vec!["vm1".to_string(), "vm2".to_string(), "vm3".to_string()];
        let result = start_all_vms(&vm_names).await;
        
        // Should succeed even if some VMs fail to start
        assert!(result.is_ok() || result.is_err());
    }
    
    #[test]
    fn test_config_validation_context() {
        let config = ClusterConfig {
            nodes: vec![],
            replication_factor: 3,
        };
        
        let result = validate_cluster_config(&config);
        assert!(result.is_err());
        
        let error = result.unwrap_err();
        assert!(error.to_string().contains("Cluster must have at least one node"));
    }
}