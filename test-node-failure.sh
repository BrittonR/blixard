#!/bin/bash

# Source cleanup functions
source ./test-cleanup.sh

echo "=== TESTING NODE FAILURE AND RECOVERY ==="
echo ""

# Pre-test cleanup
echo "0. Pre-test cleanup:"
cleanup_all_nodes
echo ""

# Start fresh cluster
echo "1. Starting fresh 3-node cluster..."
# Start Node 1 (bootstrap)
cargo run --bin blixard -- node --id 1 --data-dir ./test_node1 > node1.log 2>&1 &
NODE1_PID=$!
sleep 5

# Get ticket for joining
NODE1_TICKET=$(strings node1.log | grep "Node ticket for discovery:" | tail -1 | awk '{print $NF}')
if [ -z "$NODE1_TICKET" ]; then
    echo "❌ Failed to get Node 1 ticket"
    cleanup_all_nodes
    exit 1
fi

# Start Node 2
cargo run --bin blixard -- node --id 2 --data-dir ./test_node2 --join-ticket "$NODE1_TICKET" > node2.log 2>&1 &
NODE2_PID=$!
sleep 5

# Start Node 3
cargo run --bin blixard -- node --id 3 --data-dir ./test_node3 --join-ticket "$NODE1_TICKET" > node3.log 2>&1 &
NODE3_PID=$!
sleep 10

echo "Started cluster with PIDs: Node1=$NODE1_PID, Node2=$NODE2_PID, Node3=$NODE3_PID"
echo ""

# Get current leader status
echo "2. Current cluster state:"
bash test-cluster-health.sh | grep -E "Nodes running|leader recognitions"
echo ""

# Kill node 3
echo "3. Killing Node 3..."
if [ -n "$NODE3_PID" ]; then
    kill $NODE3_PID
    echo "   Killed Node 3 (PID: $NODE3_PID)"
    wait $NODE3_PID 2>/dev/null
else
    echo "   Node 3 PID not found"
fi
sleep 5

# Check cluster with 2 nodes
echo ""
echo "4. Cluster state with 2 nodes:"
RUNNING=$(ps aux | grep "target/debug/blixard" | grep -v grep | wc -l)
echo "   Nodes running: $RUNNING/3"
echo "   (Node 1 and 2 should still be working - majority consensus)"
echo ""

# Wait and check if cluster is still functional
echo "5. Checking if 2-node cluster maintains consensus..."
sleep 10
RECENT_NODE2=$(strings node2.log | tail -5 | grep -c "RAFT" || echo 0)
echo "   Node 2 recent Raft activity: $RECENT_NODE2 messages"
echo ""

# Restart node 3
echo "6. Restarting Node 3..."
# Get fresh ticket from Node 1
NODE1_TICKET=$(strings node1.log | grep "Node ticket for discovery:" | tail -1 | awk '{print $NF}')
if [ -z "$NODE1_TICKET" ]; then
    echo "   ❌ Failed to get Node 1 ticket for restart"
    cleanup_all_nodes
    exit 1
fi
echo "   Using ticket: $NODE1_TICKET"

cargo run --bin blixard -- node --id 3 --data-dir ./test_node3 --join-ticket "$NODE1_TICKET" > node3_restart.log 2>&1 &
NEW_NODE3_PID=$!
echo "   Started new Node 3 (PID: $NEW_NODE3_PID)"
sleep 15

# Check if node 3 rejoined
echo ""
echo "7. Checking if Node 3 rejoined..."
# Check both log files for join message
if strings node3_restart.log | grep -q "Successfully joined cluster"; then
    echo "   ✅ Node 3 successfully rejoined cluster"
elif strings node3_restart.log | grep -q "Successfully saved configuration state"; then
    echo "   ✅ Node 3 received and saved cluster configuration"
elif strings node3_restart.log | grep -q "Waiting for leader identification"; then
    echo "   ✅ Node 3 connected and waiting for leader"
else
    echo "   ⚠️  Node 3 join status unclear, checking if process is running..."
    if ps -p $NEW_NODE3_PID > /dev/null 2>&1; then
        echo "   ✅ Node 3 process is still running"
        # Check if it's in the cluster config
        if strings node1.log | tail -100 | grep -q "voters: {1, 2, 3}"; then
            echo "   ✅ Node 3 is in cluster configuration"
        fi
    else
        echo "   ❌ Node 3 process exited"
    fi
fi

# Final status
echo ""
echo "8. Final cluster status:"
bash test-cluster-health.sh | tail -1

# Cleanup
echo ""
echo "9. Test cleanup:"
cleanup_all_nodes