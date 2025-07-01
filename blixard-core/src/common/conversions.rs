//! Type conversion traits and implementations
//!
//! This module provides a unified approach to converting between
//! internal types and protocol buffer types.

use crate::{
    error::{BlixardError, BlixardResult},
    types::{VmConfig, VmStatus},
    iroh_types::{VmInfo, VmState},
};

/// Trait for converting to protocol buffer types
pub trait ToProto<T> {
    /// Convert to protocol buffer type
    fn to_proto(&self) -> T;
}

/// Trait for converting from protocol buffer types
pub trait FromProto<T> {
    /// Convert from protocol buffer type
    fn from_proto(proto: T) -> BlixardResult<Self>
    where
        Self: Sized;
}

// VM Status conversions

impl ToProto<VmState> for VmStatus {
    fn to_proto(&self) -> VmState {
        match self {
            VmStatus::Creating => VmState::Created,
            VmStatus::Starting => VmState::Starting,
            VmStatus::Running => VmState::Running,
            VmStatus::Stopping => VmState::Stopping,
            VmStatus::Stopped => VmState::Stopped,
            VmStatus::Failed => VmState::Failed,
        }
    }
}

impl FromProto<VmState> for VmStatus {
    fn from_proto(proto: VmState) -> BlixardResult<Self> {
        Ok(match proto {
            VmState::Unknown => VmStatus::Stopped, // Default for unknown
            VmState::Created => VmStatus::Creating,
            VmState::Starting => VmStatus::Starting,
            VmState::Running => VmStatus::Running,
            VmState::Stopping => VmStatus::Stopping,
            VmState::Stopped => VmStatus::Stopped,
            VmState::Failed => VmStatus::Failed,
        })
    }
}

// VM Info conversions

impl ToProto<VmInfo> for (VmConfig, VmStatus) {
    fn to_proto(&self) -> VmInfo {
        let (config, status) = self;
        VmInfo {
            name: config.name.clone(),
            state: status.to_proto() as i32,
            vcpus: config.vcpus,
            memory_mb: config.memory,
            node_id: 0, // Should be set by caller
            ip_address: config.ip_address.clone().unwrap_or_default(),
        }
    }
}

// Helper function for backward compatibility
pub fn vm_status_to_proto(status: &VmStatus) -> VmState {
    status.to_proto()
}

// Removed gRPC error conversion - now using Iroh-only transport

// Batch conversion helpers

/// Convert a vector of items to proto
pub fn vec_to_proto<T, P>(items: Vec<T>) -> Vec<P>
where
    T: ToProto<P>,
{
    items.iter().map(|item| item.to_proto()).collect()
}

/// Convert a vector of proto items to internal types
pub fn vec_from_proto<T, P>(protos: Vec<P>) -> BlixardResult<Vec<T>>
where
    T: FromProto<P>,
{
    protos.into_iter()
        .map(T::from_proto)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_vm_status_conversion() {
        // Test to_proto
        assert_eq!(VmStatus::Running.to_proto(), VmState::Running);
        assert_eq!(VmStatus::Stopped.to_proto(), VmState::Stopped);
        
        // Test from_proto
        assert_eq!(
            VmStatus::from_proto(VmState::Running).unwrap(),
            VmStatus::Running
        );
        assert_eq!(
            VmStatus::from_proto(VmState::Unknown).unwrap(),
            VmStatus::Stopped // Default
        );
    }
    
    #[test]
    fn test_error_to_status() {
        let err = BlixardError::NotFound {
            resource: "test-vm".to_string(),
        };
        let status = error_to_status(err);
        assert_eq!(status.code(), tonic::Code::NotFound);
        assert!(status.message().contains("test-vm"));
    }
}