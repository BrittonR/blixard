use crate::BlixardResult;
use super::app::{VmInfo, ClusterInfo, ClusterResourceInfo, NodeResourceInfo, PlacementStrategy};
use blixard_core::{
    proto::{
        cluster_service_client::ClusterServiceClient,
        CreateVmRequest, CreateVmWithSchedulingRequest, StartVmRequest, StopVmRequest, DeleteVmRequest, 
        GetVmStatusRequest, ListVmsRequest, ClusterStatusRequest, ClusterResourceSummaryRequest,
        ScheduleVmPlacementRequest,
    },
    types::VmStatus,
};
use tonic::transport::Channel;

/// VM client for TUI operations
pub struct VmClient {
    client: ClusterServiceClient<Channel>,
}

impl VmClient {
    pub async fn new(addr: &str) -> BlixardResult<Self> {
        let endpoint = format!("http://{}", addr);
        
        let client = ClusterServiceClient::connect(endpoint).await
            .map_err(|e| crate::BlixardError::Internal {
                message: format!("Failed to connect to blixard node: {}", e),
            })?;

        Ok(Self { client })
    }

    pub async fn list_vms(&mut self) -> BlixardResult<Vec<VmInfo>> {
        let request = tonic::Request::new(ListVmsRequest {});
        
        let response = self.client.list_vms(request).await
            .map_err(|e| crate::BlixardError::Internal {
                message: format!("Failed to list VMs: {}", e),
            })?;

        let resp = response.into_inner();
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
            });
        }

        Ok(vms)
    }

    pub async fn create_vm(&mut self, name: &str, vcpus: u32, memory: u32) -> BlixardResult<()> {
        let request = tonic::Request::new(CreateVmRequest {
            name: name.to_string(),
            config_path: String::new(),
            vcpus,
            memory_mb: memory,
        });

        let response = self.client.create_vm(request).await
            .map_err(|e| crate::BlixardError::Internal {
                message: format!("Failed to create VM: {}", e),
            })?;

        let resp = response.into_inner();
        if !resp.success {
            return Err(crate::BlixardError::Internal {
                message: format!("VM creation failed: {}", resp.message),
            });
        }

        Ok(())
    }

    pub async fn start_vm(&mut self, name: &str) -> BlixardResult<()> {
        let request = tonic::Request::new(StartVmRequest {
            name: name.to_string(),
        });

        let response = self.client.start_vm(request).await
            .map_err(|e| crate::BlixardError::Internal {
                message: format!("Failed to start VM: {}", e),
            })?;

        let resp = response.into_inner();
        if !resp.success {
            return Err(crate::BlixardError::Internal {
                message: format!("VM start failed: {}", resp.message),
            });
        }

        Ok(())
    }

    pub async fn stop_vm(&mut self, name: &str) -> BlixardResult<()> {
        let request = tonic::Request::new(StopVmRequest {
            name: name.to_string(),
        });

        let response = self.client.stop_vm(request).await
            .map_err(|e| crate::BlixardError::Internal {
                message: format!("Failed to stop VM: {}", e),
            })?;

        let resp = response.into_inner();
        if !resp.success {
            return Err(crate::BlixardError::Internal {
                message: format!("VM stop failed: {}", resp.message),
            });
        }

        Ok(())
    }

    pub async fn delete_vm(&mut self, name: &str) -> BlixardResult<()> {
        let request = tonic::Request::new(DeleteVmRequest {
            name: name.to_string(),
        });

        let response = self.client.delete_vm(request).await
            .map_err(|e| crate::BlixardError::Internal {
                message: format!("Failed to delete VM: {}", e),
            })?;

        let resp = response.into_inner();
        if !resp.success {
            return Err(crate::BlixardError::Internal {
                message: format!("VM deletion failed: {}", resp.message),
            });
        }

        Ok(())
    }

    pub async fn get_vm_status(&mut self, name: &str) -> BlixardResult<Option<VmInfo>> {
        let request = tonic::Request::new(GetVmStatusRequest {
            name: name.to_string(),
        });

        let response = self.client.get_vm_status(request).await
            .map_err(|e| crate::BlixardError::Internal {
                message: format!("Failed to get VM status: {}", e),
            })?;

        let resp = response.into_inner();
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
        }))
    }

    pub async fn get_cluster_status(&mut self) -> BlixardResult<ClusterInfo> {
        let request = tonic::Request::new(ClusterStatusRequest {});
        
        let response = self.client.get_cluster_status(request).await
            .map_err(|e| crate::BlixardError::Internal {
                message: format!("Failed to get cluster status: {}", e),
            })?;
            
        let status = response.into_inner();
        
        // Map all nodes with detailed info
        let nodes: Vec<crate::tui::app::NodeInfo> = status.nodes
            .iter()
            .map(|node| {
                let state_name = match node.state {
                    0 => "Unknown",
                    1 => "Follower", 
                    2 => "Candidate",
                    3 => "Leader",
                    _ => "Invalid",
                };
                crate::tui::app::NodeInfo {
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
        
        let request = tonic::Request::new(CreateVmWithSchedulingRequest {
            name: name.to_string(),
            config_path: String::new(),
            vcpus,
            memory_mb: memory,
            strategy: strategy_proto,
        });

        let response = self.client.create_vm_with_scheduling(request).await
            .map_err(|e| crate::BlixardError::Internal {
                message: format!("Failed to create VM with scheduling: {}", e),
            })?;

        let resp = response.into_inner();
        if !resp.success {
            return Err(crate::BlixardError::Internal {
                message: format!("VM creation with scheduling failed: {}", resp.message),
            });
        }

        Ok((resp.selected_node_id, resp.placement_reason))
    }
    
    /// Get cluster resource summary
    pub async fn get_cluster_resources(&mut self) -> BlixardResult<ClusterResourceInfo> {
        let request = tonic::Request::new(ClusterResourceSummaryRequest {});
        
        let response = self.client.get_cluster_resource_summary(request).await
            .map_err(|e| crate::BlixardError::Internal {
                message: format!("Failed to get cluster resources: {}", e),
            })?;
            
        let resp = response.into_inner();
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
        
        let request = tonic::Request::new(ScheduleVmPlacementRequest {
            name: name.to_string(),
            config_path: String::new(),
            vcpus,
            memory_mb: memory,
            strategy: strategy_proto,
        });
        
        let response = self.client.schedule_vm_placement(request).await
            .map_err(|e| crate::BlixardError::Internal {
                message: format!("Failed to schedule VM placement: {}", e),
            })?;
            
        let resp = response.into_inner();
        if !resp.success {
            return Err(crate::BlixardError::Internal {
                message: format!("VM placement scheduling failed: {}", resp.message),
            });
        }
        
        Ok((resp.selected_node_id, resp.placement_reason, resp.alternative_nodes))
    }
}