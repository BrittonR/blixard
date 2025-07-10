//! Security module for Blixard cluster
//!
//! This module provides comprehensive security features including:
//! - Token-based authentication
//! - Secure secrets management
//! - Cedar policy-based authorization
//!
//! Note: Transport security is handled by Iroh's built-in QUIC/TLS 1.3

use crate::cedar_authz::CedarAuthz;
use crate::config_v2::{AuthConfig, SecurityConfig, TlsConfig};
use crate::error::{BlixardError, BlixardResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;
// Removed tonic TLS imports - Iroh uses its own security model based on node IDs
use tracing::{info, warn};

/// Security manager for handling all security operations
#[derive(Debug)]
pub struct SecurityManager {
    /// Configuration
    config: SecurityConfig,

    /// Authentication manager
    auth_manager: Option<AuthManager>,

    /// Secrets manager
    secrets_manager: SecretsManager,

    /// Cedar authorization engine
    cedar_authz: Option<Arc<CedarAuthz>>,
}

// TLS management removed - Iroh handles transport security via QUIC/TLS 1.3

/// Authentication and authorization manager
#[derive(Debug)]
pub struct AuthManager {
    /// Valid API tokens
    valid_tokens: Arc<RwLock<HashMap<String, TokenInfo>>>,

    /// User roles and permissions
    user_roles: Arc<RwLock<HashMap<String, UserRole>>>,

    /// Configuration
    config: AuthConfig,
}

/// Secure secrets storage
#[derive(Debug)]
pub struct SecretsManager {
    /// In-memory secrets cache (encrypted)
    secrets: Arc<RwLock<HashMap<String, EncryptedSecret>>>,

    /// Master key for encryption (in production, this would be from HSM/KMS)
    master_key: [u8; 32],
}

/// Information about an API token
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenInfo {
    /// Token value (hashed)
    pub token_hash: String,

    /// Associated user/service name
    pub user: String,

    /// Token expiration timestamp
    pub expires_at: Option<std::time::SystemTime>,

    /// When the token was created
    pub created_at: std::time::SystemTime,

    /// Whether the token is active
    pub active: bool,
}

/// User role with associated permissions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserRole {
    /// Role name
    pub role: String,

    /// Resource restrictions (tenant isolation)
    pub resource_restrictions: Vec<String>,
}

/// Encrypted secret storage
#[derive(Debug, Clone)]
struct EncryptedSecret {
    /// Encrypted data
    ciphertext: Vec<u8>,

    /// Initialization vector
    iv: [u8; 16],

    /// When the secret was stored
    created_at: std::time::SystemTime,
}

/// Authentication result
#[derive(Debug, Clone)]
pub struct AuthResult {
    /// Whether authentication succeeded
    pub authenticated: bool,

    /// User identity (if authenticated)
    pub user: Option<String>,

    /// Authentication method used
    pub auth_method: String,
}

impl SecurityManager {
    /// Create a new security manager
    pub async fn new(config: SecurityConfig) -> BlixardResult<Self> {
        info!("Initializing security manager");

        // TLS is handled by Iroh's QUIC transport, no separate TLS manager needed

        // Initialize authentication manager if enabled
        let auth_manager = if config.auth.enabled {
            Some(AuthManager::new(config.auth.clone()).await?)
        } else {
            None
        };

        // Initialize secrets manager
        let secrets_manager = SecretsManager::new()?;

        // Initialize Cedar authorization if enabled
        let cedar_authz = if config.auth.enabled {
            // Look for Cedar files in standard locations
            let schema_path = Path::new("cedar/schema.cedarschema.json");
            let policies_dir = Path::new("cedar/policies");

            if schema_path.exists() && policies_dir.exists() {
                match CedarAuthz::new(schema_path, policies_dir).await {
                    Ok(cedar) => {
                        info!("Initialized Cedar authorization engine");
                        Some(Arc::new(cedar))
                    }
                    Err(e) => {
                        warn!("Failed to initialize Cedar authorization: {}", e);
                        None
                    }
                }
            } else {
                warn!("Cedar files not found - authorization will fail");
                None
            }
        } else {
            None
        };

        Ok(Self {
            config,
            auth_manager,
            secrets_manager,
            cedar_authz,
        })
    }

