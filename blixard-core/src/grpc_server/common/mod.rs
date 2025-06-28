//! Common utilities for gRPC services

pub mod middleware;
pub mod helpers;

pub use middleware::{GrpcMiddleware, extract_tenant_id};
pub use helpers::{instrument_grpc_method, record_grpc_error};