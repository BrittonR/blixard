//! Iroh client implementation for making RPC calls
//!
//! This module provides client-side functionality for calling Iroh services.

use crate::error::{BlixardError, BlixardResult};
use crate::iroh_types::Response;
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
use crate::transport::iroh_vm_service::{VmRequest, VmResponse};
use crate::transport::services::vm::{VmOperationRequest, VmOperationResponse};
use bytes::Bytes;
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, error};

/// Client for making Iroh RPC calls
#[derive(Clone)]
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
    
    pub async fn join_cluster(&self, node_id: u64, bind_address: String, p2p_node_id: Option<String>, p2p_addresses: Vec<String>, p2p_relay_url: Option<String>) -> BlixardResult<JoinClusterResponse> {
        let request = ClusterRequest::JoinCluster(JoinClusterRequest {
            node_id,
            bind_address,
            p2p_node_id,
            p2p_addresses,
            p2p_relay_url,
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
    
    pub async fn create_vm(&self, config: crate::iroh_types::VmConfig) -> BlixardResult<Response<crate::iroh_types::CreateVmResponse>> {
        let request = VmRequest::Operation(
            crate::transport::services::vm::VmOperationRequest::Create {
                name: config.name,
                config_path: String::new(), // Not used in current implementation
                vcpus: config.cpu_cores,
                memory_mb: config.memory_mb,
            }
        );
        
        let response: VmResponse = self.call_service("vm", "create", request).await?;
        
        match response {
            VmResponse::Operation(crate::transport::services::vm::VmOperationResponse::Create { success, message, vm_id }) => {
                Ok(Response::new(crate::iroh_types::CreateVmResponse { success, message, vm_id }))
            }
            _ => Err(BlixardError::Internal {
                message: "Unexpected response type".to_string(),
            }),
        }
    }
    
    pub async fn start_vm(&self, request: crate::iroh_types::StartVmRequest) -> BlixardResult<Response<crate::iroh_types::StartVmResponse>> {
        let vm_request = VmRequest::Operation(
            crate::transport::services::vm::VmOperationRequest::Start { name: request.name }
        );
        
        let response: VmResponse = self.call_service("vm", "start", vm_request).await?;
        
        match response {
            VmResponse::Operation(crate::transport::services::vm::VmOperationResponse::Start { success, message }) => {
                Ok(Response::new(crate::iroh_types::StartVmResponse { success, message }))
            }
            _ => Err(BlixardError::Internal {
                message: "Unexpected response type".to_string(),
            }),
        }
    }
    
    pub async fn stop_vm(&self, request: crate::iroh_types::StopVmRequest) -> BlixardResult<Response<crate::iroh_types::StopVmResponse>> {
        let vm_request = VmRequest::Operation(
            crate::transport::services::vm::VmOperationRequest::Stop { name: request.name }
        );
        
        let response: VmResponse = self.call_service("vm", "stop", vm_request).await?;
        
        match response {
            VmResponse::Operation(crate::transport::services::vm::VmOperationResponse::Stop { success, message }) => {
                Ok(Response::new(crate::iroh_types::StopVmResponse { success, message }))
            }
            _ => Err(BlixardError::Internal {
                message: "Unexpected response type".to_string(),
            }),
        }
    }
    
    pub async fn delete_vm(&self, request: crate::iroh_types::DeleteVmRequest) -> BlixardResult<Response<crate::iroh_types::DeleteVmResponse>> {
        let vm_request = VmRequest::Operation(
            crate::transport::services::vm::VmOperationRequest::Delete { name: request.name }
        );
        
        let response: VmResponse = self.call_service("vm", "delete", vm_request).await?;
        
        match response {
            VmResponse::Operation(crate::transport::services::vm::VmOperationResponse::Delete { success, message }) => {
                Ok(Response::new(crate::iroh_types::DeleteVmResponse { success, message }))
            }
            _ => Err(BlixardError::Internal {
                message: "Unexpected response type".to_string(),
            }),
        }
    }
    
    pub async fn get_vm_status(&self, name: String) -> BlixardResult<Option<crate::iroh_types::VmInfo>> {
        let request = VmRequest::Operation(
            crate::transport::services::vm::VmOperationRequest::GetStatus { name }
        );
        
        let response: VmResponse = self.call_service("vm", "get_status", request).await?;
        
        match response {
            VmResponse::Operation(crate::transport::services::vm::VmOperationResponse::GetStatus { found, vm_info }) => {
                let vm_info = vm_info.map(|data| crate::iroh_types::VmInfo {
                    name: data.name,
                    state: match data.state.as_str() {
                        "Creating" => 1,
                        "Starting" => 2,
                        "Running" => 3,
                        "Stopping" => 4,
                        "Stopped" => 5,
                        "Failed" => 6,
                        _ => 0,
                    },
                    node_id: data.node_id,
                    vcpus: data.vcpus,
                    memory_mb: data.memory_mb,
                    ip_address: data.ip_address,
                });
                Ok(vm_info)
            }
            _ => Err(BlixardError::Internal {
                message: "Unexpected response type".to_string(),
            }),
        }
    }
    
    pub async fn list_vms(&self) -> BlixardResult<Vec<crate::iroh_types::VmInfo>> {
        let request = VmRequest::Operation(
            crate::transport::services::vm::VmOperationRequest::List
        );
        
        let response: VmResponse = self.call_service("vm", "list", request).await?;
        
        match response {
            VmResponse::Operation(crate::transport::services::vm::VmOperationResponse::List { vms }) => {
                let vms = vms.into_iter().map(|data| crate::iroh_types::VmInfo {
                    name: data.name,
                    state: match data.state.as_str() {
                        "Creating" => 1,
                        "Starting" => 2,
                        "Running" => 3,
                        "Stopping" => 4,
                        "Stopped" => 5,
                        "Failed" => 6,
                        _ => 0,
                    },
                    node_id: data.node_id,
                    vcpus: data.vcpus,
                    memory_mb: data.memory_mb,
                    ip_address: data.ip_address,
                }).collect();
                Ok(vms)
            }
            _ => Err(BlixardError::Internal {
                message: "Unexpected response type".to_string(),
            }),
        }
    }
}

/// Wrapper client that provides a gRPC-like interface but uses Iroh underneath
#[derive(Clone)]
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
    pub async fn join_cluster(&self, node_id: u64, bind_address: String, p2p_node_id: Option<String>, p2p_addresses: Vec<String>, p2p_relay_url: Option<String>) -> BlixardResult<(bool, String, Vec<crate::iroh_types::NodeInfo>, Vec<u64>)> {
        let response = self.client.join_cluster(node_id, bind_address, p2p_node_id, p2p_addresses, p2p_relay_url).await?;
        Ok((response.success, response.message, response.peers, response.voters))
    }
    
    /// Leave a cluster
    pub async fn leave_cluster(&self, request: crate::iroh_types::LeaveRequest) -> BlixardResult<Response<crate::iroh_types::LeaveResponse>> {
        let inner_response = self.client.leave_cluster(request.node_id).await?;
        let response = crate::iroh_types::LeaveResponse {
            success: inner_response.success,
            message: inner_response.message,
        };
        Ok(Response::new(response))
    }
    
    /// Get cluster status
    pub async fn get_cluster_status(&self) -> BlixardResult<(u64, Vec<crate::iroh_types::NodeInfo>, u64)> {
        let response = self.client.get_cluster_status().await?;
        Ok((response.leader_id, response.nodes, response.term))
    }
    
    // VM operations wrappers
    
    /// Create a VM
    pub async fn create_vm(&self, config: crate::iroh_types::VmConfig) -> BlixardResult<Response<crate::iroh_types::CreateVmResponse>> {
        let response = self.client.create_vm(
            config.name,
            String::new(), // config_path not used in VmConfig
            config.cpu_cores,
            config.memory_mb,
        ).await?;
        Ok(Response::new(response))
    }
    
    /// Start a VM
    pub async fn start_vm(&self, request: crate::iroh_types::StartVmRequest) -> BlixardResult<Response<crate::iroh_types::StartVmResponse>> {
        let response = self.client.start_vm(request.name).await?;
        Ok(Response::new(response))
    }
    
    /// Stop a VM
    pub async fn stop_vm(&self, request: crate::iroh_types::StopVmRequest) -> BlixardResult<Response<crate::iroh_types::StopVmResponse>> {
        let response = self.client.stop_vm(request.name).await?;
        Ok(Response::new(response))
    }
    
    /// Delete a VM
    pub async fn delete_vm(&self, request: crate::iroh_types::DeleteVmRequest) -> BlixardResult<Response<crate::iroh_types::DeleteVmResponse>> {
        let response = self.client.delete_vm(request.name).await?;
        Ok(Response::new(response))
    }
    
    /// Get VM status
    pub async fn get_vm_status(&self, name: String) -> BlixardResult<Option<crate::iroh_types::VmInfo>> {
        let response = self.client.get_vm_status(name).await?;
        if response.found {
            Ok(response.vm_info)
        } else {
            Ok(None)
        }
    }
    
    /// List VMs
    pub async fn list_vms(&self) -> BlixardResult<Vec<crate::iroh_types::VmInfo>> {
        let response = self.client.list_vms().await?;
        Ok(response.vms)
    }
    
    // VM Health Monitoring operations
    
    /// Get VM health status
    pub async fn get_vm_health_status(&self, vm_name: String) -> BlixardResult<crate::iroh_types::GetVmHealthStatusResponse> {
        let request = crate::iroh_types::GetVmHealthStatusRequest { vm_name };
        self.client.call_service("vm", "get_health_status", request).await
    }
    
    /// Add a health check to a VM
    pub async fn add_vm_health_check(&self, vm_name: String, health_check: crate::vm_health_types::HealthCheck) -> BlixardResult<crate::iroh_types::AddVmHealthCheckResponse> {
        let request = crate::iroh_types::AddVmHealthCheckRequest { vm_name, health_check };
        self.client.call_service("vm", "add_health_check", request).await
    }
    
    /// List health checks for a VM
    pub async fn list_vm_health_checks(&self, vm_name: String) -> BlixardResult<crate::iroh_types::ListVmHealthChecksResponse> {
        let request = crate::iroh_types::ListVmHealthChecksRequest { vm_name };
        self.client.call_service("vm", "list_health_checks", request).await
    }
    
    /// Remove a health check from a VM
    pub async fn remove_vm_health_check(&self, vm_name: String, check_name: String) -> BlixardResult<crate::iroh_types::RemoveVmHealthCheckResponse> {
        let request = crate::iroh_types::RemoveVmHealthCheckRequest { vm_name, check_name };
        self.client.call_service("vm", "remove_health_check", request).await
    }
    
    /// Toggle health monitoring for a VM
    pub async fn toggle_vm_health_monitoring(&self, vm_name: String, enable: bool) -> BlixardResult<crate::iroh_types::ToggleVmHealthMonitoringResponse> {
        let request = crate::iroh_types::ToggleVmHealthMonitoringRequest { vm_name, enable };
        self.client.call_service("vm", "toggle_health_monitoring", request).await
    }
    
    /// Configure recovery policy for a VM
    pub async fn configure_vm_recovery_policy(&self, vm_name: String, policy: crate::vm_auto_recovery::RecoveryPolicy) -> BlixardResult<crate::iroh_types::ConfigureVmRecoveryPolicyResponse> {
        let request = crate::iroh_types::ConfigureVmRecoveryPolicyRequest { vm_name, policy };
        self.client.call_service("vm", "configure_recovery_policy", request).await
    }
    
    /// Health check
    pub async fn health_check(&self, _request: crate::iroh_types::HealthCheckRequest) -> BlixardResult<Response<crate::iroh_types::HealthCheckResponse>> {
        // For now, just return a simple healthy response
        // In a real implementation, this would check actual service health
        let response = crate::iroh_types::HealthCheckResponse {
            healthy: true,
            message: "Service is healthy".to_string(),
        };
        Ok(Response::new(response))
    }
    
    /// Schedule VM placement
    pub async fn schedule_vm_placement(&self, request: crate::iroh_types::TaskSchedulingRequest) -> BlixardResult<Response<crate::iroh_types::TaskSchedulingResponse>> {
        // For now, just return a simple success response
        // In a real implementation, this would call the actual scheduler
        let response = crate::iroh_types::TaskSchedulingResponse {
            success: true,
            message: "VM scheduled successfully".to_string(),
            worker_id: 1, // Default to worker 1 for testing
            placement_score: 100.0,
        };
        Ok(Response::new(response))
    }
}