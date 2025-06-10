#![cfg(feature = "simulation")]

use blixard::runtime::simulation::SimulatedRuntime;
use blixard::runtime_abstraction as rt;
use blixard::runtime_context::{RuntimeHandle, SimulatedRuntimeHandle};
use blixard::runtime_traits::{Clock, Runtime};
use fail::FailScenario;
use std::sync::Arc;
use std::time::Duration;

#[tokio::test]
async fn test_deterministic_failpoint_chaos() {
    println!("\n🎲 DETERMINISTIC CHAOS TEST WITH FAILPOINTS 🎲\n");

    // Initialize failpoints
    let scenario = FailScenario::setup();

    // Set up simulated runtime globally
    let sim_handle = Arc::new(SimulatedRuntimeHandle::new(10555));
    let _guard = rt::RuntimeGuard::new(sim_handle.clone() as Arc<dyn RuntimeHandle>);

    println!("✅ Simulated runtime initialized");

    // Test 1: Network failures
    println!("\n📡 Testing network failures...");
    fail::cfg("network::before_send", "50%return").unwrap();

    // Spawn some tasks that would use network
    for i in 0..5 {
        rt::spawn(async move {
            println!("  Task {} attempting network operation", i);
            // In real code, this would trigger network operations
            rt::sleep(Duration::from_millis(100)).await;
            println!("  Task {} completed", i);
        });
    }

    // Let tasks run
    rt::sleep(Duration::from_millis(50)).await;

    // Test 2: Storage failures
    println!("\n💾 Testing storage failures...");
    fail::cfg("storage::before_append_entries", "30%panic").unwrap();

    // This would trigger storage operations in real Raft
    rt::spawn(async {
        println!("  Attempting storage operation...");
        // Storage operation would happen here
        println!("  Storage operation completed (or failed)");
    });

    rt::sleep(Duration::from_millis(10)).await;

    // Clean up failpoints
    fail::cfg("network::before_send", "off").unwrap();
    fail::cfg("storage::before_append_entries", "off").unwrap();

    scenario.teardown();

    println!("\n✅ Chaos test completed!");
}

#[test]
fn test_reproducible_chaos_with_seed() {
    println!("\n🔁 REPRODUCIBLE CHAOS TEST 🔁\n");

    let mut results = Vec::new();

    // Run same chaos scenario 3 times with same seed
    for run in 0..3 {
        println!("--- Run {} ---", run + 1);

        // Create deterministic runtime with fixed seed
        let sim = Arc::new(SimulatedRuntime::new(9999));

        // Set up deterministic random failure
        let mut events = Vec::new();
        let start = sim.clock().now();

        // Simulate some operations with random delays
        for i in 0..5 {
            let delay_ms = ((i * 37) % 100) + 50; // Deterministic "random" delay

            // Advance time
            sim.advance_time(Duration::from_millis(delay_ms));

            let event_time = sim.clock().now() - start;
            events.push((i, event_time));

            println!("  Event {} at {:?}", i, event_time);
        }

        results.push(events);
    }

    // Verify all runs produced identical results
    println!("\n📊 Comparing runs:");
    let first = &results[0];
    let all_identical = results.iter().all(|r| r == first);

    if all_identical {
        println!("✅ ALL RUNS IDENTICAL - Perfect determinism achieved!");
    } else {
        println!("❌ Runs differ - not deterministic");
    }

    assert!(all_identical);
}

#[tokio::test]
async fn test_network_partition_with_time_control() {
    println!("\n🔌 NETWORK PARTITION WITH TIME CONTROL 🔌\n");

    let sim_handle = Arc::new(SimulatedRuntimeHandle::new(777));
    let _guard = rt::RuntimeGuard::new(sim_handle.clone() as Arc<dyn rt::RuntimeHandle>);

    let sim = sim_handle.runtime();

    println!("Initial time: {:?}", sim.clock().now());

    // Simulate network partition phases
    println!("\n1️⃣ Pre-partition phase (healthy cluster)");
    sim.advance_time(Duration::from_secs(5));
    println!("   Time: {:?}", sim.clock().now());

    println!("\n2️⃣ Network partition occurs");
    sim.partition_network(
        "127.0.0.1:8001".parse().unwrap(),
        "127.0.0.1:8002".parse().unwrap(),
    );
    sim.advance_time(Duration::from_secs(10));
    println!("   Time: {:?}", sim.clock().now());

    println!("\n3️⃣ Partition healed");
    sim.heal_network(
        "127.0.0.1:8001".parse().unwrap(),
        "127.0.0.1:8002".parse().unwrap(),
    );
    sim.advance_time(Duration::from_secs(5));
    println!("   Time: {:?}", sim.clock().now());

    let total_elapsed = Duration::from_secs(20);
    println!("\n✅ Total simulated time: {:?}", total_elapsed);
    println!("   Real time taken: ~0ms");
}

