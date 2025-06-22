//! Tests for PeerConnector component
//!
//! This module tests the peer connection management including:
//! - Connection lifecycle (connect, disconnect, reconnect)
//! - Message buffering and delivery
//! - Concurrent access patterns
//! - Error handling and resilience
//! - Connection state tracking

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{Mutex, RwLock};
use tonic::{Request, Response, Status};
use blixard_core::test_helpers::timing;

use blixard_core::node_shared::SharedNodeState;
use blixard_core::peer_connector::PeerConnector;
use blixard_core::types::NodeConfig;
use blixard_core::proto::{
    cluster_service_server::{ClusterService, ClusterServiceServer},
    JoinRequest, JoinResponse,
    LeaveRequest, LeaveResponse,
    RaftMessageRequest, RaftMessageResponse,
    ClusterStatusRequest, ClusterStatusResponse,
    CreateVmRequest, CreateVmResponse,
    StartVmRequest, StartVmResponse,
    StopVmRequest, StopVmResponse,
    ListVmsRequest, ListVmsResponse,
    GetVmStatusRequest, GetVmStatusResponse,
    HealthCheckRequest, HealthCheckResponse,
    TaskRequest, TaskResponse,
    TaskStatusRequest, TaskStatusResponse,
};


/// Mock gRPC server for testing peer connections
#[derive(Debug, Clone)]
struct MockClusterService {
    node_id: u64,
    /// Track received Raft messages
    received_messages: Arc<Mutex<Vec<raft::prelude::Message>>>,
    /// Control response behavior
    fail_responses: Arc<RwLock<bool>>,
    /// Simulate network delays
    response_delay_ms: Arc<RwLock<u64>>,
    /// Track connection attempts
    connection_count: Arc<Mutex<u32>>,
}

impl MockClusterService {
    fn new(node_id: u64) -> Self {
        Self {
            node_id,
            received_messages: Arc::new(Mutex::new(Vec::new())),
            fail_responses: Arc::new(RwLock::new(false)),
            response_delay_ms: Arc::new(RwLock::new(0)),
            connection_count: Arc::new(Mutex::new(0)),
        }
    }

    async fn get_received_messages(&self) -> Vec<raft::prelude::Message> {
        self.received_messages.lock().await.clone()
    }

    async fn clear_received_messages(&self) {
        self.received_messages.lock().await.clear();
    }

    async fn set_fail_responses(&self, fail: bool) {
        *self.fail_responses.write().await = fail;
    }

    async fn set_response_delay(&self, delay_ms: u64) {
        *self.response_delay_ms.write().await = delay_ms;
    }

    #[allow(dead_code)]
    async fn get_connection_count(&self) -> u32 {
        *self.connection_count.lock().await
    }
}

#[tonic::async_trait]
impl ClusterService for MockClusterService {
    async fn join_cluster(
        &self,
        _request: Request<JoinRequest>,
    ) -> Result<Response<JoinResponse>, Status> {
        Ok(Response::new(JoinResponse {
            success: true,
            message: String::new(),
            peers: vec![],
            voters: vec![],
        }))
    }

    async fn leave_cluster(
        &self,
        _request: Request<LeaveRequest>,
    ) -> Result<Response<LeaveResponse>, Status> {
        Ok(Response::new(LeaveResponse {
            success: true,
            message: String::new(),
        }))
    }

    async fn send_raft_message(
        &self,
        request: Request<RaftMessageRequest>,
    ) -> Result<Response<RaftMessageResponse>, Status> {
        // Track connection
        {
            let mut count = self.connection_count.lock().await;
            *count += 1;
        }

        // Simulate delay if configured
        let delay = *self.response_delay_ms.read().await;
        if delay > 0 {
            timing::robust_sleep(Duration::from_millis(delay)).await;
        }

        // Check if we should fail
        if *self.fail_responses.read().await {
            return Err(Status::unavailable("Mock service configured to fail"));
        }

        // Deserialize and store the message
        let message = match blixard_core::raft_codec::deserialize_message(&request.into_inner().raft_data) {
            Ok(msg) => msg,
            Err(e) => {
                return Ok(Response::new(RaftMessageResponse {
                    success: false,
                    error: format!("Failed to deserialize message: {}", e),
                }));
            }
        };

        self.received_messages.lock().await.push(message);

        Ok(Response::new(RaftMessageResponse {
            success: true,
            error: String::new(),
        }))
    }

