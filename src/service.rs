use crate::config::Config;
use crate::error::{BlixardError, Result};
use crate::storage::Storage;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::process::Command;
use tracing::{debug, info, warn};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceInfo {
    pub name: String,
    pub state: ServiceState,
    pub node: String,
    pub timestamp: DateTime<Utc>,
    pub user_service: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ServiceState {
    Running,
    Stopped,
    Failed,
    Unknown,
}

impl std::fmt::Display for ServiceState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ServiceState::Running => write!(f, "Running"),
            ServiceState::Stopped => write!(f, "Stopped"),
            ServiceState::Failed => write!(f, "Failed"),
            ServiceState::Unknown => write!(f, "Unknown"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterStatus {
    pub leader: Option<String>,
    pub nodes: Vec<NodeStatus>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeStatus {
    pub id: String,
    pub state: String,
    pub last_seen: DateTime<Utc>,
}

pub struct ServiceManager {
    config: Config,
    storage: Storage,
    user_mode: bool,
}

impl ServiceManager {
    pub async fn new(config: Config, user_mode: bool) -> Result<Self> {
        let storage = Storage::new(&config).await?;
        
        Ok(Self {
            config,
            storage,
            user_mode,
        })
    }
    
    pub async fn start_service(&self, service_name: &str) -> Result<()> {
        info!("Starting service: {} (user_mode: {})", service_name, self.user_mode);
        
        // Execute systemctl command
        let status = if self.user_mode {
            Command::new("systemctl")
                .args(&["--user", "start", service_name])
                .status()?
        } else {
            Command::new("systemctl")
                .args(&["start", service_name])
                .status()?
        };
        
        if !status.success() {
            return Err(BlixardError::ServiceManagementError(
                format!("Failed to start service: {}", service_name)
            ));
        }
        
        // Store service info
        let info = ServiceInfo {
            name: service_name.to_string(),
            state: ServiceState::Running,
            node: self.get_node_id(),
            timestamp: Utc::now(),
            user_service: self.user_mode,
        };
        
        self.storage.store_service(&info).await?;
        
        Ok(())
    }
    
    pub async fn stop_service(&self, service_name: &str) -> Result<()> {
        info!("Stopping service: {} (user_mode: {})", service_name, self.user_mode);
        
        // Execute systemctl command
        let status = if self.user_mode {
            Command::new("systemctl")
                .args(&["--user", "stop", service_name])
                .status()?
        } else {
            Command::new("systemctl")
                .args(&["stop", service_name])
                .status()?
        };
        
        if !status.success() {
            return Err(BlixardError::ServiceManagementError(
                format!("Failed to stop service: {}", service_name)
            ));
        }
        
        // Update service info
        let info = ServiceInfo {
            name: service_name.to_string(),
            state: ServiceState::Stopped,
            node: self.get_node_id(),
            timestamp: Utc::now(),
            user_service: self.user_mode,
        };
        
        self.storage.store_service(&info).await?;
        
        Ok(())
    }
    
    pub async fn list_services(&self) -> Result<HashMap<String, ServiceInfo>> {
        self.storage.list_services().await
    }
    
    pub async fn get_service_status(&self, service_name: &str) -> Result<ServiceInfo> {
        let info = self.storage.get_service(service_name).await?;
        
        if let Some(info) = info {
            // Check actual systemctl status
            let output = if self.user_mode {
                Command::new("systemctl")
                    .args(&["--user", "is-active", service_name])
                    .output()?
            } else {
                Command::new("systemctl")
                    .args(&["is-active", service_name])
                    .output()?
            };
            
            let state = match output.status.success() {
                true => ServiceState::Running,
                false => {
                    let output_str = String::from_utf8_lossy(&output.stdout);
                    match output_str.trim() {
                        "inactive" => ServiceState::Stopped,
                        "failed" => ServiceState::Failed,
                        _ => ServiceState::Unknown,
                    }
                }
            };
            
            // Update state if different
            if state != info.state {
                let updated_info = ServiceInfo {
                    state,
                    timestamp: Utc::now(),
                    ..info
                };
                self.storage.store_service(&updated_info).await?;
                Ok(updated_info)
            } else {
                Ok(info)
            }
        } else {
            Err(BlixardError::ServiceNotFound(service_name.to_string()))
        }
    }
    
    pub async fn remove_service(&self, service_name: &str) -> Result<()> {
        info!("Removing service from management: {}", service_name);
        self.storage.remove_service(service_name).await
    }
    
    pub async fn get_cluster_status(&self) -> Result<ClusterStatus> {
        // TODO: Implement actual cluster status retrieval
        Ok(ClusterStatus {
            leader: Some(self.get_node_id()),
            nodes: vec![
                NodeStatus {
                    id: self.get_node_id(),
                    state: "Active".to_string(),
                    last_seen: Utc::now(),
                }
            ],
        })
    }
    
    fn get_node_id(&self) -> String {
        self.config.node.id.clone()
            .unwrap_or_else(|| hostname::get()
                .map(|h| h.to_string_lossy().to_string())
                .unwrap_or_else(|_| "unknown".to_string()))
    }
}