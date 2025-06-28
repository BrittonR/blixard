//! Tests for Iroh protocol implementation

use crate::{
    error::BlixardResult,
    transport::iroh_protocol::*,
};
use bytes::Bytes;

#[test]
fn test_message_header_serialization() {
    let request_id = generate_request_id();
    let header = MessageHeader::new(MessageType::Request, 1234, request_id);
    
    let bytes = header.to_bytes();
    let decoded = MessageHeader::from_bytes(&bytes).unwrap();
    
    assert_eq!(decoded.version, PROTOCOL_VERSION);
    assert_eq!(decoded.msg_type, MessageType::Request);
    assert_eq!(decoded.payload_len, 1234);
    assert_eq!(decoded.request_id, request_id);
}

#[test]
fn test_message_type_conversion() {
    assert_eq!(MessageType::try_from(1).unwrap(), MessageType::Request);
    assert_eq!(MessageType::try_from(2).unwrap(), MessageType::Response);
    assert_eq!(MessageType::try_from(3).unwrap(), MessageType::Error);
    assert_eq!(MessageType::try_from(4).unwrap(), MessageType::StreamData);
    assert_eq!(MessageType::try_from(5).unwrap(), MessageType::StreamEnd);
    assert_eq!(MessageType::try_from(6).unwrap(), MessageType::Ping);
    assert_eq!(MessageType::try_from(7).unwrap(), MessageType::Pong);
    assert!(MessageType::try_from(99).is_err());
}

#[test]
fn test_rpc_request_serialization() -> BlixardResult<()> {
    let request = RpcRequest {
        service: "health".to_string(),
        method: "check".to_string(),
        payload: Bytes::from("test payload"),
    };
    
    let bytes = serialize_payload(&request)?;
    let decoded: RpcRequest = deserialize_payload(&bytes)?;
    
    assert_eq!(decoded.service, "health");
    assert_eq!(decoded.method, "check");
    assert_eq!(&decoded.payload[..], b"test payload");
    
    Ok(())
}

#[test]
fn test_rpc_response_serialization() -> BlixardResult<()> {
    // Success response
    let response = RpcResponse {
        success: true,
        payload: Some(Bytes::from("response data")),
        error: None,
    };
    
    let bytes = serialize_payload(&response)?;
    let decoded: RpcResponse = deserialize_payload(&bytes)?;
    
    assert!(decoded.success);
    assert_eq!(&decoded.payload.unwrap()[..], b"response data");
    assert!(decoded.error.is_none());
    
    // Error response
    let error_response = RpcResponse {
        success: false,
        payload: None,
        error: Some("Something went wrong".to_string()),
    };
    
    let bytes = serialize_payload(&error_response)?;
    let decoded: RpcResponse = deserialize_payload(&bytes)?;
    
    assert!(!decoded.success);
    assert!(decoded.payload.is_none());
    assert_eq!(decoded.error.unwrap(), "Something went wrong");
    
    Ok(())
}

#[test]
fn test_header_size_validation() {
    // Test that header rejects oversized messages
    let request_id = generate_request_id();
    let header = MessageHeader::new(MessageType::Request, MAX_MESSAGE_SIZE + 1, request_id);
    let bytes = header.to_bytes();
    
    let result = MessageHeader::from_bytes(&bytes);
    assert!(result.is_err());
}

#[test]
fn test_protocol_version_check() {
    let mut bytes = [0u8; MessageHeader::SIZE];
    bytes[0] = 99; // Invalid version
    bytes[1] = MessageType::Request as u8;
    bytes[2..6].copy_from_slice(&1234u32.to_be_bytes());
    
    let result = MessageHeader::from_bytes(&bytes);
    assert!(result.is_err());
}