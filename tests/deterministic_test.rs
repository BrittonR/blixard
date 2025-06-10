#![cfg(feature = "simulation")]

use std::sync::Arc;
use std::time::Duration;

/// Test that our runtime abstraction works
#[tokio::test]
async fn test_runtime_abstraction() {
    // Initialize simulation mode
    #[cfg(feature = "simulation")]
    blixard::runtime_abstraction::simulated::init_simulation();
    
    println!("Testing runtime abstraction");
    
    // Test spawn
    let handle = blixard::runtime_abstraction::spawn(async {
        println!("Task spawned!");
        42
    });
    
    let result = handle.await.unwrap();
    assert_eq!(result, 42);
    
    // Test sleep
    let start = blixard::runtime_abstraction::now();
    blixard::runtime_abstraction::sleep(Duration::from_millis(100)).await;
    let elapsed = blixard::runtime_abstraction::now() - start;
    
    println!("Sleep elapsed: {:?}", elapsed);
    assert!(elapsed >= Duration::from_millis(100));
    
    // Test interval
    let mut interval = blixard::runtime_abstraction::interval(Duration::from_millis(50));
    
    for i in 0..3 {
        interval.tick().await;
        println!("Interval tick {}", i);
    }
    
    println!("Runtime abstraction test passed!");
}

/// Test that we can control time in simulation mode
#[tokio::test]
#[cfg(feature = "simulation")]
async fn test_time_control() {
    use blixard::runtime::SimulatedRuntime;
    
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