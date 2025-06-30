# RBAC (Role-Based Access Control) Guide

This guide explains how to configure and use Role-Based Access Control in Blixard for fine-grained permission management.

## Overview

Blixard's RBAC system provides:
- **Multi-tenant isolation** - Complete separation between tenants
- **Role-based permissions** - Predefined roles with specific permissions
- **Resource-specific access** - Grant access to individual resources
- **Dynamic permission management** - Update permissions in real-time
- **Audit capabilities** - Track who has access to what

## Architecture

The RBAC system uses Casbin, a powerful authorization library that supports:
- ACL (Access Control List)
- RBAC (Role-Based Access Control)
- ABAC (Attribute-Based Access Control)
- Domain/Tenant isolation

### Permission Model

```
Subject (User/Role) + Domain (Tenant) + Object (Resource) + Action = Allow/Deny
```

## Default Roles

### Admin
- Full access to all resources in their tenant
- Can manage users and roles
- Can perform cluster administration

### Operator
- Create, read, update, delete VMs
- Manage tasks
- Read cluster status
- Cannot perform admin operations

### Viewer
- Read-only access to VMs, tasks, cluster status
- Access to metrics
- Cannot modify any resources

### Metrics
- Read-only access to metrics endpoints
- Designed for monitoring systems

## Configuration

### 1. Enable RBAC in Configuration

```toml
# /etc/blixard/config.toml
[security.rbac]
enabled = true
model_file = "/etc/blixard/rbac_model.conf"
policy_file = "/etc/blixard/rbac_policy.csv"
```

### 2. Default RBAC Model

The default model supports domain-based multi-tenancy:

```ini
[request_definition]
r = sub, dom, obj, act

[policy_definition]
p = sub, dom, obj, act

[role_definition]
g = _, _, _

[policy_effect]
e = some(where (p.eft == allow))

[matchers]
m = g(r.sub, p.sub, r.dom) && r.dom == p.dom && r.obj == p.obj && r.act == p.act || \
    g(r.sub, p.sub, r.dom) && r.dom == p.dom && p.obj == "*" && r.act == p.act || \
    r.sub == "root" || \
    g(r.sub, "admin", r.dom)
```

### 3. Policy File Format

```csv
# Roles (g = user, role, domain)
g, alice, admin, tenant1
g, bob, operator, tenant1
g, charlie, viewer, tenant1

# Policies (p = subject, domain, object, action)
p, admin, *, *, admin
p, operator, *, vm, *
p, operator, *, task, *
p, viewer, *, vm, read
p, viewer, *, task, read
p, viewer, *, cluster, read
```

## Usage Examples

### 1. Basic Role Assignment

```bash
# Assign admin role to user in tenant
blixard rbac add-role --user alice --role admin --tenant acme

# Assign operator role
blixard rbac add-role --user bob --role operator --tenant acme

# Remove role
blixard rbac remove-role --user bob --role operator --tenant acme
```

### 2. Resource-Specific Permissions

```bash
# Grant user access to specific VM
blixard rbac grant --user charlie --resource vm:web-server-1 --action write --tenant acme

# Grant access to all VMs matching pattern
blixard rbac grant --user david --resource "vm:prod-*" --action read --tenant acme

# Revoke specific permission
blixard rbac revoke --user charlie --resource vm:web-server-1 --action write --tenant acme
```

### 3. Custom Roles

```bash
# Create custom role with specific permissions
blixard rbac create-role --name vm-reader --tenant acme
blixard rbac add-permission --role vm-reader --resource vm --action read --tenant acme
blixard rbac add-permission --role vm-reader --resource metrics --action read --tenant acme

# Assign custom role to user
blixard rbac add-role --user eve --role vm-reader --tenant acme
```

### 4. Multi-Tenant Setup

```bash
# Create tenant isolation
blixard rbac create-tenant --name acme
blixard rbac create-tenant --name techcorp

# Users in different tenants are completely isolated
blixard rbac add-role --user alice --role admin --tenant acme
blixard rbac add-role --user alice --role viewer --tenant techcorp
```

## Integration with Authentication

RBAC works seamlessly with the authentication system:

1. User authenticates with token
2. Token contains user identity
3. RBAC checks permissions based on:
   - User identity
   - Tenant ID (from x-tenant-id header)
   - Requested resource
   - Requested action

### Headers for Multi-Tenancy

