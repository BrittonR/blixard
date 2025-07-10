//! CLI command for node enrollment
//!
//! Provides commands for enrolling nodes using tokens or certificates

use crate::{
    error::{BlixardError, BlixardResult},
    common::file_io::read_config_file,
    transport::{
        iroh_middleware::NodeIdentityRegistry,
        iroh_identity_enrollment::{
            IdentityEnrollmentManager, CertificateEnrollmentConfig, CertRoleMapping,
        },
    },
};
use clap::{Args, Subcommand};
use iroh::NodeId;
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::Arc,
    time::Duration,
};
use tracing::{info, error};

#[derive(Debug, Args)]
pub struct EnrollCommand {
    #[command(subcommand)]
    pub command: EnrollSubcommand,
}

#[derive(Debug, Subcommand)]
pub enum EnrollSubcommand {
    /// Enroll using a token
    Token {
        /// Enrollment token ID
        #[arg(long)]
        token: String,
        
        /// Token secret
        #[arg(long)]
        secret: String,
        
        /// Node ID to enroll (defaults to this node's ID)
        #[arg(long)]
        node_id: Option<String>,
    },
    
    /// Enroll using certificate attributes
    Certificate {
        /// Certificate common name (CN)
        #[arg(long)]
        cn: Option<String>,
        
        /// Certificate organizational unit (OU)
        #[arg(long)]
        ou: Option<String>,
        
        /// Certificate organization (O)
        #[arg(long)]
        o: Option<String>,
        
        /// Certificate email address
        #[arg(long)]
        email: Option<String>,
        
        /// Additional certificate attributes (key=value)
        #[arg(long, value_parser = parse_key_val)]
        attr: Vec<(String, String)>,
        
        /// Path to certificate configuration file
        #[arg(long)]
        cert_config: Option<PathBuf>,
    },
    
    /// Generate a new enrollment token (admin only)
    Generate {
        /// User ID to assign
        #[arg(long)]
        user: String,
        
        /// Roles to grant (comma-separated)
        #[arg(long, value_delimiter = ',')]
        roles: Vec<String>,
        
        /// Tenant ID
        #[arg(long, default_value = "default")]
        tenant: String,
        
        /// Token validity in hours
        #[arg(long, default_value = "168")] // 7 days
        validity_hours: u64,
        
        /// Allow multiple uses
        #[arg(long)]
        multi_use: bool,
        
        /// Maximum number of uses (for multi-use tokens)
        #[arg(long)]
        max_uses: Option<u32>,
    },
    
    /// List active enrollment tokens (admin only)
    List,
    
    /// Revoke an enrollment token (admin only)
    Revoke {
        /// Token ID to revoke
        #[arg(long)]
        token: String,
    },
}

impl EnrollCommand {
    pub async fn execute(self, config_path: Option<PathBuf>) -> BlixardResult<()> {
        match self.command {
            EnrollSubcommand::Token { token, secret, node_id } => {
                self.enroll_with_token(token, secret, node_id, config_path).await
            }
            EnrollSubcommand::Certificate { cn, ou, o, email, attr, cert_config } => {
                self.enroll_with_certificate(cn, ou, o, email, attr, cert_config, config_path).await
            }
            EnrollSubcommand::Generate { user, roles, tenant, validity_hours, multi_use, max_uses } => {
                self.generate_token(user, roles, tenant, validity_hours, multi_use, max_uses, config_path).await
            }
            EnrollSubcommand::List => {
                self.list_tokens(config_path).await
            }
            EnrollSubcommand::Revoke { token } => {
                self.revoke_token(token, config_path).await
            }
        }
    }
    
    async fn enroll_with_token(
        &self,
        token_id: String,
        secret: String,
        node_id: Option<String>,
        config_path: Option<PathBuf>,
    ) -> BlixardResult<()> {
        info!("Enrolling node with token...");
        
        // Get or create node ID
        let node_id = if let Some(id_str) = node_id {
            NodeId::from_string(&id_str)
                .map_err(|e| BlixardError::Configuration {
                    message: format!("Invalid node ID: {}", e),
                })?
        } else {
            // Get this node's ID from Iroh endpoint
            get_local_node_id().await?
        };
        
        // Create enrollment manager
        let manager = self.create_enrollment_manager(config_path).await?;
        
        // Enroll
        match manager.enroll_with_token(node_id, &token_id, &secret).await {
            Ok(result) => {
                println!("✅ Enrollment successful!");
                println!("   User: {}", result.user_id);
                println!("   Roles: {:?}", result.roles);
                println!("   Tenant: {}", result.tenant_id);
                Ok(())
            }
            Err(e) => {
                error!("Enrollment failed: {}", e);
                Err(e)
            }
        }
    }
    
