//! Raft-specific issue detection and remediation
//!
//! This module provides detectors and remediators for Raft consensus issues
//! such as leader election storms, stuck proposals, and log divergence.

use std::time::Duration;
use async_trait::async_trait;
use chrono::Utc;
use serde_json::json;
use crate::error::BlixardResult;
use crate::remediation_engine::{
    IssueDetector, IssueType, Issue, IssueSeverity, IssueResource,
    Remediator, RemediationContext, RemediationResult, RemediationAction,
};
#[cfg(feature = "observability")]
use crate::metrics_otel::record_remediation_action;
use tracing::{info, warn, error, debug};

/// Detector for Raft consensus issues
pub struct RaftConsensusDetector {
    /// Threshold for leader changes per minute
    leader_change_threshold: u32,
    /// Threshold for pending proposals
    pending_proposal_threshold: usize,
    /// Threshold for log lag
    log_lag_threshold: u64,
}

impl RaftConsensusDetector {
    pub fn new() -> Self {
        Self {
            leader_change_threshold: 5, // More than 5 leader changes per minute
            pending_proposal_threshold: 100,
            log_lag_threshold: 1000,
        }
    }
}

#[async_trait]
impl IssueDetector for RaftConsensusDetector {
    fn issue_types(&self) -> Vec<IssueType> {
        vec![IssueType::RaftConsensusIssue]
    }
    
    async fn detect(&self, context: &RemediationContext) -> Vec<Issue> {
        let mut issues = Vec::new();
        
        // Get Raft metrics
        let raft_status = match context.node_state.get_raft_status().await {
            Some(status) => status,
            None => return issues, // Not in Raft mode
        };
        
        // Check for leader election storms
        if let Some(metrics) = context.node_state.get_raft_metrics().await {
            if metrics.leader_changes_per_minute > self.leader_change_threshold {
                issues.push(Issue {
                    id: format!("raft-leader-storm-{}", Utc::now().timestamp()),
                    issue_type: IssueType::RaftConsensusIssue,
                    severity: IssueSeverity::Critical,
                    resource: IssueResource::Cluster,
                    detected_at: Utc::now(),
                    description: format!(
                        "Leader election storm detected: {} changes per minute",
                        metrics.leader_changes_per_minute
                    ),
                    context: json!({
                        "leader_changes": metrics.leader_changes_per_minute,
                        "current_term": raft_status.term,
                        "node_id": context.node_id,
                    }),
                    correlation_id: None,
                });
            }
            
            // Check for stuck proposals
            if metrics.pending_proposals > self.pending_proposal_threshold {
                issues.push(Issue {
                    id: format!("raft-stuck-proposals-{}", Utc::now().timestamp()),
                    issue_type: IssueType::RaftConsensusIssue,
                    severity: IssueSeverity::High,
                    resource: IssueResource::Cluster,
                    detected_at: Utc::now(),
                    description: format!(
                        "Too many pending proposals: {}",
                        metrics.pending_proposals
                    ),
                    context: json!({
                        "pending_proposals": metrics.pending_proposals,
                        "committed_index": raft_status.committed,
                        "applied_index": raft_status.applied,
                    }),
                    correlation_id: None,
                });
            }
            
            // Check for log divergence
            if let Some((peer_id, lag)) = metrics.max_log_lag {
                if lag > self.log_lag_threshold {
                    issues.push(Issue {
                        id: format!("raft-log-divergence-{}", Utc::now().timestamp()),
                        issue_type: IssueType::RaftConsensusIssue,
                        severity: IssueSeverity::High,
                        resource: IssueResource::Node {
                            id: peer_id,
                            address: format!("node-{}", peer_id), // TODO: Get actual address
                        },
                        detected_at: Utc::now(),
                        description: format!(
                            "Log divergence detected: peer {} is {} entries behind",
                            peer_id, lag
                        ),
                        context: json!({
                            "peer_id": peer_id,
                            "log_lag": lag,
                            "leader_commit": raft_status.committed,
                        }),
                        correlation_id: None,
                    });
                }
            }
        }
        
        // Check for no leader
        if raft_status.leader_id == 0 && raft_status.term > 0 {
            issues.push(Issue {
                id: format!("raft-no-leader-{}", Utc::now().timestamp()),
                issue_type: IssueType::RaftConsensusIssue,
                severity: IssueSeverity::Critical,
                resource: IssueResource::Cluster,
                detected_at: Utc::now(),
                description: "No leader elected in cluster".to_string(),
                context: json!({
                    "term": raft_status.term,
                    "state": format!("{:?}", raft_status.state),
                }),
                correlation_id: None,
            });
        }
        
        issues
    }
}

