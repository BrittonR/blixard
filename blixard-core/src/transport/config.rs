//! Transport configuration types

use serde::{Deserialize, Serialize};
use std::net::SocketAddr;

/// Transport configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "mode")]
pub enum TransportConfig {
    /// gRPC only mode
    #[serde(rename = "grpc")]
    Grpc(GrpcConfig),
    
    /// Iroh only mode
    #[serde(rename = "iroh")]
    Iroh(IrohConfig),
    
    /// Dual transport mode for migration
    #[serde(rename = "dual")]
    Dual {
        grpc_config: GrpcConfig,
        iroh_config: IrohConfig,
        strategy: MigrationStrategy,
    },
}

/// gRPC transport configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GrpcConfig {
    /// Address to bind the gRPC server
    pub bind_address: SocketAddr,
    
    /// Maximum message size in bytes
    #[serde(default = "default_max_message_size")]
    pub max_message_size: usize,
    
    /// Enable TLS
    #[serde(default)]
    pub tls_enabled: bool,
}

/// Iroh transport configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct IrohConfig {
    /// Enable Iroh transport
    #[serde(default = "default_true")]
    pub enabled: bool,
    
    /// Home relay server URL
    #[serde(default = "default_home_relay")]
    pub home_relay: String,
    
    /// Discovery port (0 for random)
    #[serde(default)]
    pub discovery_port: u16,
    
    /// Custom ALPN protocols
    #[serde(default)]
    pub alpn_protocols: Vec<String>,
}

/// Migration strategy configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MigrationStrategy {
    /// Services to prefer Iroh for
    #[serde(default)]
    pub prefer_iroh_for: std::collections::HashSet<ServiceType>,
    
    /// Raft-specific transport preference
    #[serde(default)]
    pub raft_transport: RaftTransportPreference,
    
    /// Fallback to gRPC if Iroh fails
    #[serde(default = "default_true")]
    pub fallback_to_grpc: bool,
}

/// Service types that can be migrated
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceType {
    Health,
    Status,
    Monitoring,
    VmOps,
    Scheduling,
    Migration,
    Raft,
}

/// Raft transport preference
#[derive(Debug, Clone, Copy, PartialEq, Deserialize, Serialize)]
#[serde(tag = "type")]
pub enum RaftTransportPreference {
    #[serde(rename = "always_grpc")]
    AlwaysGrpc,
    
    #[serde(rename = "always_iroh")]
    AlwaysIroh,
    
    #[serde(rename = "adaptive")]
    Adaptive { 
        /// Latency threshold in milliseconds
        latency_threshold_ms: f64 
    },
}

impl Default for RaftTransportPreference {
    fn default() -> Self {
        RaftTransportPreference::AlwaysIroh
    }
}

impl Default for TransportConfig {
    fn default() -> Self {
        TransportConfig::Iroh(IrohConfig::default())
    }
}

fn default_true() -> bool {
    true
}

fn default_home_relay() -> String {
    "https://relay.iroh.network".to_string()
}

fn default_max_message_size() -> usize {
    4 * 1024 * 1024 // 4MB
}

impl Default for GrpcConfig {
    fn default() -> Self {
        Self {
            bind_address: "0.0.0.0:7001".parse().unwrap(),
            max_message_size: default_max_message_size(),
            tls_enabled: false,
        }
    }
}

impl Default for IrohConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            home_relay: default_home_relay(),
            discovery_port: 0,
            alpn_protocols: vec![],
        }
    }
}

impl Default for MigrationStrategy {
    fn default() -> Self {
        Self {
            prefer_iroh_for: std::collections::HashSet::new(),
            raft_transport: RaftTransportPreference::default(),
            fallback_to_grpc: true,
        }
    }
}