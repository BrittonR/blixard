use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::oneshot;
use url::Url;

use crate::error::{BlixardError, BlixardResult};
use crate::p2p_manager::{ConnectionQuality, PeerInfo};
use crate::raft::messages::RaftProposal;
use crate::raft_manager::ProposalData;


use super::core::Node;

impl Node {
    /// Send join request to a cluster
    pub async fn send_join_request(&self) -> BlixardResult<()> {
        if let Some(join_addr) = &self.shared.config.join_addr {
            tracing::info!("Sending join request to {} (starts_with_node: {})", 
                join_addr, join_addr.starts_with("node"));

            // Get our P2P info if available
            let (p2p_node_id, p2p_addresses, p2p_relay_url) = self.gather_local_p2p_info().await;

            // Create Iroh client to contact the join address
            // Assume the join address is for node 1 (the bootstrap node)
            if let Some(raft_transport) = &self.raft_transport {
                let _endpoint = raft_transport.endpoint();

                // Parse the join address and create a NodeAddr for the target
                // For now, we'll create a simple connection to the leader
                // In a real implementation, we'd need to discover the leader's Iroh NodeId
                let leader_node_id = 1u64; // Assume node 1 is the initial leader

                // Check if join_addr is a NodeTicket for direct P2P connection FIRST
                if join_addr.starts_with("node") {
                    // Handle NodeTicket case - direct P2P connection
                    let node_ticket = join_addr.parse::<iroh_base::ticket::NodeTicket>()
                        .map_err(|e| BlixardError::ConfigurationError {
                            component: "cluster".to_string(),
                            message: format!("Invalid NodeTicket '{}': {}", join_addr, e),
                        })?;
                    
                    let node_addr = node_ticket.node_addr();
                    tracing::info!("Using NodeTicket for direct P2P connection to NodeId: {}", node_addr.node_id);
                    
                    // Execute direct P2P join using the NodeAddr from the ticket
                    let (success, message, peers, voters) = self
                        .execute_p2p_join_request(
                            &node_addr,
                            &p2p_node_id,
                            &p2p_addresses,
                            &p2p_relay_url,
                        )
                        .await?;

                    if success {
                        tracing::info!("Successfully joined cluster via NodeTicket: {}", message);

                        // Update cluster configuration
                        self.update_cluster_configuration(&voters).await?;

                        // Store peer information from response
                        self.store_peer_information(&peers).await?;

                        return Ok(());
                    } else {
                        return Err(BlixardError::ClusterJoin { reason: message });
                    }
                }
                
                // Try discovery-based join if not a NodeTicket
                if let Ok(result) = self
                    .attempt_discovery_based_join(
                        join_addr,
                        leader_node_id,
                        &p2p_node_id,
                        &p2p_addresses,
                        &p2p_relay_url,
                    )
                    .await
                {
                    return Ok(result);
                }
                
                // If neither NodeTicket nor discovery worked, try HTTP bootstrap
                {
                    // If discovery didn't work, use HTTP bootstrap mechanism
                    let bootstrap_url = self.build_bootstrap_url(join_addr).await?;

                    // Fetch bootstrap info
                    let bootstrap_info = self.fetch_bootstrap_info(&bootstrap_url).await?;

                    // Create NodeAddr from bootstrap info
                    let node_addr = self.create_p2p_node_addr(&bootstrap_info).await?;

                    // Store the leader's P2P info
                    let peer_info = PeerInfo {
                        node_id: bootstrap_info.p2p_node_id.clone(),
                        address: join_addr.clone(),
                        last_seen: chrono::Utc::now(),
                        capabilities: vec![],
                        shared_resources: HashMap::new(),
                        connection_quality: ConnectionQuality {
                            latency_ms: 0,
                            bandwidth_mbps: 100.0,
                            packet_loss: 0.0,
                            reliability_score: 1.0,
                        },
                        p2p_node_id: Some(bootstrap_info.p2p_node_id.clone()),
                        p2p_addresses: vec![],
                        p2p_relay_url: None,
                        is_connected: false,
                    };
                    self.shared
                        .add_peer_with_p2p(bootstrap_info.node_id, peer_info).await;

                    // Execute HTTP bootstrap join
                    self.execute_http_bootstrap_join(
                        &bootstrap_info,
                        &node_addr,
                        &p2p_node_id,
                        &p2p_addresses,
                        &p2p_relay_url,
                    )
                    .await?;
                }
            }
        }
        Ok(())
    }

