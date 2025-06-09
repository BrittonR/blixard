# MicroVM Monitoring in Blixard

This document outlines how Blixard monitors and manages microVMs created by microvm.nix through systemd integration.

## Overview

Blixard leverages systemd's service management capabilities to monitor microVMs created by microvm.nix. Each microVM runs as a systemd service, allowing Blixard to track state, resource usage, and health through systemd's D-Bus API and journal integration.

## 1. Tracking systemd Services Created by microvm.nix

### Service Naming Convention

MicroVM.nix creates systemd services with a predictable naming pattern:
- System services: `microvm@<vm-name>.service`
- User services: `microvm-<vm-name>.service`

### Service Discovery

```rust
use systemd::daemon;
use dbus::blocking::Connection;
use std::time::Duration;

/// Discover all microVM services managed by systemd
pub fn discover_microvm_services() -> Result<Vec<String>, Error> {
    let conn = Connection::new_system()?;
    let proxy = conn.with_proxy(
        "org.freedesktop.systemd1",
        "/org/freedesktop/systemd1",
        Duration::from_millis(5000),
    );
    
    // List all units matching microvm pattern
    let units: Vec<(String, String, String, String, String, String, 
                    dbus::Path, u32, String, dbus::Path)> = proxy.method_call(
        "org.freedesktop.systemd1.Manager",
        "ListUnits",
        (),
    )?;
    
    let microvm_services: Vec<String> = units
        .into_iter()
        .filter(|(name, _, _, _, _, _, _, _, _, _)| {
            name.starts_with("microvm@") || name.starts_with("microvm-")
        })
        .map(|(name, _, _, _, _, _, _, _, _, _)| name)
        .collect();
    
    Ok(microvm_services)
}
```

### Service Registration in Blixard

```rust
use khepri::KhepriStore;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct MicroVMService {
    pub vm_name: String,
    pub systemd_unit: String,
    pub created_at: SystemTime,
    pub node_name: String,
    pub user_service: bool,
}

impl MicroVMService {
    /// Register a new microVM service in Blixard's distributed store
    pub async fn register(
        store: &KhepriStore,
        vm_name: String,
        systemd_unit: String,
        node_name: String,
        user_service: bool,
    ) -> Result<(), Error> {
        let service = MicroVMService {
            vm_name: vm_name.clone(),
            systemd_unit,
            created_at: SystemTime::now(),
            node_name,
            user_service,
        };
        
        let path = format!("/microvms/{}", vm_name);
        store.put(&path, &service).await?;
        Ok(())
    }
}
```

## 2. Getting VM State, Resource Usage, and Health

### VM State Monitoring

```rust
use systemd::daemon::{State, SubState};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum VMState {
    Starting,
    Running,
    Stopping,
    Stopped,
    Failed,
    Unknown,
}

pub struct VMMonitor {
    conn: Connection,
}

impl VMMonitor {
    pub fn new() -> Result<Self, Error> {
        Ok(VMMonitor {
            conn: Connection::new_system()?,
        })
    }
    
    /// Get current state of a microVM service
    pub fn get_vm_state(&self, unit_name: &str) -> Result<VMState, Error> {
        let proxy = self.conn.with_proxy(
            "org.freedesktop.systemd1",
            format!("/org/freedesktop/systemd1/unit/{}", 
                    unit_name.replace(".", "_2e")),
            Duration::from_millis(5000),
        );
        
        let active_state: String = proxy.get("org.freedesktop.systemd1.Unit", "ActiveState")?;
        let sub_state: String = proxy.get("org.freedesktop.systemd1.Unit", "SubState")?;
        
        let vm_state = match (active_state.as_str(), sub_state.as_str()) {
            ("activating", _) => VMState::Starting,
            ("active", "running") => VMState::Running,
            ("deactivating", _) => VMState::Stopping,
            ("inactive", _) => VMState::Stopped,
            ("failed", _) => VMState::Failed,
            _ => VMState::Unknown,
        };
        
        Ok(vm_state)
    }
}
```

### Resource Usage Monitoring

