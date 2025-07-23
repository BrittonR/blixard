use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

use crate::error::{BlixardError, BlixardResult};
use crate::raft::messages::RaftProposal;
use crate::raft_manager::{RaftConfChange, RaftManager};
use crate::vm_backend::{VmBackendRegistry, VmManager};
use crate::vm_health_monitor::VmHealthMonitor;

use redb::{Database, ReadableTable};

use super::core::Node;

impl Node {
    /// Initialize the node with database and channels
    pub async fn initialize(&mut self) -> BlixardResult<()> {
        // Call internal initialization with default registry
        self.initialize_internal(None).await
    }

    /// Initialize VM backend with custom registry (for applications to register backends)
    pub async fn initialize_with_vm_registry(
        &mut self,
        registry: VmBackendRegistry,
    ) -> BlixardResult<()> {
        // Call the internal initialization with the custom registry
        self.initialize_internal(Some(registry)).await
    }

    /// Internal initialization that accepts an optional VM backend registry
    pub(super) async fn initialize_internal(
        &mut self,
        registry_opt: Option<VmBackendRegistry>,
    ) -> BlixardResult<()> {
        // Phase 1: Database initialization
        let db = self.setup_database().await?;

        // Phase 2: VM infrastructure
        let registry = registry_opt.unwrap_or_else(VmBackendRegistry::default);
        self.setup_vm_infrastructure(registry, db.clone()).await?;

        // Phase 3: Storage managers (quota and IP pool)
        self.setup_storage_managers(db.clone()).await?;

        // Phase 4: Security and observability
        self.setup_security_and_observability().await?;

        // Phase 5: P2P infrastructure
        self.setup_p2p_infrastructure().await?;

        // Phase 6: Raft initialization
        let (raft_manager, _message_rx, _, proposal_tx, conf_change_tx) =
            self.initialize_raft(db.clone()).await?;

        // Store raft_manager in shared state
        self.shared.set_raft_manager(raft_manager.clone()).await;

        // Phase 7: Transport setup
        let (message_tx, _unused_rx) = mpsc::unbounded_channel();

        // Set the message tx in shared state
        self.shared.set_raft_message_tx(message_tx.clone());

        let (raft_transport, outgoing_rx) = self
            .setup_transport(message_tx.clone(), conf_change_tx, proposal_tx)
            .await?;

        // Phase 8: Spawn Raft message handler
        self.spawn_raft_message_handler(raft_transport, outgoing_rx);

        // Phase 9: Spawn Raft manager with automatic recovery
        let raft_handle = self.spawn_raft_manager_with_recovery(raft_manager);
        self.raft_handle = Some(raft_handle);

        // Phase 10: Perform cluster join if configured
        self.join_cluster_if_configured().await?;

        // Mark node as initialized
        self.shared.set_initialized(true);

        Ok(())
    }

    /// Set up database infrastructure
    pub(super) async fn setup_database(&self) -> BlixardResult<Arc<Database>> {
        let data_dir = &self.shared.config.data_dir;

        // Ensure the data directory exists
        std::fs::create_dir_all(data_dir).map_err(|e| BlixardError::Storage {
            operation: "create data directory".to_string(),
            source: Box::new(e),
        })?;

        let db_path = format!("{}/blixard.db", data_dir);

        // Try to open existing database first, create if it doesn't exist
        let database = match std::fs::metadata(&db_path) {
            Ok(_) => {
                // Database exists, open it
                Database::open(&db_path).map_err(|e| BlixardError::Storage {
                    operation: "open database".to_string(),
                    source: Box::new(e),
                })?
            }
            Err(_) => {
                // Database doesn't exist, create it
                Database::create(&db_path).map_err(|e| BlixardError::Storage {
                    operation: "create database".to_string(),
                    source: Box::new(e),
                })?
            }
        };

        let db_arc = Arc::new(database);

        // Initialize all database tables
        crate::raft_storage::init_database_tables(&db_arc)?;

        self.shared.set_database(Some(db_arc.clone())).await;

        Ok(db_arc)
    }

