//! Common middleware for gRPC services
//!
//! This module provides unified authentication, authorization, and rate limiting
//! functionality for all gRPC service implementations.

use crate::{
    grpc_security::{GrpcSecurityMiddleware, SecurityContext},
    security::{Permission, SecurityManager},
    quota_manager::QuotaManager,
    resource_quotas::ApiOperation,
    authenticate_grpc, optional_authenticate_grpc,
};
use std::sync::Arc;
use std::collections::HashMap;
use tonic::{Request, Status};
use chrono::{Datelike, Timelike};

/// Unified middleware for gRPC services
#[derive(Clone)]
pub struct GrpcMiddleware {
    security: Option<GrpcSecurityMiddleware>,
    security_manager: Option<Arc<SecurityManager>>,
    quota_manager: Option<Arc<QuotaManager>>,
}

impl GrpcMiddleware {
    /// Check if Cedar authorization is available
    pub fn has_cedar(&self) -> bool {
        self.security_manager.is_some()
    }
    /// Create new middleware instance
    pub fn new(
        security: Option<GrpcSecurityMiddleware>,
        security_manager: Option<Arc<SecurityManager>>,
        quota_manager: Option<Arc<QuotaManager>>,
    ) -> Self {
        Self {
            security,
            security_manager,
            quota_manager,
        }
    }
    
    /// Authenticate request with required permission
    pub async fn authenticate<T>(
        &self,
        request: &Request<T>,
        permission: Permission,
    ) -> Result<SecurityContext, Status> {
        if let Some(ref middleware) = self.security {
            authenticate_grpc!(middleware, request, permission)
        } else {
            // Development mode - return admin context
            Ok(SecurityContext {
                authenticated: true,
                user: Some("system".to_string()),
                permissions: vec![Permission::Admin],
                auth_method: "none".to_string(),
            })
        }
    }
    
    /// Optional authentication (for public endpoints)
    pub async fn optional_authenticate<T>(
        &self,
        request: &Request<T>,
    ) -> Option<SecurityContext> {
        if let Some(ref middleware) = self.security {
            optional_authenticate_grpc!(middleware, request)
        } else {
            // Development mode - return admin context
            Some(SecurityContext {
                authenticated: true,
                user: Some("system".to_string()),
                permissions: vec![Permission::Admin],
                auth_method: "none".to_string(),
            })
        }
    }
    
    /// Combined authentication and rate limiting check
    pub async fn authenticate_and_rate_limit<T>(
        &self,
        request: &Request<T>,
        permission: Permission,
        operation: ApiOperation,
    ) -> Result<(SecurityContext, String), Status> {
        // First authenticate
        let security_context = self.authenticate(request, permission).await?;
        
        // Extract tenant ID
        let tenant_id = extract_tenant_id(request);
        
        // Check rate limits if quota manager is available
        if let Some(ref quota) = self.quota_manager {
            // Check rate limit
            if let Err(violation) = quota.check_rate_limit(&tenant_id, &operation).await {
                return Err(Status::resource_exhausted(format!(
                    "Rate limit exceeded: {}", violation
                )));
            }
            
            // Record the API request
            quota.record_api_request(&tenant_id, &operation).await;
        }
        
        Ok((security_context, tenant_id))
    }
    
    /// Check and update resource quotas for VM operations
    pub async fn check_vm_quota(
        &self,
        tenant_id: &str,
        vm_config: &crate::types::VmConfig,
        target_node_id: Option<u64>,
    ) -> Result<(), Status> {
        let quota_manager = match &self.quota_manager {
            Some(qm) => qm,
            None => {
                tracing::warn!("Quota manager not available, skipping quota checks");
                return Ok(());
            }
        };
        
        // Check resource quotas
        let resource_request = crate::resource_quotas::ResourceRequest {
            tenant_id: tenant_id.to_string(),
            node_id: target_node_id,
            vcpus: vm_config.vcpus,
            memory_mb: vm_config.memory as u64,
            disk_gb: 5, // Default disk requirement - TODO: make configurable
            timestamp: std::time::SystemTime::now(),
        };
        
        if let Err(violation) = quota_manager.check_resource_quota(&resource_request).await {
            return Err(Status::resource_exhausted(format!(
                "Resource quota exceeded: {}", violation
            )));
        }
        
        Ok(())
    }
    
    /// Update resource usage after VM creation/deletion
    pub async fn update_resource_usage(
        &self,
        tenant_id: &str,
        vcpus: i32,
        memory_mb: i64,
        disk_gb: i64,
        node_id: u64,
    ) {
        if let Some(ref quota_manager) = self.quota_manager {
            if let Err(e) = quota_manager.update_resource_usage(
                tenant_id,
                vcpus,
                memory_mb,
                disk_gb,
                node_id,
            ).await {
                tracing::error!("Failed to update resource usage: {}", e);
            }
        }
    }
    
    /// Authenticate and authorize using Cedar policy engine
    pub async fn authenticate_and_authorize_cedar<T>(
        &self,
        request: &Request<T>,
        action: &str,
        resource_type: &str,
        resource_id: &str,
    ) -> Result<(SecurityContext, String), Status> {
        // First authenticate to get user context
        let permission = Permission::ClusterRead; // Default permission for authentication only
        let security_context = self.authenticate(request, permission).await?;
        
        // Extract tenant ID
        let tenant_id = extract_tenant_id(request);
        
        // If we have a security manager with Cedar, use it for authorization
        if let Some(ref security_manager) = self.security_manager {
            // Get user ID from security context
            let user_id = security_context.user.as_ref()
                .ok_or_else(|| Status::unauthenticated("No user identity"))?;
            
            // Build Cedar context with request metadata
            let mut cedar_context = HashMap::new();
            
            // Add tenant information
            cedar_context.insert("tenant_id".to_string(), serde_json::json!(tenant_id));
            
            // Add current time for time-based policies
            let now = chrono::Utc::now();
            cedar_context.insert("hour".to_string(), serde_json::json!(now.hour()));
            cedar_context.insert("day_of_week".to_string(), serde_json::json!(now.weekday().num_days_from_monday()));
            
            // Add resource metadata if available
            if let Some(ref quota_manager) = self.quota_manager {
                if let Ok(usage) = quota_manager.get_tenant_usage(&tenant_id).await {
                    cedar_context.insert("current_vm_count".to_string(), serde_json::json!(usage.vm_count));
                    cedar_context.insert("current_cpu_usage".to_string(), serde_json::json!(usage.cpu_usage));
                    cedar_context.insert("current_memory_usage".to_string(), serde_json::json!(usage.memory_usage));
                }
            }
            
            // Build resource EntityUid
            let resource_uid = SecurityManager::build_resource_uid(resource_type, resource_id);
            
            // Check Cedar authorization
            let authorized = security_manager
                .check_permission_cedar(user_id, action, &resource_uid, cedar_context)
                .await
                .map_err(|e| Status::internal(format!("Authorization error: {}", e)))?;
            
            if !authorized {
                return Err(Status::permission_denied(format!(
                    "Cedar policy denied: {} -> {} on {}",
                    user_id, action, resource_uid
                )));
            }
        }
        
        Ok((security_context, tenant_id))
    }
}

/// Extract tenant ID from gRPC request metadata
pub fn extract_tenant_id<T>(request: &Request<T>) -> String {
    // Try to extract tenant ID from metadata
    if let Some(tenant_value) = request.metadata().get("tenant-id") {
        if let Ok(tenant_str) = tenant_value.to_str() {
            return tenant_str.to_string();
        }
    }
    
    // Default tenant if not specified
    "default".to_string()
}