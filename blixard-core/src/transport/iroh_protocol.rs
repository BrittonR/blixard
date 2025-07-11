//! Custom Iroh RPC protocol implementation
//!
//! This module implements a simple, efficient RPC protocol over Iroh's QUIC streams.
//! Design goals:
//! - Simple and debuggable
//! - Efficient for both small and large messages  
//! - Support for request/response and streaming
//! - Compatible with our existing service definitions

use crate::error::{BlixardError, BlixardResult};
use bytes::Bytes;
use iroh::endpoint::{RecvStream, SendStream};
use serde::{Deserialize, Serialize};
use tracing::{debug, error};

/// Protocol version for compatibility checking
pub const PROTOCOL_VERSION: u8 = 1;

/// Maximum message size (10MB)
pub const MAX_MESSAGE_SIZE: u32 = 10 * 1024 * 1024;

/// Message types in our protocol
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum MessageType {
    /// Request message expecting a response
    Request = 1,
    /// Response to a request
    Response = 2,
    /// Error response
    Error = 3,
    /// Stream data chunk
    StreamData = 4,
    /// End of stream marker
    StreamEnd = 5,
    /// Keep-alive ping
    Ping = 6,
    /// Keep-alive pong
    Pong = 7,
}

impl TryFrom<u8> for MessageType {
    type Error = BlixardError;

    fn try_from(value: u8) -> Result<Self, BlixardError> {
        match value {
            1 => Ok(MessageType::Request),
            2 => Ok(MessageType::Response),
            3 => Ok(MessageType::Error),
            4 => Ok(MessageType::StreamData),
            5 => Ok(MessageType::StreamEnd),
            6 => Ok(MessageType::Ping),
            7 => Ok(MessageType::Pong),
            _ => Err(BlixardError::Internal {
                message: format!("Invalid message type: {}", value),
            }),
        }
    }
}

/// Message header for framing
#[derive(Debug, Clone)]
pub struct MessageHeader {
    /// Protocol version
    pub version: u8,
    /// Type of message
    pub msg_type: MessageType,
    /// Length of payload (excluding header)
    pub payload_len: u32,
    /// Request ID for correlation (16 bytes)
    pub request_id: [u8; 16],
}

impl MessageHeader {
    /// Size of header in bytes
    pub const SIZE: usize = 24; // 1 + 1 + 4 + 16 + 2 padding

    /// Create a new message header
    pub fn new(msg_type: MessageType, payload_len: u32, request_id: [u8; 16]) -> Self {
        Self {
            version: PROTOCOL_VERSION,
            msg_type,
            payload_len,
            request_id,
        }
    }

    /// Serialize header to bytes
    pub fn to_bytes(&self) -> [u8; Self::SIZE] {
        let mut bytes = [0u8; Self::SIZE];
        bytes[0] = self.version;
        bytes[1] = self.msg_type as u8;
        bytes[2..6].copy_from_slice(&self.payload_len.to_be_bytes());
        bytes[6..22].copy_from_slice(&self.request_id);
        // bytes[22..24] are padding
        bytes
    }

    /// Deserialize header from bytes
    pub fn from_bytes(bytes: &[u8]) -> BlixardResult<Self> {
        if bytes.len() < Self::SIZE {
            return Err(BlixardError::Internal {
                message: format!("Header too short: {} bytes", bytes.len()),
            });
        }

        let version = bytes[0];
        if version != PROTOCOL_VERSION {
            return Err(BlixardError::Internal {
                message: format!("Unsupported protocol version: {}", version),
            });
        }

        let msg_type = MessageType::try_from(bytes[1])?;
        let payload_len = u32::from_be_bytes([bytes[2], bytes[3], bytes[4], bytes[5]]);

        if payload_len > MAX_MESSAGE_SIZE {
            return Err(BlixardError::Internal {
                message: format!("Message too large: {} bytes", payload_len),
            });
        }

        let mut request_id = [0u8; 16];
        request_id.copy_from_slice(&bytes[6..22]);

        Ok(Self {
            version,
            msg_type,
            payload_len,
            request_id,
        })
    }
}

