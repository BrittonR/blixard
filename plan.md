# Blixard Development Plan

## Recent Accomplishments (January 2025)

### ✅ Three-Node Cluster Formation Fixed

Successfully resolved critical issues preventing proper three-node cluster formation:

1. **Raft Message Channel Resilience** 
   - Fixed "Failed to send outgoing Raft message" errors
   - Always spawn outgoing message handler even if transport fails
   - Proper error handling for unbounded channels

2. **P2P Connection Info Propagation**
   - Extended RaftConfChange to include full P2P details
   - All nodes now receive peer node IDs, addresses, and relay URLs
   - Enables proper peer discovery in distributed clusters

3. **Worker Registration Timing**
   - Wait for leader identification before registration
   - Replace fixed delays with condition-based waiting
   - Continue operation even if registration fails

4. **Comprehensive Testing**
   - Created three_node_cluster_comprehensive_test.rs
   - Manual test script for cluster verification
   - Documented fixes in CLUSTER_FIXES_TESTING_STATUS.md

See commits: 60252f0, 89928b3, 5e41608, 7df723c, b6bc14e

## Current Focus Areas

### 1. Production Hardening
- [ ] Fix remaining test suite compilation issues
- [ ] Add integration tests for cluster scenarios
- [ ] Implement graceful shutdown procedures
- [ ] Add cluster health monitoring

### 2. Performance Optimization
- [ ] Connection pooling for Iroh P2P
- [ ] Batch Raft proposals for efficiency
- [ ] Optimize state machine operations
- [ ] Profile and optimize hot paths

### 3. Observability Enhancement
- [ ] Complete distributed tracing implementation
- [ ] Add cluster-wide metrics aggregation
- [ ] Implement log correlation across nodes
- [ ] Create operational dashboards

## Upcoming Major Features

### Cedar Policy Engine Migration

This migration will replace Blixard's current custom RBAC implementation with AWS Cedar Policy Engine, providing fine-grained, attribute-based access control with formal verification capabilities.

## Why Cedar?

1. **Formal Verification**: Cedar policies can be mathematically proven correct
2. **Policy as Code**: Declarative policies separate from application logic
3. **Performance**: Sub-millisecond policy evaluation with indexed retrieval
4. **Rich Policy Language**: Supports RBAC, ABAC, and complex conditional logic
5. **Tooling**: Built-in validation, testing, and analysis tools
6. **Production-Ready**: Used by AWS for critical authorization decisions

## Current State Analysis

### Existing RBAC System
- Simple role-based permissions (admin, operator, viewer)
- Fixed permission set: ClusterRead/Write/Admin, VmRead/Write/Delete/Execute, NodeRead/Write/Admin
- Basic resource types: Cluster, Vm(String), Node(u64)
- Token-based authentication with SHA256 hashing
- In-memory role and permission storage

### Integration Points
- gRPC middleware for auth enforcement
- SecurityManager for token validation
- Permission checks in all service implementations
- Multi-tenancy support via tenant ID extraction

## Migration Strategy

### Phase 1: Cedar Foundation (Week 1)

#### 1.1 Add Cedar Dependencies
```toml
# Cargo.toml
[dependencies]
cedar-policy = "3.0"
cedar-policy-core = "3.0"
cedar-policy-validator = "3.0"
```