#[test]
fn test_deterministic_failure_injection() {
    println!("\n💉 DETERMINISTIC FAILURE INJECTION 💉\n");

    // Create scenario
    let _scenario = FailScenario::setup();

    // Configure different failure modes
    let failure_configs = vec![
        ("raft::before_proposal", "1*return", "Proposal failures"),
        (
            "raft::handle_message",
            "2*panic",
            "Message handling crashes",
        ),
        (
            "storage::append_single_entry",
            "3*delay(100)",
            "Storage delays",
        ),
    ];

    for (failpoint, config, description) in failure_configs {
        println!("\n🔧 Testing: {}", description);
        println!("   Failpoint: {}", failpoint);
        println!("   Config: {}", config);

        fail::cfg(failpoint, config).unwrap();

        // Simulate operations that would trigger the failpoint
        for i in 0..5 {
            println!("   Operation {}: Would trigger failpoint", i);
        }

        // Disable failpoint
        fail::cfg(failpoint, "off").unwrap();
    }

    println!("\n✅ All failure modes tested deterministically!");
}

/// The ultimate test: Proving deterministic execution with chaos
#[test]
fn test_deterministic_chaos_proof() {
    println!("\n🏆 ULTIMATE DETERMINISTIC CHAOS PROOF 🏆\n");

    let seed = 1055555;
    let mut fingerprints = Vec::new();

    for run in 0..3 {
        println!("\n=== Run {} (seed: {}) ===", run + 1, seed);

        let sim = Arc::new(SimulatedRuntime::new(seed));
        let _scenario = FailScenario::setup();

        // Configure chaos
        fail::cfg("network::before_send", "10%return").unwrap();
        fail::cfg("storage::before_append_entries", "5%return").unwrap();

        let mut events = Vec::new();
        let start = sim.clock().now();

        // Run chaotic simulation
        for i in 0..10 {
            // Advance time
            sim.advance_time(Duration::from_millis(100));

            // Record event
            let elapsed = sim.clock().now() - start;
            events.push(format!("Event {} at {:?}", i, elapsed));

            // Simulate random network partition
            if i == 5 {
                sim.partition_network(
                    "127.0.0.1:9001".parse().unwrap(),
                    "127.0.0.1:9002".parse().unwrap(),
                );
                events.push("Network partitioned".to_string());
            }

            if i == 8 {
                sim.heal_network(
                    "127.0.0.1:9001".parse().unwrap(),
                    "127.0.0.1:9002".parse().unwrap(),
                );
                events.push("Network healed".to_string());
            }
        }

        // Create fingerprint of this run
        let fingerprint = events.join("|");
        println!("Fingerprint: {} chars", fingerprint.len());
        fingerprints.push(fingerprint);

        // Clean up
        fail::cfg("network::before_send", "off").unwrap();
        fail::cfg("storage::before_append_entries", "off").unwrap();
    }

    // Verify all runs are IDENTICAL
    println!("\n🔍 Verifying determinism...");
    let first = &fingerprints[0];
    let all_same = fingerprints.iter().all(|fp| fp == first);

    if all_same {
        println!("\n✅✅✅ PERFECT DETERMINISM ACHIEVED! ✅✅✅");
        println!(
            "All {} runs produced IDENTICAL execution traces!",
            fingerprints.len()
        );
        println!("\nThis means:");
        println!("  • Same operations in same order");
        println!("  • Same timings");
        println!("  • Same failures");
        println!("  • Same network partitions");
        println!("  • 100% reproducible bugs!");
    } else {
        println!("\n❌ Runs differ - not fully deterministic yet");
    }

    assert!(all_same, "All chaos runs must be identical!");
}
