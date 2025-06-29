//! Example usage of consolidated patterns
//!
//! This module demonstrates how to use the common utilities
//! to reduce code duplication and improve consistency.

#![cfg(test)]

use crate::{
    common::{
        error_context::{ErrorContext, ResultContext, error_for},
        conversions::{ToProto, ToStatus},
        metrics::{record_metric, GlobalMetricsRecorder, ScopedTimer},
        rate_limiting::{RateLimitBuilder, RateLimitingMiddleware},
    },
    error::{BlixardError, BlixardResult},
    types::{VmConfig, VmStatus},
    resource_quotas::ApiOperation,
};
use std::sync::Arc;

/// Example service using consolidated patterns
struct ExampleVmService {
    rate_limiter: Arc<dyn crate::common::rate_limiting::RateLimiter>,
    metrics_recorder: GlobalMetricsRecorder,
}

impl ExampleVmService {
    /// Example: Create VM with all patterns applied
    async fn create_vm(&self, tenant_id: &str, config: VmConfig) -> BlixardResult<String> {
        // 1. Rate limiting
        self.rate_limiter
            .check_and_record(tenant_id, &ApiOperation::VmCreate)
            .await
            .context("Rate limit check failed")?;
            
        // 2. Scoped metrics timer
        let _timer = ScopedTimer::new("create_vm").with_node_id(42);
        
        // 3. Error context building
        let vm_id = self.actually_create_vm(&config)
            .await
            .with_details("create_vm", || {
                format!("tenant={}, vm_name={}", tenant_id, config.name)
            })?;
            
        // 4. Record additional metrics
        record_metric(&self.metrics_recorder)
            .label("tenant_id", tenant_id)
            .label("vm_name", &config.name)
            .count("vm_create_success", 1);
            
        Ok(vm_id)
    }
    
    /// Example: Get VM status with error handling
    async fn get_vm_status(&self, vm_id: &str) -> BlixardResult<VmStatus> {
        // Using error builder for structured errors
        self.fetch_vm_from_db(vm_id)
            .await
            .map_err(|_| {
                error_for("get_vm_status")
                    .detail("vm_id", vm_id)
                    .detail("reason", "database error")
                    .not_found("VM")
            })?
            .ok_or_else(|| {
                error_for("get_vm_status")
                    .detail("vm_id", vm_id)
                    .not_found("VM")
            })
    }
    
    // Mock implementations
    async fn actually_create_vm(&self, _config: &VmConfig) -> BlixardResult<String> {
        Ok("vm-123".to_string())
    }
    
    async fn fetch_vm_from_db(&self, _vm_id: &str) -> BlixardResult<Option<VmStatus>> {
        Ok(Some(VmStatus::Running))
    }
}

/// Example: gRPC handler using patterns
async fn grpc_handler_example(
    service: &ExampleVmService,
    request: tonic::Request<crate::proto::CreateVmRequest>,
) -> Result<tonic::Response<crate::proto::CreateVmResponse>, tonic::Status> {
    let req = request.into_inner();
    
    // Extract tenant ID (would normally use middleware)
    let tenant_id = "example-tenant";
    
    // Create VM config
    let config = VmConfig {
        name: req.name.clone(),
        vcpus: req.vcpus,
        memory: req.memory_mb,
        config_path: format!("/vms/{}.yaml", req.name),
        ip_address: None,
        tenant_id: tenant_id.to_string(),
    };
    
    // Call service method and convert errors to Status
    let vm_id = service.create_vm(tenant_id, config)
        .await
        .log_error("Failed to create VM")  // Log on error
        .map_err(crate::common::conversions::error_to_status)?;  // Convert to gRPC Status
        
    // Convert to proto response
    let response = crate::proto::CreateVmResponse {
        success: true,
        message: format!("VM {} created successfully", vm_id),
        vm_id: vm_id.clone(),
    };
    
    Ok(tonic::Response::new(response))
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_consolidated_patterns() {
        // Set up rate limiter
        let rate_limiter = Arc::new(
            RateLimitBuilder::new()
                .vm_limits(10)  // 10 VMs per minute
                .build_in_memory()
        );
        
        // Create service
        let service = ExampleVmService {
            rate_limiter,
            metrics_recorder: GlobalMetricsRecorder,
        };
        
        // Test VM creation
        let config = VmConfig {
            name: "test-vm".to_string(),
            vcpus: 2,
            memory: 1024,
            config_path: "/test.yaml".to_string(),
            ip_address: None,
            tenant_id: "test-tenant".to_string(),
            metadata: None,
        };
        
        let result = service.create_vm("test-tenant", config).await;
        assert!(result.is_ok());
        
        // Test error context
        let status_result = service.get_vm_status("non-existent").await;
        assert!(status_result.is_ok()); // Our mock returns Ok
    }
    
    #[test]
    fn test_error_context_chaining() {
        let base_error: Result<(), std::io::Error> = Err(
            std::io::Error::new(std::io::ErrorKind::NotFound, "file not found")
        );
        
        let with_context = base_error
            .context("Failed to read config")
            .with_details("load_config", || "path=/etc/config.yaml".to_string());
            
        assert!(with_context.is_err());
        let err = with_context.unwrap_err();
        println!("Error with context: {}", err);
    }
}