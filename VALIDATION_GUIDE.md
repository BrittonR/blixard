# Cluster Formation Validation Guide

## 1. Automated Test Validation

Run the cluster formation tests multiple times to ensure stability:

```bash
# Run validation script
sh scripts/validate-cluster-formation.sh

# Or run tests manually with detailed output
cargo test test_two_node_cluster_formation --features test-helpers -- --nocapture

# Run all cluster tests
cargo test cluster_formation --features test-helpers
```

## 2. Manual Cluster Testing

Start a real 3-node cluster and verify formation:

```bash
# Terminal 1 - Bootstrap node
cargo run --features test-helpers -- node --id 1 --bind 127.0.0.1:7001 --data-dir test-data/node1

# Terminal 2 - Join node 1
cargo run --features test-helpers -- node --id 2 --bind 127.0.0.1:7002 --data-dir test-data/node2 --peers 127.0.0.1:7001

# Terminal 3 - Join node 1
cargo run --features test-helpers -- node --id 3 --bind 127.0.0.1:7003 --data-dir test-data/node3 --peers 127.0.0.1:7001

# Terminal 4 - Check cluster status
cargo run --example cluster_status_client -- http://127.0.0.1:7001
cargo run --example cluster_status_client -- http://127.0.0.1:7002
cargo run --example cluster_status_client -- http://127.0.0.1:7003
```

## 3. Log Analysis

Check the logs for proper Raft state transitions:

```bash
# Look for leader election
grep "became leader" test-data/node*.log

# Check configuration changes
grep "Applied conf change" test-data/node*.log

# Verify peer connections
grep "Connected to peer" test-data/node*.log

# Check for split-brain (multiple leaders)
grep "became leader" test-data/node*.log | sort -k1
```

## 4. Expected Behavior

### Single Node Bootstrap
1. Node 1 starts alone
2. Becomes candidate (no other nodes)
3. Votes for itself
4. Becomes leader at term 1
5. Commits empty entry to establish leadership

### Node Joining
1. Node 2 starts with --join pointing to node 1
2. Node 2 does NOT bootstrap as leader
3. Node 1 receives join request
4. Node 1 proposes configuration change
5. Configuration change is committed
6. Node 2 receives configuration via log replication
7. Both nodes see 2 members in cluster

### Verification Points
- Only one leader per term
- All nodes eventually see the same member list
- Configuration changes are logged and applied
- No "became leader" messages from joining nodes
- Heartbeats flow between all nodes

## 5. Common Issues to Check

### Split Brain
If you see multiple leaders:
```bash
grep "became leader" test-data/node*.log | grep "term: 1"
```
Should show only ONE node becoming leader at term 1.

### Configuration Propagation
All nodes should eventually report the same cluster membership:
```bash
for i in 7001 7002 7003; do
    echo "Node on port $i:"
    cargo run --example cluster_status_client -- http://127.0.0.1:$i 2>/dev/null | grep "Nodes in cluster"
done
```

### Stale Node State
If a node shows outdated membership, check:
1. Is it receiving Raft messages?
2. Are there network connectivity issues?
3. Check for "rejected msgApp" in logs

## 6. Performance Validation

Time how long cluster formation takes:
```bash
time cargo test test_three_node_cluster --features test-helpers
```

Expected: < 5 seconds for 3-node cluster formation

## 7. Stress Testing

Run multiple cluster formations in parallel:
```bash
for i in {1..5}; do
    cargo test cluster_formation --features test-helpers &
done
wait
```

All should pass without conflicts.