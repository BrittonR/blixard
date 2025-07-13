//! VM management request/response types

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// VM Health Monitoring Types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetVmHealthStatusRequest {
    pub vm_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetVmHealthStatusResponse {
    pub health_status: Option<crate::vm_health_types::VmHealthStatus>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddVmHealthCheckRequest {
    pub vm_name: String,
    pub health_check: crate::vm_health_types::HealthCheck,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddVmHealthCheckResponse {
    pub success: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListVmHealthChecksRequest {
    pub vm_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListVmHealthChecksResponse {
    pub health_checks: Vec<crate::vm_health_types::HealthCheck>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoveVmHealthCheckRequest {
    pub vm_name: String,
    pub check_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoveVmHealthCheckResponse {
    pub success: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToggleVmHealthMonitoringRequest {
    pub vm_name: String,
    pub enable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToggleVmHealthMonitoringResponse {
    pub success: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigureVmRecoveryPolicyRequest {
    pub vm_name: String,
    pub policy: crate::vm_auto_recovery::RecoveryPolicy,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigureVmRecoveryPolicyResponse {
    pub success: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmConfig {
    pub name: String,
    pub cpu_cores: u32,
    pub memory_mb: u32,
    pub disk_gb: u32,
    pub owner: String,
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateVmRequest {
    pub name: String,
    pub config_path: String,
    pub vcpus: u32,
    pub memory_mb: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateVmResponse {
    pub success: bool,
    pub message: String,
    pub vm_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartVmRequest {
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartVmResponse {
    pub success: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StopVmRequest {
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StopVmResponse {
    pub success: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteVmRequest {
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteVmResponse {
    pub success: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetVmStatusRequest {
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetVmStatusResponse {
    pub found: bool,
    pub vm_info: Option<VmInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListVmsRequest {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListVmsResponse {
    pub vms: Vec<VmInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmInfo {
    pub name: String,
    pub state: i32, // Maps to VmState enum
    pub node_id: u64,
    pub vcpus: u32,
    pub memory_mb: u32,
    pub ip_address: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(i32)]
pub enum VmState {
    VmStateUnknown = 0,
    VmStateCreated = 1,
    VmStateStarting = 2,
    VmStateRunning = 3,
    VmStateStopping = 4,
    VmStateStopped = 5,
    VmStateFailed = 6,
}

impl VmInfo {
    pub fn new(name: String, node_id: u64) -> Self {
        Self {
            name,
            state: VmState::VmStateUnknown as i32,
            node_id,
            vcpus: 0,
            memory_mb: 0,
            ip_address: String::new(),
        }
    }
}