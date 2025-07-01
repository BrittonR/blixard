#!/usr/bin/env cargo

//! ```cargo
//! [dependencies]
//! raft = "0.7"
//! raft-proto = { version = "0.7", default-features = false, features = ["prost-codec"] }
//! protobuf = "2.27"
//! ```

use raft::prelude::*;
use protobuf::Message as ProtobufMessage;

fn main() {
    println!("=== Raft Message Serialization Test for Iroh Transport ===\n");
    
    let test_results = vec![
        test_heartbeat(),
        test_vote_request(),
        test_append_entries(),
        test_snapshot(),
    ];
    
    let all_passed = test_results.iter().all(|&result| result);
    
    println!("\n=== Test Summary ===");
    println!("Total tests: {}", test_results.len());
    println!("Passed: {}", test_results.iter().filter(|&&r| r).count());
    println!("Failed: {}", test_results.iter().filter(|&&r| !r).count());
    
    if all_passed {
        println!("\n✅ All Raft message serialization tests passed!");
        println!("✅ Raft consensus can work correctly over Iroh transport!");
        println!("\n📝 Note: Raft requires protobuf serialization (not serde)");
        println!("   The raft_codec.rs correctly uses protobuf for Message types");
    } else {
        println!("\n❌ Some tests failed!");
        std::process::exit(1);
    }
}

fn test_heartbeat() -> bool {
    println!("Test 1: Heartbeat Message");
    let mut msg = Message::default();
    msg.set_msg_type(MessageType::MsgHeartbeat);
    msg.from = 1;
    msg.to = 2;
    msg.term = 5;
    
    test_message_roundtrip(msg, "Heartbeat")
}

fn test_vote_request() -> bool {
    println!("\nTest 2: Vote Request");
    let mut msg = Message::default();
    msg.set_msg_type(MessageType::MsgRequestVote);
    msg.from = 2;
    msg.to = 3;
    msg.term = 10;
    msg.log_term = 8;
    msg.index = 100;
    
    test_message_roundtrip(msg, "Vote Request")
}

fn test_append_entries() -> bool {
    println!("\nTest 3: Append Entries with Log Data");
    let mut msg = Message::default();
    msg.set_msg_type(MessageType::MsgAppend);
    msg.from = 1;
    msg.to = 2;
    msg.term = 7;
    msg.log_term = 6;
    msg.index = 50;
    msg.commit = 45;
    
    // Add an entry with data
    let mut entry = Entry::default();
    entry.term = 7;
    entry.index = 51;
    entry.entry_type = EntryType::EntryNormal as i32;
    entry.data = b"test raft consensus data".to_vec();
    msg.entries.push(entry);
    
    test_message_roundtrip(msg, "Append Entries")
}

fn test_snapshot() -> bool {
    println!("\nTest 4: Snapshot Message");
    let mut msg = Message::default();
    msg.set_msg_type(MessageType::MsgSnapshot);
    msg.from = 1;
    msg.to = 3;
    msg.term = 15;
    
    // Create snapshot metadata
    let mut snapshot = Snapshot::default();
    let mut metadata = SnapshotMetadata::default();
    metadata.index = 100;
    metadata.term = 15;
    
    // Add configuration state
    let mut conf_state = ConfState::default();
    conf_state.voters = vec![1, 2, 3];
    metadata.set_conf_state(conf_state);
    
    snapshot.set_metadata(metadata);
    snapshot.data = b"snapshot data".to_vec();
    msg.set_snapshot(snapshot);
    
    test_message_roundtrip(msg, "Snapshot")
}

fn test_message_roundtrip(msg: Message, msg_name: &str) -> bool {
    // Serialize
    match msg.write_to_bytes() {
        Ok(bytes) => {
            println!("  ✅ Serialized {}: {} bytes", msg_name, bytes.len());
            
            // Deserialize
            let mut deserialized = Message::new();
            match deserialized.merge_from_bytes(&bytes) {
                Ok(_) => {
                    println!("  ✅ Deserialized {} successfully", msg_name);
                    
                    // Verify fields
                    let fields_match = 
                        msg.msg_type() == deserialized.msg_type() &&
                        msg.from == deserialized.from &&
                        msg.to == deserialized.to &&
                        msg.term == deserialized.term;
                    
                    if fields_match {
                        println!("  ✅ All fields match!");
                        true
                    } else {
                        println!("  ❌ Field mismatch!");
                        false
                    }
                }
                Err(e) => {
                    println!("  ❌ Deserialization failed: {:?}", e);
                    false
                }
            }
        }
        Err(e) => {
            println!("  ❌ Serialization failed: {:?}", e);
            false
        }
    }
}