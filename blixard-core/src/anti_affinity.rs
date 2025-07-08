//! Anti-affinity rules for VM placement
//!
//! This module provides anti-affinity constraints that ensure VMs are spread
//! across different nodes for high availability. Rules can be hard (must) or
//! soft (prefer) constraints.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// Anti-affinity rule type
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AntiAffinityType {
    /// Hard constraint - VM placement must respect this rule
    Hard,
    /// Soft constraint - VM placement should prefer to respect this rule
    Soft { weight: f64 },
}

impl Default for AntiAffinityType {
    fn default() -> Self {
        AntiAffinityType::Soft { weight: 1.0 }
    }
}

/// Anti-affinity rule defining how VMs should be spread across nodes
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AntiAffinityRule {
    /// Type of anti-affinity (hard or soft)
    pub rule_type: AntiAffinityType,
    /// Group identifier for VMs that should be spread apart
    pub group_key: String,
    /// Maximum number of VMs from this group allowed per node (default: 1)
    #[serde(default = "default_max_per_node")]
    pub max_per_node: usize,
    /// Optional scope to limit anti-affinity (e.g., "rack", "zone", "datacenter")
    pub scope: Option<String>,
}

fn default_max_per_node() -> usize {
    1
}

impl AntiAffinityRule {
    /// Create a new hard anti-affinity rule
    pub fn hard(group_key: impl Into<String>) -> Self {
        Self {
            rule_type: AntiAffinityType::Hard,
            group_key: group_key.into(),
            max_per_node: 1,
            scope: None,
        }
    }

    /// Create a new soft anti-affinity rule with weight
    pub fn soft(group_key: impl Into<String>, weight: f64) -> Self {
        Self {
            rule_type: AntiAffinityType::Soft { weight },
            group_key: group_key.into(),
            max_per_node: 1,
            scope: None,
        }
    }

    /// Set the maximum VMs per node for this rule
    pub fn with_max_per_node(mut self, max: usize) -> Self {
        self.max_per_node = max;
        self
    }

    /// Set the scope for this rule
    pub fn with_scope(mut self, scope: impl Into<String>) -> Self {
        self.scope = Some(scope.into());
        self
    }
}

/// Collection of anti-affinity rules for a VM
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AntiAffinityRules {
    /// List of anti-affinity rules
    pub rules: Vec<AntiAffinityRule>,
}

impl AntiAffinityRules {
    /// Create a new empty rule set
    pub fn new() -> Self {
        Self { rules: Vec::new() }
    }

    /// Add a rule to the collection
    pub fn add_rule(mut self, rule: AntiAffinityRule) -> Self {
        self.rules.push(rule);
        self
    }

    /// Check if there are any hard constraints
    pub fn has_hard_constraints(&self) -> bool {
        self.rules
            .iter()
            .any(|r| matches!(r.rule_type, AntiAffinityType::Hard))
    }

    /// Get all hard constraints
    pub fn hard_constraints(&self) -> impl Iterator<Item = &AntiAffinityRule> {
        self.rules
            .iter()
            .filter(|r| matches!(r.rule_type, AntiAffinityType::Hard))
    }

    /// Get all soft constraints with their weights
    pub fn soft_constraints(&self) -> impl Iterator<Item = (&AntiAffinityRule, f64)> {
        self.rules.iter().filter_map(|r| {
            if let AntiAffinityType::Soft { weight } = r.rule_type {
                Some((r, weight))
            } else {
                None
            }
        })
    }
}

/// Anti-affinity constraint checker
pub struct AntiAffinityChecker {
    /// Map of group_key -> node_id -> VM count
    group_distribution: HashMap<String, HashMap<u64, usize>>,
}

impl AntiAffinityChecker {
    /// Create a new checker with current VM distribution
    pub fn new(vm_distribution: Vec<(String, Vec<String>, u64)>) -> Self {
        let mut group_distribution = HashMap::new();

        // Build distribution map from (vm_name, groups, node_id) tuples
        for (_vm_name, groups, node_id) in vm_distribution {
            for group in groups {
                let node_counts = group_distribution.entry(group).or_insert_with(HashMap::new);
                *node_counts.entry(node_id).or_insert(0) += 1;
            }
        }

        Self { group_distribution }
    }

