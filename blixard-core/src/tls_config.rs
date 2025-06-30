//! TLS configuration module
//!
//! This module provides TLS configuration utilities for secure communication
//! between nodes in the cluster.

use crate::error::{BlixardError, BlixardResult};
use crate::config_v2::TlsConfig;
use std::path::Path;
use tonic::transport::{Certificate, Identity, ServerTlsConfig, ClientTlsConfig};
use tracing::info;

/// Helper functions for TLS configuration
pub struct TlsConfigBuilder;

impl TlsConfigBuilder {
    /// Build server TLS configuration from config
    pub async fn build_server_config(config: &TlsConfig) -> BlixardResult<Option<ServerTlsConfig>> {
        if !config.enabled {
            return Ok(None);
        }
        
        let (cert_file, key_file) = match (&config.cert_file, &config.key_file) {
            (Some(cert), Some(key)) => (cert, key),
            _ => {
                return Err(BlixardError::Configuration {
                    message: "TLS enabled but cert_file or key_file not specified".to_string(),
                });
            }
        };
        
        // Load server certificate and key
        let cert_pem = tokio::fs::read_to_string(cert_file).await
            .map_err(|e| BlixardError::Security {
                message: format!("Failed to read certificate file {:?}: {}", cert_file, e),
            })?;
        
        let key_pem = tokio::fs::read_to_string(key_file).await
            .map_err(|e| BlixardError::Security {
                message: format!("Failed to read key file {:?}: {}", key_file, e),
            })?;
        
        let identity = Identity::from_pem(cert_pem, key_pem);
        let mut tls_config = ServerTlsConfig::new().identity(identity);
        
        // Add CA certificate for client authentication if specified
        if let Some(ca_file) = &config.ca_file {
            let ca_pem = tokio::fs::read_to_string(ca_file).await
                .map_err(|e| BlixardError::Security {
                    message: format!("Failed to read CA file {:?}: {}", ca_file, e),
                })?;
            
            let ca_cert = Certificate::from_pem(ca_pem);
            tls_config = tls_config.client_ca_root(ca_cert);
            
            if config.require_client_cert {
                info!("Requiring client certificates for mTLS");
            }
        }
        
        Ok(Some(tls_config))
    }
    
    /// Build client TLS configuration from config
    pub async fn build_client_config(config: &TlsConfig) -> BlixardResult<Option<ClientTlsConfig>> {
        if !config.enabled {
            return Ok(None);
        }
        
        let mut tls_config = ClientTlsConfig::new();
        
        // Add CA certificate for server verification
        if let Some(ca_file) = &config.ca_file {
            let ca_pem = tokio::fs::read_to_string(ca_file).await
                .map_err(|e| BlixardError::Security {
                    message: format!("Failed to read CA file {:?}: {}", ca_file, e),
                })?;
            
            let ca_cert = Certificate::from_pem(ca_pem);
            tls_config = tls_config.ca_certificate(ca_cert);
        }
        
        // Add client certificate for mTLS if available
        if let (Some(cert_file), Some(key_file)) = (&config.cert_file, &config.key_file) {
            let cert_pem = tokio::fs::read_to_string(cert_file).await
                .map_err(|e| BlixardError::Security {
                    message: format!("Failed to read certificate file {:?}: {}", cert_file, e),
                })?;
            
            let key_pem = tokio::fs::read_to_string(key_file).await
                .map_err(|e| BlixardError::Security {
                    message: format!("Failed to read key file {:?}: {}", key_file, e),
                })?;
            
            let identity = Identity::from_pem(cert_pem, key_pem);
            tls_config = tls_config.identity(identity);
        }
        
        Ok(Some(tls_config))
    }
    
    /// Check if TLS files exist and are readable
    pub async fn validate_tls_files(config: &TlsConfig) -> BlixardResult<()> {
        if !config.enabled {
            return Ok(());
        }
        
        // Check certificate file
        if let Some(cert_file) = &config.cert_file {
            if !Path::new(cert_file).exists() {
                return Err(BlixardError::Security {
                    message: format!("Certificate file does not exist: {:?}", cert_file),
                });
            }
        }
        
        // Check key file
        if let Some(key_file) = &config.key_file {
            if !Path::new(key_file).exists() {
                return Err(BlixardError::Security {
                    message: format!("Key file does not exist: {:?}", key_file),
                });
            }
        }
        
        // Check CA file
        if let Some(ca_file) = &config.ca_file {
            if !Path::new(ca_file).exists() {
                return Err(BlixardError::Security {
                    message: format!("CA file does not exist: {:?}", ca_file),
                });
            }
        }
        
        Ok(())
    }
}