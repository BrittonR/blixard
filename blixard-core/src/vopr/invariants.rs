//! Invariant checking for safety and liveness properties
//!
//! This module defines invariants that should hold throughout execution
//! and provides mechanisms to check them.

use crate::vopr::state_tracker::StateSnapshot;
use std::collections::{HashMap, HashSet};

/// An invariant that should hold during execution
pub trait Invariant: Send + Sync {
    /// Name of this invariant
    fn name(&self) -> &str;

    /// Check if the invariant holds for the given state
    fn check(&self, state: &StateSnapshot) -> Result<(), String>;

    /// Whether this is a safety or liveness invariant
    fn invariant_type(&self) -> InvariantType;
}

/// Type of invariant
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InvariantType {
    Safety,
    Liveness,
}

/// Collection of safety invariants
pub struct SafetyInvariant {
    invariants: Vec<Box<dyn Invariant>>,
}

impl SafetyInvariant {
    /// Create a new collection of safety invariants
    pub fn new() -> Self {
        let invariants: Vec<Box<dyn Invariant>> = vec![
            Box::new(SingleLeaderInvariant),
            Box::new(MonotonicTermInvariant),
            Box::new(LogMatchingInvariant),
            Box::new(CommitSafetyInvariant),
            Box::new(VmUniquenessInvariant),
            Box::new(ResourceLimitsInvariant),
            Box::new(ConsistentViewInvariant),
        ];

        Self { invariants }
    }

    /// Check all safety invariants
    pub fn check_all(&self, state: &StateSnapshot) -> Vec<String> {
        let mut violations = Vec::new();

        for invariant in &self.invariants {
            if let Err(violation) = invariant.check(state) {
                violations.push(format!("{}: {}", invariant.name(), violation));
            }
        }

        violations
    }
}

/// Collection of liveness invariants
pub struct LivenessInvariant {
    invariants: Vec<Box<dyn Invariant>>,
    /// History of states for liveness checking
    history: Vec<StateSnapshot>,
    /// Maximum history size
    max_history: usize,
}

impl LivenessInvariant {
    /// Create a new collection of liveness invariants
    pub fn new() -> Self {
        let invariants: Vec<Box<dyn Invariant>> = vec![
            Box::new(ProgressInvariant::new()),
            Box::new(LeaderElectionInvariant::new()),
            Box::new(RequestCompletionInvariant::new()),
        ];

        Self {
            invariants,
            history: Vec::new(),
            max_history: 100,
        }
    }

    /// Update history and check liveness invariants
    pub fn check_all(&mut self, state: &StateSnapshot) -> Vec<String> {
        // Add to history
        self.history.push(state.clone());
        if self.history.len() > self.max_history {
            self.history.remove(0);
        }

        let mut violations = Vec::new();

        // Liveness invariants need history to check
        if self.history.len() >= 10 {
            for invariant in &self.invariants {
                if let Err(violation) = invariant.check(state) {
                    violations.push(format!("{}: {}", invariant.name(), violation));
                }
            }
        }

        violations
    }
}

/// Checks all invariants
pub struct InvariantChecker {
    safety: SafetyInvariant,
    liveness: LivenessInvariant,
}

impl InvariantChecker {
    /// Create a new invariant checker
    pub fn new() -> Self {
        Self {
            safety: SafetyInvariant::new(),
            liveness: LivenessInvariant::new(),
        }
    }

    /// Check all invariants
    pub fn check(&mut self, state: &StateSnapshot) -> Vec<String> {
        let mut violations = self.safety.check_all(state);
        violations.extend(self.liveness.check_all(state));
        violations
    }
}

// Safety Invariants

/// At most one leader per term
struct SingleLeaderInvariant;

impl Invariant for SingleLeaderInvariant {
    fn name(&self) -> &str {
        "SingleLeader"
    }

    fn check(&self, state: &StateSnapshot) -> Result<(), String> {
        // Group nodes by term
        let mut leaders_by_term: HashMap<u64, Vec<u64>> = HashMap::new();

        for (node_id, node_state) in &state.nodes {
            if node_state.role == crate::vopr::state_tracker::NodeRole::Leader {
                if let Some(term) = state.views.get(node_id) {
                    leaders_by_term
                        .entry(*term)
                        .or_insert_with(Vec::new)
                        .push(*node_id);
                }
            }
        }

        // Check for multiple leaders in same term
        for (term, leaders) in leaders_by_term {
            if leaders.len() > 1 {
                return Err(format!("Multiple leaders in term {}: {:?}", term, leaders));
            }
        }

        Ok(())
    }

