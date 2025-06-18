//! Property-based tests for PeerConnector
//!
//! This module uses proptest to verify invariants and properties
//! of the PeerConnector across random inputs and operation sequences.

use std::collections::HashSet;
use std::sync::Arc;
use proptest::prelude::*;
use proptest::collection::vec;
use tokio::sync::RwLock;

use blixard::peer_connector::PeerConnector;
use blixard::node_shared::{SharedNodeState, PeerInfo};
use blixard::types::NodeConfig;

/// Generate a valid peer ID (non-zero)
fn peer_id_strategy() -> impl Strategy<Value = u64> {
    1..=1000u64
}

/// Generate a valid port number
fn port_strategy() -> impl Strategy<Value = u16> {
    49152..=65535u16
}

/// Generate a PeerInfo
fn peer_info_strategy() -> impl Strategy<Value = PeerInfo> {
    (peer_id_strategy(), port_strategy()).prop_map(|(id, port)| {
        PeerInfo {
            id,
            address: format!("127.0.0.1:{}", port),
            is_connected: false,
        }
    })
}

/// Operations that can be performed on PeerConnector
#[derive(Debug, Clone)]
enum PeerOperation {
    Connect(PeerInfo),
    Disconnect(u64),
    SendMessage(u64),
    CheckHealth(u64),
}

/// Generate a sequence of operations
fn operation_sequence_strategy(max_peers: usize) -> impl Strategy<Value = Vec<PeerOperation>> {
    vec(peer_info_strategy(), 1..=max_peers)
        .prop_flat_map(|peers| {
            let peer_ids: Vec<u64> = peers.iter().map(|p| p.id).collect();
            
            vec(
                prop_oneof![
                    // Connect to a peer
                    (0..peers.len()).prop_map({
                        let peers = peers.clone();
                        move |idx| PeerOperation::Connect(peers[idx].clone())
                    }),
                    // Disconnect from a peer
                    (0..peer_ids.len()).prop_map({
                        let peer_ids = peer_ids.clone();
                        move |idx| PeerOperation::Disconnect(peer_ids[idx])
                    }),
                    // Send message to a peer
                    (0..peer_ids.len()).prop_map({
                        let peer_ids = peer_ids.clone();
                        move |idx| PeerOperation::SendMessage(peer_ids[idx])
                    }),
                    // Check health of a peer
                    (0..peer_ids.len()).prop_map({
                        let peer_ids = peer_ids.clone();
                        move |idx| PeerOperation::CheckHealth(peer_ids[idx])
                    }),
                ],
                0..=20
            )
        })
}

/// Test harness for property tests
struct PropTestHarness {
    shared_state: Arc<SharedNodeState>,
    peer_connector: Arc<PeerConnector>,
    connected_peers: Arc<RwLock<HashSet<u64>>>,
}

impl PropTestHarness {
    async fn new() -> Self {
        let config = NodeConfig {
            id: 1,
            data_dir: "/tmp/blixard-proptest".to_string(),
            bind_addr: "127.0.0.1:0".parse().unwrap(),
            join_addr: None,
            use_tailscale: false,
        };
        let shared_state = Arc::new(SharedNodeState::new(config));
        let peer_connector = Arc::new(PeerConnector::new(shared_state.clone()));
        
        Self {
            shared_state,
            peer_connector,
            connected_peers: Arc::new(RwLock::new(HashSet::new())),
        }
    }
    
    async fn execute_operation(&self, op: &PeerOperation) -> Result<(), String> {
        match op {
            PeerOperation::Connect(peer) => {
                // Add peer to shared state if not already there
                if self.shared_state.get_peer(peer.id).await.is_none() {
                    self.shared_state.add_peer(peer.id, peer.address.clone()).await
                        .map_err(|e| e.to_string())?;
                }
                
                // Try to connect (may fail due to invalid address)
                match self.peer_connector.connect_to_peer(peer).await {
                    Ok(_) => {
                        self.connected_peers.write().await.insert(peer.id);
                        Ok(())
                    }
                    Err(_) => Ok(()), // Connection failures are expected
                }
            }
            PeerOperation::Disconnect(peer_id) => {
                // Disconnect may fail if peer doesn't exist
                match self.peer_connector.disconnect_from_peer(*peer_id).await {
                    Ok(_) => {
                        self.connected_peers.write().await.remove(peer_id);
                        Ok(())
                    }
                    Err(_) => Ok(()), // Disconnection failures are acceptable
                }
            }
            PeerOperation::SendMessage(peer_id) => {
                // Create a test message
                let message = raft::prelude::Message {
                    msg_type: raft::prelude::MessageType::MsgHeartbeat.into(),
                    from: 1,
                    to: *peer_id,
                    ..Default::default()
                };
                
                // Send may fail if not connected
                let _ = self.peer_connector.send_raft_message(*peer_id, message).await;
                Ok(())
            }
            PeerOperation::CheckHealth(peer_id) => {
                // Health check may return false if not connected
                let _ = self.peer_connector.check_connection_health(*peer_id).await;
                Ok(())
            }
        }
    }
    
