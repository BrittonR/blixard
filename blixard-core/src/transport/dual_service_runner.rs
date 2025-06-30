//! Dual service runner that serves both gRPC and Iroh simultaneously
//!
//! This module implements the infrastructure to run services over both
//! transports during the migration period.

use crate::{
    error::{BlixardError, BlixardResult},
    node_shared::SharedNodeState,
    proto::{
        cluster_service_server::ClusterServiceServer,
        blixard_service_server::BlixardServiceServer,
    },
    transport::{
        config::{TransportConfig, ServiceType, MigrationStrategy},
        services::{
            health::HealthServiceImpl,
            status::StatusServiceImpl,
            vm::VmServiceImpl,
        },
        iroh_service::IrohRpcServer,
        iroh_health_service::IrohHealthService,
        iroh_status_service::IrohStatusService,
        iroh_vm_service::IrohVmService,
    },
    config_global,
};
use std::{net::SocketAddr, sync::Arc};
use tonic::transport::Server;
use futures::future::try_join_all;

/// Service runner that can handle both gRPC and Iroh transports
pub struct DualServiceRunner {
    node: Arc<SharedNodeState>,
    transport_config: TransportConfig,
    grpc_addr: SocketAddr,
    iroh_endpoint: Option<iroh::Endpoint>,
}

impl DualServiceRunner {
    /// Create a new dual service runner
    pub fn new(
        node: Arc<SharedNodeState>,
        transport_config: TransportConfig,
        grpc_addr: SocketAddr,
        iroh_endpoint: Option<iroh::Endpoint>,
    ) -> Self {
        Self {
            node,
            transport_config,
            grpc_addr,
            iroh_endpoint,
        }
    }
    
    /// Start serving non-critical services based on transport configuration
    pub async fn serve_non_critical_services(self) -> BlixardResult<()> {
        match &self.transport_config {
            TransportConfig::Grpc(_) => {
                // Pure gRPC mode - serve all services via gRPC
                self.serve_grpc_only().await
            }
            TransportConfig::Iroh(_) => {
                // Pure Iroh mode - serve all services via Iroh
                self.serve_iroh_only().await
            }
            TransportConfig::Dual { strategy, .. } => {
                // Dual mode - serve based on migration strategy
                self.serve_dual_mode(strategy).await
            }
        }
    }
    
    /// Serve services in gRPC-only mode
    async fn serve_grpc_only(&self) -> BlixardResult<()> {
        // Create service implementations
        let health_service = HealthServiceImpl::new(self.node.clone());
        let status_service = StatusServiceImpl::new(self.node.clone());
        let vm_service = VmServiceImpl::new(self.node.clone());
        
        // Create a combined service that implements ClusterService
        let combined_service = CombinedNonCriticalService::new(
            health_service,
            status_service.clone(),
            vm_service,
        );
        
        // Build gRPC server
        let cluster_server = ClusterServiceServer::new(combined_service);
        let blixard_server = BlixardServiceServer::new(status_service);
        
        let mut server_builder = Server::builder();
        
        // TLS removed - not implemented yet
        tracing::warn!("TLS disabled for gRPC server - running in insecure mode");
        
        // Configure authentication if enabled
        if config_global::get().security.auth.enabled {
            // Create security manager
            // Security manager removed - not implemented yet
            
            // Create auth layer
            // Auth layer removed - authentication not implemented yet
            
            tracing::info!("Authentication enabled for gRPC server");
            
            server_builder
                .add_service(cluster_server)
                .add_service(blixard_server)
                .serve(self.grpc_addr)
                .await
                .map_err(|e| BlixardError::GrpcError(e.to_string()))?;
        } else {
            tracing::warn!("Authentication disabled for gRPC server - accepting all requests");
            
            server_builder
                .add_service(cluster_server)
                .add_service(blixard_server)
                .serve(self.grpc_addr)
                .await
                .map_err(|e| BlixardError::GrpcError(e.to_string()))?;
        }
            
        Ok(())
    }
    
