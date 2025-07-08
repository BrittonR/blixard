//! Simple test for Raft message serialization

use blixard_core::raft_codec::{deserialize_message, serialize_message};
use raft::prelude::*;

#[test]
fn test_raft_message_serialization() {
    // Create a simple heartbeat message
    let mut msg = Message::default();
    msg.set_msg_type(MessageType::MsgHeartbeat);
    msg.from = 1;
    msg.to = 2;
    msg.term = 5;

    // Serialize
    let serialized = serialize_message(&msg).unwrap();
    println!("Serialized heartbeat: {} bytes", serialized.len());

    // Deserialize
    let deserialized = deserialize_message(&serialized).unwrap();

    // Verify
    assert_eq!(msg.msg_type(), deserialized.msg_type());
    assert_eq!(msg.from, deserialized.from);
    assert_eq!(msg.to, deserialized.to);
    assert_eq!(msg.term, deserialized.term);

    println!("✅ Raft message serialization works!");
}
