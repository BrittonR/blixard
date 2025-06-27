#!/bin/bash
# Test script for TUI features with real nodes

set -e

# Colors for output
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m' # No Color

echo -e "${GREEN}=== Blixard TUI Feature Test ===${NC}"
echo

# Clean up any existing data
echo -e "${YELLOW}Cleaning up existing data...${NC}"
cargo run -- reset --force --data-dir ./test-data --vm-config-dir ./test-vm-configs --vm-data-dir ./test-vm-data

# Start first node
echo -e "${YELLOW}Starting bootstrap node (ID: 1, Port: 7001)...${NC}"
cargo run -- node --id 1 --bind 127.0.0.1:7001 --data-dir ./test-data/node1 --daemon &
NODE1_PID=$!
sleep 3

# Start second node
echo -e "${YELLOW}Starting second node (ID: 2, Port: 7002)...${NC}"
cargo run -- node --id 2 --bind 127.0.0.1:7002 --data-dir ./test-data/node2 --peers 127.0.0.1:7001 --daemon &
NODE2_PID=$!
sleep 3

# Check cluster status
echo -e "${YELLOW}Checking cluster status...${NC}"
cargo run -- cluster status

echo
echo -e "${GREEN}=== Testing TUI Features ===${NC}"
echo -e "Nodes are running. Now testing TUI features:"
echo
echo -e "1. ${YELLOW}Auto-discovery (D key)${NC} - Should find nodes on ports 7001-7002"
echo -e "2. ${YELLOW}Quick add node (+ key)${NC} - Should auto-configure node 3"
echo -e "3. ${YELLOW}Batch add nodes (b key)${NC} - Should add multiple nodes"
echo -e "4. ${YELLOW}VM operations${NC} - Create, start, stop VMs"
echo -e "5. ${YELLOW}Node management${NC} - View and manage nodes"
echo -e "6. ${YELLOW}Resource Monitoring (Tab 4)${NC} - Real-time resource usage graphs"
echo -e "   - CPU, Memory, Network, and Disk I/O graphs"
echo -e "   - Per-node resource usage table"
echo -e "   - Health alerts and performance metrics"
echo -e "7. ${YELLOW}VM Templates (+ key in VM tab)${NC} - Quick VM creation with templates"
echo -e "8. ${YELLOW}Health Dashboard${NC} - Cluster health status and alerts"
echo -e "9. ${YELLOW}Log Streaming (Shift+L)${NC} - Real-time log viewer with filtering"
echo -e "   - Filter by source, log level, and search text"
echo -e "   - Follow mode for real-time updates"
echo -e "   - Support for multiple log sources"
echo
echo -e "${GREEN}Starting TUI...${NC}"
echo -e "${YELLOW}Press 'h' in TUI for help, 'q' to quit${NC}"
echo

# Start the TUI
cargo run -- tui

# Cleanup
echo
echo -e "${YELLOW}Cleaning up...${NC}"
kill $NODE1_PID $NODE2_PID 2>/dev/null || true
wait $NODE1_PID $NODE2_PID 2>/dev/null || true

echo -e "${GREEN}Test complete!${NC}"