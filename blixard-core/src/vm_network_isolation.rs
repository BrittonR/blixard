//! VM Network Isolation Module
//!
//! This module provides network isolation for VMs using iptables/nftables rules
//! to ensure VMs cannot access cluster internal networks and are properly isolated
//! from each other based on tenant boundaries.

use crate::error::{BlixardError, BlixardResult};
use crate::types::VmConfig;
use std::collections::HashMap;
use std::process::Command;
use tracing::{info, warn, debug};

/// Network isolation backend type
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FirewallBackend {
    /// Traditional iptables
    Iptables,
    /// Modern nftables (recommended)
    Nftables,
}

/// Network isolation configuration
#[derive(Debug, Clone)]
pub struct NetworkIsolationConfig {
    /// Firewall backend to use
    pub backend: FirewallBackend,
    
    /// Cluster internal network CIDRs to protect
    pub cluster_networks: Vec<String>,
    
    /// VM network CIDR (e.g., "10.0.0.0/16")
    pub vm_network: String,
    
    /// Enable inter-VM isolation (prevent VMs from talking to each other)
    pub inter_vm_isolation: bool,
    
    /// Enable tenant isolation (VMs in different tenants can't communicate)
    pub tenant_isolation: bool,
    
    /// Allow outbound internet access for VMs
    pub allow_internet: bool,
    
    /// Custom allowed ports for VMs (e.g., DNS)
    pub allowed_ports: Vec<AllowedPort>,
}

/// Allowed port configuration
#[derive(Debug, Clone)]
pub struct AllowedPort {
    /// Protocol (tcp/udp)
    pub protocol: String,
    /// Port number
    pub port: u16,
    /// Direction (inbound/outbound)
    pub direction: PortDirection,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PortDirection {
    Inbound,
    Outbound,
}

/// VM network isolation manager
pub struct NetworkIsolationManager {
    config: NetworkIsolationConfig,
    /// Tenant to VM IP mappings
    tenant_networks: HashMap<String, Vec<String>>,
}

impl Default for NetworkIsolationConfig {
    fn default() -> Self {
        Self {
            backend: FirewallBackend::Nftables,
            cluster_networks: vec![
                "127.0.0.0/8".to_string(),      // Loopback
                "169.254.0.0/16".to_string(),   // Link-local
                "172.16.0.0/12".to_string(),    // Private range (for cluster)
            ],
            vm_network: "10.0.0.0/16".to_string(),
            inter_vm_isolation: false,
            tenant_isolation: true,
            allow_internet: true,
            allowed_ports: vec![
                AllowedPort {
                    protocol: "udp".to_string(),
                    port: 53,
                    direction: PortDirection::Outbound,
                },
                AllowedPort {
                    protocol: "tcp".to_string(),
                    port: 53,
                    direction: PortDirection::Outbound,
                },
            ],
        }
    }
}

impl NetworkIsolationManager {
    /// Create a new network isolation manager
    pub fn new(config: NetworkIsolationConfig) -> Self {
        Self {
            config,
            tenant_networks: HashMap::new(),
        }
    }
    
    /// Initialize firewall rules for VM isolation
    pub async fn initialize(&self) -> BlixardResult<()> {
        info!("Initializing VM network isolation with {:?} backend", self.config.backend);
        
        match self.config.backend {
            FirewallBackend::Iptables => self.init_iptables().await,
            FirewallBackend::Nftables => self.init_nftables().await,
        }
    }
    
    /// Add firewall rules for a new VM
    pub async fn add_vm_rules(&mut self, vm_config: &VmConfig, vm_ip: &str, tenant_id: &str) -> BlixardResult<()> {
        info!("Adding network isolation rules for VM {} (IP: {}, Tenant: {})", 
              vm_config.name, vm_ip, tenant_id);
        
        // Track tenant network
        self.tenant_networks.entry(tenant_id.to_string())
            .or_insert_with(Vec::new)
            .push(vm_ip.to_string());
        
        match self.config.backend {
            FirewallBackend::Iptables => self.add_vm_iptables_rules(vm_config, vm_ip, tenant_id).await,
            FirewallBackend::Nftables => self.add_vm_nftables_rules(vm_config, vm_ip, tenant_id).await,
        }
    }
    
