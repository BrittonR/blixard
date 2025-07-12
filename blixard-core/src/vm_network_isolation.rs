//! VM Network Isolation Module
//!
//! This module provides network isolation for VMs using iptables/nftables rules
//! to ensure VMs cannot access cluster internal networks and are properly isolated
//! from each other based on tenant boundaries.

use crate::abstractions::command::{CommandExecutor, CommandOptions, TokioCommandExecutor};
use crate::error::{BlixardError, BlixardResult};
use crate::types::VmConfig;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, info, warn};

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
    /// Command executor for running firewall commands
    command_executor: Arc<dyn CommandExecutor>,
}

impl Default for NetworkIsolationConfig {
    fn default() -> Self {
        Self {
            backend: FirewallBackend::Nftables,
            cluster_networks: vec![
                "127.0.0.0/8".to_string(),    // Loopback
                "169.254.0.0/16".to_string(), // Link-local
                "172.16.0.0/12".to_string(),  // Private range (for cluster)
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
    /// Create a new network isolation manager with default command executor
    pub fn new(config: NetworkIsolationConfig) -> Self {
        Self::with_executor(config, Arc::new(TokioCommandExecutor::new()))
    }

    /// Create a new network isolation manager with custom command executor  
    pub fn with_executor(
        config: NetworkIsolationConfig,
        command_executor: Arc<dyn CommandExecutor>,
    ) -> Self {
        Self {
            config,
            tenant_networks: HashMap::new(),
            command_executor,
        }
    }

    /// Initialize firewall rules for VM isolation
    pub async fn initialize(&self) -> BlixardResult<()> {
        info!(
            "Initializing VM network isolation with {:?} backend",
            self.config.backend
        );

        match self.config.backend {
            FirewallBackend::Iptables => self.init_iptables().await,
            FirewallBackend::Nftables => self.init_nftables().await,
        }
    }

    /// Add firewall rules for a new VM
    pub async fn add_vm_rules(
        &mut self,
        vm_config: &VmConfig,
        vm_ip: &str,
        tenant_id: &str,
    ) -> BlixardResult<()> {
        info!(
            "Adding network isolation rules for VM {} (IP: {}, Tenant: {})",
            vm_config.name, vm_ip, tenant_id
        );

        // Track tenant network
        self.tenant_networks
            .entry(tenant_id.to_string())
            .or_insert_with(Vec::new)
            .push(vm_ip.to_string());

        match self.config.backend {
            FirewallBackend::Iptables => {
                self.add_vm_iptables_rules(vm_config, vm_ip, tenant_id)
                    .await
            }
            FirewallBackend::Nftables => {
                self.add_vm_nftables_rules(vm_config, vm_ip, tenant_id)
                    .await
            }
        }
    }

    /// Remove firewall rules for a VM
    pub async fn remove_vm_rules(
        &mut self,
        vm_name: &str,
        vm_ip: &str,
        tenant_id: &str,
    ) -> BlixardResult<()> {
        info!(
            "Removing network isolation rules for VM {} (IP: {})",
            vm_name, vm_ip
        );

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
            self.run_iptables(&["-t", "filter", "-N", chain], true)
                .await?;
        }

        // Hook into main chains
        self.run_iptables(
            &["-t", "filter", "-I", "FORWARD", "-j", "BLIXARD_VM_FWD"],
            false,
        )
        .await?;
        self.run_iptables(
            &["-t", "filter", "-I", "INPUT", "-j", "BLIXARD_VM_IN"],
            false,
        )
        .await?;
        self.run_iptables(
            &["-t", "filter", "-I", "OUTPUT", "-j", "BLIXARD_VM_OUT"],
            false,
        )
        .await?;

        // Default policies - block VM access to cluster networks
        for network in &self.config.cluster_networks {
            // Block VMs from accessing cluster networks
            self.run_iptables(
                &[
                    "-t",
                    "filter",
                    "-A",
                    "BLIXARD_VM_FWD",
                    "-s",
                    &self.config.vm_network,
                    "-d",
                    network,
                    "-j",
                    "DROP",
                ],
                false,
            )
            .await?;
        }

        // Allow established connections
        self.run_iptables(
            &[
                "-t",
                "filter",
                "-A",
                "BLIXARD_VM_FWD",
                "-m",
                "state",
                "--state",
                "ESTABLISHED,RELATED",
                "-j",
                "ACCEPT",
            ],
            false,
        )
        .await?;

        Ok(())
    }

    /// Initialize nftables rules
    async fn init_nftables(&self) -> BlixardResult<()> {
        // Create nftables table and chains
        let nft_rules = format!(
            r#"
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
            self.config
                .cluster_networks
                .iter()
                .map(|net| format!(
                    "        ip saddr {} ip daddr {} drop",
                    self.config.vm_network, net
                ))
                .collect::<Vec<_>>()
                .join("\n")
        );

        self.run_nft(&["add", "table", "inet", "blixard"], true)
            .await?;
        self.run_nft_rules(&nft_rules).await?;

        Ok(())
    }

    /// Add iptables rules for a specific VM
    async fn add_vm_iptables_rules(
        &self,
        _vm_config: &VmConfig,
        vm_ip: &str,
        tenant_id: &str,
    ) -> BlixardResult<()> {
        // Block VM from accessing host
        self.run_iptables(
            &[
                "-t",
                "filter",
                "-A",
                "BLIXARD_VM_IN",
                "-s",
                vm_ip,
                "-j",
                "DROP",
            ],
            false,
        )
        .await?;

        // Inter-VM isolation
        if self.config.inter_vm_isolation {
            self.run_iptables(
                &[
                    "-t",
                    "filter",
                    "-A",
                    "BLIXARD_VM_FWD",
                    "-s",
                    vm_ip,
                    "-d",
                    &self.config.vm_network,
                    "-j",
                    "DROP",
                ],
                false,
            )
            .await?;
        }

        // Tenant isolation
        if self.config.tenant_isolation {
            // Allow communication within same tenant
            if let Some(tenant_ips) = self.tenant_networks.get(tenant_id) {
                for allowed_ip in tenant_ips {
                    if allowed_ip != vm_ip {
                        self.run_iptables(
                            &[
                                "-t",
                                "filter",
                                "-I",
                                "BLIXARD_VM_FWD",
                                "-s",
                                vm_ip,
                                "-d",
                                allowed_ip,
                                "-j",
                                "ACCEPT",
                            ],
                            false,
                        )
                        .await?;
                    }
                }
            }
        }

        // Allow specific ports
        for port in &self.config.allowed_ports {
            if port.direction == PortDirection::Outbound {
                self.run_iptables(
                    &[
                        "-t",
                        "filter",
                        "-A",
                        "BLIXARD_VM_FWD",
                        "-s",
                        vm_ip,
                        "-p",
                        &port.protocol,
                        "--dport",
                        &port.port.to_string(),
                        "-j",
                        "ACCEPT",
                    ],
                    false,
                )
                .await?;
            }
        }

        // Allow internet access if configured
        if self.config.allow_internet {
            self.run_iptables(
                &[
                    "-t",
                    "filter",
                    "-A",
                    "BLIXARD_VM_FWD",
                    "-s",
                    vm_ip,
                    "!",
                    "-d",
                    &self.config.vm_network,
                    "-j",
                    "ACCEPT",
                ],
                false,
            )
            .await?;
        }

        Ok(())
    }

    /// Add nftables rules for a specific VM
    async fn add_vm_nftables_rules(
        &self,
        vm_config: &VmConfig,
        vm_ip: &str,
        tenant_id: &str,
    ) -> BlixardResult<()> {
        let vm_chain = format!("vm_{}", vm_config.name.replace("-", "_"));

        // Create VM-specific chain
        self.run_nft(&["add", "chain", "inet", "blixard", &vm_chain], false)
            .await?;

        // Add jump rule to VM chain
        self.run_nft(
            &[
                "add",
                "rule",
                "inet",
                "blixard",
                "vm_forward",
                "ip",
                "saddr",
                vm_ip,
                "jump",
                &vm_chain,
            ],
            false,
        )
        .await?;

        // Block VM from accessing host
        self.run_nft(
            &[
                "add", "rule", "inet", "blixard", "vm_input", "ip", "saddr", vm_ip, "drop",
            ],
            false,
        )
        .await?;

        // Inter-VM isolation
        if self.config.inter_vm_isolation {
            self.run_nft(
                &[
                    "add",
                    "rule",
                    "inet",
                    "blixard",
                    &vm_chain,
                    "ip",
                    "daddr",
                    &self.config.vm_network,
                    "drop",
                ],
                false,
            )
            .await?;
        }

        // Tenant isolation
        if self.config.tenant_isolation {
            // Allow communication within same tenant
            if let Some(tenant_ips) = self.tenant_networks.get(tenant_id) {
                for allowed_ip in tenant_ips {
                    if allowed_ip != vm_ip {
                        self.run_nft(
                            &[
                                "add", "rule", "inet", "blixard", &vm_chain, "ip", "daddr",
                                allowed_ip, "accept",
                            ],
                            false,
                        )
                        .await?;
                    }
                }
            }

            // Block other tenant traffic
            self.run_nft(
                &[
                    "add",
                    "rule",
                    "inet",
                    "blixard",
                    &vm_chain,
                    "ip",
                    "daddr",
                    &self.config.vm_network,
                    "drop",
                ],
                false,
            )
            .await?;
        }

        // Allow specific ports
        for port in &self.config.allowed_ports {
            if port.direction == PortDirection::Outbound {
                self.run_nft(
                    &[
                        "add",
                        "rule",
                        "inet",
                        "blixard",
                        &vm_chain,
                        &port.protocol,
                        "dport",
                        &port.port.to_string(),
                        "accept",
                    ],
                    false,
                )
                .await?;
            }
        }

        // Allow internet access if configured
        if self.config.allow_internet {
            self.run_nft(
                &[
                    "add",
                    "rule",
                    "inet",
                    "blixard",
                    &vm_chain,
                    "ip",
                    "daddr",
                    "!=",
                    &self.config.vm_network,
                    "accept",
                ],
                false,
            )
            .await?;
        }

        Ok(())
    }

    /// Remove iptables rules for a VM
    async fn remove_vm_iptables_rules(&self, vm_name: &str, vm_ip: &str) -> BlixardResult<()> {
        // Remove rules with VM IP - this is simplified, in production we'd track rule numbers
        warn!(
            "Removing iptables rules for VM {} - manual cleanup may be required",
            vm_name
        );

        // Try to remove common rules
        self.run_iptables(
            &[
                "-t",
                "filter",
                "-D",
                "BLIXARD_VM_IN",
                "-s",
                vm_ip,
                "-j",
                "DROP",
            ],
            true,
        )
        .await?;

        Ok(())
    }

    /// Remove nftables rules for a VM
    async fn remove_vm_nftables_rules(&self, vm_name: &str, vm_ip: &str) -> BlixardResult<()> {
        let vm_chain = format!("vm_{}", vm_name.replace("-", "_"));

        // Remove jump rule
        self.run_nft(
            &[
                "delete",
                "rule",
                "inet",
                "blixard",
                "vm_forward",
                "ip",
                "saddr",
                vm_ip,
                "jump",
                &vm_chain,
            ],
            true,
        )
        .await?;

        // Delete VM chain
        self.run_nft(&["delete", "chain", "inet", "blixard", &vm_chain], true)
            .await?;

        // Remove input rule
        self.run_nft(
            &[
                "delete", "rule", "inet", "blixard", "vm_input", "ip", "saddr", vm_ip, "drop",
            ],
            true,
        )
        .await?;

        Ok(())
    }

    /// Run iptables command
    async fn run_iptables(&self, args: &[&str], ignore_errors: bool) -> BlixardResult<()> {
        debug!("Running iptables command: iptables {}", args.join(" "));

        let output = self
            .command_executor
            .execute(
                "iptables",
                args,
                CommandOptions::new().with_output_capture(),
            )
            .await
            .map_err(|e| BlixardError::Internal {
                message: format!("Failed to run iptables: {}", e),
            })?;

        if !output.success && !ignore_errors {
            let stderr = output
                .stderr_string()
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(BlixardError::Internal {
                message: format!("iptables command failed: {}", stderr),
            });
        }

        Ok(())
    }

    /// Run nft command
    async fn run_nft(&self, args: &[&str], ignore_errors: bool) -> BlixardResult<()> {
        debug!("Running nft command: nft {}", args.join(" "));

        let output = self
            .command_executor
            .execute("nft", args, CommandOptions::new().with_output_capture())
            .await
            .map_err(|e| BlixardError::Internal {
                message: format!("Failed to run nft: {}", e),
            })?;

        if !output.success && !ignore_errors {
            let stderr = output
                .stderr_string()
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(BlixardError::Internal {
                message: format!("nft command failed: {}", stderr),
            });
        }

        Ok(())
    }

    /// Run nft with rules from stdin
    async fn run_nft_rules(&self, rules: &str) -> BlixardResult<()> {
        debug!("Applying nft rules:\n{}", rules);

        let output = self
            .command_executor
            .execute(
                "nft",
                &["-f", "-"],
                CommandOptions::new()
                    .with_output_capture()
                    .with_stdin(rules.as_bytes()),
            )
            .await
            .map_err(|e| BlixardError::Internal {
                message: format!("Failed to run nft with rules: {}", e),
            })?;

        if !output.success {
            let stderr = output
                .stderr_string()
                .unwrap_or_else(|_| "Unknown error".to_string());
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
                let output = self
                    .command_executor
                    .execute(
                        "iptables",
                        &["-t", "filter", "-L", "-n", "-v"],
                        CommandOptions::new().with_output_capture(),
                    )
                    .await
                    .map_err(|e| BlixardError::Internal {
                        message: format!("Failed to list iptables rules: {}", e),
                    })?;

                output.stdout_string()
            }
            FirewallBackend::Nftables => {
                let output = self
                    .command_executor
                    .execute(
                        "nft",
                        &["list", "table", "inet", "blixard"],
                        CommandOptions::new().with_output_capture(),
                    )
                    .await
                    .map_err(|e| BlixardError::Internal {
                        message: format!("Failed to list nft rules: {}", e),
                    })?;

                output.stdout_string()
            }
        }
    }
}

/// Check if running with sufficient privileges for firewall management
pub async fn check_firewall_privileges(
    command_executor: &dyn CommandExecutor,
) -> BlixardResult<()> {
    // Check if we can run iptables/nft commands
    let output = command_executor
        .execute("id", &["-u"], CommandOptions::new().with_output_capture())
        .await
        .map_err(|e| BlixardError::Security {
            message: format!("Failed to check user ID: {}", e),
        })?;

    let uid = output.stdout_string()?.trim().to_string();

    if uid != "0" {
        // Check if we have CAP_NET_ADMIN capability
        let cap_result = command_executor
            .execute(
                "capsh",
                &["--print"],
                CommandOptions::new().with_output_capture(),
            )
            .await;

        if let Ok(cap_output) = cap_result {
            let caps = cap_output.stdout_string()?;
            if !caps.contains("cap_net_admin") {
                return Err(BlixardError::Security {
                    message:
                        "Firewall management requires root privileges or CAP_NET_ADMIN capability"
                            .to_string(),
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
    use crate::abstractions::command::{CommandOutput, MockCommandExecutor};
    use std::time::Duration;

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

    #[tokio::test]
    async fn test_run_iptables_success() {
        let config = NetworkIsolationConfig::default();
        let mock_executor = Arc::new(MockCommandExecutor::new());

        // Set up expectation for successful iptables command
        mock_executor.expect(
            "iptables",
            &["-t", "filter", "-N", "TEST_CHAIN"],
            Ok(CommandOutput {
                status: 0,
                stdout: vec![],
                stderr: vec![],
                success: true,
                duration: Duration::from_millis(10),
            }),
        );

        let manager = NetworkIsolationManager::with_executor(config, mock_executor.clone());
        let result = manager
            .run_iptables(&["-t", "filter", "-N", "TEST_CHAIN"], false)
            .await;

        assert!(result.is_ok());
        mock_executor.verify().unwrap();
    }

    #[tokio::test]
    async fn test_run_iptables_failure() {
        let config = NetworkIsolationConfig::default();
        let mock_executor = Arc::new(MockCommandExecutor::new());

        // Set up expectation for failed iptables command
        mock_executor.expect(
            "iptables",
            &["-t", "filter", "-N", "INVALID_CHAIN"],
            Ok(CommandOutput {
                status: 1,
                stdout: vec![],
                stderr: b"Chain already exists".to_vec(),
                success: false,
                duration: Duration::from_millis(10),
            }),
        );

        let manager = NetworkIsolationManager::with_executor(config, mock_executor.clone());
        let result = manager
            .run_iptables(&["-t", "filter", "-N", "INVALID_CHAIN"], false)
            .await;

        assert!(result.is_err());
        mock_executor.verify().unwrap();
    }

    #[tokio::test]
    async fn test_run_nft_success() {
        let config = NetworkIsolationConfig::default();
        let mock_executor = Arc::new(MockCommandExecutor::new());

        // Set up expectation for successful nft command
        mock_executor.expect(
            "nft",
            &["add", "table", "inet", "test"],
            Ok(CommandOutput {
                status: 0,
                stdout: vec![],
                stderr: vec![],
                success: true,
                duration: Duration::from_millis(10),
            }),
        );

        let manager = NetworkIsolationManager::with_executor(config, mock_executor.clone());
        let result = manager
            .run_nft(&["add", "table", "inet", "test"], false)
            .await;

        assert!(result.is_ok());
        mock_executor.verify().unwrap();
    }

    #[tokio::test]
    async fn test_run_nft_rules_with_stdin() {
        let config = NetworkIsolationConfig::default();
        let mock_executor = Arc::new(MockCommandExecutor::new());

        // Set up expectation for nft command with stdin
        mock_executor.expect(
            "nft",
            &["-f", "-"],
            Ok(CommandOutput {
                status: 0,
                stdout: vec![],
                stderr: vec![],
                success: true,
                duration: Duration::from_millis(20),
            }),
        );

        let manager = NetworkIsolationManager::with_executor(config, mock_executor.clone());
        let rules = "table inet test { chain input { type filter hook input priority 0; } }";
        let result = manager.run_nft_rules(rules).await;

        assert!(result.is_ok());
        mock_executor.verify().unwrap();
    }

    #[tokio::test]
    async fn test_get_rules_iptables() {
        let mut config = NetworkIsolationConfig::default();
        config.backend = FirewallBackend::Iptables;
        let mock_executor = Arc::new(MockCommandExecutor::new());

        // Set up expectation for iptables list command
        mock_executor.expect(
            "iptables", 
            &["-t", "filter", "-L", "-n", "-v"],
            Ok(CommandOutput {
                status: 0,
                stdout: b"Chain INPUT (policy ACCEPT 0 packets, 0 bytes)\nChain FORWARD (policy ACCEPT 0 packets, 0 bytes)\n".to_vec(),
                stderr: vec![],
                success: true,
                duration: Duration::from_millis(15),
            })
        );

        let manager = NetworkIsolationManager::with_executor(config, mock_executor.clone());
        let result = manager.get_rules().await;

        assert!(result.is_ok());
        let rules_output = result.unwrap();
        assert!(rules_output.contains("Chain INPUT"));
        mock_executor.verify().unwrap();
    }

    #[tokio::test]
    async fn test_get_rules_nftables() {
        let config = NetworkIsolationConfig::default(); // defaults to nftables
        let mock_executor = Arc::new(MockCommandExecutor::new());

        // Set up expectation for nft list command
        mock_executor.expect(
            "nft", 
            &["list", "table", "inet", "blixard"],
            Ok(CommandOutput {
                status: 0,
                stdout: b"table inet blixard {\n\tchain vm_forward {\n\t\ttype filter hook forward priority 0; policy accept;\n\t}\n}\n".to_vec(),
                stderr: vec![],
                success: true,
                duration: Duration::from_millis(12),
            })
        );

        let manager = NetworkIsolationManager::with_executor(config, mock_executor.clone());
        let result = manager.get_rules().await;

        assert!(result.is_ok());
        let rules_output = result.unwrap();
        assert!(rules_output.contains("table inet blixard"));
        mock_executor.verify().unwrap();
    }

    #[tokio::test]
    async fn test_check_firewall_privileges_root() {
        let mock_executor = MockCommandExecutor::new();

        // Set up expectation for id command returning root (uid 0)
        mock_executor.expect(
            "id",
            &["-u"],
            Ok(CommandOutput {
                status: 0,
                stdout: b"0\n".to_vec(),
                stderr: vec![],
                success: true,
                duration: Duration::from_millis(5),
            }),
        );

        let result = check_firewall_privileges(&mock_executor).await;

        assert!(result.is_ok());
        mock_executor.verify().unwrap();
    }

    #[tokio::test]
    async fn test_check_firewall_privileges_non_root_with_caps() {
        let mock_executor = MockCommandExecutor::new();

        // Set up expectations for non-root user with capabilities
        mock_executor.expect(
            "id",
            &["-u"],
            Ok(CommandOutput {
                status: 0,
                stdout: b"1000\n".to_vec(),
                stderr: vec![],
                success: true,
                duration: Duration::from_millis(5),
            }),
        );

        mock_executor.expect(
            "capsh",
            &["--print"],
            Ok(CommandOutput {
                status: 0,
                stdout: b"Current: = cap_net_admin+eip\n".to_vec(),
                stderr: vec![],
                success: true,
                duration: Duration::from_millis(8),
            }),
        );

        let result = check_firewall_privileges(&mock_executor).await;

        assert!(result.is_ok());
        mock_executor.verify().unwrap();
    }

    #[tokio::test]
    async fn test_check_firewall_privileges_insufficient() {
        let mock_executor = MockCommandExecutor::new();

        // Set up expectations for non-root user without capabilities
        mock_executor.expect(
            "id",
            &["-u"],
            Ok(CommandOutput {
                status: 0,
                stdout: b"1000\n".to_vec(),
                stderr: vec![],
                success: true,
                duration: Duration::from_millis(5),
            }),
        );

        mock_executor.expect(
            "capsh",
            &["--print"],
            Ok(CommandOutput {
                status: 0,
                stdout: b"Current: =\n".to_vec(), // No capabilities
                stderr: vec![],
                success: true,
                duration: Duration::from_millis(8),
            }),
        );

        let result = check_firewall_privileges(&mock_executor).await;

        assert!(result.is_err());
        if let Err(BlixardError::Security { message }) = result {
            assert!(message.contains("CAP_NET_ADMIN"));
        } else {
            panic!("Expected Security error");
        }
        mock_executor.verify().unwrap();
    }
}
