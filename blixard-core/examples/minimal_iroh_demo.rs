//! Minimal demo of Iroh RPC functionality
//!
//! This example shows the core Iroh RPC working without
//! dependencies on the rest of the Blixard codebase.

use std::time::Duration;
use tokio::time::sleep;

// Inline the essential protocol types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
enum MessageType {
    Request = 1,
    Response = 2,
    Error = 3,
    Ping = 6,
    Pong = 7,
}

#[derive(Debug, Clone)]
struct MessageHeader {
    version: u8,
    msg_type: MessageType,
    payload_len: u32,
    request_id: [u8; 16],
}

impl MessageHeader {
    fn new(msg_type: MessageType, payload_len: u32, request_id: [u8; 16]) -> Self {
        Self {
            version: 1,
            msg_type,
            payload_len,
            request_id,
        }
    }

    fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(24);
        bytes.push(self.version);
        bytes.push(self.msg_type as u8);
        bytes.extend_from_slice(&[0, 0]); // padding
        bytes.extend_from_slice(&self.payload_len.to_le_bytes());
        bytes.extend_from_slice(&self.request_id);
        bytes
    }

    fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() < 24 {
            return Err("Header too short".to_string());
        }

        let msg_type = match bytes[1] {
            1 => MessageType::Request,
            2 => MessageType::Response,
            3 => MessageType::Error,
            6 => MessageType::Ping,
            7 => MessageType::Pong,
            _ => return Err("Unknown message type".to_string()),
        };

        let mut request_id = [0u8; 16];
        request_id.copy_from_slice(&bytes[8..24]);

        Ok(Self {
            version: bytes[0],
            msg_type,
            payload_len: u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]),
            request_id,
        })
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 Minimal Iroh RPC Demo");
    println!("========================\n");

    // Create Iroh endpoints
    println!("Creating Iroh endpoints...");
    let server_endpoint = iroh::Endpoint::builder()
        .alpns(vec![b"blixard/rpc/1".to_vec()])
        .bind()
        .await?;

    let server_node_id = server_endpoint.node_id();
    let server_addr = iroh::NodeAddr::new(server_node_id);
    println!("✅ Server node ID: {}", server_node_id);

    // Start server
    let server_endpoint_clone = server_endpoint.clone();
    let server_task = tokio::spawn(async move {
        println!("🔄 Server waiting for connections...");

        while let Some(connecting) = server_endpoint_clone.accept().await {
            println!("📥 New connection from: {:?}", connecting.remote_address());

            tokio::spawn(async move {
                let connection = connecting.await.unwrap();
                while let Ok((mut send, mut recv)) = connection.accept_bi().await {
                    // Read header
                    let mut header_bytes = vec![0u8; 24];
                    if recv.read_exact(&mut header_bytes).await.is_ok() {
                        if let Ok(header) = MessageHeader::from_bytes(&header_bytes) {
                            println!("📨 Received {:?} message", header.msg_type);

                            match header.msg_type {
                                MessageType::Ping => {
                                    // Send Pong response
                                    let response_header =
                                        MessageHeader::new(MessageType::Pong, 0, header.request_id);
                                    let _ = send.write_all(&response_header.to_bytes()).await;
                                    let _ = send.finish();
                                    println!("📤 Sent Pong response");
                                }
                                MessageType::Request => {
                                    // Read payload
                                    let mut payload = vec![0u8; header.payload_len as usize];
                                    if recv.read_exact(&mut payload).await.is_ok() {
                                        let msg = String::from_utf8_lossy(&payload);
                                        println!("📝 Request payload: {}", msg);

                                        // Send response
                                        let response_payload = format!("Echo: {}", msg);
                                        let response_header = MessageHeader::new(
                                            MessageType::Response,
                                            response_payload.len() as u32,
                                            header.request_id,
                                        );
                                        let _ = send.write_all(&response_header.to_bytes()).await;
                                        let _ = send.write_all(response_payload.as_bytes()).await;
                                        let _ = send.finish();
                                        println!("📤 Sent Response");
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                }
            });
        }
    });

    // Give server time to start
    sleep(Duration::from_millis(100)).await;

    // Create client
    println!("\nCreating client endpoint...");
    let client_endpoint = iroh::Endpoint::builder()
        .alpns(vec![b"blixard/rpc/1".to_vec()])
        .bind()
        .await?;
    println!("✅ Client endpoint created");

    // Connect to server
    println!("\n🔗 Connecting to server...");
    let connection = client_endpoint
        .connect(server_addr, b"blixard/rpc/1")
        .await?;
    println!("✅ Connected!");

    // Test 1: Ping/Pong
    println!("\n📡 Test 1: Ping/Pong");
    println!("===================");
    {
        let (mut send, mut recv) = connection.open_bi().await?;
        let request_id = *uuid::Uuid::new_v4().as_bytes();
        let ping_header = MessageHeader::new(MessageType::Ping, 0, request_id);

        let start = std::time::Instant::now();
        send.write_all(&ping_header.to_bytes()).await?;
        send.finish();

        let mut response_header_bytes = vec![0u8; 24];
        recv.read_exact(&mut response_header_bytes).await?;
        let response_header = MessageHeader::from_bytes(&response_header_bytes)?;
        let latency = start.elapsed();

        assert_eq!(response_header.msg_type, MessageType::Pong);
        assert_eq!(response_header.request_id, request_id);
        println!("✅ Ping/Pong successful! Latency: {:?}", latency);
    }

    // Test 2: Request/Response
    println!("\n📡 Test 2: Request/Response");
    println!("==========================");
    {
        let messages = vec![
            "Hello, Iroh!",
            "Testing P2P RPC",
            "Blazing fast communication 🚀",
        ];

        for msg in messages {
            let (mut send, mut recv) = connection.open_bi().await?;
            let request_id = *uuid::Uuid::new_v4().as_bytes();
            let request_header =
                MessageHeader::new(MessageType::Request, msg.len() as u32, request_id);

            println!("\n→ Sending: '{}'", msg);
            let start = std::time::Instant::now();

            send.write_all(&request_header.to_bytes()).await?;
            send.write_all(msg.as_bytes()).await?;
            send.finish();

            let mut response_header_bytes = vec![0u8; 24];
            recv.read_exact(&mut response_header_bytes).await?;
            let response_header = MessageHeader::from_bytes(&response_header_bytes)?;

            let mut response_payload = vec![0u8; response_header.payload_len as usize];
            recv.read_exact(&mut response_payload).await?;
            let response_msg = String::from_utf8_lossy(&response_payload);
            let latency = start.elapsed();

            assert_eq!(response_header.msg_type, MessageType::Response);
            assert_eq!(response_header.request_id, request_id);
            println!("← Response: '{}'", response_msg);
            println!("  Latency: {:?}", latency);
        }
    }

    // Test 3: Performance
    println!("\n⚡ Test 3: Performance (100 requests)");
    println!("====================================");
    {
        let mut latencies = Vec::new();

        for i in 0..100 {
            let (mut send, mut recv) = connection.open_bi().await?;
            let request_id = *uuid::Uuid::new_v4().as_bytes();
            let msg = format!("Request {}", i);
            let request_header =
                MessageHeader::new(MessageType::Request, msg.len() as u32, request_id);

            let start = std::time::Instant::now();

            send.write_all(&request_header.to_bytes()).await?;
            send.write_all(msg.as_bytes()).await?;
            send.finish();

            let mut response_header_bytes = vec![0u8; 24];
            recv.read_exact(&mut response_header_bytes).await?;
            let response_header = MessageHeader::from_bytes(&response_header_bytes)?;

            let mut response_payload = vec![0u8; response_header.payload_len as usize];
            recv.read_exact(&mut response_payload).await?;

            latencies.push(start.elapsed());
        }

        latencies.sort();
        let total: Duration = latencies.iter().sum();
        let avg = total / latencies.len() as u32;
        let p50 = latencies[latencies.len() / 2];
        let p99 = latencies[latencies.len() * 99 / 100];

        println!("\n📊 Performance Results:");
        println!("  Total requests: {}", latencies.len());
        println!("  Average latency: {:?}", avg);
        println!("  P50 latency: {:?}", p50);
        println!("  P99 latency: {:?}", p99);
    }

    // Cleanup
    println!("\n🧹 Cleaning up...");
    server_task.abort();

    println!("\n✅ Demo completed successfully!");
    println!("\nThis demonstrates that our Iroh RPC protocol works correctly:");
    println!("- Binary message framing with headers");
    println!("- Request/response correlation");
    println!("- Low latency P2P communication");
    println!("- Reliable message delivery over QUIC");

    Ok(())
}
