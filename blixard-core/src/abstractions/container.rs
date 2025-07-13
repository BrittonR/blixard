//! Dependency injection container
//!
//! This module provides a service container for managing dependencies,
//! enabling easy swapping between production and test implementations.

use crate::{
    abstractions::{
        command::{MockCommandExecutor, TokioCommandExecutor},
        config::{GlobalConfigProvider, MockConfigProvider},
        // TODO: Implement these when TaskRepository and NodeRepository implementations are added
        // storage::{RedbTaskRepository, RedbNodeRepository, MockTaskRepository, MockNodeRepository},
        filesystem::{MockFileSystem, TokioFileSystem},
        storage::{MockVmRepository, RedbVmRepository},
        // network::{TonicNetworkClient, MockNetworkClient},
        time::{MockClock, SystemClock},
        Clock,
        CommandExecutor,
        ConfigProvider,
        FileSystem,
        NodeRepository,
        TaskRepository,
        VmRepository,
    },
    config::Config,
    error::BlixardResult,
};
use std::sync::Arc;

/// Service container for dependency injection
pub struct ServiceContainer {
    /// VM repository
    pub vm_repo: Arc<dyn VmRepository>,
    /// Task repository
    pub task_repo: Arc<dyn TaskRepository>,
    /// Node repository
    pub node_repo: Arc<dyn NodeRepository>,
    /// Filesystem abstraction
    pub filesystem: Arc<dyn FileSystem>,
    /// Command executor
    pub command_executor: Arc<dyn CommandExecutor>,
    /// Configuration provider
    pub config_provider: Arc<dyn ConfigProvider>,
    // /// Network client
    // pub network_client: Arc<dyn NetworkClient>,
    /// Clock abstraction
    pub clock: Arc<dyn Clock>,
}

impl ServiceContainer {
    /// Create production container with real implementations
    pub fn new_production(database: Arc<redb::Database>) -> Self {
        Self {
            vm_repo: Arc::new(RedbVmRepository::new(database.clone())),
            // TODO: Implement RedbTaskRepository
            // TODO: Implement RedbTaskRepository and MockTaskRepository
            task_repo: Arc::new(MockVmRepository::new()) as Arc<dyn TaskRepository>,
            // TODO: Implement RedbNodeRepository and MockNodeRepository
            node_repo: Arc::new(MockVmRepository::new()) as Arc<dyn NodeRepository>,
            filesystem: Arc::new(TokioFileSystem::new()),
            command_executor: Arc::new(TokioCommandExecutor::new()),
            config_provider: Arc::new(GlobalConfigProvider::new()),
            // network_client: Arc::new(TonicNetworkClient::new()),
            clock: Arc::new(SystemClock::new()),
        }
    }

    /// Create test container with mock implementations
    pub fn new_test() -> Self {
        Self {
            vm_repo: Arc::new(MockVmRepository::new()),
            // TODO: Use MockTaskRepository when implemented
            task_repo: Arc::new(MockVmRepository::new()) as Arc<dyn TaskRepository>,
            // TODO: Use MockNodeRepository when implemented
            node_repo: Arc::new(MockVmRepository::new()) as Arc<dyn NodeRepository>,
            filesystem: Arc::new(MockFileSystem::new()),
            command_executor: Arc::new(MockCommandExecutor::new()),
            config_provider: Arc::new(MockConfigProvider::new()),
            // network_client: Arc::new(MockNetworkClient::new()),
            clock: Arc::new(MockClock::new()),
        }
    }

    /// Create test container with specific config
    pub fn new_test_with_config(config: Config) -> Self {
        Self {
            vm_repo: Arc::new(MockVmRepository::new()),
            // TODO: Use MockTaskRepository when implemented
            task_repo: Arc::new(MockVmRepository::new()) as Arc<dyn TaskRepository>,
            // TODO: Use MockNodeRepository when implemented
            node_repo: Arc::new(MockVmRepository::new()) as Arc<dyn NodeRepository>,
            filesystem: Arc::new(MockFileSystem::new()),
            command_executor: Arc::new(MockCommandExecutor::new()),
            config_provider: Arc::new(MockConfigProvider::with_config(config)),
            // network_client: Arc::new(MockNetworkClient::new()),
            clock: Arc::new(MockClock::new()),
        }
    }
}

