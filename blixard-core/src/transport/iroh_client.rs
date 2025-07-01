//! Iroh client implementation for making RPC calls
//!
//! This module provides client-side functionality for calling Iroh services.

use crate::error::{BlixardError, BlixardResult};
use crate::transport::iroh_protocol::{
    deserialize_payload, generate_request_id, read_message, serialize_payload,
    write_message, MessageType, RpcRequest, RpcResponse,
};
use crate::transport::iroh_cluster_service::{
    ClusterRequest, ClusterResponse, JoinClusterRequest, JoinClusterResponse,
    LeaveClusterRequest, LeaveClusterResponse, ClusterStatusRequest, ClusterStatusResponse,
    TaskRequest, TaskResponse, TaskStatusRequest, TaskStatusResponse,
    ProposeTaskRequest, ProposeTaskResponse,
};
use crate::transport::iroh_vm_service::{
    VmRequest, VmResponse, CreateVmRequest as IrohCreateVmRequest, CreateVmResponse as IrohCreateVmResponse,
    StartVmRequest as IrohStartVmRequest, StartVmResponse as IrohStartVmResponse,
    StopVmRequest as IrohStopVmRequest, StopVmResponse as IrohStopVmResponse,
    DeleteVmRequest as IrohDeleteVmRequest, DeleteVmResponse as IrohDeleteVmResponse,
    GetVmStatusRequest as IrohGetVmStatusRequest, GetVmStatusResponse as IrohGetVmStatusResponse,
    ListVmsRequest as IrohListVmsRequest, ListVmsResponse as IrohListVmsResponse,
};
use bytes::Bytes;
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, error};

/// Client for making Iroh RPC calls
pub struct IrohClient {
    endpoint: Arc<iroh::Endpoint>,
    node_addr: iroh::NodeAddr,
}

impl IrohClient {
    /// Create a new Iroh client
    pub fn new(endpoint: Arc<iroh::Endpoint>, node_addr: iroh::NodeAddr) -> Self {
        Self { endpoint, node_addr }
    }
    
    /// Make an RPC call to a service
    async fn call_service<Req, Resp>(
        &self,
        service: &str,
        method: &str,
        request: Req,
    ) -> BlixardResult<Resp>
    where
        Req: serde::Serialize,
        Resp: for<'de> serde::Deserialize<'de>,
    {
        debug!("Calling {}.{} on node {}", service, method, self.node_addr.node_id);
        
        // Serialize request
        let payload = serialize_payload(&request)?;
        
        // Create RPC request
        let rpc_request = RpcRequest {
            service: service.to_string(),
            method: method.to_string(),
            payload,
        };
        
        // Connect to peer
        let conn = self.endpoint
            .connect(self.node_addr.clone(), crate::transport::BLIXARD_ALPN)
            .await
            .map_err(|e| BlixardError::NetworkError(format!("Failed to connect: {}", e)))?;
        
        // Open a bidirectional stream
        let (mut send, mut recv) = conn
            .open_bi()
            .await
            .map_err(|e| BlixardError::NetworkError(format!("Failed to open stream: {}", e)))?;
        
        // Send request
        let request_id = generate_request_id();
        let request_payload = serialize_payload(&rpc_request)?;
        write_message(&mut send, MessageType::Request, request_id, &request_payload).await?;
        
        // Set read timeout
        let timeout = Duration::from_secs(30);
        let read_fut = read_message(&mut recv);
        
        // Read response with timeout
        let (header, response_bytes) = tokio::time::timeout(timeout, read_fut)
            .await
            .map_err(|_| BlixardError::Internal {
                message: "RPC call timed out".to_string(),
            })??;
        
        // Validate response
        if header.msg_type != MessageType::Response {
            return Err(BlixardError::Internal {
                message: format!("Expected response, got {:?}", header.msg_type),
            });
        }
        
        if header.request_id != request_id {
            return Err(BlixardError::Internal {
                message: "Response request ID mismatch".to_string(),
            });
        }
        
        // Deserialize RPC response
        let rpc_response: RpcResponse = deserialize_payload(&response_bytes)?;
        
        if !rpc_response.success {
            return Err(BlixardError::Internal {
                message: rpc_response.error.unwrap_or_else(|| "Unknown error".to_string()),
            });
        }
        
        // Deserialize payload
        let payload = rpc_response.payload.ok_or_else(|| BlixardError::Internal {
            message: "Missing response payload".to_string(),
        })?;
        
        deserialize_payload(&payload)
    }
    
    // Cluster service methods
    
