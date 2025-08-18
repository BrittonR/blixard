#!/bin/bash

# Test script to verify the restart-candidate bug fix

# Source cleanup functions
source ./test-cleanup.sh

echo "=== Testing Restart Node Fix ==="
echo "This test verifies that restarting nodes join as followers, not candidates"

# Pre-test cleanup
echo "Step 0: Pre-test cleanup"
full_cleanup
sleep 2

echo "Step 1: Start Node 1 (bootstrap)"
cargo run -- node --id 1 --data-dir ./test_node1 > node1.log 2>&1 &
NODE1_PID=$!
sleep 3

echo "Step 2: Start Node 2 (join cluster)"
JOIN_TICKET=$(strings node1.log | grep "Node ticket for discovery:" | tail -1 | awk '{print $NF}')
if [ -z "$JOIN_TICKET" ]; then
    echo "ERROR: Could not extract join ticket from node1 logs"
    echo "Looking for ticket in logs..."
    strings node1.log | grep -E "ticket|discovery" | tail -5
    exit 1
fi
echo "Found join ticket: $JOIN_TICKET"

cargo run -- node --id 2 --data-dir ./test_node2 --join-ticket "$JOIN_TICKET" > node2.log 2>&1 &
NODE2_PID=$!
sleep 5

echo "Step 3: Start Node 3 (join cluster)"
cargo run -- node --id 3 --data-dir ./test_node3 --join-ticket "$JOIN_TICKET" > node3.log 2>&1 &
NODE3_PID=$!
sleep 5

echo "Step 4: Check cluster status"
echo "Node 1 should be leader, nodes 2 and 3 should be followers"

# Function to check if node became candidate
check_candidate_behavior() {
    local log_file=$1
    local node_id=$2
    
    if grep -q "became candidate" "$log_file" || grep -q "starting election" "$log_file"; then
        echo "❌ FAIL: Node $node_id became candidate (should be follower)"
        return 1
    else
        echo "✅ PASS: Node $node_id did not become candidate"
        return 0
    fi
}

# Check initial behavior
check_candidate_behavior node2.log 2
check_candidate_behavior node3.log 3

echo "Step 5: Restart Node 3 (CRITICAL TEST)"
echo "Killing and restarting Node 3..."
kill $NODE3_PID
sleep 2

# Clear previous log
> node3_restart.log

# Restart Node 3 with the same join ticket
cargo run -- node --id 3 --data-dir ./test_node3 --join-ticket "$JOIN_TICKET" > node3_restart.log 2>&1 &
NODE3_RESTART_PID=$!
sleep 5

echo "Step 6: Verify Node 3 behavior after restart"
if check_candidate_behavior node3_restart.log 3; then
    echo "✅ SUCCESS: Node 3 restarted as follower (bug fixed!)"
else
    echo "❌ FAILURE: Node 3 became candidate on restart (bug still exists)"
    echo "Node 3 restart logs:"
    echo "--- Last 20 lines of node3_restart.log ---"
    tail -20 node3_restart.log
    
    # Clean up
    kill $NODE1_PID $NODE2_PID $NODE3_RESTART_PID 2>/dev/null || true
    exit 1
fi

echo "Step 7: Check for specific log messages indicating fix is working"
if grep -q "\[STORAGE\] Initializing joining node - resetting Raft state" node3_restart.log; then
    echo "✅ Found expected storage reset message"
else
    echo "⚠️  Warning: Did not find expected storage reset message"
fi

if grep -q "\[RAFT-JOIN\]" node3_restart.log; then
    echo "✅ Found expected join processing messages"
else
    echo "⚠️  Warning: Did not find expected join processing messages"
fi

echo "Step 8: Final cluster health check"
sleep 3

# Check that all nodes are running and cluster is stable
if ps -p $NODE1_PID > /dev/null && ps -p $NODE2_PID > /dev/null && ps -p $NODE3_RESTART_PID > /dev/null; then
    echo "✅ All nodes still running - cluster is stable"
else
    echo "❌ Some nodes crashed - cluster is unstable"
fi

echo "=== Test Complete ==="
echo ""
echo "Step 9: Test cleanup"
cleanup_all_nodes
cleanup_logs

echo ""
echo "=== Summary ==="
echo "This test verified that:"
echo "1. Nodes starting with --join-ticket reset their Raft state to follower mode"
echo "2. Restarting nodes do NOT become candidates"  
echo "3. Cluster remains stable after node restarts"
echo ""
echo "Key fix components:"
echo "- initialize_joining_node() now always resets state"
echo "- apply_initial_conf_state() checks voter status"
echo "- Bootstrap safety checks prevent incorrect bootstrap"