use crate::{BlixardResult, client::{UnifiedClient, get_transport_config}};
use super::app::{VmInfo, PlacementStrategy, ClusterInfo, ClusterResourceInfo, NodeResourceInfo, ClusterNodeInfo};
use blixard_core::{
    proto::{
        CreateVmRequest, CreateVmWithSchedulingRequest, StartVmRequest, StopVmRequest, DeleteVmRequest, 
        GetVmStatusRequest, ListVmsRequest, ClusterStatusRequest, ClusterResourceSummaryRequest,
        ScheduleVmPlacementRequest,
    },
    types::VmStatus,
};
use std::time::Duration;
use tokio::time::sleep;

/// Retry configuration for network operations
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Maximum number of retry attempts
    pub max_attempts: u32,
    /// Initial delay between retries
    pub initial_delay: Duration,
    /// Maximum delay between retries
    pub max_delay: Duration,
    /// Exponential backoff multiplier
    pub backoff_multiplier: f64,
    /// Timeout for individual requests
    pub request_timeout: Duration,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(5),
            backoff_multiplier: 2.0,
            request_timeout: Duration::from_secs(10),
        }
    }
}


/// VM client for TUI operations with retry support
pub struct VmClient {
    client: UnifiedClient,
    retry_config: RetryConfig,
}

impl VmClient {
    pub async fn new(addr: &str) -> BlixardResult<Self> {
        Self::new_with_config(addr, RetryConfig::default()).await
    }
    
    pub async fn new_with_config(addr: &str, retry_config: RetryConfig) -> BlixardResult<Self> {
        let transport_config = get_transport_config();
        
        // Try to connect with retries
        let client = Self::retry_operation(
            || async {
                UnifiedClient::new(addr, transport_config.as_ref())
                    .await
                    .map_err(|e| crate::BlixardError::Internal {
                        message: format!("Failed to connect to blixard node: {}", e),
                    })
            },
            &retry_config,
            "connect",
        ).await?;

        Ok(Self { client, retry_config })
    }
    
    /// Execute an operation with retry logic
    async fn retry_operation<F, Fut, T>(
        operation: F,
        config: &RetryConfig,
        operation_name: &str,
    ) -> BlixardResult<T>
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = BlixardResult<T>>,
    {
        let mut attempt = 0;
        let mut delay = config.initial_delay;
        
        loop {
            attempt += 1;
            
            match tokio::time::timeout(config.request_timeout, operation()).await {
                Ok(Ok(result)) => return Ok(result),
                Ok(Err(e)) if attempt >= config.max_attempts => return Err(e),
                Ok(Err(e)) => {
                    tracing::warn!(
                        "Operation '{}' failed (attempt {}/{}): {}",
                        operation_name,
                        attempt,
                        config.max_attempts,
                        e
                    );
                }
                Err(_) if attempt >= config.max_attempts => {
                    return Err(crate::BlixardError::Internal {
                        message: format!(
                            "Operation '{}' timed out after {} attempts",
                            operation_name,
                            config.max_attempts
                        ),
                    });
                }
                Err(_) => {
                    tracing::warn!(
                        "Operation '{}' timed out (attempt {}/{})",
                        operation_name,
                        attempt,
                        config.max_attempts
                    );
                }
            }
            
            // Wait before retry with exponential backoff
            sleep(delay).await;
            delay = std::cmp::min(
                Duration::from_secs_f64(delay.as_secs_f64() * config.backoff_multiplier),
                config.max_delay,
            );
        }
    }

