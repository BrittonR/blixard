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
use async_trait::async_trait;
use bytes::Bytes;
use iroh::{Endpoint, discovery::dns::DnsDiscovery, protocol::{ProtocolHandler, Router}, endpoint::Connection};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::task::JoinHandle;
use tracing::{error, info, warn, debug};

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


    /// Handle an RPC stream
    async fn handle_rpc_stream(
        mut send: iroh::endpoint::SendStream,
        mut recv: iroh::endpoint::RecvStream,
        services: &HashMap<String, Arc<dyn IrohService>>,
    ) -> BlixardResult<()> {
        debug!("Starting to handle RPC stream");
        
        // Read the request
        debug!("Reading message from stream");
        let (header, payload) = read_message(&mut recv).await?;
        debug!("Read message with type {:?}, request_id: {:?}", header.msg_type, header.request_id);
        
        if header.msg_type != MessageType::Request {
            return Err(BlixardError::Internal {
                message: format!("Expected Request, got {:?}", header.msg_type),
            });
        }
        
        // Deserialize the RPC request
        let request: RpcRequest = crate::transport::iroh_protocol::deserialize_payload(&payload)?;
        debug!("Received RPC request for service: {}, method: {}", request.service, request.method);
        
        // Find the service
        let service = services.get(&request.service)
            .ok_or_else(|| BlixardError::ServiceNotFound(request.service.clone()))?;
        
        // Handle the request
        debug!("Calling service handler for {}.{}", request.service, request.method);
        let response = match service.handle_call(&request.method, request.payload).await {
            Ok(result) => RpcResponse {
                success: true,
                payload: Some(result),
                error: None,
            },
            Err(e) => RpcResponse {
                success: false,
                payload: None,
                error: Some(e.to_string()),
            },
        };
        
        debug!("Service handler completed, success: {}", response.success);
        
        // Send the response
        let response_bytes = crate::transport::iroh_protocol::serialize_payload(&response)?;
        debug!("Writing response message, size: {} bytes", response_bytes.len());
        write_message(&mut send, MessageType::Response, header.request_id, &response_bytes).await?;
        debug!("Response message written successfully");
        
        // Finish sending to signal we're done
        debug!("Finishing send stream");
        send.finish()
            .map_err(|e| BlixardError::Internal {
                message: format!("Failed to finish response stream: {}", e),
            })?;
        debug!("Send stream finished successfully");
        
        Ok(())
    }
}

/// Protocol handler for BLIXARD_ALPN
struct IrohProtocolHandler {
    services: HashMap<String, Arc<dyn IrohService>>,
}

impl std::fmt::Debug for IrohProtocolHandler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IrohProtocolHandler")
            .field("services", &self.services.len())
            .finish()
    }
}

#[async_trait]
impl ProtocolHandler for IrohProtocolHandler {
    fn accept<'a>(&'a self, connection: Connection) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), anyhow::Error>> + Send + 'a>> {
        Box::pin(async move {
            debug!("Accepted connection from {:?}", connection.remote_node_id());
            
            // Handle all streams on this connection
            loop {
                match connection.accept_bi().await {
                    Ok((send, recv)) => {
                        let services = self.services.clone();
                        tokio::spawn(async move {
                            if let Err(e) = IrohServiceRunner::handle_rpc_stream(send, recv, &services).await {
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
            
            Ok(())
        })
    }
}

/// Start the Iroh service runner
pub async fn start_iroh_services(
    shared_state: Arc<SharedNodeState>,
    bind_addr: SocketAddr,
) -> BlixardResult<JoinHandle<()>> {
    // Get or create Iroh endpoint (ensure we have Arc<Endpoint>)
    let endpoint: Arc<Endpoint> = if let Some(p2p_manager) = shared_state.get_p2p_manager().await {
        let (endpoint, _node_id) = p2p_manager.get_endpoint();
        Arc::new(endpoint)
    } else {
        // Create a new Iroh endpoint if P2P manager isn't available
        let ep = Endpoint::builder()
            .discovery(Box::new(DnsDiscovery::n0_dns()))
            .bind()
            .await
            .map_err(|e| BlixardError::Internal {
                message: format!("Failed to create Iroh endpoint: {}", e),
            })?;
        Arc::new(ep)
    };

    info!(
        "Starting Iroh services on {} with node ID {}",
        bind_addr,
        endpoint.node_id()
    );

    // Register our ALPN protocol
    let mut runner = IrohServiceRunner::new(shared_state, endpoint.clone());
    
    // Register all services
    runner.register_services().await?;
    
    let services = runner.services.clone();
    
    // Create a protocol handler for our ALPN
    let handler = IrohProtocolHandler { services };
    
    // Create router to handle incoming connections
    // Note: Router::builder takes ownership, so we need to dereference the Arc
    let router = Router::builder((*endpoint).clone())
        .accept(BLIXARD_ALPN.to_vec(), Arc::new(handler))
        .spawn();
    
    info!("Registered BLIXARD_ALPN protocol handler with {} services", runner.services.len());

    let handle = tokio::spawn(async move {
        // Keep the router alive - it will handle connections until dropped
        let _router = router;
        
        // Keep the task running until cancelled
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;
        }
    });

    Ok(handle)
}