#!/bin/bash
# Manual testing script for Blixard

set -e

# Colors
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
BLUE='\033[0;34m'
NC='\033[0m'

# Helper functions
run_cmd() {
    echo -e "${BLUE}→ $1${NC}"
    eval "$1"
    echo
}

wait_with_message() {
    echo -e "${YELLOW}$1${NC}"
    sleep $2
}

# Main test sequence
clear
echo -e "${GREEN}=== Blixard Manual Test Script ===${NC}"
echo -e "This script helps you test Blixard manually step by step\n"

# Step 1: Cleanup
echo -e "${GREEN}Step 1: Cleanup${NC}"
run_cmd "killall beam.smp 2>/dev/null || echo 'No beam processes to kill'"
wait_with_message "Waiting for processes to terminate..." 2

# Step 2: Start cluster nodes
echo -e "${GREEN}Step 2: Start cluster nodes${NC}"
echo "Starting node 1 in background..."
run_cmd "gleam run -m service_manager -- --join-cluster > /tmp/blixard_node1.log 2>&1 &"
NODE1_PID=$!
echo "Node 1 PID: $NODE1_PID"

wait_with_message "Waiting for node 1 to initialize..." 3

echo "Starting node 2 in background..."
run_cmd "gleam run -m service_manager -- --join-cluster > /tmp/blixard_node2.log 2>&1 &"
NODE2_PID=$!
echo "Node 2 PID: $NODE2_PID"

wait_with_message "Waiting for cluster to form..." 5

# Step 3: Check cluster status
echo -e "${GREEN}Step 3: Check cluster status${NC}"
run_cmd "gleam run -m service_manager -- list-cluster"

# Step 4: Test service management
echo -e "${GREEN}Step 4: Test service management${NC}"

echo -e "${BLUE}4.1: Start test-http-server${NC}"
run_cmd "gleam run -m service_manager -- start --user test-http-server"

wait_with_message "Waiting for service to start..." 2

echo -e "${BLUE}4.2: List services${NC}"
run_cmd "gleam run -m service_manager -- list"

echo -e "${BLUE}4.3: Check service status${NC}"
run_cmd "gleam run -m service_manager -- status --user test-http-server"

echo -e "${BLUE}4.4: Test HTTP endpoint${NC}"
run_cmd "curl -s http://localhost:8888 2>&1 | head -5 || echo 'HTTP server not accessible'"

echo -e "${BLUE}4.5: Stop service${NC}"
run_cmd "gleam run -m service_manager -- stop --user test-http-server"

wait_with_message "Waiting for service to stop..." 2

echo -e "${BLUE}4.6: Verify service stopped${NC}"
run_cmd "gleam run -m service_manager -- list"

# Step 5: Cleanup
echo -e "${GREEN}Step 5: Cleanup${NC}"
echo "Stopping cluster nodes..."
kill $NODE1_PID $NODE2_PID 2>/dev/null || true

echo -e "\n${GREEN}=== Test Complete ===${NC}"
echo -e "${YELLOW}Check logs at:${NC}"
echo "- /tmp/blixard_node1.log"
echo "- /tmp/blixard_node2.log"