    /// Serve services in Iroh-only mode
    async fn serve_iroh_only(&self) -> BlixardResult<()> {
        let endpoint = self.iroh_endpoint.as_ref()
            .ok_or_else(|| BlixardError::Internal {
                message: "Iroh endpoint not configured".to_string(),
            })?;
        
        // Create Iroh RPC server
        let server = Arc::new(IrohRpcServer::new(endpoint.clone()));
        
        // Register services
        let health_service = IrohHealthService::new(self.node.clone());
        server.register_service(health_service).await;
        
        let status_service = IrohStatusService::new(self.node.clone());
        server.register_service(status_service).await;
        
        let vm_service = IrohVmService::new(self.node.clone());
        server.register_service(vm_service).await;
        
        // Start serving
        server.serve().await
    }
    
    /// Serve services in dual mode based on migration strategy
    async fn serve_dual_mode(&self, strategy: &MigrationStrategy) -> BlixardResult<()> {
        let mut futures: Vec<std::pin::Pin<Box<dyn std::future::Future<Output = BlixardResult<()>> + Send>>> = Vec::new();
        
        // Always run gRPC server as fallback
        let grpc_future = self.serve_grpc_services_for_dual_mode(strategy);
        futures.push(Box::pin(grpc_future));
        
        // If Iroh is enabled and we're migrating some services
        if let Some(endpoint) = &self.iroh_endpoint {
            if !strategy.prefer_iroh_for.is_empty() {
                let iroh_future = self.serve_iroh_services_for_dual_mode(
                    endpoint.clone(),
                    strategy,
                );
                futures.push(Box::pin(iroh_future));
            }
        }
        
        // Run both transports concurrently
        try_join_all(futures).await?;
        Ok(())
    }
    
    /// Serve gRPC services in dual mode
    async fn serve_grpc_services_for_dual_mode(
        &self,
        strategy: &MigrationStrategy,
    ) -> BlixardResult<()> {
        // Create service implementations
        let health_service = HealthServiceImpl::new(self.node.clone());
        let status_service = StatusServiceImpl::new(self.node.clone());
        let vm_service = VmServiceImpl::new(self.node.clone());
        
        // Create a service filter based on migration strategy
        let filtered_service = MigrationFilteredService::new(
            health_service,
            status_service.clone(),
            vm_service,
            strategy.clone(),
        );
        
        // Build gRPC server
        let cluster_server = ClusterServiceServer::new(filtered_service);
        let blixard_server = BlixardServiceServer::new(status_service);
        
        // Configure TLS if enabled
        let mut server_builder = Server::builder();
        
        // TLS removed - not implemented yet
        tracing::warn!("TLS disabled for gRPC server - running in insecure mode");
        
        // Configure authentication if enabled
        if config_global::get().security.auth.enabled {
            // Create security manager
            // Security manager removed - not implemented yet
            
            // Create auth layer
            // Auth layer removed - authentication not implemented yet
            
            tracing::info!("Authentication enabled for gRPC server");
            
            server_builder
                .add_service(cluster_server)
                .add_service(blixard_server)
                .serve(self.grpc_addr)
                .await
                .map_err(|e| BlixardError::GrpcError(e.to_string()))?;
        } else {
            tracing::warn!("Authentication disabled for gRPC server - accepting all requests");
            
            server_builder
                .add_service(cluster_server)
                .add_service(blixard_server)
                .serve(self.grpc_addr)
                .await
                .map_err(|e| BlixardError::GrpcError(e.to_string()))?;
        }
            
        Ok(())
    }
    
