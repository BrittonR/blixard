#!/bin/bash

# Simple P2P connectivity test using mock VM backend

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo -e "${GREEN}=== Simple P2P Test ===${NC}"

# Clean up
echo "Cleaning up..."
rm -rf /tmp/blixard-p2p-test-*
pkill -f "blixard node" || true
sleep 1

# Start node 1 with mock backend
echo -e "\n${YELLOW}Starting Node 1...${NC}"
RUST_LOG=debug ./target/debug/blixard node \
    --id 1 \
    --bind 127.0.0.1:7001 \
    --data-dir /tmp/blixard-p2p-test-1 \
    --mock-vm \
    > /tmp/blixard-node1.log 2>&1 &
NODE1_PID=$!

echo "Node 1 PID: $NODE1_PID"
sleep 3

# Check if node 1 started
if ! ps -p $NODE1_PID > /dev/null; then
    echo -e "${RED}Node 1 failed to start!${NC}"
    tail -20 /tmp/blixard-node1.log
    exit 1
fi

# Start node 2 with mock backend
echo -e "\n${YELLOW}Starting Node 2...${NC}"
RUST_LOG=debug ./target/debug/blixard node \
    --id 2 \
    --bind 127.0.0.1:7002 \
    --data-dir /tmp/blixard-p2p-test-2 \
    --mock-vm \
    --peers 127.0.0.1:7001 \
    > /tmp/blixard-node2.log 2>&1 &
NODE2_PID=$!

echo "Node 2 PID: $NODE2_PID"
sleep 5

# Check if node 2 started
if ! ps -p $NODE2_PID > /dev/null; then
    echo -e "${RED}Node 2 failed to start!${NC}"
    tail -20 /tmp/blixard-node2.log
    kill $NODE1_PID 2>/dev/null || true
    exit 1
fi

echo -e "\n${GREEN}Both nodes running${NC}"

# Check for P2P initialization
echo -e "\n${YELLOW}Checking P2P logs...${NC}"

echo -e "\n--- Node 1 P2P logs ---"
grep -i "p2p\|iroh" /tmp/blixard-node1.log | tail -10 || echo "No P2P logs found"

echo -e "\n--- Node 2 P2P logs ---"
grep -i "p2p\|iroh" /tmp/blixard-node2.log | tail -10 || echo "No P2P logs found"

# Check cluster status
echo -e "\n${YELLOW}Checking cluster status...${NC}"
sleep 2

echo "Node 1 cluster status:"
curl -s http://127.0.0.1:9001/cluster/status | jq . || echo "Failed to get status"

echo -e "\nNode 2 cluster status:"
curl -s http://127.0.0.1:9002/cluster/status | jq . || echo "Failed to get status"

# Check P2P endpoints
echo -e "\n${YELLOW}Checking P2P endpoints...${NC}"

echo "Node 1 P2P status:"
curl -s http://127.0.0.1:9001/p2p/status | jq . || echo "No P2P endpoint"

echo -e "\nNode 2 P2P status:"
curl -s http://127.0.0.1:9002/p2p/status | jq . || echo "No P2P endpoint"

# Cleanup
echo -e "\n${GREEN}Test complete. Stopping nodes...${NC}"
kill $NODE1_PID $NODE2_PID 2>/dev/null || true

echo -e "\n${YELLOW}Summary of findings:${NC}"
echo "1. Check if Iroh P2P service started"
echo "2. Check if P2P addresses were exchanged"
echo "3. Check if P2P connections were established"

echo -e "\nLogs saved to:"
echo "  /tmp/blixard-node1.log"
echo "  /tmp/blixard-node2.log"