    /// Remove firewall rules for a VM
    pub async fn remove_vm_rules(&mut self, vm_name: &str, vm_ip: &str, tenant_id: &str) -> BlixardResult<()> {
        info!("Removing network isolation rules for VM {} (IP: {})", vm_name, vm_ip);
        
        // Remove from tenant tracking
        if let Some(ips) = self.tenant_networks.get_mut(tenant_id) {
            ips.retain(|ip| ip != vm_ip);
        }
        
        match self.config.backend {
            FirewallBackend::Iptables => self.remove_vm_iptables_rules(vm_name, vm_ip).await,
            FirewallBackend::Nftables => self.remove_vm_nftables_rules(vm_name, vm_ip).await,
        }
    }
    
    /// Initialize iptables rules
    async fn init_iptables(&self) -> BlixardResult<()> {
        // Create custom chains for VM isolation
        let chains = vec!["BLIXARD_VM_IN", "BLIXARD_VM_OUT", "BLIXARD_VM_FWD"];
        
        for chain in chains {
            self.run_iptables(&["-t", "filter", "-N", chain], true).await?;
        }
        
        // Hook into main chains
        self.run_iptables(&["-t", "filter", "-I", "FORWARD", "-j", "BLIXARD_VM_FWD"], false).await?;
        self.run_iptables(&["-t", "filter", "-I", "INPUT", "-j", "BLIXARD_VM_IN"], false).await?;
        self.run_iptables(&["-t", "filter", "-I", "OUTPUT", "-j", "BLIXARD_VM_OUT"], false).await?;
        
        // Default policies - block VM access to cluster networks
        for network in &self.config.cluster_networks {
            // Block VMs from accessing cluster networks
            self.run_iptables(&[
                "-t", "filter", "-A", "BLIXARD_VM_FWD",
                "-s", &self.config.vm_network,
                "-d", network,
                "-j", "DROP"
            ], false).await?;
        }
        
        // Allow established connections
        self.run_iptables(&[
            "-t", "filter", "-A", "BLIXARD_VM_FWD",
            "-m", "state", "--state", "ESTABLISHED,RELATED",
            "-j", "ACCEPT"
        ], false).await?;
        
        Ok(())
    }
    
    /// Initialize nftables rules
    async fn init_nftables(&self) -> BlixardResult<()> {
        // Create nftables table and chains
        let nft_rules = format!(r#"
table inet blixard {{
    chain vm_input {{
        type filter hook input priority 0; policy accept;
    }}
    
    chain vm_output {{
        type filter hook output priority 0; policy accept;
    }}
    
    chain vm_forward {{
        type filter hook forward priority 0; policy accept;
        
        # Block VM access to cluster networks
{}
        
        # Allow established connections
        ct state established,related accept
        
        # Default VM isolation rules will be added here
    }}
}}
"#, 
        self.config.cluster_networks.iter()
            .map(|net| format!("        ip saddr {} ip daddr {} drop", self.config.vm_network, net))
            .collect::<Vec<_>>()
            .join("\n")
        );
        
        self.run_nft(&["add", "table", "inet", "blixard"], true).await?;
        self.run_nft_rules(&nft_rules).await?;
        
        Ok(())
    }
    
    /// Add iptables rules for a specific VM
    async fn add_vm_iptables_rules(&self, _vm_config: &VmConfig, vm_ip: &str, tenant_id: &str) -> BlixardResult<()> {
        // Block VM from accessing host
        self.run_iptables(&[
            "-t", "filter", "-A", "BLIXARD_VM_IN",
            "-s", vm_ip,
            "-j", "DROP"
        ], false).await?;
        
        // Inter-VM isolation
        if self.config.inter_vm_isolation {
            self.run_iptables(&[
                "-t", "filter", "-A", "BLIXARD_VM_FWD",
                "-s", vm_ip,
                "-d", &self.config.vm_network,
                "-j", "DROP"
            ], false).await?;
        }
        
        // Tenant isolation
        if self.config.tenant_isolation {
            // Allow communication within same tenant
            if let Some(tenant_ips) = self.tenant_networks.get(tenant_id) {
                for allowed_ip in tenant_ips {
                    if allowed_ip != vm_ip {
                        self.run_iptables(&[
                            "-t", "filter", "-I", "BLIXARD_VM_FWD",
                            "-s", vm_ip,
                            "-d", allowed_ip,
                            "-j", "ACCEPT"
                        ], false).await?;
                    }
                }
            }
        }
        
        // Allow specific ports
        for port in &self.config.allowed_ports {
            if port.direction == PortDirection::Outbound {
                self.run_iptables(&[
                    "-t", "filter", "-A", "BLIXARD_VM_FWD",
                    "-s", vm_ip,
                    "-p", &port.protocol,
                    "--dport", &port.port.to_string(),
                    "-j", "ACCEPT"
                ], false).await?;
            }
        }
        
        // Allow internet access if configured
        if self.config.allow_internet {
            self.run_iptables(&[
                "-t", "filter", "-A", "BLIXARD_VM_FWD",
                "-s", vm_ip,
                "!", "-d", &self.config.vm_network,
                "-j", "ACCEPT"
            ], false).await?;
        }
        
        Ok(())
    }
    
