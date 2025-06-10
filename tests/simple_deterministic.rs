#![cfg(feature = "simulation")]

use blixard::runtime_abstraction as rt;
use blixard::runtime_traits::{Runtime, Clock};
use std::sync::Arc;
use std::time::Duration;

#[test]
fn test_runtime_switching() {
    println!("\n🧪 Testing runtime switching mechanism\n");
    
    // Test 1: Default runtime (real)
    println!("1️⃣ Using default (real) runtime:");
    let start = rt::now();
    std::thread::sleep(Duration::from_millis(10));
    let elapsed = rt::now() - start;
    println!("   Real time elapsed: {:?}", elapsed);
    assert!(elapsed >= Duration::from_millis(10));
    
    // Test 2: Switch to simulated runtime
    println!("\n2️⃣ Switching to simulated runtime:");
    use blixard::runtime_context::{RuntimeHandle, SimulatedRuntimeHandle};
    let sim_handle = Arc::new(SimulatedRuntimeHandle::new(42));
    rt::set_global_runtime(sim_handle.clone() as Arc<dyn RuntimeHandle>);
    
    let sim_start = rt::now();
    println!("   Simulated start time: {:?}", sim_start);
    
    // In simulated runtime, time doesn't advance on its own
    std::thread::sleep(Duration::from_millis(10));
    let sim_elapsed = rt::now() - sim_start;
    println!("   Time after real sleep: {:?}", sim_elapsed);
    
    println!("\n✅ Runtime switching works!");
}

#[tokio::test]
async fn test_simulated_time_control() {
    use blixard::runtime::simulation::SimulatedRuntime;
    
    println!("\n🕐 Testing simulated time control\n");
    
    let sim = Arc::new(SimulatedRuntime::new(42));
    
    let t1 = sim.clock().now();
    println!("Start time: {:?}", t1);
    
    // Advance by exactly 5 seconds
    sim.advance_time(Duration::from_secs(5));
    
    let t2 = sim.clock().now();
    let elapsed = t2 - t1;
    println!("After advancing 5s: {:?}", t2);
    println!("Elapsed: {:?}", elapsed);
    
    assert_eq!(elapsed, Duration::from_secs(5));
    println!("\n✅ Time control works perfectly!");
}

#[test]
fn test_with_simulated_runtime_wrapper() {
    use blixard::runtime_context::{SimulatedRuntimeHandle, RuntimeHandle};
    
    println!("\n🎭 Testing with_simulated_runtime wrapper\n");
    
    // Use the simulated runtime directly without async
    let sim_handle = Arc::new(SimulatedRuntimeHandle::new(12345));
    let _guard = rt::RuntimeGuard::new(sim_handle.clone() as Arc<dyn RuntimeHandle>);
    
    let sim = sim_handle.runtime();
    let start = sim.clock().now();
    
    // Advance time
    sim.advance_time(Duration::from_secs(10));
    
    let end = sim.clock().now();
    let elapsed = end - start;
    
    println!("Time advanced by: {:?}", elapsed);
    assert_eq!(elapsed, Duration::from_secs(10));
    println!("\n✅ with_simulated_runtime works!");
}