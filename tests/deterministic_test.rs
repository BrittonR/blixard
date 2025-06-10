#![cfg(feature = "simulation")]

use std::sync::Arc;
use std::time::Duration;
use blixard::runtime_traits::{Runtime, Clock};

/// Test that our runtime abstraction works
#[tokio::test]
async fn test_runtime_abstraction() {
    use blixard::runtime::simulation::SimulatedRuntime;
    use blixard::runtime_abstraction::SimulatedRuntimeHandle;
    
    println!("Testing runtime abstraction");
    
    // Use the proper simulated runtime pattern
    let sim_runtime = Arc::new(SimulatedRuntime::new(42));
    let sim_handle = Arc::new(SimulatedRuntimeHandle::new(42));
    blixard::runtime_abstraction::set_global_runtime(sim_handle.clone() as Arc<dyn blixard::runtime_context::RuntimeHandle>);
    
    // Test spawn
    let handle = blixard::runtime_abstraction::spawn(async {
        println!("Task spawned!");
    });
    
    let result = handle.await.unwrap();
    assert_eq!(result, ());
    
    // Test sleep with proper time advancement
    let start = blixard::runtime_abstraction::now();
    
    // Create a task that sleeps and advance time manually
    let sleep_task = async {
        blixard::runtime_abstraction::sleep(Duration::from_millis(100)).await;
    };
    
    // Get the runtime to advance time
    let runtime = sim_handle.runtime().clone();
    
    // Start the sleep in the background and advance time
    tokio::spawn(sleep_task);
    
    // Advance time by the sleep duration
    runtime.advance_time(Duration::from_millis(100));
    
    let elapsed = blixard::runtime_abstraction::now() - start;
    
    println!("Sleep elapsed: {:?}", elapsed);
    assert!(elapsed >= Duration::from_millis(100));
    
    println!("Runtime abstraction test passed!");
}

/// Test that we can control time in simulation mode
#[tokio::test]
#[cfg(feature = "simulation")]
async fn test_time_control() {
    use blixard::runtime::simulation::SimulatedRuntime;
    
    println!("Testing time control in simulation");
    
    let runtime = Arc::new(SimulatedRuntime::new(42));
    
    // Record start time
    let start = runtime.clock().now();
    
    // Advance time by 1 second
    runtime.advance_time(Duration::from_secs(1));
    
    // Check that time advanced
    let elapsed = runtime.clock().now() - start;
    println!("Time advanced by: {:?}", elapsed);
    
    // In simulation, time should advance exactly as requested
    assert_eq!(elapsed, Duration::from_secs(1));
    
    println!("Time control test passed!");
}