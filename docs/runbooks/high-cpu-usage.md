# Runbook: High CPU Usage

## Alert Details
- **Alert Name**: ClusterHighCPUUsage
- **Severity**: Warning
- **Component**: Resources

## Description
The cluster is using more than 90% of available CPU resources for over 10 minutes. This indicates either high VM load or insufficient cluster capacity.

## Impact
- New VM placement failures
- Degraded VM performance
- Increased latency for operations
- Risk of system instability if usage reaches 100%

## Diagnostic Steps

### 1. Identify CPU Usage Distribution
```bash
# Check cluster-wide metrics
curl -s http://prometheus:9090/api/v1/query?query=cluster_vcpus_used | jq
curl -s http://prometheus:9090/api/v1/query?query=cluster_vcpus_total | jq

# Per-node CPU usage
blixard cluster resources --detail

# Top CPU consuming VMs
blixard vm list --sort-by cpu --limit 10
```

### 2. Check Node-Level CPU
```bash
# SSH to each node and check actual CPU
for node in $(blixard node list --format json | jq -r '.nodes[].address'); do
  echo "=== CPU on $node ==="
  ssh $node "top -bn1 | head -20; mpstat 1 5"
done

# Check for CPU steal (virtualized environments)
for node in $nodes; do
  ssh $node "cat /proc/stat | grep cpu"
done
```

### 3. Identify Resource Anomalies
```bash
# Check for runaway VMs
blixard vm list --format json | jq '.vms[] | select(.cpu_usage > 90)'

# Historical CPU trends
curl -s "http://prometheus:9090/api/v1/query_range?query=cluster_vcpus_used&start=$(date -d '1 hour ago' +%s)&end=$(date +%s)&step=60" | jq

# Check for recent VM launches
blixard vm list --created-after "1 hour ago"
```

### 4. Verify Scheduler Behavior
```bash
# Check recent placement decisions
journalctl -u blixard | grep -E "placement|schedule" | tail -50

# Verify placement constraints
blixard scheduler status
```

## Resolution Steps

### Scenario 1: Legitimate High Usage
```bash
# 1. Add more nodes to cluster
blixard node add --address <new-node-ip> --capacity vcpus=64,memory=128GB

# 2. Migrate some VMs to new nodes
blixard vm migrate --strategy least-loaded --count 5

# 3. Consider scaling down non-critical VMs
blixard vm resize --name <vm-name> --vcpus <lower-value>
```

### Scenario 2: Runaway VM Process
```bash
# 1. Identify problematic VM
VM_ID=$(blixard vm list --format json | jq -r '.vms[] | select(.cpu_usage > 100) | .id' | head -1)

# 2. Connect to VM console
blixard vm console $VM_ID

# Inside VM:
# - Check top processes: top
# - Look for mining/malware: ps aux | grep -E 'xmr|mine|coin'
# - Check cron jobs: crontab -l

# 3. If malicious, terminate immediately
blixard vm stop --force $VM_ID
blixard vm delete $VM_ID
```

### Scenario 3: CPU Overcommit Issue
```bash
# 1. Check overcommit ratio
PHYSICAL_CPUS=$(lscpu | grep "^CPU(s):" | awk '{print $2}')
ALLOCATED_VCPUS=$(blixard vm list --format json | jq '[.vms[].vcpus] | add')
echo "Overcommit ratio: $(($ALLOCATED_VCPUS / $PHYSICAL_CPUS))"

# 2. Adjust overcommit settings
blixard config set scheduler.cpu_overcommit_ratio 2.0

# 3. Rebalance VMs
blixard scheduler rebalance --resource cpu
```

### Scenario 4: Host CPU Throttling
```bash
# 1. Check CPU frequency scaling
for node in $nodes; do
  ssh $node "cpupower frequency-info"
done

# 2. Set performance mode
for node in $nodes; do
  ssh $node "sudo cpupower frequency-set -g performance"
done

# 3. Disable CPU frequency scaling
for node in $nodes; do
  ssh $node "echo performance | sudo tee /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor"
done
```

### Scenario 5: Noisy Neighbor VMs
```bash
# 1. Identify VMs on same host with high CPU
blixard vm list --node <node-id> --format json | jq '.vms[] | {name, vcpus, cpu_usage}'

# 2. Separate noisy neighbors
blixard vm migrate --name <noisy-vm> --to-node <different-node> --reason "noisy neighbor"

# 3. Set CPU limits
blixard vm update --name <vm-name> --cpu-limit 80
```

## Immediate Mitigation

### Emergency CPU Reduction
```bash
# 1. Pause non-critical VMs
for vm in $(blixard vm list --tag non-critical --format json | jq -r '.vms[].name'); do
  blixard vm pause $vm
done

# 2. Reduce batch job priority
blixard vm update --tag batch --cpu-shares 512

# 3. Enable CPU quotas
blixard config set scheduler.enforce_cpu_quotas true
```

## Recovery Verification

1. **CPU Usage Below Threshold**
   ```bash
   # Should be < 80% for healthy operation
   watch -n5 'blixard cluster resources | grep CPU'
   ```

2. **No Placement Failures**
   ```bash
   # Check recent placement attempts
   journalctl -u blixard | grep placement | grep -i fail | tail -10
   ```

3. **VM Performance Restored**
   ```bash
   # Check VM metrics
   blixard vm metrics --aggregate --metric cpu_wait
   ```

## Prevention

1. **Capacity Planning**
   ```bash
   # Set up alerts at 70% usage
   cat >> /etc/blixard/alerts.yaml <<EOF
   - name: cpu_capacity_warning
     expr: cluster_vcpus_used / cluster_vcpus_total > 0.7
     for: 10m
     severity: warning
   EOF
   ```

2. **Resource Quotas**
   ```toml
   # /etc/blixard/config.toml
   [quotas]
   max_vcpus_per_vm = 32
   max_vcpus_per_tenant = 256
   cpu_overcommit_ratio = 2.0
   ```

3. **Admission Control**
   ```bash
   # Implement placement policies
   blixard policy create --name cpu-limits --rule "vm.vcpus <= 16 || approval.required"
   ```

4. **Regular Monitoring**
   - Daily capacity reports
   - Weekly trend analysis
   - Monthly capacity planning review

## Cost Optimization

1. **Right-sizing VMs**
   ```bash
   # Generate right-sizing recommendations
   blixard analyze vm-usage --period 7d --recommend
   ```

2. **Scheduling Policies**
   ```bash
   # Implement time-based scaling
   blixard scheduler add-policy --name night-scaling \
     --schedule "0 20 * * *" \
     --action "scale-down --tag batch --factor 0.5"
   ```

## Escalation

If CPU remains >95% for 30 minutes:
1. Page infrastructure team
2. Prepare for emergency capacity addition
3. Consider load shedding for non-critical workloads

## Related Runbooks
- [High Memory Usage](./high-memory-usage.md)
- [VM Placement Failures](./vm-placement-failures.md)
- [Node Resource Exhaustion](./node-resource-exhaustion.md)