    pub async fn join_cluster(&self, node_id: u64, bind_address: String) -> BlixardResult<JoinClusterResponse> {
        let request = ClusterRequest::JoinCluster(JoinClusterRequest {
            node_id,
            bind_address,
        });
        
        let response: ClusterResponse = self.call_service("cluster", "join_cluster", request).await?;
        
        match response {
            ClusterResponse::JoinCluster(resp) => Ok(resp),
            _ => Err(BlixardError::Internal {
                message: "Unexpected response type".to_string(),
            }),
        }
    }
    
    pub async fn leave_cluster(&self, node_id: u64) -> BlixardResult<LeaveClusterResponse> {
        let request = ClusterRequest::LeaveCluster(LeaveClusterRequest { node_id });
        
        let response: ClusterResponse = self.call_service("cluster", "leave_cluster", request).await?;
        
        match response {
            ClusterResponse::LeaveCluster(resp) => Ok(resp),
            _ => Err(BlixardError::Internal {
                message: "Unexpected response type".to_string(),
            }),
        }
    }
    
    pub async fn get_cluster_status(&self) -> BlixardResult<ClusterStatusResponse> {
        let request = ClusterRequest::GetClusterStatus(ClusterStatusRequest {});
        
        let response: ClusterResponse = self.call_service("cluster", "get_cluster_status", request).await?;
        
        match response {
            ClusterResponse::GetClusterStatus(resp) => Ok(resp),
            _ => Err(BlixardError::Internal {
                message: "Unexpected response type".to_string(),
            }),
        }
    }
    
    pub async fn submit_task(&self, task: TaskRequest) -> BlixardResult<TaskResponse> {
        let request = ClusterRequest::SubmitTask(task);
        
        let response: ClusterResponse = self.call_service("cluster", "submit_task", request).await?;
        
        match response {
            ClusterResponse::SubmitTask(resp) => Ok(resp),
            _ => Err(BlixardError::Internal {
                message: "Unexpected response type".to_string(),
            }),
        }
    }
    
    pub async fn get_task_status(&self, task_id: String) -> BlixardResult<TaskStatusResponse> {
        let request = ClusterRequest::GetTaskStatus(TaskStatusRequest { task_id });
        
        let response: ClusterResponse = self.call_service("cluster", "get_task_status", request).await?;
        
        match response {
            ClusterResponse::GetTaskStatus(resp) => Ok(resp),
            _ => Err(BlixardError::Internal {
                message: "Unexpected response type".to_string(),
            }),
        }
    }
    
    pub async fn propose_task(&self, task: crate::transport::iroh_cluster_service::Task) -> BlixardResult<ProposeTaskResponse> {
        let request = ClusterRequest::ProposeTask(ProposeTaskRequest { task });
        
        let response: ClusterResponse = self.call_service("cluster", "propose_task", request).await?;
        
        match response {
            ClusterResponse::ProposeTask(resp) => Ok(resp),
            _ => Err(BlixardError::Internal {
                message: "Unexpected response type".to_string(),
            }),
        }
    }
    
    // VM operations
    
    pub async fn create_vm(&self, name: String, config_path: String, vcpus: u32, memory_mb: u32) -> BlixardResult<IrohCreateVmResponse> {
        let request = VmRequest::CreateVm(IrohCreateVmRequest {
            name: name.clone(),
            config: crate::types::VmConfig {
                name,
                config_path,
                vcpus,
                memory: memory_mb,
                disk_size: 10, // Default disk size
                network: crate::types::NetworkConfig::default(),
                kernel: None,
                initrd: None,
                kernel_cmdline: None,
                tap_device: None,
                mac_address: None,
                socket_path: None,
                metrics_socket_path: None,
                log_level: None,
                jailer_cfg: None,
                boot_args: None,
                tags: std::collections::HashMap::new(),
            },
            node_id: None,
        });
        
        let response: VmResponse = self.call_service("vm", "create_vm", request).await?;
        
        match response {
            VmResponse::CreateVm(resp) => Ok(resp),
            _ => Err(BlixardError::Internal {
                message: "Unexpected response type".to_string(),
            }),
        }
    }
    
    pub async fn start_vm(&self, name: String) -> BlixardResult<IrohStartVmResponse> {
        let request = VmRequest::StartVm(IrohStartVmRequest { name });
        
        let response: VmResponse = self.call_service("vm", "start_vm", request).await?;
        
        match response {
            VmResponse::StartVm(resp) => Ok(resp),
            _ => Err(BlixardError::Internal {
                message: "Unexpected response type".to_string(),
            }),
        }
    }
    
