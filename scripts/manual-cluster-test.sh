#!/bin/bash
# Manual test to verify cluster formation

set -e

echo "=== Manual Cluster Formation Test ==="
echo
echo "This script will:"
echo "1. Start 3 nodes"
echo "2. Form a cluster"
echo "3. Verify all nodes see each other"
echo "4. Check Raft state"
echo

# Clean up any existing data
echo "Cleaning up test data..."
rm -rf test-data/node1 test-data/node2 test-data/node3
mkdir -p test-data/node1 test-data/node2 test-data/node3

# Build the project first
echo "Building project..."
cargo build --features test-helpers --release

# Start node 1 (bootstrap node)
echo
echo "Starting node 1 (bootstrap)..."
cargo run --features test-helpers --release -- node --id 1 --bind 127.0.0.1:7001 --data-dir test-data/node1 > test-data/node1.log 2>&1 &
NODE1_PID=$!
echo "Node 1 PID: $NODE1_PID"
sleep 2

# Start node 2 (joining node 1)
echo "Starting node 2 (joining node 1)..."
cargo run --features test-helpers --release -- node --id 2 --bind 127.0.0.1:7002 --data-dir test-data/node2 --peers 127.0.0.1:7001 > test-data/node2.log 2>&1 &
NODE2_PID=$!
echo "Node 2 PID: $NODE2_PID"
sleep 2

# Start node 3 (joining node 1)
echo "Starting node 3 (joining node 1)..."
cargo run --features test-helpers --release -- node --id 3 --bind 127.0.0.1:7003 --data-dir test-data/node3 --peers 127.0.0.1:7001 > test-data/node3.log 2>&1 &
NODE3_PID=$!
echo "Node 3 PID: $NODE3_PID"
sleep 2

# Function to check cluster status via gRPC
check_cluster_status() {
    local node_id=$1
    local port=$2
    echo
    echo "Checking cluster status from node $node_id (port $port)..."
    
    # Use the gRPC client example
    cargo run --release --example cluster_status_client -- http://127.0.0.1:$port 2>/dev/null | grep -E "(Node|Leader|nodes in cluster)" || echo "Failed to get status"
}

# Let cluster stabilize
echo
echo "Waiting for cluster to stabilize..."
sleep 5

# Check status from each node
check_cluster_status 1 7001
check_cluster_status 2 7002
check_cluster_status 3 7003

# Check Raft logs for key events
echo
echo "=== Raft State Transitions ==="
echo
echo "Node 1 state transitions:"
grep -E "(became leader|became follower|switched to configuration)" test-data/node1.log | tail -5 || echo "No transitions found"

echo
echo "Node 2 state transitions:"
grep -E "(became leader|became follower|switched to configuration)" test-data/node2.log | tail -5 || echo "No transitions found"

echo
echo "Node 3 state transitions:"
grep -E "(became leader|became follower|switched to configuration)" test-data/node3.log | tail -5 || echo "No transitions found"

# Check for configuration changes
echo
echo "=== Configuration Changes ==="
grep -E "Applied conf change|Added peer|configuration.*voters" test-data/node*.log | tail -10 || echo "No conf changes found"

# Cleanup
echo
echo "Cleaning up..."
kill $NODE1_PID $NODE2_PID $NODE3_PID 2>/dev/null || true
wait $NODE1_PID $NODE2_PID $NODE3_PID 2>/dev/null || true

echo
echo "Test complete! Check test-data/*.log for full logs."