    /// Gather local P2P info if available
    async fn gather_local_p2p_info(&self) -> (Option<String>, Vec<String>, Option<String>) {
        if let Some(node_addr) = self.shared.get_p2p_node_addr().await {
            // Extract components from NodeAddr
            let node_id_str = node_addr.node_id.to_string();
            let addresses: Vec<String> = node_addr.direct_addresses()
                .map(|addr| addr.to_string())
                .collect();
            let relay_url = node_addr.relay_url().map(|url| url.to_string());
            (Some(node_id_str), addresses, relay_url)
        } else {
            (None, Vec::new(), None)
        }
    }

    /// Build bootstrap URL from join address
    async fn build_bootstrap_url(&self, join_addr: &str) -> BlixardResult<String> {
        tracing::info!("Attempting HTTP bootstrap from {}", join_addr);

        // Check if join_addr is a NodeTicket first
        if join_addr.starts_with("node") {
            // Parse as NodeTicket and extract NodeAddr
            let node_ticket = join_addr.parse::<iroh_base::ticket::NodeTicket>()
                .map_err(|e| BlixardError::ConfigurationError {
                    component: "cluster".to_string(),
                    message: format!("Invalid NodeTicket '{}': {}", join_addr, e),
                })?;
            
            let node_addr = node_ticket.node_addr();
            tracing::info!("Parsed NodeTicket - NodeId: {}, Direct addresses: {:?}", 
                node_addr.node_id, node_addr.direct_addresses().collect::<Vec<_>>());
            
            // For NodeTicket, we need to establish P2P connection first
            // Return a placeholder URL that won't be used in P2P flow
            return Ok("nodeticket://direct-p2p".to_string());
        }

        // Determine if join_addr is already an HTTP URL or a socket address
        let bootstrap_url = if join_addr.starts_with("http://") || join_addr.starts_with("https://")
        {
            // Already an HTTP URL, append /bootstrap if not present
            if join_addr.ends_with("/bootstrap") {
                join_addr.to_string()
            } else {
                format!("{}/bootstrap", join_addr.trim_end_matches('/'))
            }
        } else {
            // Parse as socket address and construct HTTP URL
            let addr: SocketAddr = join_addr.parse().map_err(|e| {
                BlixardError::ConfigurationError {
                    component: "cluster".to_string(),
                    message: format!("Invalid join address '{}': {}", join_addr, e),
                }
            })?;

            // Get metrics port offset from config
            let config = crate::config_global::get()?;
            let bootstrap_port = addr.port() + config.network.metrics.port_offset;
            format!("http://{}:{}/bootstrap", addr.ip(), bootstrap_port)
        };

        Ok(bootstrap_url)
    }

    /// Fetch bootstrap info from HTTP endpoint
    async fn fetch_bootstrap_info(
        &self,
        bootstrap_url: &str,
    ) -> BlixardResult<crate::iroh_types::BootstrapInfo> {
        tracing::debug!("Fetching bootstrap info from {}", bootstrap_url);
        
        // Extract host from URL for connection pooling
        let url = Url::parse(bootstrap_url).map_err(|e| {
            BlixardError::NetworkError(format!("Invalid bootstrap URL {}: {}", bootstrap_url, e))
        })?;
        let host = url.host_str().unwrap_or("localhost");

        // Use pooled HTTP client for better performance
        let client = crate::transport::http_client_pool::get_global_client(host).await?;
        let response = match client.get(bootstrap_url).await {
            Ok(resp) => resp,
            Err(e) => {
                return Err(BlixardError::NetworkError(format!(
                    "Failed to connect to bootstrap endpoint {}: {}",
                    bootstrap_url, e
                )));
            }
        };

        if !response.status().is_success() {
            return Err(BlixardError::ClusterJoin {
                reason: format!(
                    "Bootstrap endpoint returned {}: {}",
                    response.status(),
                    response.text().await.unwrap_or_default()
                ),
            });
        }

        let bootstrap_info: crate::iroh_types::BootstrapInfo =
            response.json().await.map_err(|e| {
                BlixardError::NetworkError(format!("Invalid bootstrap response: {}", e))
            })?;

        tracing::info!(
            "Got bootstrap info: node_id={}, p2p_node_id={}, addresses={:?}",
            bootstrap_info.node_id,
            bootstrap_info.p2p_node_id,
            bootstrap_info.p2p_addresses
        );

        Ok(bootstrap_info)
    }

