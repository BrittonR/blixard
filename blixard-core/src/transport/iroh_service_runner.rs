//! Iroh service runner
//!
//! This module provides the main service runner for Iroh transport,
//! handling all RPC services over Iroh P2P connections.

use crate::error::{BlixardError, BlixardResult};
use crate::node_shared::SharedNodeState;
use crate::transport::{
    cluster_operations_adapter::ClusterOperationsAdapter,
    iroh_cluster_service::IrohClusterService,
    iroh_health_service::IrohHealthService,
    iroh_protocol::{MessageType, RpcRequest, RpcResponse, read_message, write_message},
    iroh_service::IrohService,
    iroh_status_service::IrohStatusService,
    iroh_vm_service::IrohVmService,
    BLIXARD_ALPN,
};
// VM operations are handled through SharedNodeState
use bytes::Bytes;
use iroh::{Endpoint, discovery::dns::DnsDiscovery};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::task::JoinHandle;
use tracing::{error, info, warn};

/// Iroh service runner that handles all RPC services
pub struct IrohServiceRunner {
    shared_state: Arc<SharedNodeState>,
    endpoint: Arc<Endpoint>,
    services: HashMap<String, Arc<dyn IrohService>>,
}

impl IrohServiceRunner {
    /// Create a new Iroh service runner
    pub fn new(
        shared_state: Arc<SharedNodeState>,
        endpoint: Arc<Endpoint>,
    ) -> Self {
        Self {
            shared_state,
            endpoint,
            services: HashMap::new(),
        }
    }

    /// Register all services
    pub async fn register_services(&mut self) -> BlixardResult<()> {
        // Register health service
        let health_service = Arc::new(IrohHealthService::new(self.shared_state.clone()));
        self.register_service(health_service)?;

        // Register status service
        let status_service = Arc::new(IrohStatusService::new(self.shared_state.clone()));
        self.register_service(status_service)?;

        // Register VM service
        let vm_service = Arc::new(IrohVmService::new(self.shared_state.clone()));
        self.register_service(vm_service)?;

        // Register cluster service
        let cluster_operations = Arc::new(ClusterOperationsAdapter::new(self.shared_state.clone()));
        let cluster_service = Arc::new(IrohClusterService::new(cluster_operations));
        self.register_service(cluster_service)?;

        info!(
            "Registered {} Iroh services",
            self.services.len()
        );

        Ok(())
    }

    /// Register a service
    fn register_service(&mut self, service: Arc<dyn IrohService>) -> BlixardResult<()> {
        let service_name = service.name().to_string();
        if self.services.contains_key(&service_name) {
            return Err(BlixardError::Internal {
                message: format!("Service {} already registered", service_name),
            });
        }
        self.services.insert(service_name.clone(), service);
        info!("Registered Iroh service: {}", service_name);
        Ok(())
    }

    /// Run the service runner
    pub async fn run(mut self) -> BlixardResult<()> {
        // Register all services
        self.register_services().await?;

        let services = Arc::new(self.services);
        let endpoint = self.endpoint.clone();

        info!(
            "Starting Iroh service runner on node {}",
            endpoint.node_id()
        );

        // Accept incoming connections
        loop {
            match endpoint.accept().await {
                Some(incoming) => {
                    let services = services.clone();
                    tokio::spawn(async move {
                        match incoming.accept().await {
                            Ok((connection, alpn)) => {
                                if alpn.as_bytes() != BLIXARD_ALPN {
                                    warn!("Rejected connection with unknown ALPN: {:?}", alpn);
                                    return;
                                }

                                let remote_node_id = connection.remote_node_id();
                                info!("Accepted connection from {}", remote_node_id);

                                // Handle all streams on this connection
                                loop {
                                    match connection.accept_bi().await {
                                        Ok((send, recv)) => {
                                            let services = services.clone();
                                            tokio::spawn(async move {
                                                if let Err(e) = Self::handle_rpc_stream(send, recv, &services).await {
                                                    error!("Error handling RPC stream: {}", e);
                                                }
                                            });
                                        }
                                        Err(e) => {
                                            info!("Connection closed: {}", e);
                                            break;
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                error!("Failed to accept connection: {}", e);
                            }
                        }
                    });
                }
                None => {
                    info!("Endpoint closed, shutting down service runner");
                    break;
                }
            }
        }

        Ok(())
    }

    /// Handle an RPC stream
    async fn handle_rpc_stream(
        mut send: iroh::endpoint::SendStream,
        mut recv: iroh::endpoint::RecvStream,
        services: &HashMap<String, Arc<dyn IrohService>>,
    ) -> BlixardResult<()> {
        // Read the request
        let (header, payload) = read_message(&mut recv).await?;
        
        if header.msg_type != MessageType::Request {
            return Err(BlixardError::Internal {
                message: format!("Expected Request, got {:?}", header.msg_type),
            });
        }
        
        // Deserialize the RPC request
        let request: RpcRequest = crate::transport::iroh_protocol::deserialize_payload(&payload)?;
        
        // Find the service
        let service = services.get(&request.service)
            .ok_or_else(|| BlixardError::ServiceNotFound(request.service.clone()))?;
        
        // Handle the request
        let response = match service.handle_call(&request.method, request.payload).await {
            Ok(result) => RpcResponse {
                request_id: request.request_id,
                success: true,
                payload: result,
                error: None,
            },
            Err(e) => RpcResponse {
                request_id: request.request_id,
                success: false,
                payload: Bytes::new(),
                error: Some(e.to_string()),
            },
        };
        
        // Send the response
        let response_bytes = crate::transport::iroh_protocol::serialize_payload(&response)?;
        write_message(&mut send, MessageType::Response, response_bytes).await?;
        
        Ok(())
    }
}

/// Start the Iroh service runner
pub async fn start_iroh_services(
    shared_state: Arc<SharedNodeState>,
    bind_addr: SocketAddr,
) -> BlixardResult<JoinHandle<()>> {
    // Get or create Iroh endpoint
    let endpoint = if let Some(p2p_manager) = shared_state.get_p2p_manager().await {
        match p2p_manager.endpoint() {
            Ok(endpoint) => endpoint,
            Err(e) => {
                return Err(BlixardError::Internal {
                    message: format!("Failed to get Iroh endpoint: {}", e),
                });
            }
        }
    } else {
        // Create a new Iroh endpoint if P2P manager isn't available
        let endpoint = Endpoint::builder()
            .discovery(Box::new(DnsDiscovery::n0_dns()))
            .bind(0)
            .await
            .map_err(|e| BlixardError::Internal {
                message: format!("Failed to create Iroh endpoint: {}", e),
            })?;
        Arc::new(endpoint)
    };

    info!(
        "Starting Iroh services on {} with node ID {}",
        bind_addr,
        endpoint.node_id()
    );

    let runner = IrohServiceRunner::new(shared_state, endpoint);

    let handle = tokio::spawn(async move {
        if let Err(e) = runner.run().await {
            error!("Iroh service runner error: {}", e);
        }
    });

    Ok(handle)
}