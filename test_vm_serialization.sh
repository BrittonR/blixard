#!/bin/bash
# Test script to verify VM serialization

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo -e "${YELLOW}Testing VM Serialization Fix${NC}"
echo "================================"

# Build the project
echo -e "\n${YELLOW}Building project...${NC}"
cargo build --bin blixard
if [ $? -ne 0 ]; then
    echo -e "${RED}Build failed!${NC}"
    exit 1
fi

# Start a test node
echo -e "\n${YELLOW}Starting test node...${NC}"
cargo run --bin blixard -- node --id 1 --bind 127.0.0.1:7001 --data-dir ./test-data &
NODE_PID=$!

# Wait for node to start
echo -e "${YELLOW}Waiting for node to start...${NC}"
sleep 5

# Test VM creation
echo -e "\n${YELLOW}Testing VM creation...${NC}"
cargo run --bin blixard -- vm create --name test-vm --vcpus 2 --memory 1024

# Check result
if [ $? -eq 0 ]; then
    echo -e "${GREEN}VM creation succeeded!${NC}"
    
    # List VMs to verify
    echo -e "\n${YELLOW}Listing VMs...${NC}"
    cargo run --bin blixard -- vm list
else
    echo -e "${RED}VM creation failed!${NC}"
fi

# Cleanup
echo -e "\n${YELLOW}Cleaning up...${NC}"
kill $NODE_PID 2>/dev/null
rm -rf ./test-data

echo -e "\n${GREEN}Test completed!${NC}"