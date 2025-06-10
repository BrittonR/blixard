#!/usr/bin/env bash
# Test script to run a 3-node Blixard cluster locally

set -e

echo "Building Blixard..."
cargo build --release

BINARY="./target/release/blixard"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Kill any existing nodes
echo "Cleaning up any existing nodes..."
pkill -f "blixard node" || true
sleep 1

# Create data directories
mkdir -p ./test-data/node1 ./test-data/node2 ./test-data/node3

# Start node 1 (initial leader)
echo -e "${BLUE}Starting Node 1 (ID: 1) on 127.0.0.1:7001${NC}"
$BINARY node --id 1 --bind 127.0.0.1:7001 --data-dir ./test-data/node1 > node1.log 2>&1 &
NODE1_PID=$!
echo "Node 1 PID: $NODE1_PID"
sleep 2

# Start node 2 with node 1 as peer
echo -e "${GREEN}Starting Node 2 (ID: 2) on 127.0.0.1:7002${NC}"
$BINARY node --id 2 --bind 127.0.0.1:7002 --data-dir ./test-data/node2 \
    --peer 1:127.0.0.1:7001 > node2.log 2>&1 &
NODE2_PID=$!
echo "Node 2 PID: $NODE2_PID"
sleep 2

# Start node 3 with nodes 1 and 2 as peers
echo -e "${RED}Starting Node 3 (ID: 3) on 127.0.0.1:7003${NC}"
$BINARY node --id 3 --bind 127.0.0.1:7003 --data-dir ./test-data/node3 \
    --peer 1:127.0.0.1:7001 \
    --peer 2:127.0.0.1:7002 > node3.log 2>&1 &
NODE3_PID=$!
echo "Node 3 PID: $NODE3_PID"

echo ""
echo "Cluster is running!"
echo "Logs: tail -f node1.log node2.log node3.log"
echo ""
echo "To test the cluster, you can:"
echo "  1. Watch the logs to see leader election"
echo "  2. Kill the leader to see failover"
echo "  3. Use the VM commands to create/manage VMs"
echo ""
echo "Press Ctrl+C to stop all nodes..."

# Function to cleanup on exit
cleanup() {
    echo ""
    echo "Stopping all nodes..."
    kill $NODE1_PID $NODE2_PID $NODE3_PID 2>/dev/null || true
    wait $NODE1_PID $NODE2_PID $NODE3_PID 2>/dev/null || true
    echo "Cluster stopped."
}

trap cleanup EXIT

# Wait for Ctrl+C
while true; do
    sleep 1
    
    # Check if processes are still running
    if ! kill -0 $NODE1_PID 2>/dev/null; then
        echo "Node 1 crashed! Check node1.log"
    fi
    if ! kill -0 $NODE2_PID 2>/dev/null; then
        echo "Node 2 crashed! Check node2.log"
    fi
    if ! kill -0 $NODE3_PID 2>/dev/null; then
        echo "Node 3 crashed! Check node3.log"
    fi
done