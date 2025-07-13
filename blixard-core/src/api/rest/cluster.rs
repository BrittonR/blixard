//! Cluster management REST endpoints

use axum::{
    extract::{Extension, Path, Query},
    response::Json,
};
use utoipa::path;

use crate::api::schemas::{
    JoinClusterRequest, JoinClusterResponse, LeaveClusterRequest, LeaveClusterResponse,
    ClusterStatusResponse, NodeInfo, NodeStatusDto, NodeCapabilities,
    ClusterResourceSummary, ClusterResourceUtilization, ListNodesParams,
    PaginatedResponse, ResourceMetadata,
};
use super::{AppState, json_error, handle_blixard_error, extract_pagination};

/// Get cluster status
#[utoipa::path(
    get,
    path = "/cluster/status",
    tag = "cluster",
    summary = "Get cluster status",
    description = "Returns comprehensive cluster status including nodes and resource utilization",
    responses(
        (status = 200, description = "Cluster status retrieved successfully", body = ClusterStatusResponse),
        (status = 503, description = "Cluster not available", body = ErrorResponse)
    ),
    security(
        ("bearer_auth" = []),
        ("api_key" = [])
    )
)]
pub async fn get_cluster_status(
    Extension(app_state): Extension<AppState>,
) -> Result<Json<ClusterStatusResponse>, (axum::http::StatusCode, Json<crate::api::schemas::ErrorResponse>)> {
    let shared_state = &app_state.shared_state;
    
    match shared_state.get_cluster_status().await {
        Ok(cluster_status) => {
            // Convert internal cluster status to API response
            let api_cluster_status = ClusterStatusResponse {
                cluster_id: format!("cluster_{}", cluster_status.leader_node_id.unwrap_or(0)),
                total_nodes: cluster_status.nodes.len() as u32,
                active_nodes: cluster_status.nodes.iter()
                    .filter(|n| matches!(n.status, crate::types::NodeState::Active))
                    .count() as u32,
                leader_node_id: cluster_status.leader_node_id,
                node_id: shared_state.get_id(),
                node_status: NodeStatusDto::from(shared_state.get_node_state()),
                is_leader: cluster_status.is_leader,
                raft_term: cluster_status.raft_term,
                raft_log_index: cluster_status.raft_log_index,
                nodes: cluster_status.nodes.into_iter()
                    .map(|node| NodeInfo {
                        id: node.id,
                        address: node.address,
                        status: NodeStatusDto::from(node.status),
                        is_leader: Some(node.id) == cluster_status.leader_node_id,
                        capabilities: NodeCapabilities {
                            cpu_cores: 4, // Placeholder - would come from node info
                            memory_mb: 8192,
                            disk_gb: 100,
                            features: vec!["microvm".to_string(), "networking".to_string()],
                            utilization: None,
                        },
                        metadata: ResourceMetadata {
                            created_at: chrono::Utc::now() - chrono::Duration::hours(24),
                            updated_at: chrono::Utc::now(),
                            version: "1.0.0".to_string(),
                            labels: std::collections::HashMap::new(),
                            annotations: std::collections::HashMap::new(),
                        },
                        last_seen: chrono::Utc::now(),
                    })
                    .collect(),
                resources: ClusterResourceSummary {
                    total_cpu_cores: cluster_status.nodes.len() as u32 * 4,
                    total_memory_mb: cluster_status.nodes.len() as u64 * 8192,
                    total_disk_gb: cluster_status.nodes.len() as u64 * 100,
                    allocated_cpu_cores: 0, // Would come from VM allocations
                    allocated_memory_mb: 0,
                    allocated_disk_gb: 0,
                    total_vms: 0, // Would come from VM manager
                    utilization: ClusterResourceUtilization {
                        cpu_percent: 10.0,
                        memory_percent: 15.0,
                        disk_percent: 5.0,
                        average_node_utilization: 10.0,
                    },
                },
            };
            
            Ok(Json(api_cluster_status))
        }
        Err(e) => Err(handle_blixard_error(e)),
    }
}

