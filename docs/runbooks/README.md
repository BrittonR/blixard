# Blixard Operational Runbooks

This directory contains operational runbooks for responding to alerts and incidents in the Blixard distributed VM orchestration platform.

## Quick Reference

### 🔴 Critical Alerts
- [Node Down](./node-down.md) - Blixard node unreachable
- [No Raft Leader](./no-raft-leader.md) - Cluster has no consensus leader
- [P2P Verification Failures](./p2p-verification.md) - Image integrity compromised
- [VM Recovery Failures](./vm-recovery-failures.md) - Automatic recovery failing

### 🟡 Warning Alerts
- [High CPU Usage](./high-cpu-usage.md) - Cluster CPU > 90%
- [High Memory Usage](./high-memory-usage.md) - Cluster memory > 90%
- [High Disk Usage](./high-disk-usage.md) - Cluster disk > 85%
- [VM Health Check Failures](./vm-health-failures.md) - Health checks failing > 20%
- [VM Placement Failures](./vm-placement-failures.md) - Cannot place new VMs
- [Unhealthy Nodes](./unhealthy-nodes.md) - Nodes not healthy
- [High Proposal Failures](./high-proposal-failures.md) - Raft proposals failing
- [Leader Instability](./leader-instability.md) - Frequent leader changes

### 🟢 Info Alerts
- [Storage High Throughput](./storage-throughput.md) - Unusual storage activity
- [P2P Low Cache Hit Rate](./p2p-cache.md) - P2P cache ineffective

## Using These Runbooks

### Structure
Each runbook follows this structure:
1. **Alert Details** - Alert name, severity, component
2. **Description** - What the alert means
3. **Impact** - Business and technical impact
4. **Diagnostic Steps** - How to investigate
5. **Resolution Steps** - How to fix common scenarios
6. **Recovery Verification** - How to confirm fix worked
7. **Prevention** - How to avoid recurrence
8. **Escalation** - When and how to escalate

### Best Practices
1. **Start with diagnostics** - Understand the problem before acting
2. **Document actions** - Record what you do for post-mortem
3. **Test fixes** - Verify resolution before closing incident
4. **Update runbooks** - Add new scenarios as you discover them

### Common Tools

#### Cluster Status
```bash
blixard cluster status          # Overall cluster health
blixard node list              # List all nodes
blixard raft status            # Raft consensus state
```

#### VM Management
```bash
blixard vm list                # List all VMs
blixard vm stats <vm>          # VM resource usage
blixard vm health-report       # Health check summary
```

#### Metrics & Monitoring
```bash
# Prometheus queries
curl http://localhost:9090/metrics

# Check specific metrics
curl "http://prometheus:9090/api/v1/query?query=up"
```

#### Logs
```bash
journalctl -u blixard -f       # Follow Blixard logs
journalctl -u blixard --since "1 hour ago"
```

## Alert Priority Matrix

| Severity | Response Time | Examples | Action |
|----------|--------------|----------|---------|
| Critical | < 15 min | No leader, Node down | Page on-call immediately |
| Warning | < 1 hour | High resource usage | Investigate during business hours |
| Info | < 1 day | Cache misses | Review in daily standup |

## Incident Response Process

1. **Acknowledge** - Claim the incident
2. **Assess** - Determine scope and impact
3. **Communicate** - Update stakeholders
4. **Investigate** - Use runbook diagnostics
5. **Mitigate** - Apply temporary fixes if needed
6. **Resolve** - Implement permanent fix
7. **Verify** - Confirm resolution
8. **Document** - Update incident record

## Post-Incident

After resolving an incident:
1. Create post-mortem document
2. Update relevant runbook with lessons learned
3. Create tickets for prevention work
4. Share learnings with team

## Contributing

To add or update runbooks:
1. Use the template in `_template.md`
2. Test all commands in a safe environment
3. Get review from another operator
4. Update this index

## Emergency Contacts

- Platform Team: [#platform-oncall]
- Security Team: [#security-incidents]
- Infrastructure: [#infra-oncall]

## Additional Resources

- [Blixard Architecture Docs](../architecture/)
- [Monitoring Guide](../monitoring.md)
- [Troubleshooting Guide](../troubleshooting.md)
- [Security Policies](../security/)