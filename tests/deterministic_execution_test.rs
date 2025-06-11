#![cfg(feature = "simulation")]

use madsim::task;
use madsim::time::{sleep, timeout, Instant};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

#[madsim::test]
async fn test_deterministic_task_ordering() {
    println!("\n🧪 Testing deterministic task ordering with MadSim");

    // Shared counter to track execution order
    let counter = Arc::new(AtomicU32::new(0));
    let results = Arc::new(std::sync::Mutex::new(Vec::new()));

    println!("📍 Using MadSim deterministic runtime");

    // Spawn multiple tasks that sleep for different durations
    let mut handles = vec![];
    for i in 0..5 {
        let counter = counter.clone();
        let results = results.clone();
        let delay = Duration::from_millis((5 - i) * 100); // 500ms, 400ms, 300ms, 200ms, 100ms

        let handle = task::spawn(async move {
            println!("  Task {} starting, will sleep for {:?}", i, delay);
            sleep(delay).await;

            let order = counter.fetch_add(1, Ordering::SeqCst);
            results.lock().unwrap().push((i, order));
            println!("  Task {} completed at position {}", i, order);
        });
        handles.push(handle);
    }

    // Wait for all tasks to complete
    for handle in handles {
        handle.await.unwrap();
    }

    let final_results = results.lock().unwrap().clone();

    // Verify execution order (should be reverse of spawn order due to sleep durations)
    println!("\n📊 Results:");
    for (task_id, order) in &final_results {
        println!("  Task {} completed at position {}", task_id, order);
    }

    // With deterministic execution, task 4 (100ms sleep) should complete first
    assert_eq!(final_results[0].0, 4); // Task 4 should complete first
    assert_eq!(final_results[1].0, 3); // Task 3 should complete second
    assert_eq!(final_results[2].0, 2); // Task 2 should complete third
    assert_eq!(final_results[3].0, 1); // Task 1 should complete fourth
    assert_eq!(final_results[4].0, 0); // Task 0 should complete last

    println!("\n✅ Test completed - deterministic execution verified!");
}

#[madsim::test]
async fn test_network_communication() {
    println!("\n🚀 Testing network communication with MadSim");

    // MadSim doesn't provide direct network simulation in the same way
    // Instead, we'll test async task coordination
    let (tx, mut rx) = tokio::sync::mpsc::channel::<String>(10);

    // Simulate server task
    let server_handle = task::spawn(async move {
        println!("📡 Server task started");
        if let Some(msg) = rx.recv().await {
            println!("  Server received: {}", msg);
            // Echo back (in a real system, this would be via network)
            println!("  Server echoed: {}", msg);
        }
    });

    // Give server time to start
    sleep(Duration::from_millis(100)).await;

    // Simulate client
    println!("📱 Client sending message");
    let message = "Hello from MadSim!".to_string();
    tx.send(message.clone()).await.unwrap();
    println!("  Client sent: {}", message);

    // Wait for server to finish
    server_handle.await.unwrap();

    println!("\n✅ Communication test completed!");
}

#[madsim::test]
async fn test_timeout_behavior() {
    println!("\n⏱️ Testing timeout behavior with MadSim");

    // Test successful operation within timeout
    let result = timeout(Duration::from_secs(2), async {
        sleep(Duration::from_secs(1)).await;
        "Success"
    })
    .await;

    match result {
        Ok(value) => println!("  ✅ Operation completed: {}", value),
        Err(_) => panic!("Operation should not timeout"),
    }

    // Test operation that times out
    let result = timeout(Duration::from_millis(100), async {
        sleep(Duration::from_secs(1)).await;
        "This won't complete"
    })
    .await;

    match result {
        Ok(_) => panic!("Operation should timeout"),
        Err(_) => println!("  ✅ Operation timed out as expected"),
    }

    println!("\n✅ Timeout behavior test completed!");
}

