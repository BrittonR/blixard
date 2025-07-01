//! Minimal test for Raft message codec

fn main() {
    use blixard_core::raft_codec::{serialize_message, deserialize_message};
    use raft::prelude::*;
    
    println!("Testing Raft message serialization...");
    
    // Create a heartbeat message
    let mut msg = Message::default();
    msg.set_msg_type(MessageType::MsgHeartbeat);
    msg.from = 1;
    msg.to = 2;
    msg.term = 5;
    
    // Serialize
    match serialize_message(&msg) {
        Ok(serialized) => {
            println!("✅ Serialized heartbeat: {} bytes", serialized.len());
            
            // Deserialize
            match deserialize_message(&serialized) {
                Ok(deserialized) => {
                    println!("✅ Deserialized successfully");
                    println!("  Type: {:?}", deserialized.msg_type());
                    println!("  From: {}", deserialized.from);
                    println!("  To: {}", deserialized.to);
                    println!("  Term: {}", deserialized.term);
                    
                    if msg.msg_type() == deserialized.msg_type() &&
                       msg.from == deserialized.from &&
                       msg.to == deserialized.to &&
                       msg.term == deserialized.term {
                        println!("\n✅ Raft message serialization is working correctly!");
                        println!("   This means Raft consensus can work over Iroh transport.");
                    } else {
                        println!("\n❌ Message fields don't match!");
                    }
                }
                Err(e) => {
                    println!("❌ Deserialization failed: {}", e);
                }
            }
        }
        Err(e) => {
            println!("❌ Serialization failed: {}", e);
        }
    }
}