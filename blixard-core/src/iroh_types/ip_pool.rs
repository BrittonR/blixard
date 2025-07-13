//! IP pool management request/response types

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateIpPoolRequest {
    pub pool_config: crate::ip_pool::IpPoolConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateIpPoolResponse {
    pub success: bool,
    pub message: String,
    pub pool_id: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListIpPoolsRequest {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListIpPoolsResponse {
    pub pools: Vec<crate::ip_pool::IpPoolState>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetIpPoolRequest {
    pub pool_id: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetIpPoolResponse {
    pub pool: Option<crate::ip_pool::IpPoolState>,
    pub stats: Option<crate::ip_pool_manager::PoolStats>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteIpPoolRequest {
    pub pool_id: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteIpPoolResponse {
    pub success: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetIpPoolEnabledRequest {
    pub pool_id: u64,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetIpPoolEnabledResponse {
    pub success: bool,
    pub message: String,
}