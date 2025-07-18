//! VM service implementation for dual transport
//!
//! This service handles VM lifecycle operations over both gRPC and Iroh transports.

use crate::{
    error::{BlixardError, BlixardResult},
    iroh_types::VmState,
    node_shared::SharedNodeState,
    types::{VmCommand, VmConfig, VmStatus as InternalVmStatus},
};
use async_trait::async_trait;
use std::sync::Arc;
// Removed tonic imports - using Iroh-only transport
use serde::{Deserialize, Serialize};

/// Parameters for VM creation with scheduling
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmSchedulingParams {
    pub name: String,
    pub vcpus: u32,
    pub memory_mb: u32,
    pub strategy: Option<String>,
    pub constraints: Option<Vec<String>>,
    pub features: Option<Vec<String>>,
    pub priority: Option<u32>,
}

impl VmSchedulingParams {
    /// Create new scheduling parameters with defaults
    pub fn new(name: String, vcpus: u32, memory_mb: u32) -> Self {
        Self {
            name,
            vcpus,
            memory_mb,
            strategy: None,
            constraints: None,
            features: None,
            priority: None,
        }
    }

    /// Set placement strategy
    pub fn with_strategy(mut self, strategy: String) -> Self {
        self.strategy = Some(strategy);
        self
    }

    /// Set constraints
    pub fn with_constraints(mut self, constraints: Vec<String>) -> Self {
        self.constraints = Some(constraints);
        self
    }

    /// Set required features
    pub fn with_features(mut self, features: Vec<String>) -> Self {
        self.features = Some(features);
        self
    }

    /// Set priority
    pub fn with_priority(mut self, priority: u32) -> Self {
        self.priority = Some(priority);
        self
    }
}

/// Parameters for VM placement scheduling (without creation)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmPlacementParams {
    pub name: String,
    pub vcpus: u32,
    pub memory_mb: u32,
    pub strategy: Option<String>,
    pub constraints: Option<Vec<String>>,
    pub features: Option<Vec<String>>,
}

impl VmPlacementParams {
    /// Create new placement parameters with defaults
    pub fn new(name: String, vcpus: u32, memory_mb: u32) -> Self {
        Self {
            name,
            vcpus,
            memory_mb,
            strategy: None,
            constraints: None,
            features: None,
        }
    }

    /// Set placement strategy
    pub fn with_strategy(mut self, strategy: String) -> Self {
        self.strategy = Some(strategy);
        self
    }

    /// Set constraints
    pub fn with_constraints(mut self, constraints: Vec<String>) -> Self {
        self.constraints = Some(constraints);
        self
    }

    /// Set required features
    pub fn with_features(mut self, features: Vec<String>) -> Self {
        self.features = Some(features);
        self
    }
}

