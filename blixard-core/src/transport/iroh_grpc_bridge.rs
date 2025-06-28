//! Bridge between Iroh transport and gRPC services
//!
//! This module implements the necessary adapters to run Tonic gRPC services
//! over Iroh's QUIC transport, enabling gradual migration from gRPC to Iroh.

use crate::error::{BlixardError, BlixardResult};
use bytes::Bytes;
use futures::{Stream, StreamExt};
use http::{Request, Response};
use http_body::Body as HttpBody;
use iroh::endpoint::Connection;
use std::pin::Pin;
use std::task::{Context, Poll};
use tonic::body::BoxBody;

/// HTTP/2 over QUIC adapter for Iroh connections
pub struct IrohHttpAdapter {
    connection: Connection,
}

impl IrohHttpAdapter {
    pub fn new(connection: Connection) -> Self {
        Self { connection }
    }

    /// Handle an HTTP/2 request over the Iroh connection
    pub async fn handle_request<S, B>(
        &self,
        service: S,
        request: Request<hyper::Body>,
    ) -> BlixardResult<Response<BoxBody>>
    where
        S: tonic::server::NamedService + Clone,
        S: tower::Service<Request<hyper::Body>, Response = Response<B>>,
        S::Error: Into<Box<dyn std::error::Error + Send + Sync + 'static>>,
        S::Future: Send + 'static,
        B: HttpBody<Data = Bytes> + Send + 'static,
        B::Error: Into<Box<dyn std::error::Error + Send + Sync + 'static>> + std::fmt::Display,
    {
        // Use tower service to handle the request
        let mut service = service.clone();
        
        // Poll readiness
        futures::future::poll_fn(|cx| service.poll_ready(cx))
            .await
            .map_err(|e| BlixardError::Internal {
                message: format!("Service not ready: {}", e.into()),
            })?;

        // Call the service
        let response = service.call(request).await.map_err(|e| BlixardError::Internal {
            message: format!("Service call failed: {}", e.into()),
        })?;

        // Convert response body to BoxBody
        let (parts, body) = response.into_parts();
        let body_bytes = hyper::body::to_bytes(body)
            .await
            .map_err(|e| BlixardError::Internal {
                message: format!("Failed to read body: {}", e),
            })?;
        
        let boxed_body = BoxBody::new(hyper::Body::from(body_bytes));

        Ok(Response::from_parts(parts, boxed_body))
    }
}

/// Protocol handler for running gRPC services over Iroh
pub struct GrpcProtocolHandler<S> {
    service: S,
    alpn: Vec<u8>,
}

impl<S> GrpcProtocolHandler<S> {
    /// Create a new protocol handler for a gRPC service
    pub fn for_service(service: S) -> (Self, String)
    where
        S: tonic::server::NamedService,
    {
        let service_name = S::NAME;
        let alpn = format!("grpc/{}", service_name).into_bytes();
        let alpn_str = String::from_utf8_lossy(&alpn).to_string();
        
        let handler = Self {
            service,
            alpn: alpn.clone(),
        };
        
        (handler, alpn_str)
    }

    /// Get the ALPN protocol identifier
    pub fn alpn(&self) -> &[u8] {
        &self.alpn
    }
}

/// Stream adapter for Iroh QUIC streams
pub struct IrohStreamAdapter {
    recv_stream: iroh::endpoint::RecvStream,
    send_stream: iroh::endpoint::SendStream,
}

impl IrohStreamAdapter {
    pub fn new(recv_stream: iroh::endpoint::RecvStream, send_stream: iroh::endpoint::SendStream) -> Self {
        Self {
            recv_stream,
            send_stream,
        }
    }
}

/// Client channel implementation for Iroh transport
pub struct IrohChannel {
    endpoint: iroh::Endpoint,
    node_addr: iroh::NodeAddr,
}

impl IrohChannel {
    pub async fn connect(endpoint: iroh::Endpoint, node_addr: iroh::NodeAddr) -> BlixardResult<Self> {
        // Verify we can connect to the node
        // For now, we'll just store the endpoint and node address
        // Actual connection will be established when making RPC calls
        Ok(Self { endpoint, node_addr })
    }
}

impl tower::Service<Request<BoxBody>> for IrohChannel {
    type Response = Response<BoxBody>;
    type Error = tonic::Status;
    type Future = Pin<Box<dyn std::future::Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, request: Request<BoxBody>) -> Self::Future {
        let endpoint = self.endpoint.clone();
        let node_addr = self.node_addr.clone();
        
        Box::pin(async move {
            // Connect to the peer with a gRPC ALPN
            let connection = endpoint
                .connect(node_addr, &b"h2"[..])
                .await
                .map_err(|e| tonic::Status::unavailable(format!("Connection failed: {}", e)))?;
            
            // Open a bidirectional stream
            let (send_stream, recv_stream) = connection
                .open_bi()
                .await
                .map_err(|e| tonic::Status::unavailable(format!("Stream open failed: {}", e)))?;
            
            // TODO: Implement HTTP/2 framing over QUIC streams
            // For now, return a simple error response
            Err(tonic::Status::unimplemented("Iroh transport not yet fully implemented"))
        })
    }
}

/// Service runner for Iroh transport
pub struct IrohServiceRunner<S> {
    endpoint: iroh::Endpoint,
    handler: GrpcProtocolHandler<S>,
}

impl<S> IrohServiceRunner<S>
where
    S: tonic::server::NamedService + Clone + Send + Sync + 'static,
{
    pub fn new(endpoint: iroh::Endpoint, service: S) -> Self {
        let (handler, _) = GrpcProtocolHandler::for_service(service);
        Self { endpoint, handler }
    }

    /// Start accepting connections and serving the gRPC service
    pub async fn serve(self) -> BlixardResult<()> {
        // Set the ALPN protocol
        // Note: This is a simplified version - real implementation would need
        // to properly handle ALPN negotiation
        
        loop {
            // Accept incoming connections
            let incoming = self.endpoint.accept().await;
            
            if let Some(incoming) = incoming {
                let connection = incoming.await
                    .map_err(|e| BlixardError::P2PError(format!("Accept failed: {}", e)))?;
                
                // Check ALPN
                let alpn = connection.alpn();
                if alpn.as_deref() != Some(self.handler.alpn()) {
                    continue; // Not for this service
                }
                
                // Handle the connection in a separate task
                let handler = self.handler.service.clone();
                tokio::spawn(async move {
                    // TODO: Implement proper HTTP/2 handling over QUIC
                    // This would involve:
                    // 1. Reading HTTP/2 frames from QUIC streams
                    // 2. Assembling them into HTTP requests
                    // 3. Calling the gRPC service
                    // 4. Sending responses back over QUIC
                });
            } else {
                // Endpoint closed
                break;
            }
        }
        
        Ok(())
    }
}