```rust
use procfs::process::Process;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct VMResourceUsage {
    pub cpu_usage_percent: f64,
    pub memory_usage_mb: u64,
    pub disk_read_bytes: u64,
    pub disk_write_bytes: u64,
    pub network_rx_bytes: u64,
    pub network_tx_bytes: u64,
    pub timestamp: SystemTime,
}

impl VMMonitor {
    /// Get resource usage for a microVM via systemd cgroup
    pub fn get_vm_resources(&self, unit_name: &str) -> Result<VMResourceUsage, Error> {
        let proxy = self.conn.with_proxy(
            "org.freedesktop.systemd1",
            format!("/org/freedesktop/systemd1/unit/{}", 
                    unit_name.replace(".", "_2e")),
            Duration::from_millis(5000),
        );
        
        // Get main PID of the service
        let main_pid: u32 = proxy.get("org.freedesktop.systemd1.Service", "MainPID")?;
        
        // Get cgroup path
        let cgroup: String = proxy.get("org.freedesktop.systemd1.Unit", "ControlGroup")?;
        
        // Read cgroup statistics
        let cpu_usage = read_cgroup_cpu_usage(&cgroup)?;
        let memory_usage = read_cgroup_memory_usage(&cgroup)?;
        let io_stats = read_cgroup_io_stats(&cgroup)?;
        
        // Get network stats from the VM's network namespace
        let net_stats = get_vm_network_stats(main_pid)?;
        
        Ok(VMResourceUsage {
            cpu_usage_percent: cpu_usage,
            memory_usage_mb: memory_usage / (1024 * 1024),
            disk_read_bytes: io_stats.read_bytes,
            disk_write_bytes: io_stats.write_bytes,
            network_rx_bytes: net_stats.rx_bytes,
            network_tx_bytes: net_stats.tx_bytes,
            timestamp: SystemTime::now(),
        })
    }
}

/// Read CPU usage from cgroup v2
fn read_cgroup_cpu_usage(cgroup_path: &str) -> Result<f64, Error> {
    let stat_path = format!("/sys/fs/cgroup{}/cpu.stat", cgroup_path);
    let content = std::fs::read_to_string(stat_path)?;
    
    // Parse usage_usec from cpu.stat
    for line in content.lines() {
        if let Some(usage) = line.strip_prefix("usage_usec ") {
            let usec: u64 = usage.parse()?;
            // Convert to percentage (simplified calculation)
            return Ok((usec as f64) / 1_000_000.0);
        }
    }
    
    Err(Error::ParseError("cpu usage not found"))
}
```

### Health Monitoring

```rust
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct VMHealthCheck {
    pub vm_name: String,
    pub is_healthy: bool,
    pub checks: Vec<HealthCheckResult>,
    pub timestamp: SystemTime,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct HealthCheckResult {
    pub check_type: String,
    pub passed: bool,
    pub message: String,
}

impl VMMonitor {
    /// Perform comprehensive health check on a microVM
    pub async fn check_vm_health(&self, vm_name: &str) -> Result<VMHealthCheck, Error> {
        let unit_name = format!("microvm@{}.service", vm_name);
        let mut checks = Vec::new();
        
        // Check 1: Service state
        let state = self.get_vm_state(&unit_name)?;
        checks.push(HealthCheckResult {
            check_type: "service_state".to_string(),
            passed: matches!(state, VMState::Running),
            message: format!("Service state: {:?}", state),
        });
        
        // Check 2: Recent failures in journal
        let failures = self.check_recent_failures(&unit_name)?;
        checks.push(HealthCheckResult {
            check_type: "journal_errors".to_string(),
            passed: failures == 0,
            message: format!("{} errors in last hour", failures),
        });
        
        // Check 3: Resource usage within limits
        let resources = self.get_vm_resources(&unit_name)?;
        let cpu_ok = resources.cpu_usage_percent < 90.0;
        let mem_ok = resources.memory_usage_mb < get_vm_memory_limit(&vm_name)? * 0.9;
        
        checks.push(HealthCheckResult {
            check_type: "resource_usage".to_string(),
            passed: cpu_ok && mem_ok,
            message: format!("CPU: {:.1}%, Memory: {}MB", 
                           resources.cpu_usage_percent, 
                           resources.memory_usage_mb),
        });
        
        // Check 4: Network connectivity (if applicable)
        if let Ok(network_ok) = self.check_vm_network(&vm_name).await {
            checks.push(HealthCheckResult {
                check_type: "network".to_string(),
                passed: network_ok,
                message: if network_ok { "Network OK".to_string() } 
                        else { "Network unreachable".to_string() },
            });
        }
        
        let is_healthy = checks.iter().all(|c| c.passed);
        
        Ok(VMHealthCheck {
            vm_name: vm_name.to_string(),
            is_healthy,
            checks,
            timestamp: SystemTime::now(),
        })
    }
    
    /// Check journal for recent errors
    fn check_recent_failures(&self, unit_name: &str) -> Result<u32, Error> {
        use systemd::journal;
        
        let mut reader = journal::OpenOptions::default()
            .system(true)
            .open()?;
        
        // Filter by unit and time
        reader.match_add("_SYSTEMD_UNIT", unit_name)?;
        reader.seek_tail()?;
        reader.previous_skip(100)?; // Check last 100 entries
        
        let one_hour_ago = SystemTime::now() - Duration::from_secs(3600);
        let mut error_count = 0;
        
        while let Some(entry) = reader.next_entry()? {
            if let Ok(timestamp) = entry.timestamp() {
                if timestamp < one_hour_ago {
                    break;
                }
            }
            
            if let Some(priority) = entry.get("PRIORITY") {
                if priority.parse::<u8>().unwrap_or(6) <= 3 { // Error or worse
                    error_count += 1;
                }
            }
        }
        
        Ok(error_count)
    }
}
```

