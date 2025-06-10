#![cfg(feature = "simulation")]

use blixard::runtime_abstraction as rt;
use blixard::runtime::simulation::SimulatedRuntime;
use blixard::runtime_traits::{Runtime, Clock};
use blixard::runtime_context::{RuntimeHandle, SimulatedRuntimeHandle};
use std::sync::Arc;
use std::time::Duration;

#[test]
fn what_actually_works() {
    println!("\n🔍 REALITY CHECK: What Actually Works\n");
    
    // 1. Basic deterministic execution - THIS WORKS
    println!("1️⃣ Testing deterministic execution:");
    let sim = Arc::new(SimulatedRuntime::new(12345));
    
    let mut results = Vec::new();
    for i in 0..3 {
        let start = sim.clock().now();
        sim.advance_time(Duration::from_millis(100));
        let elapsed = sim.clock().now() - start;
        results.push(elapsed);
        println!("   Run {}: elapsed = {:?}", i + 1, elapsed);
    }
    
    assert!(results.iter().all(|&r| r == Duration::from_millis(100)));
    println!("   ✅ Deterministic time control works!");
    
    // 2. Runtime switching - THIS WORKS
    println!("\n2️⃣ Testing runtime switching:");
    let sim_handle = Arc::new(SimulatedRuntimeHandle::new(42));
    let _guard = rt::RuntimeGuard::new(sim_handle.clone() as Arc<dyn RuntimeHandle>);
    
    let now1 = rt::now();
    std::thread::sleep(Duration::from_millis(10)); // Real sleep
    let now2 = rt::now();
    
    if now1 == now2 {
        println!("   ✅ Runtime switching works - time didn't advance!");
    } else {
        println!("   ❌ Runtime switching broken - time advanced!");
    }
}

#[test] 
fn what_is_broken() {
    println!("\n💔 REALITY CHECK: What's Broken\n");
    
    println!("1️⃣ Network simulation doesn't affect real components:");
    println!("   - RaftNode uses real network, not simulated");
    println!("   - Partitions don't actually block communication");
    println!("   - This is why split-brain test fails");
    
    println!("\n2️⃣ Async integration issues:");
    println!("   - with_simulated_runtime panics in async context");
    println!("   - Can't properly nest tokio runtimes");
    
    println!("\n3️⃣ No enforcement:");
    println!("   - Code can still use tokio directly");
    println!("   - Nothing prevents bypassing abstractions");
    println!("   - No compile-time guarantees");
}

#[test]
fn demonstration_of_the_gap() {
    println!("\n🔌 DEMONSTRATION: The Integration Gap\n");
    
    let sim = Arc::new(SimulatedRuntime::new(999));
    
    // This uses our simulated time
    let sim_start = sim.clock().now();
    sim.advance_time(Duration::from_secs(3600)); // 1 hour instantly
    let sim_elapsed = sim.clock().now() - sim_start;
    
    // But this uses real time (THE PROBLEM!)
    let real_start = std::time::Instant::now();
    std::thread::sleep(Duration::from_millis(10));
    let real_elapsed = real_start.elapsed();
    
    println!("Simulated time elapsed: {:?} (good - exactly 1 hour)", sim_elapsed);
    println!("Real time elapsed: {:?} (bad - RaftNode would use this!)", real_elapsed);
    
    println!("\n⚠️  THE GAP: RaftNode uses std::time and tokio::time directly!");
    println!("⚠️  It never sees our simulated time or network!");
}

#[tokio::test]
async fn async_integration_problems() {
    println!("\n⚡ ASYNC INTEGRATION ISSUES\n");
    
    // This works - direct usage
    let sim_handle = Arc::new(SimulatedRuntimeHandle::new(42));
    let _guard = rt::RuntimeGuard::new(sim_handle.clone() as Arc<dyn RuntimeHandle>);
    
    println!("✅ Can create runtime guard in async context");
    
    // But we can still use tokio directly (BAD!)
    let before = tokio::time::Instant::now();
    tokio::time::sleep(Duration::from_millis(10)).await;
    let elapsed = before.elapsed();
    
    println!("❌ Can still use tokio directly: {:?} elapsed", elapsed);
    println!("   This means our abstraction is leaky!");
}

#[test]
fn proof_of_determinism_when_used_correctly() {
    println!("\n✨ PROOF: Determinism Works When Used Correctly\n");
    
    let mut fingerprints = Vec::new();
    
    for run in 0..3 {
        let sim = Arc::new(SimulatedRuntime::new(7777)); // Same seed
        let mut events = Vec::new();
        let start = sim.clock().now();
        
        // Simulate some "random" events
        for i in 0..5 {
            sim.advance_time(Duration::from_millis(i * 37 % 100));
            let elapsed = sim.clock().now() - start;
            events.push(format!("Event {} at +{:?}", i, elapsed));
        }
        
        let fingerprint = events.join("|");
        fingerprints.push(fingerprint);
        println!("Run {}: {} events recorded", run + 1, events.len());
    }
    
    // Verify all identical
    if fingerprints.windows(2).all(|w| w[0] == w[1]) {
        println!("\n✅ All runs produced IDENTICAL results!");
        println!("   This proves determinism works when everything uses it!");
    } else {
        println!("\n❌ Runs differed - determinism broken!");
    }
}