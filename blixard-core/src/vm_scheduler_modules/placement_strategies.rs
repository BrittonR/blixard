use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::error::{BlixardError, BlixardResult};
use crate::metrics_otel;
use crate::types::{LocalityPreference, VmConfig};

use super::resource_analysis::{NodeResourceUsage, SchedulingContext, VmResourceRequirements};

/// Preemption policy for controlling VM preemption behavior
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PreemptionPolicy {
    /// Never preempt any VMs
    Never,
    /// Only preempt VMs with lower priority than the requesting VM
    LowerPriorityOnly,
    /// Preempt based on cost optimization
    CostAware {
        /// Maximum cost increase allowed for preemption (percentage)
        max_cost_increase: f64,
    },
}

/// Configuration for graceful preemption
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreemptionConfig {
    /// Grace period in seconds before forceful preemption
    pub grace_period_secs: u64,
    /// Whether to attempt live migration first
    pub try_live_migration: bool,
    /// Policy for preemption decisions
    pub policy: PreemptionPolicy,
}

/// VM placement strategy for determining where to place VMs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PlacementStrategy {
    /// Place VM on node with most available resources
    MostAvailable,
    /// Place VM on node with least available resources (bin packing)
    LeastAvailable,
    /// Spread VMs evenly across nodes
    RoundRobin,
    /// Place VM on specific node (manual placement)
    Manual { node_id: u64 },
    /// Priority-based placement with optional preemption
    PriorityBased {
        /// Base strategy to use for placement among candidates
        base_strategy: Box<PlacementStrategy>,
        /// Enable preemption of lower priority VMs
        enable_preemption: bool,
    },
    /// Locality-aware placement that prioritizes nodes based on datacenter/zone preferences
    LocalityAware {
        /// Base strategy to use within selected datacenter/zone
        base_strategy: Box<PlacementStrategy>,
        /// Whether to strictly enforce locality (fail if no nodes match)
        strict: bool,
    },
    /// Spread across failure domains (datacenters/zones) for high availability
    SpreadAcrossFailureDomains {
        /// Minimum number of failure domains to spread across
        min_domains: u32,
        /// Level to spread at: "datacenter", "zone", or "rack"
        spread_level: String,
    },
    /// Cost-optimized placement considering datacenter costs and network transfer costs
    CostOptimized {
        /// Maximum cost increase allowed vs cheapest option (percentage)
        max_cost_increase: f64,
        /// Whether to consider network transfer costs
        include_network_costs: bool,
    },
}

/// Result of a VM placement decision
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlacementDecision {
    pub target_node_id: u64,
    pub strategy_used: String,
    pub confidence_score: f64,
    pub preempted_vms: Vec<String>,
    pub resource_fit_score: f64,
    pub alternative_nodes: Vec<u64>,
}

/// Information about a VM that could be preempted
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreemptionCandidate {
    pub vm_name: String,
    pub priority: u32,
    pub node_id: u64,
    pub resources_freed: VmResourceRequirements,
    pub preemption_cost: f64,
}

impl super::VmScheduler {
    /// Apply the specified placement strategy to find the best node
    pub(super) async fn apply_placement_strategy(
        &self,
        context: &SchedulingContext,
        vm_config: &VmConfig,
        strategy: PlacementStrategy,
    ) -> BlixardResult<PlacementDecision> {
        match strategy {
            PlacementStrategy::MostAvailable => {
                self.apply_most_available_strategy(context, vm_config).await
            }
            PlacementStrategy::LeastAvailable => {
                self.apply_least_available_strategy(context, vm_config).await
            }
            PlacementStrategy::RoundRobin => {
                self.apply_round_robin_strategy(context, vm_config).await
            }
            PlacementStrategy::Manual { node_id } => {
                self.apply_manual_strategy(context, vm_config, node_id).await
            }
            PlacementStrategy::PriorityBased { base_strategy, enable_preemption } => {
                self.apply_priority_based_strategy(context, vm_config, *base_strategy, enable_preemption).await
            }
            PlacementStrategy::LocalityAware { base_strategy, strict } => {
                self.apply_locality_aware_strategy(context, vm_config, *base_strategy, strict).await
            }
            PlacementStrategy::SpreadAcrossFailureDomains { min_domains, spread_level } => {
                self.apply_spread_strategy(context, vm_config, min_domains, &spread_level).await
            }
            PlacementStrategy::CostOptimized { max_cost_increase, include_network_costs } => {
                self.apply_cost_optimized_strategy(context, vm_config, max_cost_increase, include_network_costs).await
            }
        }
    }