## 3. Correlating systemd Services with Blixard's VM Records

### Service Correlation

```rust
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct VMCorrelation {
    pub vm_name: String,
    pub systemd_unit: String,
    pub khepri_path: String,
    pub node_name: String,
    pub last_seen: SystemTime,
}

pub struct VMCorrelator {
    store: KhepriStore,
    monitor: VMMonitor,
}

impl VMCorrelator {
    /// Correlate discovered systemd services with Blixard VM records
    pub async fn correlate_services(&self) -> Result<Vec<VMCorrelation>, Error> {
        let mut correlations = Vec::new();
        
        // Get all microVM services from systemd
        let systemd_services = discover_microvm_services()?;
        
        // Get all VM records from Khepri
        let vm_records: Vec<(String, MicroVMService)> = self.store
            .list_prefix("/microvms/")
            .await?;
        
        // Build correlation map
        let mut vm_map: HashMap<String, MicroVMService> = vm_records
            .into_iter()
            .collect();
        
        for service in systemd_services {
            let vm_name = extract_vm_name(&service)?;
            
            if let Some(vm_record) = vm_map.get(&vm_name) {
                // Known VM - update last seen
                correlations.push(VMCorrelation {
                    vm_name: vm_name.clone(),
                    systemd_unit: service.clone(),
                    khepri_path: format!("/microvms/{}", vm_name),
                    node_name: vm_record.node_name.clone(),
                    last_seen: SystemTime::now(),
                });
                
                // Update last seen in Khepri
                self.update_last_seen(&vm_name).await?;
            } else {
                // Unknown VM - register it
                warn!("Found unregistered microVM service: {}", service);
                self.register_discovered_vm(&service).await?;
            }
        }
        
        // Check for stale records (VMs in Khepri but not in systemd)
        for (vm_name, record) in vm_map {
            if !correlations.iter().any(|c| c.vm_name == vm_name) {
                warn!("VM {} exists in Khepri but not in systemd", vm_name);
                // Mark as potentially stale
                self.mark_vm_stale(&vm_name).await?;
            }
        }
        
        Ok(correlations)
    }
    
    /// Extract VM name from systemd unit name
    fn extract_vm_name(unit_name: &str) -> Result<String, Error> {
        if let Some(name) = unit_name.strip_prefix("microvm@") {
            Ok(name.strip_suffix(".service").unwrap_or(name).to_string())
        } else if let Some(name) = unit_name.strip_prefix("microvm-") {
            Ok(name.strip_suffix(".service").unwrap_or(name).to_string())
        } else {
            Err(Error::ParseError("Invalid microVM service name"))
        }
    }
}
```

## 4. Code Examples for systemd Monitoring in Rust

### Complete Monitoring Service

