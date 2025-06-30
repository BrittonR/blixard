//! VM-related gRPC service implementation
//!
//! This module handles all VM lifecycle operations including creation,
//! deletion, status queries, and migration.

use crate::{
    node_shared::SharedNodeState,
    types::{VmCommand, VmMigrationTask},
    raft_manager::ResourceRequirements,
    resource_quotas::ApiOperation,
    common::conversions::{error_to_status, vm_status_to_proto},
    grpc_server::common::GrpcMiddleware,
    proto::{
        CreateVmRequest, CreateVmResponse, DeleteVmRequest, DeleteVmResponse,
        GetVmStatusRequest, GetVmStatusResponse, ListVmsRequest, ListVmsResponse,
        StartVmRequest, StartVmResponse, StopVmRequest, StopVmResponse,
        VmInfo, MigrateVmRequest, MigrateVmResponse,
        CreateVmWithSchedulingRequest, CreateVmWithSchedulingResponse,
        ScheduleVmPlacementRequest, ScheduleVmPlacementResponse,
    },
    security::Permission,
    instrument_grpc, record_grpc_error,
};
use std::sync::Arc;
use tonic::{Request, Response, Status};

/// VM service implementation
#[derive(Clone)]
pub struct VmServiceImpl {
    node: Arc<SharedNodeState>,
    middleware: GrpcMiddleware,
}

impl VmServiceImpl {
    /// Create a new VM service instance
    pub fn new(
        node: Arc<SharedNodeState>,
        security_middleware: Option<crate::grpc_security::GrpcSecurityMiddleware>,
    ) -> Self {
        // Note: Security manager will be None here, Cedar auth will use fallback
        let middleware = GrpcMiddleware::new(security_middleware, None, None);
        
        Self {
            node,
            middleware,
        }
    }
    
    /// Create a new VM service instance with quota manager
    pub async fn with_quota_manager(
        node: Arc<SharedNodeState>,
        security_middleware: Option<crate::grpc_security::GrpcSecurityMiddleware>,
    ) -> Self {
        let quota_manager = node.get_quota_manager().await;
        let security_manager = node.get_security_manager().await;
        let middleware = GrpcMiddleware::new(security_middleware, security_manager, quota_manager);
        
        Self {
            node,
            middleware,
        }
    }
    
    /// Create VM configuration from request
    fn create_vm_config(&self, name: String, vcpus: u32, memory: u32) -> crate::types::VmConfig {
        crate::types::VmConfig {
            config_path: format!("/etc/blixard/vms/{}.yaml", name),
            name,
            vcpus,
            memory,
            ip_address: None,
            tenant_id: "default".to_string(), // Will be overridden by tenant from request
            metadata: None,
            anti_affinity: None,
            ..Default::default()
        }
    }
}

// VM operation helper methods for use by ClusterServiceImpl
impl VmServiceImpl {
    pub async fn create_vm(
        &self,
        request: Request<CreateVmRequest>,
    ) -> Result<Response<CreateVmResponse>, Status> {
        // Authenticate and check rate limits
        let (_ctx, tenant_id) = self.middleware
            .authenticate_and_rate_limit(&request, Permission::VmWrite, ApiOperation::VmCreate)
            .await?;
        
        let req = instrument_grpc!(self.node, request, "create_vm");
        
        tracing::info!("Creating VM '{}' with {} vCPUs and {} MB memory", 
            req.name, req.vcpus, req.memory_mb);
        
        // Create VM configuration
        let mut vm_config = self.create_vm_config(req.name.clone(), req.vcpus, req.memory_mb);
        vm_config.tenant_id = tenant_id.clone();
        
        // Check resource quotas
        self.middleware.check_vm_quota(&tenant_id, &vm_config, None).await?;
        
        // Create the VM through Raft consensus
        match self.node.send_vm_command(VmCommand::Create { config: vm_config.clone(), node_id: self.node.get_id() }).await {
            Ok(_) => {
                // Update resource usage
                self.middleware.update_resource_usage(
                    &tenant_id,
                    req.vcpus as i32,
                    req.memory_mb as i64,
                    5, // Default disk size
                    self.node.get_id(),
                ).await;
                
                Ok(Response::new(CreateVmResponse {
                    success: true,
                    message: format!("VM '{}' created successfully", req.name),
                    vm_id: req.name,
                }))
            }
            Err(e) => {
                record_grpc_error!("create_vm", e);
                Err(error_to_status(e))
            }
        }
    }
    
