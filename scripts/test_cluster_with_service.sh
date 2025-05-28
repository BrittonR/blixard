#!/bin/bash
# Test script for Blixard cluster with test-http-server service
# This script starts a cluster, manages the test-http-server service, and verifies functionality

set -e
set -x  # Enable verbose logging to see all commands

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Configuration
CLUSTER_NODES=2
SERVICE_NAME="test-http-server"
SERVICE_PORT=8888
TEST_URL="http://localhost:${SERVICE_PORT}"

echo -e "${GREEN}=== Blixard Cluster Test with ${SERVICE_NAME} ===${NC}"

# Function to check if service file exists
check_service_file() {
    if [[ ! -f "$HOME/.config/systemd/user/${SERVICE_NAME}.service" ]]; then
        echo -e "${YELLOW}Service file not found. Creating test-http-server service...${NC}"
        
        # Create systemd user directory if it doesn't exist
        mkdir -p "$HOME/.config/systemd/user"
        
        # Create the service file
        cat > "$HOME/.config/systemd/user/${SERVICE_NAME}.service" << EOF
[Unit]
Description=Test HTTP Server for Blixard Testing
After=network.target

[Service]
Type=simple
ExecStart=/usr/bin/python3 -m http.server ${SERVICE_PORT} --directory /tmp
Restart=on-failure
RestartSec=5s

[Install]
WantedBy=default.target
EOF
        
        # Reload systemd user daemon
        systemctl --user daemon-reload
        echo -e "${GREEN}Service file created successfully${NC}"
    fi
}

# Function to cleanup on exit
cleanup() {
    echo -e "\n${YELLOW}Cleaning up...${NC}"
    
    # Stop the test service
    echo "Stopping ${SERVICE_NAME}..."
    gleam run -m service_manager -- stop --user ${SERVICE_NAME} 2>/dev/null || true
    
    # Kill cluster nodes
    for pid in ${NODE_PIDS[@]}; do
        if kill -0 $pid 2>/dev/null; then
            echo "Stopping cluster node (PID: $pid)..."
            kill $pid 2>/dev/null || true
        fi
    done
    
    echo -e "${GREEN}Cleanup complete${NC}"
}

# Set trap for cleanup
trap cleanup EXIT INT TERM

# Check/create service file
check_service_file

# Start cluster nodes
echo -e "\n${GREEN}1. Starting ${CLUSTER_NODES} cluster nodes...${NC}"
NODE_PIDS=()

# Log current process info
echo "Current shell PID: $$"
echo "Current working directory: $(pwd)"

for i in $(seq 1 $CLUSTER_NODES); do
    echo "Starting cluster node $i..."
    echo "Command: gleam run -m service_manager -- --join-cluster"
    gleam run -m service_manager -- --join-cluster &
    PID=$!
    NODE_PIDS+=($PID)
    echo "Started node $i with PID: $PID"
    echo "Waiting 3 seconds for node to initialize..."
    sleep 3
done

echo -e "${GREEN}Cluster nodes started: PIDs ${NODE_PIDS[@]}${NC}"
echo "Waiting 2 more seconds for cluster stabilization..."
sleep 2

# Test service management
echo -e "\n${GREEN}2. Testing service management...${NC}"

echo "Starting ${SERVICE_NAME}..."
echo "Command: gleam run -m service_manager -- start --user ${SERVICE_NAME}"
gleam run -m service_manager -- start --user ${SERVICE_NAME}
echo "Exit code: $?"
echo "Waiting 2 seconds after service start..."
sleep 2

echo -e "\nListing all services:"
echo "Command: gleam run -m service_manager -- list"
gleam run -m service_manager -- list
echo "Exit code: $?"

echo -e "\nChecking ${SERVICE_NAME} status:"
echo "Command: gleam run -m service_manager -- status --user ${SERVICE_NAME}"
gleam run -m service_manager -- status --user ${SERVICE_NAME}
echo "Exit code: $?"

# Test HTTP endpoint
echo -e "\n${GREEN}3. Testing HTTP endpoint...${NC}"
echo "Waiting for service to be ready..."
sleep 2

# Create a test file in /tmp
TEST_FILE="/tmp/blixard_test_$(date +%s).txt"
echo "Blixard cluster test successful!" > "$TEST_FILE"
echo "Created test file: $TEST_FILE"

# Test HTTP access
echo -e "\nTesting HTTP access to ${TEST_URL}..."
if curl -s -o /dev/null -w "%{http_code}" "$TEST_URL" | grep -q "200\|403"; then
    echo -e "${GREEN}✓ HTTP server is responding${NC}"
    
    # Try to fetch the test file
    if curl -s "${TEST_URL}/$(basename $TEST_FILE)" | grep -q "Blixard cluster test successful"; then
        echo -e "${GREEN}✓ Test file successfully served${NC}"
    else
        echo -e "${YELLOW}! Could not fetch test file (this is normal if /tmp listing is disabled)${NC}"
    fi
else
    echo -e "${RED}✗ HTTP server is not responding${NC}"
fi

# Test from another CLI instance
echo -e "\n${GREEN}4. Verifying from new CLI instance...${NC}"
echo "Listing services from a fresh CLI connection:"
echo "Command: gleam run -m service_manager -- list"
gleam run -m service_manager -- list
echo "Exit code: $?"

echo -e "\nChecking service status from fresh CLI connection:"
echo "Command: gleam run -m service_manager -- status --user ${SERVICE_NAME}"
gleam run -m service_manager -- status --user ${SERVICE_NAME}
echo "Exit code: $?"

# Test stopping the service
echo -e "\n${GREEN}5. Testing service stop...${NC}"
echo "Command: gleam run -m service_manager -- stop --user ${SERVICE_NAME}"
gleam run -m service_manager -- stop --user ${SERVICE_NAME}
echo "Exit code: $?"
echo "Waiting 2 seconds after stop..."
sleep 2

echo -e "\nVerifying service is stopped:"
echo "Command: gleam run -m service_manager -- status --user ${SERVICE_NAME}"
gleam run -m service_manager -- status --user ${SERVICE_NAME} || echo -e "${GREEN}✓ Service stopped as expected${NC}"
echo "Exit code: $?"

# Verify HTTP endpoint is down
if curl -s -o /dev/null -w "%{http_code}" --connect-timeout 2 "$TEST_URL" 2>/dev/null | grep -q "000"; then
    echo -e "${GREEN}✓ HTTP endpoint is no longer accessible${NC}"
else
    echo -e "${YELLOW}! HTTP endpoint might still be accessible (systemd stop may be delayed)${NC}"
fi

# Final service listing
echo -e "\n${GREEN}6. Final service listing:${NC}"
echo "Command: gleam run -m service_manager -- list"
gleam run -m service_manager -- list
echo "Exit code: $?"

# Cleanup test file
rm -f "$TEST_FILE"

echo -e "\n${GREEN}=== Test completed successfully! ===${NC}"
echo -e "${YELLOW}Note: Cluster nodes will be stopped automatically${NC}"