    /// Serve Iroh services in dual mode
    async fn serve_iroh_services_for_dual_mode(
        &self,
        endpoint: iroh::Endpoint,
        strategy: &MigrationStrategy,
    ) -> BlixardResult<()> {
        // Create Iroh RPC server
        let server = Arc::new(IrohRpcServer::new(endpoint));
        
        // Register services that are migrated to Iroh
        if strategy.prefer_iroh_for.contains(&ServiceType::Health) {
            let health_service = IrohHealthService::new(self.node.clone());
            server.register_service(health_service).await;
        }
        
        if strategy.prefer_iroh_for.contains(&ServiceType::Status) {
            let status_service = IrohStatusService::new(self.node.clone());
            server.register_service(status_service).await;
        }
        
        if strategy.prefer_iroh_for.contains(&ServiceType::VmOps) {
            let vm_service = IrohVmService::new(self.node.clone());
            server.register_service(vm_service).await;
        }
        
        // Start serving
        server.serve().await
    }
}

/// Combined service that implements ClusterService for non-critical operations
#[derive(Clone)]
struct CombinedNonCriticalService {
    health_service: HealthServiceImpl,
    status_service: StatusServiceImpl,
    vm_service: VmServiceImpl,
}

impl CombinedNonCriticalService {
    fn new(
        health_service: HealthServiceImpl,
        status_service: StatusServiceImpl,
        vm_service: VmServiceImpl,
    ) -> Self {
        Self {
            health_service,
            status_service,
            vm_service,
        }
    }
}

// Implement ClusterService by delegating to appropriate sub-services
#[tonic::async_trait]
impl crate::proto::cluster_service_server::ClusterService for CombinedNonCriticalService {
    async fn health_check(
        &self,
        request: tonic::Request<crate::proto::HealthCheckRequest>,
    ) -> Result<tonic::Response<crate::proto::HealthCheckResponse>, tonic::Status> {
        self.health_service.health_check(request).await
    }
    
    async fn get_cluster_status(
        &self,
        request: tonic::Request<crate::proto::ClusterStatusRequest>,
    ) -> Result<tonic::Response<crate::proto::ClusterStatusResponse>, tonic::Status> {
        self.status_service.get_cluster_status(request).await
    }
    
    // All other methods return unimplemented
    async fn join_cluster(
        &self,
        _request: tonic::Request<crate::proto::JoinRequest>,
    ) -> Result<tonic::Response<crate::proto::JoinResponse>, tonic::Status> {
        Err(tonic::Status::unimplemented("Not a non-critical service"))
    }
    
    async fn leave_cluster(
        &self,
        _request: tonic::Request<crate::proto::LeaveRequest>,
    ) -> Result<tonic::Response<crate::proto::LeaveResponse>, tonic::Status> {
        Err(tonic::Status::unimplemented("Not a non-critical service"))
    }
    
    async fn send_raft_message(
        &self,
        _request: tonic::Request<crate::proto::RaftMessageRequest>,
    ) -> Result<tonic::Response<crate::proto::RaftMessageResponse>, tonic::Status> {
        Err(tonic::Status::unimplemented("Not a non-critical service"))
    }
    
    async fn submit_task(
        &self,
        _request: tonic::Request<crate::proto::TaskRequest>,
    ) -> Result<tonic::Response<crate::proto::TaskResponse>, tonic::Status> {
        Err(tonic::Status::unimplemented("Not a non-critical service"))
    }
    
    async fn get_task_status(
        &self,
        _request: tonic::Request<crate::proto::TaskStatusRequest>,
    ) -> Result<tonic::Response<crate::proto::TaskStatusResponse>, tonic::Status> {
        Err(tonic::Status::unimplemented("Not a non-critical service"))
    }
    
    async fn create_vm(
        &self,
        request: tonic::Request<crate::proto::CreateVmRequest>,
    ) -> Result<tonic::Response<crate::proto::CreateVmResponse>, tonic::Status> {
        self.vm_service.create_vm(request).await
    }
    
    async fn create_vm_with_scheduling(
        &self,
        _request: tonic::Request<crate::proto::CreateVmWithSchedulingRequest>,
    ) -> Result<tonic::Response<crate::proto::CreateVmWithSchedulingResponse>, tonic::Status> {
        Err(tonic::Status::unimplemented("Not a non-critical service"))
    }
    
