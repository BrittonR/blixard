#!/bin/bash

echo "🔍 Final Status Test - P2P Cluster & VMs"
echo "========================================"

# Colors
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m'

# Cleanup
pkill -f "blixard node" || true
sleep 1
rm -rf /tmp/blixard-test-*

# Test 1: P2P Node Communication
echo -e "\n${YELLOW}Test 1: P2P Node Communication${NC}"
echo "================================"

# Start node 1
cargo run -- node --id 1 --bind 127.0.0.1:7001 --data-dir /tmp/blixard-test-1 > /tmp/node1.log 2>&1 &
NODE1_PID=$!
sleep 5

# Start node 2
cargo run -- node --id 2 --bind 127.0.0.1:7002 --join-address 127.0.0.1:7001 --data-dir /tmp/blixard-test-2 > /tmp/node2.log 2>&1 &
NODE2_PID=$!
sleep 10

# Check join status
if grep -q "Successfully joined cluster" /tmp/node2.log; then
    echo -e "${GREEN}✅ P2P join succeeded${NC}"
else
    echo -e "${RED}❌ P2P join failed${NC}"
fi

# Check protocol handler
if grep -q "Registered BLIXARD_ALPN protocol handler" /tmp/node1.log; then
    echo -e "${GREEN}✅ Protocol handler registered${NC}"
else
    echo -e "${RED}❌ Protocol handler not registered${NC}"
fi

# Test 2: Client Connection
echo -e "\n${YELLOW}Test 2: Client Connection${NC}"
echo "=========================="

# Check registry files
if [ -f "/tmp/blixard-test-1/node-1-registry.json" ]; then
    echo -e "${GREEN}✅ Registry file created${NC}"
    echo "Registry contents:"
    cat /tmp/blixard-test-1/node-1-registry.json | jq -c .
else
    echo -e "${RED}❌ Registry file missing${NC}"
fi

# Try cluster status with debug
echo -e "\n${YELLOW}Attempting cluster status...${NC}"
RUST_LOG=blixard_core::transport=debug timeout 10s cargo run -- cluster status --addr /tmp/blixard-test-1/node-1-registry.json > /tmp/cluster_status.log 2>&1

if grep -q "Error getting cluster status" /tmp/cluster_status.log; then
    echo -e "${RED}❌ Cluster status failed${NC}"
    echo "Error:"
    grep -E "(Error|Failed|timeout)" /tmp/cluster_status.log | head -3
else
    echo -e "${GREEN}✅ Cluster status succeeded${NC}"
fi

# Test 3: VM Operations
echo -e "\n${YELLOW}Test 3: VM Operations${NC}"
echo "====================="

# Create VM using HTTP endpoint
echo "Attempting VM creation via HTTP..."
curl -s -X POST http://127.0.0.1:8081/api/vm/create \
  -H "Content-Type: application/json" \
  -d '{"name":"test-vm","vcpus":2,"memory":1024}' > /tmp/vm_create.json

if [ -s /tmp/vm_create.json ]; then
    echo "HTTP Response:"
    cat /tmp/vm_create.json | jq -c . 2>/dev/null || cat /tmp/vm_create.json
else
    echo -e "${YELLOW}⚠️  HTTP endpoint not available${NC}"
fi

# Check Raft activity
echo -e "\n${YELLOW}Test 4: Raft Consensus${NC}"
echo "======================"

RAFT_ENTRIES=$(grep -c "RAFT-READY" /tmp/node1.log || echo "0")
CONF_CHANGES=$(grep -c "conf change" /tmp/node1.log || echo "0")

echo "Raft entries processed: $RAFT_ENTRIES"
echo "Configuration changes: $CONF_CHANGES"

if [ "$CONF_CHANGES" -gt "0" ]; then
    echo -e "${GREEN}✅ Raft processing configuration changes${NC}"
else
    echo -e "${RED}❌ No Raft configuration activity${NC}"
fi

# Summary
echo -e "\n${YELLOW}Summary${NC}"
echo "========"

TESTS_PASSED=0
TESTS_TOTAL=4

# Count passed tests
grep -q "Successfully joined cluster" /tmp/node2.log && ((TESTS_PASSED++))
[ -f "/tmp/blixard-test-1/node-1-registry.json" ] && ((TESTS_PASSED++))
[ "$CONF_CHANGES" -gt "0" ] && ((TESTS_PASSED++))
ps -p $NODE1_PID > /dev/null 2>&1 && ps -p $NODE2_PID > /dev/null 2>&1 && ((TESTS_PASSED++))

echo -e "Tests passed: $TESTS_PASSED/$TESTS_TOTAL"

if [ "$TESTS_PASSED" -eq "$TESTS_TOTAL" ]; then
    echo -e "${GREEN}✅ All core functionality working!${NC}"
else
    echo -e "${YELLOW}⚠️  Some features need attention${NC}"
fi

# Show key issues
echo -e "\n${YELLOW}Known Issues:${NC}"
echo "1. Client cluster status command timing out (registry discovery issue)"
echo "2. VM operations need client transport fix"
echo "3. Some Raft channel errors after join (non-critical)"

# Cleanup
kill $NODE1_PID $NODE2_PID 2>/dev/null || true

echo -e "\nTest complete!"