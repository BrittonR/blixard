#!/bin/bash
# Quick P2P test without full rebuild

set -e

echo "=== Quick P2P Test ==="

# Clean up
echo "Cleaning up old test data..."
rm -rf /tmp/blixard-test-*

# Make sure we're in the right directory
cd /home/brittonr/git/blixard

# Start node 1
echo "Starting node 1..."
RUST_LOG=info cargo run -- node --id 1 --bind 127.0.0.1:7001 --data-dir /tmp/blixard-test-1 > /tmp/node1.log 2>&1 &
NODE1_PID=$!
echo "Node 1 PID: $NODE1_PID"

# Wait for node 1 to start
echo "Waiting for node 1 to start..."
for i in {1..30}; do
    if curl -s http://127.0.0.1:8081/metrics > /dev/null 2>&1; then
        echo "Node 1 is up!"
        break
    fi
    if [ $i -eq 30 ]; then
        echo "Node 1 failed to start!"
        cat /tmp/node1.log
        exit 1
    fi
    sleep 1
done

# Get bootstrap info
echo "Getting bootstrap info..."
BOOTSTRAP_INFO=$(curl -s http://127.0.0.1:8081/bootstrap)
echo "Bootstrap info: $BOOTSTRAP_INFO"

# Start node 2 with join
echo "Starting node 2 with join..."
RUST_LOG=info cargo run -- node --id 2 --bind 127.0.0.1:7002 --data-dir /tmp/blixard-test-2 --join-address http://127.0.0.1:8081 > /tmp/node2.log 2>&1 &
NODE2_PID=$!
echo "Node 2 PID: $NODE2_PID"

# Wait for node 2 to start
echo "Waiting for node 2 to start..."
for i in {1..30}; do
    if curl -s http://127.0.0.1:8082/metrics > /dev/null 2>&1; then
        echo "Node 2 is up!"
        break
    fi
    if [ $i -eq 30 ]; then
        echo "Node 2 failed to start!"
        cat /tmp/node2.log
        exit 1
    fi
    sleep 1
done

# Give nodes time to establish P2P connection
echo "Waiting for P2P connection..."
sleep 10

# Check cluster status
echo -e "\n=== Cluster Status Check ==="
echo "Node 1 status:"
cargo run -- cluster status --server 127.0.0.1:7001 2>&1 | grep -E "(Leader:|Nodes:|Failed)" || echo "Status check failed"

echo -e "\nNode 2 status:"
cargo run -- cluster status --server 127.0.0.1:7002 2>&1 | grep -E "(Leader:|Nodes:|Failed)" || echo "Status check failed"

# Test VM creation
echo -e "\n=== Testing VM Creation ==="
cargo run -- vm create --name test-vm --server 127.0.0.1:7001 2>&1 | grep -E "(Success|Failed|Error)" || echo "VM creation test failed"

# Check logs for errors
echo -e "\n=== Checking for errors in logs ==="
echo "Node 1 errors:"
grep -i "error\|fail\|panic" /tmp/node1.log | tail -5 || echo "No errors found"

echo -e "\nNode 2 errors:"
grep -i "error\|fail\|panic" /tmp/node2.log | tail -5 || echo "No errors found"

# Cleanup
echo -e "\n=== Cleanup ==="
kill $NODE1_PID $NODE2_PID 2>/dev/null || true
wait $NODE1_PID $NODE2_PID 2>/dev/null || true

echo "Test complete!"