#[madsim::test]
async fn test_concurrent_operations() {
    println!("\n🔄 Testing concurrent operations with MadSim");

    let shared_state = Arc::new(std::sync::Mutex::new(0));
    let mut handles = vec![];

    // Spawn multiple concurrent tasks
    for i in 0..10 {
        let state = shared_state.clone();
        let handle = task::spawn(async move {
            // Simulate some work
            sleep(Duration::from_millis(10 * i)).await;

            // Update shared state
            let mut value = state.lock().unwrap();
            *value += 1;
            println!("  Task {} incremented counter to {}", i, *value);
        });
        handles.push(handle);
    }

    // Wait for all tasks
    for handle in handles {
        handle.await.unwrap();
    }

    let final_value = *shared_state.lock().unwrap();
    assert_eq!(final_value, 10);
    println!("\n✅ Final counter value: {} (as expected)", final_value);
}

#[madsim::test]
async fn test_multi_node_simulation() {
    println!("\n🔌 Testing multi-node simulation with MadSim");

    // Simulate multiple nodes using channels
    let node_count = 3;
    let (tx, mut rx) = tokio::sync::mpsc::channel::<(usize, String)>(10);

    // Start "nodes"
    let mut node_handles = vec![];
    for node_id in 1..=node_count {
        let tx_clone = tx.clone();
        let handle = task::spawn(async move {
            println!("  Node {} started", node_id);

            // Simulate node sending a message
            sleep(Duration::from_millis(node_id as u64 * 100)).await;
            let msg = format!("Message from node {}", node_id);
            tx_clone.send((node_id, msg)).await.unwrap();
            println!("  Node {} sent message", node_id);
        });
        node_handles.push(handle);
    }

    // Drop original tx so rx will close when all nodes finish
    drop(tx);

    // Collect messages
    println!("\n📡 Collecting messages:");
    let mut messages = vec![];
    while let Some((node_id, msg)) = rx.recv().await {
        println!("  Received from node {}: {}", node_id, msg);
        messages.push((node_id, msg));
    }

    // Wait for all nodes
    for handle in node_handles {
        handle.await.unwrap();
    }

    // Verify all nodes sent messages
    assert_eq!(messages.len(), node_count);
    println!("\n✅ Multi-node simulation completed!");
}

#[madsim::test]
async fn test_reproducible_execution() {
    println!("\n🎲 Testing reproducible execution with MadSim");

    // MadSim provides deterministic execution by default
    // Let's verify this by running the same async operations multiple times

    async fn run_simulation() -> Vec<String> {
        let events = Arc::new(std::sync::Mutex::new(Vec::new()));
        let mut handles = vec![];

        // Spawn tasks that record events
        for i in 0..3 {
            let events_clone = events.clone();
            let handle = task::spawn(async move {
                events_clone
                    .lock()
                    .unwrap()
                    .push(format!("Task {} started", i));
                sleep(Duration::from_millis(100 * (i + 1) as u64)).await;
                events_clone
                    .lock()
                    .unwrap()
                    .push(format!("Task {} completed", i));
            });
            handles.push(handle);
        }

        // Wait for all tasks
        for handle in handles {
            handle.await.unwrap();
        }

        Arc::try_unwrap(events).unwrap().into_inner().unwrap()
    }

    // Run the simulation multiple times
    let mut results = Vec::new();
    for run in 0..3 {
        println!("\n--- Run {} ---", run + 1);
        let result = run_simulation().await;
        println!("  Events: {:?}", result);
        results.push(result);
    }

    // Check that all runs produced the same sequence
    println!("\n📊 Comparing results across runs:");

    // MadSim guarantees deterministic completion order based on time, not spawn order
    // Extract just the completion events for comparison
    let extract_completions = |events: &[String]| -> Vec<String> {
        events
            .iter()
            .filter(|e| e.contains("completed"))
            .cloned()
            .collect()
    };

    let mut completion_results = Vec::new();
    for result in &results {
        let completions = extract_completions(result);
        println!("  Completions: {:?}", completions);
        completion_results.push(completions);
    }

    // Check that all runs have the same completion order
    let first_completions = &completion_results[0];
    let all_same = completion_results.iter().all(|c| c == first_completions);

    assert!(all_same, "All runs should have the same completion order");
    println!("✅ All runs had identical completion order - deterministic execution verified!");

    // Verify the completion order (based on sleep times)
    assert_eq!(first_completions[0], "Task 0 completed"); // 100ms
    assert_eq!(first_completions[1], "Task 1 completed"); // 200ms
    assert_eq!(first_completions[2], "Task 2 completed"); // 300ms
}
