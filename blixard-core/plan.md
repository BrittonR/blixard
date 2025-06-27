# Blixard Refactoring Plan

## Overview

This document outlines a systematic approach to refactoring the Blixard distributed microVM orchestration platform. The refactoring focuses on improving code quality, testability, and maintainability while preserving all existing functionality.

## Project Analysis Summary

### Current State
- **Technology Stack**: Rust, Raft consensus, gRPC, microvm.nix
- **Architecture**: Multi-node cluster with VM orchestration
- **Code Size**: 47 source files across blixard-core, blixard, and blixard-vm
- **Key Issues**:
  - 86 `unwrap()` calls in production code (potential panics)
  - Large monolithic files (grpc_server.rs: 1,702 lines, raft_manager.rs: 2,507 lines)
  - Limited abstractions for testing
  - Some code duplication in authentication and error handling

### Critical Issues Identified

1. **High-Risk unwrap() Calls**:
   - Global configuration access (RwLock poisoning risk)
   - Metrics system initialization
   - Lock operations that could panic in production

2. **Code Organization**:
   - grpc_server.rs contains all gRPC service implementations
   - Mixed concerns in single files
   - Limited separation between business logic and infrastructure

3. **Testing Challenges**:
   - Direct database access without abstraction
   - Global state usage
   - Tight coupling between components

## Refactoring Phases

### Phase 1: Critical unwrap() Fixes (High Priority) 🔴

**Objective**: Eliminate production panic risks from unwrap() calls

#### 1.1 Global Configuration Access (config_v2.rs)
```rust
// Current dangerous pattern:
let config = CONFIG.read().unwrap();

// Proposed solution:
pub enum ConfigError {
    LockPoisoned,
    NotInitialized,
}

impl GlobalConfig {
    pub fn read() -> Result<RwLockReadGuard<Config>, ConfigError> {
        CONFIG.read()
            .map_err(|_| ConfigError::LockPoisoned)
    }
    
    pub fn write() -> Result<RwLockWriteGuard<Config>, ConfigError> {
        CONFIG.write()
            .map_err(|_| ConfigError::LockPoisoned)
    }
}
```

#### 1.2 Metrics System (metrics_otel.rs)
```rust
// Current:
METRICS.get().unwrap()

// Proposed:
pub fn metrics() -> &'static Metrics {
    METRICS.get().expect("Metrics not initialized. Call init_metrics() first")
}

// Or better - return Result:
pub fn try_metrics() -> Result<&'static Metrics, MetricsError> {
    METRICS.get().ok_or(MetricsError::NotInitialized)
}
```

#### 1.3 Lock Operations
- Replace all mutex/rwlock unwrap() calls with proper error handling
- Consider using `parking_lot` which doesn't poison on panic
- Add context to lock acquisition failures

**Deliverables**:
- [ ] Fix all 86 unwrap() calls in production code
- [ ] Add proper error types and context
- [ ] Update tests to handle new error cases
- [ ] Document error handling patterns

### Phase 2: Modularize grpc_server.rs (High Priority) 📦

**Objective**: Break down 1,702-line file into focused, maintainable modules

#### 2.1 Module Structure
```
grpc_server/
├── mod.rs              # Public exports
├── common/
│   ├── middleware.rs   # Authentication, rate limiting
│   ├── conversions.rs  # Type conversions
│   └── helpers.rs      # Shared utilities
├── vm_service.rs       # VM operations
├── cluster_service.rs  # Cluster management
├── task_service.rs     # Task scheduling
└── monitoring_service.rs # Health & metrics
```

#### 2.2 Common Middleware Pattern
```rust
// grpc_server/common/middleware.rs
pub struct GrpcMiddleware {
    security: Option<GrpcSecurityMiddleware>,
    quota_manager: Option<Arc<QuotaManager>>,
}

impl GrpcMiddleware {
    pub async fn authenticate_and_rate_limit<T>(
        &self,
        request: &Request<T>,
        permission: Permission,
        operation: ApiOperation,
    ) -> Result<(SecurityContext, String), Status> {
        // Combine authentication, rate limiting, and tenant extraction
        let security_context = self.authenticate(request, permission).await?;
        let tenant_id = Self::extract_tenant_id(request);
        
        if let Some(ref quota) = self.quota_manager {
            quota.check_and_record_rate_limit(&tenant_id, &operation).await
                .map_err(|e| Status::resource_exhausted(e.to_string()))?;
        }
        
        Ok((security_context, tenant_id))
    }
}
```

