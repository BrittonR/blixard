//! Certificate-based enrollment for Iroh node identities
//!
//! This module provides automatic node registration using X.509 certificates
//! or enrollment tokens, allowing trusted nodes to automatically receive
//! appropriate roles and permissions.

use crate::{
    error::{BlixardError, BlixardResult},
    common::file_io::{read_config_file, write_config_file, file_exists},
    transport::iroh_middleware::NodeIdentityRegistry,
};
use iroh::NodeId;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, SystemTime},
};
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

/// Enrollment token for automatic node registration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnrollmentToken {
    /// Unique token ID
    pub token_id: String,
    /// Token secret (should be kept secure)
    pub secret: String,
    /// User ID to assign
    pub user_id: String,
    /// Roles to grant
    pub roles: Vec<String>,
    /// Tenant ID
    pub tenant_id: String,
    /// Expiration time
    pub expires_at: SystemTime,
    /// Whether token can be used multiple times
    pub multi_use: bool,
    /// Maximum number of uses (if multi_use)
    pub max_uses: Option<u32>,
    /// Current use count
    #[serde(default)]
    pub use_count: u32,
}

/// Certificate-based enrollment configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CertificateEnrollmentConfig {
    /// Path to CA certificate for validation
    pub ca_cert_path: PathBuf,
    /// Mapping from certificate attributes to roles
    pub cert_role_mappings: Vec<CertRoleMapping>,
    /// Default tenant for certificate-enrolled nodes
    pub default_tenant: String,
    /// Whether to allow self-signed certificates
    pub allow_self_signed: bool,
}

/// Maps certificate attributes to roles
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CertRoleMapping {
    /// Certificate field to match (e.g., "CN", "OU", "O")
    pub cert_field: String,
    /// Pattern to match (supports wildcards)
    pub pattern: String,
    /// Roles to grant if matched
    pub roles: Vec<String>,
    /// Optional tenant override
    pub tenant: Option<String>,
}

/// Enrollment result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnrollmentResult {
    pub success: bool,
    pub user_id: String,
    pub roles: Vec<String>,
    pub tenant_id: String,
    pub message: String,
}

/// Identity enrollment manager
pub struct IdentityEnrollmentManager {
    /// Node identity registry
    registry: Arc<NodeIdentityRegistry>,
    /// Active enrollment tokens
    tokens: Arc<RwLock<HashMap<String, EnrollmentToken>>>,
    /// Certificate enrollment config
    cert_config: Option<CertificateEnrollmentConfig>,
    /// Path to persist enrollment state
    state_path: Option<PathBuf>,
}

impl IdentityEnrollmentManager {
    /// Create a new enrollment manager
    pub fn new(
        registry: Arc<NodeIdentityRegistry>,
        cert_config: Option<CertificateEnrollmentConfig>,
        state_path: Option<PathBuf>,
    ) -> Self {
        Self {
            registry,
            tokens: Arc::new(RwLock::new(HashMap::new())),
            cert_config,
            state_path,
        }
    }
    
    /// Load enrollment state from disk
    pub async fn load_state(&self) -> BlixardResult<()> {
        if let Some(ref path) = self.state_path {
            if file_exists(path).await {
                let tokens: HashMap<String, EnrollmentToken> = 
                    read_config_file(path, "enrollment state").await?;
                
                let mut token_store = self.tokens.write().await;
                *token_store = tokens;
                
                info!("Loaded {} enrollment tokens from state", token_store.len());
            }
        }
        Ok(())
    }
    
    /// Save enrollment state to disk
    pub async fn save_state(&self) -> BlixardResult<()> {
        if let Some(ref path) = self.state_path {
            let tokens = self.tokens.read().await;
            write_config_file(path, &*tokens, "enrollment state", true).await?;
            debug!("Saved enrollment state to disk");
        }
        Ok(())
    }
    