    /// Set up VM infrastructure (manager and health monitor)
    pub(super) async fn setup_vm_infrastructure(
        &mut self,
        registry: VmBackendRegistry,
        db: Arc<Database>,
    ) -> BlixardResult<()> {
        // Create VM backend
        let vm_backend = self.create_vm_backend(&registry, db.clone())?;

        // Store VM backend in shared state
        self.shared.set_vm_manager(vm_backend.clone()).await;

        // Initialize VM manager
        let vm_manager = Arc::new(VmManager::new(db.clone(), vm_backend, self.shared.clone()));

        // Recover persisted VMs
        if let Err(e) = vm_manager.recover_persisted_vms().await {
            tracing::error!("Failed to recover persisted VMs: {}", e);
            // Continue initialization even if recovery fails
        }

        // Initialize VM health monitor
        let health_monitor = VmHealthMonitor::new(
            self.shared.clone(),
            vm_manager.clone(),
            Duration::from_secs(30),
        );
        self.health_monitor = Some(health_monitor);

        Ok(())
    }

    /// Set up storage infrastructure (quota and IP pool managers)
    pub(super) async fn setup_storage_managers(&self, db: Arc<Database>) -> BlixardResult<()> {
        // Initialize quota manager
        let storage = Arc::new(crate::raft_storage::RedbRaftStorage {
            database: db.clone(),
        });
        let quota_manager = Arc::new(crate::quota_manager::QuotaManager::new(storage).await?);
        self.shared.set_quota_manager(quota_manager);

        // Initialize IP pool manager
        let ip_pool_manager = Arc::new(crate::ip_pool_manager::IpPoolManager::new());

        // Load existing pools and allocations from database
        let pools = {
            let read_txn = db.begin_read().map_err(|e| BlixardError::Storage {
                operation: "begin read transaction".to_string(),
                source: Box::new(e),
            })?;

            let mut pools = Vec::new();
            if let Ok(table) = read_txn.open_table(crate::raft_storage::IP_POOL_TABLE) {
                for entry in table.iter().map_err(|e| BlixardError::Storage {
                    operation: "iterate IP pools".to_string(),
                    source: Box::new(e),
                })? {
                    let (_, value) = entry.map_err(|e| BlixardError::Storage {
                        operation: "read IP pool entry".to_string(),
                        source: Box::new(e),
                    })?;

                    let pool: crate::ip_pool::IpPoolConfig = bincode::deserialize(value.value())
                        .map_err(|e| BlixardError::Serialization {
                            operation: "deserialize IP pool".to_string(),
                            source: Box::new(e),
                        })?;
                    pools.push(pool);
                }
            }
            pools
        };

        let allocations = {
            let read_txn = db.begin_read().map_err(|e| BlixardError::Storage {
                operation: "begin read transaction".to_string(),
                source: Box::new(e),
            })?;

            let mut allocations = Vec::new();
            if let Ok(table) = read_txn.open_table(crate::raft_storage::IP_ALLOCATION_TABLE) {
                for entry in table.iter().map_err(|e| BlixardError::Storage {
                    operation: "iterate IP allocations".to_string(),
                    source: Box::new(e),
                })? {
                    let (_, value) = entry.map_err(|e| BlixardError::Storage {
                        operation: "read IP allocation entry".to_string(),
                        source: Box::new(e),
                    })?;

                    let allocation: crate::ip_pool::IpAllocation =
                        bincode::deserialize(value.value()).map_err(|e| {
                            BlixardError::Serialization {
                                operation: "deserialize IP allocation".to_string(),
                                source: Box::new(e),
                            }
                        })?;
                    allocations.push(allocation);
                }
            }
            allocations
        };

        ip_pool_manager
            .load_from_storage(pools, allocations)
            .await?;
        self.shared.set_ip_pool_manager(ip_pool_manager);

        tracing::info!("Initialized IP pool manager");
        Ok(())
    }

    /// Set up security and observability infrastructure
    pub(super) async fn setup_security_and_observability(&self) -> BlixardResult<()> {
        // Phase 4: Security and observability - placeholder for future implementation
        // This could include metrics initialization, tracing setup, etc.
        tracing::info!("Security and observability infrastructure initialized");
        Ok(())
    }

    /// Set up P2P infrastructure if enabled
    pub(super) async fn setup_p2p_infrastructure(&mut self) -> BlixardResult<()> {
        let _config = crate::config_global::get()?;

        // P2P manager initialization - placeholder for future enhancement
        // The Iroh transport handles P2P functionality directly
        tracing::info!("P2P infrastructure initialized via Iroh transport");

        Ok(())
    }

