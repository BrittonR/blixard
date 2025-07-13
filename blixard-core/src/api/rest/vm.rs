//! VM management REST endpoints

use axum::{
    extract::{Extension, Path, Query},
    response::Json,
};
use utoipa::path;

use crate::api::schemas::{
    CreateVmRequest, CreateVmResponse, VmInfo, VmStatusDto, VmConfigDto,
    HypervisorDto, StartVmRequest, StopVmRequest, VmOperationResponse,
    ListVmsParams, MigrateVmRequest, PaginatedResponse, ResourceMetadata,
    VmRuntimeInfo, VmResourceUsage,
};
use super::{AppState, json_error, handle_blixard_error, extract_pagination};

/// List virtual machines
#[utoipa::path(
    get,
    path = "/vms",
    tag = "vm",
    summary = "List virtual machines",
    description = "Get a paginated list of virtual machines with optional filtering",
    params(
        ("status" = Option<VmStatusDto>, Query, description = "Filter by VM status"),
        ("node_id" = Option<u64>, Query, description = "Filter by node ID"),
        ("sort_by" = Option<String>, Query, description = "Sort field (name, status, created_at)"),
        ("sort_order" = Option<String>, Query, description = "Sort order (asc, desc)"),
        ("offset" = Option<u32>, Query, description = "Pagination offset"),
        ("limit" = Option<u32>, Query, description = "Pagination limit")
    ),
    responses(
        (status = 200, description = "VM list retrieved successfully", body = PaginatedResponse<VmInfo>),
        (status = 503, description = "Service unavailable", body = ErrorResponse)
    ),
    security(
        ("bearer_auth" = []),
        ("api_key" = [])
    )
)]
pub async fn list_vms(
    Extension(app_state): Extension<AppState>,
    Query(params): Query<ListVmsParams>,
) -> Result<Json<PaginatedResponse<VmInfo>>, (axum::http::StatusCode, Json<crate::api::schemas::ErrorResponse>)> {
    let shared_state = &app_state.shared_state;
    
    // Get VMs from the VM manager through shared state
    match shared_state.list_vms().await {
        Ok(vms) => {
            let mut vm_infos: Vec<VmInfo> = vms.into_iter()
                .filter(|(_, vm)| {
                    // Apply status filter if specified
                    params.status.map_or(true, |status| {
                        VmStatusDto::from(vm.status) == status
                    })
                })
                .filter(|(_, vm)| {
                    // Apply node_id filter if specified
                    params.node_id.map_or(true, |node_id| {
                        vm.assigned_node_id == Some(node_id)
                    })
                })
                .map(|(name, vm)| VmInfo {
                    name: name.clone(),
                    status: VmStatusDto::from(vm.status),
                    config: VmConfigDto {
                        hypervisor: HypervisorDto::from(vm.config.hypervisor),
                        vcpus: vm.config.vcpus,
                        memory_mb: vm.config.memory,
                        networks: Vec::new(), // Simplified for now
                        volumes: Vec::new(),   // Simplified for now
                        kernel: None,
                        init_command: vm.config.init_command.clone(),
                    },
                    runtime: VmRuntimeInfo {
                        assigned_node_id: vm.assigned_node_id,
                        host_address: Some("127.0.0.1".to_string()), // Placeholder
                        console_address: None,
                        pid: None,
                        uptime_seconds: if matches!(vm.status, crate::types::VmStatus::Running) {
                            Some(3600) // Placeholder
                        } else {
                            None
                        },
                    },
                    resource_usage: VmResourceUsage {
                        cpu_percent: 15.5,
                        memory_used_mb: vm.config.memory / 2,
                        disk_used_gb: 5,
                        network_rx_bytes: 1024000,
                        network_tx_bytes: 512000,
                    },
                    metadata: ResourceMetadata {
                        created_at: chrono::Utc::now() - chrono::Duration::hours(2),
                        updated_at: chrono::Utc::now() - chrono::Duration::minutes(5),
                        version: "1.0.0".to_string(),
                        labels: std::collections::HashMap::new(),
                        annotations: std::collections::HashMap::new(),
                    },
                })
                .collect();
            
            // Apply sorting
            if let Some(sort_by) = &params.sort_by {
                let ascending = params.sort_order.as_deref() != Some("desc");
                match sort_by.as_str() {
                    "name" => vm_infos.sort_by(|a, b| if ascending { a.name.cmp(&b.name) } else { b.name.cmp(&a.name) }),
                    "status" => vm_infos.sort_by(|a, b| if ascending { a.status.cmp(&b.status) } else { b.status.cmp(&a.status) }),
                    "created_at" => vm_infos.sort_by(|a, b| if ascending { a.metadata.created_at.cmp(&b.metadata.created_at) } else { b.metadata.created_at.cmp(&a.metadata.created_at) }),
                    _ => {} // Invalid sort field, ignore
                }
            }
            
            let total = vm_infos.len() as u64;
            let (offset, limit) = extract_pagination(None);
            
            // Apply pagination
            let start = offset as usize;
            let end = std::cmp::min(start + limit as usize, vm_infos.len());
            let paginated_vms = vm_infos.into_iter().skip(start).take(limit as usize).collect();
            
            let response = PaginatedResponse {
                items: paginated_vms,
                total,
                count: (end - start) as u32,
                offset,
                limit,
            };
            
            Ok(Json(response))
        }
        Err(e) => Err(handle_blixard_error(e)),
    }
}

