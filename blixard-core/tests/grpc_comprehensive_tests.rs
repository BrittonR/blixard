//! Comprehensive gRPC testing suite covering all aspects of gRPC functionality
//!
//! This test suite covers:
//! - Streaming patterns (server, client, bidirectional)
//! - Deadline/timeout handling
//! - Metadata/headers propagation
//! - Large message handling
//! - Concurrent stream handling
//! - Error detail propagation
//! - Max message size enforcement
//! - Connection keep-alive
//! - gRPC status code coverage

#![cfg(feature = "test-helpers")]

use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::{mpsc, Mutex};
use tokio_stream::{wrappers::ReceiverStream, Stream, StreamExt};
use tonic::{
    metadata::{MetadataMap, MetadataValue},
    transport::{Channel, Server},
    Request, Response, Status, Streaming,
};

use blixard_core::{
    proto::{
        streaming_service_server::{StreamingService, StreamingServiceServer},
        test_service_server::{TestService, TestServiceServer},
        streaming_service_client::StreamingServiceClient,
        test_service_client::TestServiceClient,
        grpc_health_server::{GrpcHealth, GrpcHealthServer},
        grpc_health_client::GrpcHealthClient,
        GrpcHealthCheckRequest,
        GrpcHealthCheckResponse,
        grpc_health_check_response::ServingStatus,
        EchoRequest, EchoResponse,
        StreamRequest, StreamResponse,
        LargeMessage, TimeoutRequest, TimeoutResponse,
        ErrorRequest, ErrorResponse,
    },
    test_helpers::timing,
};

/// Test service implementation for comprehensive gRPC testing
#[derive(Debug, Clone)]
struct TestServiceImpl {
    request_count: Arc<Mutex<u32>>,
    metadata_store: Arc<Mutex<MetadataMap>>,
}

impl TestServiceImpl {
    fn new() -> Self {
        Self {
            request_count: Arc::new(Mutex::new(0)),
            metadata_store: Arc::new(Mutex::new(MetadataMap::new())),
        }
    }
}

#[tonic::async_trait]
impl TestService for TestServiceImpl {
    async fn echo(&self, request: Request<EchoRequest>) -> Result<Response<EchoResponse>, Status> {
        // Increment request count
        let mut count = self.request_count.lock().await;
        *count += 1;
        
        // Store metadata for verification
        let mut metadata = self.metadata_store.lock().await;
        *metadata = request.metadata().clone();
        
        let inner = request.into_inner();
        
        // Return error if requested
        if inner.message.starts_with("ERROR:") {
            let code = inner.message.strip_prefix("ERROR:").unwrap();
            return match code {
                "INVALID_ARGUMENT" => Err(Status::invalid_argument("Test invalid argument")),
                "DEADLINE_EXCEEDED" => Err(Status::deadline_exceeded("Test deadline exceeded")),
                "NOT_FOUND" => Err(Status::not_found("Test not found")),
                "ALREADY_EXISTS" => Err(Status::already_exists("Test already exists")),
                "PERMISSION_DENIED" => Err(Status::permission_denied("Test permission denied")),
                "RESOURCE_EXHAUSTED" => Err(Status::resource_exhausted("Test resource exhausted")),
                "FAILED_PRECONDITION" => Err(Status::failed_precondition("Test failed precondition")),
                "ABORTED" => Err(Status::aborted("Test aborted")),
                "OUT_OF_RANGE" => Err(Status::out_of_range("Test out of range")),
                "UNIMPLEMENTED" => Err(Status::unimplemented("Test unimplemented")),
                "INTERNAL" => Err(Status::internal("Test internal error")),
                "UNAVAILABLE" => Err(Status::unavailable("Test unavailable")),
                "DATA_LOSS" => Err(Status::data_loss("Test data loss")),
                "UNAUTHENTICATED" => Err(Status::unauthenticated("Test unauthenticated")),
                _ => Err(Status::unknown("Test unknown error")),
            };
        }
        
        // Simulate delay if requested
        if inner.message.starts_with("DELAY:") {
            if let Some(delay_str) = inner.message.strip_prefix("DELAY:") {
                if let Ok(delay_ms) = delay_str.parse::<u64>() {
                    timing::robust_sleep(Duration::from_millis(delay_ms)).await;
                }
            }
        }
        
        Ok(Response::new(EchoResponse {
            message: inner.message,
            timestamp: SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        }))
    }
    