    // TLS configuration methods removed - Iroh handles transport security

    // Client TLS also handled by Iroh

    /// Authenticate a request using the provided token
    pub async fn authenticate_token(&self, token: &str) -> BlixardResult<AuthResult> {
        if let Some(ref auth_manager) = self.auth_manager {
            auth_manager.authenticate_token(token).await
        } else {
            // If authentication is disabled, allow all requests
            Ok(AuthResult {
                authenticated: true,
                user: Some("anonymous".to_string()),
                auth_method: "disabled".to_string(),
            })
        }
    }

    /// Store a secret securely
    pub async fn store_secret(&mut self, key: &str, value: &str) -> BlixardResult<()> {
        self.secrets_manager.store_secret(key, value).await
    }

    /// Retrieve a secret
    pub async fn get_secret(&self, key: &str) -> BlixardResult<Option<String>> {
        self.secrets_manager.get_secret(key).await
    }

    /// Generate a new API token for a user
    pub async fn generate_token(
        &mut self,
        user: &str,
        expires_in: Option<std::time::Duration>,
    ) -> BlixardResult<String> {
        if let Some(ref mut auth_manager) = self.auth_manager {
            auth_manager.generate_token(user, expires_in).await
        } else {
            Err(BlixardError::Security {
                message: "Authentication is disabled".to_string(),
            })
        }
    }

    /// Check permission using Cedar policy engine
    pub async fn check_permission_cedar(
        &self,
        user_id: &str,
        action: &str,
        resource: &str,
        context: HashMap<String, serde_json::Value>,
    ) -> BlixardResult<bool> {
        if let Some(ref cedar) = self.cedar_authz {
            // Build principal EntityUid
            let principal = format!("User::\"{}\"", user_id);

            // Build action EntityUid
            let cedar_action = format!("Action::\"{}\"", action);

            // Use Cedar for authorization
            cedar
                .is_authorized(&principal, &cedar_action, resource, context)
                .await
        } else {
            // Cedar is required for authorization
            Err(BlixardError::AuthorizationError {
                message: "Cedar authorization engine not initialized".to_string(),
            })
        }
    }

    /// Build resource EntityUid based on resource type and ID
    pub fn build_resource_uid(resource_type: &str, resource_id: &str) -> String {
        format!("{}::\"{}\"", resource_type, resource_id)
    }

    /// Add entity to Cedar authorization engine
    pub async fn add_cedar_entity(
        &self,
        entity_type: &str,
        entity_id: &str,
        attributes: HashMap<String, serde_json::Value>,
        parents: Vec<String>,
    ) -> BlixardResult<()> {
        if let Some(ref cedar) = self.cedar_authz {
            cedar
                .add_entity(entity_type, entity_id, attributes, parents)
                .await
        } else {
            // No-op if Cedar is not available
            Ok(())
        }
    }

    /// Reload Cedar policies from disk
    pub async fn reload_cedar_policies(&self) -> BlixardResult<()> {
        if let Some(ref cedar) = self.cedar_authz {
            let policies_dir = Path::new("cedar/policies");
            cedar.reload_policies(policies_dir).await?;
            info!("Reloaded Cedar policies");
        }
        Ok(())
    }
}

// TlsManager implementation removed - Iroh handles all transport security via QUIC/TLS 1.3

impl AuthManager {
    /// Create a new authentication manager
    async fn new(config: AuthConfig) -> BlixardResult<Self> {
        info!(
            "Initializing authentication manager with method: {}",
            config.method
        );

        let manager = Self {
            valid_tokens: Arc::new(RwLock::new(HashMap::new())),
            user_roles: Arc::new(RwLock::new(HashMap::new())),
            config,
        };

        // Load tokens from file if configured
        if let Some(ref token_file) = manager.config.token_file {
            manager.load_tokens_from_file(token_file).await?;
        }

        // Create default admin role
        manager.create_default_roles().await?;

        Ok(manager)
    }

