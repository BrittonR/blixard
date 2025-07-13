use std::net::SocketAddr;
use std::sync::Arc;
use base64::prelude::*;
use blixard_core::{
    config_global,
    // tracing_otel, // Temporarily disabled: uses tonic which we're removing
    config_v2::Config,
    error::BlixardResult,
    metrics_otel,
    metrics_server,
    node::Node,
    types::NodeConfig,
    vm_backend::{VmBackend, VmBackendRegistry},
};
use blixard_vm::MicrovmBackendFactory;
use tokio::task::JoinHandle;

/// Configuration for the Blixard orchestrator
#[derive(Debug, Clone)]
pub struct OrchestratorConfig {
    /// Node configuration for distributed consensus
    pub node_config: NodeConfig,
    /// VM backend type to use ("mock", "microvm", "docker", etc.)
    pub vm_backend_type: String,
    /// Full configuration object
    pub config: Config,
}

/// Main orchestrator that coordinates between distributed systems and VM management
///
/// This is the primary entry point for the Blixard system. It creates and manages
/// both the distributed consensus layer (blixard-core) and the VM backend (blixard-vm),
/// providing a unified interface for cluster and VM operations.
///
/// ## Architecture
///
/// ```text
/// ┌─────────────────────────────────────────┐
/// │           BlixardOrchestrator           │
/// ├─────────────────────────────────────────┤
/// │  • CLI Command Processing              │
/// │  • Configuration Management            │
/// │  • Component Lifecycle                 │
/// └─────────────────────────────────────────┘
///          │                      │
///          ▼                      ▼
/// ┌─────────────────┐    ┌─────────────────┐
/// │  blixard-core   │    │   blixard-vm    │
/// │                 │    │                 │
/// │ • Raft consensus │    │ • VM lifecycle  │
/// │ • gRPC server   │    │ • microvm.nix   │
/// │ • Storage       │    │ • Monitoring    │
/// │ • Peer mgmt     │    │                 │
/// └─────────────────┘    └─────────────────┘
/// ```
pub struct BlixardOrchestrator {
    node: Node,
    metrics_handle: Option<JoinHandle<BlixardResult<()>>>,
    node_config: NodeConfig,
}

impl BlixardOrchestrator {
    /// Create a new orchestrator with the given configuration
    pub async fn new(config: OrchestratorConfig) -> BlixardResult<Self> {
        tracing::info!(
            "Creating Blixard orchestrator with VM backend: {}",
            config.vm_backend_type
        );

        // Update node config to include VM backend type
        let mut node_config = config.node_config.clone();
        node_config.vm_backend = config.vm_backend_type.clone();

        // Create the distributed consensus node
        let node = Node::new(node_config.clone());

        tracing::info!(
            "Blixard orchestrator created with {} VM backend",
            config.vm_backend_type
        );

        Ok(Self {
            node,
            metrics_handle: None,
            node_config,
        })
    }

    /// Initialize the node and start all services
    pub async fn initialize(&mut self, config: Config) -> BlixardResult<()> {
        tracing::info!("Initializing Blixard orchestrator services");

        // Initialize global configuration
        config_global::init(config)?;
        tracing::info!("Global configuration initialized");

        // Initialize metrics with Prometheus exporter
        metrics_otel::init_prometheus().map_err(|e| {
            blixard_core::error::BlixardError::Internal {
                message: format!("Failed to initialize metrics: {}", e),
            }
        })?;
        tracing::info!("Metrics initialized with Prometheus exporter");

        // Distributed tracing is already initialized in main.rs
        // Just log the current configuration
        let otlp_endpoint = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT").ok();
        if otlp_endpoint.is_some() {
            tracing::info!("Distributed tracing enabled with OTLP exporter");
        } else {
            tracing::info!("Distributed tracing available but no OTLP exporter configured");
        }

        // Set up the VM backend registry
        let mut registry = VmBackendRegistry::default(); // Includes built-in mock backend

        // Register the microvm backend
        registry.register(Arc::new(MicrovmBackendFactory));

        // TODO: Future backends can be registered here
        // registry.register(Arc::new(DockerBackendFactory));
        // registry.register(Arc::new(FirecrackerBackendFactory));

        // Initialize with the VM backend registry
        self.node.initialize_with_vm_registry(registry).await?;

        tracing::info!("Blixard orchestrator initialized successfully");
        Ok(())
    }