    /// Create P2P NodeAddr from bootstrap info
    async fn create_p2p_node_addr(
        &self,
        bootstrap_info: &crate::iroh_types::BootstrapInfo,
    ) -> BlixardResult<iroh::NodeAddr> {
        // Parse the P2P node ID
        let p2p_node_id = bootstrap_info
            .p2p_node_id
            .parse::<iroh::NodeId>()
            .map_err(|e| BlixardError::ConfigurationError {
                component: "p2p".to_string(),
                message: format!("Invalid P2P node ID: {}", e),
            })?;

        // Create NodeAddr with the bootstrap info
        let mut node_addr = iroh::NodeAddr::new(p2p_node_id);
        // Collect all valid addresses and add them at once
        let addrs: Vec<SocketAddr> = bootstrap_info
            .p2p_addresses
            .iter()
            .filter_map(|addr_str| addr_str.parse().ok())
            .collect();
        if !addrs.is_empty() {
            node_addr = node_addr.with_direct_addresses(addrs);
        }

        // Add relay URL if available
        if let Some(relay_url) = &bootstrap_info.p2p_relay_url {
            if let Ok(url) = relay_url.parse() {
                node_addr = node_addr.with_relay_url(url);
            }
        }

        Ok(node_addr)
    }

    /// Attempt discovery-based join to cluster
    async fn attempt_discovery_based_join(
        &self,
        join_addr: &str,
        leader_node_id: u64,
        p2p_node_id: &Option<String>,
        p2p_addresses: &[String],
        p2p_relay_url: &Option<String>,
    ) -> BlixardResult<()> {
        if let Some(discovery_manager) = self.shared.get_discovery_manager() {
            // Try to find the leader's info via discovery
            let discovered_nodes = discovery_manager.get_nodes().await;

            // Look for a node that might be the leader (node ID 1 or at the join address)
            for node_info in discovered_nodes {
                if node_info
                    .addresses
                    .iter()
                    .any(|addr| addr.to_string() == *join_addr)
                {
                    tracing::info!(
                        "Found leader's P2P info via discovery: {}",
                        node_info.node_id
                    );

                    // Create NodeAddr from discovered info
                    let mut node_addr = iroh::NodeAddr::new(node_info.node_id);
                    for addr in &node_info.addresses {
                        node_addr = node_addr.with_direct_addresses([*addr]);
                    }

                    // Store the leader's P2P info
                    let peer_info = PeerInfo {
                        node_id: node_info.node_id.to_string(),
                        address: join_addr.to_string(),
                        last_seen: chrono::Utc::now(),
                        capabilities: vec![],
                        shared_resources: HashMap::new(),
                        connection_quality: ConnectionQuality {
                            latency_ms: 0,
                            bandwidth_mbps: 100.0,
                            packet_loss: 0.0,
                            reliability_score: 1.0,
                        },
                        p2p_node_id: Some(node_info.node_id.to_string()),
                        p2p_addresses: node_info.addresses.iter().map(|a| a.to_string()).collect(),
                        p2p_relay_url: None,
                        is_connected: false,
                    };
                    self.shared.add_peer_with_p2p(leader_node_id, peer_info).await;

                    // Now we can make the P2P connection
                    // Note: With Iroh, connections are established on-demand when we send the join request
                    tracing::info!("Connecting to leader via P2P at {}", node_info.node_id);

                    // Execute the P2P join request
                    let (success, message, peers, voters) = self
                        .execute_p2p_join_request(
                            &node_addr,
                            p2p_node_id,
                            p2p_addresses,
                            p2p_relay_url,
                        )
                        .await?;

                    if success {
                        tracing::info!("Successfully joined cluster via P2P: {}", message);

                        // Update cluster configuration
                        self.update_cluster_configuration(&voters).await?;

                        // Store peer information from response
                        self.store_peer_information(&peers).await?;

                        return Ok(());
                    } else {
                        return Err(BlixardError::ClusterJoin { reason: message });
                    }
                }
            }
        }

        Err(BlixardError::ClusterJoin {
            reason: "Discovery-based join failed".to_string(),
        })
    }

