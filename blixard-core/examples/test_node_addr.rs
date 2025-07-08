//! Test program to verify IrohTransportV2 node_addr implementation

use blixard_core::error::BlixardResult;
use blixard_core::iroh_transport_v2::IrohTransportV2;
use tempfile::TempDir;
use tracing::info;

#[tokio::main]
async fn main() -> BlixardResult<()> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    println!("=== Testing IrohTransportV2 node_addr implementation ===\n");

    // Create a temporary directory
    let temp_dir = TempDir::new().unwrap();

    // Create the transport
    info!("Creating IrohTransportV2...");
    let transport = IrohTransportV2::new(1, temp_dir.path()).await?;

    // Get the node address
    info!("Getting node address...");
    let node_addr = transport.node_addr().await?;

    // Print the node information
    println!("\n📍 Node Address Information:");
    println!("   Node ID: {}", node_addr.node_id);

    println!("\n🌐 Direct addresses:");
    let addrs: Vec<_> = node_addr.direct_addresses().collect();
    if addrs.is_empty() {
        println!("   ❌ No direct addresses found!");
    } else {
        for addr in &addrs {
            println!("   ✅ {}", addr);
        }
    }

    println!("\n🔗 Relay URL:");
    match node_addr.relay_url() {
        Some(url) => println!("   ✅ {}", url),
        None => println!("   ❌ No relay URL configured!"),
    }

    // Summary
    println!("\n📊 Summary:");
    println!("   - Node ID: ✅ Present");
    println!(
        "   - Direct addresses: {} ({})",
        if addrs.is_empty() {
            "❌ Missing"
        } else {
            "✅ Present"
        },
        addrs.len()
    );
    println!(
        "   - Relay URL: {}",
        if node_addr.relay_url().is_some() {
            "✅ Present"
        } else {
            "❌ Missing"
        }
    );

    // Shutdown
    info!("Shutting down transport...");
    transport.shutdown().await?;

    println!("\n✅ Test completed successfully!");

    Ok(())
}