#### 1.2 Define Cedar Schema
Create `cedar/schema.cedarschema.json`:
```json
{
  "Blixard": {
    "entityTypes": {
      "User": {
        "memberOfTypes": ["Role", "Tenant"],
        "shape": {
          "type": "Record",
          "attributes": {
            "email": { "type": "String" },
            "tenant_id": { "type": "String" },
            "created_at": { "type": "Long" }
          }
        }
      },
      "Role": {
        "memberOfTypes": ["Tenant"],
        "shape": {
          "type": "Record",
          "attributes": {
            "name": { "type": "String" },
            "description": { "type": "String" }
          }
        }
      },
      "Tenant": {
        "shape": {
          "type": "Record",
          "attributes": {
            "name": { "type": "String" },
            "tier": { "type": "String", "enum": ["free", "pro", "enterprise"] },
            "quota_cpu": { "type": "Long" },
            "quota_memory": { "type": "Long" },
            "quota_vms": { "type": "Long" }
          }
        }
      },
      "VM": {
        "memberOfTypes": ["Node", "Tenant"],
        "shape": {
          "type": "Record",
          "attributes": {
            "name": { "type": "String" },
            "node_id": { "type": "Long" },
            "tenant_id": { "type": "String" },
            "cpu": { "type": "Long" },
            "memory": { "type": "Long" },
            "priority": { "type": "Long" },
            "preemptible": { "type": "Boolean" },
            "state": { "type": "String" }
          }
        }
      },
      "Node": {
        "memberOfTypes": ["Cluster"],
        "shape": {
          "type": "Record",
          "attributes": {
            "id": { "type": "Long" },
            "address": { "type": "String" },
            "capacity_cpu": { "type": "Long" },
            "capacity_memory": { "type": "Long" },
            "available_cpu": { "type": "Long" },
            "available_memory": { "type": "Long" }
          }
        }
      },
      "Cluster": {
        "shape": {
          "type": "Record",
          "attributes": {
            "name": { "type": "String" },
            "region": { "type": "String" }
          }
        }
      }
    },
    "actions": {
      "readCluster": { "appliesTo": { "principalTypes": ["User", "Role"], "resourceTypes": ["Cluster"] } },
      "manageCluster": { "appliesTo": { "principalTypes": ["User", "Role"], "resourceTypes": ["Cluster"] } },
      "joinCluster": { "appliesTo": { "principalTypes": ["User", "Role"], "resourceTypes": ["Cluster"] } },
      "leaveCluster": { "appliesTo": { "principalTypes": ["User", "Role"], "resourceTypes": ["Cluster"] } },
      
      "readNode": { "appliesTo": { "principalTypes": ["User", "Role"], "resourceTypes": ["Node"] } },
      "manageNode": { "appliesTo": { "principalTypes": ["User", "Role"], "resourceTypes": ["Node"] } },
      
      "createVM": { "appliesTo": { "principalTypes": ["User", "Role"], "resourceTypes": ["Tenant", "Node"] } },
      "readVM": { "appliesTo": { "principalTypes": ["User", "Role"], "resourceTypes": ["VM"] } },
      "updateVM": { "appliesTo": { "principalTypes": ["User", "Role"], "resourceTypes": ["VM"] } },
      "deleteVM": { "appliesTo": { "principalTypes": ["User", "Role"], "resourceTypes": ["VM"] } },
      "executeVM": { "appliesTo": { "principalTypes": ["User", "Role"], "resourceTypes": ["VM"] } },
      
      "readMetrics": { "appliesTo": { "principalTypes": ["User", "Role"], "resourceTypes": ["Node", "VM", "Cluster"] } },
      "manageBackups": { "appliesTo": { "principalTypes": ["User", "Role"], "resourceTypes": ["Cluster", "VM"] } }
    }
  }
}
```

#### 1.3 Create Cedar Policy Engine Module
Create `src/cedar_authz.rs`:
```rust
use cedar_policy::{Authorizer, Context, Decision, Entities, PolicySet, Request, Schema};
use std::sync::Arc;
use tokio::sync::RwLock;
use crate::error::{BlixardError, BlixardResult};

pub struct CedarAuthz {
    authorizer: Authorizer,
    policy_set: Arc<RwLock<PolicySet>>,
    schema: Schema,
    entities: Arc<RwLock<Entities>>,
}

impl CedarAuthz {
    pub fn new() -> BlixardResult<Self> {
        // Initialize Cedar with schema
    }
    
    pub async fn is_authorized(
        &self,
        principal: &str,
        action: &str,
        resource: &str,
        context: Context,
    ) -> BlixardResult<bool> {
        // Perform Cedar authorization check
    }
}
```