    /// Execute P2P join request
    async fn execute_p2p_join_request(
        &self,
        node_addr: &iroh::NodeAddr,
        p2p_node_id: &Option<String>,
        p2p_addresses: &[String],
        p2p_relay_url: &Option<String>,
    ) -> BlixardResult<(bool, String, Vec<crate::iroh_types::NodeInfo>, Vec<u64>)> {
        // Get the Iroh endpoint directly from shared state
        if let Some(endpoint) = self.shared.get_iroh_endpoint().await {
            // Create P2P client for the join request
            let transport_client = crate::transport::iroh_client::IrohClusterServiceClient::new(
                Arc::new(endpoint),
                node_addr.clone(),
            );

            // Send join request with our node info
            let our_node_id = self.shared.get_id();
            let our_bind_address = self.shared.get_bind_addr().to_string();
            
            // Get our P2P info
            let (our_p2p_node_id, our_p2p_addresses, our_p2p_relay_url) =
                if let Some(node_addr) = self.shared.get_p2p_node_addr().await {
                    let node_id_str = node_addr.node_id.to_string();
                    let addresses: Vec<String> = node_addr.direct_addresses()
                        .map(|addr| addr.to_string())
                        .collect();
                    let relay_url = node_addr.relay_url().map(|url| url.to_string());
                    (Some(node_id_str), addresses, relay_url)
                } else {
                    (p2p_node_id.clone(), p2p_addresses.to_vec(), p2p_relay_url.clone())
                };

            // Execute the join request
            transport_client
                .join_cluster(
                    our_node_id,
                    our_bind_address,
                    our_p2p_node_id,
                    our_p2p_addresses,
                    our_p2p_relay_url,
                )
                .await
        } else {
            Err(BlixardError::Internal {
                message: "Iroh endpoint not available for P2P join request".to_string(),
            })
        }
    }

    /// Update shared state after joining cluster
    #[allow(dead_code)] // Reserved for future cluster join state synchronization
    async fn update_shared_state_after_join(
        &self,
        _peers: &[crate::iroh_types::NodeInfo],
        _p2p_node_id: &Option<String>,
        _p2p_addresses: &[String],
        _p2p_relay_url: &Option<String>,
    ) -> BlixardResult<()> {
        // TODO: Implement proper state update after join
        // Temporarily stubbed out due to missing endpoint and node_addr variables
        tracing::warn!("update_shared_state_after_join not fully implemented");
        Ok(())
    }

    /// Store peer information from join response
    async fn store_peer_information(
        &self,
        peers: &[crate::iroh_types::NodeInfo],
    ) -> BlixardResult<()> {
        // Store all peer P2P info from the response and establish connections
        for peer_info in peers {
            if peer_info.id != self.shared.get_id() && !peer_info.p2p_node_id.is_empty() {
                // Store peer info
                let p2p_peer_info = PeerInfo {
                    node_id: peer_info.p2p_node_id.clone(),
                    address: peer_info.address.clone(),
                    last_seen: chrono::Utc::now(),
                    capabilities: vec![],
                    shared_resources: HashMap::new(),
                    connection_quality: ConnectionQuality {
                        latency_ms: 0,
                        bandwidth_mbps: 100.0,
                        packet_loss: 0.0,
                        reliability_score: 1.0,
                    },
                    p2p_node_id: Some(peer_info.p2p_node_id.clone()),
                    p2p_addresses: vec![],
                    p2p_relay_url: None,
                    is_connected: false,
                };
                self.shared.add_peer_with_p2p(peer_info.id, p2p_peer_info).await;

                // Establish P2P connection to this peer
                if let Ok(node_id_parsed) = peer_info.p2p_node_id.parse::<iroh::NodeId>() {
                    let mut node_addr = iroh::NodeAddr::new(node_id_parsed);

                    // Add relay URL if available
                    if !peer_info.p2p_relay_url.is_empty() {
                        if let Ok(relay_url) = peer_info.p2p_relay_url.parse() {
                            node_addr = node_addr.with_relay_url(relay_url);
                        }
                    }

                    // Add direct addresses
                    let addrs: Vec<SocketAddr> = peer_info
                        .p2p_addresses
                        .iter()
                        .filter_map(|addr_str| addr_str.parse().ok())
                        .collect();
                    if !addrs.is_empty() {
                        node_addr = node_addr.with_direct_addresses(addrs);
                    }

                    // Store peer P2P info for future connections
                    // With Iroh, connections are established on-demand when we actually need to communicate
                    tracing::info!(
                        "Stored P2P info for peer {} - connections will be established on-demand",
                        peer_info.id
                    );
                }
            }
        }
        Ok(())
    }

