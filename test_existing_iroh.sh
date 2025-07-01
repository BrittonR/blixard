#!/bin/bash

# Test existing Iroh binaries

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo -e "${GREEN}=== Testing Existing Iroh Binaries ===${NC}"

# Clean up
pkill -f "test_iroh" || true
rm -f /tmp/test_iroh*.log

# Test 1: Protocol handler
echo -e "\n${YELLOW}Running test_iroh_protocol...${NC}"
RUST_LOG=debug timeout 10 ./target/debug/test_iroh_protocol > /tmp/test_iroh_protocol.log 2>&1 || true

echo "Output:"
tail -30 /tmp/test_iroh_protocol.log

# Test 2: RPC test
echo -e "\n${YELLOW}Running test_iroh_rpc...${NC}"
RUST_LOG=debug timeout 10 ./target/debug/test_iroh_rpc > /tmp/test_iroh_rpc.log 2>&1 || true

echo "Output:"
tail -30 /tmp/test_iroh_rpc.log

# Check what's actually happening in Blixard
echo -e "\n${YELLOW}=== Checking Blixard P2P Status ===${NC}"

# Find P2P-related code
echo "P2P initialization points:"
grep -n "IrohTransport::new\|P2pManager::new" /home/brittonr/git/blixard/blixard-core/src/*.rs | head -10

echo -e "\n${YELLOW}P2P address exchange in gRPC:${NC}"
grep -n "p2p_address\|p2p_node_addr" /home/brittonr/git/blixard/proto/*.proto | head -10

echo -e "\n${YELLOW}Iroh transport usage:${NC}"
grep -n "iroh_transport\|IrohTransport" /home/brittonr/git/blixard/blixard-core/src/node*.rs | head -10

echo -e "\n${GREEN}Analysis complete!${NC}"