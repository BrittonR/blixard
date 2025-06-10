#![cfg(feature = "simulation")]

use blixard::runtime::simulation::SimulatedRuntime;
use blixard::runtime_traits::{Clock, Runtime};
use std::sync::Arc;
use std::time::Duration;

#[tokio::test]
async fn test_simulated_time_advances() {
    println!("Testing simulated time control");

    // Create a simulated runtime
    let sim_runtime = Arc::new(SimulatedRuntime::new(105));

    // Get initial time
    let start = sim_runtime.clock().now();
    println!("Start time: {:?}", start);

    // Advance time without any actual waiting
    sim_runtime.advance_time(Duration::from_secs(10));

    // Check time advanced
    let end = sim_runtime.clock().now();
    let elapsed = end - start;
    println!("End time: {:?}, elapsed: {:?}", end, elapsed);

    // This should be exactly 10 seconds, not dependent on real time
    assert_eq!(elapsed, Duration::from_secs(10));
    println!("✓ Time advanced correctly in simulation");
}

#[tokio::test]
async fn test_current_implementation_uses_real_time() {
    println!("Testing current runtime abstraction");

    // This test shows that our current abstraction still uses real time
    let start = blixard::runtime_abstraction::now();

    // This will actually sleep for 10ms
    blixard::runtime_abstraction::sleep(Duration::from_millis(10)).await;

    let elapsed = blixard::runtime_abstraction::now() - start;
    println!("Real elapsed time: {:?}", elapsed);

    // This will be at least 10ms of real time
    assert!(elapsed >= Duration::from_millis(10));
    println!("✗ Still using real time, not simulated time");
}