/// Join a cluster
#[utoipa::path(
    post,
    path = "/cluster/join",
    tag = "cluster",
    summary = "Join cluster",
    description = "Join an existing cluster by connecting to a cluster node",
    request_body = JoinClusterRequest,
    responses(
        (status = 200, description = "Successfully joined cluster", body = JoinClusterResponse),
        (status = 400, description = "Invalid join request", body = ErrorResponse),
        (status = 409, description = "Already in a cluster", body = ErrorResponse),
        (status = 503, description = "Service unavailable", body = ErrorResponse)
    ),
    security(
        ("bearer_auth" = []),
        ("api_key" = [])
    )
)]
pub async fn join_cluster(
    Extension(app_state): Extension<AppState>,
    Json(request): Json<JoinClusterRequest>,
) -> Result<Json<JoinClusterResponse>, (axum::http::StatusCode, Json<crate::api::schemas::ErrorResponse>)> {
    let shared_state = &app_state.shared_state;
    
    // Validate request
    if request.cluster_address.is_empty() {
        return Err(json_error(
            axum::http::StatusCode::BAD_REQUEST,
            "Cluster address is required"
        ));
    }
    
    // Check if already in a cluster (unless force is specified)
    if !request.force && shared_state.get_node_state() == crate::types::NodeState::Active {
        return Err(json_error(
            axum::http::StatusCode::CONFLICT,
            "Node is already part of a cluster. Use force=true to override."
        ));
    }
    
    // Attempt to join the cluster
    match shared_state.join_cluster(&request.cluster_address).await {
        Ok(_) => {
            // Get updated cluster status
            let cluster_status = shared_state.get_cluster_status().await
                .map_err(handle_blixard_error)?;
            
            let response = JoinClusterResponse {
                success: true,
                message: "Successfully joined cluster".to_string(),
                node_id: shared_state.get_id(),
                cluster_status: ClusterStatusResponse {
                    cluster_id: format!("cluster_{}", cluster_status.leader_node_id.unwrap_or(0)),
                    total_nodes: cluster_status.nodes.len() as u32,
                    active_nodes: cluster_status.nodes.len() as u32,
                    leader_node_id: cluster_status.leader_node_id,
                    node_id: shared_state.get_id(),
                    node_status: NodeStatusDto::from(shared_state.get_node_state()),
                    is_leader: cluster_status.is_leader,
                    raft_term: cluster_status.raft_term,
                    raft_log_index: cluster_status.raft_log_index,
                    nodes: Vec::new(), // Simplified for join response
                    resources: ClusterResourceSummary {
                        total_cpu_cores: 0,
                        total_memory_mb: 0,
                        total_disk_gb: 0,
                        allocated_cpu_cores: 0,
                        allocated_memory_mb: 0,
                        allocated_disk_gb: 0,
                        total_vms: 0,
                        utilization: ClusterResourceUtilization {
                            cpu_percent: 0.0,
                            memory_percent: 0.0,
                            disk_percent: 0.0,
                            average_node_utilization: 0.0,
                        },
                    },
                },
                request_id: uuid::Uuid::new_v4().to_string(),
            };
            
            Ok(Json(response))
        }
        Err(e) => Err(handle_blixard_error(e)),
    }
}

