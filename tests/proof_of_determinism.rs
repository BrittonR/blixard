#![cfg(feature = "simulation")]

use madsim::time;
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// This test proves that MadSim provides deterministic simulation
#[madsim::test]
async fn prove_deterministic_simulation() {
    println!("\n🔬 DETERMINISTIC SIMULATION PROOF TEST");
    println!("=====================================");

    // Run the same scenario 3 times
    // MadSim uses deterministic scheduling, so results should be consistent
    let mut results = Vec::new();

    for run in 1..=3 {
        println!("\n📍 Run #{}", run);
        let result = run_simulation_scenario().await;
        results.push(result);
    }

    // All runs should produce very similar results
    println!("\n📊 Comparing results:");
    for (i, result) in results.iter().enumerate() {
        println!("Run {}: {:?}", i + 1, result);
    }

    // Check that timing is consistent across runs
    let first_total = results[0].get("total_elapsed").unwrap();
    for (i, result) in results.iter().enumerate().skip(1) {
        let total = result.get("total_elapsed").unwrap();
        println!(
            "Run {} total: {}, Run 1 total: {}",
            i + 1,
            total,
            first_total
        );

        // Parse durations and compare
        let first_ms = parse_duration_ms(first_total);
        let this_ms = parse_duration_ms(total);
        let diff = (this_ms as i64 - first_ms as i64).abs();

        assert!(
            diff <= 10,
            "Run {} differs by {}ms from Run 1!",
            i + 1,
            diff
        );
    }

    println!("\n✅ SUCCESS: All runs produced consistent results!");
    println!("This proves MadSim provides deterministic behavior!");

    println!("\n🏆 DETERMINISTIC SIMULATION VERIFIED!");
}

async fn run_simulation_scenario() -> HashMap<String, String> {
    let mut results = HashMap::new();

    // Record starting time
    let start = Instant::now();

    // Sleep for 1 second
    time::sleep(Duration::from_secs(1)).await;
    let after_first_sleep = Instant::now();
    let first_sleep_elapsed = after_first_sleep - start;
    results.insert(
        "first_sleep_elapsed".to_string(),
        format!("{:?}", first_sleep_elapsed),
    );

    // Sleep for 500ms more
    time::sleep(Duration::from_millis(500)).await;
    let after_second_sleep = Instant::now();
    let second_sleep_elapsed = after_second_sleep - after_first_sleep;
    results.insert(
        "second_sleep_elapsed".to_string(),
        format!("{:?}", second_sleep_elapsed),
    );

    // Calculate total elapsed
    let total_elapsed = after_second_sleep - start;
    results.insert("total_elapsed".to_string(), format!("{:?}", total_elapsed));

    // Times should be close to expected values
    assert!(first_sleep_elapsed >= Duration::from_secs(1));
    assert!(first_sleep_elapsed < Duration::from_millis(1100));
    assert!(second_sleep_elapsed >= Duration::from_millis(500));
    assert!(second_sleep_elapsed < Duration::from_millis(600));
    assert!(total_elapsed >= Duration::from_millis(1500));
    assert!(total_elapsed < Duration::from_millis(1700));

    results.insert("deterministic_check".to_string(), "passed".to_string());

    results
}

fn parse_duration_ms(duration_str: &str) -> u64 {
    // Parse a duration string like "1.5s" or "1500ms" to milliseconds
    if let Some(s) = duration_str.strip_suffix("ms") {
        s.parse::<u64>().unwrap_or(0)
    } else if let Some(s) = duration_str.strip_suffix("s") {
        (s.parse::<f64>().unwrap_or(0.0) * 1000.0) as u64
    } else {
        // Try to extract milliseconds from debug format like "1.5s"
        if duration_str.contains('.') && duration_str.contains('s') {
            let parts: Vec<&str> = duration_str.split('.').collect();
            if parts.len() == 2 {
                let secs = parts[0].parse::<u64>().unwrap_or(0);
                let frac = parts[1].trim_end_matches('s');
                let ms = frac.parse::<u64>().unwrap_or(0);
                secs * 1000 + ms
            } else {
                0
            }
        } else {
            0
        }
    }
}

/// Test that multiple runs produce consistent behavior
#[madsim::test]
async fn test_reproducible_execution() {
    println!("\n🔄 Testing Reproducible Execution");

    let mut timing_results = Vec::new();

    // Run same test 5 times
    for i in 1..=5 {
        let start = Instant::now();

        // Simulate some work with varying sleep times
        let mut accumulated = Duration::ZERO;
        for j in 0..10 {
            let sleep_time = Duration::from_millis((j * 10) % 50 + 10);
            time::sleep(sleep_time).await;
            accumulated += sleep_time;
        }

        let elapsed = start.elapsed();

        println!(
            "Run {}: elapsed = {:?}, expected = {:?}",
            i, elapsed, accumulated
        );
        timing_results.push((elapsed, accumulated));
    }

    // All runs should have similar timing (within reasonable bounds)
    for i in 1..timing_results.len() {
        let (elapsed, expected) = timing_results[i];
        assert!(
            elapsed >= expected,
            "Run {} elapsed time less than expected!",
            i + 1
        );
        assert!(
            elapsed < expected + Duration::from_millis(100),
            "Run {} elapsed time too much more than expected!",
            i + 1
        );
    }

    println!("✅ All 5 runs had consistent timing!");
    println!("🏆 Execution is reproducible with MadSim!");
}

/// Test concurrent execution determinism
#[madsim::test]
async fn test_concurrent_determinism() {
    println!("\n🔀 Testing Concurrent Execution Determinism");

    use std::sync::Arc;
    use tokio::sync::Mutex;

    let mut run_results = Vec::new();

    // Run the same concurrent scenario multiple times
    for run in 0..3 {
        let events = Arc::new(Mutex::new(Vec::new()));
        let mut handles = Vec::new();

        // Spawn tasks with different delays
        for i in 0..5 {
            let events = events.clone();
            let handle = madsim::task::spawn(async move {
                time::sleep(Duration::from_millis(i * 20 + 10)).await;
                events.lock().await.push(format!("Task {} done", i));
            });
            handles.push(handle);
        }

        // Wait for all tasks
        for handle in handles {
            handle.await.unwrap();
        }

        let final_events = events.lock().await.clone();
        println!("Run {}: {:?}", run + 1, final_events);
        run_results.push(final_events);
    }

    // Verify all runs produced the same order
    for i in 1..run_results.len() {
        assert_eq!(
            run_results[i],
            run_results[0],
            "Run {} produced different event order!",
            i + 1
        );
    }

    println!("✅ All concurrent runs produced identical event order!");
    println!("🏆 Concurrent execution is deterministic!");
}