    /// Add nftables rules for a specific VM
    async fn add_vm_nftables_rules(&self, vm_config: &VmConfig, vm_ip: &str, tenant_id: &str) -> BlixardResult<()> {
        let vm_chain = format!("vm_{}", vm_config.name.replace("-", "_"));
        
        // Create VM-specific chain
        self.run_nft(&["add", "chain", "inet", "blixard", &vm_chain], false).await?;
        
        // Add jump rule to VM chain
        self.run_nft(&[
            "add", "rule", "inet", "blixard", "vm_forward",
            "ip", "saddr", vm_ip, "jump", &vm_chain
        ], false).await?;
        
        // Block VM from accessing host
        self.run_nft(&[
            "add", "rule", "inet", "blixard", "vm_input",
            "ip", "saddr", vm_ip, "drop"
        ], false).await?;
        
        // Inter-VM isolation
        if self.config.inter_vm_isolation {
            self.run_nft(&[
                "add", "rule", "inet", "blixard", &vm_chain,
                "ip", "daddr", &self.config.vm_network, "drop"
            ], false).await?;
        }
        
        // Tenant isolation
        if self.config.tenant_isolation {
            // Allow communication within same tenant
            if let Some(tenant_ips) = self.tenant_networks.get(tenant_id) {
                for allowed_ip in tenant_ips {
                    if allowed_ip != vm_ip {
                        self.run_nft(&[
                            "add", "rule", "inet", "blixard", &vm_chain,
                            "ip", "daddr", allowed_ip, "accept"
                        ], false).await?;
                    }
                }
            }
            
            // Block other tenant traffic
            self.run_nft(&[
                "add", "rule", "inet", "blixard", &vm_chain,
                "ip", "daddr", &self.config.vm_network, "drop"
            ], false).await?;
        }
        
        // Allow specific ports
        for port in &self.config.allowed_ports {
            if port.direction == PortDirection::Outbound {
                self.run_nft(&[
                    "add", "rule", "inet", "blixard", &vm_chain,
                    &port.protocol, "dport", &port.port.to_string(), "accept"
                ], false).await?;
            }
        }
        
        // Allow internet access if configured
        if self.config.allow_internet {
            self.run_nft(&[
                "add", "rule", "inet", "blixard", &vm_chain,
                "ip", "daddr", "!=", &self.config.vm_network, "accept"
            ], false).await?;
        }
        
        Ok(())
    }
    
    /// Remove iptables rules for a VM
    async fn remove_vm_iptables_rules(&self, vm_name: &str, vm_ip: &str) -> BlixardResult<()> {
        // Remove rules with VM IP - this is simplified, in production we'd track rule numbers
        warn!("Removing iptables rules for VM {} - manual cleanup may be required", vm_name);
        
        // Try to remove common rules
        self.run_iptables(&[
            "-t", "filter", "-D", "BLIXARD_VM_IN",
            "-s", vm_ip,
            "-j", "DROP"
        ], true).await?;
        
        Ok(())
    }
    
    /// Remove nftables rules for a VM
    async fn remove_vm_nftables_rules(&self, vm_name: &str, vm_ip: &str) -> BlixardResult<()> {
        let vm_chain = format!("vm_{}", vm_name.replace("-", "_"));
        
        // Remove jump rule
        self.run_nft(&[
            "delete", "rule", "inet", "blixard", "vm_forward",
            "ip", "saddr", vm_ip, "jump", &vm_chain
        ], true).await?;
        
        // Delete VM chain
        self.run_nft(&["delete", "chain", "inet", "blixard", &vm_chain], true).await?;
        
        // Remove input rule
        self.run_nft(&[
            "delete", "rule", "inet", "blixard", "vm_input",
            "ip", "saddr", vm_ip, "drop"
        ], true).await?;
        
        Ok(())
    }
    
