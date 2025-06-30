# Runbook: VM Health Check Failures

## Alert Details
- **Alert Name**: VMHealthCheckFailures
- **Severity**: Warning
- **Component**: VM Management

## Description
More than 20% of VM health checks are failing. This indicates widespread VM issues, health monitoring problems, or network connectivity issues.

## Impact
- Delayed detection of VM failures
- Ineffective auto-recovery
- Potential undetected VM outages
- SLA violations for VM availability

## Diagnostic Steps

### 1. Identify Failing Health Checks
```bash
# Get health check statistics
curl -s http://prometheus:9090/api/v1/query?query=rate(vm_health_check_failed[5m]) | jq

# List VMs with failing health checks
blixard vm list --health-status unhealthy

# Check health monitor logs
journalctl -u blixard | grep -E "health.*check|monitor" | tail -100
```

### 2. Categorize Failure Types
```bash
# Group failures by error type
blixard vm health-report --group-by error

# Common failure categories:
# - Connection timeout
# - Connection refused  
# - Authentication failed
# - Invalid response
# - Process not found
```

### 3. Check Network Connectivity
```bash
# Test connectivity to affected VMs
for vm in $(blixard vm list --health-status unhealthy --format json | jq -r '.vms[].ip'); do
  echo "Testing $vm:"
  ping -c 3 -W 1 $vm
  nc -zv $vm 22
done

# Check network interfaces on nodes
for node in $(blixard node list --format json | jq -r '.nodes[].address'); do
  echo "=== Network on $node ==="
  ssh $node "ip link show; bridge link show"
done
```

### 4. Verify Health Check Configuration
```bash
# Check global health check settings
blixard config get health_monitor

# Verify VM-specific health checks
blixard vm get <vm-name> --format json | jq '.health_check'

# Common issues:
# - Incorrect ports
# - Wrong protocols
# - Invalid endpoints
# - Insufficient timeouts
```

## Resolution Steps

### Scenario 1: Network Connectivity Issues
```bash
# 1. Check VM network interfaces
for vm in $(blixard vm list --health-status unhealthy --format json | jq -r '.vms[].name'); do
  blixard vm console $vm --command "ip addr show; ip route show"
done

# 2. Restart VM networking
blixard vm exec <vm-name> --command "systemctl restart NetworkManager"

# 3. Check host bridge/tap interfaces
ssh <node> "brctl show; ip link show type tap"

# 4. Restart host networking if needed
ssh <node> "sudo systemctl restart blixard-networking"
```

### Scenario 2: Health Check Misconfiguration
```bash
# 1. Update health check settings for affected VMs
blixard vm update <vm-name> \
  --health-check-type http \
  --health-check-port 80 \
  --health-check-path /health \
  --health-check-timeout 5s

# 2. Bulk update for similar VMs
for vm in $(blixard vm list --tag web --format json | jq -r '.vms[].name'); do
  blixard vm update $vm --health-check-timeout 10s
done

# 3. Disable health checks temporarily
blixard vm update <vm-name> --health-check-enabled false
```

### Scenario 3: VM Agent Issues
```bash
# 1. Check if health agent is running in VMs
for vm in $(blixard vm list --health-status unhealthy --format json | jq -r '.vms[].name'); do
  echo "=== Agent status in $vm ==="
  blixard vm exec $vm --command "systemctl status blixard-agent"
done

# 2. Restart agents
blixard vm exec-batch --health-status unhealthy \
  --command "systemctl restart blixard-agent"

# 3. Reinstall agent if corrupted
blixard vm exec <vm-name> --script /usr/share/blixard/install-agent.sh
```

### Scenario 4: Resource Exhaustion
```bash
# 1. Check VM resources
for vm in $(blixard vm list --health-status unhealthy --format json | jq -r '.vms[].name'); do
  echo "=== Resources in $vm ==="
  blixard vm stats $vm --format json | jq '{cpu_usage, memory_usage, disk_usage}'
done

# 2. Free up resources
# Kill memory hogs
blixard vm exec <vm-name> --command "ps aux | sort -nrk 4 | head -10"

# Clean disk space
blixard vm exec <vm-name> --command "df -h; du -sh /var/log/*"

# 3. Resize VMs if needed
blixard vm resize <vm-name> --memory +2GB
```

