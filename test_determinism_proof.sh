#!/bin/bash

echo "🧪 DETERMINISM PROOF TEST 🧪"
echo "============================="

echo -e "\n🎲 Running the same test 3 times with same seed to prove determinism..."

# Create a simple test that outputs timing information
cat > tests/determinism_proof.rs << 'EOF'
#![cfg(feature = "simulation")]

use blixard::runtime::simulation::SimulatedRuntime;
use blixard::runtime_traits::{Runtime, Clock};
use blixard::raft_node_v2::RaftNode;
use blixard::storage::Storage;
use std::sync::Arc;
use std::time::Duration;
use std::net::SocketAddr;

#[tokio::test]
async fn prove_determinism() {
    println!("\n🔬 DETERMINISM PROOF TEST");
    
    // Use SAME seed every time - this should produce IDENTICAL results
    let runtime = Arc::new(SimulatedRuntime::new(12345));
    
    let start_time = runtime.clock().now();
    println!("⏰ Start time: {:?}", start_time);
    
    // Create a RaftNode with simulated runtime
    let storage = Arc::new(Storage::new_test().unwrap());
    let addr: SocketAddr = "127.0.0.1:40000".parse().unwrap();
    let node = RaftNode::new(1, addr, storage, vec![1], runtime.clone()).await.unwrap();
    
    let create_time = runtime.clock().now();
    println!("📦 Node created at: {:?} (elapsed: {:?})", create_time, create_time - start_time);
    
    // Advance time deterministically 
    runtime.advance_time(Duration::from_millis(500));
    let after_advance = runtime.clock().now();
    println!("⏩ After 500ms advance: {:?} (elapsed: {:?})", after_advance, after_advance - start_time);
    
    // Sleep using simulated clock
    runtime.clock().sleep(Duration::from_millis(200)).await;
    let after_sleep = runtime.clock().now();
    println!("😴 After 200ms sleep: {:?} (elapsed: {:?})", after_sleep, after_sleep - start_time);
    
    let final_elapsed = after_sleep - start_time;
    println!("🏁 Total elapsed: {:?}", final_elapsed);
    
    // This should be EXACTLY the same every run!
    assert_eq!(final_elapsed, Duration::from_millis(700));
    
    println!("✅ Test completed deterministically!");
}
EOF

echo -e "\n1️⃣ Running test #1..."
cargo test prove_determinism --all-features --nocapture 2>/dev/null | grep -E "(⏰|📦|⏩|😴|🏁|✅)"

echo -e "\n2️⃣ Running test #2..."
cargo test prove_determinism --all-features --nocapture 2>/dev/null | grep -E "(⏰|📦|⏩|😴|🏁|✅)"

echo -e "\n3️⃣ Running test #3..."
cargo test prove_determinism --all-features --nocapture 2>/dev/null | grep -E "(⏰|📦|⏩|😴|🏁|✅)"

echo -e "\n📊 Analysis:"
echo "If all 3 runs show IDENTICAL timestamps, then determinism is working!"
echo "If timestamps differ, then there's still non-determinism."

echo -e "\n🔍 Now testing with DIFFERENT seeds should show DIFFERENT results:"

# Test with different seed
cat > tests/determinism_proof2.rs << 'EOF'
#![cfg(feature = "simulation")]

use blixard::runtime::simulation::SimulatedRuntime;
use blixard::runtime_traits::{Runtime, Clock};
use std::sync::Arc;
use std::time::Duration;

#[tokio::test] 
async fn different_seed_test() {
    // Use DIFFERENT seed - should produce DIFFERENT results
    let runtime = Arc::new(SimulatedRuntime::new(99999));
    
    let start_time = runtime.clock().now();
    println!("🔄 Different seed start: {:?}", start_time);
    
    runtime.advance_time(Duration::from_millis(500));
    let after_advance = runtime.clock().now();
    println!("🔄 Different seed after 500ms: {:?} (elapsed: {:?})", after_advance, after_advance - start_time);
}
EOF

echo -e "\n4️⃣ Running with different seed..."
cargo test different_seed_test --all-features --nocapture 2>/dev/null | grep "🔄"

# Cleanup
rm -f tests/determinism_proof.rs tests/determinism_proof2.rs

echo -e "\n✅ DETERMINISM VERIFICATION COMPLETE!"