#!/bin/bash
set -e

echo "Quick VM functionality test"
echo "==========================="

# 1. Start node
echo "Starting blixard node..."
timeout 30 cargo run --release -- node --id 1 --bind 127.0.0.1:7001 --data-dir ./data &
NODE_PID=$!

# Wait for node
echo "Waiting for node to start..."
sleep 10

# Check if node is running
if ! kill -0 $NODE_PID 2>/dev/null; then
    echo "ERROR: Node failed to start"
    exit 1
fi

# 2. Test VM commands
echo -e "\nTesting VM list..."
if cargo run --release -- vm list; then
    echo "✓ VM list works!"
else
    echo "✗ VM list failed"
    kill $NODE_PID 2>/dev/null || true
    exit 1
fi

echo -e "\nTesting VM create..."
if cargo run --release -- vm create --name test-vm; then
    echo "✓ VM create works!"
else
    echo "✗ VM create failed"
    kill $NODE_PID 2>/dev/null || true
    exit 1
fi

echo -e "\nListing VMs again..."
cargo run --release -- vm list

# Cleanup
echo -e "\nCleaning up..."
kill $NODE_PID 2>/dev/null || true
rm -rf ./data

echo -e "\n✅ VM backend is now properly connected!"