```rust
use tokio::time::{interval, Duration};
use systemd::daemon;

pub struct MicroVMMonitoringService {
    store: KhepriStore,
    monitor: VMMonitor,
    correlator: VMCorrelator,
    update_interval: Duration,
}

impl MicroVMMonitoringService {
    pub fn new(store: KhepriStore) -> Result<Self, Error> {
        let monitor = VMMonitor::new()?;
        let correlator = VMCorrelator {
            store: store.clone(),
            monitor: monitor.clone(),
        };
        
        Ok(Self {
            store,
            monitor,
            correlator,
            update_interval: Duration::from_secs(30),
        })
    }
    
    /// Start the monitoring loop
    pub async fn run(&self) -> Result<(), Error> {
        // Notify systemd we're ready
        daemon::notify(false, &[daemon::NotifyState::Ready])?;
        
        let mut interval = interval(self.update_interval);
        
        loop {
            interval.tick().await;
            
            // Correlate services
            let correlations = self.correlator.correlate_services().await?;
            
            // Update state for each VM
            for correlation in correlations {
                if let Err(e) = self.update_vm_state(&correlation).await {
                    error!("Failed to update VM {}: {}", correlation.vm_name, e);
                }
            }
            
            // Send watchdog ping to systemd
            daemon::notify(false, &[daemon::NotifyState::Watchdog])?;
        }
    }
    
    /// Update VM state in distributed store
    async fn update_vm_state(&self, correlation: &VMCorrelation) -> Result<(), Error> {
        // Get current state
        let state = self.monitor.get_vm_state(&correlation.systemd_unit)?;
        
        // Get resource usage
        let resources = self.monitor.get_vm_resources(&correlation.systemd_unit)?;
        
        // Perform health check
        let health = self.monitor.check_vm_health(&correlation.vm_name).await?;
        
        // Create state update
        let update = VMStateUpdate {
            vm_name: correlation.vm_name.clone(),
            state,
            resources,
            health,
            node_name: correlation.node_name.clone(),
            timestamp: SystemTime::now(),
        };
        
        // Store in Khepri (will be replicated via Raft)
        let path = format!("/microvms/{}/state", correlation.vm_name);
        self.store.put(&path, &update).await?;
        
        // Emit metrics
        self.emit_metrics(&correlation.vm_name, &update)?;
        
        Ok(())
    }
}
```

### systemd Unit File for Monitoring Service

```ini
[Unit]
Description=Blixard MicroVM Monitoring Service
After=network.target khepri.service

[Service]
Type=notify
ExecStart=/usr/local/bin/blixard-monitor
Restart=always
RestartSec=10
WatchdogSec=60

# Security hardening
NoNewPrivileges=true
PrivateTmp=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=/var/lib/blixard

[Install]
WantedBy=multi-user.target
```

## 5. Integration with Raft Consensus for State Updates

### Raft State Machine Integration

```rust
use raft::{StateChange, StateMachine};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum VMCommand {
    UpdateState {
        vm_name: String,
        state: VMStateUpdate,
    },
    RegisterVM {
        vm_name: String,
        config: MicroVMService,
    },
    RemoveVM {
        vm_name: String,
    },
    UpdateHealth {
        vm_name: String,
        health: VMHealthCheck,
    },
}

pub struct VMStateMachine {
    vms: HashMap<String, VMRecord>,
}

impl StateMachine for VMStateMachine {
    type Command = VMCommand;
    type Response = Result<(), String>;
    
    fn apply(&mut self, command: Self::Command) -> Self::Response {
        match command {
            VMCommand::UpdateState { vm_name, state } => {
                if let Some(vm) = self.vms.get_mut(&vm_name) {
                    vm.last_state = state.state;
                    vm.last_resources = Some(state.resources);
                    vm.last_health = Some(state.health);
                    vm.last_update = state.timestamp;
                    Ok(())
                } else {
                    Err(format!("VM {} not found", vm_name))
                }
            }
            VMCommand::RegisterVM { vm_name, config } => {
                self.vms.insert(vm_name.clone(), VMRecord {
                    config,
                    last_state: VMState::Unknown,
                    last_resources: None,
                    last_health: None,
                    last_update: SystemTime::now(),
                });
                Ok(())
            }
            VMCommand::RemoveVM { vm_name } => {
                self.vms.remove(&vm_name);
                Ok(())
            }
            VMCommand::UpdateHealth { vm_name, health } => {
                if let Some(vm) = self.vms.get_mut(&vm_name) {
                    vm.last_health = Some(health);
                    vm.last_update = SystemTime::now();
                    Ok(())
                } else {
                    Err(format!("VM {} not found", vm_name))
                }
            }
        }
    }
}
```

### Distributed State Updates

