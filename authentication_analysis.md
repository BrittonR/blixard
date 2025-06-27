# Authentication Pattern Analysis

## Summary of Authentication Patterns in gRPC Server

### Authentication Methods Found

1. **Regular Authentication** (`self.authenticate(&request, Permission)`)
   - Used in most gRPC methods
   - Requires a specific permission
   - Returns `SecurityContext` or error

2. **Optional Authentication** (`self.optional_authenticate(&request)`)
   - Used only in `health_check` method
   - Does not require authentication
   - Returns `Option<SecurityContext>`

### Permission Usage by Method

| Method | Permission | Line |
|--------|------------|------|
| join_cluster | Permission::ClusterWrite | 258 |
| leave_cluster | Permission::ClusterWrite | 462 |
| get_cluster_status | Permission::ClusterRead | 616 |
| create_vm | Permission::VmWrite | 701 |
| start_vm | Permission::VmWrite | 806 |
| stop_vm | Permission::VmWrite | 849 |
| delete_vm | Permission::VmWrite | 892 |
| list_vms | Permission::VmRead | 949 |
| get_vm_status | Permission::VmRead | 995 |
| migrate_vm | Permission::VmWrite | 1046 |
| health_check | optional_authenticate | 1162 |
| send_raft_message | Permission::ClusterWrite | 1186 |
| submit_task | Permission::TaskWrite | 1222 |
| get_task_status | Permission::TaskRead | 1272 |
| create_vm_with_scheduling | Permission::VmWrite | 1323 |
| schedule_vm_placement | Permission::VmRead | 1424 |
| get_cluster_resource_summary | Permission::ClusterRead | 1498 |
| get_raft_status | Permission::ClusterRead | 1564 |
| propose_task | Permission::TaskWrite | 1599 |

### Duplication Patterns

1. **Authentication Helper Methods**:
   - `authenticate()` method (lines 108-120) - wrapper around the macro
   - `optional_authenticate()` method (lines 123-135) - wrapper around the macro
   - Both methods handle the case when security middleware is not configured

2. **Rate Limiting Pattern**:
   - Applied to some methods but not all
   - Duplicated logic for checking and recording API requests
   - Methods with rate limiting:
     - join_cluster (lines 261-269)
     - get_cluster_status (lines 619-627)
     - delete_vm (lines 905-916)

3. **Tenant ID Extraction**:
   - `extract_tenant_id()` helper method (lines 138-148)
   - Used in methods that need multi-tenancy support
   - Not consistently applied across all methods

4. **Quota Checking**:
   - `check_vm_quota()` method (lines 151-192)
   - Used in create_vm and create_vm_with_scheduling
   - Combines rate limiting and resource quota checks

5. **Resource Usage Updates**:
   - `update_resource_usage()` method (lines 195-208)
   - Called after VM creation/deletion operations
   - Not consistently applied to all resource-modifying operations

### Inconsistencies Found

1. **Rate Limiting**: Only applied to 3 methods out of 19
2. **Tenant ID Extraction**: Only used in VM-related methods
3. **Quota Checks**: Only for VM creation, not for other resource-consuming operations
4. **Security Context Usage**: Always assigned to `_security_context` but rarely used
5. **Error Recording**: `record_grpc_error!` macro not consistently used

### Authentication Implementation

The authentication is implemented via:
1. **Macros** (`authenticate_grpc!` and `optional_authenticate_grpc!`) defined in `grpc_security.rs`
2. **Helper methods** in `BlixardGrpcService` that wrap these macros
3. **Security middleware** that extracts tokens from request headers
4. **Permission enum** with 8 different permission types

The macros handle:
- Token extraction from request headers
- Authentication via SecurityManager
- Permission checking
- Error conversion to gRPC Status codes