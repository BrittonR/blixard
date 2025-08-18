#!/bin/bash

echo "=== TESTING NODE FAILURE AND RECOVERY ==="
echo ""

# Get current leader status
echo "1. Current cluster state:"
bash test-cluster-health.sh | grep -E "Nodes running|leader recognitions"
echo ""

# Kill node 3
echo "2. Killing Node 3..."
NODE3_PID=$(ps aux | grep "node --id 3" | grep -v grep | awk '{print $2}')
if [ -n "$NODE3_PID" ]; then
    kill $NODE3_PID
    echo "   Killed Node 3 (PID: $NODE3_PID)"
else
    echo "   Node 3 not found"
fi
sleep 5

# Check cluster with 2 nodes
echo ""
echo "3. Cluster state with 2 nodes:"
RUNNING=$(ps aux | grep "target/debug/blixard" | grep -v grep | wc -l)
echo "   Nodes running: $RUNNING/3"
echo "   (Node 1 and 2 should still be working - majority consensus)"
echo ""

# Wait and check if cluster is still functional
echo "4. Checking if 2-node cluster maintains consensus..."
sleep 10
RECENT_NODE2=$(tail -5 node2.log | grep -c "RAFT")
echo "   Node 2 recent Raft activity: $RECENT_NODE2 messages"
echo ""

# Restart node 3
echo "5. Restarting Node 3..."
NODE1_TICKET=$(grep "Node ticket for discovery:" node1.log | tail -1 | awk '{print $NF}')
cargo run --bin blixard -- node --id 3 --data-dir ./test_node3 --join-ticket "$NODE1_TICKET" > node3_restart.log 2>&1 &
NEW_NODE3_PID=$!
echo "   Started new Node 3 (PID: $NEW_NODE3_PID)"
sleep 10

# Check if node 3 rejoined
echo ""
echo "6. Checking if Node 3 rejoined..."
if grep -q "Successfully joined cluster" node3_restart.log; then
    echo "   ✅ Node 3 successfully rejoined cluster"
else
    echo "   ❌ Node 3 failed to rejoin"
fi

# Final status
echo ""
echo "7. Final cluster status:"
bash test-cluster-health.sh | tail -1