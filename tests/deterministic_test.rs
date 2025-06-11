#![cfg(feature = "simulation")]

use madsim::time;
use std::time::{Duration, Instant};

/// Test basic MadSim functionality
#[madsim::test]
async fn test_madsim_basics() {
    println!("Testing MadSim basic functionality");

    // Test spawn
    let handle = madsim::task::spawn(async {
        println!("Task spawned!");
        42
    });

    let result = handle.await.unwrap();
    assert_eq!(result, 42);

    // Test sleep
    let start = Instant::now();
    time::sleep(Duration::from_millis(100)).await;
    let elapsed = start.elapsed();

    println!("Sleep elapsed: {:?}", elapsed);
    assert!(elapsed >= Duration::from_millis(100));

    println!("MadSim basic functionality test passed!");
}

/// Test that time control works in MadSim
#[madsim::test]
async fn test_time_control() {
    println!("Testing time control in MadSim");

    // Record start time
    let start = Instant::now();

    // Sleep for 1 second
    time::sleep(Duration::from_secs(1)).await;

    // Check that time advanced
    let elapsed = start.elapsed();
    println!("Time advanced by: {:?}", elapsed);

    // In MadSim, time should advance at least by the requested amount
    assert!(elapsed >= Duration::from_secs(1));
    assert!(elapsed < Duration::from_secs(2)); // Should be close to 1s

    println!("Time control test passed!");
}

/// Test concurrent operations with MadSim
#[madsim::test]
async fn test_concurrent_operations() {
    println!("Testing concurrent operations");

    let start = Instant::now();

    // Spawn multiple tasks
    let handles: Vec<_> = (0..5)
        .map(|i| {
            madsim::task::spawn(async move {
                time::sleep(Duration::from_millis(100 * (i + 1) as u64)).await;
                (i, Instant::now())
            })
        })
        .collect();

    // Wait for all tasks and collect results
    let mut results = Vec::new();
    for handle in handles {
        results.push(handle.await.unwrap());
    }

    // Sort by task id
    results.sort_by_key(|(i, _)| *i);

    // Verify that tasks completed in the expected time order
    for (i, (task_id, end_time)) in results.iter().enumerate() {
        let elapsed = *end_time - start;
        let expected = Duration::from_millis(100 * (task_id + 1) as u64);

        println!(
            "Task {} completed after {:?} (expected ~{:?})",
            task_id, elapsed, expected
        );

        assert!(elapsed >= expected);
        assert!(elapsed < expected + Duration::from_millis(100)); // Allow some overhead
    }

    println!("Concurrent operations test passed!");
}

/// Test deterministic behavior across runs
#[madsim::test]
async fn test_deterministic_behavior() {
    println!("Testing deterministic behavior");

    // Run the same sequence multiple times
    let mut sequences = Vec::new();

    for run in 0..3 {
        let start = Instant::now();
        let mut sequence = Vec::new();

        // Perform a series of operations
        for i in 0..3 {
            time::sleep(Duration::from_millis(50)).await;
            let elapsed = start.elapsed();
            sequence.push(elapsed);
            println!("Run {}, step {}: {:?}", run, i, elapsed);
        }

        sequences.push(sequence);
    }

    // Verify that all runs produced similar timing
    for run in 1..sequences.len() {
        for step in 0..sequences[0].len() {
            let diff = (sequences[run][step].as_millis() as i64
                - sequences[0][step].as_millis() as i64)
                .abs();

            // Allow small variations
            assert!(
                diff <= 10,
                "Run {} step {} differs by {}ms from run 0",
                run,
                step,
                diff
            );
        }
    }

    println!("Deterministic behavior verified!");
}