    /// Generate a new enrollment token
    pub async fn generate_enrollment_token(
        &self,
        user_id: String,
        roles: Vec<String>,
        tenant_id: String,
        validity_duration: Duration,
        multi_use: bool,
        max_uses: Option<u32>,
    ) -> BlixardResult<EnrollmentToken> {
        use rand::{thread_rng, Rng};
        use rand::distributions::Alphanumeric;
        
        // Generate secure random token
        let token_id = format!("enroll_{}", uuid::Uuid::new_v4());
        let secret: String = thread_rng()
            .sample_iter(&Alphanumeric)
            .take(32)
            .map(char::from)
            .collect();
        
        let token = EnrollmentToken {
            token_id: token_id.clone(),
            secret: secret.clone(),
            user_id,
            roles,
            tenant_id,
            expires_at: SystemTime::now() + validity_duration,
            multi_use,
            max_uses,
            use_count: 0,
        };
        
        // Store token
        let mut tokens = self.tokens.write().await;
        tokens.insert(token_id, token.clone());
        
        // Save state
        drop(tokens);
        self.save_state().await?;
        
        info!("Generated enrollment token: {}", token.token_id);
        Ok(token)
    }
    
    /// Enroll a node using a token
    pub async fn enroll_with_token(
        &self,
        node_id: NodeId,
        token_id: &str,
        token_secret: &str,
    ) -> BlixardResult<EnrollmentResult> {
        let mut tokens = self.tokens.write().await;
        
        // Find and validate token
        let token = tokens.get_mut(token_id)
            .ok_or_else(|| BlixardError::Security {
                message: "Invalid enrollment token".to_string(),
            })?;
        
        // Check secret
        if token.secret != token_secret {
            return Err(BlixardError::Security {
                message: "Invalid token secret".to_string(),
            });
        }
        
        // Check expiration
        if SystemTime::now() > token.expires_at {
            return Err(BlixardError::Security {
                message: "Enrollment token expired".to_string(),
            });
        }
        
        // Check usage limits
        if !token.multi_use && token.use_count > 0 {
            return Err(BlixardError::Security {
                message: "Single-use token already used".to_string(),
            });
        }
        
        if let Some(max_uses) = token.max_uses {
            if token.use_count >= max_uses {
                return Err(BlixardError::Security {
                    message: "Token usage limit exceeded".to_string(),
                });
            }
        }
        
        // Register the node
        self.registry.register_node(
            node_id,
            token.user_id.clone(),
            token.roles.clone(),
            token.tenant_id.clone(),
        ).await;
        
        // Update use count
        token.use_count += 1;
        
        // Remove single-use tokens
        let result = EnrollmentResult {
            success: true,
            user_id: token.user_id.clone(),
            roles: token.roles.clone(),
            tenant_id: token.tenant_id.clone(),
            message: format!("Successfully enrolled node {:?}", node_id),
        };
        
        if !token.multi_use {
            tokens.remove(token_id);
        }
        
        // Save state
        drop(tokens);
        self.save_state().await?;
        
        info!("Node {:?} enrolled as user {}", node_id, result.user_id);
        Ok(result)
    }
    
    /// Enroll a node using certificate attributes
    pub async fn enroll_with_certificate(
        &self,
        node_id: NodeId,
        cert_attributes: HashMap<String, String>,
    ) -> BlixardResult<EnrollmentResult> {
        let config = self.cert_config.as_ref()
            .ok_or_else(|| BlixardError::Configuration {
                message: "Certificate enrollment not configured".to_string(),
            })?;
        
        // Find matching role mappings
        let mut matched_roles = Vec::new();
        let mut tenant_id = config.default_tenant.clone();
        
        for mapping in &config.cert_role_mappings {
            if let Some(cert_value) = cert_attributes.get(&mapping.cert_field) {
                if self.matches_pattern(cert_value, &mapping.pattern) {
                    matched_roles.extend(mapping.roles.clone());
                    if let Some(ref tenant) = mapping.tenant {
                        tenant_id = tenant.clone();
                    }
                }
            }
        }
        
        if matched_roles.is_empty() {
            return Err(BlixardError::Security {
                message: "No matching certificate role mappings".to_string(),
            });
        }
        
        // Generate user ID from certificate
        let user_id = self.generate_user_id_from_cert(&cert_attributes);
        
        // Register the node
        self.registry.register_node(
            node_id,
            user_id.clone(),
            matched_roles.clone(),
            tenant_id.clone(),
        ).await;
        
        info!("Node {:?} enrolled via certificate as {}", node_id, user_id);
        
        Ok(EnrollmentResult {
            success: true,
            user_id,
            roles: matched_roles,
            tenant_id,
            message: format!("Successfully enrolled via certificate"),
        })
    }
    