```bash
# Include tenant ID in requests
curl -H "Authorization: Bearer $TOKEN" \
     -H "x-tenant-id: acme" \
     https://blixard-api/v1/vms
```

## Permission Checks in Code

### Using RBAC Manager

```rust
use blixard_core::rbac::{RbacManager, Action};

// Check if user can write to VMs in tenant
let can_write = rbac.enforce(
    "alice",
    "vm",
    &Action::Write,
    Some("acme"),
).await?;

// Check specific resource
let can_access = rbac.enforce(
    "bob",
    "vm:web-server-1",
    &Action::Read,
    Some("acme"),
).await?;
```

### Using Permission Checker

```rust
use blixard_core::rbac::ResourcePermissionChecker;

let checker = ResourcePermissionChecker::new(rbac);

// Check VM access
let can_manage = checker.can_access_vm(
    "charlie",
    "database-1",
    &Action::Write,
    Some("acme"),
).await?;

// Check node access
let can_admin = checker.can_manage_node(
    "alice",
    "node-1",
    &Action::Admin,
).await?;
```

## Best Practices

### 1. Principle of Least Privilege
- Grant minimum permissions required
- Use roles instead of direct permissions
- Regularly audit permissions

### 2. Resource Naming Conventions
- Use consistent naming: `resource_type:identifier`
- Examples: `vm:web-1`, `node:compute-1`, `task:backup-123`
- Support wildcards carefully: `vm:prod-*`

### 3. Tenant Isolation
- Always specify tenant/domain in multi-tenant setups
- Use separate policy files per tenant if needed
- Audit cross-tenant access regularly

### 4. Regular Audits
```bash
# List all admins
blixard rbac list-users --role admin --tenant acme

# List user permissions
blixard rbac list-permissions --user alice --tenant acme

# Export policies for review
blixard rbac export-policies --output policies.csv
```

## Troubleshooting

### Permission Denied Errors

1. **Check user roles**
   ```bash
   blixard rbac list-roles --user alice --tenant acme
   ```

2. **Verify resource permissions**
   ```bash
   blixard rbac check --user alice --resource vm:web-1 --action write --tenant acme
   ```

3. **Enable debug logging**
   ```bash
   export RUST_LOG=blixard_core::rbac=debug
   ```

### Common Issues

#### "User has role but no access"
- Check tenant/domain matches
- Verify policy file is loaded
- Ensure resource name format is correct

#### "Permissions not updating"
- Reload policies: `blixard rbac reload`
- Check policy file syntax
- Verify RBAC is enabled in config

## Performance Considerations

- Policy evaluation is very fast (microseconds)
- Cache frequently checked permissions
- Use role inheritance to reduce policy count
- Consider policy file size for large deployments

## Migration Guide

### From Simple Permissions to RBAC

1. **Map existing permissions to roles**
   - `Permission::Admin` → admin role
   - `Permission::VmWrite` → operator role
   - `Permission::VmRead` → viewer role

2. **Assign roles to existing users**
   ```bash
   # Export existing users
   blixard auth export-users > users.csv
   
   # Import with roles
   blixard rbac import-users --file users.csv --default-role viewer
   ```

3. **Enable RBAC gradually**
   ```toml
   [security.rbac]
   enabled = true
   fallback_to_simple = true  # Use old permissions if RBAC fails
   ```

4. **Test thoroughly** before disabling fallback

## Advanced Features

### Attribute-Based Access Control (ABAC)

Add attributes to policies for more complex rules:

```csv
# Time-based access
p, alice, acme, vm, write, time.Hour >= 9 && time.Hour <= 17

# Resource tag-based access
p, bob, acme, vm, write, resource.tags.contains("production")
```

### Dynamic Policies

Load policies from external sources:

```rust
// Load from database
rbac.load_policies_from_db().await?;

// Load from API
rbac.load_policies_from_url("https://auth.company.com/policies").await?;
```

### Policy Inheritance

Create role hierarchies:

```csv
# Manager inherits from operator
g, manager, operator, acme
# Manager additional permissions
p, manager, acme, budget, write
```

## Security Considerations

1. **Protect policy files** - Use appropriate file permissions
2. **Audit policy changes** - Log all modifications
3. **Regular reviews** - Audit permissions quarterly
4. **Separation of duties** - Don't let users manage their own permissions
5. **Default deny** - Explicitly grant permissions, deny by default