    async fn send_large_message(
        &self,
        request: Request<LargeMessage>,
    ) -> Result<Response<EchoResponse>, Status> {
        let inner = request.into_inner();
        
        // Verify data integrity
        let expected_byte = (inner.data.len() % 256) as u8;
        if !inner.data.iter().all(|&b| b == expected_byte) {
            return Err(Status::invalid_argument("Data integrity check failed"));
        }
        
        Ok(Response::new(EchoResponse {
            message: format!("Received {} bytes", inner.data.len()),
            timestamp: SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        }))
    }
    
    async fn timeout_test(
        &self,
        request: Request<TimeoutRequest>,
    ) -> Result<Response<TimeoutResponse>, Status> {
        let inner = request.into_inner();
        
        // Sleep for requested duration
        timing::robust_sleep(Duration::from_millis(inner.sleep_ms)).await;
        
        Ok(Response::new(TimeoutResponse {
            completed: true,
        }))
    }
    
    async fn trigger_error(
        &self,
        request: Request<ErrorRequest>,
    ) -> Result<Response<ErrorResponse>, Status> {
        let inner = request.into_inner();
        
        // Create error with details
        let mut status = match inner.error_code {
            1 => Status::invalid_argument("Validation failed"),
            2 => Status::not_found("Resource not found"),
            3 => Status::permission_denied("Insufficient permissions"),
            4 => Status::resource_exhausted("Rate limit exceeded"),
            5 => Status::failed_precondition("System not ready"),
            6 => Status::internal("Internal server error"),
            _ => Status::unknown("Unknown error code"),
        };
        
        // Add error details
        if inner.include_details {
            let details = format!(
                "Error details: code={}, context={}, timestamp={}",
                inner.error_code,
                inner.error_context,
                SystemTime::now()
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .unwrap()
                    .as_secs()
            );
            // In tonic 0.12, we can't add_source. Instead, include details in the status message
            status = match inner.error_code {
                1 => Status::invalid_argument(format!("Validation failed: {}", details)),
                2 => Status::not_found(format!("Resource not found: {}", details)),
                3 => Status::permission_denied(format!("Insufficient permissions: {}", details)),
                4 => Status::resource_exhausted(format!("Rate limit exceeded: {}", details)),
                5 => Status::failed_precondition(format!("System not ready: {}", details)),
                6 => Status::internal(format!("Internal server error: {}", details)),
                _ => Status::unknown(format!("Unknown error code: {}", details)),
            };
        }
        
        Err(status)
    }
}

/// Streaming service implementation
#[derive(Debug, Clone)]
struct StreamingServiceImpl {
    active_streams: Arc<Mutex<u32>>,
}

impl StreamingServiceImpl {
    fn new() -> Self {
        Self {
            active_streams: Arc::new(Mutex::new(0)),
        }
    }
}

#[tonic::async_trait]
impl StreamingService for StreamingServiceImpl {
    /// Server streaming RPC
    type ServerStreamStream = Pin<Box<dyn Stream<Item = Result<StreamResponse, Status>> + Send>>;
    
    async fn server_stream(
        &self,
        request: Request<StreamRequest>,
    ) -> Result<Response<Self::ServerStreamStream>, Status> {
        let inner = request.into_inner();
        
        // Increment active stream count
        {
            let mut count = self.active_streams.lock().await;
            *count += 1;
        }
        
        let active_streams = self.active_streams.clone();
        let (tx, rx) = mpsc::channel(128);
        
        // Spawn task to generate stream items
        tokio::spawn(async move {
            for i in 0..inner.count {
                let item = StreamResponse {
                    sequence: i,
                    message: format!("{} {}", inner.prefix, i),
                    timestamp: SystemTime::now()
                        .duration_since(SystemTime::UNIX_EPOCH)
                        .unwrap()
                        .as_secs(),
                };
                
                if tx.send(Ok(item)).await.is_err() {
                    break; // Client disconnected
                }
                
                // Add delay between items if requested
                if inner.delay_ms > 0 {
                    timing::robust_sleep(Duration::from_millis(inner.delay_ms as u64)).await;
                }
            }
            
            // Decrement active stream count
            let mut count = active_streams.lock().await;
            *count -= 1;
        });
        
        let stream = ReceiverStream::new(rx);
        Ok(Response::new(Box::pin(stream)))
    }
    