    async fn get_cluster_status(
        &self,
        _request: Request<ClusterStatusRequest>,
    ) -> Result<Response<ClusterStatusResponse>, Status> {
        Ok(Response::new(ClusterStatusResponse {
            leader_id: self.node_id,
            nodes: vec![],
            term: 0,
        }))
    }

    async fn submit_task(
        &self,
        _request: Request<TaskRequest>,
    ) -> Result<Response<TaskResponse>, Status> {
        Ok(Response::new(TaskResponse {
            accepted: true,
            message: String::new(),
            assigned_node: self.node_id,
        }))
    }

    async fn get_task_status(
        &self,
        _request: Request<TaskStatusRequest>,
    ) -> Result<Response<TaskStatusResponse>, Status> {
        Ok(Response::new(TaskStatusResponse {
            found: false,
            status: 0,
            output: String::new(),
            error: String::new(),
            execution_time_ms: 0,
        }))
    }

    async fn create_vm(
        &self,
        _request: Request<CreateVmRequest>,
    ) -> Result<Response<CreateVmResponse>, Status> {
        Ok(Response::new(CreateVmResponse {
            success: true,
            message: String::new(),
            vm_id: String::new(),
        }))
    }

    async fn start_vm(
        &self,
        _request: Request<StartVmRequest>,
    ) -> Result<Response<StartVmResponse>, Status> {
        Ok(Response::new(StartVmResponse {
            success: true,
            message: String::new(),
        }))
    }

    async fn stop_vm(
        &self,
        _request: Request<StopVmRequest>,
    ) -> Result<Response<StopVmResponse>, Status> {
        Ok(Response::new(StopVmResponse {
            success: true,
            message: String::new(),
        }))
    }

    async fn list_vms(
        &self,
        _request: Request<ListVmsRequest>,
    ) -> Result<Response<ListVmsResponse>, Status> {
        Ok(Response::new(ListVmsResponse {
            vms: vec![],
        }))
    }

    async fn get_vm_status(
        &self,
        _request: Request<GetVmStatusRequest>,
    ) -> Result<Response<GetVmStatusResponse>, Status> {
        Ok(Response::new(GetVmStatusResponse {
            found: false,
            vm_info: None,
        }))
    }

    async fn health_check(
        &self,
        _request: Request<HealthCheckRequest>,
    ) -> Result<Response<HealthCheckResponse>, Status> {
        // Check if we should fail
        if *self.fail_responses.read().await {
            return Err(Status::unavailable("Mock service configured to fail"));
        }
        
        Ok(Response::new(HealthCheckResponse {
            healthy: true,
            message: String::new(),
        }))
    }
}

/// Test harness for PeerConnector tests
struct TestHarness {
    _node_id: u64,
    shared_state: Arc<SharedNodeState>,
    peer_connector: Arc<PeerConnector>,
    mock_servers: Arc<RwLock<HashMap<u64, (MockClusterService, tokio::task::JoinHandle<()>)>>>,
}

