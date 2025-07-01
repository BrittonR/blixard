//! VM service implementation for dual transport
//!
//! This service handles VM lifecycle operations over both gRPC and Iroh transports.

use crate::{
    error::{BlixardError, BlixardResult},
    node_shared::SharedNodeState,
    types::{VmCommand, VmConfig, VmStatus as InternalVmStatus},
    iroh_types::{
        CreateVmRequest, CreateVmResponse, DeleteVmRequest, DeleteVmResponse,
        GetVmStatusRequest, GetVmStatusResponse, ListVmsRequest, ListVmsResponse,
        StartVmRequest, StartVmResponse, StopVmRequest, StopVmResponse,
        VmInfo, VmState, MigrateVmRequest, MigrateVmResponse,
        CreateVmWithSchedulingRequest, CreateVmWithSchedulingResponse,
        ScheduleVmPlacementRequest, ScheduleVmPlacementResponse,
    },
    metrics_otel::{metrics, Timer, attributes},
};
use async_trait::async_trait;
use std::sync::Arc;
// Removed tonic imports - using Iroh-only transport
use serde::{Serialize, Deserialize};

/// VM operation request types for Iroh transport
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VmOperationRequest {
    Create {
        name: String,
        config_path: String,
        vcpus: u32,
        memory_mb: u32,
    },
    Start {
        name: String,
    },
    Stop {
        name: String,
    },
    Delete {
        name: String,
    },
    List,
    GetStatus {
        name: String,
    },
    Migrate {
        vm_name: String,
        target_node_id: u64,
        live_migration: bool,
        force: bool,
    },
}

/// VM operation response for Iroh transport
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VmOperationResponse {
    Create {
        success: bool,
        message: String,
        vm_id: String,
    },
    Start {
        success: bool,
        message: String,
    },
    Stop {
        success: bool,
        message: String,
    },
    Delete {
        success: bool,
        message: String,
    },
    List {
        vms: Vec<VmInfoData>,
    },
    GetStatus {
        found: bool,
        vm_info: Option<VmInfoData>,
    },
    Migrate {
        success: bool,
        message: String,
        source_node_id: u64,
        target_node_id: u64,
        status: i32,
        duration_ms: i64,
    },
}

/// Serializable VM info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmInfoData {
    pub name: String,
    pub state: String,
    pub vcpus: u32,
    pub memory_mb: u32,
    pub node_id: u64,
    pub ip_address: String,
}

/// Trait for VM operations
#[async_trait]
pub trait VmService: Send + Sync {
    /// Create a new VM
    async fn create_vm(&self, name: String, vcpus: u32, memory_mb: u32) -> BlixardResult<String>;
    
    /// Start a VM
    async fn start_vm(&self, name: &str) -> BlixardResult<()>;
    
    /// Stop a VM
    async fn stop_vm(&self, name: &str) -> BlixardResult<()>;
    
    /// Delete a VM
    async fn delete_vm(&self, name: &str) -> BlixardResult<()>;
    
    /// List all VMs
    async fn list_vms(&self) -> BlixardResult<Vec<(VmConfig, InternalVmStatus)>>;
    
    /// Get VM status
    async fn get_vm_status(&self, name: &str) -> BlixardResult<Option<(VmConfig, InternalVmStatus)>>;
    
    /// Migrate a VM to another node
    async fn migrate_vm(
        &self,
        vm_name: &str,
        target_node_id: u64,
        live_migration: bool,
        force: bool,
    ) -> BlixardResult<()>;
}

/// VM service implementation
#[derive(Clone)]
pub struct VmServiceImpl {
    node: Arc<SharedNodeState>,
}

impl VmServiceImpl {
    /// Create a new VM service instance
    pub fn new(node: Arc<SharedNodeState>) -> Self {
        Self { node }
    }
    
    /// Convert internal VM status to proto
    fn vm_status_to_proto(status: &InternalVmStatus) -> VmState {
        match status {
            InternalVmStatus::Creating => VmState::VmStateCreated,
            InternalVmStatus::Starting => VmState::VmStateStarting,
            InternalVmStatus::Running => VmState::VmStateRunning,
            InternalVmStatus::Stopping => VmState::VmStateStopping,
            InternalVmStatus::Stopped => VmState::VmStateStopped,
            InternalVmStatus::Failed => VmState::VmStateFailed,
        }
    }
}

#[async_trait]
impl VmService for VmServiceImpl {
    async fn create_vm(&self, name: String, vcpus: u32, memory_mb: u32) -> BlixardResult<String> {
        let vm_config = VmConfig {
            name: name.clone(),
            config_path: format!("/etc/blixard/vms/{}.yaml", name),
            vcpus,
            memory: memory_mb,
            tenant_id: "default".to_string(),
            ip_address: None,
            metadata: None,
            anti_affinity: None,
            ..Default::default()
        };
        
        // Send command through Raft consensus
        self.node.send_vm_command(VmCommand::Create {
            config: vm_config,
            node_id: self.node.get_id(),
        }).await?;
        
        Ok(name)
    }
    
    async fn start_vm(&self, name: &str) -> BlixardResult<()> {
        self.node.send_vm_command(VmCommand::Start {
            name: name.to_string(),
        }).await
    }
    
    async fn stop_vm(&self, name: &str) -> BlixardResult<()> {
        self.node.send_vm_command(VmCommand::Stop {
            name: name.to_string(),
        }).await
    }
    
