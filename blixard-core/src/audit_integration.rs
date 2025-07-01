//! Integration of audit logging with core Blixard components
//!
//! This module provides helper functions and middleware to automatically
//! log security-relevant events throughout the system.

use std::sync::Arc;
// Removed tonic imports - using Iroh transport
use crate::audit_log::{
    AuditLogger, AuditEvent, AuditCategory, AuditSeverity, AuditActor,
    AuditResource, AuditOutcome, log_authentication_failure, log_authorization,
};
use crate::error::BlixardResult;
use crate::types::{VmCommand, VmStatus};
use crate::iroh_types;
use chrono::Utc;
use tracing::{info, warn};

/// Audit context for request processing
#[derive(Clone)]
pub struct AuditContext {
    logger: Arc<AuditLogger>,
    node_id: u64,
}

impl AuditContext {
    pub fn new(logger: Arc<AuditLogger>, node_id: u64) -> Self {
        Self { logger, node_id }
    }
    
    /// Extract actor information from request context
    pub fn extract_actor(&self, headers: &std::collections::HashMap<String, String>) -> AuditActor {
        // Check for user authentication token
        if let Some(user_id) = headers.get("user-id") {
            let user_name = headers.get("user-name")
                .unwrap_or(&"unknown".to_string())
                .clone();
            
            let roles = headers.get("user-roles")
                .map(|s| s.split(',').map(|r| r.trim().to_string()).collect())
                .unwrap_or_default();
            
            AuditActor::User {
                id: user_id.clone(),
                name: user_name,
                roles,
            }
        } else if let Some(service) = headers.get("service-name") {
            // Service authentication
            let api_key_id = headers.get("api-key-id").cloned();
            
            AuditActor::Service {
                name: service.clone(),
                api_key_id,
            }
        } else {
            // System component (internal calls)
            AuditActor::System {
                component: "iroh-server".to_string(),
                node_id: self.node_id,
            }
        }
    }
    
    /// Extract source IP from request context
    pub fn extract_source_ip(&self, remote_addr: Option<std::net::SocketAddr>, headers: &std::collections::HashMap<String, String>) -> Option<String> {
        remote_addr
            .map(|addr| addr.to_string())
            .or_else(|| {
                headers.get("x-forwarded-for").cloned()
            })
    }
    
    /// Log a gRPC method invocation
    pub async fn log_grpc_call(
        &self,
        method: &str,
        actor: &AuditActor,
        source_ip: Option<String>,
        success: bool,
        error: Option<&str>,
    ) -> BlixardResult<()> {
        let event = AuditEvent {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            category: AuditCategory::Access,
            severity: if success { AuditSeverity::Info } else { AuditSeverity::Warning },
            event_type: format!("grpc.{}", if success { "success" } else { "failed" }),
            actor: actor.clone(),
            resource: None,
            outcome: if success {
                AuditOutcome::Success
            } else {
                AuditOutcome::Failure {
                    reason: error.unwrap_or("unknown").to_string(),
                }
            },
            details: serde_json::json!({
                "method": method,
            }),
            correlation_id: None,
            source_ip,
            node_id: self.node_id,
        };
        
        self.logger.log(event).await
    }
    
