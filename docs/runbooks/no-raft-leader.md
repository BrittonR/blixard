# Runbook: No Raft Leader

## Alert Details
- **Alert Name**: RaftNoLeader
- **Severity**: Critical
- **Component**: Raft Consensus

## Description
The cluster has no elected Raft leader for more than 1 minute. This prevents any state changes including VM operations, configuration updates, and new node joins.

## Impact
- **Complete cluster write unavailability**
- No new VMs can be created or modified
- No configuration changes possible
- Existing VMs continue running but unmanaged
- No automatic recovery of failed VMs

## Diagnostic Steps

### 1. Check Raft Status Across All Nodes
```bash
# From any reachable node
for node in node1 node2 node3; do
  echo "=== Raft status on $node ==="
  ssh $node "blixard raft status"
done

# Via gRPC if CLI unavailable
grpcurl -plaintext <node>:7001 blixard.ClusterService/GetRaftStatus
```

### 2. Identify Split Brain Scenario
```bash
# Check for multiple leaders (split brain)
blixard cluster status | grep -E "leader|follower|candidate"

# Check network partitions
for node in node1 node2 node3; do
  echo "=== Connectivity from $node ==="
  ssh $node "for peer in node1 node2 node3; do nc -zv \$peer 7001; done"
done
```

### 3. Check Raft Logs
```bash
# On each node
journalctl -u blixard | grep -E "raft|election|vote|term" | tail -50

# Look for:
# - Election timeouts
# - Vote rejections  
# - Term mismatches
# - Message failures
```

### 4. Verify Cluster Configuration
```bash
# Check each node's view of cluster membership
for node in node1 node2 node3; do
  ssh $node "cat /var/lib/blixard/cluster.json"
done

# Ensure all nodes have consistent peer lists
```

## Resolution Steps

### Scenario 1: Network Partition
```bash
# 1. Identify and fix network issues
# Check firewall rules
sudo iptables -L -n | grep 7001

# Test connectivity between all pairs
for src in node1 node2 node3; do
  for dst in node1 node2 node3; do
    [ "$src" != "$dst" ] && ssh $src "nc -zv $dst 7001"
  done
done

# 2. If network is fixed, elections should resume automatically
# Monitor for leader election
watch -n1 "blixard raft status | grep state"
```

### Scenario 2: Corrupted Raft State
```bash
# WARNING: This may cause data loss. Use as last resort.

# 1. Stop all nodes
for node in node1 node2 node3; do
  ssh $node "sudo systemctl stop blixard"
done

# 2. Backup current state
for node in node1 node2 node3; do
  ssh $node "sudo cp -r /var/lib/blixard /var/lib/blixard.backup.$(date +%s)"
done

# 3. Clear Raft logs on all nodes
for node in node1 node2 node3; do
  ssh $node "sudo rm -rf /var/lib/blixard/raft-log/*"
done

# 4. Start nodes one by one
ssh node1 "sudo systemctl start blixard"
sleep 10
ssh node2 "sudo systemctl start blixard"
sleep 10
ssh node3 "sudo systemctl start blixard"
```

### Scenario 3: Insufficient Nodes for Quorum
```bash
# If only 1 of 3 nodes is up, quorum impossible

# 1. Bring up at least 2 nodes for a 3-node cluster
# Check and start stopped nodes
for node in node1 node2 node3; do
  ssh $node "sudo systemctl start blixard" || echo "Failed to start $node"
done

# 2. If nodes permanently lost, force reconfiguration (DANGEROUS)
# This should only be done if data loss is acceptable
blixard raft force-recover --survivors node1,node2
```

### Scenario 4: Clock Skew
```bash
# Large clock differences can prevent elections

# 1. Check time on all nodes
for node in node1 node2 node3; do
  echo -n "$node: "
  ssh $node "date '+%s'"
done

# 2. Sync time using NTP
for node in node1 node2 node3; do
  ssh $node "sudo ntpdate -s pool.ntp.org || sudo chronyc makestep"
done

# 3. Restart Blixard to pick up correct time
for node in node1 node2 node3; do
  ssh $node "sudo systemctl restart blixard"
done
```

### Scenario 5: Resource Exhaustion
```bash
# Check if Raft operations timing out due to load

# 1. Check system resources
for node in node1 node2 node3; do
  echo "=== Resources on $node ==="
  ssh $node "free -m; df -h /var/lib/blixard; top -bn1 | head -20"
done

# 2. Increase Raft timeouts temporarily
# Edit configuration on all nodes
for node in node1 node2 node3; do
  ssh $node "sudo sed -i 's/election_timeout = 1000/election_timeout = 5000/' /etc/blixard/config.toml"
done

# 3. Restart with new timeouts
for node in node1 node2 node3; do
  ssh $node "sudo systemctl restart blixard"
done
```

## Recovery Verification

1. **Confirm Leader Election**
   ```bash
   # Should show exactly one leader
   blixard raft status | grep "state.*leader"
   
   # Verify term is incrementing
   watch -n1 "blixard raft status | grep term"
   ```

2. **Test Write Operations**
   ```bash
   # Try creating a test VM
   blixard vm create --name test-recovery --vcpus 1 --memory 512
   
   # Should succeed if leader elected
   ```

3. **Check Cluster Consensus**
   ```bash
   # All nodes should agree on leader
   for node in node1 node2 node3; do
     ssh $node "blixard raft status | grep leader_id"
   done
   ```

## Prevention

1. **Network Reliability**
   - Use dedicated cluster network
   - Implement redundant network paths
   - Monitor network latency between nodes

2. **Time Synchronization**
   ```bash
   # Ensure NTP is configured
   cat > /etc/chrony/chrony.conf <<EOF
   server pool.ntp.org iburst
   makestep 1.0 3
   rtcsync
   EOF
   ```

3. **Monitoring**
   - Alert on Raft term changes
   - Monitor election frequency
   - Track peer connection stability

4. **Configuration Tuning**
   ```toml
   # /etc/blixard/config.toml
   [raft]
   election_timeout = 2000  # Increase for unstable networks
   heartbeat_interval = 500
   max_inflight_msgs = 256
   ```

## Escalation

If no leader after 15 minutes:
1. Page platform engineering lead
2. Consider emergency maintenance window
3. Prepare for potential data recovery procedures

## Related Runbooks
- [High Raft Proposal Failures](./high-proposal-failures.md)
- [Frequent Leader Changes](./leader-instability.md)
- [Network Partition](./network-partition.md)