//! Simple Raft message benchmark focusing on serialization performance

use blixard_core::error::BlixardResult;
use blixard_core::raft_codec::{deserialize_message, serialize_message};
use raft::prelude::*;
use std::time::Instant;

#[tokio::main]
async fn main() -> BlixardResult<()> {
    println!("=== Iroh Raft Message Benchmark ===\n");

    // Test different message types and sizes
    benchmark_message_type("Heartbeat", create_heartbeat())?;
    benchmark_message_type("Vote Request", create_vote_request())?;
    benchmark_message_type("Small LogAppend", create_log_append(1, 100))?;
    benchmark_message_type("Medium LogAppend", create_log_append(10, 1024))?;
    benchmark_message_type("Large LogAppend", create_log_append(100, 4096))?;
    benchmark_message_type("Snapshot", create_snapshot(1024 * 1024))?; // 1MB snapshot

    println!("\nBenchmark complete!");
    Ok(())
}

fn benchmark_message_type(name: &str, msg: Message) -> BlixardResult<()> {
    println!("Testing {}:", name);

    const ITERATIONS: usize = 10000;
    let mut serialize_times = Vec::with_capacity(ITERATIONS);
    let mut deserialize_times = Vec::with_capacity(ITERATIONS);
    let mut bytes_size = 0;

    // Warmup
    for _ in 0..100 {
        let bytes = serialize_message(&msg)?;
        let _ = deserialize_message(&bytes)?;
    }

    // Benchmark
    for _ in 0..ITERATIONS {
        let start = Instant::now();
        let bytes = serialize_message(&msg)?;
        let serialize_time = start.elapsed();
        bytes_size = bytes.len();

        let start = Instant::now();
        let _ = deserialize_message(&bytes)?;
        let deserialize_time = start.elapsed();

        serialize_times.push(serialize_time);
        deserialize_times.push(deserialize_time);
    }

    // Calculate statistics
    serialize_times.sort();
    deserialize_times.sort();

    let ser_avg = serialize_times.iter().sum::<std::time::Duration>() / ITERATIONS as u32;
    let deser_avg = deserialize_times.iter().sum::<std::time::Duration>() / ITERATIONS as u32;

    let ser_p50 = serialize_times[ITERATIONS / 2];
    let ser_p99 = serialize_times[ITERATIONS * 99 / 100];
    let deser_p50 = deserialize_times[ITERATIONS / 2];
    let deser_p99 = deserialize_times[ITERATIONS * 99 / 100];

    // Calculate throughput
    let total_bytes = bytes_size as f64 * ITERATIONS as f64;
    let total_time = serialize_times
        .iter()
        .sum::<std::time::Duration>()
        .as_secs_f64();
    let throughput_gbps = (total_bytes / total_time) / 1_000_000_000.0;

    println!("  Message size: {} bytes", bytes_size);
    println!(
        "  Serialize - Avg: {:?}, P50: {:?}, P99: {:?}",
        ser_avg, ser_p50, ser_p99
    );
    println!(
        "  Deserialize - Avg: {:?}, P50: {:?}, P99: {:?}",
        deser_avg, deser_p50, deser_p99
    );
    println!("  Throughput: {:.2} GB/s", throughput_gbps);
    println!();

    Ok(())
}

fn create_heartbeat() -> Message {
    let mut msg = Message::default();
    msg.set_msg_type(MessageType::MsgHeartbeat);
    msg.set_from(1);
    msg.set_to(2);
    msg.set_term(10);
    msg.set_commit(100);
    msg
}

fn create_vote_request() -> Message {
    let mut msg = Message::default();
    msg.set_msg_type(MessageType::MsgRequestVote);
    msg.set_from(1);
    msg.set_to(2);
    msg.set_term(10);
    msg.set_log_term(9);
    msg.set_index(100);
    msg
}

fn create_log_append(entry_count: usize, entry_size: usize) -> Message {
    let mut msg = Message::default();
    msg.set_msg_type(MessageType::MsgAppend);
    msg.set_from(1);
    msg.set_to(2);
    msg.set_term(10);
    msg.set_log_term(10);
    msg.set_index(100);
    msg.set_commit(95);

    let mut entries = Vec::new();
    for i in 0..entry_count {
        let mut entry = Entry::default();
        entry.index = 100 + i as u64;
        entry.term = 10;
        entry.data = vec![0u8; entry_size];
        entries.push(entry);
    }
    msg.set_entries(entries.into());

    msg
}

fn create_snapshot(size: usize) -> Message {
    let mut msg = Message::default();
    msg.set_msg_type(MessageType::MsgSnapshot);
    msg.set_from(1);
    msg.set_to(2);
    msg.set_term(10);

    let mut snapshot = Snapshot::default();
    let mut metadata = SnapshotMetadata::default();
    metadata.index = 1000;
    metadata.term = 10;

    let mut conf_state = ConfState::default();
    conf_state.voters = vec![1, 2, 3];
    metadata.set_conf_state(conf_state);

    snapshot.set_metadata(metadata);
    snapshot.data = vec![0u8; size];

    msg.set_snapshot(snapshot);
    msg
}