    async fn delete_vm(&self, name: &str) -> BlixardResult<()> {
        self.node.send_vm_command(VmCommand::Delete {
            name: name.to_string(),
        }).await
    }
    
    async fn list_vms(&self) -> BlixardResult<Vec<(VmConfig, InternalVmStatus)>> {
        self.node.list_vms().await
    }
    
    async fn get_vm_status(&self, name: &str) -> BlixardResult<Option<(VmConfig, InternalVmStatus)>> {
        self.node.get_vm_status(name).await
    }
    
    async fn migrate_vm(
        &self,
        vm_name: &str,
        target_node_id: u64,
        live_migration: bool,
        force: bool,
    ) -> BlixardResult<()> {
        use crate::types::VmMigrationTask;
        
        // Verify we're the leader
        if !self.node.is_leader().await {
            return Err(BlixardError::Internal {
                message: "Not the leader".to_string(),
            });
        }
        
        let migration_task = VmMigrationTask {
            vm_name: vm_name.to_string(),
            source_node_id: self.node.get_id(),
            target_node_id,
            live_migration,
            force,
        };
        
        self.node.send_vm_command(VmCommand::Migrate { task: migration_task }).await
    }
}

// All VM operations are now handled through the Iroh protocol handler below

/// Iroh protocol handler for VM service
pub struct VmProtocolHandler {
    service: VmServiceImpl,
}

impl VmProtocolHandler {
    pub fn new(node: Arc<SharedNodeState>) -> Self {
        Self {
            service: VmServiceImpl::new(node),
        }
    }
    
    /// Handle a VM operation request over Iroh
    pub async fn handle_request(
        &self,
        _connection: iroh::endpoint::Connection,
        request: VmOperationRequest,
    ) -> BlixardResult<VmOperationResponse> {
        match request {
            VmOperationRequest::Create { name, config_path: _, vcpus, memory_mb } => {
                match self.service.create_vm(name.clone(), vcpus, memory_mb).await {
                    Ok(vm_id) => Ok(VmOperationResponse::Create {
                        success: true,
                        message: format!("VM '{}' created successfully", name),
                        vm_id,
                    }),
                    Err(e) => Ok(VmOperationResponse::Create {
                        success: false,
                        message: e.to_string(),
                        vm_id: String::new(),
                    }),
                }
            }
            VmOperationRequest::Start { name } => {
                match self.service.start_vm(&name).await {
                    Ok(()) => Ok(VmOperationResponse::Start {
                        success: true,
                        message: format!("VM '{}' started", name),
                    }),
                    Err(e) => Ok(VmOperationResponse::Start {
                        success: false,
                        message: e.to_string(),
                    }),
                }
            }
            VmOperationRequest::Stop { name } => {
                match self.service.stop_vm(&name).await {
                    Ok(()) => Ok(VmOperationResponse::Stop {
                        success: true,
                        message: format!("VM '{}' stopped", name),
                    }),
                    Err(e) => Ok(VmOperationResponse::Stop {
                        success: false,
                        message: e.to_string(),
                    }),
                }
            }
            VmOperationRequest::Delete { name } => {
                match self.service.delete_vm(&name).await {
                    Ok(()) => Ok(VmOperationResponse::Delete {
                        success: true,
                        message: format!("VM '{}' deleted", name),
                    }),
                    Err(e) => Ok(VmOperationResponse::Delete {
                        success: false,
                        message: e.to_string(),
                    }),
                }
            }
            VmOperationRequest::List => {
                match self.service.list_vms().await {
                    Ok(vms) => {
                        let vm_infos = vms.into_iter().map(|(config, status)| {
                            VmInfoData {
                                name: config.name,
                                state: format!("{:?}", status),
                                vcpus: config.vcpus,
                                memory_mb: config.memory,
                                node_id: self.service.node.get_id(),
                                ip_address: config.ip_address.unwrap_or_default(),
                            }
                        }).collect();
                        Ok(VmOperationResponse::List { vms: vm_infos })
                    }
                    Err(e) => Err(e),
                }
            }
            VmOperationRequest::GetStatus { name } => {
                match self.service.get_vm_status(&name).await {
                    Ok(Some((config, status))) => {
                        Ok(VmOperationResponse::GetStatus {
                            found: true,
                            vm_info: Some(VmInfoData {
                                name: config.name,
                                state: format!("{:?}", status),
                                vcpus: config.vcpus,
                                memory_mb: config.memory,
                                node_id: self.service.node.get_id(),
                                ip_address: config.ip_address.unwrap_or_default(),
                            }),
                        })
                    }
                    Ok(None) => Ok(VmOperationResponse::GetStatus {
                        found: false,
                        vm_info: None,
                    }),
                    Err(e) => Err(e),
                }
            }
            VmOperationRequest::Migrate { vm_name, target_node_id, live_migration, force } => {
                match self.service.migrate_vm(&vm_name, target_node_id, live_migration, force).await {
                    Ok(()) => Ok(VmOperationResponse::Migrate {
                        success: true,
                        message: format!("Migration of VM '{}' started", vm_name),
                        source_node_id: self.service.node.get_id(),
                        target_node_id,
                        status: 1, // MIGRATION_STATUS_PREPARING
                        duration_ms: 0,
                    }),
                    Err(e) => Ok(VmOperationResponse::Migrate {
                        success: false,
                        message: e.to_string(),
                        source_node_id: self.service.node.get_id(),
                        target_node_id: 0,
                        status: 5, // MIGRATION_STATUS_FAILED
                        duration_ms: 0,
                    }),
                }
            }
        }
    }
}