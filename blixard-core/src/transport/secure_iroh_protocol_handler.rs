//! Secure protocol handler for Iroh connections with Cedar authorization
//!
//! This implements a ProtocolHandler that integrates authorization checks
//! before delegating to the actual service implementations.

use crate::{
    error::{BlixardError, BlixardResult},
    transport::{
        iroh_middleware::{IrohMiddleware, AuthenticatedRequest},
        iroh_protocol::{
            RpcRequest, RpcResponse, MessageType, MessageHeader,
            read_message, write_message, generate_request_id,
            serialize_payload, deserialize_payload,
        },
        iroh_secure_vm_service::{SecureIrohVmService, SecureIrohService},
    },
};
use async_trait::async_trait;
use bytes::Bytes;
use iroh::{
    protocol::ProtocolHandler,
    endpoint::Connection,
};
use std::{
    collections::HashMap,
    sync::Arc,
    time::Duration,
};
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

/// Registry of secure services
pub struct SecureServiceRegistry {
    services: Arc<RwLock<HashMap<String, Arc<dyn SecureIrohService>>>>,
    middleware: Arc<IrohMiddleware>,
}

impl SecureServiceRegistry {
    pub fn new(middleware: Arc<IrohMiddleware>) -> Self {
        Self {
            services: Arc::new(RwLock::new(HashMap::new())),
            middleware,
        }
    }
    
    pub async fn register<S: SecureIrohService + 'static>(&self, service: S) {
        let mut services = self.services.write().await;
        services.insert(service.name().to_string(), Arc::new(service));
    }
    
    pub async fn get(&self, name: &str) -> Option<Arc<dyn SecureIrohService>> {
        let services = self.services.read().await;
        services.get(name).cloned()
    }
}

/// Secure RPC protocol handler with Cedar authorization
#[derive(Clone)]
pub struct SecureRpcProtocolHandler {
    registry: Arc<SecureServiceRegistry>,
    middleware: Arc<IrohMiddleware>,
}

impl SecureRpcProtocolHandler {
    pub fn new(middleware: Arc<IrohMiddleware>) -> Self {
        Self {
            registry: Arc::new(SecureServiceRegistry::new(middleware.clone())),
            middleware,
        }
    }
    
    pub fn registry(&self) -> Arc<SecureServiceRegistry> {
        self.registry.clone()
    }
    
    /// Handle incoming RPC stream with authorization
    async fn handle_rpc_stream(
        &self,
        connection: Connection,
        mut send: iroh::endpoint::SendStream,
        mut recv: iroh::endpoint::RecvStream,
    ) -> BlixardResult<()> {
        info!("Handling secure RPC stream from {:?}", connection.remote_node_id());
        
        // Authenticate the connection once
        let auth_context = match self.middleware.authenticate_connection(&connection).await {
            Ok(ctx) => {
                info!("Authenticated connection from user: {}", ctx.user_id);
                ctx
            }
            Err(e) => {
                error!("Authentication failed: {}", e);
                // Send error response
                let error_response = RpcResponse {
                    request_id: 0,
                    success: false,
                    error: Some(format!("Authentication failed: {}", e)),
                    payload: Bytes::new(),
                };
                let _ = write_message(&mut send, &error_response).await;
                return Err(e);
            }
        };
        
        loop {
            // Read request
            let request = match read_message::<RpcRequest>(&mut recv).await {
                Ok(Some(req)) => req,
                Ok(None) => {
                    debug!("Client closed connection");
                    break;
                }
                Err(e) => {
                    error!("Failed to read request: {}", e);
                    break;
                }
            };
            
            debug!("Received RPC request: service={}, method={}", request.service, request.method);
            
            // Process request with authorization
            let response = match self.process_authorized_request(&connection, &auth_context, request.clone()).await {
                Ok(payload) => RpcResponse {
                    request_id: request.request_id,
                    success: true,
                    error: None,
                    payload,
                },
                Err(e) => {
                    error!("Request processing failed: {}", e);
                    RpcResponse {
                        request_id: request.request_id,
                        success: false,
                        error: Some(e.to_string()),
                        payload: Bytes::new(),
                    }
                }
            };
            
            // Send response
            if let Err(e) = write_message(&mut send, &response).await {
                error!("Failed to send response: {}", e);
                break;
            }
        }
        
        debug!("RPC stream handler completed");
        Ok(())
    }
    
    /// Process a request with authorization checks
    async fn process_authorized_request(
        &self,
        connection: &Connection,
        auth_context: &crate::transport::iroh_middleware::IrohAuthContext,
        request: RpcRequest,
    ) -> BlixardResult<Bytes> {
        // Get the service
        let service = self.registry.get(&request.service).await
            .ok_or_else(|| BlixardError::ServiceNotFound(request.service.clone()))?;
        
        // Log the authorization attempt
        info!(
            "User {} attempting to call {}.{}", 
            auth_context.user_id, 
            request.service, 
            request.method
        );
        
        // Use the secure call method that includes connection context
        service.handle_secure_call(connection, &request.method, request.payload).await
    }
}

#[async_trait]
impl ProtocolHandler for SecureRpcProtocolHandler {
    async fn accept(&self, connection: Connection) -> BlixardResult<()> {
        debug!("Accepting secure RPC connection from {:?}", connection.remote_node_id());
        
        // Accept bidirectional stream
        let (send, recv) = match connection.accept_bi().await {
            Ok(streams) => streams,
            Err(e) => {
                error!("Failed to accept bidirectional stream: {}", e);
                return Err(BlixardError::Connection {
                    message: format!("Failed to accept stream: {}", e),
                });
            }
        };
        
        // Handle the RPC stream
        if let Err(e) = self.handle_rpc_stream(connection.clone(), send, recv).await {
            error!("RPC stream handler error: {}", e);
        }
        
        // Wait for connection to close
        connection.closed().await;
        debug!("Connection closed");
        
        Ok(())
    }
}

/// Builder for setting up secure Iroh services
pub struct SecureIrohServiceBuilder {
    middleware: Arc<IrohMiddleware>,
    handler: SecureRpcProtocolHandler,
}

impl SecureIrohServiceBuilder {
    pub fn new(middleware: Arc<IrohMiddleware>) -> Self {
        let handler = SecureRpcProtocolHandler::new(middleware.clone());
        Self {
            middleware,
            handler,
        }
    }
    
    /// Register a VM service with security
    pub async fn with_vm_service(self, node: Arc<crate::node_shared::SharedNodeState>) -> Self {
        let vm_service = SecureIrohVmService::new(node, self.middleware.clone());
        self.handler.registry().register(vm_service).await;
        self
    }
    
    /// Register additional secure services
    pub async fn with_service<S: SecureIrohService + 'static>(self, service: S) -> Self {
        self.handler.registry().register(service).await;
        self
    }
    
    /// Build the protocol handler
    pub fn build(self) -> SecureRpcProtocolHandler {
        self.handler
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::iroh_middleware::NodeIdentityRegistry;
    
    #[tokio::test]
    async fn test_secure_protocol_handler_creation() {
        let registry = Arc::new(NodeIdentityRegistry::new());
        let middleware = Arc::new(IrohMiddleware::new(None, None, registry));
        
        let handler = SecureRpcProtocolHandler::new(middleware);
        
        // Verify the handler was created
        assert!(handler.registry.services.read().await.is_empty());
    }
}