/// Builder for customizing service container
pub struct ServiceContainerBuilder {
    vm_repo: Option<Arc<dyn VmRepository>>,
    task_repo: Option<Arc<dyn TaskRepository>>,
    node_repo: Option<Arc<dyn NodeRepository>>,
    filesystem: Option<Arc<dyn FileSystem>>,
    command_executor: Option<Arc<dyn CommandExecutor>>,
    config_provider: Option<Arc<dyn ConfigProvider>>,
    // network_client: Option<Arc<dyn NetworkClient>>,
    clock: Option<Arc<dyn Clock>>,
    database: Option<Arc<redb::Database>>,
}

impl ServiceContainerBuilder {
    /// Create new builder
    pub fn new() -> Self {
        Self {
            vm_repo: None,
            task_repo: None,
            node_repo: None,
            filesystem: None,
            command_executor: None,
            config_provider: None,
            // network_client: None,
            clock: None,
            database: None,
        }
    }

    /// Set database (for production repositories)
    pub fn with_database(mut self, database: Arc<redb::Database>) -> Self {
        self.database = Some(database);
        self
    }

    /// Set VM repository
    pub fn with_vm_repo(mut self, repo: Arc<dyn VmRepository>) -> Self {
        self.vm_repo = Some(repo);
        self
    }

    /// Set task repository
    pub fn with_task_repo(mut self, repo: Arc<dyn TaskRepository>) -> Self {
        self.task_repo = Some(repo);
        self
    }

    /// Set node repository
    pub fn with_node_repo(mut self, repo: Arc<dyn NodeRepository>) -> Self {
        self.node_repo = Some(repo);
        self
    }

    /// Set filesystem
    pub fn with_filesystem(mut self, fs: Arc<dyn FileSystem>) -> Self {
        self.filesystem = Some(fs);
        self
    }

    /// Set command executor
    pub fn with_command_executor(mut self, executor: Arc<dyn CommandExecutor>) -> Self {
        self.command_executor = Some(executor);
        self
    }

    /// Set config provider
    pub fn with_config_provider(mut self, provider: Arc<dyn ConfigProvider>) -> Self {
        self.config_provider = Some(provider);
        self
    }

    // /// Set network client
    // pub fn with_network_client(mut self, client: Arc<dyn NetworkClient>) -> Self {
    //     self.network_client = Some(client);
    //     self
    // }

    /// Set clock
    pub fn with_clock(mut self, clock: Arc<dyn Clock>) -> Self {
        self.clock = Some(clock);
        self
    }

    /// Build production container
    pub fn build_production(self) -> crate::error::BlixardResult<ServiceContainer> {
        let database = self.database.ok_or_else(|| {
            crate::error::BlixardError::configuration(
                "abstractions.container.database",
                "Database required for production container"
            )
        })?;

        Ok(ServiceContainer {
            vm_repo: self
                .vm_repo
                .unwrap_or_else(|| Arc::new(RedbVmRepository::new(database.clone()))),
            // TODO: Use RedbTaskRepository when implemented
            task_repo: self
                .task_repo
                .unwrap_or_else(|| Arc::new(MockVmRepository::new()) as Arc<dyn TaskRepository>),
            // TODO: Use RedbNodeRepository when implemented
            node_repo: self
                .node_repo
                .unwrap_or_else(|| Arc::new(MockVmRepository::new()) as Arc<dyn NodeRepository>),
            filesystem: self
                .filesystem
                .unwrap_or_else(|| Arc::new(TokioFileSystem::new())),
            command_executor: self
                .command_executor
                .unwrap_or_else(|| Arc::new(TokioCommandExecutor::new())),
            config_provider: self
                .config_provider
                .unwrap_or_else(|| Arc::new(GlobalConfigProvider::new())),
            // network_client: self.network_client.unwrap_or_else(||
            //     Arc::new(TonicNetworkClient::new())),
            clock: self.clock.unwrap_or_else(|| Arc::new(SystemClock::new())),
        })
    }

    /// Build test container
    pub fn build_test(self) -> ServiceContainer {
        ServiceContainer {
            vm_repo: self
                .vm_repo
                .unwrap_or_else(|| Arc::new(MockVmRepository::new())),
            // TODO: Use MockTaskRepository when implemented
            task_repo: self
                .task_repo
                .unwrap_or_else(|| Arc::new(MockVmRepository::new()) as Arc<dyn TaskRepository>),
            // TODO: Use MockNodeRepository when implemented
            node_repo: self
                .node_repo
                .unwrap_or_else(|| Arc::new(MockVmRepository::new()) as Arc<dyn NodeRepository>),
            filesystem: self
                .filesystem
                .unwrap_or_else(|| Arc::new(MockFileSystem::new())),
            command_executor: self
                .command_executor
                .unwrap_or_else(|| Arc::new(MockCommandExecutor::new())),
            config_provider: self
                .config_provider
                .unwrap_or_else(|| Arc::new(MockConfigProvider::new())),
            // network_client: self.network_client.unwrap_or_else(||
            //     Arc::new(MockNetworkClient::new())),
            clock: self.clock.unwrap_or_else(|| Arc::new(MockClock::new())),
        }
    }
}

