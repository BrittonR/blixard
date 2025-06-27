//! Type conversions between internal types and protobuf types

use crate::{
    error::BlixardError,
    types::VmStatus as InternalVmStatus,
    proto::VmState,
};
use tonic::Status;

/// Convert internal VM status to proto VM state
pub fn vm_status_to_proto(status: &InternalVmStatus) -> VmState {
    match status {
        InternalVmStatus::Creating => VmState::Created,
        InternalVmStatus::Starting => VmState::Starting,
        InternalVmStatus::Running => VmState::Running,
        InternalVmStatus::Stopping => VmState::Stopping,
        InternalVmStatus::Stopped => VmState::Stopped,
        InternalVmStatus::Failed => VmState::Failed,
    }
}

/// Convert internal error to gRPC status
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
        _ => Status::internal("Internal error"),
    }
}