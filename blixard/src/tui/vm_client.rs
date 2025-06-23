use crate::BlixardResult;
use super::app::{VmInfo, ClusterInfo};
use blixard_core::{
    proto::{
        cluster_service_client::ClusterServiceClient,
        CreateVmRequest, StartVmRequest, StopVmRequest, DeleteVmRequest, GetVmStatusRequest, ListVmsRequest,
        ClusterStatusRequest,
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
}