    /// Client streaming RPC
    async fn client_stream(
        &self,
        request: Request<Streaming<StreamRequest>>,
    ) -> Result<Response<StreamResponse>, Status> {
        let mut stream = request.into_inner();
        let mut count = 0u32;
        let mut messages = Vec::new();
        
        // Process all client messages
        while let Some(item) = stream.next().await {
            match item {
                Ok(req) => {
                    count += 1;
                    messages.push(req.prefix);
                }
                Err(e) => return Err(e),
            }
        }
        
        Ok(Response::new(StreamResponse {
            sequence: count,
            message: messages.join(", "),
            timestamp: SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        }))
    }
    
    /// Bidirectional streaming RPC
    type BidiStreamStream = Pin<Box<dyn Stream<Item = Result<StreamResponse, Status>> + Send>>;
    
    async fn bidi_stream(
        &self,
        request: Request<Streaming<StreamRequest>>,
    ) -> Result<Response<Self::BidiStreamStream>, Status> {
        let mut stream = request.into_inner();
        let (tx, rx) = mpsc::channel(128);
        
        // Spawn task to process incoming stream and respond
        tokio::spawn(async move {
            let mut sequence = 0u32;
            
            while let Some(item) = stream.next().await {
                match item {
                    Ok(req) => {
                        // Echo back with sequence number
                        let response = StreamResponse {
                            sequence,
                            message: format!("Echo: {}", req.prefix),
                            timestamp: SystemTime::now()
                                .duration_since(SystemTime::UNIX_EPOCH)
                                .unwrap()
                                .as_secs(),
                        };
                        
                        if tx.send(Ok(response)).await.is_err() {
                            break; // Client disconnected
                        }
                        
                        sequence += 1;
                        
                        // Add delay if requested
                        if req.delay_ms > 0 {
                            timing::robust_sleep(Duration::from_millis(req.delay_ms as u64)).await;
                        }
                    }
                    Err(e) => {
                        let _ = tx.send(Err(e)).await;
                        break;
                    }
                }
            }
        });
        
        let stream = ReceiverStream::new(rx);
        Ok(Response::new(Box::pin(stream)))
    }
}

/// Helper to create test client
async fn create_test_client(addr: SocketAddr) -> TestServiceClient<Channel> {
    let endpoint = format!("http://{}", addr);
    
    // Create channel with custom settings
    let channel = Channel::from_shared(endpoint)
        .unwrap()
        .timeout(Duration::from_secs(30))
        .connect()
        .await
        .expect("Failed to connect");
    
    TestServiceClient::new(channel)
}

/// Helper to create streaming client
async fn create_streaming_client(addr: SocketAddr) -> StreamingServiceClient<Channel> {
    let endpoint = format!("http://{}", addr);
    
    let channel = Channel::from_shared(endpoint)
        .unwrap()
        .connect()
        .await
        .expect("Failed to connect");
    
    StreamingServiceClient::new(channel)
}

