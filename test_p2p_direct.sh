#!/bin/bash

# Direct test of Iroh P2P connectivity without P2P manager

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo -e "${GREEN}=== Direct Iroh P2P Test ===${NC}"

# Clean up
echo "Cleaning up..."
pkill -f "test_iroh" || true
rm -f /tmp/test_iroh*.log
sleep 1

# Build the test binaries
echo -e "\n${YELLOW}Building test binaries...${NC}"
cargo build --bin test_iroh_minimal --bin test_iroh_protocol --bin test_iroh_rpc 2>/dev/null || {
    echo -e "${RED}Failed to build test binaries${NC}"
    echo "Available binaries:"
    ls -la target/debug/test_iroh* 2>/dev/null || echo "No test binaries found"
    exit 1
}

# Test 1: Minimal Iroh test
echo -e "\n${YELLOW}=== Test 1: Minimal Iroh Connection ===${NC}"
echo "This tests basic Iroh endpoint creation and connection..."

RUST_LOG=debug ./target/debug/test_iroh_minimal > /tmp/test_iroh_minimal.log 2>&1 &
PID1=$!
sleep 2

if ps -p $PID1 > /dev/null; then
    echo -e "${GREEN}✓ Minimal Iroh test is running${NC}"
    kill $PID1 2>/dev/null || true
else
    echo -e "${RED}✗ Minimal Iroh test failed${NC}"
    echo "Logs:"
    tail -20 /tmp/test_iroh_minimal.log
fi

# Test 2: Protocol handler test
echo -e "\n${YELLOW}=== Test 2: Iroh Protocol Handler ===${NC}"
echo "This tests custom protocol handling over Iroh..."

RUST_LOG=debug ./target/debug/test_iroh_protocol > /tmp/test_iroh_protocol.log 2>&1 &
PID2=$!
sleep 3

if ps -p $PID2 > /dev/null; then
    echo -e "${GREEN}✓ Protocol handler test is running${NC}"
    # Check for successful message exchange
    if grep -q "Received response" /tmp/test_iroh_protocol.log; then
        echo -e "${GREEN}✓ Message exchange successful${NC}"
    else
        echo -e "${YELLOW}⚠ No response received yet${NC}"
    fi
    kill $PID2 2>/dev/null || true
else
    echo -e "${RED}✗ Protocol handler test failed${NC}"
    echo "Logs:"
    tail -20 /tmp/test_iroh_protocol.log
fi

# Test 3: RPC over Iroh test
echo -e "\n${YELLOW}=== Test 3: RPC over Iroh ===${NC}"
echo "This tests RPC communication patterns..."

RUST_LOG=debug ./target/debug/test_iroh_rpc > /tmp/test_iroh_rpc.log 2>&1 &
PID3=$!
sleep 3

if ps -p $PID3 > /dev/null; then
    echo -e "${GREEN}✓ RPC test is running${NC}"
    # Check for successful RPC
    if grep -q "RPC response" /tmp/test_iroh_rpc.log; then
        echo -e "${GREEN}✓ RPC communication successful${NC}"
    else
        echo -e "${YELLOW}⚠ No RPC response yet${NC}"
    fi
    kill $PID3 2>/dev/null || true
else
    echo -e "${RED}✗ RPC test failed${NC}"
    echo "Logs:"
    tail -20 /tmp/test_iroh_rpc.log
fi

echo -e "\n${YELLOW}=== Summary ===${NC}"
echo "Test logs saved to:"
echo "  /tmp/test_iroh_minimal.log"
echo "  /tmp/test_iroh_protocol.log"
echo "  /tmp/test_iroh_rpc.log"

# Check if we have the actual Blixard nodes working
echo -e "\n${YELLOW}=== Checking Blixard Integration ===${NC}"

# Look for existing Iroh transports in Blixard
if ls -la /home/brittonr/git/blixard/blixard-core/src/transport/iroh_*.rs 2>/dev/null | head -5; then
    echo -e "${GREEN}✓ Iroh transport modules found${NC}"
else
    echo -e "${RED}✗ No Iroh transport modules found${NC}"
fi

# Check what's implemented
echo -e "\n${YELLOW}Implemented Iroh services:${NC}"
grep -l "IrohService\|iroh_service" /home/brittonr/git/blixard/blixard-core/src/transport/*.rs 2>/dev/null | while read file; do
    echo "  - $(basename $file)"
done

echo -e "\n${GREEN}Test complete!${NC}"