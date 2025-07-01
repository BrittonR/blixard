//! Modularized gRPC server implementation
//!
//! This module organizes the gRPC services into focused submodules
//! for better maintainability and testability.

pub mod services;

use crate::{
    node_shared::SharedNodeState,
    grpc_security::GrpcSecurityMiddleware,
};
use std::sync::Arc;

/// Builder for creating configured gRPC services
pub struct GrpcServiceBuilder {
    node: Arc<SharedNodeState>,
    security_middleware: Option<GrpcSecurityMiddleware>,
}

impl GrpcServiceBuilder {
    /// Create a new service builder
    pub fn new(node: Arc<SharedNodeState>) -> Self {
        Self {
            node,
            security_middleware: None,
        }
    }
    
    /// Add security middleware
    pub async fn with_security(mut self) -> Self {
        if let Some(security_manager) = self.node.get_security_manager().await {
            self.security_middleware = Some(GrpcSecurityMiddleware::new(security_manager));
        }
        self
    }
    
    /// Build task service
    pub fn build_task_service(self) -> services::TaskServiceImpl {
        services::TaskServiceImpl::new(
            self.node.clone(),
            self.security_middleware.clone(),
        )
    }
    
    /// Build monitoring service
    pub fn build_monitoring_service(self) -> services::MonitoringServiceImpl {
        services::MonitoringServiceImpl::new(
            self.node.clone(),
            self.security_middleware.clone(),
        )
    }
}