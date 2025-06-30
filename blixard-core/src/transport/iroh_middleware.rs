//! Middleware for Iroh transport with Cedar authorization support
//!
//! This module provides authorization middleware for Iroh RPC services,
//! similar to GrpcMiddleware but adapted for Iroh's connection model.

use crate::{
    error::{BlixardError, BlixardResult},
    security::{SecurityManager, Permission},
    quota_manager::QuotaManager,
    resource_quotas::ApiOperation,
};
use std::{sync::Arc, collections::HashMap};
use iroh::endpoint::Connection;
use iroh::NodeId;
use tokio::sync::RwLock;
use tracing::{debug, warn, info};
use serde::{Serialize, Deserialize};
use chrono::{Datelike, Timelike};

/// Authentication context for Iroh connections
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrohAuthContext {
    /// Authenticated user ID (derived from node ID mapping)
    pub user_id: String,
    /// User's roles
    pub roles: Vec<String>,
    /// Tenant ID
    pub tenant_id: String,
    /// Additional metadata
    pub metadata: HashMap<String, String>,
}

/// Node identity registry for mapping Iroh node IDs to users
#[derive(Debug, Clone)]
pub struct NodeIdentityRegistry {
    /// Map from Iroh NodeId to user identity
    node_to_user: Arc<RwLock<HashMap<NodeId, String>>>,
    /// Map from user to roles
    user_roles: Arc<RwLock<HashMap<String, Vec<String>>>>,
    /// Map from user to tenant
    user_tenants: Arc<RwLock<HashMap<String, String>>>,
}

