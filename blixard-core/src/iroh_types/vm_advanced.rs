//! Advanced VM operations types

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateVmWithSchedulingRequest {
    pub name: String,
    pub config_path: String,
    pub vcpus: u32,
    pub memory_mb: u32,
    pub strategy: i32, // Maps to PlacementStrategy enum
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateVmWithSchedulingResponse {
    pub success: bool,
    pub message: String,
    pub target_node_id: u64,
    pub placement_reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduleVmPlacementRequest {
    pub name: String,
    pub config_path: String,
    pub vcpus: u32,
    pub memory_mb: u32,
    pub strategy: i32, // Maps to PlacementStrategy enum
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduleVmPlacementResponse {
    pub success: bool,
    pub message: String,
    pub target_node_id: u64,
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

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NodeCapabilities {
    pub cpu_cores: u32,
    pub memory_mb: u64,
    pub disk_gb: u64,
    pub features: Vec<String>,
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