/// RPC request message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcRequest {
    /// Service name (e.g., "health", "cluster")
    pub service: String,
    /// Method name (e.g., "check", "get_status")
    pub method: String,
    /// Serialized request payload
    pub payload: Bytes,
}

/// RPC response message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcResponse {
    /// Success flag
    pub success: bool,
    /// Serialized response payload (if success)
    pub payload: Option<Bytes>,
    /// Error message (if not success)
    pub error: Option<String>,
}

/// Write a message to a stream
pub async fn write_message(
    stream: &mut SendStream,
    msg_type: MessageType,
    request_id: [u8; 16],
    payload: &[u8],
) -> BlixardResult<()> {
    if payload.len() > MAX_MESSAGE_SIZE as usize {
        return Err(BlixardError::Internal {
            message: format!("Payload too large: {} bytes", payload.len()),
        });
    }

    let header = MessageHeader::new(msg_type, payload.len() as u32, request_id);

    debug!(
        "Writing message header: type={:?}, payload_len={}",
        msg_type,
        payload.len()
    );

    // Write header
    stream.write_all(&header.to_bytes()).await.map_err(|e| {
        error!("Failed to write header to stream: {}", e);
        BlixardError::Internal {
            message: format!("Failed to write to stream: {}", e),
        }
    })?;

    debug!("Header written successfully, writing payload");

    // Write payload
    stream.write_all(payload).await.map_err(|e| {
        error!("Failed to write payload to stream: {}", e);
        BlixardError::Internal {
            message: format!("Failed to write to stream: {}", e),
        }
    })?;

    debug!("Payload written successfully");

    Ok(())
}

/// Read a message from a stream
pub async fn read_message(stream: &mut RecvStream) -> BlixardResult<(MessageHeader, Bytes)> {
    // Read header
    let mut header_bytes = [0u8; MessageHeader::SIZE];
    stream
        .read_exact(&mut header_bytes)
        .await
        .map_err(|e| BlixardError::Internal {
            message: format!("Failed to read from stream: {}", e),
        })?;

    let header = MessageHeader::from_bytes(&header_bytes)?;

    // Read payload
    let mut payload = vec![0u8; header.payload_len as usize];
    stream
        .read_exact(&mut payload)
        .await
        .map_err(|e| BlixardError::Internal {
            message: format!("Failed to read from stream: {}", e),
        })?;

    Ok((header, Bytes::from(payload)))
}

/// Generate a new request ID
pub fn generate_request_id() -> [u8; 16] {
    let uuid = uuid::Uuid::new_v4();
    *uuid.as_bytes()
}

/// Helper to serialize a value to bytes
pub fn serialize_payload<T: Serialize>(value: &T) -> BlixardResult<Bytes> {
    bincode::serialize(value)
        .map(Bytes::from)
        .map_err(|e| BlixardError::SerializationError(e))
}

/// Helper to deserialize bytes to a value
pub fn deserialize_payload<T: for<'de> Deserialize<'de>>(bytes: &[u8]) -> BlixardResult<T> {
    bincode::deserialize(bytes).map_err(|e| BlixardError::SerializationError(e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_header_roundtrip() {
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
        assert!(MessageType::try_from(99).is_err());
    }

    #[test]
    fn test_payload_serialization() {
        #[derive(Serialize, Deserialize, PartialEq, Debug)]
        struct TestData {
            id: u64,
            name: String,
        }

        let data = TestData {
            id: 42,
            name: "test".to_string(),
        };

        let bytes = serialize_payload(&data).unwrap();
        let decoded: TestData = deserialize_payload(&bytes).unwrap();

        assert_eq!(decoded, data);
    }
}
