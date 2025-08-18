# Running a Three-Node Cluster Manually

## Prerequisites
1. Build the project: `cargo build`
2. Kill any existing nodes: `pkill -9 -f blixard`
3. Clean up old data: `rm -rf test_node*`

## Step 1: Start Node 1 (Bootstrap Leader)
```bash
# Terminal 1
cargo run --bin blixard -- node --id 1 --data-dir ./test_node1
```

Wait for the line: `Node ticket created: nodeac4gtj2w...` (copy this ticket!)

## Step 2: Start Node 2 (Follower)
```bash
# Terminal 2  
cargo run --bin blixard -- node --id 2 --data-dir ./test_node2 --join-ticket <NODE1_TICKET>
```

Wait for: `Successfully joined cluster`

## Step 3: Start Node 3 (Follower)
```bash
# Terminal 3
cargo run --bin blixard -- node --id 3 --data-dir ./test_node3 --join-ticket <NODE1_TICKET>
```

Wait for: `Successfully joined cluster`

## Verify It's Working

In each terminal, you should see:
- Node 1: Regular "Sending MsgHeartbeat" messages to nodes 2 and 3
- Node 2 & 3: Regular "MsgHeartbeatResponse" messages to node 1

Or check the logs:
```bash
# See heartbeat activity
grep "MsgHeartbeat" node*.log | tail -10

# Check if all nodes recognize the leader
grep "leader_id: Some(1)" node*.log

# Watch live activity
tail -f node1.log  # In one terminal
tail -f node2.log  # In another
tail -f node3.log  # In another
```

## Stop the Cluster
```bash
pkill -f blixard
```

## Troubleshooting

If a node fails to join:
1. Check the ticket is copied correctly (it's very long!)
2. Make sure ports 50001, 50002, 50003 are free
3. Check node1.log for errors
4. Try with fresh data directories: `rm -rf test_node*`