/// Leave the cluster
#[utoipa::path(
    post,
    path = "/cluster/leave",
    tag = "cluster",
    summary = "Leave cluster",
    description = "Leave the current cluster and return to standalone mode",
    request_body = LeaveClusterRequest,
    responses(
        (status = 200, description = "Successfully left cluster", body = LeaveClusterResponse),
        (status = 400, description = "Invalid leave request", body = ErrorResponse),
        (status = 409, description = "Not in a cluster", body = ErrorResponse)
    ),
    security(
        ("bearer_auth" = []),
        ("api_key" = [])
    )
)]
pub async fn leave_cluster(
    Extension(app_state): Extension<AppState>,
    Json(request): Json<LeaveClusterRequest>,
) -> Result<Json<LeaveClusterResponse>, (axum::http::StatusCode, Json<crate::api::schemas::ErrorResponse>)> {
    let shared_state = &app_state.shared_state;
    
    // Check if in a cluster
    if shared_state.get_node_state() != crate::types::NodeState::Active {
        return Err(json_error(
            axum::http::StatusCode::CONFLICT,
            "Node is not part of a cluster"
        ));
    }
    
    // Attempt to leave the cluster
    match shared_state.leave_cluster().await {
        Ok(_) => {
            let response = LeaveClusterResponse {
                success: true,
                message: "Successfully left cluster".to_string(),
                drained_vms: 0, // Would track VMs that were migrated away
                request_id: uuid::Uuid::new_v4().to_string(),
            };
            
            Ok(Json(response))
        }
        Err(e) => Err(handle_blixard_error(e)),
    }
}

/// List cluster nodes
#[utoipa::path(
    get,
    path = "/cluster/nodes",
    tag = "cluster",
    summary = "List cluster nodes",
    description = "Get a list of all nodes in the cluster with optional filtering",
    params(
        ("status" = Option<NodeStatusDto>, Query, description = "Filter by node status"),
        ("include_utilization" = Option<bool>, Query, description = "Include resource utilization data"),
        ("sort_by" = Option<String>, Query, description = "Sort field (id, address, status, last_seen)"),
        ("sort_order" = Option<String>, Query, description = "Sort order (asc, desc)"),
        ("offset" = Option<u32>, Query, description = "Pagination offset"),
        ("limit" = Option<u32>, Query, description = "Pagination limit")
    ),
    responses(
        (status = 200, description = "Node list retrieved successfully", body = PaginatedResponse<NodeInfo>),
        (status = 503, description = "Cluster not available", body = ErrorResponse)
    ),
    security(
        ("bearer_auth" = []),
        ("api_key" = [])
    )
)]
pub async fn list_nodes(
    Extension(app_state): Extension<AppState>,
    Query(params): Query<ListNodesParams>,
) -> Result<Json<PaginatedResponse<NodeInfo>>, (axum::http::StatusCode, Json<crate::api::schemas::ErrorResponse>)> {
    let shared_state = &app_state.shared_state;
    
    match shared_state.get_cluster_status().await {
        Ok(cluster_status) => {
            let mut nodes: Vec<NodeInfo> = cluster_status.nodes.into_iter()
                .filter(|node| {
                    // Apply status filter if specified
                    params.status.map_or(true, |status| {
                        NodeStatusDto::from(node.status) == status
                    })
                })
                .map(|node| NodeInfo {
                    id: node.id,
                    address: node.address,
                    status: NodeStatusDto::from(node.status),
                    is_leader: Some(node.id) == cluster_status.leader_node_id,
                    capabilities: NodeCapabilities {
                        cpu_cores: 4,
                        memory_mb: 8192,
                        disk_gb: 100,
                        features: vec!["microvm".to_string()],
                        utilization: if params.include_utilization {
                            Some(crate::api::schemas::NodeResourceUtilization {
                                cpu_allocated: 1,
                                memory_allocated_mb: 1024,
                                disk_allocated_gb: 10,
                                vm_count: 2,
                                cpu_percent: 25.0,
                                memory_percent: 12.5,
                                disk_percent: 10.0,
                            })
                        } else {
                            None
                        },
                    },
                    metadata: ResourceMetadata {
                        created_at: chrono::Utc::now() - chrono::Duration::hours(24),
                        updated_at: chrono::Utc::now(),
                        version: "1.0.0".to_string(),
                        labels: std::collections::HashMap::new(),
                        annotations: std::collections::HashMap::new(),
                    },
                    last_seen: chrono::Utc::now(),
                })
                .collect();
            
            // Apply sorting
            if let Some(sort_by) = &params.sort_by {
                let ascending = params.sort_order.as_deref() != Some("desc");
                match sort_by.as_str() {
                    "id" => nodes.sort_by(|a, b| if ascending { a.id.cmp(&b.id) } else { b.id.cmp(&a.id) }),
                    "address" => nodes.sort_by(|a, b| if ascending { a.address.cmp(&b.address) } else { b.address.cmp(&a.address) }),
                    "last_seen" => nodes.sort_by(|a, b| if ascending { a.last_seen.cmp(&b.last_seen) } else { b.last_seen.cmp(&a.last_seen) }),
                    _ => {} // Invalid sort field, ignore
                }
            }
            
            let total = nodes.len() as u64;
            let (offset, limit) = extract_pagination(None);
            
            // Apply pagination
            let start = offset as usize;
            let end = std::cmp::min(start + limit as usize, nodes.len());
            let paginated_nodes = nodes.into_iter().skip(start).take(limit as usize).collect();
            
            let response = PaginatedResponse {
                items: paginated_nodes,
                total,
                count: (end - start) as u32,
                offset,
                limit,
            };
            
            Ok(Json(response))
        }
        Err(e) => Err(handle_blixard_error(e)),
    }
}

