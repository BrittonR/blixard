//! Middleware for Iroh transport with Cedar authorization support
//!
//! This module provides authorization middleware for Iroh RPC services,
//! similar to GrpcMiddleware but adapted for Iroh's connection model.

use crate::{
    error::{BlixardError, BlixardResult},
    quota_manager::QuotaManager,
    resource_quotas::ApiOperation,
    security::SecurityManager,
};
use chrono::{Datelike, Timelike};
use iroh::endpoint::Connection;
use iroh::NodeId;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::Arc};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

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
    pub async fn register_node(
        &self,
        node_id: NodeId,
        user_id: String,
        roles: Vec<String>,
        tenant_id: String,
    ) {
        let mut node_map = self.node_to_user.write().await;
        node_map.insert(node_id, user_id.clone());

        let mut role_map = self.user_roles.write().await;
        role_map.insert(user_id.clone(), roles);

        let mut tenant_map = self.user_tenants.write().await;
        tenant_map.insert(user_id, tenant_id);
    }

    /// Look up user identity from node ID
    pub async fn get_user_identity(
        &self,
        node_id: &NodeId,
    ) -> Option<(String, Vec<String>, String)> {
        let node_map = self.node_to_user.read().await;
        if let Some(user_id) = node_map.get(node_id) {
            let role_map = self.user_roles.read().await;
            let tenant_map = self.user_tenants.read().await;

            let roles = role_map.get(user_id).cloned().unwrap_or_default();
            let tenant_id = tenant_map
                .get(user_id)
                .cloned()
                .unwrap_or_else(|| "default".to_string());

            Some((user_id.clone(), roles, tenant_id))
        } else {
            None
        }
    }

    /// Load identities from persistent storage
    pub async fn load_from_storage(&self, _path: &std::path::Path) -> BlixardResult<()> {
        // TODO: Implement loading from file/database
        // For now, hardcode some test identities
        info!("Loading node identities from storage");

        // Example: Register known cluster nodes
        // In production, this would load from a secure store
        Ok(())
    }
}

