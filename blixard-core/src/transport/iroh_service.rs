//! Service abstraction for Iroh RPC
//!
//! This module defines the trait-based service infrastructure for handling
//! RPC calls over Iroh transport.

use crate::{
    error::{BlixardError, BlixardResult},
    transport::iroh_protocol::{
        RpcRequest, RpcResponse, MessageType, MessageHeader,
        read_message, write_message, generate_request_id,
        serialize_payload, deserialize_payload,
    },
};
use async_trait::async_trait;
use bytes::Bytes;
use iroh::{
    endpoint::{Connection, RecvStream, SendStream},
    Endpoint, NodeAddr,
};
use std::{
    collections::HashMap,
    sync::Arc,
    time::Duration,
};
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

/// Trait for implementing Iroh RPC services
#[async_trait]
pub trait IrohService: Send + Sync + 'static {
    /// Service name
    fn name(&self) -> &'static str;
    
    /// Handle a method call
    async fn handle_call(&self, method: &str, payload: Bytes) -> BlixardResult<Bytes>;
    
    /// List available methods (for introspection)
    fn methods(&self) -> Vec<&'static str> {
        vec![]
    }
}

/// Service registry for managing multiple services
pub struct ServiceRegistry {
    services: HashMap<String, Arc<dyn IrohService>>,
}

impl ServiceRegistry {
    /// Create a new service registry
    pub fn new() -> Self {
        Self {
            services: HashMap::new(),
        }
    }
    
    /// Register a service
    pub fn register<S: IrohService>(&mut self, service: S) {
        let name = service.name().to_string();
        self.services.insert(name, Arc::new(service));
    }
    
    /// Get a service by name
    pub fn get(&self, name: &str) -> Option<Arc<dyn IrohService>> {
        self.services.get(name).cloned()
    }
    
    /// List all registered services
    pub fn list_services(&self) -> Vec<String> {
        self.services.keys().cloned().collect()
    }
}

/// Server that handles incoming RPC connections
pub struct IrohRpcServer {
    endpoint: Endpoint,
    registry: Arc<RwLock<ServiceRegistry>>,
}

impl IrohRpcServer {
    /// Create a new RPC server
    pub fn new(endpoint: Endpoint) -> Self {
        Self {
            endpoint,
            registry: Arc::new(RwLock::new(ServiceRegistry::new())),
        }
    }
    
    /// Register a service
    pub async fn register_service<S: IrohService>(&self, service: S) {
        let mut registry = self.registry.write().await;
        registry.register(service);
        info!("Registered service: {}", registry.services.keys().last().unwrap());
    }
    
    /// Start accepting connections
    pub async fn serve(self: Arc<Self>) -> BlixardResult<()> {
        info!("Iroh RPC server starting on node {}", self.endpoint.node_id());
        
        loop {
            match self.endpoint.accept().await {
                Some(incoming) => {
                    let server = self.clone();
                    tokio::spawn(async move {
                        match incoming.accept() {
                            Ok(connecting) => {
                                match connecting.await {
                                    Ok(connection) => {
                                        if let Err(e) = server.handle_connection(connection).await {
                                            error!("Connection handling error: {}", e);
                                        }
                                    }
                                    Err(e) => {
                                        error!("Connection error: {}", e);
                                    }
                                }
                            }
                            Err(e) => {
                                error!("Accept error: {}", e);
                            }
                        }
                    });
                }
                None => {
                    // Endpoint closed
                    break;
                }
            }
        }
        
        Ok(())
    }
    
    /// Handle a single connection
    async fn handle_connection(&self, connection: Connection) -> BlixardResult<()> {
        let remote_node_id = connection.remote_node_id();
        match remote_node_id {
            Ok(id) => debug!("New connection from {}", id),
            Err(e) => debug!("New connection from unknown node: {}", e),
        }
        
        loop {
            match connection.accept_bi().await {
                Ok((send, recv)) => {
                    let registry = self.registry.clone();
                    tokio::spawn(async move {
                        if let Err(e) = handle_stream(registry, send, recv).await {
                            warn!("Stream handling error: {}", e);
                        }
                    });
                }
                Err(e) => {
                    debug!("Connection closed: {}", e);
                    break;
                }
            }
        }
        
        Ok(())
    }
}

