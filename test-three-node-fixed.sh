#!/usr/bin/env bash

echo "=== THREE NODE CLUSTER TEST WITH FIXED PORTS ==="
echo "Node 1: port 50001"
echo "Node 2: port 50002"  
echo "Node 3: port 50003"
echo ""

# Kill any existing processes
pkill -9 -f "blixard" 2>/dev/null
rm -rf test_node1 test_node2 test_node3

echo "Starting Node 1 (Bootstrap Leader on port 50001)..."
cargo run --bin blixard -- node --id 1 --data-dir ./test_node1 > node1.log 2>&1 &
NODE1_PID=$!
echo "Node 1 PID: $NODE1_PID"

# Wait for node 1 to start
sleep 5

# Get node 1 ticket (look for either format)
NODE1_TICKET=$(grep -E "Node ticket (created|for discovery):" node1.log | awk '{print $NF}')
if [ -z "$NODE1_TICKET" ]; then
    echo "ERROR: Failed to get Node 1 ticket"
    tail -20 node1.log
    exit 1
fi
echo "Node 1 ticket: $NODE1_TICKET"

echo ""
echo "Starting Node 2 (joining cluster on port 50002)..."
cargo run --bin blixard -- node --id 2 --data-dir ./test_node2 --join-ticket "$NODE1_TICKET" > node2.log 2>&1 &
NODE2_PID=$!
echo "Node 2 PID: $NODE2_PID"

# Wait for node 2 to join
sleep 10

echo "Checking Node 2 status..."
if grep -q "Successfully joined cluster" node2.log; then
    echo "✅ Node 2 joined successfully"
else
    echo "❌ Node 2 failed to join"
fi

if grep -q "leader_id: Some(1)" node2.log; then
    echo "✅ Node 2 sees Node 1 as leader"
else
    echo "❌ Node 2 doesn't see leader"
fi

echo ""
echo "Starting Node 3 (joining cluster on port 50003)..."
cargo run --bin blixard -- node --id 3 --data-dir ./test_node3 --join-ticket "$NODE1_TICKET" > node3.log 2>&1 &
NODE3_PID=$!
echo "Node 3 PID: $NODE3_PID"

# Wait for node 3 to join
sleep 10

echo "Checking Node 3 status..."
if grep -q "Successfully joined cluster" node3.log; then
    echo "✅ Node 3 joined successfully"
else
    echo "❌ Node 3 failed to join"
fi

if grep -q "leader_id: Some(1)" node3.log; then
    echo "✅ Node 3 sees Node 1 as leader"
else
    echo "❌ Node 3 doesn't see leader"
fi

echo ""
echo "=== FINAL CLUSTER STATUS ==="

# Check if all nodes are still running
RUNNING=$(ps aux | grep "cargo run.*blixard" | grep -v grep | wc -l)
echo "Nodes still running: $RUNNING/3"

# Check leader status
echo ""
echo "Leader recognition:"
grep -h "leader_id: Some(1)" node*.log | wc -l | xargs -I {} echo "Nodes recognizing Node 1 as leader: {}/3"

# Check cluster membership
echo ""
echo "Node 1 cluster members:"
grep "voters.*\[" node1.log | tail -1

echo ""
echo "To monitor: tail -f node*.log"
echo "To stop: pkill -f blixard"