### Scenario 5: Firewall/Security Group Issues
```bash
# 1. Check firewall rules on VMs
blixard vm exec-batch --health-status unhealthy \
  --command "iptables -L -n -v | grep -E 'DROP|REJECT'"

# 2. Verify security groups
blixard security-group list --vm <vm-name>

# 3. Add health check exceptions
blixard security-group add-rule default \
  --source-cidr 10.0.0.0/8 \
  --port 8080 \
  --protocol tcp \
  --comment "Health checks"
```

## Health Monitor Recovery

### Reset Health Monitor
```bash
# 1. Clear health check state
blixard health-monitor reset --clear-history

# 2. Restart health monitor component
systemctl restart blixard-health-monitor

# 3. Force immediate health check
blixard health-monitor check-all --force
```

### Adjust Health Check Parameters
```bash
# 1. Increase timeouts globally
blixard config set health_monitor.timeout 30s
blixard config set health_monitor.interval 60s

# 2. Reduce parallel checks to avoid overload
blixard config set health_monitor.max_concurrent_checks 10

# 3. Configure retries
blixard config set health_monitor.retries 3
blixard config set health_monitor.retry_delay 5s
```

## Recovery Verification

1. **Health Check Success Rate**
   ```bash
   # Should be > 95%
   watch -n10 'curl -s http://localhost:9090/api/v1/query?query=1-rate(vm_health_check_failed[5m])/rate(vm_health_checks_total[5m]) | jq .data.result[0].value[1]'
   ```

2. **All VMs Monitored**
   ```bash
   # Compare VM count with monitored count
   TOTAL_VMS=$(blixard vm list --format json | jq '.vms | length')
   MONITORED=$(blixard health-monitor status --format json | jq '.monitored_vms')
   echo "Coverage: $MONITORED/$TOTAL_VMS"
   ```

3. **Recent Health History**
   ```bash
   blixard health-monitor history --period 1h --format table
   ```

## Prevention

1. **Health Check Best Practices**
   ```yaml
   # Default health check template
   health_check:
     type: http
     port: 8080
     path: /health
     timeout: 10s
     interval: 30s
     success_threshold: 2
     failure_threshold: 3
   ```

2. **Monitoring Configuration**
   ```toml
   # /etc/blixard/config.toml
   [health_monitor]
   enabled = true
   interval = "30s"
   timeout = "10s"
   max_concurrent_checks = 50
   
   # Circuit breaker settings
   failure_threshold = 5
   recovery_timeout = "5m"
   ```

3. **VM Agent Configuration**
   ```bash
   # Ensure agent auto-starts
   cat > /etc/systemd/system/blixard-agent.service <<EOF
   [Unit]
   Description=Blixard VM Agent
   After=network-online.target
   Wants=network-online.target
   
   [Service]
   Type=simple
   Restart=always
   RestartSec=10
   ExecStart=/usr/bin/blixard-agent
   
   [Install]
   WantedBy=multi-user.target
   EOF
   ```

4. **Network Reliability**
   - Use dedicated health check network
   - Implement redundant paths
   - Monitor network latency

## Troubleshooting Tips

1. **Enable Debug Logging**
   ```bash
   blixard config set health_monitor.log_level debug
   systemctl restart blixard-health-monitor
   tail -f /var/log/blixard/health-monitor.log
   ```

2. **Manual Health Check**
   ```bash
   # Test specific VM
   blixard health-monitor check --vm <vm-name> --verbose
   ```

3. **Health Check Simulation**
   ```bash
   # Simulate health check from node
   curl -v http://<vm-ip>:8080/health
   ```

## Escalation

If health check failures persist above 30% for 30 minutes:
1. Disable auto-recovery to prevent flapping
2. Page VM infrastructure team
3. Consider health monitor component failure

## Related Runbooks
- [VM Recovery Failures](./vm-recovery-failures.md)
- [High Network Latency](./high-network-latency.md)
- [VM Agent Issues](./vm-agent-issues.md)