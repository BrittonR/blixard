//! Service factory for creating transport-agnostic services

use crate::error::{BlixardError, BlixardResult};
use crate::proto::cluster_service_server::{ClusterService, ClusterServiceServer};
use crate::proto::blixard_service_server::{BlixardService, BlixardServiceServer};
use crate::transport::config::{TransportConfig, ServiceType, MigrationStrategy};
use crate::transport::iroh_grpc_bridge::{IrohServiceRunner, GrpcProtocolHandler};
use std::net::SocketAddr;
use std::sync::Arc;
use tonic::transport::Server;
use async_trait::async_trait;

/// Trait for running services over different transports
#[async_trait]
pub trait ServiceRunner: Send + Sync {
    /// Start serving the service
    async fn serve(self: Box<Self>) -> BlixardResult<()>;
    
    /// Get the transport type
    fn transport_type(&self) -> &'static str;
}

/// gRPC service runner
pub struct GrpcServiceRunner<S> {
    server: Server,
    service: S,
    addr: SocketAddr,
}

impl<S> GrpcServiceRunner<S> {
    pub fn new(addr: SocketAddr, service: S) -> Self {
        Self {
            server: Server::builder(),
            service,
            addr,
        }
    }
}

// We need to implement ServiceRunner for specific concrete types
// since impl Trait is not allowed in impl headers

/// Dual transport service runner
pub struct DualServiceRunner<S> {
    grpc_runner: GrpcServiceRunner<S>,
    iroh_runner: Option<IrohServiceRunner<S>>,
    strategy: MigrationStrategy,
}

impl<S: Clone + tonic::server::NamedService + Send + Sync + 'static> DualServiceRunner<S> {
    pub fn new(
        grpc_addr: SocketAddr,
        service: S,
        iroh_endpoint: Option<iroh::Endpoint>,
        strategy: MigrationStrategy,
    ) -> Self {
        let grpc_runner = GrpcServiceRunner::new(grpc_addr, service.clone());
        let iroh_runner = iroh_endpoint.map(|endpoint| IrohServiceRunner::new(endpoint, service));
        
        Self {
            grpc_runner,
            iroh_runner,
            strategy,
        }
    }
}

#[async_trait]
impl<S> ServiceRunner for DualServiceRunner<S>
where
    S: tonic::server::NamedService + Clone + Send + Sync + 'static,
{
    async fn serve(self: Box<Self>) -> BlixardResult<()> {
        // In dual mode, run both transports
        // For now, just run gRPC since we need to implement ServiceRunner for GrpcServiceRunner
        // TODO: Implement proper dual transport serving
        Err(BlixardError::NotImplemented {
            feature: "Dual transport serving".to_string(),
        })
    }
    
    fn transport_type(&self) -> &'static str {
        "dual"
    }
}

/// Factory for creating services with the appropriate transport
pub struct ServiceFactory {
    transport_config: TransportConfig,
    iroh_endpoint: Option<Arc<iroh::Endpoint>>,
}

impl ServiceFactory {
    pub fn new(transport_config: TransportConfig, iroh_endpoint: Option<Arc<iroh::Endpoint>>) -> Self {
        Self {
            transport_config,
            iroh_endpoint,
        }
    }
    
    /// Start serving a cluster service with the configured transport
    pub async fn serve_cluster_service<I>(&self, implementation: I, addr: SocketAddr) -> BlixardResult<()>
    where
        I: ClusterService + Clone + Send + Sync + 'static,
    {
        let server = ClusterServiceServer::new(implementation);
        
        match &self.transport_config {
            TransportConfig::Grpc(_) => {
                Server::builder()
                    .add_service(server)
                    .serve(addr)
                    .await
                    .map_err(|e| BlixardError::GrpcError(e.to_string()))?;
                Ok(())
            }
            TransportConfig::Iroh(_) => {
                let endpoint = self.iroh_endpoint.as_ref()
                    .ok_or_else(|| BlixardError::Internal {
                        message: "Iroh endpoint not initialized".to_string(),
                    })?;
                let runner = IrohServiceRunner::new((**endpoint).clone(), server);
                runner.serve().await
            }
            TransportConfig::Dual { .. } => {
                // For now, just serve gRPC
                // TODO: Implement dual transport serving
                Server::builder()
                    .add_service(server)
                    .serve(addr)
                    .await
                    .map_err(|e| BlixardError::GrpcError(e.to_string()))?;
                Ok(())
            }
        }
    }
    
    /// Determine which transport to use for a specific service type
    pub fn transport_for_service(&self, service_type: ServiceType) -> TransportType {
        match &self.transport_config {
            TransportConfig::Grpc(_) => TransportType::Grpc,
            TransportConfig::Iroh(_) => TransportType::Iroh,
            TransportConfig::Dual { strategy, .. } => {
                if strategy.prefer_iroh_for.contains(&service_type) {
                    TransportType::Iroh
                } else {
                    TransportType::Grpc
                }
            }
        }
    }
}

/// Transport type indicator
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportType {
    Grpc,
    Iroh,
}