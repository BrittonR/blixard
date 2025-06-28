//! Standalone test for Iroh RPC

use blixard_core::transport::iroh_protocol::*;
use std::io::Cursor;

fn main() {
    println!("Testing Iroh RPC protocol...");
    
    // Test message header
    let request_id = generate_request_id();
    let header = MessageHeader::new(MessageType::Request, 100, request_id);
    println!("Created header: {:?}", header);
    
    // Test serialization
    let bytes = header.to_bytes();
    println!("Serialized to {} bytes", bytes.len());
    
    // Test deserialization
    let decoded = MessageHeader::from_bytes(&bytes).unwrap();
    println!("Decoded header: {:?}", decoded);
    
    assert_eq!(header.version, decoded.version);
    assert_eq!(header.msg_type, decoded.msg_type);
    assert_eq!(header.payload_len, decoded.payload_len);
    assert_eq!(header.request_id, decoded.request_id);
    
    println!("Protocol test passed!");
}