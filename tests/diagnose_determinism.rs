#![cfg(feature = "simulation")]

use blixard::runtime::simulation::SimulatedRuntime;
use blixard::runtime_traits::{Runtime, Clock};
use std::sync::Arc;
use std::time::Duration;

#[test]
fn diagnose_determinism_failure() {
    println!("\n🔬 DIAGNOSING DETERMINISM FAILURE\n");
    
    // Test 1: Are Instant values deterministic?
    println!("Test 1: Instant creation");
    let sim1 = Arc::new(SimulatedRuntime::new(7777));
    let sim2 = Arc::new(SimulatedRuntime::new(7777)); // Same seed
    
    let instant1 = sim1.clock().now();
    let instant2 = sim2.clock().now();
    
    println!("  Sim1 start: {:?}", instant1);
    println!("  Sim2 start: {:?}", instant2);
    
    if instant1 == instant2 {
        println!("  ✅ Initial instants are identical");
    } else {
        println!("  ❌ Initial instants differ - THIS IS THE PROBLEM!");
    }
    
    // Test 2: Is time advancement deterministic?
    println!("\nTest 2: Time advancement");
    sim1.advance_time(Duration::from_secs(5));
    sim2.advance_time(Duration::from_secs(5));
    
    let after1 = sim1.clock().now();
    let after2 = sim2.clock().now();
    
    println!("  Sim1 after 5s: {:?}", after1);
    println!("  Sim2 after 5s: {:?}", after2);
    
    let elapsed1 = after1 - instant1;
    let elapsed2 = after2 - instant2;
    
    println!("  Sim1 elapsed: {:?}", elapsed1);
    println!("  Sim2 elapsed: {:?}", elapsed2);
    
    if elapsed1 == elapsed2 {
        println!("  ✅ Elapsed times are identical");
    } else {
        println!("  ❌ Elapsed times differ!");
    }
    
    // Test 3: Check the actual Instant representation
    println!("\nTest 3: Instant internals");
    
    // Create multiple runtimes with same seed
    let mut instants = Vec::new();
    for i in 0..3 {
        let sim = Arc::new(SimulatedRuntime::new(7777));
        let now = sim.clock().now();
        instants.push(now);
        println!("  Runtime {} start: {:?}", i + 1, now);
    }
    
    // Check if they're all the same
    let all_same = instants.windows(2).all(|w| w[0] == w[1]);
    
    if all_same {
        println!("  ✅ All instants from same seed are identical");
    } else {
        println!("  ❌ Instants from same seed differ!");
        println!("  This means SimulatedClock is using non-deterministic initialization");
    }
}

#[test]
fn check_instant_source() {
    println!("\n🕐 CHECKING INSTANT SOURCE\n");
    
    // The problem might be that SimulatedClock initializes 
    // its start time from the real system clock
    
    let sim = Arc::new(SimulatedRuntime::new(42));
    let instant1 = sim.clock().now();
    
    // Wait a bit of real time
    std::thread::sleep(Duration::from_millis(10));
    
    // Create another runtime with same seed
    let sim2 = Arc::new(SimulatedRuntime::new(42));
    let instant2 = sim2.clock().now();
    
    println!("First runtime:  {:?}", instant1);
    println!("Second runtime: {:?} (created 10ms later)", instant2);
    
    if instant1 == instant2 {
        println!("✅ Good: Instants are the same despite real time delay");
    } else {
        println!("❌ BAD: Instants differ - using real time for initialization!");
        println!("   This is why determinism is broken!");
    }
}

#[test]
fn minimal_determinism_test() {
    println!("\n🎯 MINIMAL DETERMINISM TEST\n");
    
    // The absolute simplest test of determinism
    let mut results = Vec::new();
    
    for run in 0..3 {
        let sim = Arc::new(SimulatedRuntime::new(9999));
        let start = sim.clock().now();
        
        // Do exact same operations
        sim.advance_time(Duration::from_millis(100));
        sim.advance_time(Duration::from_millis(200));
        sim.advance_time(Duration::from_millis(300));
        
        let end = sim.clock().now();
        let total = end - start;
        
        results.push((start, end, total));
        println!("Run {}: start={:?}, end={:?}, total={:?}", 
                 run + 1, start, end, total);
    }
    
    // Check results
    let (start0, end0, total0) = results[0];
    let all_same = results.iter().all(|&(s, e, t)| {
        s == start0 && e == end0 && t == total0
    });
    
    if all_same {
        println!("\n✅ Determinism achieved!");
    } else {
        println!("\n❌ Not deterministic!");
        println!("The likely cause: SimulatedClock uses Instant::now() for initialization");
    }
}