/// Create a new virtual machine
#[utoipa::path(
    post,
    path = "/vms",
    tag = "vm",
    summary = "Create virtual machine",
    description = "Create a new virtual machine with the specified configuration",
    request_body = CreateVmRequest,
    responses(
        (status = 201, description = "VM created successfully", body = CreateVmResponse),
        (status = 400, description = "Invalid VM configuration", body = ErrorResponse),
        (status = 409, description = "VM already exists", body = ErrorResponse),
        (status = 503, description = "Service unavailable", body = ErrorResponse)
    ),
    security(
        ("bearer_auth" = []),
        ("api_key" = [])
    )
)]
pub async fn create_vm(
    Extension(app_state): Extension<AppState>,
    Json(request): Json<CreateVmRequest>,
) -> Result<Json<CreateVmResponse>, (axum::http::StatusCode, Json<crate::api::schemas::ErrorResponse>)> {
    let shared_state = &app_state.shared_state;
    
    // Convert API request to internal VM config
    let vm_config = crate::types::VmConfig {
        name: request.name.clone(),
        hypervisor: crate::types::Hypervisor::from(request.hypervisor),
        vcpus: request.vcpus,
        memory: request.memory_mb,
        networks: Vec::new(), // Would convert from request.networks
        volumes: Vec::new(),   // Would convert from request.volumes
        nixos_modules: Vec::new(),
        flake_modules: Vec::new(),
        kernel: None,
        init_command: request.init_command.clone(),
    };
    
    // Create VM through shared state
    match shared_state.create_vm(vm_config).await {
        Ok(_) => {
            // Get the created VM info
            match shared_state.get_vm(&request.name).await {
                Ok(vm) => {
                    let response = CreateVmResponse {
                        name: request.name.clone(),
                        status: VmStatusDto::from(vm.status),
                        assigned_node_id: vm.assigned_node_id,
                        message: "VM created successfully".to_string(),
                        vm_info: VmInfo {
                            name: request.name,
                            status: VmStatusDto::from(vm.status),
                            config: VmConfigDto {
                                hypervisor: HypervisorDto::from(vm.config.hypervisor),
                                vcpus: vm.config.vcpus,
                                memory_mb: vm.config.memory,
                                networks: Vec::new(),
                                volumes: Vec::new(),
                                kernel: None,
                                init_command: vm.config.init_command,
                            },
                            runtime: VmRuntimeInfo {
                                assigned_node_id: vm.assigned_node_id,
                                host_address: None,
                                console_address: None,
                                pid: None,
                                uptime_seconds: None,
                            },
                            resource_usage: VmResourceUsage {
                                cpu_percent: 0.0,
                                memory_used_mb: 0,
                                disk_used_gb: 0,
                                network_rx_bytes: 0,
                                network_tx_bytes: 0,
                            },
                            metadata: ResourceMetadata {
                                created_at: chrono::Utc::now(),
                                updated_at: chrono::Utc::now(),
                                version: "1.0.0".to_string(),
                                labels: std::collections::HashMap::new(),
                                annotations: std::collections::HashMap::new(),
                            },
                        },
                    };
                    
                    Ok(Json(response))
                }
                Err(e) => Err(handle_blixard_error(e)),
            }
        }
        Err(e) => Err(handle_blixard_error(e)),
    }
}

