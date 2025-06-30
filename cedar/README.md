# Cedar Policy Engine for Blixard

This directory contains the Cedar policy definitions and schema for Blixard's authorization system.

## Overview

Cedar is a language for writing and enforcing authorization policies. It provides:

- **Formal Verification**: Policies can be mathematically proven correct
- **Fine-grained Access Control**: Support for RBAC, ABAC, and complex conditional logic
- **Performance**: Sub-millisecond policy evaluation
- **Safety**: Strong typing and validation prevent common policy mistakes

## Directory Structure

```
cedar/
├── schema.cedarschema.json    # Cedar schema defining entities and actions
├── policies/
│   ├── base_roles.cedar      # Basic RBAC policies (admin, operator, viewer)
│   └── advanced.cedar        # Advanced policies (quotas, time-based, etc.)
└── README.md                 # This file
```

## Schema

The schema (`schema.cedarschema.json`) defines:

### Entity Types
- **User**: System users with email, tenant_id, and creation timestamp
- **Role**: Authorization roles (admin, operator, viewer, etc.)
- **Tenant**: Multi-tenancy support with resource quotas
- **VM**: Virtual machines with resource allocation and state
- **Node**: Physical/virtual nodes in the cluster
- **Cluster**: Cluster-level resources

### Actions
- **Cluster Operations**: readCluster, manageCluster, joinCluster, leaveCluster
- **Node Operations**: readNode, manageNode
- **VM Operations**: createVM, readVM, updateVM, deleteVM, executeVM
- **Monitoring**: readMetrics, manageBackups

## Policies

### Base Roles (`base_roles.cedar`)

Implements traditional RBAC with three roles:

1. **Admin**: Full system access
2. **Operator**: VM management and cluster read access
3. **Viewer**: Read-only access

### Advanced Policies (`advanced.cedar`)

Implements sophisticated access control:

1. **Multi-tenancy Isolation**: Users can only access resources in their tenant
2. **Resource Quotas**: Enforce CPU, memory, and VM count limits
3. **Time-based Access**: Restrict destructive operations to maintenance windows
4. **Priority Protection**: Prevent deletion of high-priority resources
5. **Capacity Enforcement**: Block VM creation when nodes lack resources
6. **Emergency Override**: Allow admins to bypass restrictions in emergencies

## Usage Examples

### Basic Authorization Check

```rust
// Check if a user can read a cluster
let authorized = cedar.is_authorized(
    "User::\"alice\"",
    "Action::\"readCluster\"",
    "Cluster::\"prod\"",
    HashMap::new(),
).await?;
```

### Authorization with Context

```rust
// Check VM deletion with time context
let mut context = HashMap::new();
context.insert("hour".to_string(), json!(14)); // 2 PM

let authorized = cedar.is_authorized(
    "User::\"bob\"",
    "Action::\"deleteVM\"",
    "VM::\"vm-123\"",
    context,
).await?;
```

### Resource Quota Check

```rust
// Check if VM creation would exceed quotas
let mut context = HashMap::new();
context.insert("current_vm_count".to_string(), json!(45));
context.insert("requested_cpu".to_string(), json!(4));
context.insert("current_cpu_usage".to_string(), json!(800));

let authorized = cedar.is_authorized(
    "User::\"operator\"",
    "Action::\"createVM\"",
    "Tenant::\"acme\"",
    context,
).await?;
```

## Writing New Policies

### Policy Structure

Cedar policies follow this general structure:

```cedar
permit|forbid(
    principal [in EntityType],
    action [== Action | in [Actions]],
    resource [in EntityType]
) when {
    // Optional conditions
} unless {
    // Optional exception conditions
};
```

### Best Practices

1. **Start with Deny-by-Default**: Only permit what's explicitly allowed
2. **Use Forbid Sparingly**: Forbid rules override permits, use carefully
3. **Test Policies**: Always test new policies with various scenarios
4. **Document Intent**: Add comments explaining why a policy exists
5. **Version Control**: Track policy changes in Git

### Example: Adding a New Role

```cedar
// DevOps Role - Can manage infrastructure but not delete production VMs
permit(
    principal in Role::"devops",
    action in [
        Action::"readCluster", 
        Action::"readNode",
        Action::"readVM",
        Action::"createVM",
        Action::"updateVM",
        Action::"readMetrics"
    ],
    resource
);

// Prevent DevOps from deleting production VMs
forbid(
    principal in Role::"devops",
    action == Action::"deleteVM",
    resource
) when {
    resource has environment &&
    resource.environment == "production"
};
```

## Validation

### Schema Validation

Policies are validated against the schema to ensure:
- Entity types exist
- Actions are defined
- Attributes are correctly typed

### Policy Testing

Use the test suite to validate policies:

```bash
cargo test cedar_policy_tests
```

### Coverage Check

Ensure all actions are covered:

```rust
let uncovered = cedar.validate_policy_coverage().await?;
```

## Migration from RBAC

To migrate from the existing RBAC system:

1. **Map Permissions**: Convert Permission enum values to Cedar actions
2. **Create Entities**: Convert users and roles to Cedar entities
3. **Test in Shadow Mode**: Run Cedar alongside existing RBAC
4. **Compare Decisions**: Ensure Cedar matches existing behavior
5. **Switch Over**: Enable Cedar as primary authorization

## Troubleshooting

### Common Issues

1. **"Invalid principal format"**: Ensure entity UIDs follow format `Type::"id"`
2. **"Policy validation failed"**: Check policy syntax and schema compliance
3. **"Unexpected deny"**: Check forbid rules and context values
4. **"Missing attributes"**: Ensure entities have required attributes

### Debugging

Enable debug logging to see authorization decisions:

```rust
RUST_LOG=blixard_core::cedar_authz=debug cargo run
```

## Security Considerations

1. **Policy Review**: All policy changes should be reviewed
2. **Principle of Least Privilege**: Grant minimum necessary permissions
3. **Regular Audits**: Review and update policies regularly
4. **Emergency Access**: Ensure break-glass procedures exist
5. **Logging**: Log all authorization decisions for audit

## Future Enhancements

1. **Policy Templates**: Reusable policy patterns
2. **Dynamic Policies**: Runtime policy updates without restart
3. **Policy Analytics**: Track which policies are used/unused
4. **Integration with External Identity Providers**: OIDC/SAML support
5. **Policy Simulation**: Test policy changes before deployment