    /// Start the node and all services
    pub async fn start(&mut self) -> BlixardResult<()> {
        tracing::info!("Starting Blixard node services");
        self.node.start().await?;

        // Write node discovery information
        if let Err(e) = self.write_node_registry().await {
            tracing::warn!("Failed to write node registry: {}", e);
        }

        // Start metrics server if enabled
        if let Ok(config) = config_global::get() {
            if config.network.metrics.enabled {
                let bind_addr_str = self.node.shared().get_bind_addr();
                let bind_addr: SocketAddr = bind_addr_str.parse()
                    .map_err(|e| tracing::warn!("Failed to parse bind address '{}': {}", bind_addr_str, e))
                    .unwrap_or_else(|_| "127.0.0.1:7001".parse().unwrap());
                let metrics_port = bind_addr.port() + config.network.metrics.port_offset;
                let metrics_addr = SocketAddr::new(bind_addr.ip(), metrics_port);

                self.metrics_handle = Some(metrics_server::spawn_metrics_server(
                    metrics_addr,
                    self.node.shared().clone(),
                ));
                tracing::info!(
                    "Metrics server started on http://{}{}",
                    metrics_addr,
                    config.network.metrics.path
                );
            } else {
                tracing::info!("Metrics server disabled by configuration");
            }
        } else {
            tracing::warn!("Failed to get configuration for metrics server");
        }

        tracing::info!("Blixard node started successfully");
        Ok(())
    }

    /// Stop the node and all services
    pub async fn stop(&mut self) -> BlixardResult<()> {
        tracing::info!("Stopping Blixard orchestrator");

        // Stop metrics server
        if let Some(handle) = self.metrics_handle.take() {
            handle.abort();
        }

        self.node.stop().await?;

        // Shutdown tracing to flush any pending spans
        // tracing_otel::shutdown(); // Temporarily disabled: tracing_otel uses tonic which we're removing

        tracing::info!("Blixard orchestrator stopped");
        Ok(())
    }

    /// Get the node for direct access to distributed operations
    pub fn node(&self) -> &Node {
        &self.node
    }

    /// Get the VM backend for direct access to VM operations
    pub async fn vm_backend(&self) -> Option<Arc<dyn VmBackend>> {
        self.node.shared().get_vm_manager()
    }

    /// Get the bind address of the gRPC server
    pub fn bind_address(&self) -> std::net::SocketAddr {
        self.node.shared().get_bind_addr()
            .parse()
            .unwrap_or_else(|_| "127.0.0.1:7001".parse().unwrap())
    }

    /// Write node registry information for discovery
    async fn write_node_registry(&self) -> BlixardResult<()> {
        let shared = self.node.shared();
        let node_id = shared.config.id;
        let bind_addr = shared.get_bind_addr();

        // Get P2P information from Iroh transport
        let (iroh_node_id, direct_addresses, relay_url) =
            if let Some(node_addr) = shared.get_p2p_node_addr().await {
                // Extract components from NodeAddr
                let node_id_base64 = base64::prelude::BASE64_STANDARD.encode(
                    node_addr.node_id.as_bytes(),
                );
                let addresses: Vec<String> = node_addr.direct_addresses()
                    .map(|addr| addr.to_string())
                    .collect();
                let relay = node_addr.relay_url().map(|url| url.to_string());
                (node_id_base64, addresses, relay)
            } else {
                tracing::warn!("P2P node address not available, using placeholder values");
                (
                    "placeholder_node_id".to_string(),
                    vec![bind_addr.to_string()],
                    None,
                )
            };

        // Create registry entry
        let entry = crate::node_discovery::NodeRegistryEntry {
            cluster_node_id: node_id,
            iroh_node_id,
            direct_addresses,
            relay_url,
            address: bind_addr.to_string(),
        };

        // Write to data directory
        let data_dir = std::path::Path::new(&self.node_config.data_dir);
        let registry_path = data_dir.join(format!("node-{}-registry.json", node_id));

        let registry_path_str = registry_path.to_str()
            .ok_or_else(|| blixard_core::error::BlixardError::InvalidConfiguration {
                message: format!("Registry path contains invalid Unicode characters: {}", registry_path.display()),
            })?;
        crate::node_discovery::save_node_registry(registry_path_str, &entry).await?;

        tracing::info!("Node registry written to: {}", registry_path.display());
        Ok(())
    }
}

impl Drop for BlixardOrchestrator {
    fn drop(&mut self) {
        tracing::debug!("BlixardOrchestrator dropped");
    }
}