    /// Check if a value matches a pattern (supports * wildcard)
    fn matches_pattern(&self, value: &str, pattern: &str) -> bool {
        if pattern.contains('*') {
            // Simple wildcard matching
            let parts: Vec<&str> = pattern.split('*').collect();
            if parts.is_empty() {
                return true;
            }
            
            let mut pos = 0;
            for (i, part) in parts.iter().enumerate() {
                if part.is_empty() {
                    continue;
                }
                
                if i == 0 && !value.starts_with(part) {
                    return false;
                }
                
                if let Some(found_pos) = value[pos..].find(part) {
                    pos += found_pos + part.len();
                } else {
                    return false;
                }
            }
            
            if let Some(last_part) = parts.last() {
                if !last_part.is_empty() && !value.ends_with(last_part) {
                    return false;
                }
            }
            
            true
        } else {
            value == pattern
        }
    }
    
    /// Generate user ID from certificate attributes
    fn generate_user_id_from_cert(&self, attrs: &HashMap<String, String>) -> String {
        // Try common name first
        if let Some(cn) = attrs.get("CN") {
            return cn.clone();
        }
        
        // Fall back to email
        if let Some(email) = attrs.get("emailAddress") {
            return email.clone();
        }
        
        // Fall back to organizational unit + organization
        if let (Some(ou), Some(o)) = (attrs.get("OU"), attrs.get("O")) {
            return format!("{}@{}", ou, o);
        }
        
        // Last resort - use any available attribute
        attrs.values().next().cloned().unwrap_or_else(|| "unknown".to_string())
    }
    
    /// Revoke an enrollment token
    pub async fn revoke_token(&self, token_id: &str) -> BlixardResult<()> {
        let mut tokens = self.tokens.write().await;
        tokens.remove(token_id);
        
        drop(tokens);
        self.save_state().await?;
        
        info!("Revoked enrollment token: {}", token_id);
        Ok(())
    }
    
    /// List active enrollment tokens (without secrets)
    pub async fn list_tokens(&self) -> Vec<EnrollmentTokenInfo> {
        let tokens = self.tokens.read().await;
        tokens.values().map(|t| EnrollmentTokenInfo {
            token_id: t.token_id.clone(),
            user_id: t.user_id.clone(),
            roles: t.roles.clone(),
            tenant_id: t.tenant_id.clone(),
            expires_at: t.expires_at,
            multi_use: t.multi_use,
            use_count: t.use_count,
            max_uses: t.max_uses,
        }).collect()
    }
}

/// Public token information (no secrets)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnrollmentTokenInfo {
    pub token_id: String,
    pub user_id: String,
    pub roles: Vec<String>,
    pub tenant_id: String,
    pub expires_at: SystemTime,
    pub multi_use: bool,
    pub use_count: u32,
    pub max_uses: Option<u32>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    
    #[tokio::test]
    async fn test_pattern_matching() {
        let manager = IdentityEnrollmentManager::new(
            Arc::new(NodeIdentityRegistry::new()),
            None,
            None,
        );
        
        assert!(manager.matches_pattern("test.example.com", "*.example.com"));
        assert!(manager.matches_pattern("node1.cluster.local", "node*.cluster.local"));
        assert!(manager.matches_pattern("admin-user", "admin-*"));
        assert!(!manager.matches_pattern("test.example.com", "*.example.org"));
    }
    
    #[tokio::test]
    async fn test_enrollment_token_generation() {
        let registry = Arc::new(NodeIdentityRegistry::new());
        let manager = IdentityEnrollmentManager::new(registry, None, None);
        
        let token = manager.generate_enrollment_token(
            "test-user".to_string(),
            vec!["operator".to_string()],
            "tenant-1".to_string(),
            Duration::from_secs(3600),
            false,
            None,
        ).await.unwrap();
        
        assert_eq!(token.user_id, "test-user");
        assert_eq!(token.roles, vec!["operator"]);
        assert!(!token.multi_use);
    }
}