# Runbook: Blixard Node Down

## Alert Details
- **Alert Name**: ClusterNodeDown
- **Severity**: Critical
- **Component**: Cluster

## Description
A Blixard node has been unreachable for more than 2 minutes. This indicates the node process has crashed, the host is down, or there's a network connectivity issue.

## Impact
- Reduced cluster capacity for VM scheduling
- Potential loss of VMs running on the affected node
- Decreased fault tolerance if multiple nodes are affected
- Possible Raft consensus issues if this reduces cluster below quorum

## Diagnostic Steps

### 1. Identify Affected Node
```bash
# Check Prometheus targets
curl -s http://prometheus:9090/api/v1/targets | jq '.data.activeTargets[] | select(.health=="down")'

# Check cluster status
blixard cluster status
```

### 2. Verify Node Connectivity
```bash
# Ping the node
ping -c 3 <node-ip>

# Check SSH access
ssh <node-ip> "echo 'Node reachable via SSH'"

# Check gRPC port
nc -zv <node-ip> 7001
```

### 3. Check Node Process
If the node is reachable:
```bash
# SSH to the node
ssh <node-ip>

# Check if Blixard process is running
systemctl status blixard
ps aux | grep blixard

# Check recent logs
journalctl -u blixard -n 100 --no-pager
```

### 4. Check System Resources
```bash
# On the affected node
df -h          # Disk space
free -m        # Memory
top -bn1       # CPU and process list
dmesg | tail   # Kernel messages
```

### 5. Check Network Issues
```bash
# From another cluster node
traceroute <down-node-ip>
mtr -r -c 10 <down-node-ip>

# Check iptables rules
sudo iptables -L -n -v
```

## Resolution Steps

### Scenario 1: Process Crashed
```bash
# Restart the service
sudo systemctl restart blixard

# Monitor startup
sudo journalctl -u blixard -f
```

### Scenario 2: Out of Resources
```bash
# Free disk space
sudo du -sh /var/log/* | sort -h
sudo journalctl --vacuum-time=2d

# Free memory (if safe)
sudo sync && echo 3 > /proc/sys/vm/drop_caches

# Then restart Blixard
sudo systemctl restart blixard
```

### Scenario 3: Network Issues
```bash
# Restart network service
sudo systemctl restart NetworkManager
# or
sudo systemctl restart systemd-networkd

# Verify network configuration
ip addr show
ip route show
```

### Scenario 4: Host Down
1. Access host via out-of-band management (IPMI/iDRAC)
2. Check power status and attempt power cycle
3. If virtual machine, check hypervisor console
4. Contact infrastructure team if physical access needed

### Scenario 5: Persistent Failure
If the node cannot be recovered:
```bash
# Remove node from cluster (from a healthy node)
blixard cluster remove-node <node-id>

# Migrate VMs if possible
blixard vm migrate --from-node <node-id> --strategy redistribute
```

## Recovery Verification

1. **Verify Node is Back Online**
   ```bash
   blixard cluster status | grep <node-id>
   ```

2. **Check Metrics Recovery**
   ```bash
   curl -s http://<node-ip>:9090/metrics | grep up
   ```

3. **Verify Raft Participation**
   ```bash
   blixard raft status
   ```

4. **Check VM Health**
   ```bash
   blixard vm list --node <node-id>
   ```

## Prevention

1. **Monitor System Resources**
   - Set up alerts for disk space < 10%
   - Alert on memory usage > 90%
   - Monitor systemd service failures

2. **Implement Auto-Recovery**
   ```ini
   # /etc/systemd/system/blixard.service
   [Service]
   Restart=always
   RestartSec=10
   StartLimitBurst=5
   StartLimitIntervalSec=300
   ```

3. **Regular Health Checks**
   - Implement external monitoring (Nagios/Zabbix)
   - Use multiple monitoring paths
   - Set up automated recovery scripts

4. **Capacity Planning**
   - Ensure adequate resources with 20% headroom
   - Plan for N+1 redundancy
   - Regular load testing

## Escalation

If issue persists after 30 minutes:
1. Page on-call SRE team
2. Engage platform engineering team
3. Consider declaring partial outage if multiple nodes affected

## Related Runbooks
- [Cluster Unhealthy Nodes](./unhealthy-nodes.md)
- [Raft No Leader](./no-raft-leader.md)
- [VM Recovery Failures](./vm-recovery-failures.md)