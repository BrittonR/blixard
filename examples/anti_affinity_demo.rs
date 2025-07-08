//! Example demonstrating anti-affinity rules for VM placement
//!
//! This example shows how to use anti-affinity rules to ensure VMs are
//! spread across different nodes for high availability.

use blixard_core::{
    anti_affinity::{AntiAffinityRule, AntiAffinityRules},
    types::VmConfig,
    vm_scheduler::PlacementStrategy,
};
use std::collections::HashMap;
use tracing::info;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("blixard=info".parse()?),
        )
        .init();

    info!("Anti-affinity Rules Demo");
    info!("========================\n");

    // Example 1: Hard anti-affinity for web servers
    info!("Example 1: Web server high availability");
    info!("---------------------------------------");

    let web_anti_affinity =
        AntiAffinityRules::new().add_rule(AntiAffinityRule::hard("web-frontend"));

    let web_vm1 = VmConfig {
        name: "web-server-1".to_string(),
        config_path: "/etc/vms/web.nix".to_string(),
        vcpus: 4,
        memory: 8192,
        tenant_id: "prod-tenant".to_string(),
        ip_address: Some("10.0.1.10".to_string()),
        metadata: None,
        anti_affinity: Some(web_anti_affinity.clone()),
        priority: 700,
        preemptible: false,
        locality_preference: Default::default(),
        health_check_config: None,
    };

    info!(
        "VM: {} with hard anti-affinity group 'web-frontend'",
        web_vm1.name
    );
    info!("This ensures each web server runs on a different node");
    info!("");

    // Example 2: Soft anti-affinity for cache layers
    info!("Example 2: Cache layer distribution");
    info!("-----------------------------------");

    let cache_anti_affinity =
        AntiAffinityRules::new().add_rule(AntiAffinityRule::soft("cache-layer", 0.8));

    let cache_vm = VmConfig {
        name: "redis-cache-1".to_string(),
        config_path: "/etc/vms/redis.nix".to_string(),
        vcpus: 2,
        memory: 4096,
        tenant_id: "prod-tenant".to_string(),
        ip_address: Some("10.0.2.10".to_string()),
        metadata: None,
        anti_affinity: Some(cache_anti_affinity),
        priority: 500,
        preemptible: true,
        locality_preference: Default::default(),
        health_check_config: None,
    };

    info!("VM: {} with soft anti-affinity (weight=0.8)", cache_vm.name);
    info!("Prefers spreading cache instances but allows co-location if needed");
    info!("");

    // Example 3: Database with max instances per node
    info!("Example 3: Database replicas with max per node");
    info!("----------------------------------------------");

    let db_anti_affinity = AntiAffinityRules::new()
        .add_rule(AntiAffinityRule::hard("postgres-cluster").with_max_per_node(2));

    let db_vm = VmConfig {
        name: "postgres-replica-1".to_string(),
        config_path: "/etc/vms/postgres.nix".to_string(),
        vcpus: 8,
        memory: 16384,
        tenant_id: "prod-tenant".to_string(),
        ip_address: Some("10.0.3.10".to_string()),
        metadata: None,
        anti_affinity: Some(db_anti_affinity),
        priority: 900,
        preemptible: false,
        locality_preference: Default::default(),
        health_check_config: None,
    };

    info!("VM: {} with max 2 instances per node", db_vm.name);
    info!("Allows up to 2 database replicas per node for balanced distribution");
    info!("");

    // Example 4: Multiple anti-affinity rules
    info!("Example 4: Multiple anti-affinity constraints");
    info!("---------------------------------------------");

    let multi_anti_affinity = AntiAffinityRules::new()
        .add_rule(AntiAffinityRule::hard("app-tier"))
        .add_rule(AntiAffinityRule::soft("customer-xyz", 1.5))
        .add_rule(AntiAffinityRule::hard("region-us-east").with_scope("datacenter"));

    let app_vm = VmConfig {
        name: "app-server-1".to_string(),
        config_path: "/etc/vms/app.nix".to_string(),
        vcpus: 4,
        memory: 8192,
        tenant_id: "customer-xyz".to_string(),
        ip_address: Some("10.0.4.10".to_string()),
        metadata: None,
        anti_affinity: Some(multi_anti_affinity),
        priority: 600,
        preemptible: false,
        locality_preference: Default::default(),
        health_check_config: None,
    };

    info!("VM: {} with multiple constraints:", app_vm.name);
    info!("  - Hard: Must not share node with other 'app-tier' VMs");
    info!("  - Soft: Prefer not to share with other 'customer-xyz' VMs (weight=1.5)");
    info!("  - Hard: Must not share datacenter with other 'region-us-east' VMs");
    info!("");

    // Example 5: Placement strategies with anti-affinity
    info!("Example 5: Combining placement strategies with anti-affinity");
    info!("------------------------------------------------------------");

    let strategies = vec![
        PlacementStrategy::MostAvailable,
        PlacementStrategy::LeastAvailable,
        PlacementStrategy::RoundRobin,
        PlacementStrategy::Manual { node_id: 5 },
    ];

    for strategy in strategies {
        info!("Strategy: {:?}", strategy);
        info!("Anti-affinity rules are evaluated after resource filtering");
        info!("- Hard constraints eliminate nodes from consideration");
        info!("- Soft constraints adjust the placement score");
        info!("");
    }

    // Example 6: Optional anti-affinity
    info!("Example 6: Optional anti-affinity for flexibility");
    info!("-------------------------------------------------");

    let optional_vm_with_rules = VmConfig {
        name: "flexible-vm-1".to_string(),
        config_path: "/etc/vms/generic.nix".to_string(),
        vcpus: 2,
        memory: 4096,
        tenant_id: "default".to_string(),
        ip_address: None,
        metadata: None,
        anti_affinity: Some(
            AntiAffinityRules::new().add_rule(AntiAffinityRule::soft("optional-spread", 0.5)),
        ),
        priority: 500,
        preemptible: true,
        locality_preference: Default::default(),
        health_check_config: None,
    };

    let optional_vm_without_rules = VmConfig {
        name: "flexible-vm-2".to_string(),
        config_path: "/etc/vms/generic.nix".to_string(),
        vcpus: 2,
        memory: 4096,
        tenant_id: "default".to_string(),
        ip_address: None,
        metadata: None,
        anti_affinity: None, // No anti-affinity rules
        priority: 500,
        preemptible: true,
        locality_preference: Default::default(),
        health_check_config: None,
    };

    info!(
        "VM '{}': Has optional soft anti-affinity",
        optional_vm_with_rules.name
    );
    info!(
        "VM '{}': No anti-affinity constraints",
        optional_vm_without_rules.name
    );
    info!("Both approaches are valid depending on requirements");
    info!("");

    // Example 7: Metadata for grouping
    info!("Example 7: Using metadata with anti-affinity groups");
    info!("---------------------------------------------------");

    let mut metadata = HashMap::new();
    metadata.insert("app_version".to_string(), "v2.1.0".to_string());
    metadata.insert("deployment".to_string(), "blue".to_string());

    let versioned_anti_affinity =
        AntiAffinityRules::new().add_rule(AntiAffinityRule::hard("app-v2.1.0-blue"));

    let versioned_vm = VmConfig {
        name: "app-v2-1".to_string(),
        config_path: "/etc/vms/app-v2.nix".to_string(),
        vcpus: 4,
        memory: 8192,
        tenant_id: "prod-tenant".to_string(),
        ip_address: None,
        metadata: Some(metadata),
        anti_affinity: Some(versioned_anti_affinity),
        priority: 600,
        preemptible: false,
        locality_preference: Default::default(),
        health_check_config: None,
    };

    info!(
        "VM: {} with version-specific anti-affinity",
        versioned_vm.name
    );
    info!("Metadata: {:?}", versioned_vm.metadata);
    info!("Useful for blue-green deployments or canary releases");

    info!("\nAnti-affinity demo completed!");
    Ok(())
}