```rust
use khepri::consensus::{RaftNode, Proposal};

pub struct DistributedVMMonitor {
    raft_node: RaftNode,
    local_monitor: VMMonitor,
}

impl DistributedVMMonitor {
    /// Submit VM state update through Raft consensus
    pub async fn update_vm_state_consensus(
        &self,
        vm_name: &str,
        state: VMStateUpdate,
    ) -> Result<(), Error> {
        let command = VMCommand::UpdateState {
            vm_name: vm_name.to_string(),
            state,
        };
        
        // Serialize command
        let payload = bincode::serialize(&command)?;
        
        // Submit to Raft
        let proposal = Proposal {
            data: payload,
            client_id: self.get_node_id(),
            sequence: self.next_sequence(),
        };
        
        match self.raft_node.propose(proposal).await {
            Ok(_) => {
                debug!("VM state update accepted by Raft");
                Ok(())
            }
            Err(e) => {
                error!("Failed to submit VM state update: {}", e);
                Err(Error::ConsensusError(e.to_string()))
            }
        }
    }
    
    /// Handle state changes from Raft
    pub async fn handle_state_change(&self, change: StateChange) -> Result<(), Error> {
        match change {
            StateChange::Applied { index, command } => {
                let cmd: VMCommand = bincode::deserialize(&command)?;
                info!("Applied VM command at index {}: {:?}", index, cmd);
                
                // Update local caches if needed
                self.update_local_cache(&cmd)?;
            }
            StateChange::LeaderChange { new_leader } => {
                info!("Raft leader changed to: {:?}", new_leader);
                // Potentially pause monitoring if we're not the leader
                if !self.is_leader() {
                    self.pause_active_monitoring().await?;
                }
            }
            StateChange::MembershipChange { added, removed } => {
                info!("Cluster membership changed: +{:?} -{:?}", added, removed);
                // Rebalance VM monitoring responsibilities
                self.rebalance_monitoring().await?;
            }
        }
        Ok(())
    }
}
```

### Monitoring Coordination

```rust
/// Coordinate monitoring across cluster nodes
pub struct ClusterMonitorCoordinator {
    node_id: String,
    raft: RaftNode,
    assigned_vms: RwLock<HashSet<String>>,
}

impl ClusterMonitorCoordinator {
    /// Determine which node monitors which VMs
    pub async fn coordinate_monitoring(&self) -> Result<(), Error> {
        // Only leader coordinates
        if !self.raft.is_leader() {
            return Ok(());
        }
        
        // Get all VMs and cluster nodes
        let all_vms = self.get_all_vms().await?;
        let active_nodes = self.raft.get_members();
        
        // Simple round-robin assignment
        let mut assignments: HashMap<String, Vec<String>> = HashMap::new();
        for node in &active_nodes {
            assignments.insert(node.clone(), Vec::new());
        }
        
        for (i, vm) in all_vms.iter().enumerate() {
            let node_idx = i % active_nodes.len();
            let node = &active_nodes[node_idx];
            assignments.get_mut(node).unwrap().push(vm.clone());
        }
        
        // Submit assignments through Raft
        let command = VMCommand::UpdateMonitoringAssignments { assignments };
        self.submit_command(command).await?;
        
        Ok(())
    }
    
    /// Check if this node should monitor a specific VM
    pub async fn should_monitor(&self, vm_name: &str) -> bool {
        self.assigned_vms.read().await.contains(vm_name)
    }
}
```

## Best Practices

### 1. Efficient Monitoring
- Use systemd's event-based notifications instead of polling when possible
- Batch updates to reduce Raft proposal overhead
- Cache frequently accessed data locally

### 2. Fault Tolerance
- Handle systemd D-Bus disconnections gracefully
- Implement exponential backoff for failed operations
- Maintain local state cache for read operations during network partitions

### 3. Security
- Use systemd's security features (PrivateNetwork, SystemCallFilter)
- Validate all VM names and paths to prevent injection
- Implement rate limiting for state updates

### 4. Performance
- Use async/await for non-blocking operations
- Implement connection pooling for D-Bus
- Compress large state updates before Raft replication

## Troubleshooting

### Common Issues

1. **"Failed to connect to system bus"**
   - Ensure the monitoring service has proper D-Bus permissions
   - Check if D-Bus system daemon is running

2. **"VM state not updating in cluster"**
   - Verify Raft cluster is healthy
   - Check network connectivity between nodes
   - Ensure monitoring service is running on leader node

3. **"High CPU usage in monitoring"**
   - Increase monitoring interval
   - Check for journal spam from failing VMs
   - Verify cgroup v2 is properly configured

### Debug Commands

```bash
# Check monitoring service status
systemctl status blixard-monitor

# View monitoring logs
journalctl -u blixard-monitor -f

# List all microVM services
systemctl list-units 'microvm*'

# Check D-Bus permissions
busctl tree org.freedesktop.systemd1

# Verify Raft cluster state
blixard cluster status
```

## Future Enhancements

1. **Predictive Health Monitoring**
   - Machine learning for anomaly detection
   - Predictive failure analysis

2. **Advanced Resource Management**
   - Dynamic resource allocation based on usage
   - Cross-VM resource balancing

3. **Integration with Prometheus/Grafana**
   - Export metrics in Prometheus format
   - Pre-built Grafana dashboards

4. **VM Migration Support**
   - Live migration coordination through Raft
   - State transfer optimization