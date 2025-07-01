//! Native Rust types for Iroh transport, replacing Protocol Buffer definitions
//!
//! This module contains all the request/response types and data structures
//! previously defined in proto files, now as native Rust types for use
//! with Iroh's transport layer.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// Simple Response wrapper to replace tonic::Response
#[derive(Debug, Clone)]
pub struct Response<T> {
    inner: T,
}

impl<T> Response<T> {
    pub fn new(inner: T) -> Self {
        Self { inner }
    }
    
    pub fn into_inner(self) -> T {
        self.inner
    }
    
    pub fn get_ref(&self) -> &T {
        &self.inner
    }
}

// ============================================================================
// Cluster Management Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterStatusRequest {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterStatusResponse {
    pub leader_id: u64,
    pub nodes: Vec<NodeInfo>,
    pub term: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JoinRequest {
    pub node_id: u64,
    pub bind_address: String,
    /// Optional P2P node address information for the joining node
    pub p2p_node_addr: Option<iroh::NodeAddr>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JoinResponse {
    pub success: bool,
    pub message: String,
    pub peers: Vec<NodeInfo>,
    pub voters: Vec<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeaveRequest {
    pub node_id: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeaveResponse {
    pub success: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeInfo {
    pub id: u64,
    pub address: String,
    pub state: i32,  // Maps to NodeState enum
    pub p2p_node_id: String,
    pub p2p_addresses: Vec<String>,
    pub p2p_relay_url: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(i32)]
pub enum NodeState {
    NodeStateUnknown = 0,
    NodeStateFollower = 1,
    NodeStateCandidate = 2,
    NodeStateLeader = 3,
}

// ============================================================================
// VM Management Types
// ============================================================================

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
    pub state: i32,  // Maps to VmState enum
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

// ============================================================================
// Task Management Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRequest {
    pub task_id: String,
    pub task_type: String,
    pub payload: Vec<u8>,
    pub resource_requirements: TaskResourceRequirements,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskResourceRequirements {
    pub cpu_cores: u32,
    pub memory_mb: u64,
    pub disk_gb: u64,
    pub gpu: bool,
    pub features: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskResponse {
    pub success: bool,
    pub message: String,
    pub task_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskStatusRequest {
    pub task_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskStatusResponse {
    pub found: bool,
    pub task_info: Option<TaskInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskInfo {
    pub task_id: String,
    pub status: i32,  // Maps to TaskStatus enum
    pub worker_id: String,
    pub result: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskSchedulingRequest {
    pub vm_name: String,
    pub placement_strategy: String,
    pub required_features: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskSchedulingResponse {
    pub success: bool,
    pub message: String,
    pub worker_id: u64,
    pub placement_score: f64,
}

// ============================================================================
// Health Check Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckRequest {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckResponse {
    pub healthy: bool,
    pub message: String,
}

// ============================================================================
// P2P Status Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetP2pStatusRequest {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetP2pStatusResponse {
    pub enabled: bool,
    pub node_id: u64,
    pub node_id_base64: String,
    pub direct_addresses: Vec<String>,
    pub relay_url: String,
}

// ============================================================================
// Raft Message Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RaftMessageRequest {
    pub message: Vec<u8>,  // Serialized Raft message
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RaftMessageResponse {
    pub success: bool,
}

// ============================================================================
// Advanced VM Operations
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateVmWithSchedulingRequest {
    pub name: String,
    pub config_path: String,
    pub vcpus: u32,
    pub memory_mb: u32,
    pub strategy: i32,  // Maps to PlacementStrategy enum
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateVmWithSchedulingResponse {
    pub success: bool,
    pub message: String,
    pub selected_node_id: u64,
    pub placement_reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduleVmPlacementRequest {
    pub name: String,
    pub config_path: String,
    pub vcpus: u32,
    pub memory_mb: u32,
    pub strategy: i32,  // Maps to PlacementStrategy enum
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduleVmPlacementResponse {
    pub success: bool,
    pub message: String,
    pub selected_node_id: u64,
    pub placement_reason: String,
    pub alternative_nodes: Vec<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrateVmRequest {
    pub vm_name: String,
    pub target_node_id: u64,
    pub live_migration: bool,
    pub force: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrateVmResponse {
    pub success: bool,
    pub message: String,
    pub source_node_id: u64,
    pub target_node_id: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterResourceSummaryRequest {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterResourceSummaryResponse {
    pub success: bool,
    pub message: String,
    pub summary: Option<ClusterResourceSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterResourceSummary {
    pub total_nodes: u32,
    pub total_vcpus: u32,
    pub used_vcpus: u32,
    pub total_memory_mb: u64,
    pub used_memory_mb: u64,
    pub total_disk_gb: u64,
    pub used_disk_gb: u64,
    pub total_running_vms: u32,
    pub nodes: Vec<NodeResourceSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeResourceSummary {
    pub node_id: u64,
    pub used_vcpus: u32,
    pub used_memory_mb: u64,
    pub used_disk_gb: u64,
    pub running_vms: u32,
    pub capabilities: Option<NodeCapabilities>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeCapabilities {
    pub cpu_cores: u32,
    pub memory_mb: u64,
    pub disk_gb: u64,
    pub features: Vec<String>,
}

// ============================================================================
// Security Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnrollRequest {
    pub node_id: u64,
    pub csr_pem: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnrollResponse {
    pub success: bool,
    pub message: String,
    pub certificate_pem: String,
    pub ca_certificate_pem: String,
}

// ============================================================================
// Type Conversions
// ============================================================================

impl From<i32> for NodeState {
    fn from(value: i32) -> Self {
        match value {
            1 => NodeState::NodeStateFollower,
            2 => NodeState::NodeStateCandidate,
            3 => NodeState::NodeStateLeader,
            _ => NodeState::NodeStateUnknown,
        }
    }
}

impl From<NodeState> for i32 {
    fn from(state: NodeState) -> Self {
        state as i32
    }
}

impl From<i32> for VmState {
    fn from(value: i32) -> Self {
        match value {
            1 => VmState::VmStateCreated,
            2 => VmState::VmStateStarting,
            3 => VmState::VmStateRunning,
            4 => VmState::VmStateStopping,
            5 => VmState::VmStateStopped,
            6 => VmState::VmStateFailed,
            _ => VmState::VmStateUnknown,
        }
    }
}

impl From<VmState> for i32 {
    fn from(state: VmState) -> Self {
        state as i32
    }
}

// P2P VM image sharing types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShareVmImageRequest {
    pub image_path: String,
    pub image_name: String,
    pub description: String,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShareVmImageResponse {
    pub success: bool,
    pub message: String,
    pub ticket: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetVmImageRequest {
    pub ticket: String,
    pub target_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetVmImageResponse {
    pub success: bool,
    pub message: String,
    pub bytes_received: u64,
    pub total_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListP2pImagesRequest {
    pub filter_tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListP2pImagesResponse {
    pub images: Vec<P2pImageInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct P2pImageInfo {
    pub name: String,
    pub description: String,
    pub size_bytes: u64,
    pub created_at: i64,
    pub tags: Vec<String>,
    pub ticket: String,
}