### Phase 2: Policy Migration (Week 1-2)

#### 2.1 Convert Existing Roles to Cedar Policies

Create `cedar/policies/base_roles.cedar`:
```cedar
// Admin Role - Full system access
permit(
    principal in Role::"admin",
    action in [
        Action::"readCluster", Action::"manageCluster", 
        Action::"joinCluster", Action::"leaveCluster",
        Action::"readNode", Action::"manageNode",
        Action::"createVM", Action::"readVM", Action::"updateVM", 
        Action::"deleteVM", Action::"executeVM",
        Action::"readMetrics", Action::"manageBackups"
    ],
    resource
);

// Operator Role - Manage VMs and read cluster state
permit(
    principal in Role::"operator",
    action in [
        Action::"readCluster", Action::"readNode",
        Action::"createVM", Action::"readVM", Action::"updateVM", 
        Action::"deleteVM", Action::"executeVM",
        Action::"readMetrics"
    ],
    resource
);

// Viewer Role - Read-only access
permit(
    principal in Role::"viewer",
    action in [
        Action::"readCluster", Action::"readNode",
        Action::"readVM", Action::"readMetrics"
    ],
    resource
);
```

#### 2.2 Add Advanced Policies

Create `cedar/policies/advanced.cedar`:
```cedar
// Multi-tenancy: Users can only access resources in their tenant
permit(
    principal,
    action,
    resource
) when {
    principal.tenant_id == resource.tenant_id
};

// Resource quotas: Deny VM creation if tenant quota exceeded
forbid(
    principal,
    action == Action::"createVM",
    resource in Tenant
) when {
    resource.quota_vms <= context.current_vm_count
};

// Time-based access: Operators can only delete VMs during maintenance windows
forbid(
    principal in Role::"operator",
    action == Action::"deleteVM",
    resource
) unless {
    context.hour >= 22 || context.hour <= 6
};

// Priority-based protection: Prevent deletion of high-priority VMs by non-admins
forbid(
    principal,
    action == Action::"deleteVM",
    resource
) when {
    resource.priority >= 900 && !(principal in Role::"admin")
};

// Node capacity enforcement
forbid(
    principal,
    action == Action::"createVM",
    resource in Node
) when {
    resource.available_cpu < context.requested_cpu ||
    resource.available_memory < context.requested_memory
};
```

### Phase 3: Integration (Week 2) ✅ COMPLETE

#### 3.1 Update Security Manager
Modify `src/security.rs` to integrate Cedar:
```rust
pub struct SecurityManager {
    cedar_authz: Arc<CedarAuthz>,
    // ... existing fields
}

impl SecurityManager {
    pub async fn check_permission_cedar(
        &self,
        user_id: &str,
        action: &str,
        resource: &str,
        context: HashMap<String, Value>,
    ) -> BlixardResult<bool> {
        self.cedar_authz.is_authorized(user_id, action, resource, context).await
    }
}
```

#### 3.2 Update Middleware
Modify `src/grpc_server/common/middleware.rs`:
```rust
pub async fn authenticate_and_authorize_cedar<T>(
    &self,
    request: Request<T>,
    action: &str,
    resource_type: &str,
) -> Result<(Request<T>, AuthContext), Status> {
    // Extract auth token
    let auth_context = self.authenticate(request)?;
    
    // Build Cedar context
    let context = self.build_cedar_context(&auth_context);
    
    // Check authorization with Cedar
    let authorized = self.security_manager
        .check_permission_cedar(
            &auth_context.user_id,
            action,
            resource_type,
            context
        )
        .await
        .map_err(|e| Status::internal(e.to_string()))?;
    
    if !authorized {
        return Err(Status::permission_denied("Insufficient permissions"));
    }
    
    Ok((request, auth_context))
}
```