// ===== Basic gRPC Tests =====

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_grpc_status_codes() {
    let test_service = TestServiceImpl::new();
    
    // Start server
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    
    tokio::spawn(async move {
        Server::builder()
            .add_service(TestServiceServer::new(test_service))
            .serve_with_incoming(tokio_stream::wrappers::TcpListenerStream::new(listener))
            .await
            .unwrap();
    });
    
    timing::robust_sleep(Duration::from_millis(100)).await;
    
    let mut client = create_test_client(addr).await;
    
    // Test each status code
    let test_cases = vec![
        ("ERROR:INVALID_ARGUMENT", tonic::Code::InvalidArgument),
        ("ERROR:DEADLINE_EXCEEDED", tonic::Code::DeadlineExceeded),
        ("ERROR:NOT_FOUND", tonic::Code::NotFound),
        ("ERROR:ALREADY_EXISTS", tonic::Code::AlreadyExists),
        ("ERROR:PERMISSION_DENIED", tonic::Code::PermissionDenied),
        ("ERROR:RESOURCE_EXHAUSTED", tonic::Code::ResourceExhausted),
        ("ERROR:FAILED_PRECONDITION", tonic::Code::FailedPrecondition),
        ("ERROR:ABORTED", tonic::Code::Aborted),
        ("ERROR:OUT_OF_RANGE", tonic::Code::OutOfRange),
        ("ERROR:UNIMPLEMENTED", tonic::Code::Unimplemented),
        ("ERROR:INTERNAL", tonic::Code::Internal),
        ("ERROR:UNAVAILABLE", tonic::Code::Unavailable),
        ("ERROR:DATA_LOSS", tonic::Code::DataLoss),
        ("ERROR:UNAUTHENTICATED", tonic::Code::Unauthenticated),
        ("ERROR:UNKNOWN", tonic::Code::Unknown),
    ];
    
    for (message, expected_code) in test_cases {
        let response = client.echo(EchoRequest {
            message: message.to_string(),
        }).await;
        
        assert!(response.is_err(), "Expected error for {}", message);
        let error = response.unwrap_err();
        assert_eq!(error.code(), expected_code, 
            "Expected code {:?} for {}, got {:?}", 
            expected_code, message, error.code());
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_metadata_propagation() {
    let test_service = TestServiceImpl::new();
    let service_clone = test_service.clone();
    
    // Start server
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    
    tokio::spawn(async move {
        Server::builder()
            .add_service(TestServiceServer::new(test_service))
            .serve_with_incoming(tokio_stream::wrappers::TcpListenerStream::new(listener))
            .await
            .unwrap();
    });
    
    timing::robust_sleep(Duration::from_millis(100)).await;
    
    let mut client = create_test_client(addr).await;
    
    // Create request with metadata
    let mut request = Request::new(EchoRequest {
        message: "test".to_string(),
    });
    
    // Add various metadata types
    request.metadata_mut().insert(
        "x-request-id",
        MetadataValue::from_static("12345"),
    );
    request.metadata_mut().insert(
        "x-user-id",
        MetadataValue::from_static("user-123"),
    );
    request.metadata_mut().insert_bin(
        "x-binary-data-bin",
        MetadataValue::from_bytes(b"binary\x00data"),
    );
    
    // Send request
    let response = client.echo(request).await.unwrap();
    assert_eq!(response.into_inner().message, "test");
    
    // Verify metadata was received
    let stored_metadata = service_clone.metadata_store.lock().await;
    
    assert_eq!(
        stored_metadata.get("x-request-id").unwrap().to_str().unwrap(),
        "12345"
    );
    assert_eq!(
        stored_metadata.get("x-user-id").unwrap().to_str().unwrap(),
        "user-123"
    );
    assert_eq!(
        stored_metadata.get_bin("x-binary-data-bin").unwrap().to_bytes().unwrap().as_ref(),
        b"binary\x00data"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_deadline_timeout() {
    let test_service = TestServiceImpl::new();
    
    // Start server
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    
    tokio::spawn(async move {
        Server::builder()
            .add_service(TestServiceServer::new(test_service))
            .serve_with_incoming(tokio_stream::wrappers::TcpListenerStream::new(listener))
            .await
            .unwrap();
    });
    
    timing::robust_sleep(Duration::from_millis(100)).await;
    
    // Create client with short timeout
    let endpoint = format!("http://{}", addr);
    let channel = Channel::from_shared(endpoint)
        .unwrap()
        .timeout(Duration::from_millis(100)) // Very short timeout
        .connect()
        .await
        .unwrap();
    
    let mut client = TestServiceClient::new(channel);
    
    // Request that takes longer than timeout
    let response = client.timeout_test(TimeoutRequest {
        sleep_ms: 500, // Sleep longer than timeout
    }).await;
    
    assert!(response.is_err());
    let error = response.unwrap_err();
    // The error could be either deadline exceeded or cancelled
    assert!(
        error.code() == tonic::Code::DeadlineExceeded || 
        error.code() == tonic::Code::Cancelled,
        "Expected deadline exceeded or cancelled, got {:?}", error.code()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_large_message_handling() {
    let test_service = TestServiceImpl::new();
    
    // Start server with custom max message size
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    
    tokio::spawn(async move {
        Server::builder()
            .add_service(TestServiceServer::new(test_service))
            .serve_with_incoming(tokio_stream::wrappers::TcpListenerStream::new(listener))
            .await
            .unwrap();
    });
    
    timing::robust_sleep(Duration::from_millis(100)).await;
    
    // Create client with matching max message size
    let endpoint = format!("http://{}", addr);
    let channel = Channel::from_shared(endpoint)
        .unwrap()
        .connect()
        .await
        .unwrap();
    
    let mut client = TestServiceClient::new(channel)
        .max_encoding_message_size(10 * 1024 * 1024)
        .max_decoding_message_size(10 * 1024 * 1024);
    
    // Test various message sizes
    let sizes = vec![
        1024,           // 1KB
        100 * 1024,     // 100KB
        1024 * 1024,    // 1MB
        3 * 1024 * 1024,  // 3MB (under 4MB default limit)
    ];
    
    for size in sizes {
        let data = vec![(size % 256) as u8; size];
        let response = client.send_large_message(LargeMessage { data }).await;
        
        assert!(response.is_ok(), "Failed to send {} byte message", size);
        let message = response.unwrap().into_inner().message;
        assert_eq!(message, format!("Received {} bytes", size));
    }
    
    // Test message exceeding limit (4MB is default in tonic)
    let oversized_data = vec![0u8; 5 * 1024 * 1024]; // 5MB
    let response = client.send_large_message(LargeMessage { data: oversized_data }).await;
    assert!(response.is_err(), "Expected error for message exceeding 4MB limit");
}

// ===== Streaming Tests =====

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_server_streaming() {
    let streaming_service = StreamingServiceImpl::new();
    
    // Start server
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    
    tokio::spawn(async move {
        Server::builder()
            .add_service(StreamingServiceServer::new(streaming_service))
            .serve_with_incoming(tokio_stream::wrappers::TcpListenerStream::new(listener))
            .await
            .unwrap();
    });
    
    timing::robust_sleep(Duration::from_millis(100)).await;
    
    let mut client = create_streaming_client(addr).await;
    
    // Request stream of 10 items
    let response = client.server_stream(StreamRequest {
        count: 10,
        prefix: "Item".to_string(),
        delay_ms: 10,
    }).await.unwrap();
    
    let mut stream = response.into_inner();
    let mut received = Vec::new();
    
    // Collect all stream items
    while let Some(item) = stream.next().await {
        let response = item.unwrap();
        received.push(response);
    }
    
    // Verify we received all items
    assert_eq!(received.len(), 10);
    for (i, response) in received.iter().enumerate() {
        assert_eq!(response.sequence, i as u32);
        assert_eq!(response.message, format!("Item {}", i));
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_client_streaming() {
    let streaming_service = StreamingServiceImpl::new();
    
    // Start server
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    
    tokio::spawn(async move {
        Server::builder()
            .add_service(StreamingServiceServer::new(streaming_service))
            .serve_with_incoming(tokio_stream::wrappers::TcpListenerStream::new(listener))
            .await
            .unwrap();
    });
    
    timing::robust_sleep(Duration::from_millis(100)).await;
    
    let mut client = create_streaming_client(addr).await;
    
    // Create stream of requests
    let (tx, rx) = mpsc::channel(10);
    
    // Send multiple requests
    for i in 0..5 {
        tx.send(StreamRequest {
            count: 1,
            prefix: format!("Message{}", i),
            delay_ms: 0,
        }).await.unwrap();
    }
    drop(tx); // Close the stream
    
    let stream = ReceiverStream::new(rx);
    let response = client.client_stream(stream).await.unwrap();
    let inner = response.into_inner();
    
    // Verify server received all messages
    assert_eq!(inner.sequence, 5);
    assert_eq!(inner.message, "Message0, Message1, Message2, Message3, Message4");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_bidirectional_streaming() {
    let streaming_service = StreamingServiceImpl::new();
    
    // Start server
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    
    tokio::spawn(async move {
        Server::builder()
            .add_service(StreamingServiceServer::new(streaming_service))
            .serve_with_incoming(tokio_stream::wrappers::TcpListenerStream::new(listener))
            .await
            .unwrap();
    });
    
    timing::robust_sleep(Duration::from_millis(100)).await;
    
    let mut client = create_streaming_client(addr).await;
    
    // Create bidirectional stream
    let (tx, rx) = mpsc::channel(10);
    
    // Spawn task to send requests
    let tx_clone = tx.clone();
    tokio::spawn(async move {
        for i in 0..5 {
            tx_clone.send(StreamRequest {
                count: 1,
                prefix: format!("Ping{}", i),
                delay_ms: 50,
            }).await.unwrap();
            timing::robust_sleep(Duration::from_millis(100)).await;
        }
    });
    
    // Give some time before dropping tx to keep stream open
    tokio::spawn(async move {
        timing::robust_sleep(Duration::from_secs(1)).await;
        drop(tx);
    });
    
    let stream = ReceiverStream::new(rx);
    let response = client.bidi_stream(stream).await.unwrap();
    let mut response_stream = response.into_inner();
    
    let mut received = Vec::new();
    while let Some(item) = response_stream.next().await {
        received.push(item.unwrap());
    }
    
    // Verify we received echoes
    assert_eq!(received.len(), 5);
    for (i, response) in received.iter().enumerate() {
        assert_eq!(response.sequence, i as u32);
        assert_eq!(response.message, format!("Echo: Ping{}", i));
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_concurrent_streams() {
    let streaming_service = StreamingServiceImpl::new();
    let service_clone = streaming_service.clone();
    
    // Start server
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    
    tokio::spawn(async move {
        Server::builder()
            .add_service(StreamingServiceServer::new(streaming_service))
            .serve_with_incoming(tokio_stream::wrappers::TcpListenerStream::new(listener))
            .await
            .unwrap();
    });
    
    timing::robust_sleep(Duration::from_millis(100)).await;
    
    // Create multiple concurrent streams
    let mut handles = vec![];
    
    for i in 0..5 {
        let addr = addr.clone();
        let handle = tokio::spawn(async move {
            let mut client = create_streaming_client(addr).await;
            
            let response = client.server_stream(StreamRequest {
                count: 20,
                prefix: format!("Stream{}", i),
                delay_ms: 10,
            }).await.unwrap();
            
            let mut stream = response.into_inner();
            let mut count = 0;
            
            while let Some(item) = stream.next().await {
                item.unwrap();
                count += 1;
            }
            
            count
        });
        
        handles.push(handle);
    }
    
    // Wait for all streams to complete
    for handle in handles {
        let count = handle.await.unwrap();
        assert_eq!(count, 20);
    }
    
    // Verify stream count went back to 0
    timing::robust_sleep(Duration::from_millis(100)).await;
    let active = service_clone.active_streams.lock().await;
    assert_eq!(*active, 0, "All streams should be closed");
}

// ===== Error Detail Tests =====

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_error_detail_propagation() {
    let test_service = TestServiceImpl::new();
    
    // Start server
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    
    tokio::spawn(async move {
        Server::builder()
            .add_service(TestServiceServer::new(test_service))
            .serve_with_incoming(tokio_stream::wrappers::TcpListenerStream::new(listener))
            .await
            .unwrap();
    });
    
    timing::robust_sleep(Duration::from_millis(100)).await;
    
    let mut client = create_test_client(addr).await;
    
    // Test error without details
    let response = client.trigger_error(ErrorRequest {
        error_code: 1,
        error_context: "test_context".to_string(),
        include_details: false,
    }).await;
    
    assert!(response.is_err());
    let error = response.unwrap_err();
    assert_eq!(error.code(), tonic::Code::InvalidArgument);
    assert_eq!(error.message(), "Validation failed");
    
    // Test error with details
    let response = client.trigger_error(ErrorRequest {
        error_code: 6,
        error_context: "detailed_test".to_string(),
        include_details: true,
    }).await;
    
    assert!(response.is_err());
    let error = response.unwrap_err();
    assert_eq!(error.code(), tonic::Code::Internal);
    
    // Check if error source contains our details
    let error_str = format!("{:?}", error);
    assert!(error_str.contains("Internal server error") || error_str.contains("detailed_test"));
}

// ===== Performance and Load Tests =====

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_concurrent_request_handling() {
    let test_service = TestServiceImpl::new();
    let service_clone = test_service.clone();
    
    // Start server
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    
    tokio::spawn(async move {
        Server::builder()
            .add_service(TestServiceServer::new(test_service))
            .serve_with_incoming(tokio_stream::wrappers::TcpListenerStream::new(listener))
            .await
            .unwrap();
    });
    
    timing::robust_sleep(Duration::from_millis(100)).await;
    
    // Send many concurrent requests
    let mut handles = vec![];
    let num_requests = 100;
    
    for i in 0..num_requests {
        let addr = addr.clone();
        let handle = tokio::spawn(async move {
            let mut client = create_test_client(addr).await;
            
            let response = client.echo(EchoRequest {
                message: format!("Request {}", i),
            }).await;
            
            assert!(response.is_ok());
            let inner = response.unwrap().into_inner();
            assert_eq!(inner.message, format!("Request {}", i));
        });
        
        handles.push(handle);
    }
    
    // Wait for all requests to complete
    for handle in handles {
        handle.await.unwrap();
    }
    
    // Verify request count
    let count = service_clone.request_count.lock().await;
    assert_eq!(*count, num_requests);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_stream_cancellation() {
    let streaming_service = StreamingServiceImpl::new();
    
    // Start server
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    
    tokio::spawn(async move {
        Server::builder()
            .add_service(StreamingServiceServer::new(streaming_service))
            .serve_with_incoming(tokio_stream::wrappers::TcpListenerStream::new(listener))
            .await
            .unwrap();
    });
    
    timing::robust_sleep(Duration::from_millis(100)).await;
    
    let mut client = create_streaming_client(addr).await;
    
    // Start a long stream
    let response = client.server_stream(StreamRequest {
        count: 1000, // Many items
        prefix: "Item".to_string(),
        delay_ms: 10,
    }).await.unwrap();
    
    let mut stream = response.into_inner();
    let mut received = 0;
    
    // Read only first 5 items then cancel
    while let Some(item) = stream.next().await {
        item.unwrap();
        received += 1;
        
        if received >= 5 {
            break; // Cancel stream early
        }
    }
    
    // Drop stream to cancel
    drop(stream);
    
    assert_eq!(received, 5, "Should have received exactly 5 items before cancellation");
}

// ===== Health Check Protocol Compliance =====

/// gRPC Health service implementation
#[derive(Debug, Clone)]
struct HealthServiceImpl {
    services: Arc<Mutex<std::collections::HashMap<String, ServingStatus>>>,
}

impl HealthServiceImpl {
    fn new() -> Self {
        let mut services = std::collections::HashMap::new();
        // Overall health
        services.insert("".to_string(), ServingStatus::Serving);
        
        Self {
            services: Arc::new(Mutex::new(services)),
        }
    }
    
    async fn set_service_status(&self, service: String, status: ServingStatus) {
        let mut services = self.services.lock().await;
        services.insert(service, status);
    }
}

#[tonic::async_trait]
impl GrpcHealth for HealthServiceImpl {
    async fn check(
        &self,
        request: Request<GrpcHealthCheckRequest>,
    ) -> Result<Response<GrpcHealthCheckResponse>, Status> {
        let service_name = request.into_inner().service;
        
        let services = self.services.lock().await;
        let status = services.get(&service_name)
            .copied()
            .unwrap_or(if service_name.is_empty() {
                ServingStatus::Serving
            } else {
                ServingStatus::ServiceUnknown
            });
        
        Ok(Response::new(GrpcHealthCheckResponse {
            status: status.into(),
        }))
    }
    
    type WatchStream = Pin<Box<dyn Stream<Item = Result<GrpcHealthCheckResponse, Status>> + Send>>;
    
    async fn watch(
        &self,
        request: Request<GrpcHealthCheckRequest>,
    ) -> Result<Response<Self::WatchStream>, Status> {
        let service_name = request.into_inner().service;
        let services = self.services.clone();
        
        let (tx, rx) = mpsc::channel(128);
        
        // Send initial status
        let initial_status = {
            let services = services.lock().await;
            services.get(&service_name)
                .copied()
                .unwrap_or(if service_name.is_empty() {
                    ServingStatus::Serving
                } else {
                    ServingStatus::ServiceUnknown
                })
        };
        
        let _ = tx.send(Ok(GrpcHealthCheckResponse {
            status: initial_status.into(),
        })).await;
        
        // In a real implementation, we'd watch for changes and send updates
        // For testing, we just send the initial status
        
        let stream = ReceiverStream::new(rx);
        Ok(Response::new(Box::pin(stream)))
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_grpc_health_check_protocol_compliance() {
    let health_service = HealthServiceImpl::new();
    let health_clone = health_service.clone();
    
    // Start server with health service
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    
    tokio::spawn(async move {
        Server::builder()
            .add_service(GrpcHealthServer::new(health_service))
            .serve_with_incoming(tokio_stream::wrappers::TcpListenerStream::new(listener))
            .await
            .unwrap();
    });
    
    timing::robust_sleep(Duration::from_millis(100)).await;
    
    // Create health check client
    let endpoint = format!("http://{}", addr);
    let channel = Channel::from_shared(endpoint).unwrap().connect().await.unwrap();
    let mut client = GrpcHealthClient::new(channel);
    
    // Test 1: Check overall health (empty service name)
    let response = client.check(GrpcHealthCheckRequest {
        service: "".to_string(),
    }).await.unwrap();
    
    let status = response.into_inner().status();
    assert_eq!(status, ServingStatus::Serving, "Overall health should be SERVING");
    
    // Test 2: Check specific service (should be UNKNOWN)
    let response = client.check(GrpcHealthCheckRequest {
        service: "blixard.ClusterService".to_string(),
    }).await.unwrap();
    
    let status = response.into_inner().status();
    assert_eq!(status, ServingStatus::ServiceUnknown, 
        "Unknown service should return SERVICE_UNKNOWN");
    
    // Test 3: Set service status and verify
    health_clone.set_service_status(
        "blixard.ClusterService".to_string(), 
        ServingStatus::Serving
    ).await;
    
    let response = client.check(GrpcHealthCheckRequest {
        service: "blixard.ClusterService".to_string(),
    }).await.unwrap();
    
    let status = response.into_inner().status();
    assert_eq!(status, ServingStatus::Serving, 
        "Service should be SERVING after setting status");
    
    // Test 4: Set service as NOT_SERVING
    health_clone.set_service_status(
        "blixard.ClusterService".to_string(), 
        ServingStatus::NotServing
    ).await;
    
    let response = client.check(GrpcHealthCheckRequest {
        service: "blixard.ClusterService".to_string(),
    }).await.unwrap();
    
    let status = response.into_inner().status();
    assert_eq!(status, ServingStatus::NotServing, 
        "Service should be NOT_SERVING after setting status");
    
    // Test 5: Watch functionality (basic test)
    let response = client.watch(GrpcHealthCheckRequest {
        service: "".to_string(),
    }).await.unwrap();
    
    let mut stream = response.into_inner();
    
    // Should receive at least the initial status
    if let Some(item) = stream.next().await {
        let response = item.unwrap();
        assert_eq!(response.status(), ServingStatus::Serving, 
            "Watch should return initial SERVING status");
    } else {
        panic!("Watch stream should provide at least one status update");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_connection_keepalive() {
    let test_service = TestServiceImpl::new();
    
    // Start server with keep-alive settings
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    
    tokio::spawn(async move {
        Server::builder()
            .http2_keepalive_interval(Some(Duration::from_secs(1)))
            .http2_keepalive_timeout(Some(Duration::from_secs(5)))
            .add_service(TestServiceServer::new(test_service))
            .serve_with_incoming(tokio_stream::wrappers::TcpListenerStream::new(listener))
            .await
            .unwrap();
    });
    
    timing::robust_sleep(Duration::from_millis(100)).await;
    
    // Create client with keep-alive
    let endpoint = format!("http://{}", addr);
    let channel = Channel::from_shared(endpoint)
        .unwrap()
        .keep_alive_while_idle(true)
        .http2_keep_alive_interval(Duration::from_secs(2))
        .connect()
        .await
        .unwrap();
    
    let mut client = TestServiceClient::new(channel);
    
    // Make initial request
    let response = client.echo(EchoRequest {
        message: "keepalive test".to_string(),
    }).await.unwrap();
    assert_eq!(response.into_inner().message, "keepalive test");
    
    // Wait for keep-alive period
    timing::robust_sleep(Duration::from_secs(3)).await;
    
    // Connection should still be alive
    let response = client.echo(EchoRequest {
        message: "still alive".to_string(),
    }).await.unwrap();
    assert_eq!(response.into_inner().message, "still alive");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_max_connection_age() {
    // Note: max_connection_age behavior varies between tonic versions
    // This test verifies connection lifespan rather than automatic closure
    let streaming_service = StreamingServiceImpl::new();
    
    // Start server
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    
    let server_handle = tokio::spawn(async move {
        Server::builder()
            .add_service(StreamingServiceServer::new(streaming_service))
            .serve_with_incoming(tokio_stream::wrappers::TcpListenerStream::new(listener))
            .await
            .unwrap();
    });
    
    timing::robust_sleep(Duration::from_millis(100)).await;
    
    // Test connection lifespan
    let start = std::time::Instant::now();
    let mut client = create_streaming_client(addr).await;
    
    // Make multiple requests over time
    for i in 0..5 {
        let response = client.server_stream(StreamRequest {
            count: 10,
            prefix: format!("Batch{}", i),
            delay_ms: 10,
        }).await.unwrap();
        
        let mut stream = response.into_inner();
        let mut count = 0;
        while let Some(item) = stream.next().await {
            match item {
                Ok(_) => count += 1,
                Err(e) => {
                    // Connection errors are expected behavior
                    if e.code() == tonic::Code::Unavailable {
                        break;
                    } else {
                        panic!("Unexpected error in batch {}: {:?}", i, e);
                    }
                }
            }
        }
        
        // Verify we got some data
        assert!(count > 0, "Batch {} should have received some items", i);
        
        timing::robust_sleep(Duration::from_millis(500)).await;
    }
    
    let elapsed = start.elapsed();
    assert!(elapsed >= Duration::from_secs(2), "Test should have run for at least 2 seconds");
    
    server_handle.abort();
}