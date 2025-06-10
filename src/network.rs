use anyhow::{Result, Context};
use raft::prelude::*;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, info, warn, error};

#[cfg(feature = "failpoints")]
use fail::fail_point;

/// Network layer for Raft message passing
pub struct RaftNetwork {
    node_id: u64,
    bind_addr: SocketAddr,
    peers: Arc<RwLock<HashMap<u64, SocketAddr>>>,
    incoming_tx: mpsc::Sender<Message>,
}

impl RaftNetwork {
    pub fn new(
        node_id: u64, 
        bind_addr: SocketAddr,
        incoming_tx: mpsc::Sender<Message>,
    ) -> Self {
        Self {
            node_id,
            bind_addr,
            peers: Arc::new(RwLock::new(HashMap::new())),
            incoming_tx,
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
        let listener = TcpListener::bind(self.bind_addr).await
            .context("Failed to bind to address")?;
        
        info!("Node {} listening on {}", self.node_id, self.bind_addr);
        
        loop {
            let (stream, peer_addr) = listener.accept().await?;
            debug!("Accepted connection from {}", peer_addr);
            
            let network = self.clone();
            tokio::spawn(async move {
                if let Err(e) = network.handle_connection(stream).await {
                    warn!("Error handling connection from {}: {}", peer_addr, e);
                }
            });
        }
    }
    
    /// Handle an incoming connection
    async fn handle_connection(&self, mut stream: TcpStream) -> Result<()> {
        loop {
            // Read message length (4 bytes)
            let mut len_buf = [0u8; 4];
            match stream.read_exact(&mut len_buf).await {
                Ok(_) => {},
                Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                    debug!("Connection closed");
                    return Ok(());
                }
                Err(e) => return Err(e.into()),
            }
            
            let len = u32::from_be_bytes(len_buf) as usize;
            if len > 10 * 1024 * 1024 { // 10MB max message size
                return Err(anyhow::anyhow!("Message too large: {} bytes", len));
            }
            
            // Read message data
            let mut buf = vec![0u8; len];
            stream.read_exact(&mut buf).await?;
            
            // Deserialize Raft message
            match deserialize_message(&buf) {
                Ok(msg) => {
                    debug!("Received message from node {}: {:?}", msg.from, msg.msg_type);
                    if let Err(e) = self.incoming_tx.send(msg).await {
                        error!("Failed to forward message: {}", e);
                        return Err(e.into());
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
        fail_point!("network::before_send", |_| Err(anyhow::anyhow!("Injected network send failure")));
        
        let peers = self.peers.read().await;
        
        let addr = match peers.get(&msg.to) {
            Some(addr) => *addr,
            None => {
                warn!("Unknown peer: {}", msg.to);
                return Ok(());
            }
        };
        
        drop(peers); // Release the lock before network I/O
        
        debug!("Sending message to node {} at {}: {:?}", msg.to, addr, msg.msg_type);
        
        #[cfg(feature = "failpoints")]
        fail_point!("network::before_connect");
        
        // Connect and send with timeout
        let connect_future = TcpStream::connect(addr);
        match tokio::time::timeout(std::time::Duration::from_secs(1), connect_future).await {
            Ok(Ok(mut stream)) => {
                let data = serialize_message(&msg)?;
                let len = (data.len() as u32).to_be_bytes();
                
                stream.write_all(&len).await?;
                stream.write_all(&data).await?;
                stream.flush().await?;
                
                debug!("Message sent successfully");
                Ok(())
            }
            Ok(Err(e)) => {
                debug!("Failed to connect to {}: {} (node may not be ready)", addr, e);
                Ok(()) // Don't fail - Raft will retry
            }
            Err(_) => {
                debug!("Connection to {} timed out (node may not be ready)", addr);
                Ok(()) // Don't fail - Raft will retry
            }
        }
    }
}

/// Serialize a Raft message
fn serialize_message(msg: &Message) -> Result<Vec<u8>> {
    // We need to manually serialize because Message doesn't implement Serialize
    let mut data = Vec::new();
    
    // Write fields in order
    data.extend(&(msg.msg_type as i32).to_be_bytes());
    data.extend(&msg.to.to_be_bytes());
    data.extend(&msg.from.to_be_bytes());
    data.extend(&msg.term.to_be_bytes());
    data.extend(&msg.log_term.to_be_bytes());
    data.extend(&msg.index.to_be_bytes());
    data.extend(&msg.commit.to_be_bytes());
    data.push(if msg.reject { 1 } else { 0 }); // bool as u8
    data.extend(&msg.reject_hint.to_be_bytes());
    
    // Write entries
    data.extend(&(msg.entries.len() as u32).to_be_bytes());
    for entry in &msg.entries {
        data.extend(&(entry.entry_type as i32).to_be_bytes());
        data.extend(&entry.term.to_be_bytes());
        data.extend(&entry.index.to_be_bytes());
        data.extend(&(entry.data.len() as u32).to_be_bytes());
        data.extend(&entry.data);
    }
    
    // Write snapshot if present
    if let Some(ref snapshot) = msg.snapshot {
        if snapshot.get_metadata().index > 0 {
            data.push(1); // Has snapshot
            let snapshot_data = snapshot.get_data();
            data.extend(&(snapshot_data.len() as u32).to_be_bytes());
            data.extend(snapshot_data);
        } else {
            data.push(0); // No snapshot
        }
    } else {
        data.push(0); // No snapshot
    }
    
    // Write context
    data.extend(&(msg.context.len() as u32).to_be_bytes());
    data.extend(&msg.context);
    
    Ok(data)
}

/// Deserialize a Raft message
fn deserialize_message(data: &[u8]) -> Result<Message> {
    if data.len() < 53 { // Minimum message size (adjusted for i32 msg_type)
        return Err(anyhow::anyhow!("Message too short"));
    }
    
    let mut cursor = 0;
    let read_i32 = |cursor: &mut usize| -> i32 {
        let bytes = &data[*cursor..*cursor + 4];
        *cursor += 4;
        i32::from_be_bytes(bytes.try_into().unwrap())
    };
    
    let read_u64 = |cursor: &mut usize| -> u64 {
        let bytes = &data[*cursor..*cursor + 8];
        *cursor += 8;
        u64::from_be_bytes(bytes.try_into().unwrap())
    };
    
    let read_u32 = |cursor: &mut usize| -> u32 {
        let bytes = &data[*cursor..*cursor + 4];
        *cursor += 4;
        u32::from_be_bytes(bytes.try_into().unwrap())
    };
    
    let read_bool = |cursor: &mut usize| -> bool {
        let b = data[*cursor];
        *cursor += 1;
        b != 0
    };
    
    let mut msg = Message::default();
    msg.set_msg_type(MessageType::from_i32(read_i32(&mut cursor)).unwrap());
    msg.to = read_u64(&mut cursor);
    msg.from = read_u64(&mut cursor);
    msg.term = read_u64(&mut cursor);
    msg.log_term = read_u64(&mut cursor);
    msg.index = read_u64(&mut cursor);
    msg.commit = read_u64(&mut cursor);
    msg.reject = read_bool(&mut cursor);
    msg.reject_hint = read_u64(&mut cursor);
    
    // Read entries
    let entry_count = read_u32(&mut cursor) as usize;
    for _ in 0..entry_count {
        let mut entry = Entry::default();
        entry.set_entry_type(EntryType::from_i32(read_i32(&mut cursor)).unwrap());
        entry.term = read_u64(&mut cursor);
        entry.index = read_u64(&mut cursor);
        let data_len = read_u32(&mut cursor) as usize;
        if cursor + data_len > data.len() {
            return Err(anyhow::anyhow!("Invalid entry data length"));
        }
        entry.data = data[cursor..cursor + data_len].to_vec().into();
        cursor += data_len;
        msg.entries.push(entry);
    }
    
    // Read snapshot
    if cursor < data.len() && read_bool(&mut cursor) {
        let snapshot_len = read_u32(&mut cursor) as usize;
        if cursor + snapshot_len > data.len() {
            return Err(anyhow::anyhow!("Invalid snapshot length"));
        }
        let mut snapshot = Snapshot::default();
        snapshot.set_data(data[cursor..cursor + snapshot_len].to_vec().into());
        cursor += snapshot_len;
        msg.set_snapshot(snapshot);
    }
    
    // Read context
    if cursor + 4 <= data.len() {
        let context_len = read_u32(&mut cursor) as usize;
        if cursor + context_len <= data.len() {
            msg.context = data[cursor..cursor + context_len].to_vec().into();
        }
    }
    
    Ok(msg)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_message_serialization() {
        let mut msg = Message::default();
        msg.set_msg_type(MessageType::MsgAppend);
        msg.to = 2;
        msg.from = 1;
        msg.term = 5;
        msg.index = 10;
        
        let data = serialize_message(&msg).unwrap();
        let msg2 = deserialize_message(&data).unwrap();
        
        assert_eq!(msg.msg_type, msg2.msg_type);
        assert_eq!(msg.to, msg2.to);
        assert_eq!(msg.from, msg2.from);
        assert_eq!(msg.term, msg2.term);
        assert_eq!(msg.index, msg2.index);
    }
}