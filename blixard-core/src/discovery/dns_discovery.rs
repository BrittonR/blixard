//! DNS-based discovery provider using SRV and TXT records.
//!
//! This provider queries DNS for service records to discover Iroh nodes.
//! It supports both standard DNS and DNS-SD (DNS Service Discovery).

use async_trait::async_trait;
use iroh::NodeId;
use std::net::{IpAddr, SocketAddr};
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, RwLock};
use tokio::time::interval;
use tracing::{debug, error, info, warn};
use trust_dns_resolver::config::{ResolverConfig, ResolverOpts};
use trust_dns_resolver::TokioAsyncResolver;

use crate::discovery::{DiscoveryEvent, DiscoveryProvider, DiscoveryState, IrohNodeInfo};
use crate::error::{BlixardError, BlixardResult};

/// DNS discovery provider that queries DNS for node information
pub struct DnsDiscoveryProvider {
    /// DNS domains to query
    domains: Vec<String>,

    /// DNS resolver
    resolver: TokioAsyncResolver,

    /// Shared discovery state
    state: Arc<DiscoveryState>,

    /// Whether the provider is running
    running: Arc<RwLock<bool>>,

    /// Refresh interval
    refresh_interval: Duration,

    /// Background task handle
    task_handle: Arc<RwLock<Option<tokio::task::JoinHandle<()>>>>,

    /// Event channel for subscribers
    event_sender: mpsc::Sender<DiscoveryEvent>,
    /// Event receiver for external subscribers to discovery events
    /// Currently unused pending external event subscription API
    #[allow(dead_code)]
    event_receiver: Arc<RwLock<Option<mpsc::Receiver<DiscoveryEvent>>>>,
}

impl DnsDiscoveryProvider {
    /// Create a new DNS discovery provider
    pub fn new(domains: Vec<String>, refresh_interval: Duration) -> BlixardResult<Self> {
        let (tx, rx) = mpsc::channel(100);

        // Create DNS resolver
        let resolver =
            TokioAsyncResolver::tokio(ResolverConfig::default(), ResolverOpts::default());

        Ok(Self {
            domains,
            resolver,
            state: Arc::new(DiscoveryState::new()),
            running: Arc::new(RwLock::new(false)),
            refresh_interval,
            task_handle: Arc::new(RwLock::new(None)),
            event_sender: tx,
            event_receiver: Arc::new(RwLock::new(Some(rx))),
        })
    }