/// VM operation request types for Iroh transport
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VmOperationRequest {
    Create {
        name: String,
        config_path: String,
        vcpus: u32,
        memory_mb: u32,
    },
    CreateWithScheduling {
        params: VmSchedulingParams,
    },
    SchedulePlacement {
        params: VmPlacementParams,
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
    CreateWithScheduling {
        success: bool,
        message: String,
        vm_id: String,
        assigned_node_id: u64,
        placement_decision: String,
    },
    SchedulePlacement {
        success: bool,
        assigned_node_id: u64,
        placement_score: f32,
        placement_reason: String,
        alternative_nodes: Vec<u64>,
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

    /// Create a new VM with automatic scheduling
    async fn create_vm_with_scheduling(
        &self,
        params: VmSchedulingParams,
    ) -> BlixardResult<(String, u64, String)>; // Returns (vm_id, node_id, placement_decision)

    /// Schedule VM placement without creating
    async fn schedule_vm_placement(
        &self,
        params: VmPlacementParams,
    ) -> BlixardResult<(u64, f32, String, Vec<u64>)>; // Returns (node_id, score, reason, alternatives)

    /// Start a VM
    async fn start_vm(&self, name: &str) -> BlixardResult<()>;

    /// Stop a VM
    async fn stop_vm(&self, name: &str) -> BlixardResult<()>;

    /// Delete a VM
    async fn delete_vm(&self, name: &str) -> BlixardResult<()>;

    /// List all VMs
    async fn list_vms(&self) -> BlixardResult<Vec<(VmConfig, InternalVmStatus)>>;

    /// Get VM status
    async fn get_vm_status(
        &self,
        name: &str,
    ) -> BlixardResult<Option<(VmConfig, InternalVmStatus)>>;

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
    #[allow(dead_code)]
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

    /// Parse placement strategy from string
    fn parse_strategy(strategy_str: Option<String>) -> crate::vm_scheduler::PlacementStrategy {
        use crate::vm_scheduler::PlacementStrategy;

        match strategy_str.as_deref() {
            Some("most-available") => PlacementStrategy::MostAvailable,
            Some("least-available") => PlacementStrategy::LeastAvailable,
            Some("round-robin") => PlacementStrategy::RoundRobin,
            _ => PlacementStrategy::MostAvailable, // Default
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
        let command = VmCommand::Create {
            config: vm_config,
            node_id: self.node.get_id(),
        };
        let command_str =
            serde_json::to_string(&command).map_err(|e| crate::error::BlixardError::Internal {
                message: format!("Failed to serialize VM command: {}", e),
            })?;
        self.node.send_vm_command(&name, command_str).await?;

        Ok(name)
    }

    async fn start_vm(&self, name: &str) -> BlixardResult<()> {
        let command = VmCommand::Start {
            name: name.to_string(),
        };
        let command_str =
            serde_json::to_string(&command).map_err(|e| crate::error::BlixardError::Internal {
                message: format!("Failed to serialize VM command: {}", e),
            })?;
        self.node
            .send_vm_command(name, command_str)
            .await
            .map(|_| ())
    }

    async fn stop_vm(&self, name: &str) -> BlixardResult<()> {
        let command = VmCommand::Stop {
            name: name.to_string(),
        };
        let command_str =
            serde_json::to_string(&command).map_err(|e| crate::error::BlixardError::Internal {
                message: format!("Failed to serialize VM command: {}", e),
            })?;
        self.node
            .send_vm_command(name, command_str)
            .await
            .map(|_| ())
    }

    async fn delete_vm(&self, name: &str) -> BlixardResult<()> {
        let command = VmCommand::Delete {
            name: name.to_string(),
        };
        let command_str =
            serde_json::to_string(&command).map_err(|e| crate::error::BlixardError::Internal {
                message: format!("Failed to serialize VM command: {}", e),
            })?;
        self.node
            .send_vm_command(name, command_str)
            .await
            .map(|_| ())
    }

    async fn list_vms(&self) -> BlixardResult<Vec<(VmConfig, InternalVmStatus)>> {
        let vm_states = self.node.list_vms().await?;
        Ok(vm_states
            .into_iter()
            .map(|state| (state.config, state.status))
            .collect())
    }

    async fn get_vm_status(
        &self,
        name: &str,
    ) -> BlixardResult<Option<(VmConfig, InternalVmStatus)>> {
        // TODO: Fix this - node.get_vm_status returns String but should return VM state
        let _status_str = self.node.get_vm_status(name).await?;
        // For now, return None - this needs proper implementation
        Ok(None)
    }

    async fn create_vm_with_scheduling(
        &self,
        params: VmSchedulingParams,
    ) -> BlixardResult<(String, u64, String)> {
        use crate::anti_affinity::{AntiAffinityRule, AntiAffinityRules};

        let mut vm_config = VmConfig {
            name: params.name.clone(),
            config_path: format!("/etc/blixard/vms/{}.yaml", params.name),
            vcpus: params.vcpus,
            memory: params.memory_mb,
            tenant_id: "default".to_string(),
            ip_address: None,
            metadata: None,
            anti_affinity: None,
            priority: params.priority.unwrap_or(100),
            ..Default::default()
        };

        // Convert constraints to anti-affinity rules
        if let Some(constraints) = params.constraints {
            let rules: Vec<AntiAffinityRule> = constraints
                .into_iter()
                .map(|group| AntiAffinityRule::hard(group))
                .collect();
            if !rules.is_empty() {
                vm_config.anti_affinity = Some(AntiAffinityRules { rules });
            }
        }

        // Add required features
        if let Some(features) = params.features {
            vm_config.metadata = Some(
                vec![("required_features".to_string(), features.join(","))]
                    .into_iter()
                    .collect(),
            );
        }

        let _strategy = Self::parse_strategy(params.strategy);

        // Use the scheduling method from SharedNodeState
        let _result = self.node.create_vm_with_scheduling(vm_config).await?;

        // TODO: Integrate with proper scheduler that returns PlacementDecision
        // For now, return placeholder values since create_vm_with_scheduling returns String, not PlacementDecision
        Ok((params.name, 1u64, "VM created with default placement".to_string()))
    }

    async fn schedule_vm_placement(
        &self,
        params: VmPlacementParams,
    ) -> BlixardResult<(u64, f32, String, Vec<u64>)> {
        use crate::anti_affinity::{AntiAffinityRule, AntiAffinityRules};

        let mut vm_config = VmConfig {
            name: params.name.clone(),
            config_path: format!("/etc/blixard/vms/{}.yaml", params.name),
            vcpus: params.vcpus,
            memory: params.memory_mb,
            tenant_id: "default".to_string(),
            ip_address: None,
            metadata: None,
            anti_affinity: None,
            ..Default::default()
        };

        // Convert constraints to anti-affinity rules
        if let Some(constraints) = params.constraints {
            let rules: Vec<AntiAffinityRule> = constraints
                .into_iter()
                .map(|group| AntiAffinityRule::hard(group))
                .collect();
            if !rules.is_empty() {
                vm_config.anti_affinity = Some(AntiAffinityRules { rules });
            }
        }

        // Add required features
        if let Some(features) = params.features {
            vm_config.metadata = Some(
                vec![("required_features".to_string(), features.join(","))]
                    .into_iter()
                    .collect(),
            );
        }

        let _strategy = Self::parse_strategy(params.strategy.clone());

        // TODO: Implement VM scheduling integration
        let decision = crate::vm_scheduler_modules::placement_strategies::PlacementDecision {
            target_node_id: 1, // For now, always assign to node 1
            strategy_used: params.strategy.unwrap_or_else(|| "default".to_string()),
            confidence_score: 100.0,
            preempted_vms: Vec::new(),
            resource_fit_score: 100.0,
            selected_node_id: 1, // Same as target_node_id for compatibility
            alternative_nodes: Vec::new(),
            reason: "Default assignment".to_string(),
        };

        Ok((
            decision.target_node_id,
            decision.confidence_score as f32,
            decision.reason,
            decision.alternative_nodes,
        ))
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
        if !self.node.is_leader() {
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

        let command = VmCommand::Migrate {
            task: migration_task,
        };
        let command_str =
            serde_json::to_string(&command).map_err(|e| crate::error::BlixardError::Internal {
                message: format!("Failed to serialize VM command: {}", e),
            })?;
        self.node.send_vm_command(vm_name, command_str).await?;
        Ok(())
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
            VmOperationRequest::Create {
                name,
                config_path: _,
                vcpus,
                memory_mb,
            } => match self.service.create_vm(name.clone(), vcpus, memory_mb).await {
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
            },
            VmOperationRequest::Start { name } => match self.service.start_vm(&name).await {
                Ok(()) => Ok(VmOperationResponse::Start {
                    success: true,
                    message: format!("VM '{}' started", name),
                }),
                Err(e) => Ok(VmOperationResponse::Start {
                    success: false,
                    message: e.to_string(),
                }),
            },
            VmOperationRequest::Stop { name } => match self.service.stop_vm(&name).await {
                Ok(()) => Ok(VmOperationResponse::Stop {
                    success: true,
                    message: format!("VM '{}' stopped", name),
                }),
                Err(e) => Ok(VmOperationResponse::Stop {
                    success: false,
                    message: e.to_string(),
                }),
            },
            VmOperationRequest::Delete { name } => match self.service.delete_vm(&name).await {
                Ok(()) => Ok(VmOperationResponse::Delete {
                    success: true,
                    message: format!("VM '{}' deleted", name),
                }),
                Err(e) => Ok(VmOperationResponse::Delete {
                    success: false,
                    message: e.to_string(),
                }),
            },
            VmOperationRequest::List => match self.service.list_vms().await {
                Ok(vms) => {
                    let vm_infos = vms
                        .into_iter()
                        .map(|(config, status)| VmInfoData {
                            name: config.name,
                            state: format!("{:?}", status),
                            vcpus: config.vcpus,
                            memory_mb: config.memory,
                            node_id: self.service.node.get_id(),
                            ip_address: config.ip_address.unwrap_or_default(),
                        })
                        .collect();
                    Ok(VmOperationResponse::List { vms: vm_infos })
                }
                Err(e) => Err(e),
            },
            VmOperationRequest::GetStatus { name } => {
                match self.service.get_vm_status(&name).await {
                    Ok(Some((config, status))) => Ok(VmOperationResponse::GetStatus {
                        found: true,
                        vm_info: Some(VmInfoData {
                            name: config.name,
                            state: format!("{:?}", status),
                            vcpus: config.vcpus,
                            memory_mb: config.memory,
                            node_id: self.service.node.get_id(),
                            ip_address: config.ip_address.unwrap_or_default(),
                        }),
                    }),
                    Ok(None) => Ok(VmOperationResponse::GetStatus {
                        found: false,
                        vm_info: None,
                    }),
                    Err(e) => Err(e),
                }
            }
            VmOperationRequest::Migrate {
                vm_name,
                target_node_id,
                live_migration,
                force,
            } => {
                match self
                    .service
                    .migrate_vm(&vm_name, target_node_id, live_migration, force)
                    .await
                {
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
            VmOperationRequest::CreateWithScheduling {
                params,
            } => {
                match self
                    .service
                    .create_vm_with_scheduling(params.clone())
                    .await
                {
                    Ok((vm_id, node_id, reason)) => Ok(VmOperationResponse::CreateWithScheduling {
                        success: true,
                        message: format!("VM '{}' created successfully on node {}", params.name, node_id),
                        vm_id,
                        assigned_node_id: node_id,
                        placement_decision: reason,
                    }),
                    Err(e) => Ok(VmOperationResponse::CreateWithScheduling {
                        success: false,
                        message: e.to_string(),
                        vm_id: String::new(),
                        assigned_node_id: 0,
                        placement_decision: String::new(),
                    }),
                }
            }
            VmOperationRequest::SchedulePlacement {
                params,
            } => {
                match self
                    .service
                    .schedule_vm_placement(params)
                    .await
                {
                    Ok((node_id, score, reason, alternatives)) => {
                        Ok(VmOperationResponse::SchedulePlacement {
                            success: true,
                            assigned_node_id: node_id,
                            placement_score: score,
                            placement_reason: reason,
                            alternative_nodes: alternatives,
                        })
                    }
                    Err(e) => Ok(VmOperationResponse::SchedulePlacement {
                        success: false,
                        assigned_node_id: 0,
                        placement_score: 0.0,
                        placement_reason: e.to_string(),
                        alternative_nodes: vec![],
                    }),
                }
            }
        }
    }
}