    /// Log a VM command execution
    pub async fn log_vm_command(
        &self,
        command: &VmCommand,
        actor: &AuditActor,
        success: bool,
        error: Option<String>,
    ) -> BlixardResult<()> {
        let (event_type, resource, details) = match command {
            VmCommand::Create { config, node_id } => (
                "created",
                AuditResource::Vm {
                    name: config.name.clone(),
                    tenant_id: config.tenant_id.clone(),
                },
                serde_json::json!({
                    "vcpus": config.vcpus,
                    "memory": config.memory,
                    "node_id": node_id,
                    "priority": config.priority,
                    "preemptible": config.preemptible,
                }),
            ),
            VmCommand::Start { name } => (
                "started",
                AuditResource::Vm {
                    name: name.clone(),
                    tenant_id: "unknown".to_string(), // Would need to look up
                },
                serde_json::json!({}),
            ),
            VmCommand::Stop { name } => (
                "stopped",
                AuditResource::Vm {
                    name: name.clone(),
                    tenant_id: "unknown".to_string(),
                },
                serde_json::json!({}),
            ),
            VmCommand::Delete { name } => (
                "deleted",
                AuditResource::Vm {
                    name: name.clone(),
                    tenant_id: "unknown".to_string(),
                },
                serde_json::json!({}),
            ),
            VmCommand::UpdateStatus { name, status, .. } => (
                "status_updated",
                AuditResource::Vm {
                    name: name.clone(),
                    tenant_id: "unknown".to_string(),
                },
                serde_json::json!({
                    "new_status": format!("{:?}", status),
                }),
            ),
            VmCommand::Migrate { task } => (
                "migrated",
                AuditResource::Vm {
                    name: task.vm_name.clone(),
                    tenant_id: "unknown".to_string(),
                },
                serde_json::json!({
                    "source_node": task.source_node_id,
                    "target_node": task.target_node_id,
                    "live_migration": task.live_migration,
                }),
            ),
        };
        
        let event = AuditEvent {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            category: AuditCategory::VmLifecycle,
            severity: if success { AuditSeverity::Info } else { AuditSeverity::Error },
            event_type: format!("vm.{}", event_type),
            actor: actor.clone(),
            resource: Some(resource),
            outcome: if success {
                AuditOutcome::Success
            } else {
                AuditOutcome::Failure {
                    reason: error.unwrap_or_else(|| "operation failed".to_string()),
                }
            },
            details,
            correlation_id: None,
            source_ip: None,
            node_id: self.node_id,
        };
        
        self.logger.log(event).await
    }
    
    /// Log a cluster membership change
    pub async fn log_cluster_membership(
        &self,
        event_type: &str,
        node_id: u64,
        node_address: &str,
        actor: &AuditActor,
        success: bool,
        details: serde_json::Value,
    ) -> BlixardResult<()> {
        let event = AuditEvent {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            category: AuditCategory::ClusterMembership,
            severity: if success { AuditSeverity::Warning } else { AuditSeverity::Error },
            event_type: format!("cluster.{}", event_type),
            actor: actor.clone(),
            resource: Some(AuditResource::Node {
                id: node_id,
                address: node_address.to_string(),
            }),
            outcome: if success {
                AuditOutcome::Success
            } else {
                AuditOutcome::Failure {
                    reason: "operation failed".to_string(),
                }
            },
            details,
            correlation_id: None,
            source_ip: None,
            node_id: self.node_id,
        };
        
        self.logger.log(event).await
    }
    
    /// Log a resource scheduling decision
    pub async fn log_scheduling_decision(
        &self,
        vm_name: &str,
        selected_node: Option<u64>,
        strategy: &str,
        constraints: serde_json::Value,
    ) -> BlixardResult<()> {
        let event = AuditEvent {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            category: AuditCategory::ResourceManagement,
            severity: AuditSeverity::Info,
            event_type: "scheduling.decision".to_string(),
            actor: AuditActor::System {
                component: "scheduler".to_string(),
                node_id: self.node_id,
            },
            resource: Some(AuditResource::Vm {
                name: vm_name.to_string(),
                tenant_id: "unknown".to_string(),
            }),
            outcome: if selected_node.is_some() {
                AuditOutcome::Success
            } else {
                AuditOutcome::Failure {
                    reason: "no suitable node found".to_string(),
                }
            },
            details: serde_json::json!({
                "selected_node": selected_node,
                "strategy": strategy,
                "constraints": constraints,
            }),
            correlation_id: None,
            source_ip: None,
            node_id: self.node_id,
        };
        
        self.logger.log(event).await
    }
    
    /// Log a security policy change
    pub async fn log_policy_change(
        &self,
        policy_name: &str,
        policy_type: &str,
        action: &str,
        actor: &AuditActor,
        details: serde_json::Value,
    ) -> BlixardResult<()> {
        let event = AuditEvent {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            category: AuditCategory::SecurityPolicy,
            severity: AuditSeverity::Critical,
            event_type: format!("policy.{}", action),
            actor: actor.clone(),
            resource: Some(AuditResource::Policy {
                name: policy_name.to_string(),
                policy_type: policy_type.to_string(),
            }),
            outcome: AuditOutcome::Success,
            details,
            correlation_id: None,
            source_ip: None,
            node_id: self.node_id,
        };
        
        self.logger.log(event).await
    }
}

/// Middleware for automatic audit logging of gRPC requests
pub struct AuditInterceptor {
    context: AuditContext,
}

impl AuditInterceptor {
    pub fn new(context: AuditContext) -> Self {
        Self { context }
    }
    
