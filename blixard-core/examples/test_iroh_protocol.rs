//! Test just the Iroh protocol layer

use blixard_core::transport::iroh_protocol::*;

fn main() {
    println!("Testing Iroh RPC protocol...\n");

    // Test message header
    println!("1. Testing Message Header");
    println!("-------------------------");
    let request_id = generate_request_id();
    let header = MessageHeader::new(MessageType::Request, 100, request_id);
    println!("Created header: {:?}", header);
    println!("  Version: {}", header.version);
    println!("  Type: {:?}", header.msg_type);
    println!("  Payload length: {}", header.payload_len);
    println!("  Request ID: {:02x?}", &header.request_id[..8]); // First 8 bytes

    // Test serialization
    println!("\n2. Testing Serialization");
    println!("------------------------");
    let bytes = header.to_bytes();
    println!("Serialized to {} bytes", bytes.len());
    assert_eq!(bytes.len(), 24, "Header should be exactly 24 bytes");

    // Test deserialization
    println!("\n3. Testing Deserialization");
    println!("--------------------------");
    let decoded = MessageHeader::from_bytes(&bytes).unwrap();
    println!("Decoded header: {:?}", decoded);
    assert_eq!(header.version, decoded.version);
    assert_eq!(header.msg_type, decoded.msg_type);
    assert_eq!(header.payload_len, decoded.payload_len);
    assert_eq!(header.request_id, decoded.request_id);
    println!("✅ Header round-trip successful!");

    // Test RPC request/response
    println!("\n4. Testing RPC Messages");
    println!("-----------------------");

    let request = RpcRequest {
        service: "test".to_string(),
        method: "ping".to_string(),
        payload: bytes::Bytes::from("hello"),
    };

    let request_bytes = serialize_payload(&request).unwrap();
    println!("RPC request serialized to {} bytes", request_bytes.len());

    let decoded_request: RpcRequest = deserialize_payload(&request_bytes).unwrap();
    assert_eq!(request.service, decoded_request.service);
    assert_eq!(request.method, decoded_request.method);
    assert_eq!(request.payload, decoded_request.payload);
    println!("✅ RPC request round-trip successful!");

    let response = RpcResponse {
        success: true,
        payload: Some(bytes::Bytes::from("world")),
        error: None,
    };

    let response_bytes = serialize_payload(&response).unwrap();
    println!("RPC response serialized to {} bytes", response_bytes.len());

    let decoded_response: RpcResponse = deserialize_payload(&response_bytes).unwrap();
    assert_eq!(response.success, decoded_response.success);
    assert_eq!(response.payload, decoded_response.payload);
    assert_eq!(response.error, decoded_response.error);
    println!("✅ RPC response round-trip successful!");

    // Test all message types
    println!("\n5. Testing All Message Types");
    println!("----------------------------");
    let msg_types = vec![
        MessageType::Request,
        MessageType::Response,
        MessageType::Error,
        MessageType::StreamData,
        MessageType::StreamEnd,
        MessageType::Ping,
        MessageType::Pong,
    ];

    for msg_type in msg_types {
        let header = MessageHeader::new(msg_type, 0, generate_request_id());
        let bytes = header.to_bytes();
        let decoded = MessageHeader::from_bytes(&bytes).unwrap();
        assert_eq!(header.msg_type, decoded.msg_type);
        println!("✅ {:?} round-trip successful!", msg_type);
    }

    println!("\n✨ All protocol tests passed!");
}