    pub async fn start_vm(
        &self,
        request: Request<StartVmRequest>,
    ) -> Result<Response<StartVmResponse>, Status> {
        // Authenticate
        let _ctx = self.middleware
            .authenticate(&request, Permission::VmWrite)
            .await?;
        
        let req = instrument_grpc!(self.node, request, "start_vm");
        
        tracing::info!("Starting VM '{}'", req.name);
        
        match self.node.send_vm_command(VmCommand::Start { name: req.name.clone() }).await {
            Ok(_) => {
                Ok(Response::new(StartVmResponse {
                    success: true,
                    message: format!("VM '{}' start command issued", req.name),
                }))
            }
            Err(e) => {
                record_grpc_error!("start_vm", e);
                Err(error_to_status(e))
            }
        }
    }
    
    pub async fn stop_vm(
        &self,
        request: Request<StopVmRequest>,
    ) -> Result<Response<StopVmResponse>, Status> {
        // Authenticate
        let _ctx = self.middleware
            .authenticate(&request, Permission::VmWrite)
            .await?;
        
        let req = instrument_grpc!(self.node, request, "stop_vm");
        
        tracing::info!("Stopping VM '{}'", req.name);
        
        match self.node.send_vm_command(VmCommand::Stop { name: req.name.clone() }).await {
            Ok(_) => {
                Ok(Response::new(StopVmResponse {
                    success: true,
                    message: format!("VM '{}' stop command issued", req.name),
                }))
            }
            Err(e) => {
                record_grpc_error!("stop_vm", e);
                Err(error_to_status(e))
            }
        }
    }
    
    pub async fn delete_vm(
        &self,
        request: Request<DeleteVmRequest>,
    ) -> Result<Response<DeleteVmResponse>, Status> {
        // Authenticate and check rate limits
        let (_ctx, tenant_id) = self.middleware
            .authenticate_and_rate_limit(&request, Permission::VmWrite, ApiOperation::VmDelete)
            .await?;
        
        let req = instrument_grpc!(self.node, request, "delete_vm");
        
        tracing::info!("Deleting VM '{}'", req.name);
        
        // Get VM info before deletion for quota updates
        if let Ok(vms) = self.node.list_vms().await {
            if let Some((vm_config, _)) = vms.iter().find(|(cfg, _)| cfg.name == req.name) {
                // Delete the VM through Raft consensus
                match self.node.send_vm_command(VmCommand::Delete { name: req.name.clone() }).await {
                    Ok(_) => {
                        // Update resource usage (negative values to release resources)
                        self.middleware.update_resource_usage(
                            &tenant_id,
                            -(vm_config.vcpus as i32),
                            -(vm_config.memory as i64),
                            -5, // Default disk size
                            self.node.get_id(),
                        ).await;
                        
                        Ok(Response::new(DeleteVmResponse {
                            success: true,
                            message: format!("VM '{}' deleted successfully", req.name),
                        }))
                    }
                    Err(e) => {
                        record_grpc_error!("delete_vm", e);
                        Err(error_to_status(e))
                    }
                }
            } else {
                Ok(Response::new(DeleteVmResponse {
                    success: false,
                    message: format!("VM '{}' not found", req.name),
                }))
            }
        } else {
            Err(Status::internal("Failed to list VMs"))
        }
    }
    
    pub async fn list_vms(
        &self,
        request: Request<ListVmsRequest>,
    ) -> Result<Response<ListVmsResponse>, Status> {
        // Authenticate
        let _ctx = self.middleware
            .authenticate(&request, Permission::VmRead)
            .await?;
        
        let _req = instrument_grpc!(self.node, request, "list_vms");
        
        match self.node.list_vms().await {
            Ok(vms) => {
                let vm_infos = vms.into_iter().map(|(config, status)| {
                    VmInfo {
                        name: config.name,
                        state: vm_status_to_proto(&status) as i32,
                        vcpus: config.vcpus,
                        memory_mb: config.memory,
                        node_id: self.node.get_id(),
                        ip_address: config.ip_address.unwrap_or_default(),
                    }
                }).collect();
                
                Ok(Response::new(ListVmsResponse {
                    vms: vm_infos,
                }))
            }
            Err(e) => {
                record_grpc_error!("list_vms", e);
                Err(error_to_status(e))
            }
        }
    }
    
