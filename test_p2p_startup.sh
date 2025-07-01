#!/bin/bash
# Test script to verify nodes can start with P2P enabled

set -e

echo "Building blixard binary..."
cargo build --bin blixard

echo "Starting node 1 with P2P enabled..."
./target/debug/blixard node \
    --id 1 \
    --bind 127.0.0.1:7001 \
    --data-dir /tmp/blixard-test-1 \
    --p2p-enabled &

NODE1_PID=$!
sleep 2

echo "Checking if node 1 started successfully..."
if kill -0 $NODE1_PID 2>/dev/null; then
    echo "✅ Node 1 is running with PID $NODE1_PID"
else
    echo "❌ Node 1 failed to start"
    exit 1
fi

echo "Starting node 2 with P2P enabled..."
./target/debug/blixard node \
    --id 2 \
    --bind 127.0.0.1:7002 \
    --data-dir /tmp/blixard-test-2 \
    --p2p-enabled &

NODE2_PID=$!
sleep 2

echo "Checking if node 2 started successfully..."
if kill -0 $NODE2_PID 2>/dev/null; then
    echo "✅ Node 2 is running with PID $NODE2_PID"
else
    echo "❌ Node 2 failed to start"
    kill $NODE1_PID
    exit 1
fi

echo "Both nodes started successfully with P2P enabled!"
echo "Node 1 PID: $NODE1_PID"
echo "Node 2 PID: $NODE2_PID"

echo "Cleaning up..."
kill $NODE1_PID $NODE2_PID 2>/dev/null || true
rm -rf /tmp/blixard-test-1 /tmp/blixard-test-2

echo "Test completed successfully!"