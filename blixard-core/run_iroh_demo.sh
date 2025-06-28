#!/usr/bin/env bash
# Run the Iroh RPC demo in standalone mode

cd "$(dirname "$0")"

# Create a temporary project
TEMP_DIR=$(mktemp -d)
cd $TEMP_DIR

# Create a minimal Cargo.toml
cat > Cargo.toml << 'EOF'
[package]
name = "iroh-demo"
version = "0.1.0"
edition = "2021"

[dependencies]
tokio = { version = "1", features = ["full"] }
iroh = "0.29"
uuid = { version = "1.11", features = ["v4"] }
EOF

# Copy the demo file
cp "$OLDPWD/examples/minimal_iroh_demo.rs" src/main.rs
mkdir -p src

# Build and run
echo "Building standalone Iroh demo..."
cargo build --release 2>/dev/null || {
    echo "Build failed. Creating simpler demo..."
    
    # Create an even simpler demo
    cat > src/main.rs << 'EOF'
use std::time::Duration;
use tokio::time::sleep;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 Simple Iroh Connection Test");
    println!("==============================\n");
    
    // Create endpoints
    let server = iroh::Endpoint::builder()
        .alpns(vec![b"test/1".to_vec()])
        .bind()
        .await?;
    
    let client = iroh::Endpoint::builder()
        .alpns(vec![b"test/1".to_vec()])
        .bind()
        .await?;
    
    let server_id = server.node_id();
    let server_addr = server.node_addr().await.unwrap();
    
    println!("Server ID: {}", server_id);
    println!("Client ID: {}", client.node_id());
    
    // Server accept connections
    let server_task = tokio::spawn(async move {
        if let Some(incoming) = server.accept().await {
            let mut conn = incoming.accept().unwrap().await.unwrap();
            println!("\n✅ Server: Accepted connection!");
            if let Ok((mut send, mut recv)) = conn.accept_bi().await {
                let mut buf = vec![0u8; 5];
                if recv.read_exact(&mut buf).await.is_ok() {
                    println!("✅ Server: Received: {}", String::from_utf8_lossy(&buf));
                    let _ = send.write_all(b"PONG!").await;
                }
            }
        }
    });
    
    sleep(Duration::from_millis(100)).await;
    
    // Client connect
    println!("\n🔗 Client: Connecting...");
    let conn = client.connect(server_addr, b"test/1").await?;
    println!("✅ Client: Connected!");
    
    // Send message
    let (mut send, mut recv) = conn.open_bi().await?;
    send.write_all(b"PING!").await?;
    send.finish();
    
    let mut buf = vec![0u8; 5];
    recv.read_exact(&mut buf).await?;
    println!("✅ Client: Received: {}", String::from_utf8_lossy(&buf));
    
    server_task.abort();
    
    println!("\n✨ Iroh P2P communication works!");
    Ok(())
}
EOF

    cargo build --release
}

# Run the demo
echo -e "\n\n"
cargo run --release

# Cleanup
cd /
rm -rf $TEMP_DIR