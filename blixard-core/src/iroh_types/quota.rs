//! Quota management types for Iroh transport

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Set tenant quota request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetTenantQuotaRequest {
    pub tenant_id: String,
    pub max_vms: Option<u32>,
    pub max_vcpus: Option<u32>,
    pub max_memory_mb: Option<u64>,
    pub max_disk_gb: Option<u64>,
    pub max_vms_per_node: Option<u32>,
    pub priority: u8,
}

/// Set tenant quota response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetTenantQuotaResponse {
    pub success: bool,
    pub message: String,
}

/// Get tenant quota request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetTenantQuotaRequest {
    pub tenant_id: String,
}

/// Get tenant quota response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetTenantQuotaResponse {
    pub quota: Option<TenantQuotaInfo>,
}

/// List tenant quotas request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListTenantQuotasRequest {}

/// List tenant quotas response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListTenantQuotasResponse {
    pub quotas: Vec<TenantQuotaInfo>,
}

/// Get tenant usage request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetTenantUsageRequest {
    pub tenant_id: String,
}

/// Get tenant usage response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetTenantUsageResponse {
    pub usage: Option<TenantUsageInfo>,
}

/// Remove tenant quota request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoveTenantQuotaRequest {
    pub tenant_id: String,
}

/// Remove tenant quota response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoveTenantQuotaResponse {
    pub success: bool,
    pub message: String,
}

/// Tenant quota information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TenantQuotaInfo {
    pub tenant_id: String,
    pub max_vms: u32,
    pub max_vcpus: u32,
    pub max_memory_mb: u64,
    pub max_disk_gb: u64,
    pub max_vms_per_node: u32,
    pub priority: u8,
}

/// Tenant usage information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TenantUsageInfo {
    pub tenant_id: String,
    pub active_vms: u32,
    pub used_vcpus: u32,
    pub used_memory_mb: u64,
    pub used_disk_gb: u64,
    pub vms_per_node: HashMap<u64, u32>,
}