    pub async fn get_vm_status(
        &self,
        request: Request<GetVmStatusRequest>,
    ) -> Result<Response<GetVmStatusResponse>, Status> {
        // Authenticate
        let _ctx = self.middleware
            .authenticate(&request, Permission::VmRead)
            .await?;
        
        let req = instrument_grpc!(self.node, request, "get_vm_status");
        
        match self.node.get_vm_status(&req.name).await {
            Ok(Some((config, status))) => {
                Ok(Response::new(GetVmStatusResponse {
                    found: true,
                    vm_info: Some(VmInfo {
                        name: config.name,
                        state: vm_status_to_proto(&status) as i32,
                        vcpus: config.vcpus,
                        memory_mb: config.memory,
                        node_id: self.node.get_id(),
                        ip_address: config.ip_address.unwrap_or_default(),
                    }),
                }))
            }
            Ok(None) => {
                Ok(Response::new(GetVmStatusResponse {
                    found: false,
                    vm_info: None,
                }))
            }
            Err(e) => {
                record_grpc_error!("get_vm_status", e);
                Err(error_to_status(e))
            }
        }
    }
    
    pub async fn migrate_vm(
        &self,
        request: Request<MigrateVmRequest>,
    ) -> Result<Response<MigrateVmResponse>, Status> {
        // Authenticate
        let _ctx = self.middleware
            .authenticate(&request, Permission::VmWrite)
            .await?;
        
        let req = instrument_grpc!(self.node, request, "migrate_vm");
        
        tracing::info!("Migrating VM '{}' to node {}", 
            req.vm_name, req.target_node_id);
        
        // Verify we're the leader
        if !self.node.is_leader().await {
            return Err(Status::failed_precondition("Not the leader"));
        }
        
        // Create migration task as a VM command
        let migration_task = VmMigrationTask {
            vm_name: req.vm_name.clone(),
            source_node_id: self.node.get_id(),
            target_node_id: req.target_node_id,
            live_migration: req.live_migration,
            force: req.force,
        };
        
        let vm_command = VmCommand::Migrate { task: migration_task };
        
        match self.node.send_vm_command(vm_command).await {
            Ok(()) => {
                Ok(Response::new(MigrateVmResponse {
                    success: true,
                    message: format!("Migration of VM '{}' started", req.vm_name),
                    source_node_id: self.node.get_id(),
                    target_node_id: req.target_node_id,
                    status: 1, // MIGRATION_STATUS_PREPARING
                    duration_ms: 0, // Not available yet
                }))
            }
            Err(e) => {
                record_grpc_error!("migrate_vm", e);
                Err(error_to_status(e))
            }
        }
    }
    
    pub async fn create_vm_with_scheduling(
        &self,
        request: Request<CreateVmWithSchedulingRequest>,
    ) -> Result<Response<CreateVmWithSchedulingResponse>, Status> {
        // Authenticate and check rate limits
        let (_ctx, tenant_id) = self.middleware
            .authenticate_and_rate_limit(&request, Permission::VmWrite, ApiOperation::VmCreate)
            .await?;
        
        let req = instrument_grpc!(self.node, request, "create_vm_with_scheduling");
        
        tracing::info!("Creating VM '{}' with scheduling", req.name);
        
        // Create VM configuration
        let mut vm_config = self.create_vm_config(req.name.clone(), req.vcpus, req.memory_mb);
        vm_config.tenant_id = tenant_id.clone();
        
        // Create resource requirements for scheduling
        let resource_reqs = ResourceRequirements {
            cpu_cores: req.vcpus,
            memory_mb: req.memory_mb as u64,
            disk_gb: 5, // Default
            required_features: vec![],
        };
        
        // TODO: Implement VM scheduling
        Err(Status::unimplemented("VM scheduling not implemented"))
    }
    
    pub async fn schedule_vm_placement(
        &self,
        request: Request<ScheduleVmPlacementRequest>,
    ) -> Result<Response<ScheduleVmPlacementResponse>, Status> {
        // Authenticate
        let _ctx = self.middleware
            .authenticate(&request, Permission::VmRead)
            .await?;
        
        let req = instrument_grpc!(self.node, request, "schedule_vm_placement");
        
        // Create resource requirements
        let resource_reqs = ResourceRequirements {
            cpu_cores: req.vcpus,
            memory_mb: req.memory_mb as u64,
            disk_gb: 5, // Default
            required_features: vec![],
        };
        
        // TODO: Implement VM placement scheduling
        Err(Status::unimplemented("VM placement scheduling not implemented"))
    }
}