    pub async fn list_vms(&mut self) -> BlixardResult<Vec<VmInfo>> {
        let request = ListVmsRequest {};
        let resp = self.client.list_vms(request).await
            .map_err(|e| crate::BlixardError::Internal {
                message: format!("Failed to list VMs: {}", e),
            })?;
        let mut vms = Vec::new();

        for vm in resp.vms {
            let status = match vm.state {
                0 => VmStatus::Stopped,    // VM_STATE_UNKNOWN -> Stopped
                1 => VmStatus::Creating,   // VM_STATE_CREATED -> Creating  
                2 => VmStatus::Starting,   // VM_STATE_STARTING -> Starting
                3 => VmStatus::Running,    // VM_STATE_RUNNING -> Running
                4 => VmStatus::Stopping,   // VM_STATE_STOPPING -> Stopping
                5 => VmStatus::Stopped,    // VM_STATE_STOPPED -> Stopped
                6 => VmStatus::Failed,     // VM_STATE_FAILED -> Failed
                _ => VmStatus::Failed,
            };

            vms.push(VmInfo {
                name: vm.name,
                status,
                vcpus: vm.vcpus,
                memory: vm.memory_mb,
                node_id: vm.node_id,
                ip_address: if vm.ip_address.is_empty() { None } else { Some(vm.ip_address) },
                uptime: None, // TODO: Get from VM status
                cpu_usage: None, // TODO: Get from VM metrics
                memory_usage: None, // TODO: Get from VM metrics
                placement_strategy: None, // TODO: Add to proto
                created_at: None, // TODO: Add to proto
                config_path: None, // TODO: Add to proto
            });
        }

        Ok(vms)
    }

    pub async fn create_vm(&mut self, name: &str, vcpus: u32, memory: u32) -> BlixardResult<()> {
        let request = CreateVmRequest {
            name: name.to_string(),
            config_path: String::new(),
            vcpus,
            memory_mb: memory,
        };

        let resp = self.client.create_vm(request).await
            .map_err(|e| crate::BlixardError::Internal {
                message: format!("Failed to create VM: {}", e),
            })?;

        if !resp.success {
            return Err(crate::BlixardError::Internal {
                message: format!("VM creation failed: {}", resp.message),
            });
        }

        Ok(())
    }

    pub async fn start_vm(&mut self, name: &str) -> BlixardResult<()> {
        let request = StartVmRequest {
            name: name.to_string(),
        };

        let resp = self.client.start_vm(request).await
            .map_err(|e| crate::BlixardError::Internal {
                message: format!("Failed to start VM: {}", e),
            })?;

        if !resp.success {
            return Err(crate::BlixardError::Internal {
                message: format!("VM start failed: {}", resp.message),
            });
        }

        Ok(())
    }

    pub async fn stop_vm(&mut self, name: &str) -> BlixardResult<()> {
        let request = StopVmRequest {
            name: name.to_string(),
        };

        let resp = self.client.stop_vm(request).await
            .map_err(|e| crate::BlixardError::Internal {
                message: format!("Failed to stop VM: {}", e),
            })?;

        if !resp.success {
            return Err(crate::BlixardError::Internal {
                message: format!("VM stop failed: {}", resp.message),
            });
        }

        Ok(())
    }

    pub async fn delete_vm(&mut self, name: &str) -> BlixardResult<()> {
        let request = DeleteVmRequest {
            name: name.to_string(),
        };

        let resp = self.client.delete_vm(request).await
            .map_err(|e| crate::BlixardError::Internal {
                message: format!("Failed to delete VM: {}", e),
            })?;

        if !resp.success {
            return Err(crate::BlixardError::Internal {
                message: format!("VM deletion failed: {}", resp.message),
            });
        }

        Ok(())
    }

    #[allow(dead_code)]
    pub async fn get_vm_status(&mut self, name: &str) -> BlixardResult<Option<VmInfo>> {
        let request = GetVmStatusRequest {
            name: name.to_string(),
        };

        let resp = self.client.get_vm_status(request).await
            .map_err(|e| crate::BlixardError::Internal {
                message: format!("Failed to get VM status: {}", e),
            })?;

        if !resp.found {
            return Ok(None);
        }

        let vm = resp.vm_info.unwrap();
        let status = match vm.state {
            0 => VmStatus::Stopped,    // VM_STATE_UNKNOWN -> Stopped
            1 => VmStatus::Creating,   // VM_STATE_CREATED -> Creating
            2 => VmStatus::Starting,   // VM_STATE_STARTING -> Starting
            3 => VmStatus::Running,    // VM_STATE_RUNNING -> Running
            4 => VmStatus::Stopping,   // VM_STATE_STOPPING -> Stopping
            5 => VmStatus::Stopped,    // VM_STATE_STOPPED -> Stopped
            6 => VmStatus::Failed,     // VM_STATE_FAILED -> Failed
            _ => VmStatus::Failed,
        };

        Ok(Some(VmInfo {
            name: vm.name,
            status,
            vcpus: vm.vcpus,
            memory: vm.memory_mb,
            node_id: vm.node_id,
            ip_address: if vm.ip_address.is_empty() { None } else { Some(vm.ip_address) },
            uptime: None, // TODO: Get from VM status
            cpu_usage: None, // TODO: Get from VM metrics
            memory_usage: None, // TODO: Get from VM metrics
            placement_strategy: None, // TODO: Add to proto
            created_at: None, // TODO: Add to proto
            config_path: None, // TODO: Add to proto
        }))
    }

