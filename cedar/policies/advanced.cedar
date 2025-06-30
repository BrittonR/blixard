// Advanced policies for Blixard
// These policies implement sophisticated access control including multi-tenancy,
// resource quotas, time-based access, and priority-based protection

// Multi-tenancy: Users can only access resources in their tenant
// This is a fundamental isolation policy
permit(
    principal,
    action,
    resource
) when {
    principal.tenant_id == resource.tenant_id
};

// Resource quotas: Deny VM creation if tenant quota exceeded
// Prevents resource exhaustion by enforcing limits
forbid(
    principal,
    action == Action::"createVM",
    resource in Tenant
) when {
    resource.quota_vms <= context.current_vm_count
};

// CPU quota enforcement
forbid(
    principal,
    action == Action::"createVM",
    resource in Tenant
) when {
    context.requested_cpu > 0 &&
    resource.quota_cpu < (context.current_cpu_usage + context.requested_cpu)
};

// Memory quota enforcement
forbid(
    principal,
    action == Action::"createVM",
    resource in Tenant
) when {
    context.requested_memory > 0 &&
    resource.quota_memory < (context.current_memory_usage + context.requested_memory)
};

// Time-based access: Operators can only delete VMs during maintenance windows
// Maintenance window is 10 PM to 6 AM
forbid(
    principal in Role::"operator",
    action == Action::"deleteVM",
    resource
) unless {
    context.hour >= 22 || context.hour <= 6
};

// Priority-based protection: Prevent deletion of high-priority VMs by non-admins
// VMs with priority >= 900 are considered critical
forbid(
    principal,
    action == Action::"deleteVM",
    resource
) when {
    resource has priority &&
    resource.priority >= 900 && 
    !(principal in Role::"admin")
};

// Node capacity enforcement: Prevent VM creation on nodes without capacity
forbid(
    principal,
    action == Action::"createVM",
    resource in Node
) when {
    context.requested_cpu > 0 &&
    resource.available_cpu < context.requested_cpu
};

forbid(
    principal,
    action == Action::"createVM",
    resource in Node
) when {
    context.requested_memory > 0 &&
    resource.available_memory < context.requested_memory
};

// Preemptible VM protection: Only admins can delete non-preemptible VMs
forbid(
    principal,
    action == Action::"deleteVM",
    resource
) when {
    resource has preemptible &&
    resource.preemptible == false && 
    !(principal in Role::"admin")
};

// Failed VM cleanup: Allow operators to delete failed VMs anytime
permit(
    principal in Role::"operator",
    action == Action::"deleteVM",
    resource
) when {
    resource has state &&
    resource.state == "Failed"
};

// Emergency access: Admins can always perform critical operations
// This overrides time-based restrictions for admins
permit(
    principal in Role::"admin",
    action in [
        Action::"deleteVM",
        Action::"updateVM",
        Action::"manageCluster",
        Action::"manageNode"
    ],
    resource
) when {
    context has emergency &&
    context.emergency == true
};

// Audit access: Allow read access to metrics for compliance
permit(
    principal in Role::"auditor",
    action == Action::"readMetrics",
    resource
);

// Service account access: Allow automated systems to perform specific actions
permit(
    principal in Role::"service-account",
    action in [
        Action::"readVM",
        Action::"readMetrics",
        Action::"readNode"
    ],
    resource
) when {
    principal has service_name &&
    principal.service_name == "monitoring-system"
};