    /// Load tokens from a file
    async fn load_tokens_from_file(&self, token_file: &Path) -> BlixardResult<()> {
        if !token_file.exists() {
            warn!(
                "Token file {:?} does not exist, creating empty token store",
                token_file
            );
            return Ok(());
        }

        let content =
            tokio::fs::read_to_string(token_file)
                .await
                .map_err(|e| BlixardError::Security {
                    message: format!("Failed to read token file {:?}: {}", token_file, e),
                })?;

        let tokens: HashMap<String, TokenInfo> =
            serde_json::from_str(&content).map_err(|e| BlixardError::Security {
                message: format!("Failed to parse token file: {}", e),
            })?;

        let mut valid_tokens = self.valid_tokens.write().await;
        *valid_tokens = tokens;

        info!("Loaded {} tokens from file", valid_tokens.len());
        Ok(())
    }

    /// Create default user roles
    async fn create_default_roles(&self) -> BlixardResult<()> {
        let mut user_roles = self.user_roles.write().await;

        // Admin role - full access
        user_roles.insert(
            "admin".to_string(),
            UserRole {
                role: "admin".to_string(),
                resource_restrictions: vec![],
            },
        );

        // Read-only role
        user_roles.insert(
            "readonly".to_string(),
            UserRole {
                role: "readonly".to_string(),
                resource_restrictions: vec![],
            },
        );

        // VM operator role
        user_roles.insert(
            "vm-operator".to_string(),
            UserRole {
                role: "vm-operator".to_string(),
                resource_restrictions: vec![],
            },
        );

        info!("Created default user roles");
        Ok(())
    }

    /// Authenticate a token
    async fn authenticate_token(&self, token: &str) -> BlixardResult<AuthResult> {
        let token_hash = Self::hash_token(token);
        let valid_tokens = self.valid_tokens.read().await;

        if let Some(token_info) = valid_tokens.get(&token_hash) {
            // Check if token is active
            if !token_info.active {
                return Ok(AuthResult {
                    authenticated: false,
                    user: None,
                    auth_method: "token".to_string(),
                });
            }

            // Check if token is expired
            if let Some(expires_at) = token_info.expires_at {
                if std::time::SystemTime::now() > expires_at {
                    return Ok(AuthResult {
                        authenticated: false,
                        user: None,
                        auth_method: "token".to_string(),
                    });
                }
            }

            Ok(AuthResult {
                authenticated: true,
                user: Some(token_info.user.clone()),
                auth_method: "token".to_string(),
            })
        } else {
            Ok(AuthResult {
                authenticated: false,
                user: None,
                auth_method: "token".to_string(),
            })
        }
    }

    /// Generate a new API token
    async fn generate_token(
        &mut self,
        user: &str,
        expires_in: Option<std::time::Duration>,
    ) -> BlixardResult<String> {
        // Generate a cryptographically secure random token
        let token = Self::generate_secure_token();
        let token_hash = Self::hash_token(&token);

        let expires_at = expires_in.map(|duration| std::time::SystemTime::now() + duration);

        let token_info = TokenInfo {
            token_hash: token_hash.clone(),
            user: user.to_string(),
            expires_at,
            created_at: std::time::SystemTime::now(),
            active: true,
        };

        let mut valid_tokens = self.valid_tokens.write().await;
        valid_tokens.insert(token_hash, token_info);

        info!("Generated new token for user: {}", user);
        Ok(token)
    }

    /// Generate a cryptographically secure token
    fn generate_secure_token() -> String {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        let token_bytes: [u8; 32] = rng.gen();
        hex::encode(token_bytes)
    }

    /// Hash a token for secure storage
    fn hash_token(token: &str) -> String {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(token.as_bytes());
        hex::encode(hasher.finalize())
    }
}