### Phase 4: Service Updates (Week 2-3)

#### 4.1 Update Each gRPC Service
Example for VM Service:
```rust
// src/grpc_server/services/vm_service.rs
async fn create_vm(&self, request: Request<CreateVmRequest>) -> Result<Response<CreateVmResponse>, Status> {
    let (request, auth_context) = self.middleware
        .authenticate_and_authorize_cedar(
            request,
            "createVM",
            &format!("Tenant::{}", auth_context.tenant_id)
        )
        .await?;
    
    // ... rest of implementation
}
```

### Phase 5: Testing and Validation (Week 3)

#### 5.1 Policy Validation Tests
Create `tests/cedar_policy_tests.rs`:
```rust
#[test]
fn test_policy_validation() {
    // Validate all policies against schema
    // Test for policy conflicts
    // Verify coverage of all actions
}

#[test]
fn test_authorization_scenarios() {
    // Test multi-tenancy isolation
    // Test role-based permissions
    // Test quota enforcement
    // Test time-based access
}
```

#### 5.2 Integration Tests
Update existing auth tests to use Cedar policies.

### Phase 6: Migration Tools (Week 3-4)

#### 6.1 Role Migration Tool
Create `tools/migrate_roles_to_cedar.rs`:
```rust
// Tool to convert existing role assignments to Cedar entities
// Generate Cedar entity files from current user/role mappings
```

#### 6.2 Policy Management CLI
Extend CLI with Cedar commands:
```bash
blixard policy validate    # Validate Cedar policies
blixard policy list       # List active policies
blixard policy add        # Add new policy
blixard policy test       # Test authorization scenarios
```

### Phase 7: Documentation and Deployment (Week 4)

#### 7.1 Documentation
- Policy authoring guide
- Common policy patterns
- Migration guide for operators
- Troubleshooting guide

#### 7.2 Gradual Rollout
1. Deploy with Cedar in shadow mode (log decisions, don't enforce)
2. Compare Cedar decisions with existing RBAC
3. Enable Cedar enforcement for read operations
4. Enable Cedar for all operations
5. Remove old RBAC code

## Benefits After Migration

1. **Enhanced Security**: Mathematically proven policy correctness
2. **Flexibility**: Complex policies without code changes
3. **Auditability**: All policies in declarative files
4. **Performance**: Optimized policy evaluation engine
5. **Maintainability**: Clear separation of authorization logic
6. **Compliance**: Better support for regulatory requirements

## Risk Mitigation

1. **Shadow Mode**: Run Cedar alongside existing system initially
2. **Policy Validation**: Automated testing of all policy changes
3. **Rollback Plan**: Keep old RBAC code until Cedar is proven
4. **Monitoring**: Detailed metrics on authorization decisions
5. **Training**: Team education on Cedar policy language

## Success Metrics

- Zero authorization-related security incidents
- Sub-millisecond authorization latency (p99)
- 100% policy test coverage
- Reduced time to implement new access control requirements
- Improved audit compliance scores

## Next Steps

1. Review and approve migration plan
2. Set up Cedar development environment
3. Begin Phase 1 implementation
4. Schedule weekly progress reviews

---

## Timeline

### Q1 2025
- ✅ Three-node cluster formation (COMPLETE)
- [ ] Production hardening and testing
- [ ] Cedar policy engine migration (Phases 1-3)
- [ ] Performance optimization sprint

### Q2 2025
- [ ] Cedar migration completion (Phases 4-7)
- [ ] Advanced scheduling features
- [ ] Multi-datacenter support
- [ ] Enterprise features (backup, DR)

### Q3 2025
- [ ] Cloud provider integrations
- [ ] Kubernetes operator
- [ ] Advanced networking features
- [ ] Security certifications

---

*Last Updated: 2025-01-31*