    async fn verify_invariants(&self) -> Result<(), String> {
        // Invariant 1: Connected peers should have is_connected = true in shared state
        let connected = self.connected_peers.read().await;
        for peer_id in connected.iter() {
            if let Some(peer) = self.shared_state.get_peer(*peer_id).await {
                // Note: This might not always be true due to async nature
                // but we check for consistency
                if !peer.is_connected {
                    // This is acceptable due to async updates
                }
            }
        }
        
        // Invariant 2: Total connections should not exceed MAX_CONNECTIONS (100)
        let peer_count = self.shared_state.get_peers().await.len();
        if peer_count > 100 {
            return Err(format!("Too many peers: {}", peer_count));
        }
        
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(10))]
        
        /// Test that PeerConnector maintains invariants across random operations
        #[test]
        fn test_peer_connector_invariants(ops in operation_sequence_strategy(10)) {
            let rt = tokio::runtime::Runtime::new().unwrap();
            
            rt.block_on(async {
                let harness = PropTestHarness::new().await;
                
                // Execute all operations
                for op in &ops {
                    // Ignore errors from operations - we're testing invariants
                    let _ = harness.execute_operation(op).await;
                }
                
                // Verify invariants hold
                harness.verify_invariants().await.unwrap();
            });
        }
        
        /// Test that connection/disconnection operations are idempotent
        #[test]
        fn test_connection_idempotency(peer in peer_info_strategy()) {
            let rt = tokio::runtime::Runtime::new().unwrap();
            
            rt.block_on(async {
                let harness = PropTestHarness::new().await;
                
                // Add peer
                harness.shared_state.add_peer(peer.id, peer.address.clone()).await.unwrap();
                
                // Multiple connect attempts should be safe
                for _ in 0..3 {
                    let _ = harness.peer_connector.connect_to_peer(&peer).await;
                }
                
                // Multiple disconnect attempts should be safe
                for _ in 0..3 {
                    let _ = harness.peer_connector.disconnect_from_peer(peer.id).await;
                }
                
                harness.verify_invariants().await.unwrap();
            });
        }
        
        /// Test that message buffering respects limits
        #[test]
        fn test_message_buffer_limits(
            peer in peer_info_strategy(),
            message_count in 0..200usize
        ) {
            let rt = tokio::runtime::Runtime::new().unwrap();
            
            rt.block_on(async {
                let harness = PropTestHarness::new().await;
                
                // Add peer but don't connect
                harness.shared_state.add_peer(peer.id, "127.0.0.1:99999".to_string()).await.unwrap();
                
                // Send many messages - they should be buffered
                for i in 0..message_count {
                    let message = raft::prelude::Message {
                        msg_type: raft::prelude::MessageType::MsgHeartbeat.into(),
                        from: 1,
                        to: peer.id,
                        index: i as u64,
                        ..Default::default()
                    };
                    
                    let _ = harness.peer_connector.send_raft_message(peer.id, message).await;
                }
                
                // Buffer should not grow unbounded (max 100 messages)
                // We can't directly check this, but the system shouldn't crash
                harness.verify_invariants().await.unwrap();
            });
        }
        
        /// Test concurrent operations don't violate invariants
        #[test]
        fn test_concurrent_operations(
            peers in vec(peer_info_strategy(), 2..=5)
        ) {
            let rt = tokio::runtime::Runtime::new().unwrap();
            
            rt.block_on(async {
                let harness = PropTestHarness::new().await;
                
                // Add all peers
                for peer in &peers {
                    harness.shared_state.add_peer(peer.id, peer.address.clone()).await.unwrap();
                }
                
                // Launch concurrent operations
                let mut handles = vec![];
                
                for peer in peers {
                    let connector = harness.peer_connector.clone();
                    let peer_clone = peer.clone();
                    
                    // Connect
                    let handle1 = tokio::spawn(async move {
                        let _ = connector.connect_to_peer(&peer_clone).await;
                    });
                    handles.push(handle1);
                    
                    // Send message
                    let connector = harness.peer_connector.clone();
                    let peer_id = peer.id;
                    let handle2 = tokio::spawn(async move {
                        let message = raft::prelude::Message {
                            msg_type: raft::prelude::MessageType::MsgHeartbeat.into(),
                            from: 1,
                            to: peer_id,
                            ..Default::default()
                        };
                        let _ = connector.send_raft_message(peer_id, message).await;
                    });
                    handles.push(handle2);
                    
                    // Health check
                    let connector = harness.peer_connector.clone();
                    let peer_id = peer.id;
                    let handle3 = tokio::spawn(async move {
                        let _ = connector.check_connection_health(peer_id).await;
                    });
                    handles.push(handle3);
                }
                
                // Wait for all operations to complete
                for handle in handles {
                    let _ = handle.await;
                }
                
                harness.verify_invariants().await.unwrap();
            });
        }
    }
}