    async fn start_vm(
        &self,
        request: tonic::Request<crate::proto::StartVmRequest>,
    ) -> Result<tonic::Response<crate::proto::StartVmResponse>, tonic::Status> {
        self.vm_service.start_vm(request).await
    }
    
    async fn stop_vm(
        &self,
        request: tonic::Request<crate::proto::StopVmRequest>,
    ) -> Result<tonic::Response<crate::proto::StopVmResponse>, tonic::Status> {
        self.vm_service.stop_vm(request).await
    }
    
    async fn delete_vm(
        &self,
        request: tonic::Request<crate::proto::DeleteVmRequest>,
    ) -> Result<tonic::Response<crate::proto::DeleteVmResponse>, tonic::Status> {
        self.vm_service.delete_vm(request).await
    }
    
    async fn list_vms(
        &self,
        request: tonic::Request<crate::proto::ListVmsRequest>,
    ) -> Result<tonic::Response<crate::proto::ListVmsResponse>, tonic::Status> {
        self.vm_service.list_vms(request).await
    }
    
    async fn get_vm_status(
        &self,
        request: tonic::Request<crate::proto::GetVmStatusRequest>,
    ) -> Result<tonic::Response<crate::proto::GetVmStatusResponse>, tonic::Status> {
        self.vm_service.get_vm_status(request).await
    }
    
    async fn migrate_vm(
        &self,
        request: tonic::Request<crate::proto::MigrateVmRequest>,
    ) -> Result<tonic::Response<crate::proto::MigrateVmResponse>, tonic::Status> {
        self.vm_service.migrate_vm(request).await
    }
    
    async fn schedule_vm_placement(
        &self,
        _request: tonic::Request<crate::proto::ScheduleVmPlacementRequest>,
    ) -> Result<tonic::Response<crate::proto::ScheduleVmPlacementResponse>, tonic::Status> {
        Err(tonic::Status::unimplemented("Not a non-critical service"))
    }
    
    async fn get_cluster_resource_summary(
        &self,
        _request: tonic::Request<crate::proto::ClusterResourceSummaryRequest>,
    ) -> Result<tonic::Response<crate::proto::ClusterResourceSummaryResponse>, tonic::Status> {
        Err(tonic::Status::unimplemented("Not a non-critical service"))
    }
    
    async fn get_p2p_status(
        &self,
        _request: tonic::Request<crate::proto::GetP2pStatusRequest>,
    ) -> Result<tonic::Response<crate::proto::GetP2pStatusResponse>, tonic::Status> {
        Err(tonic::Status::unimplemented("Not a non-critical service"))
    }
    
    async fn share_vm_image(
        &self,
        _request: tonic::Request<crate::proto::ShareVmImageRequest>,
    ) -> Result<tonic::Response<crate::proto::ShareVmImageResponse>, tonic::Status> {
        Err(tonic::Status::unimplemented("Not a non-critical service"))
    }
    
    async fn get_vm_image(
        &self,
        _request: tonic::Request<crate::proto::GetVmImageRequest>,
    ) -> Result<tonic::Response<crate::proto::GetVmImageResponse>, tonic::Status> {
        Err(tonic::Status::unimplemented("Not a non-critical service"))
    }
    
    async fn list_p2p_images(
        &self,
        _request: tonic::Request<crate::proto::ListP2pImagesRequest>,
    ) -> Result<tonic::Response<crate::proto::ListP2pImagesResponse>, tonic::Status> {
        Err(tonic::Status::unimplemented("Not a non-critical service"))
    }
}

/// Service that filters requests based on migration strategy
#[derive(Clone)]
struct MigrationFilteredService {
    combined: CombinedNonCriticalService,
    strategy: MigrationStrategy,
}