    /// Query DNS for nodes in a specific domain
    async fn query_domain(&self, domain: &str) -> BlixardResult<Vec<IrohNodeInfo>> {
        let mut discovered_nodes = Vec::new();

        // Query SRV records
        debug!("Querying SRV records for domain: {}", domain);

        match self.resolver.srv_lookup(domain).await {
            Ok(srv_response) => {
                for srv in srv_response.iter() {
                    // The target hostname should encode the node ID
                    let target = srv.target().to_string();

                    // Try to extract node ID from the hostname
                    // Expected format: <node-id>.<domain>
                    if let Some(node_id_str) = target.split('.').next() {
                        if let Ok(node_id) = node_id_str.parse::<NodeId>() {
                            // Resolve the target to get IP addresses
                            let port = srv.port();

                            match self.resolver.ipv4_lookup(srv.target().clone()).await {
                                Ok(ipv4_response) => {
                                    let addresses: Vec<SocketAddr> = ipv4_response
                                        .iter()
                                        .map(|ip| SocketAddr::new(IpAddr::V4(ip.0), port))
                                        .collect();

                                    if !addresses.is_empty() {
                                        let mut info = IrohNodeInfo::new(node_id, addresses);
                                        info.metadata
                                            .insert("source".to_string(), "dns".to_string());
                                        info.metadata
                                            .insert("domain".to_string(), domain.to_string());
                                        discovered_nodes.push(info);
                                    }
                                }
                                Err(e) => {
                                    warn!("Failed to resolve IPv4 for {}: {}", srv.target(), e);
                                }
                            }

                            // Also try IPv6
                            match self.resolver.ipv6_lookup(srv.target().clone()).await {
                                Ok(ipv6_response) => {
                                    let addresses: Vec<SocketAddr> = ipv6_response
                                        .iter()
                                        .map(|ip| SocketAddr::new(IpAddr::V6(ip.0), port))
                                        .collect();

                                    if !addresses.is_empty() {
                                        if let Some(existing) = discovered_nodes
                                            .iter_mut()
                                            .find(|n| n.node_id == node_id)
                                        {
                                            existing.addresses.extend(addresses);
                                        }
                                    }
                                }
                                Err(e) => {
                                    debug!("Failed to resolve IPv6 for {}: {}", srv.target(), e);
                                }
                            }
                        }
                    }
                }
            }
            Err(e) => {
                debug!("No SRV records found for {}: {}", domain, e);
            }
        }

        // Query TXT records for additional metadata
        match self.resolver.txt_lookup(domain).await {
            Ok(txt_response) => {
                for txt in txt_response.iter() {
                    for data in txt.iter() {
                        if let Ok(text) = std::str::from_utf8(data) {
                            // Parse TXT record for node information
                            // Expected format: "node=<node_id> addr=<address>"
                            self.parse_txt_record(text, &mut discovered_nodes);
                        }
                    }
                }
            }
            Err(e) => {
                debug!("No TXT records found for {}: {}", domain, e);
            }
        }

        Ok(discovered_nodes)
    }

    /// Parse a TXT record for node information
    fn parse_txt_record(&self, text: &str, nodes: &mut Vec<IrohNodeInfo>) {
        let mut node_id = None;
        let mut addresses = Vec::new();

        for part in text.split_whitespace() {
            if let Some(value) = part.strip_prefix("node=") {
                node_id = value.parse::<NodeId>().ok();
            } else if let Some(value) = part.strip_prefix("addr=") {
                if let Ok(addr) = SocketAddr::from_str(value) {
                    addresses.push(addr);
                }
            }
        }

        if let Some(id) = node_id {
            if !addresses.is_empty() {
                let mut info = IrohNodeInfo::new(id, addresses);
                info.metadata
                    .insert("source".to_string(), "dns-txt".to_string());
                nodes.push(info);
            }
        }
    }

    /// Background task to periodically refresh DNS records
    async fn refresh_task(provider: Arc<RwLock<Self>>) {
        let mut refresh_timer = interval(provider.read().await.refresh_interval);

        loop {
            refresh_timer.tick().await;

            let provider_read = provider.read().await;
            if !*provider_read.running.read().await {
                break;
            }

            info!("Refreshing DNS discovery records");

            // Query all domains
            for domain in &provider_read.domains {
                match provider_read.query_domain(domain).await {
                    Ok(nodes) => {
                        for node in nodes {
                            provider_read.state.add_node(node.clone()).await;
                            let _ = provider_read
                                .event_sender
                                .send(DiscoveryEvent::NodeDiscovered(node))
                                .await;
                        }
                    }
                    Err(e) => {
                        error!("Failed to query domain {}: {}", domain, e);
                    }
                }
            }

            // Clean up stale nodes
            provider_read.state.cleanup_stale_nodes(300).await; // 5 minutes
        }
    }
}

