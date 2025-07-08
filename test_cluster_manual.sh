#!/bin/bash
# Manual test script to verify three-node cluster formation

set -e

echo "=== Manual Three-Node Cluster Test ==="
echo "This test verifies the cluster formation fixes work correctly"
echo ""

# Clean up
echo "Cleaning up old test data..."
rm -rf /tmp/blixard-manual-test
mkdir -p /tmp/blixard-manual-test

# Build the binary in debug mode (faster)
echo "Building blixard binary..."
cargo build --bin blixard --features test-helpers || {
    echo "Build failed!"
    exit 1
}

echo ""
echo "Starting three-node cluster..."
echo "Node 1 (bootstrap): 127.0.0.1:7001"
echo "Node 2 (joining):   127.0.0.1:7002"  
echo "Node 3 (joining):   127.0.0.1:7003"
echo ""

# Start node 1
echo "Starting Node 1..."
RUST_LOG=blixard=info,raft=info cargo run --bin blixard -- node --id 1 --bind 127.0.0.1:7001 --data-dir /tmp/blixard-manual-test/node1 > /tmp/blixard-manual-test/node1.log 2>&1 &
PID1=$!

sleep 5

# Start node 2
echo "Starting Node 2 (joining via node 1)..."
RUST_LOG=blixard=info,raft=info cargo run --bin blixard -- node --id 2 --bind 127.0.0.1:7002 --data-dir /tmp/blixard-manual-test/node2 --join 127.0.0.1:7001 > /tmp/blixard-manual-test/node2.log 2>&1 &
PID2=$!

sleep 5

# Start node 3
echo "Starting Node 3 (joining via node 1)..."
RUST_LOG=blixard=info,raft=info cargo run --bin blixard -- node --id 3 --bind 127.0.0.1:7003 --data-dir /tmp/blixard-manual-test/node3 --join 127.0.0.1:7001 > /tmp/blixard-manual-test/node3.log 2>&1 &
PID3=$!

echo ""
echo "Cluster starting... waiting 10 seconds for convergence"
sleep 10

echo ""
echo "=== Checking Cluster Status ==="

# Function to check if process is running
check_process() {
    if kill -0 $1 2>/dev/null; then
        echo "✓ Node $2 is running (PID: $1)"
        return 0
    else
        echo "✗ Node $2 has crashed!"
        return 1
    fi
}

# Check all nodes are still running
ALL_GOOD=true
check_process $PID1 1 || ALL_GOOD=false
check_process $PID2 2 || ALL_GOOD=false
check_process $PID3 3 || ALL_GOOD=false

echo ""
echo "=== Log Analysis ==="

# Check for key events in logs
echo "Node 1 bootstrap status:"
grep -E "Bootstrap node starting|Raft manager started|Worker.*registered" /tmp/blixard-manual-test/node1.log | tail -5 || echo "  No relevant entries"

echo ""
echo "Node 2 join status:"
grep -E "joined cluster|Leader identified|Worker.*registered|P2P connection" /tmp/blixard-manual-test/node2.log | tail -5 || echo "  No relevant entries"

echo ""
echo "Node 3 join status:"
grep -E "joined cluster|Leader identified|Worker.*registered|P2P connection" /tmp/blixard-manual-test/node3.log | tail -5 || echo "  No relevant entries"

echo ""
echo "=== Error Check ==="
echo "Checking for critical errors..."

# Check for the specific errors we fixed
if grep -q "Failed to send outgoing Raft message" /tmp/blixard-manual-test/*.log; then
    echo "✗ FOUND: 'Failed to send outgoing Raft message' errors!"
else
    echo "✓ No 'Failed to send outgoing Raft message' errors"
fi

if grep -q "Unknown peer" /tmp/blixard-manual-test/*.log | grep -v "Dropping message to unknown peer"; then
    echo "✗ FOUND: 'Unknown peer' errors!"
else
    echo "✓ No critical 'Unknown peer' errors"
fi

if grep -q "panic" /tmp/blixard-manual-test/*.log; then
    echo "✗ FOUND: panic in logs!"
else
    echo "✓ No panics found"
fi

echo ""
echo "=== Test Result ==="
if [ "$ALL_GOOD" = true ]; then
    echo "✅ SUCCESS: All nodes are running"
    echo ""
    echo "You can check the logs at:"
    echo "  /tmp/blixard-manual-test/node1.log"
    echo "  /tmp/blixard-manual-test/node2.log"
    echo "  /tmp/blixard-manual-test/node3.log"
else
    echo "❌ FAILURE: Some nodes have crashed"
fi

echo ""
echo "Cleaning up..."
kill $PID1 $PID2 $PID3 2>/dev/null || true

echo "Test complete!"