impl MigrationFilteredService {
    fn new(
        health_service: HealthServiceImpl,
        status_service: StatusServiceImpl,
        vm_service: VmServiceImpl,
        strategy: MigrationStrategy,
    ) -> Self {
        Self {
            combined: CombinedNonCriticalService::new(health_service, status_service, vm_service),
            strategy,
        }
    }
    
    /// Check if a service type should be handled by gRPC
    fn should_handle_grpc(&self, service_type: ServiceType) -> bool {
        !self.strategy.prefer_iroh_for.contains(&service_type)
    }
}

// Implement ClusterService by filtering based on migration strategy
#[tonic::async_trait]
impl crate::proto::cluster_service_server::ClusterService for MigrationFilteredService {
    async fn health_check(
        &self,
        request: tonic::Request<crate::proto::HealthCheckRequest>,
    ) -> Result<tonic::Response<crate::proto::HealthCheckResponse>, tonic::Status> {
        if self.should_handle_grpc(ServiceType::Health) {
            self.combined.health_check(request).await
        } else {
            Err(tonic::Status::unavailable("Service migrated to Iroh"))
        }
    }
    
    async fn get_cluster_status(
        &self,
        request: tonic::Request<crate::proto::ClusterStatusRequest>,
    ) -> Result<tonic::Response<crate::proto::ClusterStatusResponse>, tonic::Status> {
        if self.should_handle_grpc(ServiceType::Status) {
            self.combined.get_cluster_status(request).await
        } else {
            Err(tonic::Status::unavailable("Service migrated to Iroh"))
        }
    }
    
    // Forward all other methods to combined service
    async fn join_cluster(
        &self,
        request: tonic::Request<crate::proto::JoinRequest>,
    ) -> Result<tonic::Response<crate::proto::JoinResponse>, tonic::Status> {
        self.combined.join_cluster(request).await
    }
    
    async fn leave_cluster(
        &self,
        request: tonic::Request<crate::proto::LeaveRequest>,
    ) -> Result<tonic::Response<crate::proto::LeaveResponse>, tonic::Status> {
        self.combined.leave_cluster(request).await
    }
    
    async fn send_raft_message(
        &self,
        request: tonic::Request<crate::proto::RaftMessageRequest>,
    ) -> Result<tonic::Response<crate::proto::RaftMessageResponse>, tonic::Status> {
        self.combined.send_raft_message(request).await
    }
    
    async fn submit_task(
        &self,
        request: tonic::Request<crate::proto::TaskRequest>,
    ) -> Result<tonic::Response<crate::proto::TaskResponse>, tonic::Status> {
        self.combined.submit_task(request).await
    }
    
    async fn get_task_status(
        &self,
        request: tonic::Request<crate::proto::TaskStatusRequest>,
    ) -> Result<tonic::Response<crate::proto::TaskStatusResponse>, tonic::Status> {
        self.combined.get_task_status(request).await
    }
    
    async fn create_vm(
        &self,
        request: tonic::Request<crate::proto::CreateVmRequest>,
    ) -> Result<tonic::Response<crate::proto::CreateVmResponse>, tonic::Status> {
        if self.should_handle_grpc(ServiceType::VmOps) {
            self.combined.create_vm(request).await
        } else {
            Err(tonic::Status::unavailable("Service migrated to Iroh"))
        }
    }
    
    async fn create_vm_with_scheduling(
        &self,
        request: tonic::Request<crate::proto::CreateVmWithSchedulingRequest>,
    ) -> Result<tonic::Response<crate::proto::CreateVmWithSchedulingResponse>, tonic::Status> {
        self.combined.create_vm_with_scheduling(request).await
    }
    
    async fn start_vm(
        &self,
        request: tonic::Request<crate::proto::StartVmRequest>,
    ) -> Result<tonic::Response<crate::proto::StartVmResponse>, tonic::Status> {
        if self.should_handle_grpc(ServiceType::VmOps) {
            self.combined.start_vm(request).await
        } else {
            Err(tonic::Status::unavailable("Service migrated to Iroh"))
        }
    }
    