/// Get specific node information
#[utoipa::path(
    get,
    path = "/cluster/nodes/{node_id}",
    tag = "cluster",
    summary = "Get node information",
    description = "Get detailed information about a specific cluster node",
    params(
        ("node_id" = u64, Path, description = "Node ID to retrieve")
    ),
    responses(
        (status = 200, description = "Node information retrieved successfully", body = NodeInfo),
        (status = 404, description = "Node not found", body = ErrorResponse),
        (status = 503, description = "Cluster not available", body = ErrorResponse)
    ),
    security(
        ("bearer_auth" = []),
        ("api_key" = [])
    )
)]
pub async fn get_node(
    Extension(app_state): Extension<AppState>,
    Path(node_id): Path<u64>,
) -> Result<Json<NodeInfo>, (axum::http::StatusCode, Json<crate::api::schemas::ErrorResponse>)> {
    let shared_state = &app_state.shared_state;
    
    match shared_state.get_cluster_status().await {
        Ok(cluster_status) => {
            if let Some(node) = cluster_status.nodes.into_iter().find(|n| n.id == node_id) {
                let node_info = NodeInfo {
                    id: node.id,
                    address: node.address,
                    status: NodeStatusDto::from(node.status),
                    is_leader: Some(node.id) == cluster_status.leader_node_id,
                    capabilities: NodeCapabilities {
                        cpu_cores: 4,
                        memory_mb: 8192,
                        disk_gb: 100,
                        features: vec!["microvm".to_string(), "networking".to_string()],
                        utilization: Some(crate::api::schemas::NodeResourceUtilization {
                            cpu_allocated: 1,
                            memory_allocated_mb: 1024,
                            disk_allocated_gb: 10,
                            vm_count: 2,
                            cpu_percent: 25.0,
                            memory_percent: 12.5,
                            disk_percent: 10.0,
                        }),
                    },
                    metadata: ResourceMetadata {
                        created_at: chrono::Utc::now() - chrono::Duration::hours(24),
                        updated_at: chrono::Utc::now(),
                        version: "1.0.0".to_string(),
                        labels: std::collections::HashMap::new(),
                        annotations: std::collections::HashMap::new(),
                    },
                    last_seen: chrono::Utc::now(),
                };
                
                Ok(Json(node_info))
            } else {
                Err(json_error(
                    axum::http::StatusCode::NOT_FOUND,
                    format!("Node {} not found in cluster", node_id)
                ))
            }
        }
        Err(e) => Err(handle_blixard_error(e)),
    }
}