impl Default for ServiceContainerBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// Placeholder implementations for missing repositories

use crate::raft_manager::TaskSpec;
use async_trait::async_trait;
use std::collections::HashMap;

/// Production task repository (placeholder)
struct RedbTaskRepository {
    #[allow(dead_code)] // Database connection for future task storage implementation
    database: Arc<redb::Database>,
}

impl RedbTaskRepository {
    #[allow(dead_code)]
    fn new(database: Arc<redb::Database>) -> Self {
        Self { database }
    }
}

#[async_trait]
impl TaskRepository for RedbTaskRepository {
    async fn create(&self, _task_id: &str, _task: &TaskSpec) -> BlixardResult<()> {
        Err(crate::error::BlixardError::NotImplemented {
            feature: "Task repository".to_string(),
        })
    }

    async fn get(&self, _task_id: &str) -> BlixardResult<Option<TaskSpec>> {
        Ok(None)
    }

    async fn update_status(&self, _task_id: &str, _status: &str) -> BlixardResult<()> {
        Ok(())
    }

    async fn list_by_status(&self, _status: &str) -> BlixardResult<Vec<(String, TaskSpec)>> {
        Ok(Vec::new())
    }

    async fn delete(&self, _task_id: &str) -> BlixardResult<()> {
        Ok(())
    }
}

/// Mock task repository (placeholder)
struct MockTaskRepository;

impl MockTaskRepository {
    #[allow(dead_code)]
    fn new() -> Self {
        Self
    }
}

#[async_trait]
impl TaskRepository for MockTaskRepository {
    async fn create(&self, _task_id: &str, _task: &TaskSpec) -> BlixardResult<()> {
        Ok(())
    }

    async fn get(&self, _task_id: &str) -> BlixardResult<Option<TaskSpec>> {
        Ok(None)
    }

    async fn update_status(&self, _task_id: &str, _status: &str) -> BlixardResult<()> {
        Ok(())
    }

    async fn list_by_status(&self, _status: &str) -> BlixardResult<Vec<(String, TaskSpec)>> {
        Ok(Vec::new())
    }

    async fn delete(&self, _task_id: &str) -> BlixardResult<()> {
        Ok(())
    }
}

/// Production node repository (placeholder)
struct RedbNodeRepository {
    #[allow(dead_code)] // Database connection for future node storage implementation
    database: Arc<redb::Database>,
}

impl RedbNodeRepository {
    #[allow(dead_code)]
    fn new(database: Arc<redb::Database>) -> Self {
        Self { database }
    }
}

#[async_trait]
impl NodeRepository for RedbNodeRepository {
    async fn store_node_info(&self, _node_id: u64, _bind_address: &str) -> BlixardResult<()> {
        Ok(())
    }

    async fn get_node_info(&self, _node_id: u64) -> BlixardResult<Option<String>> {
        Ok(None)
    }

    async fn list_nodes(&self) -> BlixardResult<HashMap<u64, String>> {
        Ok(HashMap::new())
    }

    async fn remove_node(&self, _node_id: u64) -> BlixardResult<()> {
        Ok(())
    }

    async fn update_node_health(&self, _node_id: u64, _healthy: bool) -> BlixardResult<()> {
        Ok(())
    }
}

/// Mock node repository (placeholder)
struct MockNodeRepository;

impl MockNodeRepository {
    #[allow(dead_code)]
    fn new() -> Self {
        Self
    }
}

#[async_trait]
impl NodeRepository for MockNodeRepository {
    async fn store_node_info(&self, _node_id: u64, _bind_address: &str) -> BlixardResult<()> {
        Ok(())
    }

    async fn get_node_info(&self, _node_id: u64) -> BlixardResult<Option<String>> {
        Ok(None)
    }

    async fn list_nodes(&self) -> BlixardResult<HashMap<u64, String>> {
        Ok(HashMap::new())
    }

    async fn remove_node(&self, _node_id: u64) -> BlixardResult<()> {
        Ok(())
    }

    async fn update_node_health(&self, _node_id: u64, _healthy: bool) -> BlixardResult<()> {
        Ok(())
    }
}