impl TestHarness {
    async fn new(node_id: u64) -> Self {
        let config = NodeConfig {
            id: node_id,
            data_dir: format!("/tmp/blixard-test-{}", node_id),
            bind_addr: "127.0.0.1:0".parse().unwrap(),
            join_addr: None,
            use_tailscale: false,
        };
        let shared_state = Arc::new(SharedNodeState::new(config));
        let peer_connector = Arc::new(PeerConnector::new(shared_state.clone()));
        
        Self {
            _node_id: node_id,
            shared_state,
            peer_connector,
            mock_servers: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Start a mock server for a peer
    async fn start_mock_peer(&self, peer_id: u64, addr: SocketAddr) -> MockClusterService {
        let service = MockClusterService::new(peer_id);
        
        let server = ClusterServiceServer::new(service.clone());
        let handle = tokio::spawn(async move {
            let _ = tonic::transport::Server::builder()
                .add_service(server)
                .serve(addr)
                .await;
        });

        // Wait for server to be ready
        timing::wait_for_condition_with_backoff(
            || async {
                // Try to connect to verify server is ready
                tokio::net::TcpStream::connect(addr).await.is_ok()
            },
            Duration::from_secs(5),
            Duration::from_millis(50),
        ).await.expect("Mock server should start");

        // Add peer to shared state if it doesn't exist yet
        if self.shared_state.get_peer(peer_id).await.is_none() {
            self.shared_state.add_peer(peer_id, addr.to_string()).await.unwrap();
        }

        let mut servers = self.mock_servers.write().await;
        servers.insert(peer_id, (service.clone(), handle));
        service
    }

    /// Stop a mock server
    #[allow(dead_code)]
    async fn stop_mock_peer(&self, peer_id: u64) {
        let mut servers = self.mock_servers.write().await;
        if let Some((_, handle)) = servers.remove(&peer_id) {
            handle.abort();
        }
    }

    /// Get mock service for a peer
    #[allow(dead_code)]
    async fn get_mock_service(&self, peer_id: u64) -> Option<MockClusterService> {
        let servers = self.mock_servers.read().await;
        servers.get(&peer_id).map(|(service, _)| service.clone())
    }

    /// Cleanup all resources
    async fn cleanup(self) {
        let mut servers = self.mock_servers.write().await;
        for (_, (_, handle)) in servers.drain() {
            handle.abort();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use raft::prelude::*;

    /// Test basic connection lifecycle: connect -> send -> disconnect
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_connection_lifecycle() {
        let harness = TestHarness::new(1).await;
        
        // Start a mock peer
        let peer_addr = get_available_addr().await;
        let mock_service = harness.start_mock_peer(2, peer_addr).await;
        
        // Get peer info
        let peer = harness.shared_state.get_peer(2).await.unwrap();
        
        // Connect to peer
        harness.peer_connector.connect_to_peer(&peer).await.unwrap();
        
        // Verify connection status
        assert!(harness.shared_state.get_peer(2).await.unwrap().is_connected);
        
        // Send a message
        let message = create_test_message(1, 2);
        harness.peer_connector.send_raft_message(2, message.clone()).await.unwrap();
        
        // Wait for message to be received
        timing::wait_for_condition_with_backoff(
            || async {
                mock_service.get_received_messages().await.len() > 0
            },
            Duration::from_secs(5),
            Duration::from_millis(50),
        ).await.expect("Message should be received");
        
        let received = mock_service.get_received_messages().await;
        assert_eq!(received.len(), 1);
        assert_eq!(received[0].from, 1);
        assert_eq!(received[0].to, 2);
        
        // Disconnect
        harness.peer_connector.disconnect_from_peer(2).await.unwrap();
        
        // Verify disconnection
        assert!(!harness.shared_state.get_peer(2).await.unwrap().is_connected);
        
        harness.cleanup().await;
    }

    /// Test connection failure handling
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_connection_failure() {
        let harness = TestHarness::new(1).await;
        
        // Add peer with invalid address
        harness.shared_state.add_peer(2, "invalid:address:999999".to_string()).await.unwrap();
        
        // Get peer info for connection attempt
        let peer_info = harness.shared_state.get_peer(2).await.unwrap();
        
        // Try to connect - should fail
        let result = harness.peer_connector.connect_to_peer(&peer_info).await;
        assert!(result.is_err());
        
        // Verify peer is marked as disconnected
        assert!(!harness.shared_state.get_peer(2).await.unwrap().is_connected);
        
        harness.cleanup().await;
    }

    /// Test duplicate connection attempts
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_duplicate_connection_attempts() {
        let harness = TestHarness::new(1).await;
        
        // Start a mock peer with delay
        let peer_addr = get_available_addr().await;
        let mock_service = harness.start_mock_peer(2, peer_addr).await;
        mock_service.set_response_delay(200).await; // Slow connection
        
        let peer = harness.shared_state.get_peer(2).await.unwrap();
        
        // Start multiple connection attempts concurrently
        let connector1 = harness.peer_connector.clone();
        let connector2 = harness.peer_connector.clone();
        let peer1 = peer.clone();
        let peer2 = peer.clone();
        
        let (result1, result2) = tokio::join!(
            connector1.connect_to_peer(&peer1),
            connector2.connect_to_peer(&peer2)
        );
        
        // Both should succeed (one should detect duplicate and return early)
        assert!(result1.is_ok());
        assert!(result2.is_ok());
        
        // Only one actual connection should be made
        assert!(harness.peer_connector.get_connection(2).await.is_some());
        
        harness.cleanup().await;
    }

    /// Test message buffering when peer is disconnected
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_message_buffering() {
        let harness = TestHarness::new(1).await;
        
        // Start a mock peer but don't connect yet
        let peer_addr = get_available_addr().await;
        let mock_service = harness.start_mock_peer(2, peer_addr).await;
        
        // Send messages before connecting - should be buffered
        let message1 = create_test_message(1, 2);
        let message2 = create_test_message_with_type(1, 2, MessageType::MsgHeartbeat);
        
        // These should succeed (messages get buffered)
        harness.peer_connector.send_raft_message(2, message1.clone()).await.unwrap();
        harness.peer_connector.send_raft_message(2, message2.clone()).await.unwrap();
        
        // Wait for async connection and message delivery
        timing::wait_for_condition_with_backoff(
            || async {
                mock_service.get_received_messages().await.len() >= 2
            },
            Duration::from_secs(5),
            Duration::from_millis(50),
        ).await.expect("Messages should be delivered after connection");
        
        // Messages should have been delivered
        let received = mock_service.get_received_messages().await;
        assert_eq!(received.len(), 2);
        assert_eq!(received[0].msg_type(), MessageType::MsgPropose);
        assert_eq!(received[1].msg_type(), MessageType::MsgHeartbeat);
        
        harness.cleanup().await;
    }

    /// Test buffer overflow protection
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_buffer_overflow_protection() {
        let harness = TestHarness::new(1).await;
        
        // Add peer but don't start server (so connection will fail)
        harness.shared_state.add_peer(2, "127.0.0.1:59999".to_string()).await.unwrap();
        
        // Send more than MAX_BUFFERED_MESSAGES (100)
        for i in 0..150 {
            let mut message = create_test_message(1, 2);
            message.index = i;
            let _ = harness.peer_connector.send_raft_message(2, message).await;
        }
        
        // Buffer should not grow beyond limit
        // We can't directly inspect the buffer, but the system should not crash
        // and memory usage should be bounded
        
        harness.cleanup().await;
    }

    /// Test concurrent message sending
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_concurrent_message_sending() {
        let harness = TestHarness::new(1).await;
        
        // Start a mock peer
        let peer_addr = get_available_addr().await;
        let mock_service = harness.start_mock_peer(2, peer_addr).await;
        
        // Connect to peer
        let peer = harness.shared_state.get_peer(2).await.unwrap();
        harness.peer_connector.connect_to_peer(&peer).await.unwrap();
        
        // Send multiple messages concurrently
        let mut handles = vec![];
        for i in 0..10 {
            let connector = harness.peer_connector.clone();
            let handle = tokio::spawn(async move {
                let mut message = create_test_message(1, 2);
                message.index = i;
                connector.send_raft_message(2, message).await
            });
            handles.push(handle);
        }
        
        // Wait for all sends to complete
        for handle in handles {
            assert!(handle.await.unwrap().is_ok());
        }
        
        // Wait for all messages to be received
        timing::wait_for_condition_with_backoff(
            || async {
                mock_service.get_received_messages().await.len() >= 10
            },
            Duration::from_secs(5),
            Duration::from_millis(50),
        ).await.expect("All concurrent messages should be received");
        
        let received = mock_service.get_received_messages().await;
        assert_eq!(received.len(), 10);
        
        // Check that all indices are present (order may vary)
        let mut indices: Vec<u64> = received.iter().map(|m| m.index).collect();
        indices.sort();
        assert_eq!(indices, vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9]);
        
        harness.cleanup().await;
    }

    /// Test automatic reconnection on send failure
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_automatic_reconnection() {
        let harness = TestHarness::new(1).await;
        
        // Start a mock peer
        let peer_addr = get_available_addr().await;
        let mock_service = harness.start_mock_peer(2, peer_addr).await;
        
        // Connect to peer
        let peer = harness.shared_state.get_peer(2).await.unwrap();
        harness.peer_connector.connect_to_peer(&peer).await.unwrap();
        
        // Configure mock to fail responses
        mock_service.set_fail_responses(true).await;
        
        // Send a message - should fail and trigger reconnection
        let message = create_test_message(1, 2);
        let result = harness.peer_connector.send_raft_message(2, message.clone()).await;
        assert!(result.is_err());
        
        // Fix the mock service
        mock_service.set_fail_responses(false).await;
        
        // Wait for automatic reconnection
        timing::wait_for_condition_with_backoff(
            || async {
                harness.peer_connector.get_connection(2).await.is_some()
            },
            Duration::from_secs(5),
            Duration::from_millis(100),
        ).await.expect("Connection should be re-established");
        
        // Clear any buffered messages that were sent during reconnection
        mock_service.clear_received_messages().await;
        
        // Send another message - should succeed now
        let message2 = create_test_message_with_type(1, 2, MessageType::MsgHeartbeat);
        harness.peer_connector.send_raft_message(2, message2.clone()).await.unwrap();
        
        // Wait for message to be received
        timing::wait_for_condition_with_backoff(
            || async {
                mock_service.get_received_messages().await.len() > 0
            },
            Duration::from_secs(5),
            Duration::from_millis(50),
        ).await.expect("Message should be received after reconnection");
        
        let received = mock_service.get_received_messages().await;
        assert_eq!(received.len(), 1);
        assert_eq!(received[0].msg_type(), MessageType::MsgHeartbeat);
        
        harness.cleanup().await;
    }

    /// Test connection maintenance background task
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_connection_maintenance() {
        let harness = TestHarness::new(1).await;
        
        // Start maintenance task
        harness.peer_connector.clone().start_connection_maintenance().await;
        
        // Get a port but don't start a server
        let peer_addr = get_available_addr().await;
        
        // Add a peer that's not listening
        harness.shared_state.add_peer(2, peer_addr.to_string()).await.unwrap();
        
        // Wait for maintenance task to attempt connection (it will fail)
        timing::robust_sleep(Duration::from_secs(6)).await;
        
        // Should not be connected
        assert!(!harness.shared_state.get_peer(2).await.unwrap().is_connected);
        
        // Now start the mock server
        let _mock_service = harness.start_mock_peer(2, peer_addr).await;
        
        // Wait for maintenance to connect
        timing::wait_for_condition_with_backoff(
            || async {
                harness.shared_state.get_peer(2).await.map(|p| p.is_connected).unwrap_or(false)
            },
            Duration::from_secs(10),
            Duration::from_millis(500),
        ).await.expect("Maintenance should establish connection");
        
        // Should be connected now
        assert!(harness.shared_state.get_peer(2).await.unwrap().is_connected);
        
        harness.cleanup().await;
    }

    /// Test exponential backoff on connection failures
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_exponential_backoff() {
        let harness = TestHarness::new(1).await;
        
        // Add peer with invalid address
        harness.shared_state.add_peer(2, "127.0.0.1:59999".to_string()).await.unwrap();
        let peer = harness.shared_state.get_peer(2).await.unwrap();
        
        // First attempt should work
        let start = std::time::Instant::now();
        let result1 = harness.peer_connector.connect_to_peer(&peer).await;
        assert!(result1.is_err());
        let _elapsed1 = start.elapsed();
        
        // Second attempt should be rejected immediately (in backoff)
        let start = std::time::Instant::now();
        let result2 = harness.peer_connector.connect_to_peer(&peer).await;
        assert!(result2.is_err());
        let elapsed2 = start.elapsed();
        assert!(elapsed2 < Duration::from_millis(50)); // Should fail fast
        
        // Wait for backoff period
        timing::robust_sleep(Duration::from_millis(150)).await;
        
        // Third attempt should work (after backoff)
        let result3 = harness.peer_connector.connect_to_peer(&peer).await;
        assert!(result3.is_err()); // Still fails to connect, but attempt is made
        
        harness.cleanup().await;
    }
    
    /// Test backoff reset on successful connection
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_backoff_reset_on_success() {
        let harness = TestHarness::new(1).await;
        
        // Add peer but don't start server
        let peer_addr = get_available_addr().await;
        harness.shared_state.add_peer(2, peer_addr.to_string()).await.unwrap();
        let peer = harness.shared_state.get_peer(2).await.unwrap();
        
        // Fail a few times to build up backoff
        for _ in 0..3 {
            let _ = harness.peer_connector.connect_to_peer(&peer).await;
            timing::robust_sleep(Duration::from_millis(50)).await;
        }
        
        // Start the server
        let _mock_service = harness.start_mock_peer(2, peer_addr).await;
        
        // Wait for backoff and connect
        timing::robust_sleep(Duration::from_millis(500)).await;
        harness.peer_connector.connect_to_peer(&peer).await.unwrap();
        
        // Disconnect
        harness.peer_connector.disconnect_from_peer(2).await.unwrap();
        
        // Next connection attempt should not have backoff
        let start = std::time::Instant::now();
        harness.peer_connector.connect_to_peer(&peer).await.unwrap();
        let elapsed = start.elapsed();
        assert!(elapsed < Duration::from_millis(200)); // Should connect quickly
        
        harness.cleanup().await;
    }
    
    /// Test connection health checking
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_connection_health_check() {
        let harness = TestHarness::new(1).await;
        
        // Start a mock peer
        let peer_addr = get_available_addr().await;
        let mock_service = harness.start_mock_peer(2, peer_addr).await;
        
        // Connect to peer
        let peer = harness.shared_state.get_peer(2).await.unwrap();
        harness.peer_connector.connect_to_peer(&peer).await.unwrap();
        
        // Health check should succeed
        assert!(harness.peer_connector.check_connection_health(2).await);
        
        // Configure mock to fail health checks
        mock_service.set_fail_responses(true).await;
        
        // Health check should fail
        assert!(!harness.peer_connector.check_connection_health(2).await);
        
        // Fix the mock
        mock_service.set_fail_responses(false).await;
        
        // Health check should succeed again
        assert!(harness.peer_connector.check_connection_health(2).await);
        
        harness.cleanup().await;
    }
    
    /// Test automatic disconnection of unhealthy connections
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_unhealthy_connection_disconnection() {
        let harness = TestHarness::new(1).await;
        
        // Start a mock peer
        let peer_addr = get_available_addr().await;
        let mock_service = harness.start_mock_peer(2, peer_addr).await;
        
        // Connect to peer
        let peer = harness.shared_state.get_peer(2).await.unwrap();
        harness.peer_connector.connect_to_peer(&peer).await.unwrap();
        
        // Start maintenance (includes health checking)
        harness.peer_connector.clone().start_connection_maintenance().await;
        
        // Verify connected
        assert!(harness.shared_state.get_peer(2).await.unwrap().is_connected);
        
        // Configure mock to fail health checks
        mock_service.set_fail_responses(true).await;
        
        // Wait for health check to detect and disconnect
        timing::wait_for_condition_with_backoff(
            || async {
                !harness.shared_state.get_peer(2).await.map(|p| p.is_connected).unwrap_or(true)
            },
            Duration::from_secs(15),
            Duration::from_millis(500),
        ).await.expect("Unhealthy connection should be disconnected");
        
        // Should be disconnected now
        assert!(!harness.shared_state.get_peer(2).await.unwrap().is_connected);
        
        // Fix the mock
        mock_service.set_fail_responses(false).await;
        
        // Wait for reconnection
        timing::wait_for_condition_with_backoff(
            || async {
                harness.shared_state.get_peer(2).await.map(|p| p.is_connected).unwrap_or(false)
            },
            Duration::from_secs(10),
            Duration::from_millis(500),
        ).await.expect("Connection should be re-established after fixing mock");
        
        // Should be reconnected
        assert!(harness.shared_state.get_peer(2).await.unwrap().is_connected);
        
        harness.cleanup().await;
    }
    
    /// Test circuit breaker pattern
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_circuit_breaker() {
        let harness = TestHarness::new(1).await;
        
        // Add peer with invalid address
        harness.shared_state.add_peer(2, "127.0.0.1:59999".to_string()).await.unwrap();
        let peer = harness.shared_state.get_peer(2).await.unwrap();
        
        // Make 5 failed attempts to trigger circuit breaker
        for i in 0..5 {
            let result = harness.peer_connector.connect_to_peer(&peer).await;
            assert!(result.is_err());
            
            // Small delay between attempts
            if i < 4 {
                timing::robust_sleep(Duration::from_millis(150)).await;
            }
        }
        
        // Circuit should be open now - attempts should fail immediately
        let start = std::time::Instant::now();
        let result = harness.peer_connector.connect_to_peer(&peer).await;
        assert!(result.is_err());
        let elapsed = start.elapsed();
        assert!(elapsed < Duration::from_millis(50)); // Should fail fast
        
        // The circuit breaker is open, but we might get either error message
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("Circuit breaker open") || err_msg.contains("backoff period"), 
            "Expected circuit breaker or backoff error but got: {}", err_msg);
        
        harness.cleanup().await;
    }
    
    /// Test circuit breaker half-open state
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_circuit_breaker_half_open() {
        let harness = TestHarness::new(1).await;
        
        // Get a port but don't start server yet
        let peer_addr = get_available_addr().await;
        harness.shared_state.add_peer(2, peer_addr.to_string()).await.unwrap();
        let peer = harness.shared_state.get_peer(2).await.unwrap();
        
        // Trigger circuit breaker with failures
        for i in 0..5 {
            let _ = harness.peer_connector.connect_to_peer(&peer).await;
            if i < 4 {
                timing::robust_sleep(Duration::from_millis(150)).await;
            }
        }
        
        // Start the server now
        let _mock_service = harness.start_mock_peer(2, peer_addr).await;
        
        // Wait 30 seconds for circuit to transition to half-open
        // (In real code we'd configure this timeout, using a shorter time for tests)
        timing::robust_sleep(Duration::from_secs(31)).await;
        
        // Next attempt should succeed and close the circuit
        harness.peer_connector.connect_to_peer(&peer).await.unwrap();
        
        // Subsequent attempts should work immediately (circuit closed)
        harness.peer_connector.disconnect_from_peer(2).await.unwrap();
        let start = std::time::Instant::now();
        harness.peer_connector.connect_to_peer(&peer).await.unwrap();
        let elapsed = start.elapsed();
        assert!(elapsed < Duration::from_millis(200)); // Should connect quickly
        
        harness.cleanup().await;
    }
    
    /// Test connection pool limits
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_connection_pool_limits() {
        let harness = TestHarness::new(1).await;
        
        // Create mock servers up to the limit (100)
        let mut mock_services = vec![];
        const LIMIT: usize = 100;
        
        // Add peers and connect to them
        for i in 0..LIMIT {
            let peer_id = (i + 2) as u64;
            let peer_addr = get_available_addr().await;
            
            // Start mock server
            let mock_service = harness.start_mock_peer(peer_id, peer_addr).await;
            mock_services.push(mock_service);
            
            // Connect to peer
            let peer = harness.shared_state.get_peer(peer_id).await.unwrap();
            harness.peer_connector.connect_to_peer(&peer).await.unwrap();
        }
        
        // Try to connect to one more peer - should fail
        let peer_addr = get_available_addr().await;
        let _mock_service = harness.start_mock_peer(102, peer_addr).await;
        let peer = harness.shared_state.get_peer(102).await.unwrap();
        
        let result = harness.peer_connector.connect_to_peer(&peer).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Connection pool limit"));
        
        // Disconnect from one peer
        harness.peer_connector.disconnect_from_peer(2).await.unwrap();
        
        // Now we should be able to connect
        harness.peer_connector.connect_to_peer(&peer).await.unwrap();
        
        harness.cleanup().await;
    }
    
    /// Test connection pool with failed connections
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_connection_pool_cleanup() {
        let harness = TestHarness::new(1).await;
        
        // Create some successful connections
        for i in 0..5 {
            let peer_id = (i + 2) as u64;
            let peer_addr = get_available_addr().await;
            let _mock_service = harness.start_mock_peer(peer_id, peer_addr).await;
            let peer = harness.shared_state.get_peer(peer_id).await.unwrap();
            harness.peer_connector.connect_to_peer(&peer).await.unwrap();
        }
        
        // Send a message to peer 2 with the mock configured to fail
        let mock_service = harness.get_mock_service(2).await.unwrap();
        mock_service.set_fail_responses(true).await;
        
        let message = create_test_message(1, 2);
        let _ = harness.peer_connector.send_raft_message(2, message).await;
        
        // Wait for connection cleanup after failure
        timing::robust_sleep(Duration::from_millis(100)).await;
        
        // We should be able to make a new connection (pool not exhausted)
        let peer_addr = get_available_addr().await;
        let _mock_service = harness.start_mock_peer(10, peer_addr).await;
        let peer = harness.shared_state.get_peer(10).await.unwrap();
        harness.peer_connector.connect_to_peer(&peer).await.unwrap();
        
        harness.cleanup().await;
    }

    // Helper functions

    async fn get_available_addr() -> SocketAddr {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        listener.local_addr().unwrap()
    }

    fn create_test_message(from: u64, to: u64) -> Message {
        let mut message = Message::default();
        message.msg_type = MessageType::MsgPropose.into();
        message.from = from;
        message.to = to;
        message.term = 1;
        message.index = 1;
        message.entries = vec![Entry {
            term: 1,
            index: 1,
            entry_type: EntryType::EntryNormal.into(),
            data: b"test".to_vec(),
            context: vec![],
            sync_log: false,
        }];
        message
    }

    fn create_test_message_with_type(from: u64, to: u64, msg_type: MessageType) -> Message {
        let mut message = create_test_message(from, to);
        message.msg_type = msg_type.into();
        message
    }
}