//! Type conversion traits and implementations
//!
//! This module provides a unified approach to converting between
//! internal types and protocol buffer types.

use crate::{
    error::{BlixardError, BlixardResult},
    types::{VmConfig, VmStatus, NodeState as InternalNodeState},
    proto::{VmInfo, VmState, NodeState as ProtoNodeState, NodeInfo},
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
            memory: config.memory,
            node_id: 0, // Should be set by caller
            ip_address: config.ip_address.clone().unwrap_or_default(),
        }
    }
}

// Node State conversions

impl ToProto<ProtoNodeState> for InternalNodeState {
    fn to_proto(&self) -> ProtoNodeState {
        match self {
            InternalNodeState::Uninitialized => ProtoNodeState::Unknown,
            InternalNodeState::Initialized => ProtoNodeState::Initialized,
            InternalNodeState::JoiningCluster => ProtoNodeState::JoiningCluster,
            InternalNodeState::Active => ProtoNodeState::Active,
            InternalNodeState::LeavingCluster => ProtoNodeState::LeavingCluster,
            InternalNodeState::Error => ProtoNodeState::Error,
        }
    }
}

impl FromProto<ProtoNodeState> for InternalNodeState {
    fn from_proto(proto: ProtoNodeState) -> BlixardResult<Self> {
        Ok(match proto {
            ProtoNodeState::Unknown => InternalNodeState::Uninitialized,
            ProtoNodeState::Initialized => InternalNodeState::Initialized,
            ProtoNodeState::JoiningCluster => InternalNodeState::JoiningCluster,
            ProtoNodeState::Active => InternalNodeState::Active,
            ProtoNodeState::LeavingCluster => InternalNodeState::LeavingCluster,
            ProtoNodeState::Error => InternalNodeState::Error,
        })
    }
}

// Helper function for backward compatibility
pub fn vm_status_to_proto(status: &VmStatus) -> VmState {
    status.to_proto()
}

// Error to Status conversion (moved from grpc_server/common/conversions.rs)

use tonic::Status;

/// Convert BlixardError to gRPC Status
pub fn error_to_status(err: BlixardError) -> Status {
    match err {
        BlixardError::NotImplemented { feature } => {
            Status::unimplemented(format!("Feature not implemented: {}", feature))
        }
        BlixardError::ServiceNotFound(name) => {
            Status::not_found(format!("Service not found: {}", name))
        }
        BlixardError::ServiceAlreadyExists(name) => {
            Status::already_exists(format!("Service already exists: {}", name))
        }
        BlixardError::ConfigError(msg) => {
            Status::invalid_argument(format!("Configuration error: {}", msg))
        }
        BlixardError::Storage { operation, .. } => {
            Status::internal(format!("Storage error during {}", operation))
        }
        BlixardError::Serialization { operation, .. } => {
            Status::internal(format!("Serialization error during {}", operation))
        }
        BlixardError::Raft { operation, .. } => {
            Status::internal(format!("Raft error during {}", operation))
        }
        BlixardError::NodeError(msg) => {
            Status::internal(format!("Node error: {}", msg))
        }
        BlixardError::ClusterError(msg) => {
            Status::internal(format!("Cluster error: {}", msg))
        }
        BlixardError::ClusterJoin { reason } => {
            Status::failed_precondition(format!("Failed to join cluster: {}", reason))
        }
        BlixardError::VmOperationFailed { operation, details } => {
            Status::internal(format!("VM operation '{}' failed: {}", operation, details))
        }
        BlixardError::SchedulingError { message } => {
            Status::resource_exhausted(format!("Scheduling error: {}", message))
        }
        BlixardError::Security { message } => {
            Status::permission_denied(format!("Security error: {}", message))
        }
        BlixardError::NotFound { resource } => {
            Status::not_found(format!("Resource not found: {}", resource))
        }
        BlixardError::Internal { message } => {
            Status::internal(format!("Internal error: {}", message))
        }
        BlixardError::ServiceManagementError(msg) => {
            Status::internal(format!("Service management error: {}", msg))
        }
        BlixardError::IoError(e) => {
            Status::internal(format!("I/O error: {}", e))
        }
        BlixardError::NetworkError(e) => {
            Status::unavailable(format!("Network error: {}", e))
        }
        BlixardError::SystemError(msg) => {
            Status::internal(format!("System error: {}", msg))
        }
        _ => Status::internal("Internal error"),
    }
}

/// Extension trait for Result types to convert to Status
pub trait ToStatus<T> {
    /// Convert Result to Status
    fn to_status(self) -> Result<T, Status>;
}

impl<T> ToStatus<T> for BlixardResult<T> {
    fn to_status(self) -> Result<T, Status> {
        self.map_err(error_to_status)
    }
}

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