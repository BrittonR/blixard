#![cfg(feature = "simulation")]

use madsim::time;
use std::time::{Duration, Instant};

#[madsim::test]
async fn test_simulated_time_control() {
    println!("\n🕐 Testing simulated time control\n");

    let t1 = Instant::now();
    println!("Start time: {:?}", t1);

    // Sleep for 5 seconds
    time::sleep(Duration::from_secs(5)).await;

    let t2 = Instant::now();
    let elapsed = t2 - t1;
    println!("After sleeping 5s: {:?}", t2);
    println!("Elapsed: {:?}", elapsed);

    // With MadSim, time advances precisely
    assert!(elapsed >= Duration::from_secs(5));
    assert!(elapsed < Duration::from_secs(6)); // Should be close to 5s
    println!("\n✅ Time control works perfectly!");
}

#[madsim::test]
async fn test_deterministic_execution() {
    println!("\n🎭 Testing deterministic execution\n");

    // Run the same sequence multiple times
    let mut results = Vec::new();

    for i in 0..3 {
        let start = Instant::now();

        // Perform some time-based operations
        time::sleep(Duration::from_millis(100)).await;
        let mid = Instant::now();
        time::sleep(Duration::from_millis(200)).await;
        let end = Instant::now();

        let result = (mid - start, end - mid, end - start);
        results.push(result);

        println!(
            "Run {}: first={:?}, second={:?}, total={:?}",
            i, result.0, result.1, result.2
        );
    }

    // In MadSim, timing should be consistent
    for i in 1..results.len() {
        // Allow small variations due to test execution overhead
        assert!((results[i].0.as_millis() as i64 - results[0].0.as_millis() as i64).abs() <= 10);
        assert!((results[i].1.as_millis() as i64 - results[0].1.as_millis() as i64).abs() <= 10);
        assert!((results[i].2.as_millis() as i64 - results[0].2.as_millis() as i64).abs() <= 10);
    }

    println!("\n✅ Deterministic execution verified!");
}

#[madsim::test]
async fn test_concurrent_time_advance() {
    println!("\n⚡ Testing concurrent time operations\n");

    let start = Instant::now();

    // Spawn multiple tasks that sleep for different durations
    let handles = vec![
        madsim::task::spawn(async move {
            time::sleep(Duration::from_millis(100)).await;
            Instant::now()
        }),
        madsim::task::spawn(async move {
            time::sleep(Duration::from_millis(200)).await;
            Instant::now()
        }),
        madsim::task::spawn(async move {
            time::sleep(Duration::from_millis(300)).await;
            Instant::now()
        }),
    ];

    // Wait for all tasks
    let mut times = Vec::new();
    for handle in handles {
        times.push(handle.await.unwrap());
    }

    // Check that times are in expected order
    for i in 1..times.len() {
        assert!(
            times[i] > times[i - 1],
            "Times should be in increasing order"
        );
    }

    // Check durations
    let durations: Vec<_> = times.iter().map(|t| *t - start).collect();
    println!("Task durations: {:?}", durations);

    assert!(durations[0] >= Duration::from_millis(100));
    assert!(durations[1] >= Duration::from_millis(200));
    assert!(durations[2] >= Duration::from_millis(300));

    println!("\n✅ Concurrent time operations work correctly!");
}