#[async_trait]
impl DiscoveryProvider for DnsDiscoveryProvider {
    async fn start(&mut self) -> BlixardResult<()> {
        let mut running = self.running.write().await;
        if *running {
            return Err(BlixardError::Internal {
                message: "DNS discovery provider is already running".to_string(),
            });
        }

        info!(
            "Starting DNS discovery provider for domains: {:?}",
            self.domains
        );

        // Do initial query
        for domain in &self.domains.clone() {
            match self.query_domain(domain).await {
                Ok(nodes) => {
                    info!("Found {} nodes in domain {}", nodes.len(), domain);
                    for node in nodes {
                        self.state.add_node(node.clone()).await;
                        let _ = self
                            .event_sender
                            .send(DiscoveryEvent::NodeDiscovered(node))
                            .await;
                    }
                }
                Err(e) => {
                    warn!("Failed to query domain {}: {}", domain, e);
                }
            }
        }

        *running = true;

        // Start background refresh task
        let provider = Arc::new(RwLock::new(self.clone()));
        let handle = tokio::spawn(Self::refresh_task(provider));
        *self.task_handle.write().await = Some(handle);

        Ok(())
    }

    async fn stop(&mut self) -> BlixardResult<()> {
        let mut running = self.running.write().await;
        if !*running {
            return Ok(());
        }

        info!("Stopping DNS discovery provider");

        *running = false;

        // Cancel background task
        if let Some(handle) = self.task_handle.write().await.take() {
            handle.abort();
        }

        // Clear DNS-discovered nodes
        let nodes = self.state.nodes.read().await;
        let dns_nodes: Vec<NodeId> = nodes
            .iter()
            .filter(|(_, info)| {
                info.metadata.get("source") == Some(&"dns".to_string())
                    || info.metadata.get("source") == Some(&"dns-txt".to_string())
            })
            .map(|(id, _)| *id)
            .collect();
        drop(nodes);

        for node_id in dns_nodes {
            self.state.remove_node(node_id).await;
        }

        Ok(())
    }

    async fn get_nodes(&self) -> BlixardResult<Vec<IrohNodeInfo>> {
        let nodes = self.state.nodes.read().await;
        Ok(nodes.values().cloned().collect())
    }

    async fn subscribe(&self) -> BlixardResult<mpsc::Receiver<DiscoveryEvent>> {
        let (tx, rx) = mpsc::channel(100);
        self.state.subscribers.write().await.push(tx);
        Ok(rx)
    }

    fn name(&self) -> &str {
        "dns"
    }

    fn is_running(&self) -> bool {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async { *self.running.read().await })
        })
    }
}

// Implement Clone manually due to resolver
impl Clone for DnsDiscoveryProvider {
    fn clone(&self) -> Self {
        let (tx, rx) = mpsc::channel(100);

        Self {
            domains: self.domains.clone(),
            resolver: TokioAsyncResolver::tokio(ResolverConfig::default(), ResolverOpts::default()),
            state: self.state.clone(),
            running: self.running.clone(),
            refresh_interval: self.refresh_interval,
            task_handle: Arc::new(RwLock::new(None)),
            event_sender: tx,
            event_receiver: Arc::new(RwLock::new(Some(rx))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_dns_discovery_creation() {
        let domains = vec!["_iroh._tcp.example.com".to_string()];
        let provider = DnsDiscoveryProvider::new(domains, Duration::from_secs(60)).unwrap();

        assert!(!provider.is_running());
        assert_eq!(provider.name(), "dns");
    }

    #[test]
    fn test_txt_record_parsing() {
        let domains = vec![];
        let provider = DnsDiscoveryProvider::new(domains, Duration::from_secs(60)).unwrap();

        let mut nodes = Vec::new();

        // Valid record
        let node_id = NodeId::from_bytes(&[1u8; 32]).unwrap();
        let txt = format!("node={} addr=127.0.0.1:8080", node_id);
        provider.parse_txt_record(&txt, &mut nodes);

        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].node_id, node_id);
        assert_eq!(nodes[0].addresses.len(), 1);

        // Invalid node ID
        nodes.clear();
        provider.parse_txt_record("node=invalid addr=127.0.0.1:8080", &mut nodes);
        assert_eq!(nodes.len(), 0);

        // Invalid address
        nodes.clear();
        let txt = format!("node={} addr=invalid", node_id);
        provider.parse_txt_record(&txt, &mut nodes);
        assert_eq!(nodes.len(), 0);
    }
}