impl NodeIdentityRegistry {
    pub fn new() -> Self {
        Self {
            node_to_user: Arc::new(RwLock::new(HashMap::new())),
            user_roles: Arc::new(RwLock::new(HashMap::new())),
            user_tenants: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    
    /// Register a node ID with a user identity
    pub async fn register_node(&self, node_id: NodeId, user_id: String, roles: Vec<String>, tenant_id: String) {
        let mut node_map = self.node_to_user.write().await;
        node_map.insert(node_id, user_id.clone());
        
        let mut role_map = self.user_roles.write().await;
        role_map.insert(user_id.clone(), roles);
        
        let mut tenant_map = self.user_tenants.write().await;
        tenant_map.insert(user_id, tenant_id);
    }
    
    /// Look up user identity from node ID
    pub async fn get_user_identity(&self, node_id: &NodeId) -> Option<(String, Vec<String>, String)> {
        let node_map = self.node_to_user.read().await;
        if let Some(user_id) = node_map.get(node_id) {
            let role_map = self.user_roles.read().await;
            let tenant_map = self.user_tenants.read().await;
            
            let roles = role_map.get(user_id).cloned().unwrap_or_default();
            let tenant_id = tenant_map.get(user_id).cloned().unwrap_or_else(|| "default".to_string());
            
            Some((user_id.clone(), roles, tenant_id))
        } else {
            None
        }
    }
    
    /// Load identities from persistent storage
    pub async fn load_from_storage(&self, path: &std::path::Path) -> BlixardResult<()> {
        // TODO: Implement loading from file/database
        // For now, hardcode some test identities
        info!("Loading node identities from storage");
        
        // Example: Register known cluster nodes
        // In production, this would load from a secure store
        Ok(())
    }
}

/// Iroh middleware for authorization
#[derive(Clone)]
pub struct IrohMiddleware {
    security_manager: Option<Arc<SecurityManager>>,
    quota_manager: Option<Arc<QuotaManager>>,
    identity_registry: Arc<NodeIdentityRegistry>,
}

impl IrohMiddleware {
    /// Create new Iroh middleware
    pub fn new(
        security_manager: Option<Arc<SecurityManager>>,
        quota_manager: Option<Arc<QuotaManager>>,
        identity_registry: Arc<NodeIdentityRegistry>,
    ) -> Self {
        Self {
            security_manager,
            quota_manager,
            identity_registry,
        }
    }
    
    /// Check if Cedar authorization is available
    pub fn has_cedar(&self) -> bool {
        self.security_manager.is_some()
    }
    
    /// Authenticate connection and get auth context
    pub async fn authenticate_connection(&self, connection: &Connection) -> BlixardResult<IrohAuthContext> {
        let node_id = connection.remote_node_id();
        
        // Look up user identity from node ID
        if let Some((user_id, roles, tenant_id)) = self.identity_registry.get_user_identity(&node_id).await {
            debug!("Authenticated Iroh connection from user: {}", user_id);
            
            Ok(IrohAuthContext {
                user_id,
                roles,
                tenant_id,
                metadata: HashMap::new(),
            })
        } else {
            // Unknown node - could allow anonymous access or reject
            warn!("Unknown Iroh node ID: {:?}", node_id);
            
            // For cluster nodes, we might allow based on node ID alone
            if self.is_cluster_node(&node_id).await {
                Ok(IrohAuthContext {
                    user_id: format!("node:{}", node_id),
                    roles: vec!["cluster_node".to_string()],
                    tenant_id: "system".to_string(),
                    metadata: HashMap::new(),
                })
            } else {
                Err(BlixardError::Security {
                    message: format!("Unknown node identity: {:?}", node_id),
                })
            }
        }
    }
    
    /// Check if a node ID belongs to the cluster
    async fn is_cluster_node(&self, node_id: &NodeId) -> bool {
        // TODO: Check against known cluster members
        // For now, return false
        false
    }
    
    /// Authorize an action using Cedar
    pub async fn authorize_cedar(
        &self,
        auth_context: &IrohAuthContext,
        action: &str,
        resource_type: &str,
        resource_id: &str,
    ) -> BlixardResult<bool> {
        if let Some(ref security_manager) = self.security_manager {
            // Build Cedar context
            let mut cedar_context = HashMap::new();
            
            // Add tenant information
            cedar_context.insert("tenant_id".to_string(), serde_json::json!(auth_context.tenant_id));
            
            // Add current time for time-based policies
            let now = chrono::Utc::now();
            cedar_context.insert("hour".to_string(), serde_json::json!(now.hour()));
            cedar_context.insert("day_of_week".to_string(), serde_json::json!(now.weekday().num_days_from_monday()));
            
            // Add user roles
            cedar_context.insert("user_roles".to_string(), serde_json::json!(auth_context.roles));
            
            // Add resource metadata if available from quota manager
            if let Some(ref quota_manager) = self.quota_manager {
                if let Ok(usage) = quota_manager.get_tenant_usage(&auth_context.tenant_id).await {
                    cedar_context.insert("current_vm_count".to_string(), serde_json::json!(usage.vm_count));
                    cedar_context.insert("current_cpu_usage".to_string(), serde_json::json!(usage.cpu_usage));
                    cedar_context.insert("current_memory_usage".to_string(), serde_json::json!(usage.memory_usage));
                }
            }
            
            // Build resource EntityUid
            let resource_uid = SecurityManager::build_resource_uid(resource_type, resource_id);
            
            // Check Cedar authorization
            security_manager
                .check_permission_cedar(&auth_context.user_id, action, &resource_uid, cedar_context)
                .await
        } else {
            // Fall back to role-based check
            self.check_permission_fallback(auth_context, action).await
        }
    }
    
    /// Fallback permission check when Cedar is not available
    async fn check_permission_fallback(&self, auth_context: &IrohAuthContext, action: &str) -> BlixardResult<bool> {
        // Map Cedar actions to traditional permissions
        let permission = match action {
            "readCluster" | "readNode" => Permission::ClusterRead,
            "manageCluster" | "joinCluster" | "leaveCluster" => Permission::ClusterWrite,
            "readVM" => Permission::VmRead,
            "createVM" | "updateVM" | "deleteVM" | "executeVM" => Permission::VmWrite,
            "readMetrics" => Permission::MetricsRead,
            _ => return Ok(false),
        };
        
        // Check if any role grants this permission
        // Simple implementation - in production, would check role definitions
        Ok(auth_context.roles.iter().any(|role| {
            match role.as_str() {
                "admin" => true, // Admin has all permissions
                "operator" => matches!(permission, Permission::VmRead | Permission::VmWrite | Permission::ClusterRead),
                "viewer" => matches!(permission, Permission::VmRead | Permission::ClusterRead | Permission::MetricsRead),
                "cluster_node" => matches!(permission, Permission::ClusterRead | Permission::ClusterWrite),
                _ => false,
            }
        }))
    }
    
    /// Check and update resource quotas
    pub async fn check_resource_quota(
        &self,
        tenant_id: &str,
        resource_request: &crate::resource_quotas::ResourceRequest,
    ) -> BlixardResult<()> {
        if let Some(ref quota_manager) = self.quota_manager {
            quota_manager.check_resource_quota(resource_request).await?;
        }
        Ok(())
    }
    
    /// Record API operation for rate limiting
    pub async fn record_api_operation(&self, tenant_id: &str, operation: &ApiOperation) {
        if let Some(ref quota_manager) = self.quota_manager {
            quota_manager.record_api_request(tenant_id, operation).await;
        }
    }
}

/// Extension trait for adding auth metadata to RPC requests
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthenticatedRequest<T> {
    /// The actual request payload
    pub request: T,
    /// Authentication context (optional for backward compatibility)
    pub auth: Option<IrohAuthContext>,
}

impl<T> AuthenticatedRequest<T> {
    pub fn new(request: T) -> Self {
        Self {
            request,
            auth: None,
        }
    }
    
    pub fn with_auth(request: T, auth: IrohAuthContext) -> Self {
        Self {
            request,
            auth: Some(auth),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_node_identity_registry() {
        let registry = NodeIdentityRegistry::new();
        
        // Create a test node ID (normally this would be a real cryptographic ID)
        let node_id = NodeId::new([1u8; 32]);
        
        // Register the node
        registry.register_node(
            node_id,
            "test-user".to_string(),
            vec!["operator".to_string()],
            "tenant-1".to_string(),
        ).await;
        
        // Look up the identity
        let identity = registry.get_user_identity(&node_id).await;
        assert!(identity.is_some());
        
        let (user_id, roles, tenant_id) = identity.unwrap();
        assert_eq!(user_id, "test-user");
        assert_eq!(roles, vec!["operator"]);
        assert_eq!(tenant_id, "tenant-1");
    }
    
    #[tokio::test]
    async fn test_fallback_permissions() {
        let registry = Arc::new(NodeIdentityRegistry::new());
        let middleware = IrohMiddleware::new(None, None, registry);
        
        // Test admin role
        let admin_ctx = IrohAuthContext {
            user_id: "admin-user".to_string(),
            roles: vec!["admin".to_string()],
            tenant_id: "default".to_string(),
            metadata: HashMap::new(),
        };
        
        assert!(middleware.check_permission_fallback(&admin_ctx, "createVM").await.unwrap());
        assert!(middleware.check_permission_fallback(&admin_ctx, "manageCluster").await.unwrap());
        
        // Test operator role
        let operator_ctx = IrohAuthContext {
            user_id: "operator-user".to_string(),
            roles: vec!["operator".to_string()],
            tenant_id: "default".to_string(),
            metadata: HashMap::new(),
        };
        
        assert!(middleware.check_permission_fallback(&operator_ctx, "createVM").await.unwrap());
        assert!(!middleware.check_permission_fallback(&operator_ctx, "manageCluster").await.unwrap());
    }
}