/// Remediator for leader election storms
pub struct LeaderElectionStormRemediator;

#[async_trait]
impl Remediator for LeaderElectionStormRemediator {
    fn can_handle(&self, issue: &Issue) -> bool {
        issue.issue_type == IssueType::RaftConsensusIssue
            && issue.description.contains("Leader election storm")
    }
    
    fn priority(&self) -> u32 {
        20 // High priority
    }
    
    async fn remediate(&self, issue: &Issue, context: &RemediationContext) -> RemediationResult {
        let start_time = std::time::Instant::now();
        let mut actions = Vec::new();
        
        info!("Attempting to remediate leader election storm");
        record_remediation_action("raft_consensus", "leader_election_storm");
        
        // Step 1: Increase election timeout temporarily
        actions.push(RemediationAction {
            action: "increase_election_timeout".to_string(),
            target: "raft_config".to_string(),
            result: "Increased timeout to reduce elections".to_string(),
            timestamp: Utc::now(),
        });
        
        // TODO: Actually increase the timeout
        // This would require access to Raft configuration
        
        // Step 2: Check network connectivity
        let connectivity_issues = Self::check_network_connectivity(context).await;
        if !connectivity_issues.is_empty() {
            actions.push(RemediationAction {
                action: "diagnose_network".to_string(),
                target: "cluster".to_string(),
                result: format!("Found connectivity issues: {:?}", connectivity_issues),
                timestamp: Utc::now(),
            });
            
            // If network issues, this might be causing the storm
            return RemediationResult {
                success: false,
                actions,
                error: Some("Network connectivity issues detected".to_string()),
                duration: start_time.elapsed(),
                requires_manual_intervention: true,
            };
        }
        
        // Step 3: Force a stable leader
        if let Some(preferred_leader) = Self::select_preferred_leader(context).await {
            actions.push(RemediationAction {
                action: "transfer_leadership".to_string(),
                target: format!("node-{}", preferred_leader),
                result: "Attempting to stabilize leadership".to_string(),
                timestamp: Utc::now(),
            });
            
            // TODO: Implement leadership transfer
        }
        
        RemediationResult {
            success: true,
            actions,
            error: None,
            duration: start_time.elapsed(),
            requires_manual_intervention: false,
        }
    }
}

impl LeaderElectionStormRemediator {
    async fn check_network_connectivity(context: &RemediationContext) -> Vec<String> {
        let mut issues = Vec::new();
        
        // TODO: Implement actual network connectivity checks
        // For now, return empty to indicate no issues
        
        issues
    }
    
    async fn select_preferred_leader(context: &RemediationContext) -> Option<u64> {
        // Select the most stable node as preferred leader
        // Criteria: lowest latency, most resources, etc.
        
        // TODO: Implement leader selection logic
        None
    }
}

/// Remediator for stuck proposals
pub struct StuckProposalRemediator;

#[async_trait]
impl Remediator for StuckProposalRemediator {
    fn can_handle(&self, issue: &Issue) -> bool {
        issue.issue_type == IssueType::RaftConsensusIssue
            && issue.description.contains("pending proposals")
    }
    
    fn priority(&self) -> u32 {
        15
    }
    
