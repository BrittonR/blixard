//! Common utilities for gRPC services

pub mod middleware;
pub mod conversions;
pub mod helpers;

pub use middleware::GrpcMiddleware;
pub use conversions::{vm_status_to_proto, error_to_status};
pub use helpers::{extract_tenant_id, instrument_grpc_method, record_grpc_error};