    async fn stop_vm(
        &self,
        request: tonic::Request<crate::proto::StopVmRequest>,
    ) -> Result<tonic::Response<crate::proto::StopVmResponse>, tonic::Status> {
        if self.should_handle_grpc(ServiceType::VmOps) {
            self.combined.stop_vm(request).await
        } else {
            Err(tonic::Status::unavailable("Service migrated to Iroh"))
        }
    }
    
    async fn delete_vm(
        &self,
        request: tonic::Request<crate::proto::DeleteVmRequest>,
    ) -> Result<tonic::Response<crate::proto::DeleteVmResponse>, tonic::Status> {
        if self.should_handle_grpc(ServiceType::VmOps) {
            self.combined.delete_vm(request).await
        } else {
            Err(tonic::Status::unavailable("Service migrated to Iroh"))
        }
    }
    
    async fn list_vms(
        &self,
        request: tonic::Request<crate::proto::ListVmsRequest>,
    ) -> Result<tonic::Response<crate::proto::ListVmsResponse>, tonic::Status> {
        if self.should_handle_grpc(ServiceType::VmOps) {
            self.combined.list_vms(request).await
        } else {
            Err(tonic::Status::unavailable("Service migrated to Iroh"))
        }
    }
    
    async fn get_vm_status(
        &self,
        request: tonic::Request<crate::proto::GetVmStatusRequest>,
    ) -> Result<tonic::Response<crate::proto::GetVmStatusResponse>, tonic::Status> {
        if self.should_handle_grpc(ServiceType::VmOps) {
            self.combined.get_vm_status(request).await
        } else {
            Err(tonic::Status::unavailable("Service migrated to Iroh"))
        }
    }
    
    async fn migrate_vm(
        &self,
        request: tonic::Request<crate::proto::MigrateVmRequest>,
    ) -> Result<tonic::Response<crate::proto::MigrateVmResponse>, tonic::Status> {
        if self.should_handle_grpc(ServiceType::VmOps) {
            self.combined.migrate_vm(request).await
        } else {
            Err(tonic::Status::unavailable("Service migrated to Iroh"))
        }
    }
    
    async fn schedule_vm_placement(
        &self,
        request: tonic::Request<crate::proto::ScheduleVmPlacementRequest>,
    ) -> Result<tonic::Response<crate::proto::ScheduleVmPlacementResponse>, tonic::Status> {
        self.combined.schedule_vm_placement(request).await
    }
    
    async fn get_cluster_resource_summary(
        &self,
        request: tonic::Request<crate::proto::ClusterResourceSummaryRequest>,
    ) -> Result<tonic::Response<crate::proto::ClusterResourceSummaryResponse>, tonic::Status> {
        self.combined.get_cluster_resource_summary(request).await
    }
    
    async fn get_p2p_status(
        &self,
        request: tonic::Request<crate::proto::GetP2pStatusRequest>,
    ) -> Result<tonic::Response<crate::proto::GetP2pStatusResponse>, tonic::Status> {
        self.combined.get_p2p_status(request).await
    }
    
    async fn share_vm_image(
        &self,
        request: tonic::Request<crate::proto::ShareVmImageRequest>,
    ) -> Result<tonic::Response<crate::proto::ShareVmImageResponse>, tonic::Status> {
        self.combined.share_vm_image(request).await
    }
    
    async fn get_vm_image(
        &self,
        request: tonic::Request<crate::proto::GetVmImageRequest>,
    ) -> Result<tonic::Response<crate::proto::GetVmImageResponse>, tonic::Status> {
        self.combined.get_vm_image(request).await
    }
    
    async fn list_p2p_images(
        &self,
        request: tonic::Request<crate::proto::ListP2pImagesRequest>,
    ) -> Result<tonic::Response<crate::proto::ListP2pImagesResponse>, tonic::Status> {
        self.combined.list_p2p_images(request).await
    }
}

