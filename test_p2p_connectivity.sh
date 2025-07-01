#!/bin/bash

# Test P2P connectivity between two Blixard nodes

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo -e "${GREEN}=== Blixard P2P Connectivity Test ===${NC}"

# Clean up any existing data
echo "Cleaning up previous test data..."
rm -rf /tmp/blixard-p2p-test-*

# Start node 1
echo -e "\n${YELLOW}Starting Node 1 (Bootstrap)...${NC}"
RUST_LOG=debug ./target/debug/blixard node \
    --id 1 \
    --bind 127.0.0.1:7001 \
    --data-dir /tmp/blixard-p2p-test-1 \
    > /tmp/blixard-node1.log 2>&1 &
NODE1_PID=$!

echo "Node 1 PID: $NODE1_PID"
sleep 3

# Check if node 1 started successfully
if ! ps -p $NODE1_PID > /dev/null; then
    echo -e "${RED}Node 1 failed to start!${NC}"
    cat /tmp/blixard-node1.log
    exit 1
fi

# Start node 2 and join cluster
echo -e "\n${YELLOW}Starting Node 2 (Join cluster)...${NC}"
RUST_LOG=debug ./target/debug/blixard node \
    --id 2 \
    --bind 127.0.0.1:7002 \
    --data-dir /tmp/blixard-p2p-test-2 \
    --join 127.0.0.1:7001 \
    > /tmp/blixard-node2.log 2>&1 &
NODE2_PID=$!

echo "Node 2 PID: $NODE2_PID"
sleep 5

# Check if node 2 started successfully
if ! ps -p $NODE2_PID > /dev/null; then
    echo -e "${RED}Node 2 failed to start!${NC}"
    echo "Node 2 logs:"
    cat /tmp/blixard-node2.log
    echo -e "\nNode 1 logs:"
    cat /tmp/blixard-node1.log
    kill $NODE1_PID 2>/dev/null || true
    exit 1
fi

echo -e "\n${GREEN}Both nodes are running${NC}"

# Function to check logs for patterns
check_logs() {
    local pattern="$1"
    local description="$2"
    
    echo -n "Checking for $description... "
    if grep -q "$pattern" /tmp/blixard-node*.log 2>/dev/null; then
        echo -e "${GREEN}FOUND${NC}"
        grep "$pattern" /tmp/blixard-node*.log | head -5
        return 0
    else
        echo -e "${RED}NOT FOUND${NC}"
        return 1
    fi
}

echo -e "\n${YELLOW}=== Checking P2P Components ===${NC}"

# Check for P2P initialization
check_logs "Starting Iroh P2P service" "Iroh P2P service initialization"
check_logs "Iroh node started" "Iroh node startup"
check_logs "Local NodeAddr" "P2P address generation"

# Check for P2P info exchange
echo -e "\n${YELLOW}=== Checking P2P Info Exchange ===${NC}"
check_logs "Broadcasting P2P info" "P2P info broadcast"
check_logs "Received P2P info" "P2P info reception"
check_logs "p2p_address" "P2P address in messages"

# Check for connection attempts
echo -e "\n${YELLOW}=== Checking P2P Connections ===${NC}"
check_logs "Attempting P2P connection" "P2P connection attempts"
check_logs "P2P connection established" "Successful P2P connections"
check_logs "Added P2P connection" "P2P connection tracking"

# Check for errors
echo -e "\n${YELLOW}=== Checking for Errors ===${NC}"
if grep -i "error.*p2p\|p2p.*error\|iroh.*error" /tmp/blixard-node*.log 2>/dev/null; then
    echo -e "${RED}P2P-related errors found:${NC}"
    grep -i "error.*p2p\|p2p.*error\|iroh.*error" /tmp/blixard-node*.log | head -10
else
    echo -e "${GREEN}No P2P errors found${NC}"
fi

# Check cluster status
echo -e "\n${YELLOW}=== Checking Cluster Status ===${NC}"
sleep 2
echo "Querying cluster status from Node 1..."
curl -s http://127.0.0.1:9001/cluster/status | jq . || echo "Failed to get cluster status"

echo -e "\n${YELLOW}=== Checking P2P Status ===${NC}"
echo "Querying P2P status from Node 1..."
curl -s http://127.0.0.1:9001/p2p/status | jq . || echo "Failed to get P2P status"

echo "Querying P2P status from Node 2..."
curl -s http://127.0.0.1:9002/p2p/status | jq . || echo "Failed to get P2P status"

# Extract relevant log sections
echo -e "\n${YELLOW}=== Recent P2P Activity (Node 1) ===${NC}"
grep -E "p2p|P2P|iroh|Iroh" /tmp/blixard-node1.log | tail -20 || echo "No P2P activity found"

echo -e "\n${YELLOW}=== Recent P2P Activity (Node 2) ===${NC}"
grep -E "p2p|P2P|iroh|Iroh" /tmp/blixard-node2.log | tail -20 || echo "No P2P activity found"

# Keep nodes running for manual inspection
echo -e "\n${GREEN}Test complete. Nodes are still running for manual inspection.${NC}"
echo "Node 1 PID: $NODE1_PID (logs: /tmp/blixard-node1.log)"
echo "Node 2 PID: $NODE2_PID (logs: /tmp/blixard-node2.log)"
echo ""
echo "To stop nodes: kill $NODE1_PID $NODE2_PID"
echo "To view logs: tail -f /tmp/blixard-node*.log"
echo ""
echo "Press Ctrl+C to stop nodes and exit..."

# Wait for user interrupt
trap "echo -e '\n${YELLOW}Stopping nodes...${NC}'; kill $NODE1_PID $NODE2_PID 2>/dev/null || true; exit 0" INT
while true; do
    sleep 1
done