    /// Check if placing a VM with given anti-affinity rules on a node violates hard constraints
    pub fn check_hard_constraints(
        &self,
        rules: &AntiAffinityRules,
        candidate_node: u64,
    ) -> Result<(), String> {
        for rule in rules.hard_constraints() {
            let current_count = self
                .group_distribution
                .get(&rule.group_key)
                .and_then(|nodes| nodes.get(&candidate_node))
                .copied()
                .unwrap_or(0);

            if current_count >= rule.max_per_node {
                return Err(format!(
                    "Hard anti-affinity constraint violated: group '{}' already has {} VMs on node {} (max: {})",
                    rule.group_key, current_count, candidate_node, rule.max_per_node
                ));
            }
        }

        Ok(())
    }

    /// Calculate penalty score for soft constraints (0.0 = no penalty, higher = worse)
    pub fn calculate_soft_constraint_penalty(
        &self,
        rules: &AntiAffinityRules,
        candidate_node: u64,
    ) -> f64 {
        let mut total_penalty = 0.0;

        for (rule, weight) in rules.soft_constraints() {
            let current_count = self
                .group_distribution
                .get(&rule.group_key)
                .and_then(|nodes| nodes.get(&candidate_node))
                .copied()
                .unwrap_or(0);

            // Calculate penalty based on how much we exceed the preferred max
            if current_count >= rule.max_per_node {
                let excess = (current_count - rule.max_per_node + 1) as f64;
                total_penalty += excess * weight;
            }
        }

        total_penalty
    }

    /// Get nodes that violate hard constraints for the given rules
    pub fn get_violating_nodes(&self, rules: &AntiAffinityRules) -> HashSet<u64> {
        let mut violating_nodes = HashSet::new();

        for rule in rules.hard_constraints() {
            if let Some(node_counts) = self.group_distribution.get(&rule.group_key) {
                for (&node_id, &count) in node_counts {
                    if count >= rule.max_per_node {
                        violating_nodes.insert(node_id);
                    }
                }
            }
        }

        violating_nodes
    }

    /// Get distribution statistics for a group
    pub fn get_group_distribution(&self, group_key: &str) -> Option<&HashMap<u64, usize>> {
        self.group_distribution.get(group_key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_anti_affinity_rule_builders() {
        let hard_rule = AntiAffinityRule::hard("web-app");
        assert!(matches!(hard_rule.rule_type, AntiAffinityType::Hard));
        assert_eq!(hard_rule.group_key, "web-app");
        assert_eq!(hard_rule.max_per_node, 1);

        let soft_rule = AntiAffinityRule::soft("cache", 0.5)
            .with_max_per_node(2)
            .with_scope("rack");

        if let AntiAffinityType::Soft { weight } = soft_rule.rule_type {
            assert_eq!(weight, 0.5);
        } else {
            panic!("Expected soft rule");
        }
        assert_eq!(soft_rule.max_per_node, 2);
        assert_eq!(soft_rule.scope.as_deref(), Some("rack"));
    }

    #[test]
    fn test_anti_affinity_checker() {
        // Current distribution:
        // - web-app: node1=2, node2=1
        // - database: node1=1, node3=1
        let distribution = vec![
            ("web1".to_string(), vec!["web-app".to_string()], 1),
            ("web2".to_string(), vec!["web-app".to_string()], 1),
            ("web3".to_string(), vec!["web-app".to_string()], 2),
            ("db1".to_string(), vec!["database".to_string()], 1),
            ("db2".to_string(), vec!["database".to_string()], 3),
        ];

        let checker = AntiAffinityChecker::new(distribution);

        // Test hard constraint check
        let rules = AntiAffinityRules::new()
            .add_rule(AntiAffinityRule::hard("web-app").with_max_per_node(2));

        // Node 1 already has 2 web-app VMs (at limit)
        assert!(checker.check_hard_constraints(&rules, 1).is_err());
        // Node 2 has 1 web-app VM (under limit)
        assert!(checker.check_hard_constraints(&rules, 2).is_ok());
        // Node 3 has 0 web-app VMs
        assert!(checker.check_hard_constraints(&rules, 3).is_ok());

        // Test soft constraint penalties
        let soft_rules = AntiAffinityRules::new().add_rule(AntiAffinityRule::soft("web-app", 1.0));

        // Node 1: 2 VMs (1 over preferred max of 1) = penalty 2.0
        assert_eq!(
            checker.calculate_soft_constraint_penalty(&soft_rules, 1),
            2.0
        );
        // Node 2: 1 VM (at preferred max) = penalty 1.0
        assert_eq!(
            checker.calculate_soft_constraint_penalty(&soft_rules, 2),
            1.0
        );
        // Node 3: 0 VMs = no penalty
        assert_eq!(
            checker.calculate_soft_constraint_penalty(&soft_rules, 3),
            0.0
        );
    }
}
