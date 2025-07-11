//! Iroh cluster service implementation
//!
//! This module implements cluster management operations over Iroh transport.

use crate::error::{BlixardError, BlixardResult};
use crate::iroh_types::NodeInfo;
use crate::transport::iroh_protocol::{
    deserialize_payload, generate_request_id, read_message, serialize_payload, write_message,
    MessageType, RpcRequest, RpcResponse,
};
use crate::transport::iroh_service::IrohService;
use async_trait::async_trait;
use bytes::Bytes;
use iroh::endpoint::{RecvStream, SendStream};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Cluster service message types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ClusterRequest {
    JoinCluster(JoinClusterRequest),
    LeaveCluster(LeaveClusterRequest),
    GetClusterStatus(ClusterStatusRequest),
    SubmitTask(TaskRequest),
    GetTaskStatus(TaskStatusRequest),
    ProposeTask(ProposeTaskRequest),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ClusterResponse {
    JoinCluster(JoinClusterResponse),
    LeaveCluster(LeaveClusterResponse),
    GetClusterStatus(ClusterStatusResponse),
    SubmitTask(TaskResponse),
    GetTaskStatus(TaskStatusResponse),
    ProposeTask(ProposeTaskResponse),
}

// Request/Response types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JoinClusterRequest {
    pub node_id: u64,
    pub bind_address: String,
    pub p2p_node_id: Option<String>,
    pub p2p_addresses: Vec<String>,
    pub p2p_relay_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JoinClusterResponse {
    pub success: bool,
    pub message: String,
    pub peers: Vec<NodeInfo>,
    pub voters: Vec<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeaveClusterRequest {
    pub node_id: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeaveClusterResponse {
    pub success: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterStatusRequest {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterStatusResponse {
    pub leader_id: u64,
    pub nodes: Vec<NodeInfo>,
    pub term: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRequest {
    pub task_id: String,
    pub command: String,
    pub args: Vec<String>,
    pub cpu_cores: u32,
    pub memory_mb: u64,
    pub disk_gb: u64,
    pub required_features: Vec<String>,
    pub timeout_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskResponse {
    pub accepted: bool,
    pub message: String,
    pub assigned_node: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskStatusRequest {
    pub task_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskStatusResponse {
    pub found: bool,
    pub status: TaskStatus,
    pub output: String,
    pub error: String,
    pub execution_time_ms: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum TaskStatus {
    Unknown,
    Pending,
    Running,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposeTaskRequest {
    pub task: Task,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposeTaskResponse {
    pub success: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub command: String,
    pub args: Vec<String>,
    pub cpu_cores: u32,
    pub memory_mb: u64,
}

/// Trait for cluster operations
#[async_trait]
pub trait ClusterOperations: Send + Sync {
    async fn join_cluster(
        &self,
        node_id: u64,
        bind_address: String,
        p2p_node_id: Option<String>,
        p2p_addresses: Vec<String>,
        p2p_relay_url: Option<String>,
    ) -> BlixardResult<(bool, String, Vec<NodeInfo>, Vec<u64>)>;
    async fn leave_cluster(&self, node_id: u64) -> BlixardResult<(bool, String)>;
    async fn get_cluster_status(&self) -> BlixardResult<(u64, Vec<NodeInfo>, u64)>;
    async fn submit_task(&self, task: TaskRequest) -> BlixardResult<(bool, String, u64)>;
    async fn get_task_status(
        &self,
        task_id: String,
    ) -> BlixardResult<Option<(TaskStatus, String, String, u64)>>;
    async fn propose_task(&self, task: Task) -> BlixardResult<(bool, String)>;
}

/// Iroh cluster service implementation
pub struct IrohClusterService {
    operations: Arc<dyn ClusterOperations>,
}

impl IrohClusterService {
    pub fn new(operations: Arc<dyn ClusterOperations>) -> Self {
        Self { operations }
    }

    async fn handle_request(&self, request: ClusterRequest) -> BlixardResult<ClusterResponse> {
        match request {
            ClusterRequest::JoinCluster(req) => {
                let (success, message, peers, voters) = self
                    .operations
                    .join_cluster(
                        req.node_id,
                        req.bind_address,
                        req.p2p_node_id,
                        req.p2p_addresses,
                        req.p2p_relay_url,
                    )
                    .await?;
                Ok(ClusterResponse::JoinCluster(JoinClusterResponse {
                    success,
                    message,
                    peers,
                    voters,
                }))
            }
            ClusterRequest::LeaveCluster(req) => {
                let (success, message) = self.operations.leave_cluster(req.node_id).await?;
                Ok(ClusterResponse::LeaveCluster(LeaveClusterResponse {
                    success,
                    message,
                }))
            }
            ClusterRequest::GetClusterStatus(_) => {
                let (leader_id, nodes, term) = self.operations.get_cluster_status().await?;
                Ok(ClusterResponse::GetClusterStatus(ClusterStatusResponse {
                    leader_id,
                    nodes,
                    term,
                }))
            }
            ClusterRequest::SubmitTask(req) => {
                let (accepted, message, assigned_node) = self.operations.submit_task(req).await?;
                Ok(ClusterResponse::SubmitTask(TaskResponse {
                    accepted,
                    message,
                    assigned_node,
                }))
            }
            ClusterRequest::GetTaskStatus(req) => {
                let result = self.operations.get_task_status(req.task_id).await?;

                let (found, status, output, error, execution_time_ms) =
                    if let Some((status, output, error, time)) = result {
                        (true, status, output, error, time)
                    } else {
                        (false, TaskStatus::Unknown, String::new(), String::new(), 0)
                    };

                Ok(ClusterResponse::GetTaskStatus(TaskStatusResponse {
                    found,
                    status,
                    output,
                    error,
                    execution_time_ms,
                }))
            }
            ClusterRequest::ProposeTask(req) => {
                let (success, message) = self.operations.propose_task(req.task).await?;
                Ok(ClusterResponse::ProposeTask(ProposeTaskResponse {
                    success,
                    message,
                }))
            }
        }
    }
}

#[async_trait]
impl IrohService for IrohClusterService {
    fn name(&self) -> &'static str {
        "cluster"
    }

    async fn handle_call(&self, method: &str, payload: Bytes) -> BlixardResult<Bytes> {
        match method {
            "join_cluster" => {
                let request: ClusterRequest = deserialize_payload(&payload)?;

                let response = match request {
                    ClusterRequest::JoinCluster(req) => {
                        let (success, message, peers, voters) = self
                            .operations
                            .join_cluster(
                                req.node_id,
                                req.bind_address,
                                req.p2p_node_id,
                                req.p2p_addresses,
                                req.p2p_relay_url,
                            )
                            .await?;
                        ClusterResponse::JoinCluster(JoinClusterResponse {
                            success,
                            message,
                            peers,
                            voters,
                        })
                    }
                    _ => {
                        return Err(BlixardError::Internal {
                            message: "Unexpected request type for join_cluster".to_string(),
                        })
                    }
                };

                serialize_payload(&response)
            }
            "leave_cluster" => {
                let request: ClusterRequest = deserialize_payload(&payload)?;

                let response = match request {
                    ClusterRequest::LeaveCluster(req) => {
                        let (success, message) = self.operations.leave_cluster(req.node_id).await?;
                        ClusterResponse::LeaveCluster(LeaveClusterResponse { success, message })
                    }
                    _ => {
                        return Err(BlixardError::Internal {
                            message: "Unexpected request type for leave_cluster".to_string(),
                        })
                    }
                };

                serialize_payload(&response)
            }
            "get_cluster_status" => {
                let response = {
                    let (leader_id, nodes, term) = self.operations.get_cluster_status().await?;
                    ClusterResponse::GetClusterStatus(ClusterStatusResponse {
                        leader_id,
                        nodes,
                        term,
                    })
                };

                serialize_payload(&response)
            }
            _ => Err(BlixardError::NotImplemented {
                feature: format!("Cluster service method: {}", method),
            }),
        }
    }
}

/// Client for cluster service
pub struct IrohClusterClient;

impl IrohClusterClient {
    pub async fn join_cluster(
        send: &mut SendStream,
        recv: &mut RecvStream,
        node_id: u64,
        bind_address: String,
        p2p_node_id: Option<String>,
        p2p_addresses: Vec<String>,
        p2p_relay_url: Option<String>,
    ) -> BlixardResult<JoinClusterResponse> {
        let request = ClusterRequest::JoinCluster(JoinClusterRequest {
            node_id,
            bind_address,
            p2p_node_id,
            p2p_addresses,
            p2p_relay_url,
        });

        let rpc_request = RpcRequest {
            service: "cluster".to_string(),
            method: "join_cluster".to_string(),
            payload: serialize_payload(&request)?,
        };

        let request_id = generate_request_id();
        let payload = serialize_payload(&rpc_request)?;

        write_message(send, MessageType::Request, request_id, &payload).await?;

        let (header, response_bytes) = read_message(recv).await?;
        if header.msg_type != MessageType::Response {
            return Err(BlixardError::Internal {
                message: "Expected response message".to_string(),
            });
        }

        let rpc_response: RpcResponse = deserialize_payload(&response_bytes)?;
        if !rpc_response.success {
            return Err(BlixardError::Internal {
                message: rpc_response
                    .error
                    .unwrap_or_else(|| "Unknown error".to_string()),
            });
        }

        let response: ClusterResponse =
            deserialize_payload(&rpc_response.payload.ok_or_else(|| BlixardError::Internal {
                message: "Missing response payload".to_string(),
            })?)?;

        match response {
            ClusterResponse::JoinCluster(resp) => Ok(resp),
            _ => Err(BlixardError::Internal {
                message: "Unexpected response type".to_string(),
            }),
        }
    }

    pub async fn get_cluster_status(
        send: &mut SendStream,
        recv: &mut RecvStream,
    ) -> BlixardResult<ClusterStatusResponse> {
        let request = ClusterRequest::GetClusterStatus(ClusterStatusRequest {});

        let rpc_request = RpcRequest {
            service: "cluster".to_string(),
            method: "get_cluster_status".to_string(),
            payload: serialize_payload(&request)?,
        };

        let request_id = generate_request_id();
        let payload = serialize_payload(&rpc_request)?;

        write_message(send, MessageType::Request, request_id, &payload).await?;

        let (header, response_bytes) = read_message(recv).await?;
        if header.msg_type != MessageType::Response {
            return Err(BlixardError::Internal {
                message: "Expected response message".to_string(),
            });
        }

        let rpc_response: RpcResponse = deserialize_payload(&response_bytes)?;
        if !rpc_response.success {
            return Err(BlixardError::Internal {
                message: rpc_response
                    .error
                    .unwrap_or_else(|| "Unknown error".to_string()),
            });
        }

        let response: ClusterResponse =
            deserialize_payload(&rpc_response.payload.ok_or_else(|| BlixardError::Internal {
                message: "Missing response payload".to_string(),
            })?)?;

        match response {
            ClusterResponse::GetClusterStatus(resp) => Ok(resp),
            _ => Err(BlixardError::Internal {
                message: "Unexpected response type".to_string(),
            }),
        }
    }
}
