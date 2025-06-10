#!/bin/bash
# Test script for Rust Blixard implementation

set -e

echo "=== Blixard Rust Implementation Test Suite ==="
echo

# Colors for output
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Function to print test results
print_result() {
    if [ $1 -eq 0 ]; then
        echo -e "${GREEN}✓ $2${NC}"
    else
        echo -e "${RED}✗ $2${NC}"
        return 1
    fi
}

# Function to cleanup test data
cleanup() {
    echo -e "\n${YELLOW}Cleaning up test data...${NC}"
    rm -rf test-data/node*
    pkill -f "blixard" || true
    sleep 1
}

# Ensure cleanup on exit
trap cleanup EXIT

echo "=== 1. Building Blixard ==="
cargo build --release
print_result $? "Build successful"

echo -e "\n=== 2. Running Unit Tests ==="
cargo test --lib
print_result $? "Unit tests passed"

echo -e "\n=== 3. Running Property Tests ==="
cargo test property_test
print_result $? "Property tests passed"

echo -e "\n=== 4. Running Integration Tests ==="
cargo test --test "*integration*" -- --test-threads=1
print_result $? "Integration tests passed"

echo -e "\n=== 5. Running Simulation Tests (if madsim available) ==="
if cargo test --features simulation simulation 2>&1 | grep -q "running"; then
    cargo test --features simulation simulation
    print_result $? "Simulation tests passed"
else
    echo -e "${YELLOW}⚠ Simulation tests skipped (madsim not configured)${NC}"
fi

echo -e "\n=== 6. Testing CLI Commands ==="

# Test help command
echo "Testing help command..."
cargo run -- --help > /dev/null 2>&1
print_result $? "Help command works"

# Test version command
echo "Testing version command..."
cargo run -- --version > /dev/null 2>&1
print_result $? "Version command works"

echo -e "\n=== 7. Testing Single Node Setup ==="

# Start a single node
echo "Starting single node..."
mkdir -p test-data/node1
cargo run -- \
    --node-id 1 \
    --data-dir test-data/node1 \
    --bind-addr 127.0.0.1:9001 \
    > test-data/node1/node.log 2>&1 &
NODE1_PID=$!

sleep 3

# Check if node is running
if ps -p $NODE1_PID > /dev/null; then
    print_result 0 "Single node started successfully"
else
    print_result 1 "Single node failed to start"
    cat test-data/node1/node.log
    exit 1
fi

echo -e "\n=== 8. Testing Multi-Node Cluster ==="

# Start node 2
echo "Starting node 2..."
mkdir -p test-data/node2
cargo run -- \
    --node-id 2 \
    --data-dir test-data/node2 \
    --bind-addr 127.0.0.1:9002 \
    --join-addr 127.0.0.1:9001 \
    > test-data/node2/node.log 2>&1 &
NODE2_PID=$!

# Start node 3
echo "Starting node 3..."
mkdir -p test-data/node3
cargo run -- \
    --node-id 3 \
    --data-dir test-data/node3 \
    --bind-addr 127.0.0.1:9003 \
    --join-addr 127.0.0.1:9001 \
    > test-data/node3/node.log 2>&1 &
NODE3_PID=$!

sleep 5

# Check if all nodes are running
CLUSTER_OK=1
for pid in $NODE1_PID $NODE2_PID $NODE3_PID; do
    if ! ps -p $pid > /dev/null; then
        CLUSTER_OK=0
        break
    fi
done

print_result $CLUSTER_OK "3-node cluster started successfully"

echo -e "\n=== 9. Testing Service Management (if implemented) ==="

# Try to create a service
echo "Testing service creation..."
if cargo run -- service create test-service --node-id 1 2>&1 | grep -q "not yet implemented"; then
    echo -e "${YELLOW}⚠ Service management not yet implemented${NC}"
else
    cargo run -- service create test-service --node-id 1
    print_result $? "Service created"
    
    # List services
    cargo run -- service list
    print_result $? "Service list works"
fi

echo -e "\n=== 10. Checking Logs for Errors ==="

ERROR_COUNT=0
for i in 1 2 3; do
    if [ -f "test-data/node$i/node.log" ]; then
        ERRORS=$(grep -i "error\|panic" "test-data/node$i/node.log" | grep -v "level=error" | wc -l)
        if [ $ERRORS -gt 0 ]; then
            echo -e "${RED}Found $ERRORS errors in node$i log:${NC}"
            grep -i "error\|panic" "test-data/node$i/node.log" | head -5
            ERROR_COUNT=$((ERROR_COUNT + ERRORS))
        fi
    fi
done

if [ $ERROR_COUNT -eq 0 ]; then
    print_result 0 "No errors in logs"
else
    print_result 1 "$ERROR_COUNT errors found in logs"
fi

echo -e "\n=== Test Summary ==="
echo "All basic tests completed. Check the output above for any failures."
echo
echo "To run more specific tests:"
echo "  - Fault injection: cargo test --features failpoints"
echo "  - Model checking: cargo test model_checking (currently disabled)"
echo "  - Benchmarks: cargo bench"
echo "  - Loom tests: cargo test --features loom"