//! Test to verify P2P address collection fix
#![cfg(feature = "test-helpers")]

use blixard_core::test_helpers::TestCluster;

#[tokio::test]
async fn test_p2p_address_collection_fixed() {
    // Create a test cluster
    let cluster = TestCluster::builder()
        .with_nodes(2)
        .build()
        .await
        .expect("Failed to create cluster");
    
    // The cluster should have automatically formed with proper P2P connections
    // Wait a bit for peers to discover each other
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    
    // Get the first node and check its peers
    let node1 = cluster.get_node(1).expect("Should get node 1");
    let peers1 = node1.shared_state.get_peers().await;
    
    // Since we have a 2-node cluster, node 1 should see node 2
    assert_eq!(peers1.len(), 1, "Node 1 should see exactly one peer");
    let peer = &peers1[0];
    
    // The key test: verify P2P addresses were properly collected
    assert!(!peer.p2p_addresses.is_empty(), "Peer should have P2P addresses - the address collection fix should ensure this");
    assert!(peer.p2p_node_id.is_some(), "Peer should have P2P node ID");
    
    // Check that the addresses are valid socket addresses
    for addr_str in &peer.p2p_addresses {
        assert!(addr_str.parse::<std::net::SocketAddr>().is_ok(), 
                "P2P address {} should be a valid socket address", addr_str);
    }
    
    println!("P2P address collection test passed!");
    println!("Peer has {} P2P addresses: {:?}", peer.p2p_addresses.len(), peer.p2p_addresses);
}