    /// Run iptables command
    async fn run_iptables(&self, args: &[&str], ignore_errors: bool) -> BlixardResult<()> {
        debug!("Running iptables command: iptables {}", args.join(" "));
        
        let output = Command::new("iptables")
            .args(args)
            .output()
            .map_err(|e| BlixardError::Internal {
                message: format!("Failed to run iptables: {}", e),
            })?;
        
        if !output.status.success() && !ignore_errors {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(BlixardError::Internal {
                message: format!("iptables command failed: {}", stderr),
            });
        }
        
        Ok(())
    }
    
    /// Run nft command
    async fn run_nft(&self, args: &[&str], ignore_errors: bool) -> BlixardResult<()> {
        debug!("Running nft command: nft {}", args.join(" "));
        
        let output = Command::new("nft")
            .args(args)
            .output()
            .map_err(|e| BlixardError::Internal {
                message: format!("Failed to run nft: {}", e),
            })?;
        
        if !output.status.success() && !ignore_errors {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(BlixardError::Internal {
                message: format!("nft command failed: {}", stderr),
            });
        }
        
        Ok(())
    }
    
    /// Run nft with rules from stdin
    async fn run_nft_rules(&self, rules: &str) -> BlixardResult<()> {
        use std::io::Write;
        use std::process::Stdio;
        
        debug!("Applying nft rules:\n{}", rules);
        
        let mut child = Command::new("nft")
            .arg("-f")
            .arg("-")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| BlixardError::Internal {
                message: format!("Failed to spawn nft: {}", e),
            })?;
        
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(rules.as_bytes())
                .map_err(|e| BlixardError::Internal {
                    message: format!("Failed to write nft rules: {}", e),
                })?;
        }
        
        let output = child.wait_with_output()
            .map_err(|e| BlixardError::Internal {
                message: format!("Failed to execute nft: {}", e),
            })?;
        
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(BlixardError::Internal {
                message: format!("nft rules failed: {}", stderr),
            });
        }
        
        Ok(())
    }
    
    /// Get current firewall rules (for debugging)
    pub async fn get_rules(&self) -> BlixardResult<String> {
        match self.config.backend {
            FirewallBackend::Iptables => {
                let output = Command::new("iptables")
                    .args(&["-t", "filter", "-L", "-n", "-v"])
                    .output()
                    .map_err(|e| BlixardError::Internal {
                        message: format!("Failed to list iptables rules: {}", e),
                    })?;
                
                Ok(String::from_utf8_lossy(&output.stdout).to_string())
            }
            FirewallBackend::Nftables => {
                let output = Command::new("nft")
                    .args(&["list", "table", "inet", "blixard"])
                    .output()
                    .map_err(|e| BlixardError::Internal {
                        message: format!("Failed to list nft rules: {}", e),
                    })?;
                
                Ok(String::from_utf8_lossy(&output.stdout).to_string())
            }
        }
    }
}

/// Check if running with sufficient privileges for firewall management
pub fn check_firewall_privileges() -> BlixardResult<()> {
    // Check if we can run iptables/nft commands
    let output = Command::new("id")
        .arg("-u")
        .output()
        .map_err(|e| BlixardError::Security {
            message: format!("Failed to check user ID: {}", e),
        })?;
    
    let uid = String::from_utf8_lossy(&output.stdout).trim().to_string();
    
    if uid != "0" {
        // Check if we have CAP_NET_ADMIN capability
        let cap_output = Command::new("capsh")
            .args(&["--print"])
            .output();
        
        if let Ok(output) = cap_output {
            let caps = String::from_utf8_lossy(&output.stdout);
            if !caps.contains("cap_net_admin") {
                return Err(BlixardError::Security {
                    message: "Firewall management requires root privileges or CAP_NET_ADMIN capability".to_string(),
                });
            }
        } else {
            return Err(BlixardError::Security {
                message: "Firewall management requires root privileges".to_string(),
            });
        }
    }
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_default_config() {
        let config = NetworkIsolationConfig::default();
        assert_eq!(config.backend, FirewallBackend::Nftables);
        assert!(config.tenant_isolation);
        assert!(config.allow_internet);
        assert!(!config.inter_vm_isolation);
    }
    
    #[test]
    fn test_network_isolation_manager() {
        let config = NetworkIsolationConfig::default();
        let manager = NetworkIsolationManager::new(config);
        assert!(manager.tenant_networks.is_empty());
    }
}