#### 2.3 Service Implementation Example
```rust
// grpc_server/vm_service.rs
pub struct VmServiceImpl {
    node: Arc<SharedNodeState>,
    middleware: GrpcMiddleware,
}

#[tonic::async_trait]
impl BlixardService for VmServiceImpl {
    async fn create_vm(&self, request: Request<CreateVmRequest>) 
        -> Result<Response<CreateVmResponse>, Status> 
    {
        let (ctx, tenant_id) = self.middleware
            .authenticate_and_rate_limit(&request, Permission::VmWrite, ApiOperation::VmCreate)
            .await?;
            
        let req = instrument_grpc!(self.node, request, "create_vm");
        
        // VM creation logic...
    }
}
```

**Deliverables**:
- [ ] Create module structure
- [ ] Extract VM-related methods to vm_service.rs
- [ ] Extract cluster methods to cluster_service.rs
- [ ] Extract task methods to task_service.rs
- [ ] Consolidate common patterns in middleware
- [ ] Update all imports and tests

### Phase 3: Testability Improvements (Medium Priority) 🧪

**Objective**: Introduce abstractions for better testing and modularity

#### 3.1 Repository Pattern
```rust
// Define trait for VM storage operations
#[async_trait]
pub trait VmRepository: Send + Sync {
    async fn create(&self, vm: &VmConfig) -> Result<(), StorageError>;
    async fn get(&self, id: &str) -> Result<Option<VmConfig>, StorageError>;
    async fn list(&self) -> Result<Vec<VmConfig>, StorageError>;
    async fn update(&self, vm: &VmConfig) -> Result<(), StorageError>;
    async fn delete(&self, id: &str) -> Result<(), StorageError>;
}

// Production implementation
pub struct RedbVmRepository {
    db: Arc<Database>,
}

// Test implementation
pub struct MockVmRepository {
    vms: Arc<RwLock<HashMap<String, VmConfig>>>,
}
```

#### 3.2 Dependency Injection
```rust
pub struct ServiceContainer {
    vm_repo: Arc<dyn VmRepository>,
    task_repo: Arc<dyn TaskRepository>,
    quota_manager: Arc<dyn QuotaManager>,
    security_manager: Arc<dyn SecurityManager>,
}

impl ServiceContainer {
    pub fn new_production(db: Arc<Database>) -> Self {
        Self {
            vm_repo: Arc::new(RedbVmRepository::new(db.clone())),
            task_repo: Arc::new(RedbTaskRepository::new(db)),
            // ...
        }
    }
    
    pub fn new_test() -> Self {
        Self {
            vm_repo: Arc::new(MockVmRepository::default()),
            task_repo: Arc::new(MockTaskRepository::default()),
            // ...
        }
    }
}
```

**Deliverables**:
- [ ] Define repository traits for each domain
- [ ] Implement production repositories
- [ ] Create mock implementations
- [ ] Refactor services to use repositories
- [ ] Add unit tests using mocks

### Phase 4: Consolidate Patterns (Medium Priority) 🔄

**Objective**: Reduce code duplication and standardize patterns

#### 4.1 Error Context Enhancement
```rust
pub trait ErrorContext<T> {
    fn context(self, msg: &str) -> Result<T, BlixardError>;
    fn with_context<F>(self, f: F) -> Result<T, BlixardError>
    where
        F: FnOnce() -> String;
}

impl<T, E> ErrorContext<T> for Result<T, E>
where
    E: Into<BlixardError>,
{
    fn context(self, msg: &str) -> Result<T, BlixardError> {
        self.map_err(|e| {
            let base_error = e.into();
            BlixardError::WithContext {
                error: Box::new(base_error),
                context: msg.to_string(),
            }
        })
    }
}
```

