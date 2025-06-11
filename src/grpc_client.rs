use crate::runtime_traits::Runtime;
use crate::types::{VmConfig, VmStatus};
use anyhow::Result;
use std::sync::Arc;
use tonic::transport::Channel;

// Include the generated proto code
pub use crate::grpc_server::blixard;

use blixard::{
    cluster_service_client::ClusterServiceClient,
    *,
};

/// gRPC client with runtime abstraction
pub struct GrpcClient<R: Runtime + 'static> {
    client: ClusterServiceClient<Channel>,
    runtime: Arc<R>,
}

impl<R: Runtime + 'static> GrpcClient<R> {
    pub async fn connect(addr: String, runtime: Arc<R>) -> Result<Self> {
        let client = ClusterServiceClient::connect(addr).await?;
        Ok(Self { client, runtime })
    }

    pub async fn health_check(&mut self) -> Result<bool> {
        let request = tonic::Request::new(HealthCheckRequest {});
        let response = self.client.health_check(request).await?;
        Ok(response.into_inner().healthy)
    }

    pub async fn create_vm(&mut self, config: VmConfig) -> Result<()> {
        let request = tonic::Request::new(CreateVmRequest {
            name: config.name,
            config_path: config.config_path,
            vcpus: config.vcpus,
            memory_mb: config.memory,
        });

        let response = self.client.create_vm(request).await?;
        let resp = response.into_inner();
        
        if resp.success {
            Ok(())
        } else {
            Err(anyhow::anyhow!("Failed to create VM: {}", resp.message))
        }
    }

    pub async fn start_vm(&mut self, name: String) -> Result<()> {
        let request = tonic::Request::new(StartVmRequest { name });
        let response = self.client.start_vm(request).await?;
        let resp = response.into_inner();
        
        if resp.success {
            Ok(())
        } else {
            Err(anyhow::anyhow!("Failed to start VM: {}", resp.message))
        }
    }

    pub async fn stop_vm(&mut self, name: String) -> Result<()> {
        let request = tonic::Request::new(StopVmRequest { name });
        let response = self.client.stop_vm(request).await?;
        let resp = response.into_inner();
        
        if resp.success {
            Ok(())
        } else {
            Err(anyhow::anyhow!("Failed to stop VM: {}", resp.message))
        }
    }

    pub async fn list_vms(&mut self) -> Result<Vec<(String, VmStatus)>> {
        let request = tonic::Request::new(ListVmsRequest {});
        let response = self.client.list_vms(request).await?;
        let resp = response.into_inner();
        
        let vms = resp.vms.into_iter().map(|vm| {
            let status = match vm.state() {
                VmState::Unknown | VmState::Created => VmStatus::Creating,
                VmState::Starting => VmStatus::Starting,
                VmState::Running => VmStatus::Running,
                VmState::Stopping => VmStatus::Stopping,
                VmState::Stopped => VmStatus::Stopped,
                VmState::Failed => VmStatus::Failed,
            };
            (vm.name, status)
        }).collect();
        
        Ok(vms)
    }

    pub async fn get_cluster_status(&mut self) -> Result<ClusterStatusResponse> {
        let request = tonic::Request::new(ClusterStatusRequest {});
        let response = self.client.get_cluster_status(request).await?;
        Ok(response.into_inner())
    }
}