/// Iroh middleware for authorization
#[derive(Clone, Debug)]
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
    pub async fn authenticate_connection(
        &self,
        connection: &Connection,
    ) -> BlixardResult<IrohAuthContext> {
        let node_id = connection
            .remote_node_id()
            .map_err(|e| BlixardError::Security {
                message: format!("Failed to get remote node ID: {}", e),
            })?;

        // Look up user identity from node ID
        if let Some((user_id, roles, tenant_id)) =
            self.identity_registry.get_user_identity(&node_id).await
        {
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
    async fn is_cluster_node(&self, _node_id: &NodeId) -> bool {
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
            cedar_context.insert(
                "tenant_id".to_string(),
                serde_json::json!(auth_context.tenant_id),
            );

            // Add current time for time-based policies
            let now = chrono::Utc::now();
            cedar_context.insert("hour".to_string(), serde_json::json!(now.hour()));
            cedar_context.insert(
                "day_of_week".to_string(),
                serde_json::json!(now.weekday().num_days_from_monday()),
            );

            // Add user roles
            cedar_context.insert(
                "user_roles".to_string(),
                serde_json::json!(auth_context.roles),
            );

            // Add resource metadata if available from quota manager
            if let Some(ref quota_manager) = self.quota_manager {
                let usage = quota_manager
                    .get_tenant_usage(&auth_context.tenant_id)
                    .await;
                cedar_context.insert(
                    "current_vm_count".to_string(),
                    serde_json::json!(usage.vm_usage.active_vms),
                );
                cedar_context.insert(
                    "current_cpu_usage".to_string(),
                    serde_json::json!(usage.vm_usage.used_vcpus),
                );
                cedar_context.insert(
                    "current_memory_usage".to_string(),
                    serde_json::json!(usage.vm_usage.used_memory_mb),
                );
            }

            // Build resource EntityUid
            let resource_uid = SecurityManager::build_resource_uid(resource_type, resource_id);

            // Check Cedar authorization
            security_manager
                .check_permission_cedar(&auth_context.user_id, action, &resource_uid, cedar_context)
                .await
        } else {
            // Cedar is required for authorization
            Err(BlixardError::AuthorizationError {
                message: "Cedar authorization engine not available".to_string(),
            })
        }
    }

    /// Check and update resource quotas
    pub async fn check_resource_quota(
        &self,
        _tenant_id: &str,
        resource_request: &crate::resource_quotas::ResourceRequest,
    ) -> BlixardResult<()> {
        if let Some(ref quota_manager) = self.quota_manager {
            quota_manager
                .check_resource_quota(resource_request)
                .await
                .map_err(|e| BlixardError::Internal {
                    message: format!("Quota check failed: {}", e),
                })?;
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
        let test_bytes = [1u8; 32];
        let public_key = iroh::PublicKey::from_bytes(&test_bytes).expect("valid public key");
        let node_id = NodeId::from(public_key);

        // Register the node
        registry
            .register_node(
                node_id,
                "test-user".to_string(),
                vec!["operator".to_string()],
                "tenant-1".to_string(),
            )
            .await;

        // Look up the identity
        let identity = registry.get_user_identity(&node_id).await;
        assert!(identity.is_some());

        let (user_id, roles, tenant_id) = identity.unwrap();
        assert_eq!(user_id, "test-user");
        assert_eq!(roles, vec!["operator"]);
        assert_eq!(tenant_id, "tenant-1");
    }

    #[tokio::test]
    async fn test_cedar_authorization_without_cedar() {
        let registry = Arc::new(NodeIdentityRegistry::new());
        let middleware = IrohMiddleware::new(None, None, registry);

        // When no SecurityManager is configured, Cedar authorization should return errors
        assert!(!middleware.has_cedar());

        // Test admin role
        let admin_ctx = IrohAuthContext {
            user_id: "admin-user".to_string(),
            roles: vec!["admin".to_string()],
            tenant_id: "default".to_string(),
            metadata: HashMap::new(),
        };

        // Without Cedar, authorization should fail
        let create_vm_result = middleware.authorize_cedar(&admin_ctx, "createVM", "VM", "test-vm").await;
        assert!(create_vm_result.is_err());
        
        let manage_cluster_result = middleware.authorize_cedar(&admin_ctx, "manageCluster", "Cluster", "main").await;
        assert!(manage_cluster_result.is_err());

        // Test operator role  
        let operator_ctx = IrohAuthContext {
            user_id: "operator-user".to_string(),
            roles: vec!["operator".to_string()],
            tenant_id: "default".to_string(),
            metadata: HashMap::new(),
        };

        // Without Cedar, all authorization should fail regardless of role
        let operator_create_result = middleware.authorize_cedar(&operator_ctx, "createVM", "VM", "test-vm").await;
        assert!(operator_create_result.is_err());
        
        let operator_manage_result = middleware.authorize_cedar(&operator_ctx, "manageCluster", "Cluster", "main").await;
        assert!(operator_manage_result.is_err());
    }

    #[tokio::test]
    async fn test_authentication_context_creation() {
        let registry = Arc::new(NodeIdentityRegistry::new());
        
        // Register a test user
        let test_node_id = NodeId::new(&[1; 32]);
        registry.register_user(test_node_id, "test-user".to_string(), vec!["operator".to_string()], "tenant-1".to_string()).await;
        
        let middleware = IrohMiddleware::new(None, None, registry);
        
        // Test different auth context scenarios
        let contexts = vec![
            IrohAuthContext {
                user_id: "admin".to_string(),
                roles: vec!["admin".to_string(), "operator".to_string()],
                tenant_id: "default".to_string(),
                metadata: HashMap::new(),
            },
            IrohAuthContext {
                user_id: "regular-user".to_string(),
                roles: vec!["user".to_string()],
                tenant_id: "tenant-a".to_string(),
                metadata: HashMap::from([
                    ("department".to_string(), "engineering".to_string()),
                    ("region".to_string(), "us-west".to_string()),
                ]),
            },
            IrohAuthContext {
                user_id: "service-account".to_string(),
                roles: vec!["service".to_string(), "read-only".to_string()],
                tenant_id: "system".to_string(),
                metadata: HashMap::new(),
            },
        ];
        
        // Verify each context has the expected structure
        for ctx in contexts {
            assert!(!ctx.user_id.is_empty());
            assert!(!ctx.roles.is_empty());
            assert!(!ctx.tenant_id.is_empty());
            
            // Test that contexts can be used with middleware
            let auth_result = middleware.authorize_cedar(&ctx, "read", "VM", "test").await;
            // Should fail since no Cedar is configured, but shouldn't panic
            assert!(auth_result.is_err());
        }
    }
}
