// This shows what needs to be fixed in raft_node.rs

// Option 1: Change the constructor to accept peer addresses
pub async fn new(
    node_id: u64, 
    bind_addr: SocketAddr, 
    storage: Arc<Storage>, 
    peers: HashMap<u64, SocketAddr>  // Changed from Vec<u64>
) -> Result<Self> {
    // ... existing code ...
    
    // After creating the network, register all peers
    for (peer_id, peer_addr) in peers {
        network.add_peer(peer_id, peer_addr).await;
    }
    
    // ... rest of constructor
}

// Option 2: Add a method to register peers after construction
impl RaftNode {
    pub async fn register_peers(&self, peers: HashMap<u64, SocketAddr>) {
        for (peer_id, peer_addr) in peers {
            self.network.add_peer(peer_id, peer_addr).await;
        }
    }
}