/// Handle a single request/response stream
async fn handle_stream(
    registry: Arc<RwLock<ServiceRegistry>>,
    mut send: SendStream,
    mut recv: RecvStream,
) -> BlixardResult<()> {
    // Read request
    let (header, payload) = read_message(&mut recv).await?;
    
    if header.msg_type != MessageType::Request {
        return Err(BlixardError::Internal {
            message: format!("Expected request, got {:?}", header.msg_type),
        });
    }
    
    // Deserialize RPC request
    let request: RpcRequest = deserialize_payload(&payload)?;
    
    // Find service
    let registry = registry.read().await;
    let service = registry.get(&request.service).ok_or_else(|| {
        BlixardError::NotFound {
            resource: format!("Service '{}'", request.service),
        }
    })?;
    
    // Handle the call
    let response = match service.handle_call(&request.method, request.payload).await {
        Ok(payload) => RpcResponse {
            success: true,
            payload: Some(payload),
            error: None,
        },
        Err(e) => RpcResponse {
            success: false,
            payload: None,
            error: Some(e.to_string()),
        },
    };
    
    // Send response
    let response_bytes = serialize_payload(&response)?;
    write_message(
        &mut send,
        MessageType::Response,
        header.request_id,
        &response_bytes,
    )
    .await?;
    
    // Close stream
    send.finish()
        .map_err(|e| BlixardError::Internal {
            message: format!("Failed to finish stream: {}", e),
        })?;
    
    Ok(())
}

/// Client for making RPC calls
pub struct IrohRpcClient {
    endpoint: Endpoint,
    connections: Arc<RwLock<HashMap<iroh::NodeId, Connection>>>,
}

impl IrohRpcClient {
    /// Create a new RPC client
    pub fn new(endpoint: Endpoint) -> Self {
        Self {
            endpoint,
            connections: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    
    /// Make an RPC call
    pub async fn call<Req, Res>(
        &self,
        node_addr: NodeAddr,
        service: &str,
        method: &str,
        request: Req,
    ) -> BlixardResult<Res>
    where
        Req: serde::Serialize,
        Res: for<'de> serde::Deserialize<'de>,
    {
        // Get or create connection
        let connection = self.get_or_create_connection(node_addr).await?;
        
        // Open bidirectional stream
        let (mut send, mut recv) = connection
            .open_bi()
            .await
            .map_err(|e| BlixardError::Internal {
                message: format!("Failed to open stream: {}", e),
            })?;
        
        // Prepare request
        let request_bytes = serialize_payload(&request)?;
        let rpc_request = RpcRequest {
            service: service.to_string(),
            method: method.to_string(),
            payload: request_bytes,
        };
        
        // Send request
        let request_id = generate_request_id();
        let rpc_bytes = serialize_payload(&rpc_request)?;
        write_message(&mut send, MessageType::Request, request_id, &rpc_bytes).await?;
        
        // Finish sending
        send.finish()
            .map_err(|e| BlixardError::Internal {
                message: format!("Failed to finish send: {}", e),
            })?;
        
        // Read response
        let (header, payload) = read_message(&mut recv).await?;
        
        if header.request_id != request_id {
            return Err(BlixardError::Internal {
                message: "Request ID mismatch".to_string(),
            });
        }
        
        if header.msg_type != MessageType::Response {
            return Err(BlixardError::Internal {
                message: format!("Expected response, got {:?}", header.msg_type),
            });
        }
        
        // Deserialize response
        let response: RpcResponse = deserialize_payload(&payload)?;
        
        if response.success {
            let payload = response.payload.ok_or_else(|| BlixardError::Internal {
                message: "Success response missing payload".to_string(),
            })?;
            deserialize_payload(&payload)
        } else {
            Err(BlixardError::Internal {
                message: response.error.unwrap_or_else(|| "Unknown error".to_string()),
            })
        }
    }
    
    /// Get or create a connection to a node
    async fn get_or_create_connection(&self, node_addr: NodeAddr) -> BlixardResult<Connection> {
        let node_id = node_addr.node_id;
        
        // Check existing connection
        {
            let connections = self.connections.read().await;
            if let Some(conn) = connections.get(&node_id) {
                // TODO: Check if connection is still alive
                return Ok(conn.clone());
            }
        }
        
        // Create new connection
        info!("Connecting to {}", node_id);
        let connection = self
            .endpoint
            .connect(node_addr, b"blixard/rpc/1")
            .await
            .map_err(|e| BlixardError::Internal {
                message: format!("Failed to connect: {}", e),
            })?;
        
        // Store connection
        {
            let mut connections = self.connections.write().await;
            connections.insert(node_id, connection.clone());
        }
        
        Ok(connection)
    }
    
    /// Close connection to a node
    pub async fn close_connection(&self, node_id: iroh::NodeId) {
        let mut connections = self.connections.write().await;
        connections.remove(&node_id);
    }
}