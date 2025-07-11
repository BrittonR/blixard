//! Secure protocol handler for Iroh connections with Cedar authorization
//!
//! This implements a ProtocolHandler that integrates authorization checks
//! before delegating to the actual service implementations.

use crate::{
    error::{BlixardError, BlixardResult},
    transport::{
        iroh_middleware::IrohMiddleware,
        iroh_protocol::{
            deserialize_payload, read_message, serialize_payload,
            write_message, MessageType, RpcRequest, RpcResponse,
        },
        iroh_secure_vm_service::{SecureIrohService, SecureIrohVmService},
    },
};
use bytes::Bytes;
use iroh::{
    endpoint::Connection,
    protocol::{AcceptError, ProtocolHandler},
};
use std::{collections::HashMap, sync::Arc};
use tokio::sync::RwLock;
use tracing::{debug, error, info};

/// Registry of secure services
pub struct SecureServiceRegistry {
    services: Arc<RwLock<HashMap<String, Arc<dyn SecureIrohService>>>>,
    middleware: Arc<IrohMiddleware>,
}

impl std::fmt::Debug for SecureServiceRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SecureServiceRegistry")
            .field("middleware", &self.middleware)
            .field("services", &"<dyn SecureIrohService>")
            .finish()
    }
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
#[derive(Clone, Debug)]
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
        info!(
            "Handling secure RPC stream from {:?}",
            connection.remote_node_id()
        );

        // Authenticate the connection once
        let auth_context = match self.middleware.authenticate_connection(&connection).await {
            Ok(ctx) => {
                info!("Authenticated connection from user: {}", ctx.user_id);
                ctx
            }
            Err(e) => {
                error!("Authentication failed: {}", e);
                // Send error response
                let _error_response = RpcResponse {
                    success: false,
                    error: Some(format!("Authentication failed: {}", e)),
                    payload: None,
                };
                // TODO: Need to handle write_message parameters properly
                return Err(e);
            }
        };

        loop {
            // Read request
            let (header, payload) = match read_message(&mut recv).await {
                Ok(result) => result,
                Err(e) => {
                    error!("Failed to read request: {}", e);
                    break;
                }
            };

            if header.msg_type != MessageType::Request {
                error!("Expected Request, got {:?}", header.msg_type);
                break;
            }

            // Deserialize the RPC request
            let request: RpcRequest = match deserialize_payload(&payload) {
                Ok(req) => req,
                Err(e) => {
                    error!("Failed to deserialize request: {}", e);
                    break;
                }
            };

            debug!(
                "Received RPC request: service={}, method={}",
                request.service, request.method
            );

            // Process request with authorization
            let response = match self
                .process_authorized_request(&connection, &auth_context, request.clone())
                .await
            {
                Ok(payload) => RpcResponse {
                    success: true,
                    error: None,
                    payload: Some(payload),
                },
                Err(e) => {
                    error!("Request processing failed: {}", e);
                    RpcResponse {
                        success: false,
                        error: Some(e.to_string()),
                        payload: None,
                    }
                }
            };

            // Send response
            let response_bytes = serialize_payload(&response)?;
            if let Err(e) = write_message(
                &mut send,
                MessageType::Response,
                header.request_id,
                &response_bytes,
            )
            .await
            {
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
        let service = self
            .registry
            .get(&request.service)
            .await
            .ok_or_else(|| BlixardError::ServiceNotFound(request.service.clone()))?;

        // Log the authorization attempt
        info!(
            "User {} attempting to call {}.{}",
            auth_context.user_id, request.service, request.method
        );

        // Use the secure call method that includes connection context
        service
            .handle_secure_call(connection, &request.method, request.payload)
            .await
    }
}

impl ProtocolHandler for SecureRpcProtocolHandler {
    fn accept<'a>(
        &'a self,
        connection: Connection,
    ) -> impl futures::Future<Output = std::result::Result<(), AcceptError>> + std::marker::Send
    {
        Box::pin(async move {
            debug!(
                "Accepting secure RPC connection from {:?}",
                connection.remote_node_id()
            );

            // Handle all streams on this connection
            loop {
                match connection.accept_bi().await {
                    Ok((send, recv)) => {
                        // Handle the RPC stream
                        if let Err(e) = self.handle_rpc_stream(connection.clone(), send, recv).await
                        {
                            error!("RPC stream handler error: {}", e);
                            // Continue handling other streams
                        }
                    }
                    Err(e) => {
                        // Connection closed, this is normal
                        info!("Connection closed: {}", e);
                        break;
                    }
                }
            }

            Ok(())
        })
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
