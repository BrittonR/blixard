//! Simple Raft transport benchmark

use raft::prelude::*;
use std::time::Instant;

fn main() {
    println!("=== Raft Transport Benchmark ===\n");

    // Create test messages
    let messages = create_test_messages();
    println!("Created {} test messages", messages.len());

    // Benchmark serialization
    benchmark_serialization(&messages);

    // Show message sizes
    show_message_sizes(&messages);
}

fn create_test_messages() -> Vec<Message> {
    let mut messages = Vec::new();

    // Election messages (high priority)
    for i in 0..10 {
        let mut msg = Message::default();
        msg.set_msg_type(MessageType::MsgRequestVote);
        msg.set_from(1);
        msg.set_to(2);
        msg.set_term(i as u64);
        messages.push(msg);
    }

    // Heartbeats (medium priority)
    for i in 0..50 {
        let mut msg = Message::default();
        msg.set_msg_type(MessageType::MsgHeartbeat);
        msg.set_from(1);
        msg.set_to(2);
        msg.set_term(10);
        msg.set_commit(i as u64);
        messages.push(msg);
    }

    // Log entries (normal priority)
    for i in 0..20 {
        let mut msg = Message::default();
        msg.set_msg_type(MessageType::MsgAppend);
        msg.set_from(1);
        msg.set_to(2);
        msg.set_term(10);
        msg.set_index(100 + i as u64);

        // Add some entries
        let mut entries = vec![];
        for j in 0..5 {
            let mut entry = Entry::default();
            entry.index = 100 + i as u64 + j;
            entry.term = 10;
            entry.data = vec![0u8; 1024]; // 1KB per entry
            entries.push(entry);
        }
        msg.set_entries(entries.into());

        messages.push(msg);
    }

    // One large snapshot
    let mut snapshot_msg = Message::default();
    snapshot_msg.set_msg_type(MessageType::MsgSnapshot);
    snapshot_msg.set_from(1);
    snapshot_msg.set_to(2);
    snapshot_msg.set_term(10);

    let mut snapshot = Snapshot::default();
    snapshot.set_data(vec![0u8; 1024 * 1024]); // 1MB snapshot
    snapshot_msg.set_snapshot(snapshot);
    messages.push(snapshot_msg);

    messages
}

fn benchmark_serialization(messages: &[Message]) {
    use blixard_core::raft_codec::{deserialize_message, serialize_message};

    println!("\n--- Serialization Benchmark ---");

    let start = Instant::now();
    let mut total_bytes = 0;

    for msg in messages {
        match serialize_message(msg) {
            Ok(bytes) => total_bytes += bytes.len(),
            Err(e) => eprintln!("Serialization error: {}", e),
        }
    }

    let elapsed = start.elapsed();
    println!("Serialized {} messages in {:?}", messages.len(), elapsed);
    println!(
        "Total bytes: {} ({:.2} MB)",
        total_bytes,
        total_bytes as f64 / 1024.0 / 1024.0
    );
    println!(
        "Average message size: {} bytes",
        total_bytes / messages.len()
    );
    println!(
        "Throughput: {:.2} MB/s",
        (total_bytes as f64 / 1024.0 / 1024.0) / elapsed.as_secs_f64()
    );
}

fn show_message_sizes(messages: &[Message]) {
    use blixard_core::raft_codec::serialize_message;
    use std::collections::HashMap;

    println!("\n--- Message Size Analysis ---");

    let mut sizes_by_type: HashMap<MessageType, Vec<usize>> = HashMap::new();

    for msg in messages {
        if let Ok(bytes) = serialize_message(msg) {
            sizes_by_type
                .entry(msg.msg_type())
                .or_default()
                .push(bytes.len());
        }
    }

    for (msg_type, sizes) in sizes_by_type {
        let total: usize = sizes.iter().sum();
        let avg = total / sizes.len();
        let min = sizes.iter().min().unwrap_or(&0);
        let max = sizes.iter().max().unwrap_or(&0);

        println!("{:?}:", msg_type);
        println!("  Count: {}", sizes.len());
        println!("  Average size: {} bytes", avg);
        println!("  Min/Max: {}/{} bytes", min, max);
    }
}