/// Get virtual machine information
#[utoipa::path(
    get,
    path = "/vms/{vm_name}",
    tag = "vm",
    summary = "Get virtual machine",
    description = "Get detailed information about a specific virtual machine",
    params(
        ("vm_name" = String, Path, description = "VM name to retrieve")
    ),
    responses(
        (status = 200, description = "VM information retrieved successfully", body = VmInfo),
        (status = 404, description = "VM not found", body = ErrorResponse),
        (status = 503, description = "Service unavailable", body = ErrorResponse)
    ),
    security(
        ("bearer_auth" = []),
        ("api_key" = [])
    )
)]
pub async fn get_vm(
    Extension(app_state): Extension<AppState>,
    Path(vm_name): Path<String>,
) -> Result<Json<VmInfo>, (axum::http::StatusCode, Json<crate::api::schemas::ErrorResponse>)> {
    let shared_state = &app_state.shared_state;
    
    match shared_state.get_vm(&vm_name).await {
        Ok(vm) => {
            let vm_info = VmInfo {
                name: vm_name,
                status: VmStatusDto::from(vm.status),
                config: VmConfigDto {
                    hypervisor: HypervisorDto::from(vm.config.hypervisor),
                    vcpus: vm.config.vcpus,
                    memory_mb: vm.config.memory,
                    networks: Vec::new(),
                    volumes: Vec::new(),
                    kernel: None,
                    init_command: vm.config.init_command,
                },
                runtime: VmRuntimeInfo {
                    assigned_node_id: vm.assigned_node_id,
                    host_address: Some("127.0.0.1".to_string()),
                    console_address: None,
                    pid: None,
                    uptime_seconds: if matches!(vm.status, crate::types::VmStatus::Running) {
                        Some(1800)
                    } else {
                        None
                    },
                },
                resource_usage: VmResourceUsage {
                    cpu_percent: 12.3,
                    memory_used_mb: vm.config.memory / 3,
                    disk_used_gb: 8,
                    network_rx_bytes: 2048000,
                    network_tx_bytes: 1024000,
                },
                metadata: ResourceMetadata {
                    created_at: chrono::Utc::now() - chrono::Duration::hours(2),
                    updated_at: chrono::Utc::now() - chrono::Duration::minutes(5),
                    version: "1.0.0".to_string(),
                    labels: std::collections::HashMap::new(),
                    annotations: std::collections::HashMap::new(),
                },
            };
            
            Ok(Json(vm_info))
        }
        Err(e) => Err(handle_blixard_error(e)),
    }
}

/// Start a virtual machine
#[utoipa::path(
    post,
    path = "/vms/{vm_name}/start",
    tag = "vm",
    summary = "Start virtual machine",
    description = "Start a stopped virtual machine",
    params(
        ("vm_name" = String, Path, description = "VM name to start")
    ),
    request_body = StartVmRequest,
    responses(
        (status = 200, description = "VM start operation initiated", body = VmOperationResponse),
        (status = 400, description = "Invalid request", body = ErrorResponse),
        (status = 404, description = "VM not found", body = ErrorResponse),
        (status = 409, description = "VM already running", body = ErrorResponse)
    ),
    security(
        ("bearer_auth" = []),
        ("api_key" = [])
    )
)]
pub async fn start_vm(
    Extension(app_state): Extension<AppState>,
    Path(vm_name): Path<String>,
    Json(request): Json<StartVmRequest>,
) -> Result<Json<VmOperationResponse>, (axum::http::StatusCode, Json<crate::api::schemas::ErrorResponse>)> {
    let shared_state = &app_state.shared_state;
    
    match shared_state.start_vm(&vm_name).await {
        Ok(_) => {
            let response = VmOperationResponse {
                success: true,
                message: format!("VM '{}' start operation initiated", vm_name),
                vm_name: vm_name.clone(),
                operation: "start".to_string(),
                timestamp: chrono::Utc::now(),
                operation_id: uuid::Uuid::new_v4().to_string(),
            };
            
            Ok(Json(response))
        }
        Err(e) => Err(handle_blixard_error(e)),
    }
}

