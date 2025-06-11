#![cfg(feature = "simulation")]

use std::time::{Duration, Instant};

#[madsim::test]
async fn test_madsim_time_control() {
    println!("Testing MadSim time control");

    // Get initial time
    let start = Instant::now();
    println!("Start time: {:?}", start);

    // Sleep for 10 seconds - in simulation this is instant
    madsim::time::sleep(Duration::from_secs(10)).await;

    // Check time advanced
    let end = Instant::now();
    let elapsed = end - start;
    println!("End time: {:?}, elapsed: {:?}", end, elapsed);

    // Time should advance by at least the requested amount
    assert!(elapsed >= Duration::from_secs(10));
    assert!(elapsed < Duration::from_secs(11)); // Should be close to 10s
    println!("✓ Time advanced correctly in MadSim simulation");
}

#[madsim::test]
async fn test_madsim_intercepts_all_apis() {
    println!("Testing MadSim API interception");

    // Test that std::time::Instant is intercepted
    let start = Instant::now();
    
    // Test that madsim::time::sleep is used
    madsim::time::sleep(Duration::from_millis(100)).await;
    
    let elapsed = start.elapsed();
    println!("Elapsed time: {:?}", elapsed);

    // This should be at least 100ms of simulated time
    assert!(elapsed >= Duration::from_millis(100));
    assert!(elapsed < Duration::from_millis(110)); // Close to requested time
    
    println!("✓ MadSim successfully intercepts time APIs");
}

#[madsim::test]
async fn test_deterministic_task_ordering() {
    println!("Testing deterministic task execution order");
    
    use std::sync::Arc;
    use tokio::sync::Mutex;
    
    let events = Arc::new(Mutex::new(Vec::new()));
    
    // Spawn tasks that sleep for different durations
    let events1 = events.clone();
    let task1 = madsim::task::spawn(async move {
        madsim::time::sleep(Duration::from_millis(100)).await;
        events1.lock().await.push("Task 1 completed");
    });
    
    let events2 = events.clone();
    let task2 = madsim::task::spawn(async move {
        madsim::time::sleep(Duration::from_millis(50)).await;
        events2.lock().await.push("Task 2 completed");
    });
    
    let events3 = events.clone();
    let task3 = madsim::task::spawn(async move {
        madsim::time::sleep(Duration::from_millis(150)).await;
        events3.lock().await.push("Task 3 completed");
    });
    
    // Wait for all tasks
    let _ = tokio::join!(task1, task2, task3);
    
    // Check execution order
    let final_events = events.lock().await;
    println!("Execution order: {:?}", *final_events);
    
    // Tasks should complete in order of their sleep duration
    assert_eq!(final_events[0], "Task 2 completed"); // 50ms
    assert_eq!(final_events[1], "Task 1 completed"); // 100ms
    assert_eq!(final_events[2], "Task 3 completed"); // 150ms
    
    println!("✓ Tasks executed in deterministic order");
}

#[madsim::test]
async fn test_instant_precision() {
    println!("Testing time measurement precision");
    
    let mut measurements = Vec::new();
    
    // Take multiple time measurements
    for i in 0..5 {
        let start = Instant::now();
        madsim::time::sleep(Duration::from_millis(10)).await;
        let elapsed = start.elapsed();
        measurements.push(elapsed);
        println!("Measurement {}: {:?}", i + 1, elapsed);
    }
    
    // All measurements should be consistent
    for elapsed in &measurements {
        assert!(*elapsed >= Duration::from_millis(10));
        assert!(*elapsed < Duration::from_millis(15)); // Allow small overhead
    }
    
    println!("✓ Time measurements are consistent and precise");
}
