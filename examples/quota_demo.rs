//! Resource quota demonstration example
//!
//! This example shows how resource quotas and rate limiting work in Blixard.

use blixard_core::{
    quota_system::{QuotaManager, ResourceType, TenantQuota, RateLimit},
    types::VmConfig,
    error::BlixardResult,
};
use std::sync::Arc;
use std::collections::HashMap;
use chrono::Utc;

#[tokio::main]
async fn main() -> BlixardResult<()> {
    // Initialize logging
    tracing_subscriber::fmt::init();
    
    println!("=== Blixard Resource Quota Demo ===\n");
    
    // Create quota manager
    println!("1. Creating quota manager...");
    let quota_manager = Arc::new(QuotaManager::new());
    
    // Demo 1: Basic tenant setup with default quotas
    println!("\n2. Setting up tenants with default quotas...");
    quota_manager.init_tenant("startup-a", None).await?;
    quota_manager.init_tenant("startup-b", None).await?;
    
    let usage_a = quota_manager.get_tenant_usage("startup-a").await?;
    println!("   Startup A default quotas:");
    for resource in &usage_a.resources {
        println!("   - {:?}: {}/{} ({}%)", 
            resource.resource_type, 
            resource.used, 
            resource.limit,
            resource.usage_percent as u32
        );
    }
    
    // Demo 2: Custom quotas for enterprise tenant
    println!("\n3. Creating enterprise tenant with higher quotas...");
    let mut enterprise_quota = TenantQuota::new("enterprise-corp".to_string());
    
    // Set higher limits
    enterprise_quota.quotas.insert(
        ResourceType::Cpu,
        blixard_core::quota_system::ResourceQuota::new(ResourceType::Cpu, 500, 400),
    );
    enterprise_quota.quotas.insert(
        ResourceType::Memory,
        blixard_core::quota_system::ResourceQuota::new(ResourceType::Memory, 512000, 409600), // 500GB
    );
    enterprise_quota.quotas.insert(
        ResourceType::VmCount,
        blixard_core::quota_system::ResourceQuota::new(ResourceType::VmCount, 200, 160),
    );
    
    quota_manager.init_tenant("enterprise-corp", Some(enterprise_quota)).await?;
    
    // Demo 3: Resource allocation lifecycle
    println!("\n4. Demonstrating resource allocation lifecycle...");
    
    // Create a VM configuration
    let vm1 = VmConfig {
        name: "web-server-1".to_string(),
        config_path: "/test".to_string(),
        vcpus: 4,
        memory: 8192, // 8GB
        tenant_id: "startup-a".to_string(),
        ip_address: None,
        metadata: None,
        anti_affinity: None,
        priority: 500,
        preemptible: false,
        locality_preference: Default::default(),
        health_check_config: None,
    };
    
    // Check quota before allocation
    let check_result = quota_manager.check_quota("startup-a", &vm1).await?;
    println!("   Quota check for web-server-1: {}", 
        if check_result.allowed { "ALLOWED" } else { "DENIED" }
    );
    
    if check_result.allowed {
        // Reserve resources
        let mut resources = HashMap::new();
        resources.insert(ResourceType::Cpu, vm1.vcpus as u64);
        resources.insert(ResourceType::Memory, vm1.memory as u64);
        resources.insert(ResourceType::VmCount, 1);
        
        quota_manager.reserve_resources("startup-a", &vm1.name, resources.clone()).await?;
        println!("   ✓ Resources reserved for web-server-1");
        
        // Check usage after reservation
        let usage = quota_manager.get_tenant_usage("startup-a").await?;
        let cpu_usage = usage.resources.iter()
            .find(|r| r.resource_type == ResourceType::Cpu)
            .unwrap();
        println!("   CPU after reservation: {}/{} reserved, {}/{} used", 
            cpu_usage.reserved, cpu_usage.limit, cpu_usage.used, cpu_usage.limit);
        
        // Simulate VM creation success - commit resources
        quota_manager.commit_resources("startup-a", &vm1.name, resources.clone()).await?;
        println!("   ✓ Resources committed for web-server-1");
        
        // Check usage after commit
        let usage = quota_manager.get_tenant_usage("startup-a").await?;
        let cpu_usage = usage.resources.iter()
            .find(|r| r.resource_type == ResourceType::Cpu)
            .unwrap();
        println!("   CPU after commit: {}/{} reserved, {}/{} used", 
            cpu_usage.reserved, cpu_usage.limit, cpu_usage.used, cpu_usage.limit);
    }
    
    // Demo 4: Multiple VMs and approaching limits
    println!("\n5. Creating multiple VMs and approaching quota limits...");
    
    let vms = vec![
        ("db-server-1", 8, 16384),
        ("cache-server-1", 2, 4096),
        ("worker-1", 4, 8192),
        ("worker-2", 4, 8192),
    ];
    
    for (name, vcpus, memory) in vms {
        let vm_config = VmConfig {
            name: name.to_string(),
            config_path: "/test".to_string(),
            vcpus,
            memory,
            tenant_id: "startup-a".to_string(),
            ip_address: None,
            metadata: None,
            anti_affinity: None,
            priority: 500,
            preemptible: false,
            locality_preference: Default::default(),
            health_check_config: None,
        };
        
        let check_result = quota_manager.check_quota("startup-a", &vm_config).await?;
        
        if check_result.allowed {
            let mut resources = HashMap::new();
            resources.insert(ResourceType::Cpu, vcpus as u64);
            resources.insert(ResourceType::Memory, memory as u64);
            resources.insert(ResourceType::VmCount, 1);
            
            quota_manager.reserve_resources("startup-a", name, resources.clone()).await?;
            quota_manager.commit_resources("startup-a", name, resources).await?;
            println!("   ✓ Created VM: {} ({}vCPU, {}MB)", name, vcpus, memory);
        } else {
            println!("   ✗ Cannot create VM {}: {}", name, 
                check_result.reason.unwrap_or_else(|| "Quota exceeded".to_string()));
        }
    }
    
    // Show current usage
    let usage = quota_manager.get_tenant_usage("startup-a").await?;
    println!("\n   Current usage for startup-a:");
    for resource in &usage.resources {
        let warning = if resource.at_warning { " ⚠️  WARNING" } else { "" };
        println!("   - {:?}: {}/{} ({}%){}", 
            resource.resource_type, 
            resource.used, 
            resource.limit,
            resource.usage_percent as u32,
            warning
        );
    }
    
    // Demo 5: Rate limiting
    println!("\n6. Demonstrating rate limiting...");
    
    // Add rate limit: 5 VM creations per minute
    quota_manager.add_rate_limit("startup-b", "vm_create", 5, 60).await?;
    println!("   Added rate limit: 5 VM creations per 60 seconds for startup-b");
    
    // Try to create VMs rapidly
    for i in 1..=7 {
        let allowed = quota_manager.check_rate_limit("startup-b", "vm_create").await?;
        
        if allowed {
            quota_manager.record_rate_limited_operation("startup-b", "vm_create").await?;
            println!("   ✓ VM creation {} allowed", i);
        } else {
            println!("   ✗ VM creation {} RATE LIMITED", i);
        }
    }
    
    // Demo 6: Resource cleanup
    println!("\n7. Cleaning up resources (deleting VMs)...");
    
    // Release resources for web-server-1
    let mut resources = HashMap::new();
    resources.insert(ResourceType::Cpu, 4);
    resources.insert(ResourceType::Memory, 8192);
    resources.insert(ResourceType::VmCount, 1);
    
    quota_manager.release_resources("startup-a", "web-server-1", resources).await?;
    println!("   ✓ Released resources for web-server-1");
    
    // Show updated usage
    let usage = quota_manager.get_tenant_usage("startup-a").await?;
    let cpu_usage = usage.resources.iter()
        .find(|r| r.resource_type == ResourceType::Cpu)
        .unwrap();
    println!("   CPU after release: {}/{} used", cpu_usage.used, cpu_usage.limit);
    
    // Demo 7: Usage history
    println!("\n8. Viewing usage history...");
    let history = quota_manager.get_usage_history("startup-a", Some(ResourceType::Cpu), 5).await;
    
    println!("   Recent CPU usage events for startup-a:");
    for record in history {
        let operation_type = if record.amount > 0 { "allocated" } else { "released" };
        println!("   - {} {} CPU(s) for {} at {}", 
            operation_type, 
            record.amount.abs(), 
            record.vm_id.unwrap_or_else(|| "N/A".to_string()),
            record.timestamp.format("%H:%M:%S")
        );
    }
    
    // Demo 8: Dynamic quota adjustment
    println!("\n9. Dynamically adjusting quotas...");
    
    // Increase CPU quota for startup-a
    quota_manager.set_tenant_quota("startup-a", ResourceType::Cpu, 150, 120).await?;
    println!("   ✓ Increased startup-a CPU quota to 150 (warning at 120)");
    
    // Now we can create more VMs
    let big_vm = VmConfig {
        name: "analytics-server".to_string(),
        config_path: "/test".to_string(),
        vcpus: 32,
        memory: 65536, // 64GB
        tenant_id: "startup-a".to_string(),
        ip_address: None,
        metadata: None,
        anti_affinity: None,
        priority: 700,
        preemptible: false,
        locality_preference: Default::default(),
        health_check_config: None,
    };
    
    let check_result = quota_manager.check_quota("startup-a", &big_vm).await?;
    println!("   Analytics server (32 vCPU) quota check: {}", 
        if check_result.allowed { "ALLOWED" } else { "DENIED" }
    );
    
    println!("\n=== Resource Quota Demo Complete ===");
    println!("\nKey concepts demonstrated:");
    println!("- Multi-tenant resource isolation");
    println!("- Resource reservation and commit pattern");
    println!("- Quota enforcement and warning thresholds");
    println!("- Rate limiting for API operations");
    println!("- Dynamic quota adjustment");
    println!("- Usage tracking and history");
    
    Ok(())
}