    /// Execute HTTP bootstrap join process
    async fn execute_http_bootstrap_join(
        &self,
        _bootstrap_info: &crate::iroh_types::BootstrapInfo,
        node_addr: &iroh::NodeAddr,
        _p2p_node_id: &Option<String>,
        _p2p_addresses: &[String],
        _p2p_relay_url: &Option<String>,
    ) -> BlixardResult<()> {
        // Now connect via P2P using Iroh endpoint directly
        if let Some(endpoint) = self.shared.get_iroh_endpoint().await {
            tracing::info!("Connecting to leader via P2P at {}", node_addr.node_id);
            
            // TODO: Implement P2P peer connection using endpoint directly
            // For now, we'll proceed with creating the transport client
            
            // Create P2P client and send join request
            let endpoint_arc = Arc::new(endpoint);
            let transport_client = crate::transport::iroh_client::IrohClusterServiceClient::new(
                endpoint_arc,
                node_addr.clone(),
            );

            // Now proceed with the join request via P2P
            let our_node_id = self.shared.get_id();
            let our_bind_address = self.shared.get_bind_addr().to_string();
            let (our_p2p_node_id, our_p2p_addresses, our_p2p_relay_url) =
                if let Some(node_addr) = self.shared.get_p2p_node_addr().await {
                    // Extract components from NodeAddr
                    let node_id_str = node_addr.node_id.to_string();
                    let addresses: Vec<String> = node_addr.direct_addresses()
                        .map(|addr| addr.to_string())
                        .collect();
                    let relay_url = node_addr.relay_url().map(|url| url.to_string());
                    (Some(node_id_str), addresses, relay_url)
                } else {
                    (None, Vec::new(), None)
                };

            match transport_client
                .join_cluster(
                    our_node_id,
                    our_bind_address,
                    our_p2p_node_id,
                    our_p2p_addresses,
                    our_p2p_relay_url,
                )
                .await
            {
                Ok((success, message, peers, voters)) => {
                    if success {
                        tracing::info!("Successfully joined cluster via P2P bootstrap");
                        tracing::info!(
                            "Join response contains {} peers and {} voters",
                            peers.len(),
                            voters.len()
                        );

                        // Update local configuration state with current cluster voters
                        self.update_cluster_configuration(&voters).await?;

                        // Store peer information from response
                        self.store_peer_information(&peers).await?;

                        Ok(())
                    } else {
                        Err(BlixardError::ClusterJoin {
                            reason: format!("Join request failed: {}", message),
                        })
                    }
                }
                Err(e) => {
                    Err(BlixardError::ClusterJoin {
                        reason: format!("Failed to send join request via P2P: {}", e),
                    })
                }
            }
        } else {
            Err(BlixardError::Internal {
                message: "Iroh endpoint not available for P2P cluster join".to_string(),
            })
        }
    }