impl SecretsManager {
    /// Create a new secrets manager
    fn new() -> BlixardResult<Self> {
        // Generate secure master key from environment or derive from password
        let master_key = match std::env::var("BLIXARD_MASTER_KEY") {
            Ok(key_hex) => {
                let bytes = hex::decode(key_hex)
                    .map_err(|_| BlixardError::Security { 
                        message: "Invalid master key format - must be 64 hex characters".to_string() 
                    })?;
                if bytes.len() != 32 {
                    return Err(BlixardError::Security { 
                        message: "Master key must be exactly 32 bytes (64 hex characters)".to_string() 
                    });
                }
                let mut key = [0u8; 32];
                key.copy_from_slice(&bytes);
                key
            }
            Err(_) => {
                // For production, require explicit key
                return Err(BlixardError::Security {
                    message: "BLIXARD_MASTER_KEY environment variable required for secure secrets storage. Generate with: openssl rand -hex 32".to_string()
                });
            }
        };

        Ok(Self {
            secrets: Arc::new(RwLock::new(HashMap::new())),
            master_key,
        })
    }

    /// Store a secret securely
    async fn store_secret(&mut self, key: &str, value: &str) -> BlixardResult<()> {
        let encrypted = self.encrypt_secret(value)?;
        let mut secrets = self.secrets.write().await;
        secrets.insert(key.to_string(), encrypted);
        info!("Stored secret: {}", key);
        Ok(())
    }

    /// Retrieve a secret
    async fn get_secret(&self, key: &str) -> BlixardResult<Option<String>> {
        let secrets = self.secrets.read().await;
        if let Some(encrypted) = secrets.get(key) {
            let decrypted = self.decrypt_secret(encrypted)?;
            Ok(Some(decrypted))
        } else {
            Ok(None)
        }
    }

    /// Encrypt a secret value
    fn encrypt_secret(&self, value: &str) -> BlixardResult<EncryptedSecret> {
        use aes_gcm::{aead::Aead, Aes256Gcm, KeyInit, Nonce};
        use rand::Rng;

        let cipher =
            Aes256Gcm::new_from_slice(&self.master_key).map_err(|e| BlixardError::Security {
                message: format!("Failed to create cipher: {}", e),
            })?;

        let mut rng = rand::thread_rng();
        let iv: [u8; 16] = rng.gen();
        let nonce = Nonce::from_slice(&iv[..12]); // AES-GCM uses 12-byte nonce

        let ciphertext =
            cipher
                .encrypt(nonce, value.as_bytes())
                .map_err(|e| BlixardError::Security {
                    message: format!("Failed to encrypt secret: {}", e),
                })?;

        Ok(EncryptedSecret {
            ciphertext,
            iv,
            created_at: std::time::SystemTime::now(),
        })
    }

    /// Decrypt a secret value
    fn decrypt_secret(&self, encrypted: &EncryptedSecret) -> BlixardResult<String> {
        use aes_gcm::{aead::Aead, Aes256Gcm, KeyInit, Nonce};

        let cipher =
            Aes256Gcm::new_from_slice(&self.master_key).map_err(|e| BlixardError::Security {
                message: format!("Failed to create cipher: {}", e),
            })?;

        let nonce = Nonce::from_slice(&encrypted.iv[..12]);

        let plaintext = cipher
            .decrypt(nonce, encrypted.ciphertext.as_slice())
            .map_err(|e| BlixardError::Security {
                message: format!("Failed to decrypt secret: {}", e),
            })?;

        String::from_utf8(plaintext).map_err(|e| BlixardError::Security {
            message: format!("Invalid UTF-8 in decrypted secret: {}", e),
        })
    }
}

// Temporarily disabled: uses tonic/gRPC which we're removing
// /// Extract authentication token from gRPC request metadata
// pub fn extract_auth_token<T>(request: &tonic::Request<T>) -> Option<String> {
//     // Try Authorization header first
//     if let Some(auth_header) = request.metadata().get("authorization") {
//         if let Ok(auth_str) = auth_header.to_str() {
//             if auth_str.starts_with("Bearer ") {
//                 return Some(auth_str[7..].to_string());
//             }
//         }
//     }
//
//     // Try x-api-token header as fallback
//     if let Some(token_header) = request.metadata().get("x-api-token") {
//         if let Ok(token_str) = token_header.to_str() {
//             return Some(token_str.to_string());
//         }
//     }
//
//     None
// }