#### 4.2 Centralized Conversions
```rust
// grpc_server/common/conversions.rs
pub mod conversions {
    use super::*;
    
    pub fn vm_status_to_proto(status: &InternalVmStatus) -> VmState {
        match status {
            InternalVmStatus::Creating => VmState::Created,
            InternalVmStatus::Starting => VmState::Starting,
            InternalVmStatus::Running => VmState::Running,
            InternalVmStatus::Stopping => VmState::Stopping,
            InternalVmStatus::Stopped => VmState::Stopped,
            InternalVmStatus::Failed => VmState::Failed,
        }
    }
    
    pub fn error_to_status(err: BlixardError) -> Status {
        match err {
            BlixardError::NotImplemented { feature } => {
                Status::unimplemented(format!("Feature not implemented: {}", feature))
            }
            BlixardError::ServiceNotFound(name) => {
                Status::not_found(format!("Service not found: {}", name))
            }
            // ... comprehensive mapping
        }
    }
}
```

**Deliverables**:
- [ ] Create unified error handling traits
- [ ] Consolidate conversion functions
- [ ] Extract rate limiting logic
- [ ] Standardize metrics recording
- [ ] Document patterns

### Phase 5: Performance Optimizations (Low Priority) ⚡

**Objective**: Reduce unnecessary allocations and improve efficiency

#### 5.1 String Interning
```rust
use once_cell::sync::Lazy;
use dashmap::DashMap;
use std::sync::Arc;

static INTERNED_STRINGS: Lazy<DashMap<String, Arc<str>>> = 
    Lazy::new(|| DashMap::new());

pub fn intern_string(s: &str) -> Arc<str> {
    INTERNED_STRINGS
        .entry(s.to_string())
        .or_insert_with(|| Arc::from(s))
        .clone()
}
```

#### 5.2 Connection Pooling
```rust
pub struct GrpcConnectionPool {
    connections: Arc<RwLock<HashMap<u64, Channel>>>,
    max_idle_time: Duration,
}

impl GrpcConnectionPool {
    pub async fn get_or_create(&self, node_id: u64, addr: &str) -> Result<Channel, Error> {
        // Check existing connection
        if let Some(channel) = self.connections.read().await.get(&node_id) {
            return Ok(channel.clone());
        }
        
        // Create new connection
        let channel = Channel::from_shared(addr.to_string())?
            .connect()
            .await?;
            
        self.connections.write().await.insert(node_id, channel.clone());
        Ok(channel)
    }
}
```

**Deliverables**:
- [ ] Implement string interning for common values
- [ ] Add connection pooling for gRPC clients
- [ ] Reduce unnecessary clones
- [ ] Add caching where appropriate
- [ ] Profile and optimize hot paths

## Migration Strategy

### Incremental Approach
1. **Feature Flags**: Use feature flags to gradually roll out changes
2. **Backward Compatibility**: Maintain gRPC protocol compatibility
3. **Parallel Development**: Phases 3-5 can proceed in parallel after Phase 2
4. **Comprehensive Testing**: Add tests for each refactored component

### PR Structure

**PR 1: Fix Critical unwrap() Calls**
- Replace dangerous unwrap() in config, metrics, and lock operations
- Add proper error handling and context
- Update tests to handle new error cases

**PR 2: Extract VM Service Module**
- Create grpc_server/vm_service.rs
- Move VM-related methods
- Add integration tests

**PR 3: Extract Cluster Service Module**
- Create grpc_server/cluster_service.rs
- Move cluster management methods
- Update imports and tests

**PR 4: Extract Task Service Module**
- Create grpc_server/task_service.rs
- Move task scheduling methods
- Add tests

**PR 5: Introduce Repository Pattern**
- Define repository traits
- Implement for VM operations
- Add mock implementations

### Testing Strategy

1. **Unit Tests**: Test individual components with mocks
2. **Integration Tests**: Test service interactions
3. **Property Tests**: Use PropTest for invariants
4. **Simulation Tests**: Use MadSim for distributed scenarios

### Success Metrics

- [ ] Zero unwrap() calls in production code
- [ ] No file larger than 500 lines
- [ ] 90%+ unit test coverage
- [ ] All new code uses dependency injection
- [ ] Performance benchmarks show no regression

## Timeline

- **Week 1-2**: Phase 1 (Critical unwrap() fixes)
- **Week 3-4**: Phase 2 (Modularize grpc_server.rs)
- **Week 5-6**: Phase 3 (Testability improvements)
- **Week 7-8**: Phase 4 (Consolidate patterns)
- **Week 9-10**: Phase 5 (Performance optimizations)

## Notes

- Preserve all existing functionality
- Document architectural decisions with ADRs
- Update developer documentation
- Consider creating a style guide for future development