    /// Update cluster configuration with voters
    async fn update_cluster_configuration(&self, voters: &[u64]) -> BlixardResult<()> {
        if !voters.is_empty() {
            let our_node_id = self.shared.get_id();
            let is_voter = voters.contains(&our_node_id);
            
            tracing::info!("[JOIN-CONFIG] Updating local configuration with voters: {:?}", voters);
            tracing::info!("[JOIN-CONFIG] Our node {} is {} a voter in this configuration", 
                our_node_id, if is_voter { "ALREADY" } else { "NOT" });
            
            if let Some(db) = self.shared.get_database().await {
                let storage = crate::raft_storage::RedbRaftStorage { database: db.clone() };
                let mut conf_state = raft::prelude::ConfState::default();
                conf_state.voters = voters.to_vec();
                
                if let Err(e) = storage.save_conf_state(&conf_state) {
                    tracing::warn!("Failed to save initial configuration state: {}", e);
                } else {
                    tracing::info!(
                        "[JOIN-CONFIG] Successfully saved configuration state with voters: {:?}",
                        voters
                    );
                    
                    // Apply configuration to running Raft node if available
                    if let Some(raft_manager) = self.shared.get_raft_manager().await {
                        tracing::info!("[JOIN-CONFIG] Applying configuration to running Raft node");
                        
                        // CRITICAL: This should only update the node's knowledge of the cluster
                        // It should NOT make the node a voter unless it's actually in the voters list
                        if let Err(e) = raft_manager.apply_initial_conf_state(conf_state.clone()).await {
                            tracing::warn!("[JOIN-CONFIG] Failed to apply initial conf state: {}", e);
                        } else {
                            tracing::info!("[JOIN-CONFIG] Successfully applied configuration with voters: {:?}", voters);
                            
                            if !is_voter {
                                tracing::info!("[JOIN-CONFIG] Node {} will remain follower until explicitly added as voter", our_node_id);
                            }
                        }
                    } else {
                        tracing::info!("[JOIN-CONFIG] RaftManager not yet available - configuration saved to storage");
                    }
                }
            } else {
                tracing::warn!("No database available to save configuration state");
            }
        } else {
            tracing::warn!("Join response contained empty voters list!");
        }
        Ok(())
    }

    /// Perform cluster join if configured
    pub(super) async fn join_cluster_if_configured(&mut self) -> BlixardResult<()> {
        if let Some(join_addr) = &self.shared.config.join_addr.clone() {
            // Wait for Raft to be ready
            self.wait_for_raft_ready().await?;

            // Pre-connect to the node we're joining
            self.pre_connect_to_leader(&join_addr).await?;

            // Send join request
            self.send_join_request().await?;

            // Wait for leader to be identified
            self.wait_for_leader_identification().await?;

            // Register as worker through Raft
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
            let topology = self.shared.config.topology.clone();
            self.shared
                .register_worker_through_raft(
                    self.shared.get_id(),
                    self.shared.get_bind_addr().to_string(),
                    capabilities,
                    topology,
                )
                .await?;
        } else {
            // Bootstrap mode - already registered as worker directly
            tracing::info!("Node {} bootstrapped successfully", self.shared.get_id());
        }

        Ok(())
    }