    /// Register bootstrap worker in database
    pub(super) async fn register_bootstrap_worker(&self, db: Arc<Database>) -> BlixardResult<()> {
        let txn = db.begin_write().map_err(|e| BlixardError::Storage {
            operation: "begin write transaction".to_string(),
            source: Box::new(e),
        })?;

        // Register this node as a worker
        let capabilities = crate::raft_manager::WorkerCapabilities {
            cpu_cores: num_cpus::get() as u32,
            memory_mb: Self::estimate_available_memory(),
            disk_gb: Self::estimate_available_disk(&self.shared.config.data_dir),
            features: match self.shared.config.vm_backend.as_str() {
                "microvm" => vec!["microvm".to_string()],
                "docker" => vec!["container".to_string()],
                "mock" => vec!["mock".to_string()],
                _ => vec![],
            },
        };

        // For bootstrap, we'll store the capabilities directly
        let worker_data =
            bincode::serialize(&capabilities).map_err(|e| BlixardError::Serialization {
                operation: "serialize worker capabilities".to_string(),
                source: Box::new(e),
            })?;

        {
            let mut worker_table = txn.open_table(crate::raft_storage::WORKER_TABLE)?;
            let key_bytes = self.shared.config.id.to_be_bytes();
            worker_table.insert(key_bytes.as_slice(), worker_data.as_slice())?;
        }

        txn.commit().map_err(|e| BlixardError::Storage {
            operation: "commit worker registration".to_string(),
            source: Box::new(e),
        })?;

        tracing::info!(
            "Registered bootstrap node {} as worker with {} CPUs, {} MB memory, {} GB disk",
            self.shared.config.id,
            capabilities.cpu_cores,
            capabilities.memory_mb,
            capabilities.disk_gb
        );

        Ok(())
    }

    /// Initialize Raft manager and channels
    pub(super) async fn initialize_raft(
        &mut self,
        db: Arc<Database>,
    ) -> BlixardResult<(
        RaftManager,
        mpsc::UnboundedReceiver<(u64, raft::prelude::Message)>,
        mpsc::UnboundedReceiver<RaftConfChange>,
        mpsc::UnboundedSender<RaftProposal>,
        mpsc::UnboundedSender<RaftConfChange>,
    )> {
        let joining_cluster = self.shared.config.join_addr.is_some();
        let storage = crate::raft_storage::RedbRaftStorage {
            database: db.clone(),
        };

        // Initialize based on whether we're joining or bootstrapping
        if !joining_cluster {
            // Bootstrap mode - initialize storage with this node
            tracing::info!(
                "Bootstrapping new cluster with node {}",
                self.shared.config.id
            );
            storage.initialize_single_node(self.shared.config.id)?;

            // Register as worker in bootstrap mode
            self.register_bootstrap_worker(db.clone()).await?;
        } else {
            // Join mode - storage will be initialized during join
            tracing::info!("Preparing to join existing cluster");
        }

        // Create Raft manager
        let (raft_manager, proposal_tx, _message_tx, conf_change_tx, message_rx) =
            RaftManager::new(
                self.shared.config.id,
                db.clone(),
                vec![], // No initial peers for bootstrap/join
                Arc::downgrade(&self.shared),
            )?;

        // Set up batch processing if enabled
        let final_proposal_tx = self.setup_batch_processing(proposal_tx.clone())?;

        // Store the proposal tx in shared state
        self.shared.set_raft_proposal_tx(final_proposal_tx.clone());

        // Create a conf_change_rx for the return value (not used in this refactored version)
        let (_, conf_change_rx) = mpsc::unbounded_channel();

        Ok((
            raft_manager,
            message_rx,
            conf_change_rx,
            final_proposal_tx,
            conf_change_tx,
        ))
    }

    /// Set up batch processing for Raft proposals if enabled
    pub(super) fn setup_batch_processing(
        &self,
        proposal_tx: mpsc::UnboundedSender<RaftProposal>,
    ) -> BlixardResult<mpsc::UnboundedSender<RaftProposal>> {
        let config = crate::config_global::get()?;
        let batch_config = crate::raft_batch_processor::BatchConfig {
            enabled: config.cluster.raft.batch_processing.enabled,
            max_batch_size: config.cluster.raft.batch_processing.max_batch_size,
            batch_timeout_ms: config.cluster.raft.batch_processing.batch_timeout_ms,
            max_batch_bytes: config.cluster.raft.batch_processing.max_batch_bytes,
        };

        if batch_config.enabled {
            let (batch_tx, batch_processor) = crate::raft_batch_processor::create_batch_processor(
                batch_config,
                proposal_tx,
                self.shared.get_id(),
            );

            tokio::spawn(async move {
                batch_processor.run().await;
                tracing::debug!("Batch processor completed");
            });

            Ok(batch_tx)
        } else {
            Ok(proposal_tx)
        }
    }
}