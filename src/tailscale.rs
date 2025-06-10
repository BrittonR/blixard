use anyhow::{Result, Context};
use serde::Deserialize;
use tokio::process::Command;
use tracing::{debug, info};

#[derive(Debug, Deserialize)]
struct TailscaleStatus {
    #[serde(rename = "Self")]
    self_node: Option<TailscalePeer>,
    #[serde(rename = "Peer")]
    peers: Option<serde_json::Map<String, serde_json::Value>>,
}

#[derive(Debug, Deserialize)]
struct TailscalePeer {
    #[serde(rename = "HostName")]
    hostname: String,
    #[serde(rename = "TailscaleIPs")]
    tailscale_ips: Vec<String>,
    #[serde(rename = "Tags")]
    tags: Option<Vec<String>>,
}

pub struct TailscaleDiscovery;

impl TailscaleDiscovery {
    /// Get the current node's Tailscale hostname
    pub async fn get_hostname() -> Result<String> {
        let output = Command::new("tailscale")
            .args(&["status", "--json"])
            .output()
            .await
            .context("Failed to run tailscale status")?;
        
        if !output.status.success() {
            anyhow::bail!("tailscale status failed");
        }
        
        let status: TailscaleStatus = serde_json::from_slice(&output.stdout)
            .context("Failed to parse tailscale status")?;
        
        let self_node = status.self_node
            .ok_or_else(|| anyhow::anyhow!("No self node in tailscale status"))?;
        
        Ok(self_node.hostname)
    }
    
    /// Discover all Blixard nodes in the Tailscale network
    pub async fn discover_nodes() -> Result<Vec<NodeEndpoint>> {
        info!("Discovering Blixard nodes via Tailscale");
        
        let output = Command::new("tailscale")
            .args(&["status", "--json"])
            .output()
            .await
            .context("Failed to run tailscale status")?;
        
        if !output.status.success() {
            anyhow::bail!("tailscale status failed");
        }
        
        let status: serde_json::Value = serde_json::from_slice(&output.stdout)
            .context("Failed to parse tailscale status")?;
        
        let mut nodes = Vec::new();
        
        // Check self node
        if let Some(self_node) = status.get("Self") {
            if is_blixard_node(self_node) {
                if let Some(endpoint) = parse_node_endpoint(self_node) {
                    nodes.push(endpoint);
                }
            }
        }
        
        // Check peer nodes
        if let Some(peers) = status.get("Peer").and_then(|p| p.as_object()) {
            for (_id, peer) in peers {
                if is_blixard_node(peer) {
                    if let Some(endpoint) = parse_node_endpoint(peer) {
                        nodes.push(endpoint);
                    }
                }
            }
        }
        
        debug!("Discovered {} Blixard nodes", nodes.len());
        Ok(nodes)
    }
    
    /// Check if Tailscale is available and authenticated
    pub async fn is_available() -> bool {
        Command::new("tailscale")
            .arg("status")
            .output()
            .await
            .map(|output| output.status.success())
            .unwrap_or(false)
    }
}

fn is_blixard_node(node: &serde_json::Value) -> bool {
    // Check if node has the "blixard" tag
    if let Some(tags) = node.get("Tags").and_then(|t| t.as_array()) {
        tags.iter().any(|tag| {
            tag.as_str()
                .map(|s| s == "tag:blixard" || s.contains("blixard"))
                .unwrap_or(false)
        })
    } else {
        // If no tags, check hostname for "blixard" prefix
        node.get("HostName")
            .and_then(|h| h.as_str())
            .map(|h| h.starts_with("blixard-"))
            .unwrap_or(false)
    }
}

fn parse_node_endpoint(node: &serde_json::Value) -> Option<NodeEndpoint> {
    let hostname = node.get("HostName")?.as_str()?.to_string();
    
    let tailscale_ips: Vec<String> = node
        .get("TailscaleIPs")
        .and_then(|ips| ips.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|ip| ip.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();
    
    if tailscale_ips.is_empty() {
        return None;
    }
    
    // Extract node ID from hostname (e.g., "blixard-1" -> 1)
    let node_id = hostname
        .strip_prefix("blixard-")
        .and_then(|s| s.parse::<u64>().ok());
    
    Some(NodeEndpoint {
        hostname,
        tailscale_ip: tailscale_ips[0].clone(),
        node_id,
    })
}

#[derive(Debug, Clone)]
pub struct NodeEndpoint {
    pub hostname: String,
    pub tailscale_ip: String,
    pub node_id: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_tailscale_available() {
        // This will only pass if tailscale is installed
        let available = TailscaleDiscovery::is_available().await;
        println!("Tailscale available: {}", available);
    }
}