    pub async fn get_cluster_status(&mut self) -> BlixardResult<ClusterInfo> {
        let request = ClusterStatusRequest {};
        
        let status = self.client.get_cluster_status(request).await
            .map_err(|e| crate::BlixardError::Internal {
                message: format!("Failed to get cluster status: {}", e),
            })?;
        
        // Map all nodes with detailed info
        let nodes: Vec<ClusterNodeInfo> = status.nodes
            .iter()
            .map(|node| {
                let state_name = match node.state {
                    0 => "Unknown",
                    1 => "Follower", 
                    2 => "Candidate",
                    3 => "Leader",
                    _ => "Invalid",
                };
                ClusterNodeInfo {
                    id: node.id,
                    address: node.address.clone(),
                    state: state_name.to_string(),
                    is_current: node.id == status.leader_id || status.nodes.len() == 1, // Rough heuristic
                }
            })
            .collect();
        
        // Find current node info
        let (current_node_id, current_node_state) = nodes
            .iter()
            .find(|node| node.id == status.leader_id || status.nodes.len() == 1)
            .map(|node| (node.id, node.state.clone()))
            .unwrap_or((0, "Unknown".to_string()));
        
        Ok(ClusterInfo {
            leader_id: status.leader_id,
            term: status.term,
            node_count: status.nodes.len(),
            current_node_id,
            current_node_state,
            nodes,
        })
    }
    
    /// Create VM with intelligent scheduling
    #[allow(dead_code)]
    pub async fn create_vm_with_scheduling(
        &mut self, 
        name: &str, 
        vcpus: u32, 
        memory: u32, 
        placement_strategy: PlacementStrategy
    ) -> BlixardResult<(u64, String)> {
        let strategy_proto = match placement_strategy {
            PlacementStrategy::MostAvailable => 0, // PLACEMENT_STRATEGY_MOST_AVAILABLE
            PlacementStrategy::LeastAvailable => 1, // PLACEMENT_STRATEGY_LEAST_AVAILABLE
            PlacementStrategy::RoundRobin => 2, // PLACEMENT_STRATEGY_ROUND_ROBIN
            PlacementStrategy::Manual => 3, // PLACEMENT_STRATEGY_MANUAL
        };
        
        let request = CreateVmWithSchedulingRequest {
            name: name.to_string(),
            config_path: String::new(),
            vcpus,
            memory_mb: memory,
            strategy: strategy_proto,
        };

        let resp = self.client.create_vm_with_scheduling(request).await
            .map_err(|e| crate::BlixardError::Internal {
                message: format!("Failed to create VM with scheduling: {}", e),
            })?;

        if !resp.success {
            return Err(crate::BlixardError::Internal {
                message: format!("VM creation with scheduling failed: {}", resp.message),
            });
        }

        Ok((resp.selected_node_id, resp.placement_reason))
    }
    