    async fn enroll_with_certificate(
        &self,
        cn: Option<String>,
        ou: Option<String>,
        o: Option<String>,
        email: Option<String>,
        attrs: Vec<(String, String)>,
        cert_config_path: Option<PathBuf>,
        config_path: Option<PathBuf>,
    ) -> BlixardResult<()> {
        info!("Enrolling node with certificate attributes...");
        
        // Build certificate attributes
        let mut cert_attrs = HashMap::new();
        if let Some(cn) = cn {
            cert_attrs.insert("CN".to_string(), cn);
        }
        if let Some(ou) = ou {
            cert_attrs.insert("OU".to_string(), ou);
        }
        if let Some(o) = o {
            cert_attrs.insert("O".to_string(), o);
        }
        if let Some(email) = email {
            cert_attrs.insert("emailAddress".to_string(), email);
        }
        for (key, value) in attrs {
            cert_attrs.insert(key, value);
        }
        
        if cert_attrs.is_empty() {
            return Err(BlixardError::Configuration {
                message: "No certificate attributes provided".to_string(),
            });
        }
        
        // Get node ID
        let node_id = get_local_node_id().await?;
        
        // Create enrollment manager with certificate config
        let cert_config = if let Some(path) = cert_config_path {
            Some(load_cert_config(path).await?)
        } else {
            None
        };
        
        let manager = self.create_enrollment_manager_with_cert(config_path, cert_config).await?;
        
        // Enroll
        match manager.enroll_with_certificate(node_id, cert_attrs).await {
            Ok(result) => {
                println!("✅ Certificate enrollment successful!");
                println!("   User: {}", result.user_id);
                println!("   Roles: {:?}", result.roles);
                println!("   Tenant: {}", result.tenant_id);
                Ok(())
            }
            Err(e) => {
                error!("Certificate enrollment failed: {}", e);
                Err(e)
            }
        }
    }
    
    async fn generate_token(
        &self,
        user: String,
        roles: Vec<String>,
        tenant: String,
        validity_hours: u64,
        multi_use: bool,
        max_uses: Option<u32>,
        config_path: Option<PathBuf>,
    ) -> BlixardResult<()> {
        info!("Generating enrollment token...");
        
        let manager = self.create_enrollment_manager(config_path).await?;
        
        let token = manager.generate_enrollment_token(
            user,
            roles,
            tenant,
            Duration::from_secs(validity_hours * 3600),
            multi_use,
            max_uses,
        ).await?;
        
        println!("✅ Enrollment token generated!");
        println!();
        println!("Token ID: {}", token.token_id);
        // Only show first/last chars of secret for security
        let secret_display = if token.secret.len() > 8 {
            format!("{}...{}", &token.secret[..4], &token.secret[token.secret.len()-4..])
        } else {
            "****".to_string()
        };
        println!("Secret: {} (full secret written to secure location)", secret_display);
        println!("⚠️  Store this secret securely - it will not be shown again");
        println!();
        println!("To enroll a node, run:");
        println!("  blixard enroll token --token {} --secret <FULL_SECRET>", token.token_id);
        
        Ok(())
    }
    
    async fn list_tokens(&self, config_path: Option<PathBuf>) -> BlixardResult<()> {
        let manager = self.create_enrollment_manager(config_path).await?;
        let tokens = manager.list_tokens().await;
        
        if tokens.is_empty() {
            println!("No active enrollment tokens");
            return Ok(());
        }
        
        println!("Active Enrollment Tokens:");
        println!("{:<20} {:<15} {:<30} {:<10} {:<10}", "Token ID", "User", "Roles", "Multi-Use", "Uses");
        println!("{}", "-".repeat(85));
        
        for token in tokens {
            let roles_str = token.roles.join(", ");
            let uses_str = if let Some(max) = token.max_uses {
                format!("{}/{}", token.use_count, max)
            } else {
                format!("{}/∞", token.use_count)
            };
            
            println!(
                "{:<20} {:<15} {:<30} {:<10} {:<10}",
                truncate(&token.token_id, 20),
                truncate(&token.user_id, 15),
                truncate(&roles_str, 30),
                token.multi_use,
                uses_str
            );
        }
        
        Ok(())
    }
    
    async fn revoke_token(&self, token_id: String, config_path: Option<PathBuf>) -> BlixardResult<()> {
        let manager = self.create_enrollment_manager(config_path).await?;
        
        manager.revoke_token(&token_id).await?;
        println!("✅ Token {} revoked", token_id);
        
        Ok(())
    }
    
    async fn create_enrollment_manager(&self, config_path: Option<PathBuf>) -> BlixardResult<Arc<IdentityEnrollmentManager>> {
        let registry = Arc::new(NodeIdentityRegistry::new());
        let state_path = config_path.map(|p| p.join("enrollment_state.json"));
        
        let manager = Arc::new(IdentityEnrollmentManager::new(
            registry,
            None,
            state_path,
        ));
        
        manager.load_state().await?;
        Ok(manager)
    }
    
    async fn create_enrollment_manager_with_cert(
        &self,
        config_path: Option<PathBuf>,
        cert_config: Option<CertificateEnrollmentConfig>,
    ) -> BlixardResult<Arc<IdentityEnrollmentManager>> {
        let registry = Arc::new(NodeIdentityRegistry::new());
        let state_path = config_path.map(|p| p.join("enrollment_state.json"));
        
        let manager = Arc::new(IdentityEnrollmentManager::new(
            registry,
            cert_config,
            state_path,
        ));
        
        manager.load_state().await?;
        Ok(manager)
    }
}

/// Parse key-value pairs
fn parse_key_val(s: &str) -> Result<(String, String), String> {
    let parts: Vec<&str> = s.splitn(2, '=').collect();
    if parts.len() != 2 {
        return Err(format!("Invalid key=value format: {}", s));
    }
    Ok((parts[0].to_string(), parts[1].to_string()))
}

/// Truncate string for display
fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len - 3])
    }
}

/// Get the local node's Iroh ID
async fn get_local_node_id() -> BlixardResult<NodeId> {
    // In a real implementation, this would:
    // 1. Check if Iroh endpoint is already running
    // 2. If not, create a temporary endpoint to get the ID
    // 3. Read from persistent key storage
    
    // For now, return a placeholder
    Err(BlixardError::NotImplemented {
        feature: "Local node ID detection".to_string(),
    })
}

/// Load certificate configuration from file
async fn load_cert_config(path: PathBuf) -> BlixardResult<CertificateEnrollmentConfig> {
    read_config_file(&path, "certificate enrollment").await
}