/// Create a default security configuration for development
pub fn default_dev_security_config() -> SecurityConfig {
    SecurityConfig {
        tls: TlsConfig {
            enabled: false,
            cert_file: None,
            key_file: None,
            ca_file: None,
            require_client_cert: false,
        },
        auth: AuthConfig {
            enabled: false,
            method: "token".to_string(),
            token_file: None,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_security_manager_creation() {
        let config = default_dev_security_config();
        let security_manager = SecurityManager::new(config).await
            .expect("Failed to create security manager with dev config");

        // Iroh handles transport security via QUIC/TLS - no separate TLS config needed
        // Verify that authentication is properly disabled in dev config
        let auth_result = security_manager
            .authenticate_token("any-token")
            .await
            .expect("Token authentication should not fail with dev config");
        assert!(auth_result.authenticated);
        assert_eq!(auth_result.user, Some("anonymous".to_string()));
        assert_eq!(auth_result.auth_method, "disabled");
        
        // Test that various token formats are accepted when auth is disabled
        let test_tokens = vec![
            "simple-token",
            "bearer-token-123",
            "",
            "very-long-token-with-special-characters-!@#$%",
        ];
        
        for token in test_tokens {
            let result = security_manager.authenticate_token(token).await
                .expect("Authentication should not fail when disabled");
            assert!(result.authenticated, "Token '{}' should be accepted when auth is disabled", token);
        }
    }

    #[tokio::test]
    async fn test_token_authentication() {
        let config = SecurityConfig {
            tls: TlsConfig {
                enabled: false,
                cert_file: None,
                key_file: None,
                ca_file: None,
                require_client_cert: false,
            },
            auth: AuthConfig {
                enabled: true,
                method: "token".to_string(),
                token_file: None,
            },
        };

        let mut security_manager = SecurityManager::new(config).await
            .expect("Failed to create security manager with auth enabled");

        // Generate a token
        let token = security_manager
            .generate_token("test-user", Some(std::time::Duration::from_secs(3600)))
            .await
            .expect("Failed to generate token for test user");

        // Authenticate with the token
        let auth_result = security_manager.authenticate_token(&token).await
            .expect("Failed to authenticate valid token");
        assert!(auth_result.authenticated);
        assert_eq!(auth_result.user.expect("Authenticated user should have a name"), "test-user");

        // Test invalid token
        let invalid_result = security_manager
            .authenticate_token("invalid-token")
            .await
            .expect("Authentication check should not fail, even for invalid tokens");
        assert!(!invalid_result.authenticated);
    }

    #[tokio::test]
    async fn test_secrets_management() {
        let config = default_dev_security_config();
        let mut security_manager = SecurityManager::new(config).await
            .expect("Failed to create security manager for secrets test");

        // Store a secret
        security_manager
            .store_secret("test-key", "secret-value")
            .await
            .expect("Failed to store secret");

        // Retrieve the secret
        let retrieved = security_manager.get_secret("test-key").await
            .expect("Failed to retrieve stored secret");
        assert_eq!(retrieved.expect("Retrieved secret should not be None"), "secret-value");

        // Non-existent secret
        let missing = security_manager.get_secret("missing-key").await
            .expect("Getting missing secret should not fail");
        assert!(missing.is_none());
    }

    #[test]
    fn test_token_hashing() {
        let token = "test-token";
        let hash1 = AuthManager::hash_token(token);
        let hash2 = AuthManager::hash_token(token);

        // Same token should produce same hash
        assert_eq!(hash1, hash2);

        // Different tokens should produce different hashes
        let different_hash = AuthManager::hash_token("different-token");
        assert_ne!(hash1, different_hash);
    }
}