    /// Filter candidate nodes based on resource requirements and anti-affinity
    pub(super) fn filter_candidate_nodes<'a>(
        &self,
        context: &'a SchedulingContext,
        vm_config: &VmConfig,
    ) -> Vec<&'a NodeResourceUsage> {
        // Filter nodes that can accommodate the VM
        let mut candidate_nodes: Vec<_> = context
            .node_usage
            .iter()
            .filter(|node| node.can_accommodate(&context.requirements))
            .collect();

        // Apply anti-affinity constraints if present
        if let (Some(checker), Some(rules)) = (&context.anti_affinity_checker, &vm_config.anti_affinity) {
            candidate_nodes.retain(|node| {
                checker.check_placement(&vm_config.name, node.node_id, rules).is_ok()
            });
        }

        candidate_nodes
    }

    /// Apply "most available" placement strategy
    async fn apply_most_available_strategy(
        &self,
        context: &SchedulingContext,
        vm_config: &VmConfig,
    ) -> BlixardResult<PlacementDecision> {
        let candidate_nodes = self.filter_candidate_nodes(context, vm_config);

        if candidate_nodes.is_empty() {
            return Err(BlixardError::SchedulingError {
                message: "No suitable nodes found for VM placement".to_string(),
            });
        }

        // Find node with highest availability score
        let best_node = candidate_nodes
            .iter()
            .max_by(|a, b| {
                a.calculate_availability_score()
                    .partial_cmp(&b.calculate_availability_score())
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .unwrap();

        let confidence_score = best_node.calculate_availability_score();
        let resource_fit_score = self.calculate_resource_fit_score(best_node, &context.requirements);

        let alternative_nodes: Vec<u64> = candidate_nodes
            .iter()
            .filter(|node| node.node_id != best_node.node_id)
            .take(3)
            .map(|node| node.node_id)
            .collect();

        Ok(PlacementDecision {
            target_node_id: best_node.node_id,
            strategy_used: "MostAvailable".to_string(),
            confidence_score,
            preempted_vms: Vec::new(),
            resource_fit_score,
            alternative_nodes,
        })
    }

    /// Apply "least available" placement strategy (bin packing)
    async fn apply_least_available_strategy(
        &self,
        context: &SchedulingContext,
        vm_config: &VmConfig,
    ) -> BlixardResult<PlacementDecision> {
        let candidate_nodes = self.filter_candidate_nodes(context, vm_config);

        if candidate_nodes.is_empty() {
            return Err(BlixardError::SchedulingError {
                message: "No suitable nodes found for VM placement".to_string(),
            });
        }

        // Find node with lowest availability score (most utilized but still fits)
        let best_node = candidate_nodes
            .iter()
            .min_by(|a, b| {
                a.calculate_availability_score()
                    .partial_cmp(&b.calculate_availability_score())
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .unwrap();

        let confidence_score = 100.0 - best_node.calculate_availability_score(); // Invert for bin packing
        let resource_fit_score = self.calculate_resource_fit_score(best_node, &context.requirements);

        let alternative_nodes: Vec<u64> = candidate_nodes
            .iter()
            .filter(|node| node.node_id != best_node.node_id)
            .take(3)
            .map(|node| node.node_id)
            .collect();

        Ok(PlacementDecision {
            target_node_id: best_node.node_id,
            strategy_used: "LeastAvailable".to_string(),
            confidence_score,
            preempted_vms: Vec::new(),
            resource_fit_score,
            alternative_nodes,
        })
    }

    /// Apply round-robin placement strategy
    async fn apply_round_robin_strategy(
        &self,
        context: &SchedulingContext,
        vm_config: &VmConfig,
    ) -> BlixardResult<PlacementDecision> {
        let candidate_nodes = self.filter_candidate_nodes(context, vm_config);

        if candidate_nodes.is_empty() {
            return Err(BlixardError::SchedulingError {
                message: "No suitable nodes found for VM placement".to_string(),
            });
        }

        // Simple round-robin based on VM name hash
        let vm_hash = calculate_string_hash(&vm_config.name);
        let selected_index = (vm_hash as usize) % candidate_nodes.len();
        let best_node = candidate_nodes[selected_index];

        let confidence_score = 75.0; // Medium confidence for round-robin
        let resource_fit_score = self.calculate_resource_fit_score(best_node, &context.requirements);

        let alternative_nodes: Vec<u64> = candidate_nodes
            .iter()
            .filter(|node| node.node_id != best_node.node_id)
            .take(3)
            .map(|node| node.node_id)
            .collect();

        Ok(PlacementDecision {
            target_node_id: best_node.node_id,
            strategy_used: "RoundRobin".to_string(),
            confidence_score,
            preempted_vms: Vec::new(),
            resource_fit_score,
            alternative_nodes,
        })
    }

    /// Apply manual placement strategy
    async fn apply_manual_strategy(
        &self,
        context: &SchedulingContext,
        vm_config: &VmConfig,
        target_node_id: u64,
    ) -> BlixardResult<PlacementDecision> {
        // Find the specific node
        let target_node = context
            .node_usage
            .iter()
            .find(|node| node.node_id == target_node_id)
            .ok_or_else(|| BlixardError::SchedulingError {
                message: format!("Target node {} not found", target_node_id),
            })?;

        // Check if the node can accommodate the VM
        if !target_node.can_accommodate(&context.requirements) {
            return Err(BlixardError::SchedulingError {
                message: format!(
                    "Target node {} cannot accommodate VM requirements",
                    target_node_id
                ),
            });
        }

        // Check anti-affinity constraints
        if let (Some(checker), Some(rules)) = (&context.anti_affinity_checker, &vm_config.anti_affinity) {
            checker.check_placement(&vm_config.name, target_node_id, rules)
                .map_err(|e| BlixardError::SchedulingError {
                    message: format!("Anti-affinity violation: {}", e),
                })?;
        }

        let confidence_score = 100.0; // High confidence for manual placement
        let resource_fit_score = self.calculate_resource_fit_score(target_node, &context.requirements);

        Ok(PlacementDecision {
            target_node_id,
            strategy_used: "Manual".to_string(),
            confidence_score,
            preempted_vms: Vec::new(),
            resource_fit_score,
            alternative_nodes: Vec::new(),
        })
    }

    /// Apply priority-based placement strategy
    async fn apply_priority_based_strategy(
        &self,
        context: &SchedulingContext,
        vm_config: &VmConfig,
        base_strategy: PlacementStrategy,
        enable_preemption: bool,
    ) -> BlixardResult<PlacementDecision> {
        // First try the base strategy
        match self.apply_placement_strategy(context, vm_config, base_strategy.clone()).await {
            Ok(decision) => Ok(decision),
            Err(_) if enable_preemption => {
                // Base strategy failed, try preemption
                self.apply_preemption_strategy(context, vm_config, base_strategy).await
            }
            Err(e) => Err(e),
        }
    }

    /// Apply locality-aware placement strategy
    async fn apply_locality_aware_strategy(
        &self,
        context: &SchedulingContext,
        vm_config: &VmConfig,
        base_strategy: PlacementStrategy,
        strict: bool,
    ) -> BlixardResult<PlacementDecision> {
        // Filter nodes based on locality preferences
        let locality_filtered_context = self.filter_by_locality(context, vm_config, strict)?;
        
        // Apply base strategy on filtered nodes
        self.apply_placement_strategy(&locality_filtered_context, vm_config, base_strategy).await
    }

    /// Apply spread strategy across failure domains
    async fn apply_spread_strategy(
        &self,
        context: &SchedulingContext,
        vm_config: &VmConfig,
        min_domains: u32,
        spread_level: &str,
    ) -> BlixardResult<PlacementDecision> {
        // Group nodes by failure domain
        let domain_groups = self.group_nodes_by_domain(context, spread_level);
        
        if domain_groups.len() < min_domains as usize {
            return Err(BlixardError::SchedulingError {
                message: format!(
                    "Not enough failure domains available: {} required, {} found",
                    min_domains, domain_groups.len()
                ),
            });
        }

        // Find the domain with least VMs for this application
        let best_domain = self.find_best_spread_domain(&domain_groups, vm_config).await?;
        
        // Apply most available strategy within the chosen domain
        let filtered_context = self.filter_context_by_domain(context, &best_domain, spread_level);
        self.apply_most_available_strategy(&filtered_context, vm_config).await
    }

    /// Apply cost-optimized placement strategy
    async fn apply_cost_optimized_strategy(
        &self,
        context: &SchedulingContext,
        vm_config: &VmConfig,
        max_cost_increase: f64,
        include_network_costs: bool,
    ) -> BlixardResult<PlacementDecision> {
        let candidate_nodes = self.filter_candidate_nodes(context, vm_config);

        if candidate_nodes.is_empty() {
            return Err(BlixardError::SchedulingError {
                message: "No suitable nodes found for VM placement".to_string(),
            });
        }

        // Calculate costs for each candidate
        let mut node_costs: Vec<(u64, f64)> = Vec::new();
        for node in &candidate_nodes {
            let cost = self.calculate_placement_cost(node, vm_config, include_network_costs).await;
            node_costs.push((node.node_id, cost));
        }

        // Sort by cost
        node_costs.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

        let cheapest_cost = node_costs[0].1;
        let max_allowed_cost = cheapest_cost * (1.0 + max_cost_increase / 100.0);

        // Find best node within cost constraints
        let (best_node_id, best_cost) = node_costs
            .iter()
            .find(|(_, cost)| *cost <= max_allowed_cost)
            .copied()
            .unwrap_or(node_costs[0]);

        let best_node = candidate_nodes
            .iter()
            .find(|node| node.node_id == best_node_id)
            .unwrap();

        let confidence_score = 100.0 - ((best_cost - cheapest_cost) / cheapest_cost * 100.0);
        let resource_fit_score = self.calculate_resource_fit_score(best_node, &context.requirements);

        let alternative_nodes: Vec<u64> = node_costs
            .iter()
            .filter(|(node_id, _)| *node_id != best_node_id)
            .take(3)
            .map(|(node_id, _)| *node_id)
            .collect();

        Ok(PlacementDecision {
            target_node_id: best_node_id,
            strategy_used: "CostOptimized".to_string(),
            confidence_score,
            preempted_vms: Vec::new(),
            resource_fit_score,
            alternative_nodes,
        })
    }

    /// Calculate how well the VM's resource requirements fit on the node
    fn calculate_resource_fit_score(&self, node: &NodeResourceUsage, requirements: &VmResourceRequirements) -> f64 {
        let cpu_fit = requirements.vcpus as f64 / node.capabilities.cpu_cores as f64;
        let memory_fit = requirements.memory_mb as f64 / node.capabilities.memory_mb as f64;
        let disk_fit = requirements.disk_gb as f64 / node.capabilities.disk_gb as f64;

        // Lower is better (less resource consumption relative to capacity)
        let fit_score = (cpu_fit + memory_fit + disk_fit) / 3.0;
        (1.0 - fit_score).max(0.0) * 100.0
    }

    /// Apply preemption strategy (placeholder implementation)
    async fn apply_preemption_strategy(
        &self,
        _context: &SchedulingContext,
        _vm_config: &VmConfig,
        _base_strategy: PlacementStrategy,
    ) -> BlixardResult<PlacementDecision> {
        // TODO: Implement preemption logic
        Err(BlixardError::SchedulingError {
            message: "Preemption not yet implemented".to_string(),
        })
    }

    /// Filter scheduling context by locality preferences (placeholder)
    fn filter_by_locality(
        &self,
        context: &SchedulingContext,
        _vm_config: &VmConfig,
        _strict: bool,
    ) -> BlixardResult<SchedulingContext> {
        // TODO: Implement locality filtering
        Ok(context.clone())
    }

    /// Group nodes by failure domain (placeholder)
    fn group_nodes_by_domain(
        &self,
        _context: &SchedulingContext,
        _spread_level: &str,
    ) -> HashMap<String, Vec<u64>> {
        // TODO: Implement domain grouping
        HashMap::new()
    }

    /// Find best domain for spreading (placeholder)
    async fn find_best_spread_domain(
        &self,
        _domain_groups: &HashMap<String, Vec<u64>>,
        _vm_config: &VmConfig,
    ) -> BlixardResult<String> {
        // TODO: Implement domain selection
        Err(BlixardError::SchedulingError {
            message: "Domain spread not yet implemented".to_string(),
        })
    }

    /// Filter context by specific domain (placeholder)
    fn filter_context_by_domain(
        &self,
        context: &SchedulingContext,
        _domain: &str,
        _spread_level: &str,
    ) -> SchedulingContext {
        // TODO: Implement domain filtering
        context.clone()
    }

    /// Calculate placement cost for a node (placeholder)
    async fn calculate_placement_cost(
        &self,
        node: &NodeResourceUsage,
        _vm_config: &VmConfig,
        _include_network_costs: bool,
    ) -> f64 {
        // Simple cost model based on node cost per hour
        node.cost_per_hour.unwrap_or(1.0)
    }
}

/// Simple hash function for strings
fn calculate_string_hash(s: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    s.hash(&mut hasher);
    hasher.finish()
}