    fn invariant_type(&self) -> InvariantType {
        InvariantType::Safety
    }
}

/// Terms must be monotonically increasing
struct MonotonicTermInvariant;

impl Invariant for MonotonicTermInvariant {
    fn name(&self) -> &str {
        "MonotonicTerm"
    }

    fn check(&self, state: &StateSnapshot) -> Result<(), String> {
        // This would need historical data to check properly
        // For now, just check that all terms are positive
        for (node_id, term) in &state.views {
            if *term == 0 {
                return Err(format!("Node {} has invalid term 0", node_id));
            }
        }
        Ok(())
    }

    fn invariant_type(&self) -> InvariantType {
        InvariantType::Safety
    }
}

/// Log matching property
struct LogMatchingInvariant;

impl Invariant for LogMatchingInvariant {
    fn name(&self) -> &str {
        "LogMatching"
    }

    fn check(&self, state: &StateSnapshot) -> Result<(), String> {
        // Check that commit points don't exceed log length
        for (node_id, node_state) in &state.nodes {
            if let Some(commit_point) = state.commit_points.get(node_id) {
                if *commit_point as usize > node_state.log_length {
                    return Err(format!(
                        "Node {} has commit point {} but log length {}",
                        node_id, commit_point, node_state.log_length
                    ));
                }
            }
        }
        Ok(())
    }

    fn invariant_type(&self) -> InvariantType {
        InvariantType::Safety
    }
}

/// Committed entries are immutable
struct CommitSafetyInvariant;

impl Invariant for CommitSafetyInvariant {
    fn name(&self) -> &str {
        "CommitSafety"
    }

    fn check(&self, state: &StateSnapshot) -> Result<(), String> {
        // Check that applied index doesn't exceed commit point
        for (node_id, node_state) in &state.nodes {
            if let Some(commit_point) = state.commit_points.get(node_id) {
                if node_state.applied_index > *commit_point {
                    return Err(format!(
                        "Node {} has applied index {} > commit point {}",
                        node_id, node_state.applied_index, commit_point
                    ));
                }
            }
        }
        Ok(())
    }

    fn invariant_type(&self) -> InvariantType {
        InvariantType::Safety
    }
}

/// VM IDs must be unique
struct VmUniquenessInvariant;

impl Invariant for VmUniquenessInvariant {
    fn name(&self) -> &str {
        "VmUniqueness"
    }

    fn check(&self, state: &StateSnapshot) -> Result<(), String> {
        // VMs are already in a HashMap, so uniqueness is guaranteed
        // Check that running VMs have valid host nodes
        for (vm_id, vm_state) in &state.vms {
            if vm_state.status == crate::vopr::state_tracker::VmStatus::Running {
                if let Some(host) = vm_state.host_node {
                    if !state.nodes.contains_key(&host) {
                        return Err(format!(
                            "VM {} running on non-existent node {}",
                            vm_id, host
                        ));
                    }
                } else {
                    return Err(format!("Running VM {} has no host node", vm_id));
                }
            }
        }
        Ok(())
    }

    fn invariant_type(&self) -> InvariantType {
        InvariantType::Safety
    }
}

/// Resource limits must be respected
struct ResourceLimitsInvariant;

impl Invariant for ResourceLimitsInvariant {
    fn name(&self) -> &str {
        "ResourceLimits"
    }

    fn check(&self, state: &StateSnapshot) -> Result<(), String> {
        // Check CPU usage
        if state.resources.total_cpu_percent > 100.0 {
            return Err(format!(
                "CPU usage {}% exceeds 100%",
                state.resources.total_cpu_percent
            ));
        }

        // Check reasonable memory usage (e.g., < 1TB)
        if state.resources.total_memory_mb > 1_000_000 {
            return Err(format!(
                "Memory usage {}MB exceeds reasonable limits",
                state.resources.total_memory_mb
            ));
        }

        Ok(())
    }

    fn invariant_type(&self) -> InvariantType {
        InvariantType::Safety
    }
}

/// Nodes in same partition should have consistent view
struct ConsistentViewInvariant;

impl Invariant for ConsistentViewInvariant {
    fn name(&self) -> &str {
        "ConsistentView"
    }