    /// Wait for Raft manager to be ready
    async fn wait_for_raft_ready(&self) -> BlixardResult<()> {
        let start = std::time::Instant::now();
        let timeout = Duration::from_secs(30);

        while start.elapsed() < timeout {
            // Check if core Raft dependencies are ready (not full node initialization)
            let has_database = self.shared.get_database().await.is_some();
            let has_endpoint = self.shared.get_iroh_endpoint().await.is_some();
            
            if has_database && has_endpoint {
                tracing::info!("Raft dependencies are ready (database and P2P endpoint available)");
                return Ok(());
            }
            
            tracing::debug!("Waiting for Raft dependencies - database: {}, endpoint: {}", has_database, has_endpoint);
            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        Err(BlixardError::Internal {
            message: "Timeout waiting for Raft dependencies (database and P2P endpoint) to initialize".to_string(),
        })
    }

    /// Pre-connect to the leader node
    async fn pre_connect_to_leader(&self, join_addr: &str) -> BlixardResult<()> {
        tracing::info!("Pre-connecting to leader at {}", join_addr);

        if let Some(_peer_connector) = self.shared.get_peer_connector() {
            let parts: Vec<&str> = join_addr.split(':').collect();
            if parts.len() == 2 {
                if let Ok(port) = parts[1].parse::<u16>() {
                    let addr = format!("{}:{}", parts[0], port);
                    // Note: Pre-connection will happen during join process when leader ID is known
                    tracing::debug!("Will connect to {} during join process", addr);
                }
            }
        }
        Ok(())
    }

    /// Wait for leader identification after joining
    async fn wait_for_leader_identification(&self) -> BlixardResult<()> {
        let start = std::time::Instant::now();
        let timeout = Duration::from_secs(30);

        tracing::info!("Waiting for leader identification...");

        while start.elapsed() < timeout {
            let raft_status = self.shared.get_raft_status().await;
            if let Some(leader_id) = raft_status.leader_id {
                tracing::info!("Leader identified: {}", leader_id);
                return Ok(());
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }

        Err(BlixardError::Internal {
            message: "Timeout waiting for leader identification".to_string(),
        })
    }

    /// Join a cluster and optionally register as a worker
    pub async fn join_cluster(
        &mut self,
        peer_addr: Option<std::net::SocketAddr>,
    ) -> BlixardResult<()> {
        // Check if initialized
        if !self.shared.is_initialized() {
            return Err(BlixardError::Internal {
                message: "Node not initialized".to_string(),
            });
        }

        if peer_addr.is_none() {
            // Bootstrap mode: register as worker immediately
            self.register_as_worker().await?;
        } else {
            // Join existing cluster - worker registration will happen later
            // after the node has successfully joined and can identify the leader
            tracing::info!(
                "Joining existing cluster at {:?} - worker registration deferred",
                peer_addr
            );
        }

        Ok(())
    }

    /// Register this node as a worker in the cluster
    pub async fn register_as_worker(&self) -> BlixardResult<()> {
        let node_id = self.shared.get_id();
        let address = self.shared.get_bind_addr().to_string();
        // Get system resources - use reasonable defaults
        let total_memory_mb = Self::estimate_available_memory();
        let disk_gb = Self::estimate_available_disk(&self.shared.config.data_dir);

        let capabilities = crate::raft_manager::WorkerCapabilities {
            cpu_cores: num_cpus::get() as u32,
            memory_mb: total_memory_mb,
            disk_gb,
            features: vec!["microvm".to_string()],
        };

        // Get topology from node config
        let topology = self.shared.config.topology.clone();

        // Check if we're the bootstrap node (no join_addr in config)
        if self.shared.config.join_addr.is_none() {
            // Bootstrap mode: When starting as a single-node cluster, we can write
            // directly to the database. This is the ONLY exception to the rule that
            // all state must go through Raft consensus.
            if let Some(db) = self.shared.get_database().await {
                let read_txn = db.begin_read()?;
                let worker_table = read_txn.open_table(crate::raft_storage::WORKER_TABLE)?;

                // Check if already registered
                if worker_table
                    .get(node_id.to_le_bytes().as_slice())?
                    .is_none()
                {
                    drop(read_txn);

                    // Not registered yet, register now
                    let write_txn = db.begin_write()?;
                    {
                        let mut worker_table =
                            write_txn.open_table(crate::raft_storage::WORKER_TABLE)?;
                        let mut status_table =
                            write_txn.open_table(crate::raft_storage::WORKER_STATUS_TABLE)?;

                        let worker_data = bincode::serialize(&(address.clone(), &capabilities))?;
                        worker_table
                            .insert(node_id.to_le_bytes().as_slice(), worker_data.as_slice())?;
                        status_table.insert(
                            node_id.to_le_bytes().as_slice(),
                            [crate::raft_manager::WorkerStatus::Online as u8].as_slice(),
                        )?;
                    }
                    write_txn.commit()?;

                    tracing::info!("Node {} registered as worker in bootstrap mode", node_id);
                } else {
                    tracing::info!("Node {} already registered as worker", node_id);
                }
            }
        } else {
            // Join existing cluster via Raft proposal - use the new method
            self.shared
                .register_worker_through_raft(node_id, address, capabilities, topology)
                .await?;
        }

        Ok(())
    }

    /// Leave the cluster
    pub async fn leave_cluster(&mut self) -> BlixardResult<()> {
        // Check if initialized
        if !self.shared.is_initialized() {
            return Err(BlixardError::Internal {
                message: "Node not initialized".to_string(),
            });
        }

        {
            let proposal_data = ProposalData::RemoveWorker {
                node_id: self.shared.get_id(),
            };

            let (response_tx, response_rx) = oneshot::channel();
            let proposal = RaftProposal {
                id: uuid::Uuid::new_v4().as_bytes().to_vec(),
                data: proposal_data,
                response_tx: Some(response_tx),
            };

            self.shared.send_raft_proposal(proposal).await?;

            // Wait for response
            response_rx.await.map_err(|_| BlixardError::Internal {
                message: "Leave proposal response channel closed".to_string(),
            })??;
        }

        Ok(())
    }

    /// Get cluster status
    pub async fn get_cluster_status(&self) -> BlixardResult<(u64, Vec<u64>, u64)> {
        self.shared.get_cluster_status().await
    }
}