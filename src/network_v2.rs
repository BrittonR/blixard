use anyhow::{Context, Result};
use raft::prelude::*;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, error, info, warn};

#[cfg(feature = "failpoints")]
use fail::fail_point;

use crate::raft_message_wrapper::RaftMessageWrapper;
use crate::runtime_traits::Runtime;

/// Network layer for Raft message passing
/// Now generic over Runtime to support simulation
pub struct RaftNetwork<R: Runtime> {
    node_id: u64,
    bind_addr: SocketAddr,
    peers: Arc<RwLock<HashMap<u64, SocketAddr>>>,
    incoming_tx: mpsc::Sender<Message>,
    runtime: Arc<R>,
}

impl<R: Runtime + 'static> RaftNetwork<R> {
    pub fn new(
        node_id: u64,
        bind_addr: SocketAddr,
        incoming_tx: mpsc::Sender<Message>,
        runtime: Arc<R>,
    ) -> Self {
        Self {
            node_id,
            bind_addr,
            peers: Arc::new(RwLock::new(HashMap::new())),
            incoming_tx,
            runtime,
        }
    }

    /// Add a peer to the network
    pub async fn add_peer(&self, node_id: u64, addr: SocketAddr) {
        let mut peers = self.peers.write().await;
        peers.insert(node_id, addr);
        info!("Added peer {} at {}", node_id, addr);
    }

    /// Remove a peer from the network
    pub async fn remove_peer(&self, node_id: u64) {
        let mut peers = self.peers.write().await;
        peers.remove(&node_id);
        info!("Removed peer {}", node_id);
    }

    /// Start listening for incoming connections
    pub async fn listen(self: Arc<Self>) -> Result<()> {
        // For now, always use TCP implementation
        // TODO: Add simulation-specific network handling
        self.listen_tcp().await
    }

    /// TCP-based listening (for real runtime)
    async fn listen_tcp(self: Arc<Self>) -> Result<()> {
        let listener = TcpListener::bind(self.bind_addr)
            .await
            .context("Failed to bind to address")?;

        info!("Node {} listening on {}", self.node_id, self.bind_addr);

        loop {
            let (stream, peer_addr) = listener.accept().await?;
            debug!("Accepted connection from {}", peer_addr);

            let network = self.clone();
            self.runtime.spawn(Box::pin(async move {
                if let Err(e) = network.handle_connection(stream).await {
                    warn!("Error handling connection from {}: {}", peer_addr, e);
                }
            }));
        }
    }

    /// Handle an incoming connection
    async fn handle_connection(&self, mut stream: TcpStream) -> Result<()> {
        loop {
            // Read message length (4 bytes)
            let mut len_buf = [0u8; 4];
            match stream.read_exact(&mut len_buf).await {
                Ok(_) => {}
                Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                    debug!("Connection closed");
                    return Ok(());
                }
                Err(e) => {
                    return Err(e.into());
                }
            }

            let msg_len = u32::from_be_bytes(len_buf) as usize;
            if msg_len > 10 * 1024 * 1024 {
                // 10MB max message size
                return Err(anyhow::anyhow!("Message too large: {} bytes", msg_len));
            }

            // Read message data
            let mut msg_buf = vec![0u8; msg_len];
            stream.read_exact(&mut msg_buf).await?;

            // Deserialize and forward the message
            match bincode::deserialize::<RaftMessageWrapper>(&msg_buf) {
                Ok(wrapper) => {
                    let msg = Message::from(wrapper);
                    debug!(
                        "Node {} received message from node {}",
                        self.node_id, msg.from
                    );
                    if let Err(e) = self.incoming_tx.send(msg).await {
                        error!("Failed to forward message: {}", e);
                        return Ok(()); // Channel closed, exit
                    }
                }
                Err(e) => {
                    warn!("Failed to deserialize message: {}", e);
                }
            }
        }
    }

    /// Send a message to a peer
    pub async fn send_message(&self, msg: Message) -> Result<()> {
        #[cfg(feature = "failpoints")]
        fail_point!("network::before_send", |_| Err(anyhow::anyhow!(
            "Injected network failure"
        )));

        let peers = self.peers.read().await;
        let peer_addr = peers
            .get(&msg.to)
            .ok_or_else(|| anyhow::anyhow!("Unknown peer: {}", msg.to))?
            .clone();
        drop(peers);

        debug!(
            "Node {} sending message to node {} at {}",
            self.node_id, msg.to, peer_addr
        );

        // Serialize the message using wrapper
        let wrapper = RaftMessageWrapper::from(msg);
        let msg_data = bincode::serialize(&wrapper)?;

        // For now, always use TCP
        // TODO: Add simulation-specific sending
        self.send_tcp(peer_addr, msg_data).await?;

        Ok(())
    }

    /// Send message over TCP (for real runtime)
    async fn send_tcp(&self, peer_addr: SocketAddr, msg_data: Vec<u8>) -> Result<()> {
        // Try to connect with timeout using tokio directly for now
        // TODO: Add timeout support to runtime abstraction
        let connect_future = TcpStream::connect(peer_addr);

        let mut stream = match tokio::time::timeout(Duration::from_secs(1), connect_future).await {
            Ok(Ok(stream)) => stream,
            Ok(Err(e)) => {
                warn!("Failed to connect to {}: {}", peer_addr, e);
                return Err(e.into());
            }
            Err(_) => {
                warn!("Connection to {} timed out", peer_addr);
                return Err(anyhow::anyhow!("Connection timeout"));
            }
        };

        // Send message length
        let len_buf = (msg_data.len() as u32).to_be_bytes();
        stream.write_all(&len_buf).await?;

        // Send message data
        stream.write_all(&msg_data).await?;
        stream.flush().await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::simulation::SimulatedRuntime;

    #[tokio::test]
    async fn test_network_with_simulated_runtime() {
        let runtime = Arc::new(SimulatedRuntime::new(42));
        let (tx, _rx) = mpsc::channel(10);
        let addr = "127.0.0.1:8000".parse().unwrap();

        let network = Arc::new(RaftNetwork::new(1, addr, tx, runtime.clone()));

        // Add a peer
        network.add_peer(2, "127.0.0.1:8001".parse().unwrap()).await;

        // Verify peer was added
        let peers = network.peers.read().await;
        assert_eq!(peers.len(), 1);
        assert!(peers.contains_key(&2));
    }
}