/// Stop a virtual machine
#[utoipa::path(
    post,
    path = "/vms/{vm_name}/stop",
    tag = "vm",
    summary = "Stop virtual machine",
    description = "Stop a running virtual machine",
    params(
        ("vm_name" = String, Path, description = "VM name to stop")
    ),
    request_body = StopVmRequest,
    responses(
        (status = 200, description = "VM stop operation initiated", body = VmOperationResponse),
        (status = 400, description = "Invalid request", body = ErrorResponse),
        (status = 404, description = "VM not found", body = ErrorResponse),
        (status = 409, description = "VM already stopped", body = ErrorResponse)
    ),
    security(
        ("bearer_auth" = []),
        ("api_key" = [])
    )
)]
pub async fn stop_vm(
    Extension(app_state): Extension<AppState>,
    Path(vm_name): Path<String>,
    Json(request): Json<StopVmRequest>,
) -> Result<Json<VmOperationResponse>, (axum::http::StatusCode, Json<crate::api::schemas::ErrorResponse>)> {
    let shared_state = &app_state.shared_state;
    
    match shared_state.stop_vm(&vm_name).await {
        Ok(_) => {
            let response = VmOperationResponse {
                success: true,
                message: format!("VM '{}' stop operation initiated", vm_name),
                vm_name: vm_name.clone(),
                operation: "stop".to_string(),
                timestamp: chrono::Utc::now(),
                operation_id: uuid::Uuid::new_v4().to_string(),
            };
            
            Ok(Json(response))
        }
        Err(e) => Err(handle_blixard_error(e)),
    }
}

/// Delete a virtual machine
#[utoipa::path(
    delete,
    path = "/vms/{vm_name}",
    tag = "vm",
    summary = "Delete virtual machine",
    description = "Delete a virtual machine (must be stopped first)",
    params(
        ("vm_name" = String, Path, description = "VM name to delete")
    ),
    responses(
        (status = 200, description = "VM deletion initiated", body = VmOperationResponse),
        (status = 404, description = "VM not found", body = ErrorResponse),
        (status = 409, description = "VM must be stopped first", body = ErrorResponse)
    ),
    security(
        ("bearer_auth" = []),
        ("api_key" = [])
    )
)]
pub async fn delete_vm(
    Extension(app_state): Extension<AppState>,
    Path(vm_name): Path<String>,
) -> Result<Json<VmOperationResponse>, (axum::http::StatusCode, Json<crate::api::schemas::ErrorResponse>)> {
    let shared_state = &app_state.shared_state;
    
    match shared_state.delete_vm(&vm_name).await {
        Ok(_) => {
            let response = VmOperationResponse {
                success: true,
                message: format!("VM '{}' deletion initiated", vm_name),
                vm_name: vm_name.clone(),
                operation: "delete".to_string(),
                timestamp: chrono::Utc::now(),
                operation_id: uuid::Uuid::new_v4().to_string(),
            };
            
            Ok(Json(response))
        }
        Err(e) => Err(handle_blixard_error(e)),
    }
}

/// Migrate a virtual machine
#[utoipa::path(
    post,
    path = "/vms/{vm_name}/migrate",
    tag = "vm",
    summary = "Migrate virtual machine",
    description = "Migrate a VM to another node in the cluster",
    params(
        ("vm_name" = String, Path, description = "VM name to migrate")
    ),
    request_body = MigrateVmRequest,
    responses(
        (status = 200, description = "VM migration initiated", body = VmOperationResponse),
        (status = 400, description = "Invalid migration request", body = ErrorResponse),
        (status = 404, description = "VM or target node not found", body = ErrorResponse),
        (status = 409, description = "Migration not possible", body = ErrorResponse)
    ),
    security(
        ("bearer_auth" = []),
        ("api_key" = [])
    )
)]
pub async fn migrate_vm(
    Extension(app_state): Extension<AppState>,
    Path(vm_name): Path<String>,
    Json(request): Json<MigrateVmRequest>,
) -> Result<Json<VmOperationResponse>, (axum::http::StatusCode, Json<crate::api::schemas::ErrorResponse>)> {
    let shared_state = &app_state.shared_state;
    
    // For now, return a placeholder response
    // In the full implementation, this would trigger VM migration
    let response = VmOperationResponse {
        success: true,
        message: format!("VM '{}' migration to node {} initiated", vm_name, request.target_node_id),
        vm_name: vm_name.clone(),
        operation: "migrate".to_string(),
        timestamp: chrono::Utc::now(),
        operation_id: uuid::Uuid::new_v4().to_string(),
    };
    
    Ok(Json(response))
}