    /// Get cluster resource summary
    #[allow(dead_code)]
    pub async fn get_cluster_resources(&mut self) -> BlixardResult<ClusterResourceInfo> {
        let request = ClusterResourceSummaryRequest {};
        
        let resp = self.client.get_cluster_resource_summary(request).await
            .map_err(|e| crate::BlixardError::Internal {
                message: format!("Failed to get cluster resources: {}", e),
            })?;
            
        if !resp.success {
            return Err(crate::BlixardError::Internal {
                message: format!("Failed to get cluster resources: {}", resp.message),
            });
        }
        
        let summary = resp.summary.ok_or_else(|| crate::BlixardError::Internal {
            message: "No cluster resource summary in response".to_string(),
        })?;
        
        let nodes = summary.nodes
            .into_iter()
            .map(|node| {
                let capabilities = node.capabilities.unwrap_or_default();
                NodeResourceInfo {
                    node_id: node.node_id,
                    cpu_cores: capabilities.cpu_cores,
                    memory_mb: capabilities.memory_mb,
                    disk_gb: capabilities.disk_gb,
                    used_vcpus: node.used_vcpus,
                    used_memory_mb: node.used_memory_mb,
                    used_disk_gb: node.used_disk_gb,
                    running_vms: node.running_vms,
                    features: capabilities.features,
                }
            })
            .collect();
        
        Ok(ClusterResourceInfo {
            total_nodes: summary.total_nodes,
            total_vcpus: summary.total_vcpus,
            used_vcpus: summary.used_vcpus,
            total_memory_mb: summary.total_memory_mb,
            used_memory_mb: summary.used_memory_mb,
            total_disk_gb: summary.total_disk_gb,
            used_disk_gb: summary.used_disk_gb,
            total_running_vms: summary.total_running_vms,
            nodes,
        })
    }
    
    /// Schedule VM placement (dry run)
    #[allow(dead_code)]
    pub async fn schedule_vm_placement(
        &mut self,
        name: &str,
        vcpus: u32,
        memory: u32,
        placement_strategy: PlacementStrategy,
    ) -> BlixardResult<(u64, String, Vec<u64>)> {
        let strategy_proto = match placement_strategy {
            PlacementStrategy::MostAvailable => 0,
            PlacementStrategy::LeastAvailable => 1,
            PlacementStrategy::RoundRobin => 2,
            PlacementStrategy::Manual => 3,
        };
        
        let request = ScheduleVmPlacementRequest {
            name: name.to_string(),
            config_path: String::new(),
            vcpus,
            memory_mb: memory,
            strategy: strategy_proto,
        };
        
        let resp = self.client.schedule_vm_placement(request).await
            .map_err(|e| crate::BlixardError::Internal {
                message: format!("Failed to schedule VM placement: {}", e),
            })?;
            
        if !resp.success {
            return Err(crate::BlixardError::Internal {
                message: format!("VM placement scheduling failed: {}", resp.message),
            });
        }
        
        Ok((resp.selected_node_id, resp.placement_reason, resp.alternative_nodes))
    }
    
    /// Join a node to the cluster
    pub async fn join_cluster(&mut self, node_id: u64, bind_address: &str) -> BlixardResult<String> {
        use blixard_core::proto::JoinRequest;
        
        let request = JoinRequest {
            node_id,
            bind_address: bind_address.to_string(),
        };
        
        let resp = self.client.join_cluster(request).await
            .map_err(|e| crate::BlixardError::Internal {
                message: format!("Failed to join cluster: {}", e),
            })?;
            
        if !resp.success {
            return Err(crate::BlixardError::Internal {
                message: format!("Join cluster failed: {}", resp.message),
            });
        }
        
        Ok(resp.message)
    }
    
    /// Migrate a VM to another node
    pub async fn migrate_vm(
        &mut self, 
        vm_name: &str, 
        target_node_id: u64, 
        live_migration: bool
    ) -> BlixardResult<(u64, u64, String)> {
        use blixard_core::proto::MigrateVmRequest;
        
        let request = MigrateVmRequest {
            vm_name: vm_name.to_string(),
            target_node_id,
            live_migration,
            force: false,
        };
        
        let resp = self.client.migrate_vm(request).await
            .map_err(|e| crate::BlixardError::Internal {
                message: format!("Failed to migrate VM: {}", e),
            })?;
        
        if !resp.success {
            return Err(crate::BlixardError::Internal {
                message: format!("VM migration failed: {}", resp.message),
            });
        }
        
        Ok((resp.source_node_id, resp.target_node_id, resp.message))
    }
}