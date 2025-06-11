use crate::node::Node;
use crate::runtime_traits::Runtime;
use crate::types::VmConfig;
use anyhow::Result;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::RwLock;
use tonic::{transport::Server, Request, Response, Status};

// Include the generated proto code
pub mod blixard {
    tonic::include_proto!("blixard");
}

use blixard::{
    cluster_service_server::{ClusterService, ClusterServiceServer},
    *,
};

/// gRPC server implementation with runtime abstraction
pub struct GrpcServer<R: Runtime + 'static> {
    node: Arc<RwLock<Node>>,
    runtime: Arc<R>,
}

impl<R: Runtime + 'static> GrpcServer<R> {
    pub fn new(node: Arc<RwLock<Node>>, runtime: Arc<R>) -> Self {
        Self { node, runtime }
    }

    pub async fn serve(self, addr: SocketAddr) -> Result<()> {
        let service = ClusterServiceServer::new(self);

        Server::builder()
            .add_service(service)
            .serve(addr)
            .await?;

        Ok(())
    }
}

#[tonic::async_trait]
impl<R: Runtime + 'static> ClusterService for GrpcServer<R> {
    async fn join_cluster(
        &self,
        request: Request<JoinRequest>,
    ) -> Result<Response<JoinResponse>, Status> {
        let req = request.into_inner();
        
        // TODO: Implement actual join logic
        let response = JoinResponse {
            success: true,
            message: format!("Node {} joined cluster", req.node_id),
            peers: vec![],
        };

        Ok(Response::new(response))
    }

    async fn leave_cluster(
        &self,
        request: Request<LeaveRequest>,
    ) -> Result<Response<LeaveResponse>, Status> {
        let req = request.into_inner();
        
        let response = LeaveResponse {
            success: true,
            message: format!("Node {} left cluster", req.node_id),
        };

        Ok(Response::new(response))
    }

    async fn get_cluster_status(
        &self,
        _request: Request<ClusterStatusRequest>,
    ) -> Result<Response<ClusterStatusResponse>, Status> {
        let response = ClusterStatusResponse {
            leader_id: 1, // TODO: Get actual leader
            nodes: vec![],
            term: 0,
        };

        Ok(Response::new(response))
    }

    async fn create_vm(
        &self,
        request: Request<CreateVmRequest>,
    ) -> Result<Response<CreateVmResponse>, Status> {
        let req = request.into_inner();
        
        let node = self.node.read().await;
        
        let config = VmConfig {
            name: req.name.clone(),
            config_path: req.config_path,
            vcpus: req.vcpus,
            memory: req.memory_mb,
        };

        match node.create_vm(config).await {
            Ok(()) => {
                let response = CreateVmResponse {
                    success: true,
                    message: "VM created successfully".to_string(),
                    vm_id: req.name,
                };
                Ok(Response::new(response))
            }
            Err(e) => {
                let response = CreateVmResponse {
                    success: false,
                    message: e.to_string(),
                    vm_id: String::new(),
                };
                Ok(Response::new(response))
            }
        }
    }

    async fn start_vm(
        &self,
        request: Request<StartVmRequest>,
    ) -> Result<Response<StartVmResponse>, Status> {
        let req = request.into_inner();
        let node = self.node.read().await;

        match node.start_vm(req.name).await {
            Ok(()) => {
                let response = StartVmResponse {
                    success: true,
                    message: "VM started successfully".to_string(),
                };
                Ok(Response::new(response))
            }
            Err(e) => {
                let response = StartVmResponse {
                    success: false,
                    message: e.to_string(),
                };
                Ok(Response::new(response))
            }
        }
    }

    async fn stop_vm(
        &self,
        request: Request<StopVmRequest>,
    ) -> Result<Response<StopVmResponse>, Status> {
        let req = request.into_inner();
        let node = self.node.read().await;

        match node.stop_vm(req.name).await {
            Ok(()) => {
                let response = StopVmResponse {
                    success: true,
                    message: "VM stopped successfully".to_string(),
                };
                Ok(Response::new(response))
            }
            Err(e) => {
                let response = StopVmResponse {
                    success: false,
                    message: e.to_string(),
                };
                Ok(Response::new(response))
            }
        }
    }

    async fn list_vms(
        &self,
        _request: Request<ListVmsRequest>,
    ) -> Result<Response<ListVmsResponse>, Status> {
        // TODO: Implement actual VM listing
        let response = ListVmsResponse { vms: vec![] };
        Ok(Response::new(response))
    }

    async fn get_vm_status(
        &self,
        request: Request<GetVmStatusRequest>,
    ) -> Result<Response<GetVmStatusResponse>, Status> {
        let req = request.into_inner();
        
        // TODO: Implement actual status retrieval
        let response = GetVmStatusResponse {
            found: false,
            vm_info: None,
        };
        
        Ok(Response::new(response))
    }

    async fn health_check(
        &self,
        _request: Request<HealthCheckRequest>,
    ) -> Result<Response<HealthCheckResponse>, Status> {
        let response = HealthCheckResponse {
            healthy: true,
            message: "Node is healthy".to_string(),
        };
        
        Ok(Response::new(response))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime_traits::RealRuntime;
    use crate::storage::Storage;
    use crate::types::NodeConfig;
    use std::collections::HashMap;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_grpc_health_check() {
        let temp_dir = TempDir::new().unwrap();
        let config = NodeConfig {
            id: 1,
            data_dir: temp_dir.path().to_str().unwrap().to_string(),
            bind_addr: "127.0.0.1:0".parse().unwrap(),
            join_addr: None,
            use_tailscale: false,
        };

        let storage = Arc::new(Storage::new(temp_dir.path().join("test.db")).unwrap());
        let runtime = Arc::new(RealRuntime::new());
        let node = Node::new_with_storage(config, HashMap::new(), storage)
            .await
            .unwrap();
        let node = Arc::new(RwLock::new(node));

        let server = GrpcServer::new(node, runtime);
        
        let request = Request::new(HealthCheckRequest {});
        let response = server.health_check(request).await.unwrap();
        
        assert!(response.into_inner().healthy);
    }
}