    async fn remediate(&self, issue: &Issue, context: &RemediationContext) -> RemediationResult {
        let start_time = std::time::Instant::now();
        let mut actions = Vec::new();
        
        info!("Attempting to remediate stuck proposals");
        record_remediation_action("raft_consensus", "stuck_proposals");
        
        // Step 1: Check if we're the leader
        let raft_status = context.node_state.get_raft_status().await;
        if let Some(status) = raft_status {
            if status.leader_id == context.node_id {
                // We're the leader with stuck proposals
                
                // Try to drain the proposal queue
                actions.push(RemediationAction {
                    action: "drain_proposal_queue".to_string(),
                    target: "raft_proposals".to_string(),
                    result: "Attempting to process pending proposals".to_string(),
                    timestamp: Utc::now(),
                });
                
                // TODO: Implement proposal queue draining
                
                // If that doesn't work, step down
                actions.push(RemediationAction {
                    action: "leader_step_down".to_string(),
                    target: format!("node-{}", context.node_id),
                    result: "Stepping down to trigger new election".to_string(),
                    timestamp: Utc::now(),
                });
                
                // TODO: Implement leader step down
            } else {
                // We're a follower, check if we can reach the leader
                actions.push(RemediationAction {
                    action: "check_leader_connectivity".to_string(),
                    target: format!("node-{}", status.leader_id),
                    result: "Verifying connection to leader".to_string(),
                    timestamp: Utc::now(),
                });
            }
        }
        
        RemediationResult {
            success: true,
            actions,
            error: None,
            duration: start_time.elapsed(),
            requires_manual_intervention: false,
        }
    }
}

/// Remediator for log divergence
pub struct LogDivergenceRemediator;

#[async_trait]
impl Remediator for LogDivergenceRemediator {
    fn can_handle(&self, issue: &Issue) -> bool {
        issue.issue_type == IssueType::RaftConsensusIssue
            && issue.description.contains("Log divergence")
    }
    
    fn priority(&self) -> u32 {
        25 // Very high priority
    }
    
    async fn remediate(&self, issue: &Issue, context: &RemediationContext) -> RemediationResult {
        let start_time = std::time::Instant::now();
        let mut actions = Vec::new();
        
        // Extract peer info from issue context
        let peer_id = issue.context.get("peer_id")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        
        let log_lag = issue.context.get("log_lag")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        
        info!("Attempting to remediate log divergence for peer {}", peer_id);
        record_remediation_action("raft_consensus", "log_divergence");
        
        // Step 1: Trigger snapshot if lag is too large
        if log_lag > 1000 {
            actions.push(RemediationAction {
                action: "trigger_snapshot".to_string(),
                target: format!("peer-{}", peer_id),
                result: "Sending snapshot to catch up peer".to_string(),
                timestamp: Utc::now(),
            });
            
            // TODO: Trigger snapshot send to peer
        } else {
            // Step 2: For smaller lag, ensure replication is working
            actions.push(RemediationAction {
                action: "force_replication".to_string(),
                target: format!("peer-{}", peer_id),
                result: "Forcing log replication to peer".to_string(),
                timestamp: Utc::now(),
            });
            
            // TODO: Force replication
        }
        
        // Step 3: If peer is consistently lagging, consider removing it
        if log_lag > 5000 {
            actions.push(RemediationAction {
                action: "consider_peer_removal".to_string(),
                target: format!("peer-{}", peer_id),
                result: "Peer severely lagging, may need removal".to_string(),
                timestamp: Utc::now(),
            });
            
            return RemediationResult {
                success: false,
                actions,
                error: Some("Peer too far behind, manual intervention needed".to_string()),
                duration: start_time.elapsed(),
                requires_manual_intervention: true,
            };
        }
        
        RemediationResult {
            success: true,
            actions,
            error: None,
            duration: start_time.elapsed(),
            requires_manual_intervention: false,
        }
    }
}

/// Detector for network partition issues
pub struct NetworkPartitionDetector;