    /// Process a gRPC request with audit logging
    pub async fn intercept<T, F, R>(
        &self,
        request: Request<T>,
        method: &str,
        handler: F,
    ) -> Result<tonic::Response<R>, Status>
    where
        T: std::fmt::Debug,
        F: std::future::Future<Output = Result<tonic::Response<R>, Status>>,
    {
        let actor = self.context.extract_actor(&request);
        let source_ip = self.context.extract_source_ip(&request);
        
        // Log the request
        info!("Processing {} request from {:?}", method, actor);
        
        // Execute the handler
        let result = handler.await;
        
        // Log the outcome
        match &result {
            Ok(_) => {
                self.context.log_grpc_call(method, &actor, source_ip, true, None)
                    .await
                    .unwrap_or_else(|e| warn!("Failed to log audit event: {}", e));
            }
            Err(status) => {
                self.context.log_grpc_call(
                    method,
                    &actor,
                    source_ip,
                    false,
                    Some(&status.message()),
                ).await
                .unwrap_or_else(|e| warn!("Failed to log audit event: {}", e));
            }
        }
        
        result
    }
}

/// Helper trait for audit-aware operations
#[async_trait::async_trait]
pub trait AuditAware {
    /// Get the audit context
    fn audit_context(&self) -> &AuditContext;
    
    /// Log an operation with audit context
    async fn audit_operation<F, R>(
        &self,
        operation: &str,
        category: AuditCategory,
        resource: Option<AuditResource>,
        f: F,
    ) -> BlixardResult<R>
    where
        F: std::future::Future<Output = BlixardResult<R>> + Send,
        R: Send,
    {
        let start = std::time::Instant::now();
        let result = f.await;
        let duration = start.elapsed();
        
        let (success, error) = match &result {
            Ok(_) => (true, None),
            Err(e) => (false, Some(format!("{:?}", e))),
        };
        
        let event = AuditEvent {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            category,
            severity: if success { AuditSeverity::Info } else { AuditSeverity::Error },
            event_type: operation.to_string(),
            actor: AuditActor::System {
                component: "blixard-core".to_string(),
                node_id: self.audit_context().node_id,
            },
            resource,
            outcome: if success {
                AuditOutcome::Success
            } else {
                AuditOutcome::Failure {
                    reason: error.unwrap_or_else(|| "unknown".to_string()),
                }
            },
            details: serde_json::json!({
                "duration_ms": duration.as_millis(),
            }),
            correlation_id: None,
            source_ip: None,
            node_id: self.audit_context().node_id,
        };
        
        self.audit_context().logger.log(event).await?;
        
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audit_log::AuditConfig;
    use tempfile::TempDir;
    // Removed tonic metadata imports
    
    #[tokio::test]
    async fn test_audit_context_actor_extraction() {
        let temp_dir = TempDir::new().unwrap();
        let config = AuditConfig {
            log_dir: temp_dir.path().to_path_buf(),
            ..Default::default()
        };
        
        let logger = Arc::new(AuditLogger::new(config, 1).await.unwrap());
        let context = AuditContext::new(logger, 1);
        
        // Test user actor extraction
        let mut request = Request::new(());
        request.metadata_mut().insert("user-id", "alice".parse().unwrap());
        request.metadata_mut().insert("user-name", "Alice Smith".parse().unwrap());
        request.metadata_mut().insert("user-roles", "admin,developer".parse().unwrap());
        
        match context.extract_actor(&request) {
            AuditActor::User { id, name, roles } => {
                assert_eq!(id, "alice");
                assert_eq!(name, "Alice Smith");
                assert_eq!(roles, vec!["admin", "developer"]);
            }
            _ => panic!("Expected User actor"),
        }
        
        // Test service actor extraction
        let mut request = Request::new(());
        request.metadata_mut().insert("service-name", "monitoring-service".parse().unwrap());
        request.metadata_mut().insert("api-key-id", "key-123".parse().unwrap());
        
        match context.extract_actor(&request) {
            AuditActor::Service { name, api_key_id } => {
                assert_eq!(name, "monitoring-service");
                assert_eq!(api_key_id, Some("key-123".to_string()));
            }
            _ => panic!("Expected Service actor"),
        }
        
        // Test system actor (default)
        let request = Request::new(());
        match context.extract_actor(&request) {
            AuditActor::System { component, node_id } => {
                assert_eq!(component, "grpc-server");
                assert_eq!(node_id, 1);
            }
            _ => panic!("Expected System actor"),
        }
    }
}