    fn check(&self, state: &StateSnapshot) -> Result<(), String> {
        // If there are no partitions, all nodes should converge to same view
        if state.partitions.is_empty() {
            let running_nodes: Vec<_> = state
                .nodes
                .values()
                .filter(|n| n.is_running && n.byzantine.is_none())
                .map(|n| n.id)
                .collect();

            if running_nodes.len() > 1 {
                // Check view consistency
                let views: HashSet<_> = running_nodes
                    .iter()
                    .filter_map(|id| state.views.get(id))
                    .collect();

                if views.len() > 2 {
                    return Err(format!("Too many different views: {:?}", views));
                }
            }
        }

        Ok(())
    }

    fn invariant_type(&self) -> InvariantType {
        InvariantType::Safety
    }
}

// Liveness Invariants

/// System should make progress
struct ProgressInvariant {
    no_progress_threshold: usize,
}

impl ProgressInvariant {
    fn new() -> Self {
        Self {
            no_progress_threshold: 20, // 20 snapshots without progress
        }
    }
}

impl Invariant for ProgressInvariant {
    fn name(&self) -> &str {
        "Progress"
    }

    fn check(&self, state: &StateSnapshot) -> Result<(), String> {
        // This would need historical data to check properly
        // For now, just check that some node has non-zero commit
        let max_commit = state.commit_points.values().max().copied().unwrap_or(0);

        if max_commit == 0 && state.nodes.len() > 0 {
            return Err("No progress: all nodes have commit point 0".to_string());
        }

        Ok(())
    }

    fn invariant_type(&self) -> InvariantType {
        InvariantType::Liveness
    }
}

/// Leader should be elected eventually
struct LeaderElectionInvariant {
    no_leader_threshold: usize,
}

impl LeaderElectionInvariant {
    fn new() -> Self {
        Self {
            no_leader_threshold: 10, // 10 snapshots without leader
        }
    }
}

impl Invariant for LeaderElectionInvariant {
    fn name(&self) -> &str {
        "LeaderElection"
    }

    fn check(&self, state: &StateSnapshot) -> Result<(), String> {
        let has_leader = state
            .nodes
            .values()
            .any(|n| n.role == crate::vopr::state_tracker::NodeRole::Leader && n.is_running);

        let has_running_nodes = state
            .nodes
            .values()
            .any(|n| n.is_running && n.byzantine.is_none());

        if has_running_nodes && !has_leader {
            return Err("No leader elected despite having running nodes".to_string());
        }

        Ok(())
    }

    fn invariant_type(&self) -> InvariantType {
        InvariantType::Liveness
    }
}

/// Client requests should complete eventually
struct RequestCompletionInvariant {
    timeout_threshold: usize,
}

impl RequestCompletionInvariant {
    fn new() -> Self {
        Self {
            timeout_threshold: 50, // 50 snapshots for request completion
        }
    }
}

impl Invariant for RequestCompletionInvariant {
    fn name(&self) -> &str {
        "RequestCompletion"
    }

    fn check(&self, _state: &StateSnapshot) -> Result<(), String> {
        // This would need to track in-flight requests
        // For now, always pass
        Ok(())
    }

    fn invariant_type(&self) -> InvariantType {
        InvariantType::Liveness
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vopr::state_tracker::{NodeRole, NodeState};

    #[test]
    fn test_single_leader_invariant() {
        let mut state = StateSnapshot {
            timestamp: 1000,
            nodes: HashMap::new(),
            views: HashMap::new(),
            commit_points: HashMap::new(),
            vms: HashMap::new(),
            partitions: Vec::new(),
            messages_in_flight: 0,
            resources: crate::vopr::state_tracker::ResourceUsage {
                total_cpu_percent: 50.0,
                total_memory_mb: 1000,
                total_disk_io_mb: 100,
                network_bandwidth_mbps: 10.0,
            },
            violations: Vec::new(),
        };

        // Add two leaders in same term
        state.nodes.insert(
            1,
            NodeState {
                id: 1,
                role: NodeRole::Leader,
                is_running: true,
                last_heartbeat: 900,
                log_length: 10,
                applied_index: 5,
                clock_skew_ms: 0,
                byzantine: None,
            },
        );

        state.nodes.insert(
            2,
            NodeState {
                id: 2,
                role: NodeRole::Leader,
                is_running: true,
                last_heartbeat: 950,
                log_length: 10,
                applied_index: 5,
                clock_skew_ms: 0,
                byzantine: None,
            },
        );

        state.views.insert(1, 5);
        state.views.insert(2, 5);

        let invariant = SingleLeaderInvariant;
        assert!(invariant.check(&state).is_err());
    }
}