/// Service filter for Iroh transport that only handles migrated services
#[derive(Clone)]
struct IrohMigrationFilteredService {
    combined: CombinedNonCriticalService,
    strategy: MigrationStrategy,
}

impl IrohMigrationFilteredService {
    fn new(
        health_service: HealthServiceImpl,
        status_service: StatusServiceImpl,
        vm_service: VmServiceImpl,
        strategy: MigrationStrategy,
    ) -> Self {
        Self {
            combined: CombinedNonCriticalService::new(health_service, status_service, vm_service),
            strategy,
        }
    }
    
    /// Check if a service type should be handled by Iroh
    fn should_handle_iroh(&self, service_type: ServiceType) -> bool {
        self.strategy.prefer_iroh_for.contains(&service_type)
    }
}

// Implement ClusterService for Iroh by only handling migrated services
#[tonic::async_trait]
impl crate::proto::cluster_service_server::ClusterService for IrohMigrationFilteredService {
    async fn health_check(
        &self,
        request: tonic::Request<crate::proto::HealthCheckRequest>,
    ) -> Result<tonic::Response<crate::proto::HealthCheckResponse>, tonic::Status> {
        if self.should_handle_iroh(ServiceType::Health) {
            self.combined.health_check(request).await
        } else {
            Err(tonic::Status::unavailable("Service not migrated to Iroh"))
        }
    }
    
    async fn get_cluster_status(
        &self,
        request: tonic::Request<crate::proto::ClusterStatusRequest>,
    ) -> Result<tonic::Response<crate::proto::ClusterStatusResponse>, tonic::Status> {
        if self.should_handle_iroh(ServiceType::Status) {
            self.combined.get_cluster_status(request).await
        } else {
            Err(tonic::Status::unavailable("Service not migrated to Iroh"))
        }
    }
    
    // All other methods return unavailable (not migrated to Iroh)
    async fn join_cluster(
        &self,
        _request: tonic::Request<crate::proto::JoinRequest>,
    ) -> Result<tonic::Response<crate::proto::JoinResponse>, tonic::Status> {
        Err(tonic::Status::unavailable("Service not migrated to Iroh"))
    }
    
    async fn leave_cluster(
        &self,
        _request: tonic::Request<crate::proto::LeaveRequest>,
    ) -> Result<tonic::Response<crate::proto::LeaveResponse>, tonic::Status> {
        Err(tonic::Status::unavailable("Service not migrated to Iroh"))
    }
    
    async fn send_raft_message(
        &self,
        _request: tonic::Request<crate::proto::RaftMessageRequest>,
    ) -> Result<tonic::Response<crate::proto::RaftMessageResponse>, tonic::Status> {
        Err(tonic::Status::unavailable("Service not migrated to Iroh"))
    }
    
    async fn submit_task(
        &self,
        _request: tonic::Request<crate::proto::TaskRequest>,
    ) -> Result<tonic::Response<crate::proto::TaskResponse>, tonic::Status> {
        Err(tonic::Status::unavailable("Service not migrated to Iroh"))
    }
    
    async fn get_task_status(
        &self,
        _request: tonic::Request<crate::proto::TaskStatusRequest>,
    ) -> Result<tonic::Response<crate::proto::TaskStatusResponse>, tonic::Status> {
        Err(tonic::Status::unavailable("Service not migrated to Iroh"))
    }
    
    async fn create_vm(
        &self,
        _request: tonic::Request<crate::proto::CreateVmRequest>,
    ) -> Result<tonic::Response<crate::proto::CreateVmResponse>, tonic::Status> {
        Err(tonic::Status::unavailable("Service not migrated to Iroh"))
    }
    
    async fn create_vm_with_scheduling(
        &self,
        _request: tonic::Request<crate::proto::CreateVmWithSchedulingRequest>,
    ) -> Result<tonic::Response<crate::proto::CreateVmWithSchedulingResponse>, tonic::Status> {
        Err(tonic::Status::unavailable("Service not migrated to Iroh"))
    }
    