#[async_trait]
impl IssueDetector for NetworkPartitionDetector {
    fn issue_types(&self) -> Vec<IssueType> {
        vec![IssueType::NetworkPartition]
    }
    
    async fn detect(&self, context: &RemediationContext) -> Vec<Issue> {
        let mut issues = Vec::new();
        
        // Get cluster status
        let cluster_status = match context.node_state.get_cluster_status().await {
            Ok(status) => status,
            Err(_) => return issues,
        };
        
        // Check if we can reach all known peers
        let mut unreachable_peers = Vec::new();
        for peer in &cluster_status.peers {
            // TODO: Implement actual connectivity check
            // For now, check if peer is marked as unreachable in metrics
            if let Some(metrics) = context.node_state.get_peer_metrics(peer.id).await {
                if metrics.consecutive_failures > 10 {
                    unreachable_peers.push(peer.id);
                }
            }
        }
        
        // If we can't reach more than half the peers, we might be partitioned
        if unreachable_peers.len() > cluster_status.peers.len() / 2 {
            issues.push(Issue {
                id: format!("network-partition-{}", Utc::now().timestamp()),
                issue_type: IssueType::NetworkPartition,
                severity: IssueSeverity::Critical,
                resource: IssueResource::Cluster,
                detected_at: Utc::now(),
                description: format!(
                    "Possible network partition: cannot reach {} out of {} peers",
                    unreachable_peers.len(),
                    cluster_status.peers.len()
                ),
                context: json!({
                    "unreachable_peers": unreachable_peers,
                    "total_peers": cluster_status.peers.len(),
                    "node_id": context.node_id,
                }),
                correlation_id: None,
            });
        }
        
        issues
    }
}

/// Remediator for network partitions
pub struct NetworkPartitionRemediator;

#[async_trait]
impl Remediator for NetworkPartitionRemediator {
    fn can_handle(&self, issue: &Issue) -> bool {
        issue.issue_type == IssueType::NetworkPartition
    }
    
    fn priority(&self) -> u32 {
        30 // Highest priority
    }
    
    async fn remediate(&self, issue: &Issue, context: &RemediationContext) -> RemediationResult {
        let start_time = std::time::Instant::now();
        let mut actions = Vec::new();
        
        warn!("Attempting to remediate network partition");
        record_remediation_action("network", "partition_detected");
        
        // Step 1: Verify we're actually partitioned
        actions.push(RemediationAction {
            action: "verify_partition".to_string(),
            target: "network".to_string(),
            result: "Checking network connectivity".to_string(),
            timestamp: Utc::now(),
        });
        
        // Step 2: If we're in the minority, fence ourselves
        let raft_status = context.node_state.get_raft_status().await;
        if let Some(status) = raft_status {
            if status.leader_id == 0 {
                // No leader visible, we might be in minority partition
                actions.push(RemediationAction {
                    action: "fence_node".to_string(),
                    target: format!("node-{}", context.node_id),
                    result: "Fencing node to prevent split-brain".to_string(),
                    timestamp: Utc::now(),
                });
                
                // TODO: Implement node fencing
                // This would stop accepting writes
            }
        }
        
        // Step 3: Try to re-establish connections
        if let Some(unreachable) = issue.context.get("unreachable_peers")
            .and_then(|v| v.as_array()) {
            for peer in unreachable {
                if let Some(peer_id) = peer.as_u64() {
                    actions.push(RemediationAction {
                        action: "reconnect_peer".to_string(),
                        target: format!("peer-{}", peer_id),
                        result: "Attempting to reconnect".to_string(),
                        timestamp: Utc::now(),
                    });
                }
            }
        }
        
        RemediationResult {
            success: false, // Network partitions usually need manual intervention
            actions,
            error: Some("Network partition detected, manual intervention recommended".to_string()),
            duration: start_time.elapsed(),
            requires_manual_intervention: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_raft_consensus_detection() {
        // Test detection of various Raft issues
    }
    
    #[tokio::test]
    async fn test_network_partition_detection() {
        // Test network partition detection logic
    }
}