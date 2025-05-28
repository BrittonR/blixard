#!/bin/bash
# Quick diagnostic script for Blixard

# Colors
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
BLUE='\033[0;34m'
NC='\033[0m'

echo -e "${GREEN}=== Blixard Diagnostics ===${NC}\n"

# 1. Check Erlang processes
echo -e "${BLUE}1. Erlang/BEAM Processes:${NC}"
BEAM_COUNT=$(ps aux | grep beam.smp | grep -v grep | wc -l)
if [ $BEAM_COUNT -gt 0 ]; then
    echo -e "${GREEN}Found $BEAM_COUNT BEAM process(es):${NC}"
    ps aux | grep beam.smp | grep -v grep | awk '{print "  PID:", $2, "Node:", $12, $13}' | head -5
else
    echo -e "${YELLOW}No BEAM processes running${NC}"
fi
echo

# 2. Check EPMD
echo -e "${BLUE}2. Erlang Port Mapper Daemon (epmd):${NC}"
if epmd -names 2>/dev/null | grep -q "node"; then
    epmd -names 2>/dev/null
else
    echo -e "${YELLOW}No nodes registered with epmd${NC}"
fi
echo

# 3. Check test service
echo -e "${BLUE}3. Test HTTP Server Status:${NC}"
if systemctl --user is-active test-http-server >/dev/null 2>&1; then
    echo -e "${GREEN}test-http-server is active${NC}"
    echo -n "  Port 8888: "
    if nc -z localhost 8888 2>/dev/null; then
        echo -e "${GREEN}OPEN${NC}"
    else
        echo -e "${RED}CLOSED${NC}"
    fi
else
    echo -e "${YELLOW}test-http-server is not active${NC}"
fi
echo

# 4. Quick service list (if nodes are running)
if [ $BEAM_COUNT -gt 0 ]; then
    echo -e "${BLUE}4. Managed Services:${NC}"
    cd "$(dirname "$0")/.." 2>/dev/null
    timeout 5 gleam run -m service_manager -- list 2>&1 | grep -A 10 "CLI-Managed Services" | grep "•" || echo -e "${YELLOW}No services found or timeout${NC}"
else
    echo -e "${BLUE}4. Managed Services:${NC}"
    echo -e "${YELLOW}Cannot list services - no cluster nodes running${NC}"
fi
echo

# 5. Recent logs
echo -e "${BLUE}5. Recent Blixard Activity:${NC}"
if [ -f /tmp/blixard_node1.log ]; then
    echo "Last 5 lines from node1 log:"
    tail -5 /tmp/blixard_node1.log 2>/dev/null | sed 's/^/  /'
else
    echo -e "${YELLOW}No recent logs found${NC}"
fi

echo -e "\n${GREEN}=== Quick Commands ===${NC}"
echo "Start cluster:  gleam run -m service_manager -- --join-cluster"
echo "List services:  gleam run -m service_manager -- list"
echo "Start service:  gleam run -m service_manager -- start --user test-http-server"
echo "Stop all:       killall beam.smp"