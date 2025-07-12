//! Security configuration (TLS, authentication)

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Security configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct SecurityConfig {
    /// TLS configuration
    pub tls: TlsConfig,

    /// Authentication configuration
    pub auth: AuthConfig,
}

/// TLS configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct TlsConfig {
    /// Enable TLS
    pub enabled: bool,

    /// Certificate file
    pub cert_file: Option<PathBuf>,

    /// Key file
    pub key_file: Option<PathBuf>,

    /// CA certificate file
    pub ca_file: Option<PathBuf>,

    /// Require client certificates
    pub require_client_cert: bool,
}

/// Authentication configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AuthConfig {
    /// Enable authentication
    pub enabled: bool,

    /// Authentication method (token, mtls)
    pub method: String,

    /// Token file (for token auth)
    pub token_file: Option<PathBuf>,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            method: "token".to_string(),
            token_file: None,
        }
    }
}