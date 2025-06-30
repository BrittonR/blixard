//! Role-Based Access Control (RBAC) module
//!
//! This module provides fine-grained access control for Blixard operations
//! using roles and permissions.

use crate::error::{BlixardError, BlixardResult};
use std::collections::{HashMap, HashSet};
use serde::{Serialize, Deserialize};
use async_trait::async_trait;

/// Permission types for different operations
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Permission {
    // Cluster operations
    ClusterRead,
    ClusterWrite,
    ClusterAdmin,
    
    // VM operations
    VmRead,
    VmWrite,
    VmDelete,
    VmExecute,
    
    // Node operations
    NodeRead,
    NodeWrite,
    NodeAdmin,
    
    // Admin operations
    AdminAll,
}

/// Resource types that can be controlled
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ResourceType {
    Cluster,
    Vm(String),  // VM name
    Node(u64),   // Node ID
    All,
}

/// Action types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Action {
    Read,
    Write,
    Delete,
    Execute,
    Admin,
}

/// Role definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Role {
    pub name: String,
    pub permissions: HashSet<Permission>,
    pub description: String,
}

/// User context with roles and permissions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserContext {
    pub user_id: String,
    pub roles: Vec<String>,
    pub permissions: HashSet<Permission>,
    pub tenant_id: String,
}

/// RBAC manager
#[derive(Debug)]
pub struct RbacManager {
    roles: HashMap<String, Role>,
    user_roles: HashMap<String, Vec<String>>,
}

impl RbacManager {
    /// Create a new RBAC manager with default roles
    pub fn new() -> Self {
        let mut manager = Self {
            roles: HashMap::new(),
            user_roles: HashMap::new(),
        };
        
        // Initialize default roles
        manager.init_default_roles();
        manager
    }
    
    /// Initialize default roles
    fn init_default_roles(&mut self) {
        // Admin role - full access
        self.roles.insert("admin".to_string(), Role {
            name: "admin".to_string(),
            permissions: vec![
                Permission::AdminAll,
                Permission::ClusterRead,
                Permission::ClusterWrite,
                Permission::ClusterAdmin,
                Permission::VmRead,
                Permission::VmWrite,
                Permission::VmDelete,
                Permission::VmExecute,
                Permission::NodeRead,
                Permission::NodeWrite,
                Permission::NodeAdmin,
            ].into_iter().collect(),
            description: "Full administrative access".to_string(),
        });
        
        // Operator role - manage VMs
        self.roles.insert("operator".to_string(), Role {
            name: "operator".to_string(),
            permissions: vec![
                Permission::ClusterRead,
                Permission::VmRead,
                Permission::VmWrite,
                Permission::VmExecute,
                Permission::NodeRead,
            ].into_iter().collect(),
            description: "VM operator access".to_string(),
        });
        
        // Viewer role - read-only
        self.roles.insert("viewer".to_string(), Role {
            name: "viewer".to_string(),
            permissions: vec![
                Permission::ClusterRead,
                Permission::VmRead,
                Permission::NodeRead,
            ].into_iter().collect(),
            description: "Read-only access".to_string(),
        });
    }
    
    /// Check if a user has a specific permission
    pub fn has_permission(&self, user_id: &str, permission: Permission) -> bool {
        if let Some(role_names) = self.user_roles.get(user_id) {
            for role_name in role_names {
                if let Some(role) = self.roles.get(role_name) {
                    if role.permissions.contains(&permission) || 
                       role.permissions.contains(&Permission::AdminAll) {
                        return true;
                    }
                }
            }
        }
        false
    }
    
    /// Get user context with all permissions
    pub fn get_user_context(&self, user_id: &str, tenant_id: &str) -> Option<UserContext> {
        if let Some(role_names) = self.user_roles.get(user_id) {
            let mut permissions = HashSet::new();
            
            for role_name in role_names {
                if let Some(role) = self.roles.get(role_name) {
                    permissions.extend(role.permissions.iter().cloned());
                }
            }
            
            Some(UserContext {
                user_id: user_id.to_string(),
                roles: role_names.clone(),
                permissions,
                tenant_id: tenant_id.to_string(),
            })
        } else {
            None
        }
    }
    
    /// Assign a role to a user
    pub fn assign_role(&mut self, user_id: &str, role_name: &str) -> BlixardResult<()> {
        if !self.roles.contains_key(role_name) {
            return Err(BlixardError::NotFound {
                resource: format!("Role: {}", role_name),
            });
        }
        
        self.user_roles
            .entry(user_id.to_string())
            .or_insert_with(Vec::new)
            .push(role_name.to_string());
        
        Ok(())
    }
    
    /// Remove a role from a user
    pub fn remove_role(&mut self, user_id: &str, role_name: &str) -> BlixardResult<()> {
        if let Some(roles) = self.user_roles.get_mut(user_id) {
            roles.retain(|r| r != role_name);
        }
        Ok(())
    }
}

/// Trait for resources that can be permission-checked
#[async_trait]
pub trait ResourcePermissionChecker {
    /// Check if a user can perform an action on this resource
    async fn check_permission(
        &self,
        user_context: &UserContext,
        action: Action,
    ) -> BlixardResult<()>;
}

impl Default for RbacManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_default_roles() {
        let rbac = RbacManager::new();
        
        // Admin should have all permissions
        rbac.assign_role("admin_user", "admin").unwrap();
        assert!(rbac.has_permission("admin_user", Permission::AdminAll));
        assert!(rbac.has_permission("admin_user", Permission::VmWrite));
        
        // Viewer should only have read permissions
        rbac.assign_role("viewer_user", "viewer").unwrap();
        assert!(rbac.has_permission("viewer_user", Permission::VmRead));
        assert!(!rbac.has_permission("viewer_user", Permission::VmWrite));
    }
}