    pub async fn stop_vm(&self, name: String) -> BlixardResult<IrohStopVmResponse> {
        let request = VmRequest::StopVm(IrohStopVmRequest { name });
        
        let response: VmResponse = self.call_service("vm", "stop_vm", request).await?;
        
        match response {
            VmResponse::StopVm(resp) => Ok(resp),
            _ => Err(BlixardError::Internal {
                message: "Unexpected response type".to_string(),
            }),
        }
    }
    
    pub async fn delete_vm(&self, name: String) -> BlixardResult<IrohDeleteVmResponse> {
        let request = VmRequest::DeleteVm(IrohDeleteVmRequest { name });
        
        let response: VmResponse = self.call_service("vm", "delete_vm", request).await?;
        
        match response {
            VmResponse::DeleteVm(resp) => Ok(resp),
            _ => Err(BlixardError::Internal {
                message: "Unexpected response type".to_string(),
            }),
        }
    }
    
    pub async fn get_vm_status(&self, name: String) -> BlixardResult<IrohGetVmStatusResponse> {
        let request = VmRequest::GetVmStatus(IrohGetVmStatusRequest { name });
        
        let response: VmResponse = self.call_service("vm", "get_vm_status", request).await?;
        
        match response {
            VmResponse::GetVmStatus(resp) => Ok(resp),
            _ => Err(BlixardError::Internal {
                message: "Unexpected response type".to_string(),
            }),
        }
    }
    
    pub async fn list_vms(&self) -> BlixardResult<IrohListVmsResponse> {
        let request = VmRequest::ListVms(IrohListVmsRequest {});
        
        let response: VmResponse = self.call_service("vm", "list_vms", request).await?;
        
        match response {
            VmResponse::ListVms(resp) => Ok(resp),
            _ => Err(BlixardError::Internal {
                message: "Unexpected response type".to_string(),
            }),
        }
    }
}

/// Wrapper client that provides a gRPC-like interface but uses Iroh underneath
pub struct IrohClusterServiceClient {
    client: IrohClient,
}

impl IrohClusterServiceClient {
    pub fn new(endpoint: Arc<iroh::Endpoint>, node_addr: iroh::NodeAddr) -> Self {
        Self {
            client: IrohClient::new(endpoint, node_addr),
        }
    }
    
    /// Join a cluster
    pub async fn join_cluster(&self, node_id: u64, bind_address: String) -> BlixardResult<(bool, String, Vec<crate::types::NodeInfo>, Vec<u64>)> {
        let response = self.client.join_cluster(node_id, bind_address).await?;
        Ok((response.success, response.message, response.peers, response.voters))
    }
    
    /// Leave a cluster
    pub async fn leave_cluster(&self, node_id: u64) -> BlixardResult<(bool, String)> {
        let response = self.client.leave_cluster(node_id).await?;
        Ok((response.success, response.message))
    }
    
    /// Get cluster status
    pub async fn get_cluster_status(&self) -> BlixardResult<(u64, Vec<crate::types::NodeInfo>, u64)> {
        let response = self.client.get_cluster_status().await?;
        Ok((response.leader_id, response.nodes, response.term))
    }
    
    // VM operations wrappers
    
    /// Create a VM
    pub async fn create_vm(&self, name: String, config_path: String, vcpus: u32, memory_mb: u32) -> BlixardResult<(bool, String, String)> {
        let response = self.client.create_vm(name, config_path, vcpus, memory_mb).await?;
        Ok((response.success, response.message, response.vm_id))
    }
    
    /// Start a VM
    pub async fn start_vm(&self, name: String) -> BlixardResult<(bool, String)> {
        let response = self.client.start_vm(name).await?;
        Ok((response.success, response.message))
    }
    
    /// Stop a VM
    pub async fn stop_vm(&self, name: String) -> BlixardResult<(bool, String)> {
        let response = self.client.stop_vm(name).await?;
        Ok((response.success, response.message))
    }
    
    /// Delete a VM
    pub async fn delete_vm(&self, name: String) -> BlixardResult<(bool, String)> {
        let response = self.client.delete_vm(name).await?;
        Ok((response.success, response.message))
    }
    
    /// Get VM status
    pub async fn get_vm_status(&self, name: String) -> BlixardResult<Option<crate::types::VmInfo>> {
        let response = self.client.get_vm_status(name).await?;
        if response.found {
            Ok(response.vm_info)
        } else {
            Ok(None)
        }
    }
    
    /// List VMs
    pub async fn list_vms(&self) -> BlixardResult<Vec<crate::types::VmInfo>> {
        let response = self.client.list_vms().await?;
        Ok(response.vms)
    }
}