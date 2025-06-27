#!/bin/bash
# Test script for TUI connection status indicators

set -e

echo "=== Testing TUI Connection Status Indicators ==="
echo

# Colors for output
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m' # No Color

# Function to run TUI in background
run_tui() {
    echo -e "${GREEN}Starting TUI...${NC}"
    cargo run --bin blixard -- tui &
    TUI_PID=$!
    echo "TUI started with PID: $TUI_PID"
    sleep 2
}

# Function to start a test node
start_node() {
    local node_id=$1
    local port=$2
    echo -e "${GREEN}Starting node $node_id on port $port...${NC}"
    cargo run --bin blixard -- node --id $node_id --bind 127.0.0.1:$port --data-dir /tmp/blixard-test-$node_id &
    NODE_PID=$!
    echo "Node started with PID: $NODE_PID"
    sleep 3
}

# Function to stop a process
stop_process() {
    local pid=$1
    local name=$2
    if [ ! -z "$pid" ] && kill -0 $pid 2>/dev/null; then
        echo -e "${YELLOW}Stopping $name (PID: $pid)...${NC}"
        kill $pid 2>/dev/null || true
        wait $pid 2>/dev/null || true
    fi
}

# Cleanup function
cleanup() {
    echo
    echo -e "${YELLOW}Cleaning up...${NC}"
    stop_process $TUI_PID "TUI"
    stop_process $NODE_PID "Node"
    rm -rf /tmp/blixard-test-*
    echo -e "${GREEN}Cleanup complete${NC}"
}

# Set trap for cleanup
trap cleanup EXIT

# Test scenarios
echo "=== Test 1: TUI starts without cluster ==="
echo "The TUI should show:"
echo "- Connection state: Failed or Disconnected"
echo "- Retry attempts visible"
echo "- Network quality: Unknown"
echo
run_tui
echo "Observe the TUI for 10 seconds..."
sleep 10

echo
echo "=== Test 2: Starting cluster node ==="
echo "The TUI should show:"
echo "- Connection state changing to Connecting"
echo "- Then Connected with latency"
echo "- Network quality indicator"
echo
start_node 1 7001
echo "Observe the TUI connecting..."
sleep 5

echo
echo "=== Test 3: Simulating network issues ==="
echo "Stopping the node to simulate disconnect..."
echo "The TUI should show:"
echo "- Connection state: Disconnected"
echo "- Automatic reconnection attempts"
echo "- Retry counter incrementing"
echo
stop_process $NODE_PID "Node"
NODE_PID=""
echo "Observe reconnection attempts for 15 seconds..."
sleep 15

echo
echo "=== Test 4: Node comes back online ==="
echo "The TUI should automatically reconnect"
echo
start_node 1 7001
echo "Observe automatic reconnection..."
sleep 10

echo
echo -e "${GREEN}=== Connection Status Test Complete ===${NC}"
echo
echo "Key observations to verify:"
echo "1. Connection states transition correctly"
echo "2. Latency is displayed when connected"
echo "3. Network quality indicators work"
echo "4. Automatic reconnection works"
echo "5. Retry counter and delays are visible"
echo
echo "Press Ctrl+C to exit the TUI and cleanup"

# Keep script running
wait $TUI_PID