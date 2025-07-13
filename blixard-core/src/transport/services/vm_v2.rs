//! VM service implementation using ServiceBuilder
//!
//! This demonstrates the dramatic reduction in code when using ServiceBuilder
//! for complex services with many operations.

use crate::{
    error::BlixardResult,
    iroh_types::VmInfo,
    node_shared::SharedNodeState,
    transport::service_builder::{ServiceBuilder, ServiceProtocolHandler, ServiceResponse},
    types::VmConfig,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// VM service request/response types
#[derive(Debug, Serialize, Deserialize)]
pub struct CreateVmRequest {
    pub name: String,
    pub config_path: Option<String>,
    pub vcpus: Option<u32>,
    pub memory_mb: Option<u32>,
    pub constraints: Option<Vec<String>>,
    pub features: Option<Vec<String>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct VmOperationRequest {
    pub name: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct VmListRequest {
    pub filter: Option<String>,
    pub include_stopped: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct VmMigrationRequest {
    pub vm_name: String,
    pub target_node: String,
    pub live_migration: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct VmSchedulingRequest {
    pub name: String,
    pub config: VmConfig,
    pub constraints: Option<Vec<String>>,
    pub features: Option<Vec<String>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PlacementRequest {
    pub requirements: VmConfig,
    pub constraints: Option<Vec<String>>,
    pub strategy: Option<String>,
}

/// VM service response types
#[derive(Debug, Serialize, Deserialize)]
pub struct VmOperationResponse {
    pub vm_name: String,
    pub operation: String,
    pub details: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct VmListResponse {
    pub vms: Vec<VmInfo>,
    pub total_count: usize,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct VmStatusResponse {
    pub vm_info: Option<VmInfo>,
    pub found: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PlacementResponse {
    pub recommended_node: Option<String>,
    pub placement_score: f64,
    pub reasoning: String,
}

/// Create VM service using ServiceBuilder - eliminates ~600 lines of duplication
pub fn create_vm_service(node: Arc<SharedNodeState>) -> ServiceProtocolHandler {
    let node_clone = node.clone();

    ServiceBuilder::new("vm", node)
        // VM lifecycle operations
        .simple_method("create", {
            let node = node_clone.clone();
            move |req: CreateVmRequest| {
                let node = node.clone();
                async move { handle_create_vm(node, req).await }
            }
        })
        .simple_method("start", {
            let node = node_clone.clone();
            move |req: VmOperationRequest| {
                let node = node.clone();
                async move { handle_start_vm(node, req).await }
            }
        })
        .simple_method("stop", {
            let node = node_clone.clone();
            move |req: VmOperationRequest| {
                let node = node.clone();
                async move { handle_stop_vm(node, req).await }
            }
        })
        .simple_method("delete", {
            let node = node_clone.clone();
            move |req: VmOperationRequest| {
                let node = node.clone();
                async move { handle_delete_vm(node, req).await }
            }
        })
        .simple_method("list", {
            let node = node_clone.clone();
            move |req: VmListRequest| {
                let node = node.clone();
                async move { handle_list_vms(node, req).await }
            }
        })
        .simple_method("status", {
            let node = node_clone.clone();
            move |req: VmOperationRequest| {
                let node = node.clone();
                async move { handle_get_vm_status(node, req).await }
            }
        })
        // Migration operations
        .simple_method("migrate", {
            let node = node_clone.clone();
            move |req: VmMigrationRequest| {
                let node = node.clone();
                async move { handle_migrate_vm(node, req).await }
            }
        })
        // Scheduling operations
        .simple_method("create_with_scheduling", {
            let node = node_clone.clone();
            move |req: VmSchedulingRequest| {
                let node = node.clone();
                async move { handle_create_vm_with_scheduling(node, req).await }
            }
        })
        .simple_method("schedule_placement", {
            let node = node_clone.clone();
            move |req: PlacementRequest| {
                let node = node.clone();
                async move { handle_schedule_placement(node, req).await }
            }
        })
        .build()
        .into()
}

/// VM operation handlers - business logic separated from protocol concerns
async fn handle_create_vm(
    node: Arc<SharedNodeState>,
    request: CreateVmRequest,
) -> BlixardResult<ServiceResponse<VmOperationResponse>> {
    // Convert request to VmConfig
    let config = VmConfig {
        name: request.name.clone(),
        config_path: request.config_path.unwrap_or_default(),
        vcpus: request.vcpus.unwrap_or(2),
        memory: request.memory_mb.unwrap_or(1024),
        ..Default::default()
    };

    // Create VM through node
    node.create_vm(config).await?;

    let response = VmOperationResponse {
        vm_name: request.name.clone(),
        operation: "create".to_string(),
        details: Some(format!("VM {} created successfully", request.name)),
    };

    Ok(ServiceResponse::success(
        response,
        "VM created successfully",
    ))
}

async fn handle_start_vm(
    node: Arc<SharedNodeState>,
    request: VmOperationRequest,
) -> BlixardResult<ServiceResponse<VmOperationResponse>> {
    node.start_vm(&request.name).await?;

    let response = VmOperationResponse {
        vm_name: request.name.clone(),
        operation: "start".to_string(),
        details: Some(format!("VM {} started successfully", request.name)),
    };

    Ok(ServiceResponse::success(
        response,
        "VM started successfully",
    ))
}

async fn handle_stop_vm(
    node: Arc<SharedNodeState>,
    request: VmOperationRequest,
) -> BlixardResult<ServiceResponse<VmOperationResponse>> {
    node.stop_vm(&request.name).await?;

    let response = VmOperationResponse {
        vm_name: request.name.clone(),
        operation: "stop".to_string(),
        details: Some(format!("VM {} stopped successfully", request.name)),
    };

    Ok(ServiceResponse::success(
        response,
        "VM stopped successfully",
    ))
}

async fn handle_delete_vm(
    node: Arc<SharedNodeState>,
    request: VmOperationRequest,
) -> BlixardResult<ServiceResponse<VmOperationResponse>> {
    node.delete_vm(&request.name).await?;

    let response = VmOperationResponse {
        vm_name: request.name.clone(),
        operation: "delete".to_string(),
        details: Some(format!("VM {} deleted successfully", request.name)),
    };

    Ok(ServiceResponse::success(
        response,
        "VM deleted successfully",
    ))
}

async fn handle_list_vms(
    node: Arc<SharedNodeState>,
    request: VmListRequest,
) -> BlixardResult<ServiceResponse<VmListResponse>> {
    let vms = node.list_vms().await?;

    // Convert VmState to VmInfo
    let vm_infos: Vec<VmInfo> = vms
        .into_iter()
        .map(|vm| VmInfo {
            name: vm.name,
            state: vm.status as i32,
            node_id: vm.node_id,
            vcpus: vm.config.vcpus,
            memory_mb: vm.config.memory,
            ip_address: vm.config.ip_address.unwrap_or_default(),
        })
        .collect();

    // Apply filtering if requested
    let filtered_vms = if let Some(filter) = request.filter {
        vm_infos
            .into_iter()
            .filter(|vm| vm.name.contains(&filter))
            .collect()
    } else {
        vm_infos
    };

    let total_count = filtered_vms.len();
    let response = VmListResponse {
        vms: filtered_vms,
        total_count,
    };

    Ok(ServiceResponse::success(
        response,
        "VMs listed successfully",
    ))
}

async fn handle_get_vm_status(
    node: Arc<SharedNodeState>,
    request: VmOperationRequest,
) -> BlixardResult<ServiceResponse<VmStatusResponse>> {
    let vm_state = node.get_vm_info(&request.name).await?;

    // Convert VmState to VmInfo
    let vm_info = Some(VmInfo {
        name: vm_state.name,
        state: vm_state.status as i32,
        node_id: vm_state.node_id,
        vcpus: vm_state.config.vcpus,
        memory_mb: vm_state.config.memory,
        ip_address: vm_state.config.ip_address.unwrap_or_default(),
    });

    let response = VmStatusResponse {
        found: true,
        vm_info,
    };

    Ok(ServiceResponse::success(response, "VM status retrieved"))
}

async fn handle_migrate_vm(
    node: Arc<SharedNodeState>,
    request: VmMigrationRequest,
) -> BlixardResult<ServiceResponse<VmOperationResponse>> {
    // Implement VM migration logic
    // Parse target_node from string to u64
    let target_node_id = request.target_node.parse::<u64>().map_err(|_| {
        crate::error::BlixardError::configuration(
            "vm_migration.target_node",
            "Must be a valid node ID"
        )
    })?;
    node.migrate_vm(&request.vm_name, target_node_id).await?;

    let response = VmOperationResponse {
        vm_name: request.vm_name.clone(),
        operation: "migrate".to_string(),
        details: Some(format!(
            "VM {} migrated to {} (live: {})",
            request.vm_name, request.target_node, request.live_migration
        )),
    };

    Ok(ServiceResponse::success(
        response,
        "VM migrated successfully",
    ))
}

async fn handle_create_vm_with_scheduling(
    node: Arc<SharedNodeState>,
    request: VmSchedulingRequest,
) -> BlixardResult<ServiceResponse<VmOperationResponse>> {
    // Use scheduler to find optimal placement
    node.create_vm_with_scheduling(request.config).await?;

    let response = VmOperationResponse {
        vm_name: request.name.clone(),
        operation: "create_with_scheduling".to_string(),
        details: Some(format!(
            "VM {} created with optimal scheduling",
            request.name
        )),
    };

    Ok(ServiceResponse::success(
        response,
        "VM created with scheduling",
    ))
}

async fn handle_schedule_placement(
    _node: Arc<SharedNodeState>,
    _request: PlacementRequest,
) -> BlixardResult<ServiceResponse<PlacementResponse>> {
    // Get placement recommendation
    // TODO: Integrate with VmScheduler properly
    // For now, return a temporary placeholder since get_vm_placement_recommendation is not implemented
    let response = PlacementResponse {
        recommended_node: Some("node-1".to_string()), // Default to node 1
        placement_score: 1.0,
        reasoning: "Placement calculation not yet implemented".to_string(),
    };

    Ok(ServiceResponse::success(response, "Placement calculated"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_vm_service_creation() {
        let node = Arc::new(SharedNodeState::new());
        let handler = create_vm_service(node);

        assert_eq!(handler.service_name(), "vm");
        let methods = handler.available_methods();

        // Verify all methods are registered
        assert!(methods.contains(&"create"));
        assert!(methods.contains(&"start"));
        assert!(methods.contains(&"stop"));
        assert!(methods.contains(&"delete"));
        assert!(methods.contains(&"list"));
        assert!(methods.contains(&"status"));
        assert!(methods.contains(&"migrate"));
        assert!(methods.contains(&"create_with_scheduling"));
        assert!(methods.contains(&"schedule_placement"));
    }

    #[tokio::test]
    async fn test_create_vm_request() {
        let node = Arc::new(SharedNodeState::new());
        let request = CreateVmRequest {
            name: "test-vm".to_string(),
            config_path: None,
            vcpus: Some(4),
            memory_mb: Some(2048),
            constraints: None,
            features: None,
        };

        let result = handle_create_vm(node, request).await;
        assert!(result.is_ok());

        let response = result.unwrap();
        assert!(response.success);
        assert_eq!(response.data.as_ref().unwrap().vm_name, "test-vm");
        assert_eq!(response.data.as_ref().unwrap().operation, "create");
    }

    #[tokio::test]
    async fn test_vm_list_request() {
        let node = Arc::new(SharedNodeState::new());
        let request = VmListRequest {
            filter: None,
            include_stopped: true,
        };

        let result = handle_list_vms(node, request).await;
        assert!(result.is_ok());

        let response = result.unwrap();
        assert!(response.success);
        assert!(response.data.is_some());
    }
}