    async fn start_vm(
        &self,
        _request: tonic::Request<crate::proto::StartVmRequest>,
    ) -> Result<tonic::Response<crate::proto::StartVmResponse>, tonic::Status> {
        Err(tonic::Status::unavailable("Service not migrated to Iroh"))
    }
    
    async fn stop_vm(
        &self,
        _request: tonic::Request<crate::proto::StopVmRequest>,
    ) -> Result<tonic::Response<crate::proto::StopVmResponse>, tonic::Status> {
        Err(tonic::Status::unavailable("Service not migrated to Iroh"))
    }
    
    async fn delete_vm(
        &self,
        _request: tonic::Request<crate::proto::DeleteVmRequest>,
    ) -> Result<tonic::Response<crate::proto::DeleteVmResponse>, tonic::Status> {
        Err(tonic::Status::unavailable("Service not migrated to Iroh"))
    }
    
    async fn list_vms(
        &self,
        _request: tonic::Request<crate::proto::ListVmsRequest>,
    ) -> Result<tonic::Response<crate::proto::ListVmsResponse>, tonic::Status> {
        Err(tonic::Status::unavailable("Service not migrated to Iroh"))
    }
    
    async fn get_vm_status(
        &self,
        _request: tonic::Request<crate::proto::GetVmStatusRequest>,
    ) -> Result<tonic::Response<crate::proto::GetVmStatusResponse>, tonic::Status> {
        Err(tonic::Status::unavailable("Service not migrated to Iroh"))
    }
    
    async fn migrate_vm(
        &self,
        _request: tonic::Request<crate::proto::MigrateVmRequest>,
    ) -> Result<tonic::Response<crate::proto::MigrateVmResponse>, tonic::Status> {
        Err(tonic::Status::unavailable("Service not migrated to Iroh"))
    }
    
    async fn schedule_vm_placement(
        &self,
        _request: tonic::Request<crate::proto::ScheduleVmPlacementRequest>,
    ) -> Result<tonic::Response<crate::proto::ScheduleVmPlacementResponse>, tonic::Status> {
        Err(tonic::Status::unavailable("Service not migrated to Iroh"))
    }
    
    async fn get_cluster_resource_summary(
        &self,
        _request: tonic::Request<crate::proto::ClusterResourceSummaryRequest>,
    ) -> Result<tonic::Response<crate::proto::ClusterResourceSummaryResponse>, tonic::Status> {
        Err(tonic::Status::unavailable("Service not migrated to Iroh"))
    }
    
    async fn get_p2p_status(
        &self,
        _request: tonic::Request<crate::proto::GetP2pStatusRequest>,
    ) -> Result<tonic::Response<crate::proto::GetP2pStatusResponse>, tonic::Status> {
        Err(tonic::Status::unavailable("Service not migrated to Iroh"))
    }
    
    async fn share_vm_image(
        &self,
        _request: tonic::Request<crate::proto::ShareVmImageRequest>,
    ) -> Result<tonic::Response<crate::proto::ShareVmImageResponse>, tonic::Status> {
        Err(tonic::Status::unavailable("Service not migrated to Iroh"))
    }
    
    async fn get_vm_image(
        &self,
        _request: tonic::Request<crate::proto::GetVmImageRequest>,
    ) -> Result<tonic::Response<crate::proto::GetVmImageResponse>, tonic::Status> {
        Err(tonic::Status::unavailable("Service not migrated to Iroh"))
    }
    
    async fn list_p2p_images(
        &self,
        _request: tonic::Request<crate::proto::ListP2pImagesRequest>,
    ) -> Result<tonic::Response<crate::proto::ListP2pImagesResponse>, tonic::Status> {
        Err(tonic::Status::unavailable("Service not migrated to Iroh"))
    }
}