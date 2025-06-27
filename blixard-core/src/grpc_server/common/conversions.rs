//! Type conversions between internal types and protobuf types
//!
//! This module re-exports common conversions to maintain backward compatibility
//! while the codebase migrates to use the centralized conversions.

pub use crate::common::conversions::{
    error_to_status,
    vm_status_to_proto,
    ToProto,
    FromProto,
    ToStatus,
};

// Re-export for backward compatibility
pub use crate::types::VmStatus as InternalVmStatus;
pub use crate::proto::VmState;